// Exercise the same coordinator monomorphized in the firmware with a real App,
// a recorded calibration, and mock read-only NOR/LCD adapters.
#[path = "../../../../firmware-diagnostic/src/scan_flow.rs"]
mod firmware;

use super::*;
use firmware::{Action, Scanner};
use uv98_handheld_ui::controller::ScanState;
use uv98_radio_runtime::{scan, State as RadioState};

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
