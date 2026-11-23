// Exercise the same coordinator monomorphized in the firmware with a real App,
// a recorded calibration, and mock read-only NOR/LCD adapters.
#[path = "../../../../firmware-diagnostic/src/scan_flow.rs"]
mod firmware;

use super::*;
use firmware::{cancels_scan, Action, Scanner};
use uv98_handheld_ui::controller::{Notice, ScanState, SideAction};
use uv98_radio_runtime::{ptt, scan, State as RadioState};

type TestApp = App<Memory, Panel, NoDelay>;

fn unexpected_clock_sample() -> u32 {
    panic!("the scan skip path must not sample time in this step")
}

fn start(app: &mut TestApp, scanner: &mut Scanner, now: u32) -> bool {
    assert_eq!(
        event(app, InputEvent::Hold(Key::Digit3)).request,
        Some(Request::StartScan)
    );
    let action = scanner.start(app, now).unwrap();
    scanner
        .finish_input(app, action, false, false, unexpected_clock_sample)
        .unwrap()
}

fn tick(app: &mut TestApp, scanner: &mut Scanner, now: u32, state: RadioState, open: bool) -> bool {
    let retune = scanner.advance(app, now, state, open).unwrap();
    // On the board, a requested retune stops the old RX before pause is published.
    app.set_scan_paused(scanner.paused()).unwrap();
    retune
}

#[test]
fn priority_visit_waits_for_closed_audio_and_can_be_skipped_until_restart() {
    let mut memory = Memory::new();
    add_channel(&mut memory, 2, 43_350_000);
    add_channel(&mut memory, 7, 43_375_000);
    add_channel(&mut memory, 20, 43_400_000);
    add_zone(&mut memory, 0, b"Local", &[20, 0, 7, 2]);
    let original = memory.bytes.clone();
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    menu(&mut app, controller::MenuItem::Zones);
    event(&mut app, InputEvent::Short(Key::Down));
    event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(app.status().view.channel, Some(20));
    menu(&mut app, controller::MenuItem::ScanPriority);
    event(&mut app, InputEvent::Short(Key::Down));
    event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(app.scan_zone_priority(), Some(20));

    let mut scanner = Scanner::new();
    assert!(start(&mut app, &mut scanner, 0));
    assert_eq!(app.status().view.channel, Some(0));
    assert!(!tick(&mut app, &mut scanner, 1, RadioState::Off, false));
    assert_eq!(app.status().view.scan, ScanState::Requested);
    assert!(!tick(
        &mut app,
        &mut scanner,
        2,
        RadioState::Receiving,
        false
    ));
    assert_eq!(app.status().view.scan, ScanState::Running);
    for now in [3, 155, 2000] {
        assert!(!tick(
            &mut app,
            &mut scanner,
            now,
            RadioState::Receiving,
            true
        ));
        assert_eq!(app.status().view.scan, ScanState::Paused);
        assert_eq!(app.status().view.channel, Some(0));
    }
    let quiet_at = 2100;
    assert!(!tick(
        &mut app,
        &mut scanner,
        quiet_at,
        RadioState::Receiving,
        false
    ));
    let resume_at = quiet_at + scan::RESUME_MS;
    assert!(!tick(
        &mut app,
        &mut scanner,
        resume_at - 1,
        RadioState::Receiving,
        false
    ));
    assert!(tick(
        &mut app,
        &mut scanner,
        resume_at,
        RadioState::Receiving,
        false
    ));
    assert_eq!(app.status().view.channel, Some(20));

    assert!(!tick(
        &mut app,
        &mut scanner,
        resume_at + 1,
        RadioState::Receiving,
        true
    ));
    let skip = event(&mut app, InputEvent::Short(Key::Star));
    assert_eq!(skip.request, Some(Request::SkipScanTarget));
    assert!(!skip.ptt_tx_requested);
    let skipped_at = resume_at + 2;
    assert!(scanner
        .finish_input(&mut app, Action::None, false, true, || skipped_at)
        .unwrap());
    assert_eq!(app.status().view.channel, Some(7));
    let tuned_at = skipped_at + 1;
    assert!(!tick(
        &mut app,
        &mut scanner,
        tuned_at,
        RadioState::Receiving,
        false
    ));
    assert!(tick(
        &mut app,
        &mut scanner,
        tuned_at + scan::DWELL_MS,
        RadioState::Receiving,
        false
    ));
    assert_eq!(app.status().view.channel, Some(2));

    let now = tuned_at + scan::DWELL_MS + 1;
    assert!(!tick(
        &mut app,
        &mut scanner,
        now,
        RadioState::Receiving,
        true
    ));
    // Raw PTT wins over a skip delivered in the same pass, before the skip clock
    // is read or a new channel is selected.
    assert!(!scanner
        .finish_input(&mut app, Action::None, true, true, unexpected_clock_sample)
        .unwrap());
    assert!(!scanner.active());
    assert_eq!(app.status().view.scan, ScanState::Stopped);
    assert_eq!(app.status().view.channel, Some(2));
    assert!(!scanner
        .finish_input(
            &mut app,
            Action::None,
            false,
            false,
            unexpected_clock_sample
        )
        .unwrap());
    assert!(!scanner.active());

    assert!(start(&mut app, &mut scanner, now + 1));
    assert_eq!(app.status().view.channel, Some(0));
    assert!(!tick(
        &mut app,
        &mut scanner,
        now + 2,
        RadioState::Receiving,
        false
    ));
    assert!(tick(
        &mut app,
        &mut scanner,
        now + 2 + scan::DWELL_MS,
        RadioState::Receiving,
        false
    ));
    assert_eq!(app.status().view.channel, Some(20));
    let stop = scanner.stop(&mut app).unwrap();
    assert_eq!(stop, Action::Stop);
    assert!(!scanner
        .finish_input(&mut app, stop, false, false, unexpected_clock_sample)
        .unwrap());
    assert_eq!(app.status().view.scan, ScanState::Stopped);
    let (memory, _) = app.into_parts();
    assert_eq!(memory.bytes, original);
    assert!(memory.commands.iter().all(|command| *command == 3));
}

#[test]
fn timeout_rejects_scan_with_or_without_a_late_receiving_ack() {
    for late_state in [RadioState::Off, RadioState::Receiving] {
        let mut memory = Memory::new();
        add_channel(&mut memory, 2, 43_350_000);
        let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
        app.start(0).unwrap();
        let mut scanner = Scanner::new();
        assert!(start(&mut app, &mut scanner, 10));
        assert_eq!(app.status().view.channel, Some(2));
        let timeout_at = 10 + scan::TUNE_TIMEOUT_MS;
        assert!(!tick(
            &mut app,
            &mut scanner,
            timeout_at - 1,
            RadioState::Off,
            false
        ));
        assert_eq!(app.status().view.scan, ScanState::Requested);
        assert!(!tick(&mut app, &mut scanner, timeout_at, late_state, false));
        assert!(!scanner.active());
        assert_eq!(app.status().view.scan, ScanState::Stopped);
        assert!(start(&mut app, &mut scanner, timeout_at + 1));
        assert_eq!(app.status().view.channel, Some(0));
    }
}

#[test]
fn vfo_scan_skips_one_point_and_recovers_after_a_backward_clock() {
    let mut app = app(Memory::new(), PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    event(&mut app, InputEvent::Hold(Key::Menu));
    let view = app.status().view;
    assert_eq!(view.mode, Mode::Vfo);
    let step = controller::STEP_10HZ[usize::from(view.step_index)];
    let mut scanner = Scanner::new();
    assert!(start(&mut app, &mut scanner, 10));
    let first = app.status().view.vfo_10hz;
    assert_eq!(first, view.vfo_10hz - 50_000);
    assert!(!tick(
        &mut app,
        &mut scanner,
        11,
        RadioState::Receiving,
        true
    ));
    let skip = event(&mut app, InputEvent::Short(Key::Star));
    assert_eq!(skip.request, Some(Request::SkipScanTarget));
    assert!(scanner
        .finish_input(&mut app, Action::None, false, true, || 12)
        .unwrap());
    assert_eq!(app.status().view.vfo_10hz, first + step);
    assert!(!tick(
        &mut app,
        &mut scanner,
        11,
        RadioState::Receiving,
        false
    ));
    assert_eq!(app.status().view.scan, ScanState::Stopped);
    assert!(!scanner.active());
    assert!(start(&mut app, &mut scanner, 13));
    assert_eq!(app.status().view.vfo_10hz, first + step - 50_000);
}

#[test]
fn either_side_bound_to_scan_can_start_hold_and_stop_without_arming_ptt() {
    let released = (
        ptt::Input {
            upper: false,
            lower: false,
            clean: true,
        },
        4092,
        InputSample::Stable {
            matrix: 0,
            ptt_pressed: false,
        },
    );
    let upper = (
        ptt::Input {
            upper: true,
            ..released.0
        },
        4092,
        InputSample::Stable {
            matrix: 0,
            ptt_pressed: true,
        },
    );
    let lower = (
        ptt::Input {
            lower: true,
            ..released.0
        },
        2457,
        InputSample::WithAux {
            matrix: 0,
            ptt_pressed: false,
            aux: Some(Key::Aux16),
        },
    );
    let chord = (
        ptt::Input {
            upper: true,
            clean: false,
            ..released.0
        },
        2010,
        InputSample::Unstable,
    );

    for (index, scan_key, tx_key) in [(0, upper, lower), (1, lower, upper)] {
        let mut memory = Memory::new();
        add_channel(&mut memory, 2, 43_350_000);
        let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
        app.start(0).unwrap();
        app.enable_fast_matrix_input();
        app.enable_fast_ptt_input();
        menu(&mut app, controller::MenuItem::SideKeys);
        if index == 1 {
            event(&mut app, InputEvent::Short(Key::Down));
        }
        event(&mut app, InputEvent::Short(Key::Menu));
        for _ in index..4 {
            event(&mut app, InputEvent::Short(Key::Down));
        }
        event(&mut app, InputEvent::Short(Key::Menu));
        assert_eq!(app.status().view.side_bindings[index], SideAction::Scan);
        event(&mut app, InputEvent::Hold(Key::Exit));

        let mut guard = ptt::Guard::new();
        let mut scanner = Scanner::new();
        let mut pass = |app: &mut TestApp, scanner: &mut Scanner, now, (input, adc, sample)| {
            // Mirror the foreground order: guard sees the scan state from before
            // the sample. UI request dispatch precedes cancellation and tuning.
            guard.observe(input, scanner.active());
            let update = app.sample(now, sample).unwrap();
            let action = match update.request {
                Some(Request::StartScan) => scanner.start(app, now).unwrap(),
                Some(Request::StopScan) => scanner.stop(app).unwrap(),
                None => Action::None,
                other => panic!("unexpected request {other:?}"),
            };
            let cancel = cancels_scan(input, adc, app.status().view.side_bindings);
            let retune = scanner
                .finish_input(app, action, cancel, false, unexpected_clock_sample)
                .unwrap();
            if update.ptt_tx_requested {
                assert!(!guard.allow_edge());
                app.reject_tx().unwrap();
            }
            retune
        };

        for at in [0, 30] {
            assert!(!pass(&mut app, &mut scanner, at, released));
        }
        for at in [100, 106] {
            assert!(!pass(&mut app, &mut scanner, at, scan_key));
        }
        assert!(pass(&mut app, &mut scanner, 112, scan_key));
        assert_eq!(app.status().view.channel, Some(2));
        assert!(!tick(
            &mut app,
            &mut scanner,
            113,
            RadioState::Receiving,
            true
        ));
        assert_eq!(app.status().view.scan, ScanState::Paused);
        assert!(!pass(&mut app, &mut scanner, 120, scan_key));
        for at in [200, 212] {
            assert!(!pass(&mut app, &mut scanner, at, released));
            assert!(scanner.active());
        }
        for at in [250, 256] {
            assert!(!pass(&mut app, &mut scanner, at, scan_key));
            assert!(scanner.active());
        }
        assert!(!pass(&mut app, &mut scanner, 262, scan_key));
        assert!(!scanner.active());
        for at in [270, 282] {
            assert!(!pass(&mut app, &mut scanner, at, released));
            assert!(!scanner.active());
        }

        // Long 3 still starts, and a real PTT cancels even before debounce.
        assert!(start(&mut app, &mut scanner, 300));
        for at in [310, 316, 322] {
            assert!(!pass(&mut app, &mut scanner, at, tx_key));
            assert!(!scanner.active());
        }
        for at in [330, 342] {
            assert!(!pass(&mut app, &mut scanner, at, released));
        }
        assert!(start(&mut app, &mut scanner, 350));
        assert!(!pass(&mut app, &mut scanner, 360, chord));
        assert!(!scanner.active());
        assert_eq!(app.status().view.ptt_slot, None);
    }
}

#[test]
fn codeplug_exclusions_apply_to_all_zone_and_priority_without_hiding_manual_channels() {
    let mut memory = Memory::new();
    for channel in [2, 7, 11, 20] {
        add_channel(&mut memory, channel, 43_350_000 + channel as u32 * 100);
    }
    use uv98_codeplug::channels::{patch_record, ChannelPatch};
    for (channel, patch) in [
        (
            2,
            ChannelPatch {
                scan_add: Some(false),
                ..Default::default()
            },
        ),
        (
            7,
            ChannelPatch {
                operation_mode: Some(OperationMode::TransmitOnly),
                ..Default::default()
            },
        ),
        (
            11,
            ChannelPatch {
                operation_mode: Some(OperationMode::ReceiveOnly),
                ..Default::default()
            },
        ),
    ] {
        let first = CHANNELS_ADDRESS as usize + channel * CHANNEL_LEN;
        patch_record(&mut memory.bytes[first..first + CHANNEL_LEN], &patch).unwrap();
    }
    add_zone(&mut memory, 0, b"Local", &[0, 2, 7, 20, 11]);
    let original = memory.bytes.clone();
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let mut scanner = Scanner::new();
    assert_eq!(app.status().view.zone, None);
    assert_eq!(app.next_catalog_index(Some(0), true), Some(2));
    assert_eq!(app.next_catalog_index(Some(2), true), Some(7));
    assert!(start(&mut app, &mut scanner, 0));
    assert_eq!(app.status().view.channel, Some(11));
    assert!(!tick(
        &mut app,
        &mut scanner,
        1,
        RadioState::Receiving,
        false
    ));
    assert!(tick(
        &mut app,
        &mut scanner,
        1 + scan::DWELL_MS,
        RadioState::Receiving,
        false
    ));
    assert_eq!(app.status().view.channel, Some(20));
    scanner.stop(&mut app).unwrap();

    menu(&mut app, controller::MenuItem::Zones);
    event(&mut app, InputEvent::Short(Key::Down));
    event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(app.status().view.zone, Some(0));
    assert_eq!(app.status().view.channel, Some(0));
    assert!(start(&mut app, &mut scanner, 200));
    assert_eq!(app.status().view.channel, Some(20));
    scanner.stop(&mut app).unwrap();

    // Up follows the reverse zone order. The two ineligible slots keep
    // their original order and remain manually selectable.
    for channel in [7, 2] {
        event(&mut app, InputEvent::Short(Key::Up));
        assert_eq!(app.status().view.channel, Some(channel));
        menu(&mut app, controller::MenuItem::ScanPriority);
        event(&mut app, InputEvent::Short(Key::Down));
        event(&mut app, InputEvent::Short(Key::Menu));
        assert_eq!(app.status().view.notice, Notice::ScanPriorityUnavailable);
        assert_eq!(app.scan_zone_priority(), None);
    }
    event(&mut app, InputEvent::Short(Key::Down));
    event(&mut app, InputEvent::Short(Key::Down));
    assert_eq!(app.status().view.channel, Some(20));
    menu(&mut app, controller::MenuItem::ScanPriority);
    event(&mut app, InputEvent::Short(Key::Down));
    event(&mut app, InputEvent::Short(Key::Menu));
    assert_eq!(app.scan_zone_priority(), Some(20));
    assert!(start(&mut app, &mut scanner, 201));
    assert_eq!(app.status().view.channel, Some(11));
    assert!(!tick(
        &mut app,
        &mut scanner,
        202,
        RadioState::Receiving,
        false
    ));
    assert!(tick(
        &mut app,
        &mut scanner,
        202 + scan::DWELL_MS,
        RadioState::Receiving,
        false
    ));
    assert_eq!(app.status().view.channel, Some(20));
    let (memory, _) = app.into_parts();
    assert_eq!(memory.bytes, original);
    assert!(memory.commands.iter().all(|command| *command == 3));
}

#[test]
fn empty_and_single_eligible_zones_stop_without_retuning_the_current_channel() {
    let mut memory = Memory::new();
    memory.bytes[CHANNELS_ADDRESS as usize + 19] = 0x80;
    add_channel(&mut memory, 2, 43_350_000);
    memory.bytes[CHANNELS_ADDRESS as usize + 2 * CHANNEL_LEN + 1] = 2;
    add_channel(&mut memory, 3, 43_375_000);
    add_zone(&mut memory, 0, b"Excluded", &[0, 2]);
    add_zone(&mut memory, 1, b"Empty", &[]);
    add_zone(&mut memory, 2, b"Single", &[0, 3, 2]);
    add_zone(&mut memory, 3, b"Hidden", &[3]);
    memory.bytes[ZONES_ADDRESS as usize + 3 * RECORD_LEN + 1] = 2;
    let mut app = app(memory, PRESERVED_CALIBRATION_SHA256);
    app.start(0).unwrap();
    let mut scanner = Scanner::new();
    let next_zone = |app: &mut TestApp, zone| {
        menu(app, controller::MenuItem::Zones);
        event(app, InputEvent::Short(Key::Down));
        event(app, InputEvent::Short(Key::Menu));
        assert_eq!(app.status().view.zone, zone);
    };

    next_zone(&mut app, Some(0));
    assert!(!start(&mut app, &mut scanner, 0));
    assert!(!scanner.active());
    assert_eq!(app.status().view.channel, Some(0));
    next_zone(&mut app, Some(1));
    assert_eq!(app.status().view.channel, None);
    assert_eq!(event(&mut app, InputEvent::Hold(Key::Digit3)).request, None);
    assert!(!scanner.active());
    next_zone(&mut app, Some(2));
    assert!(start(&mut app, &mut scanner, 1));
    assert_eq!(app.status().view.channel, Some(3));
    assert!(!tick(
        &mut app,
        &mut scanner,
        2,
        RadioState::Receiving,
        false
    ));
    assert!(!tick(
        &mut app,
        &mut scanner,
        2 + scan::DWELL_MS,
        RadioState::Receiving,
        false
    ));
    assert!(!scanner.active());
    assert_eq!(app.status().view.channel, Some(3));
    assert!(!start(&mut app, &mut scanner, 3 + scan::DWELL_MS));
    assert!(!scanner.active());
    next_zone(&mut app, None);
}
