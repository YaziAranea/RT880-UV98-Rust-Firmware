extern crate std;
use super::*;
use std::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Indicators(bool, bool),
    Backlights(bool, bool),
    Read(u8),
    Write(u8, u16),
    Mute(bool),
    Antenna(bool),
    Hf(bool, bool),
}
struct Mock {
    words: [u16; 128],
    events: Vec<Event>,
    fail_after: Option<usize>,
    calls: usize,
    mismatch: Option<u8>,
    muted: bool,
    hf: bool,
    fail_from: Option<usize>,
    verification_failure: bool,
}
impl Mock {
    fn new() -> Self {
        Self {
            words: [0; 128],
            events: Vec::new(),
            fail_after: None,
            calls: 0,
            mismatch: None,
            muted: true,
            hf: false,
            fail_from: None,
            verification_failure: false,
        }
    }
    fn event(&mut self, event: Event) -> Result<(), &'static str> {
        self.events.push(event);
        self.calls += 1;
        if self.fail_after == Some(self.calls)
            || self.fail_from.is_some_and(|first| self.calls >= first)
        {
            Err("injected")
        } else {
            Ok(())
        }
    }
}
impl Io for Mock {
    type Error = &'static str;
    fn read(&mut self, r: u8) -> Result<u16, Self::Error> {
        self.event(Event::Read(r))?;
        Ok(self.words[r as usize] ^ u16::from(self.mismatch == Some(r)))
    }
    fn write(&mut self, r: u8, v: u16) -> Result<(), Self::Error> {
        self.event(Event::Write(r, v))?;
        self.words[r as usize] = v;
        Ok(())
    }
    fn mute(&mut self, m: bool) -> Result<(), Self::Error> {
        self.event(Event::Mute(m))?;
        self.muted = m;
        Ok(())
    }
    fn antenna(&mut self, h: bool) -> Result<(), Self::Error> {
        self.event(Event::Antenna(h))
    }
    fn hf_gates(&mut self, h: bool, b: bool) -> Result<(), Self::Error> {
        self.event(Event::Hf(h, b))?;
        self.hf = h;
        Ok(())
    }
    fn indicators(&mut self, rx: bool, tx: bool) -> Result<(), Self::Error> {
        self.event(Event::Indicators(rx, tx))
    }
    fn backlights(&mut self, display: bool, keys: bool) -> Result<(), Self::Error> {
        self.event(Event::Backlights(display, keys))
    }
    fn verify_control(&mut self) -> Result<(), Self::Error> {
        if self.verification_failure {
            Err("control ownership lost")
        } else {
            Ok(())
        }
    }
}
fn cal() -> Calibration<'static> {
    Calibration::parse(include_bytes!(
        "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
    ))
    .unwrap()
}
fn rx() -> RxRequest {
    RxRequest {
        frequency_10hz: 14_550_000,
        width: sequence::Width::Wide,
        level: 1,
        tone: 0,
        tone_mode: 0,
        option_nibble: 0,
        squelch_delay: 10,
        modulation: Modulation::Fm,
    }
}
fn tx() -> TxRequest {
    TxRequest {
        frequency_10hz: 14_550_000,
        width: sequence::Width::Wide,
        tone: 0,
        tone_mode: 0,
        option_nibble: 0,
        power: 0,
        mic_gain: 31,
    }
}
fn pump(r: &mut Runtime<Mock>, now: &mut u32, held: bool) {
    for _ in 0..300 {
        *now = now.wrapping_add(2);
        r.tick(*now, held).unwrap();
        if r.state() != State::Configuring {
            return;
        }
    }
    panic!("bounded plan did not finish");
}
fn ready() -> (Runtime<Mock>, u32) {
    let mut r = Runtime::new(Mock::new());
    let mut now = 0;
    r.cold_start(&cal(), now).unwrap();
    pump(&mut r, &mut now, false);
    assert!(r.initialized());
    r.receive(&cal(), rx(), now).unwrap();
    pump(&mut r, &mut now, false);
    assert_eq!(r.state(), State::Receiving);
    (r, now)
}
fn assert_off(r: &Runtime<Mock>) {
    assert_eq!(r.io.words[0x33] & 0x0c, 0);
    assert_eq!(r.io.words[0x36], 0x007f);
    assert_eq!(r.io.words[0x30], 0);
    assert!(r.io.muted && !r.io.hf);
}
#[test]
fn cold_rx_tx_release_round_trip() {
    let (mut r, mut now) = ready();
    r.transmit(&cal(), tx(), now, true).unwrap();
    pump(&mut r, &mut now, true);
    assert_eq!(r.state(), State::Transmitting);
    assert_eq!(r.io.words[0x33] & 0x0c, 8);
    assert_ne!(r.io.words[0x36] & 0x80, 0);
    let p = r.tick(now + 1, false).unwrap();
    assert!(p.tx_ended && !p.tx_timed_out);
    assert_off(&r);
    r.receive(&cal(), rx(), now + 2).unwrap();
    now += 2;
    pump(&mut r, &mut now, false);
    assert_eq!(r.state(), State::Receiving);
}

#[test]
fn signaling_access_requires_live_tx_and_cannot_change_rf_registers() {
    let (mut r, mut now) = ready();
    assert!(r.signaling_write(0x70, 0).is_err());
    assert_eq!(r.state(), State::Receiving);
    r.transmit(&cal(), tx(), now, true).unwrap();
    pump(&mut r, &mut now, true);
    let rf = [
        r.io.words[0x30],
        r.io.words[0x33],
        r.io.words[0x36],
        r.io.words[0x38],
        r.io.words[0x39],
    ];
    r.signaling_write(0x71, 10324).unwrap();
    assert_eq!(r.signaling_read(0x71), Ok(10324));
    assert_eq!(
        rf,
        [
            r.io.words[0x30],
            r.io.words[0x33],
            r.io.words[0x36],
            r.io.words[0x38],
            r.io.words[0x39]
        ]
    );
    r.signaling_microphone(false).unwrap();
    assert_eq!(r.io.words[0x30], rf[0] & !2);
    r.signaling_microphone(true).unwrap();
    assert_eq!(r.io.words[0x30], rf[0]);
    let fault = r.signaling_write(0x36, 0xffff).unwrap_err();
    assert!(fault.cleanup_attempted);
    assert_off(&r);
}

#[test]
fn signaling_failed_readback_unkeys_instead_of_continuing_tones() {
    let (mut r, mut now) = ready();
    r.transmit(&cal(), tx(), now, true).unwrap();
    pump(&mut r, &mut now, true);
    r.io.mismatch = Some(0x70);
    assert!(matches!(
        r.signaling_write(0x70, 0x9c00).unwrap_err().reason,
        Error::Readback { register: 0x70, .. }
    ));
    assert_eq!(r.state(), State::Failed);
    assert_off(&r);
}

#[test]
fn interrupted_signaling_does_not_leave_tone2_in_receive() {
    for exit in 0..3 {
        let (mut r, mut now) = ready();
        r.transmit(&cal(), tx(), now, true).unwrap();
        pump(&mut r, &mut now, true);
        let baseline50 = r.io.words[0x50];
        r.signaling_microphone(false).unwrap();
        r.signaling_write(0x70, 0xd3d3).unwrap();
        r.signaling_write(0x50, baseline50 | 0x8000).unwrap();
        match exit {
            0 => r.stop().unwrap(),
            1 => {
                assert!(r.tick(now + TX_TIMEOUT_MS + 1, true).unwrap().tx_timed_out);
            }
            _ => {
                assert!(r.tick(now - 1, true).is_err());
            }
        }
        assert_off(&r);
        assert_eq!(r.io.words[0x70] & 0x8080, 0);
        assert_eq!(r.io.words[0x50], baseline50 & !0x8000);
        if exit == 0 {
            r.receive(&cal(), rx(), now + 1).unwrap();
            now += 1;
            pump(&mut r, &mut now, false);
            assert_eq!(r.state(), State::Receiving);
            assert_eq!(r.io.words[0x70] & 0x8080, 0);
        }
    }
}

#[test]
fn modulation_cleanup_failure_still_attempts_each_independent_off_request() {
    for offset in [8, 9] {
        let (mut r, mut now) = ready();
        r.transmit(&cal(), tx(), now, true).unwrap();
        pump(&mut r, &mut now, true);
        let before50 = r.io.words[0x50];
        let before70 = r.io.words[0x70];
        r.signaling_write(0x70, 0xd3d3).unwrap();
        r.signaling_write(0x50, before50 | 0x8000).unwrap();
        let event_start = r.io.events.len();
        r.io.fail_after = Some(r.io.calls + offset);
        let fault = r.stop().unwrap_err();
        assert_eq!(fault.cleanup_failures, 0x80);
        let events = &r.io.events[event_start..];
        assert_eq!(&events[..7], &OFF_REQUESTS);
        assert_eq!(
            &events[7..],
            &[
                Event::Write(0x70, before70 & !0x8080),
                Event::Write(0x50, before50 & !0x8000)
            ]
        );
        assert_off(&r);
    }
}

#[test]
fn cb_vhf_uhf_and_800_path_follow_each_frequency_without_stale_relay_cache() {
    let (mut r, mut now) = ready();
    for (frequency, mask, relay) in [
        (2_713_500, 0x0001, true),
        (14_550_000, 0x0010, false),
        (85_000_000, 0x0002, true),
        (43_350_000, 0x0020, false),
        (2_713_500, 0x0001, true),
    ] {
        let start = r.io.events.len();
        r.receive(
            &cal(),
            RxRequest {
                frequency_10hz: frequency,
                ..rx()
            },
            now,
        )
        .unwrap();
        pump(&mut r, &mut now, false);
        assert_eq!(r.state(), State::Receiving);
        assert_eq!(r.io.words[0x33] & 0x003f, mask);
        assert_eq!(r.io.words[0x36], 0x007f);
        assert!(!r.io.hf);
        assert_eq!(
            r.io.events[start..]
                .iter()
                .filter_map(|event| match event {
                    Event::Antenna(high) => Some(*high),
                    _ => None,
                })
                .last(),
            Some(relay)
        );
    }
    // The stock factory table has no 800.000 MHz entry. Never retain the
    // previous 27 MHz plan while pretending the gap was tuned successfully.
    assert!(r
        .receive(
            &cal(),
            RxRequest {
                frequency_10hz: 80_000_000,
                ..rx()
            },
            now
        )
        .is_err());
}
#[test]
fn ptt_release_cancels_every_prefix_without_later_enable() {
    for prefix in 0..100 {
        let (mut r, mut now) = ready();
        r.transmit(&cal(), tx(), now, true).unwrap();
        for _ in 0..prefix {
            now += 2;
            r.tick(now, true).unwrap();
        }
        now += 2;
        let p = r.tick(now, false).unwrap();
        assert!(p.tx_ended);
        assert_off(&r);
        let writes = r.io.events.len();
        for _ in 0..10 {
            now += 2;
            r.tick(now, false).unwrap();
        }
        assert_eq!(r.io.events.len(), writes);
    }
}
#[test]
fn timeout_equality_unkeys_and_needs_explicit_new_start() {
    let (mut r, mut now) = ready();
    let start = now;
    r.transmit(&cal(), tx(), now, true).unwrap();
    pump(&mut r, &mut now, true);
    assert_eq!(
        r.tick(start + TX_TIMEOUT_MS - 1, true).unwrap().state,
        State::Transmitting
    );
    assert!(r.tick(start + TX_TIMEOUT_MS, true).unwrap().tx_timed_out);
    assert_off(&r);
    assert_eq!(
        r.tick(start + TX_TIMEOUT_MS + 1, true).unwrap().state,
        State::Off
    );
}
#[test]
fn every_io_failure_is_terminal_and_attempts_all_off_requests() {
    for point in 1..100 {
        let (mut r, mut now) = ready();
        r.transmit(&cal(), tx(), now, true).unwrap();
        r.io.fail_after = Some(r.io.calls + point);
        let mut failed = false;
        for _ in 0..150 {
            now += 2;
            if r.tick(now, true).is_err() {
                failed = true;
                break;
            }
        }
        if failed {
            assert_eq!(r.state(), State::Failed);
            assert_off(&r);
            let count = r.io.events.len();
            assert!(r.tick(now + 1, true).is_err());
            assert_eq!(count, r.io.events.len());
        }
    }
}
#[test]
fn wrong_frequency_readback_and_backwards_clock_fail_closed() {
    let (mut r, mut now) = ready();
    r.io.mismatch = Some(0x38);
    r.transmit(&cal(), tx(), now, true).unwrap();
    for _ in 0..100 {
        now += 2;
        if r.tick(now, true).is_err() {
            break;
        }
    }
    assert_eq!(r.state(), State::Failed);
    assert_off(&r);
    let (mut r, now) = ready();
    assert!(r.tick(now - 1, false).is_err());
    assert_off(&r);
}
#[test]
fn unknown_tone_does_not_open_audio_but_monitor_does() {
    let (mut r, _) = ready();
    r.io.words[0x0c] = 2;
    r.io.words[0x67] = 511;
    let mut a = Audio {
        modulation: Modulation::Am,
        gain: 50,
        dac: 12,
        squelch: 1,
        monitor: false,
        tone_required: true,
    };
    assert!(!r.signal(a, None).unwrap().audio_open);
    a.monitor = true;
    assert!(r.signal(a, None).unwrap().audio_open);
    assert_eq!(r.io.words[0x47], 0x6740);
    a.monitor = false;
    assert!(!r.signal(a, Some(false)).unwrap().audio_open);
    assert!(r.io.muted);
}

#[test]
fn hardware_observation_acks_pending_irq_but_never_opens_on_stale_tone() {
    let (mut r, _) = ready();
    r.io.words[0x0c] = 0x403; // SQL+CT flag, IRQ remains pending in this mock.
    let before = r.io.events.len();
    let result = r.observe(audio(), 0x1375).unwrap();
    assert!(!result.audio_open);
    assert_eq!(
        &r.io.events[before..before + 4],
        &[
            Event::Read(0x0c),
            Event::Read(0x0b),
            Event::Write(0x02, 0),
            Event::Read(0x0c)
        ]
    );
    r.io.words[0x0c] = 0x402;
    assert!(r.observe(audio(), 0x1375).unwrap().audio_open);
    r.io.words[0x0c] = 0x802; // loss flag overrides accepted CT.
    assert!(!r.observe(audio(), 0x1375).unwrap().audio_open);
}

#[test]
fn am_observation_applies_stock_gain_update_and_retune_resets_history() {
    let (mut r, mut now) = ready();
    let mut a = audio();
    a.modulation = Modulation::Am;
    r.io.words[0x67] = 200;
    r.io.words[0x0c] = 2;
    let expected = sequence::am_agc::State::new().sample(200).unwrap();
    r.observe(a, 0).unwrap();
    assert_eq!(r.am_agc, expected.state);
    assert_eq!(r.io.words[0x13], expected.register13);
    r.stop().unwrap();
    r.receive(&cal(), rx(), now).unwrap();
    pump(&mut r, &mut now, false);
    assert_eq!(r.am_agc, sequence::am_agc::State::new());
}

#[test]
fn am_unchanged_gain_skips_only_write_not_fresh_samples_or_state_updates() {
    let (mut r, mut now) = ready();
    let mut a = audio();
    a.modulation = Modulation::Am;
    r.io.words[0x67] = 142;
    r.io.words[0x0c] = 2;
    r.observe(a, 0).unwrap();
    r.io.events.clear();
    let expected = r.am_agc.sample(142).unwrap();
    r.observe(a, 0).unwrap();
    assert_eq!(r.am_agc, expected.state);
    assert_eq!(r.io.events, [Event::Read(0x0c), Event::Read(0x67)]);
    r.stop().unwrap();
    r.receive(&cal(), rx(), now).unwrap();
    pump(&mut r, &mut now, false);
    assert_eq!(r.am_gain_word, None);
    r.io.events.clear();
    r.observe(a, 0).unwrap();
    assert!(r
        .io
        .events
        .contains(&Event::Write(0x13, expected.register13)));
}

#[test]
fn configuration_bursts_respect_delay_and_release_before_next_operation() {
    let (mut r, mut now) = ready();
    r.transmit(&cal(), tx(), now, true).unwrap();
    for _ in 0..64 {
        if r.configuration_step_ready(now) {
            r.tick(now, true).unwrap();
        } else {
            break;
        }
    }
    assert_eq!(r.state(), State::Configuring);
    assert!(r.wait.is_some());
    assert!(!r.configuration_step_ready(now));
    r.io.events.clear();
    // Release must stop even while a delay is still pending.
    assert!(r.tick(now, false).unwrap().tx_ended);
    assert_eq!(r.state(), State::Off);
    assert_eq!(r.io.words[0x36], 0x007f);
    assert_eq!(r.io.words[0x33], 0);
    assert_eq!(r.io.words[0x30], 0);
    now += 10;
    assert!(!r.configuration_step_ready(now));
}

fn scope_pass(r: &mut Runtime<Mock>, scope: &mut scope::Scope, now: &mut u32) -> scope::Completion {
    *now = now.wrapping_add(5);
    r.tick(*now, false).unwrap();
    scope.step(r, &cal(), || *now).unwrap()
}

#[test]
fn scope_actual_samples_two_sweeps_and_full_original_receive_restore() {
    let (mut r, mut now) = ready();
    let mut scope = scope::Scope::new();
    let original_filter = r.io.words[0x43];
    scope.start(&r, &cal(), rx(), 1250, now).unwrap();
    for pass in 1..=2 {
        r.io.words[0x67] = 163 + pass as u16 * 10;
        let previous = *scope.completed_plot();
        for _ in 0..1200 {
            scope_pass(&mut r, &mut scope, &mut now);
            if scope.completed_plot().sweep == pass {
                break;
            }
            assert_eq!(
                *scope.completed_plot(),
                previous,
                "partial sweep leaked to UI"
            );
        }
        let frame = scope.completed_plot();
        assert_eq!(frame.sweep, pass);
        for index in 0..81 {
            assert!(frame.fresh(index));
            assert_eq!(frame.latest[index], 163 + pass as u16 * 10);
            assert_eq!(frame.peaks[index], 163 + pass as u16 * 10);
        }
    }
    assert_eq!(r.state(), State::Sweeping);
    assert!(r.io.muted);
    scope.event(now, uv98_spectrum::compact::Event::Stop);
    let mut restored = false;
    for _ in 0..200 {
        if let scope::Completion::Restored { request, .. } =
            scope_pass(&mut r, &mut scope, &mut now)
        {
            assert_eq!(request, rx());
            restored = true;
            break;
        }
    }
    assert!(restored && !scope.active());
    assert_eq!(r.state(), State::Receiving);
    assert_eq!(r.io.words[0x43], original_filter);
    let expected = sequence::sweep::QualifiedRange::new(
        &cal(),
        rx().frequency_10hz,
        rx().frequency_10hz,
        rx().frequency_10hz,
        1250,
    )
    .unwrap()
    .origin_words();
    assert_eq!((r.io.words[0x38], r.io.words[0x39]), expected);
    assert_eq!(r.io.words[0x36], 0x007f);
    assert_eq!(r.io.words[0x33] & 0x0c, 0);
}

#[test]
fn scope_holds_a_peak_without_retuning_and_resumes_past_the_same_carrier() {
    let (mut radio, mut now) = ready();
    let mut scope = scope::Scope::new();
    scope.start(&radio, &cal(), rx(), 1_250, now).unwrap();
    scope.set_trigger(Some(24), true);
    let range = scope.window().unwrap().range();
    let peak = range.frequency_10hz(24).unwrap();
    let peak_words = sequence::sweep::QualifiedRange::new(&cal(), peak, peak, peak, 1_250)
        .unwrap()
        .origin_words();

    for sweep in 1..=4 {
        if sweep == 3 {
            scope.set_trigger(Some(24), false);
        }
        for _ in 0..1200 {
            radio.io.words[0x67] = if (radio.io.words[0x38], radio.io.words[0x39]) == peak_words {
                220
            } else {
                163
            };
            scope_pass(&mut radio, &mut scope, &mut now);
            if scope.completed_plot().sweep == sweep {
                break;
            }
        }
        assert_eq!(scope.completed_plot().sweep, sweep);
        assert!(scope.completed_plot().elapsed_ms > 0);
        assert_eq!(scope.completed_plot().latest[24], 220);
        if sweep == 1 {
            assert_eq!(scope.completed_plot().held_bin, Some(24));
            radio.io.events.clear();
            for _ in 0..50 {
                scope_pass(&mut radio, &mut scope, &mut now);
            }
            assert!(
                radio.io.events.is_empty(),
                "holding must not advance the sweep"
            );
            scope.resume();
        } else {
            assert_eq!(scope.completed_plot().held_bin, None);
            if sweep >= 3 {
                assert_eq!(scope.completed_plot().trigger_bin, Some(24));
            }
        }
    }

    scope.event(now, uv98_spectrum::compact::Event::Stop);
    for _ in 0..200 {
        if let scope::Completion::Restored { request, .. } =
            scope_pass(&mut radio, &mut scope, &mut now)
        {
            assert_eq!(request, rx());
            assert_eq!(radio.state(), State::Receiving);
            return;
        }
    }
    panic!("original RX was not restored");
}

#[test]
fn scope_ptt_cancels_every_initial_prefix_and_requires_release_for_restart() {
    for prefix in 0..100 {
        let (mut r, mut now) = ready();
        let mut scope = scope::Scope::new();
        scope.start(&r, &cal(), rx(), 1250, now).unwrap();
        for _ in 0..prefix {
            scope_pass(&mut r, &mut scope, &mut now);
        }
        scope.event(now, uv98_spectrum::compact::Event::Ptt(true));
        let mut restored = false;
        for _ in 0..200 {
            if let scope::Completion::Restored { request, reason } =
                scope_pass(&mut r, &mut scope, &mut now)
            {
                assert_eq!(request, rx());
                assert_eq!(reason, uv98_spectrum::compact::StopReason::Ptt);
                restored = true;
                break;
            }
        }
        assert!(restored);
        assert_eq!(r.state(), State::Receiving);
        assert_eq!(r.io.words[0x36], 0x007f);
        assert_eq!(r.io.words[0x33] & 0x0c, 0);
        assert!(matches!(
            scope.start(&r, &cal(), rx(), 1250, now),
            Err(scope::StartError::Protocol(
                uv98_spectrum::compact::StartError::PttHeld
            ))
        ));
        scope.event(now, uv98_spectrum::compact::Event::Ptt(false));
        scope.start(&r, &cal(), rx(), 1250, now).unwrap();
    }
}

#[test]
fn scope_exit_reports_the_exact_original_request_not_just_a_frequency() {
    let (mut r, mut now) = ready();
    let mut original = rx();
    original.frequency_10hz = 43_297_500;
    original.width = sequence::Width::Narrow;
    original.tone = 0x2013;
    original.tone_mode = 1;
    original.option_nibble = 1;
    original.level = 3;
    r.receive(&cal(), original, now).unwrap();
    pump(&mut r, &mut now, false);
    assert_eq!(r.state(), State::Receiving);
    let mut scope = scope::Scope::new();
    scope.start(&r, &cal(), original, 1250, now).unwrap();
    for _ in 0..150 {
        scope_pass(&mut r, &mut scope, &mut now);
        if scope.plot().valid(2) {
            break;
        }
    }
    let mut completion = None;
    scope.event(now, uv98_spectrum::compact::Event::Stop);
    for _ in 0..200 {
        let result = scope_pass(&mut r, &mut scope, &mut now);
        if matches!(result, scope::Completion::Restored { .. }) {
            completion = Some(result);
            break;
        }
    }
    assert_eq!(
        completion,
        Some(scope::Completion::Restored {
            request: original,
            reason: uv98_spectrum::compact::StopReason::Exit,
        })
    );
    assert!(!scope.active());
    assert_eq!(r.state(), State::Receiving);
    // The reported request describes what the runtime acknowledged, so caching
    // it is safe only when ordinary policy still wants these exact fields.
    assert!(policy::restored_current(Some(policy_profile(original)), original).is_some());
    let mut changed = original;
    changed.tone = 0;
    assert!(policy::restored_current(Some(policy_profile(original)), changed).is_none());
}

fn policy_profile(receive: RxRequest) -> policy::Profile {
    policy::Profile {
        receive: Some(receive),
        transmit: None,
        audio: Audio {
            modulation: receive.modulation,
            gain: 1,
            dac: 1,
            squelch: receive.level,
            monitor: false,
            tone_required: receive.tone != 0,
        },
        frequency: receive.frequency_10hz,
        route: 4,
        power: 0,
    }
}

#[test]
fn scope_explicit_window_is_validated_before_any_receiver_work() {
    use uv98_spectrum::{Window, WindowError};
    let (r, now) = ready();
    let mut scope = scope::Scope::new();
    let original = rx();
    assert_eq!(
        scope.start_window(
            &r,
            &cal(),
            original,
            Window::centered(original.frequency_10hz + 250_000, 1_000_000, 12_500).unwrap(),
            now
        ),
        Err(scope::StartError::CenterMismatch)
    );
    assert_eq!(
        Window::centered(original.frequency_10hz, 20_000, 12_500),
        Err(WindowError::TooNarrow)
    );
    assert_eq!(
        Window::centered(original.frequency_10hz + 2_000_000, 4_000_000, 2_500),
        Err(WindowError::TooWide)
    );
    let window = Window::centered(original.frequency_10hz, 500_000, 12_500).unwrap();
    scope
        .start_window(&r, &cal(), original, window, now)
        .unwrap();
    assert_eq!(scope.window(), Some(window));
    assert_eq!(scope.range().unwrap().len(), 41);
    assert_eq!(
        scope.range().unwrap().frequency_10hz(20),
        Some(original.frequency_10hz)
    );
}

#[test]
fn codex_scope_identical_window_restart_invalidates_previous_publication() {
    use crate::spectrum_flow::{bind_bin, PublishedWindow, SelectionError};
    use uv98_spectrum::Window;

    let (mut radio, mut now) = ready();
    let mut scope = scope::Scope::new();
    let window = Window::centered(rx().frequency_10hz, 100_000, 1250).unwrap();
    scope
        .start_window(&radio, &cal(), rx(), window, now)
        .unwrap();
    for _ in 0..200 {
        scope_pass(&mut radio, &mut scope, &mut now);
        if scope.plot().valid(0) {
            break;
        }
    }
    assert!(scope.plot().valid(0));
    let published = PublishedWindow {
        generation: scope.generation(),
        start_10hz: window.start_10hz(),
        step_10hz: window.step_10hz(),
        len: window.len() as u8,
    };
    scope.event(now, uv98_spectrum::compact::Event::Stop);
    let mut restored = false;
    for _ in 0..200 {
        if matches!(
            scope_pass(&mut radio, &mut scope, &mut now),
            scope::Completion::Restored { .. }
        ) {
            restored = true;
            break;
        }
    }
    assert!(restored);
    scope.discard();
    scope
        .start_window(&radio, &cal(), rx(), window, now)
        .unwrap();
    assert_ne!(scope.generation(), published.generation);
    assert_eq!(
        bind_bin(Some(published), scope.window(), scope.generation(), 0),
        Err(SelectionError::StaleWindow),
        "same frequencies do not make the previous session's measurement current"
    );
    assert!(!scope.plot().valid(0));
}

#[test]
fn codex_scope_refused_start_preserves_current_window_identity_and_receiver() {
    use uv98_spectrum::Window;

    let (radio, now) = ready();
    let mut scope = scope::Scope::new();
    let window = Window::centered(rx().frequency_10hz, 100_000, 1250).unwrap();
    let wrong_center = Window::centered(rx().frequency_10hz + 1250, 100_000, 1250).unwrap();
    let events = radio.io.events.len();
    assert_eq!(
        scope.start_window(&radio, &cal(), rx(), wrong_center, now),
        Err(scope::StartError::CenterMismatch)
    );
    assert_eq!(scope.generation(), 0);
    assert_eq!(scope.window(), None);
    scope
        .start_window(&radio, &cal(), rx(), window, now)
        .unwrap();
    let generation = scope.generation();
    assert_eq!(
        scope.start_window(&radio, &cal(), rx(), wrong_center, now),
        Err(scope::StartError::Busy)
    );
    assert_eq!(scope.generation(), generation);
    assert_eq!(scope.window(), Some(window));
    assert_eq!(radio.io.events.len(), events);
    assert_eq!(radio.state(), State::Receiving);
}

#[test]
fn delayed_accept_cannot_complete_a_newer_intent_or_bin() {
    use crate::spectrum_flow::{self, Action, Intent};
    let (mut r, mut now) = ready();
    let mut scope = scope::Scope::new();
    scope.start(&r, &cal(), rx(), 1250, now).unwrap();
    for _ in 0..150 {
        scope_pass(&mut r, &mut scope, &mut now);
        if scope.plot().valid(2) && scope.plot().valid(5) {
            break;
        }
    }
    assert!(scope.plot().valid(2) && scope.plot().valid(5));
    assert!(scope.can_accept_bin_action());
    scope.event(now, uv98_spectrum::compact::Event::Accept { index: 2 });
    assert!(
        !scope.can_accept_bin_action(),
        "an accept during Restoring must be refused by the coordinator"
    );
    assert!(!scope.can_restart());
    // A newer UI intent for bin 5 arrives before the restore ACK for bin 2.
    scope.event(now, uv98_spectrum::compact::Event::Accept { index: 5 });
    let mut reason = None;
    for _ in 0..200 {
        if let scope::Completion::Restored { reason: r, .. } =
            scope_pass(&mut r, &mut scope, &mut now)
        {
            reason = Some(r);
            break;
        }
    }
    let reason = reason.expect("old restore completes");
    assert_eq!(
        reason,
        uv98_spectrum::compact::StopReason::Accepted { index: 2 }
    );
    assert_eq!(
        spectrum_flow::completion(Some(Intent::Listen(5)), reason),
        Action::Finish,
        "new bin must not be completed by the old restore"
    );
    assert_eq!(
        spectrum_flow::completion(Some(Intent::AcceptVfo(5)), reason),
        Action::Finish
    );
    assert_eq!(
        spectrum_flow::completion(Some(Intent::Listen(2)), reason),
        Action::BeginListen,
        "only the confirmed bin may complete"
    );
}

#[test]
fn scope_refuses_window_crossing_route_before_touching_receiver() {
    use uv98_spectrum::Window;
    let (r, now) = ready();
    let mut scope = scope::Scope::new();
    let mut outside = rx();
    outside.frequency_10hz = 66_000_000;
    let window = Window::centered(66_000_000, 200_000, 2_500).unwrap();
    assert!(matches!(
        scope.start_window(&r, &cal(), outside, window, now),
        Err(scope::StartError::Qualification(_))
    ));
    assert!(!scope.active());
    assert_eq!(scope.range(), None);
}

#[test]
fn scope_temporary_listen_returns_exact_original_then_discard_removes_stale_window() {
    let (mut r, mut now) = ready();
    let mut scope = scope::Scope::new();
    scope.start(&r, &cal(), rx(), 1250, now).unwrap();
    assert!(!scope.begin_listen(now), "sweep is not restored yet");
    for _ in 0..150 {
        scope_pass(&mut r, &mut scope, &mut now);
        if scope.plot().valid(3) {
            break;
        }
    }
    scope.event(now, uv98_spectrum::compact::Event::Accept { index: 3 });
    let mut selected = None;
    for _ in 0..200 {
        if let scope::Completion::Restored { request, .. } =
            scope_pass(&mut r, &mut scope, &mut now)
        {
            selected = Some(request);
            break;
        }
    }
    let selected = selected.expect("selected RX restored");
    assert_eq!(
        selected.frequency_10hz,
        scope.range().unwrap().frequency_10hz(3).unwrap()
    );
    assert!(scope.begin_listen(now));
    assert!(scope.listening() && scope.active());
    assert!(!scope.listen_expired(now));
    assert!(!scope.listen_expired(now + 14_999));
    assert!(scope.listen_expired(now + 15_000));

    assert_eq!(scope.end_listen(&mut r, &cal(), now).unwrap(), None);
    assert!(scope.listen_restoring());
    pump(&mut r, &mut now, false);
    assert_eq!(
        scope.end_listen(&mut r, &cal(), now).unwrap(),
        Some(rx()),
        "listen must return the exact original receive plan"
    );
    assert!(!scope.active());
    assert_eq!(r.state(), State::Receiving);

    // A refused new window must not resurrect this finished session's data.
    scope.discard();
    assert_eq!(scope.range(), None);
    assert_eq!(scope.window(), None);
    assert!(!scope.plot().valid(0));
    assert!(!scope.active());
}

#[test]
fn scope_accept_completes_only_after_full_receive_on_selected_frequency() {
    let (mut r, mut now) = ready();
    let mut scope = scope::Scope::new();
    scope.start(&r, &cal(), rx(), 1250, now).unwrap();
    for _ in 0..150 {
        scope_pass(&mut r, &mut scope, &mut now);
        if scope.plot().valid(3) {
            break;
        }
    }
    assert!(scope.plot().valid(3));
    let frequency = scope.range().unwrap().frequency_10hz(3).unwrap();
    scope.event(now, uv98_spectrum::compact::Event::Accept { index: 3 });
    assert_eq!(
        scope_pass(&mut r, &mut scope, &mut now),
        scope::Completion::None
    );
    assert_eq!(r.state(), State::Configuring);
    let mut selected = None;
    for _ in 0..200 {
        if let scope::Completion::Restored { request, .. } =
            scope_pass(&mut r, &mut scope, &mut now)
        {
            selected = Some(request.frequency_10hz);
            assert_eq!(request.width, sequence::Width::Wide);
            assert_eq!(
                (request.tone, request.tone_mode, request.option_nibble),
                (0, 0, 0)
            );
            assert_eq!(request.modulation, rx().modulation);
            break;
        }
    }
    assert_eq!(selected, Some(frequency));
    assert_eq!(r.state(), State::Receiving);
    let expected =
        sequence::sweep::QualifiedRange::new(&cal(), frequency, frequency, frequency, 1250)
            .unwrap()
            .origin_words();
    assert_eq!((r.io.words[0x38], r.io.words[0x39]), expected);
}

#[test]
fn scope_invalid_grid_does_not_touch_receiver_and_prepare_timeout_restores() {
    let (mut r, mut now) = ready();
    let mut scope = scope::Scope::new();
    r.io.events.clear();
    assert!(scope.start(&r, &cal(), rx(), 0, now).is_err());
    assert!(scope.start(&r, &cal(), rx(), 100_000, now).is_err());
    assert!(r.io.events.is_empty());
    scope.start(&r, &cal(), rx(), 1250, now).unwrap();
    now += 3000; // No command was executed before its preparation deadline.
    scope.step(&mut r, &cal(), || now).unwrap();
    assert!(matches!(
        scope.state(),
        uv98_spectrum::compact::State::Restoring(
            uv98_spectrum::compact::StopReason::PrepareTimeout
        )
    ));
    for _ in 0..200 {
        if matches!(
            scope_pass(&mut r, &mut scope, &mut now),
            scope::Completion::Restored { .. }
        ) {
            break;
        }
    }
    assert!(!scope.active());
    assert_eq!(r.state(), State::Receiving);
}

#[test]
fn scope_restore_timeout_is_terminal_and_cleans_up() {
    let (mut r, mut now) = ready();
    let mut scope = scope::Scope::new();
    scope.start(&r, &cal(), rx(), 1250, now).unwrap();
    scope.event(now, uv98_spectrum::compact::Event::Stop);
    scope.step(&mut r, &cal(), || now).unwrap();
    now += 3000; // The restore plan was deliberately not advanced.
    assert!(scope.step(&mut r, &cal(), || now).is_err());
    assert_eq!(r.state(), State::Failed);
    assert_off(&r);
}

fn audio() -> Audio {
    Audio {
        modulation: Modulation::Fm,
        gain: 58,
        dac: 15,
        squelch: 1,
        monitor: false,
        tone_required: false,
    }
}

const OFF_REQUESTS: [Event; 7] = [
    Event::Mute(true),
    Event::Hf(false, false),
    Event::Write(0x33, 0),
    Event::Write(0x36, 0x007f),
    Event::Write(0x30, 0),
    Event::Write(0x47, 0x6040),
    Event::Indicators(false, false),
];

#[test]
fn invalid_and_reentrant_calls_cancel_every_tx_prefix_including_live_tx() {
    for prefix in 0..100 {
        for operation in 0..7 {
            let (mut runtime, mut now) = ready();
            runtime.transmit(&cal(), tx(), now, true).unwrap();
            for _ in 0..prefix {
                now += 2;
                runtime.tick(now, true).unwrap();
            }
            assert!(runtime.tx_active_or_starting());
            let before = runtime.io.events.len();
            let fault = match operation {
                0 => {
                    let mut invalid = tx();
                    invalid.power = u8::MAX;
                    runtime.transmit(&cal(), invalid, now, true).unwrap_err()
                }
                1 => runtime.transmit(&cal(), tx(), now, false).unwrap_err(),
                2 => runtime.transmit(&cal(), tx(), now, true).unwrap_err(),
                3 => {
                    let mut invalid = rx();
                    invalid.frequency_10hz = 0;
                    runtime.receive(&cal(), invalid, now).unwrap_err()
                }
                4 => runtime.receive(&cal(), rx(), now).unwrap_err(),
                5 => runtime.cold_start(&cal(), now).unwrap_err(),
                _ => runtime.signal(audio(), None).unwrap_err(),
            };
            assert!(
                fault.cleanup_attempted,
                "prefix={prefix}, operation={operation}"
            );
            assert_eq!(fault.cleanup_failures, 0);
            assert_eq!(runtime.state(), State::Failed);
            assert_eq!(&runtime.io.events[before..], &OFF_REQUESTS);
            assert_off(&runtime);
            assert!(!runtime.tx_active_or_starting());
            let after = runtime.io.events.len();
            let next = runtime.tick(now + 1, true).unwrap_err();
            assert!(!next.cleanup_attempted);
            assert_eq!(runtime.io.events.len(), after); // Old plan cannot resume.
        }
    }
}

#[test]
fn request_rejections_before_initialization_do_not_claim_cleanup_or_rx_success() {
    let mut runtime = Runtime::new(Mock::new());
    let fault = runtime.transmit(&cal(), tx(), 0, true).unwrap_err();
    assert_eq!(fault.reason, Error::NotInitialized);
    assert!(!fault.cleanup_attempted);
    assert_eq!(runtime.state(), State::Off);
    assert!(runtime.io.events.is_empty());
    runtime.cold_start(&cal(), 0).unwrap();
    let fault = runtime.receive(&cal(), rx(), 1).unwrap_err();
    assert_eq!(fault.reason, Error::NotInitialized);
    assert!(fault.cleanup_attempted);
    assert_eq!(runtime.state(), State::Failed);
    assert_off(&runtime);
}

#[test]
fn every_cleanup_failure_is_reported_and_all_independent_requests_are_attempted() {
    for failed in 0..7 {
        let (mut runtime, mut now) = ready();
        runtime.transmit(&cal(), tx(), now, true).unwrap();
        pump(&mut runtime, &mut now, true);
        let before = runtime.io.events.len();
        runtime.io.fail_after = Some(runtime.io.calls + failed + 1);
        let fault = runtime.tick(now - 1, true).unwrap_err();
        assert_eq!(fault.reason, Error::Clock);
        assert!(fault.cleanup_attempted);
        assert_eq!(fault.cleanup_failures, 1 << failed);
        assert_eq!(&runtime.io.events[before..], &OFF_REQUESTS);
        assert_eq!(runtime.state(), State::Failed);
        assert!(!runtime.tx_active_or_starting());
    }
    let (mut runtime, mut now) = ready();
    runtime.transmit(&cal(), tx(), now, true).unwrap();
    pump(&mut runtime, &mut now, true);
    let before = runtime.io.events.len();
    runtime.io.fail_from = Some(runtime.io.calls + 1);
    let fault = runtime.stop().unwrap_err();
    assert!(fault.cleanup_attempted);
    assert_eq!(fault.cleanup_failures, 0x7f);
    assert_eq!(&runtime.io.events[before..], &OFF_REQUESTS);
    // Logical Failed cannot be mistaken for confirmed PA-off after bus failure.
    assert_eq!(runtime.state(), State::Failed);
    assert_ne!(runtime.io.words[0x36] & 0x80, 0);
    assert!(!runtime.stop().unwrap_err().cleanup_attempted);
}

#[test]
fn cold_start_and_tx_deadlines_use_wrapping_monotonic_time() {
    let mut runtime = Runtime::new(Mock::new());
    let mut now = u32::MAX - 16;
    runtime.cold_start(&cal(), now).unwrap();
    pump(&mut runtime, &mut now, false);
    assert!(now < 1000);
    assert!(runtime.initialized());
    runtime.receive(&cal(), rx(), now).unwrap();
    pump(&mut runtime, &mut now, false);
    // Move forward in two unambiguous half-range steps to just before wrap.
    now = now.wrapping_add(1_000_000_000);
    runtime.tick(now, false).unwrap();
    now = now.wrapping_add(1_000_000_000);
    runtime.tick(now, false).unwrap();
    now = now.wrapping_add(1_000_000_000);
    runtime.tick(now, false).unwrap();
    now = u32::MAX - 10;
    runtime.tick(now, false).unwrap();
    let start = now;
    runtime.transmit(&cal(), tx(), now, true).unwrap();
    pump(&mut runtime, &mut now, true);
    assert_eq!(runtime.state(), State::Transmitting);
    let before_deadline = start.wrapping_add(TX_TIMEOUT_MS - 1);
    assert_eq!(
        runtime.tick(before_deadline, true).unwrap().state,
        State::Transmitting
    );
    let expiry = runtime
        .tick(start.wrapping_add(TX_TIMEOUT_MS), true)
        .unwrap();
    assert!(expiry.tx_ended && expiry.tx_timed_out);
    assert_off(&runtime);
}

#[test]
fn timeout_and_release_precede_pending_enable_even_after_slow_polling() {
    for target in [Target::Cold, Target::Receive, Target::Transmit] {
        let (mut runtime, now) = if target == Target::Cold {
            (Runtime::new(Mock::new()), 0)
        } else {
            ready()
        };
        match target {
            Target::Cold => runtime.cold_start(&cal(), now).unwrap(),
            Target::Receive => runtime.receive(&cal(), rx(), now).unwrap(),
            Target::Transmit => runtime.transmit(&cal(), tx(), now, true).unwrap(),
        }
        runtime.tick(now + CONFIG_TIMEOUT_MS - 1, true).unwrap();
        assert_eq!(runtime.state(), State::Configuring);
        let before = runtime.io.events.len();
        let fault = runtime.tick(now + CONFIG_TIMEOUT_MS, true).unwrap_err();
        assert_eq!(fault.reason, Error::ConfigurationTimeout);
        assert!(fault.cleanup_attempted);
        assert_eq!(&runtime.io.events[before..], &OFF_REQUESTS);
        assert_off(&runtime);
    }
    let (mut runtime, now) = ready();
    runtime.transmit(&cal(), tx(), now, true).unwrap();
    let before = runtime.io.events.len();
    let stopped = runtime.tick(now + CONFIG_TIMEOUT_MS + 1, false).unwrap();
    assert!(stopped.tx_ended);
    assert!(!stopped.tx_timed_out);
    assert_eq!(
        &runtime.io.events[before..before + OFF_REQUESTS.len()],
        &OFF_REQUESTS
    );
    assert_off(&runtime);
}

#[test]
fn delayed_polling_never_catches_up_multiple_plan_steps_in_one_tick() {
    let (mut runtime, now) = ready();
    runtime.transmit(&cal(), tx(), now, true).unwrap();
    let before = runtime.io.events.len();
    runtime.tick(now + 1000, true).unwrap();
    // A late tick executes only the initial mute, not a burst reaching PA enable.
    assert_eq!(&runtime.io.events[before..], &[Event::Mute(true)]);
    runtime.tick(now + 2000, true).unwrap();
    assert_eq!(runtime.state(), State::Configuring);
    assert_eq!(runtime.io.words[0x33] & 0x0c, 0);
    assert_eq!(runtime.io.words[0x36] & 0x80, 0);
    runtime.tick(now + 2001, false).unwrap();
    assert_off(&runtime);
}

#[test]
fn control_verification_fault_unkeys_before_any_next_plan_operation() {
    for prefix in 0..100 {
        let (mut runtime, mut now) = ready();
        runtime.transmit(&cal(), tx(), now, true).unwrap();
        for _ in 0..prefix {
            now += 2;
            runtime.tick(now, true).unwrap();
        }
        runtime.io.verification_failure = true;
        let before = runtime.io.events.len();
        let fault = runtime.tick(now + 1, true).unwrap_err();
        assert_eq!(fault.reason, Error::Io("control ownership lost"));
        assert!(fault.cleanup_attempted);
        assert_eq!(&runtime.io.events[before..], &OFF_REQUESTS);
        assert_off(&runtime);
    }
}

#[test]
fn lighting_is_cached_and_failure_uses_the_radio_off_path() {
    let (mut r, mut now) = ready();
    r.set_backlights(false, true).unwrap();
    let before = r.io.events.len();
    r.set_backlights(false, true).unwrap();
    assert_eq!(r.io.events.len(), before);
    r.stop().unwrap();
    r.transmit(&cal(), tx(), now, true).unwrap();
    pump(&mut r, &mut now, true);
    assert_eq!(r.state(), State::Transmitting);
    r.io.fail_after = Some(r.io.calls + 1);
    let before = r.io.events.len();
    let fault = r.set_backlights(false, false).unwrap_err();
    assert!(fault.cleanup_attempted);
    assert_eq!(r.io.events[before], Event::Backlights(false, false));
    assert_eq!(&r.io.events[before + 1..], &OFF_REQUESTS);
    assert_off(&r);
}

#[test]
fn indicators_follow_actual_audio_and_tx_and_do_not_rewrite_when_unchanged() {
    let (mut r, mut now) = ready();
    r.synchronize_indicators().unwrap();
    let before = r.io.events.len();
    r.synchronize_indicators().unwrap();
    assert_eq!(r.io.events.len(), before);
    r.io.words[0x0c] = 2;
    r.observe(audio(), 0).unwrap();
    r.synchronize_indicators().unwrap();
    assert_eq!(r.io.events.last(), Some(&Event::Indicators(true, false)));
    r.stop().unwrap();
    r.transmit(&cal(), tx(), now, true).unwrap();
    pump(&mut r, &mut now, true);
    r.synchronize_indicators().unwrap();
    assert_eq!(r.io.events.last(), Some(&Event::Indicators(false, true)));
    r.tick(now + 1, false).unwrap();
    assert!(r
        .io
        .events
        .iter()
        .rev()
        .any(|e| *e == Event::Indicators(false, false)));
}

#[test]
fn closed_am_squelch_does_not_reduce_gain_before_first_audio() {
    let (mut r, _) = ready();
    let mut a = audio();
    a.modulation = Modulation::Am;
    r.io.words[0x0c] = 0;
    r.io.words[0x67] = 170;
    let initial = r.am_agc;
    for _ in 0..300 {
        assert!(!r.observe(a, 0).unwrap().audio_open);
    }
    assert_eq!(r.am_agc, initial);
    r.io.words[0x0c] = 2;
    assert!(r.observe(a, 0).unwrap().audio_open);
    assert_ne!(r.am_agc, initial);
}

fn scope_cancel_accepted_restore(cancel: uv98_spectrum::compact::Event) {
    use uv98_spectrum::compact::{Event as ScopeEvent, State as ScopeState, StopReason};
    let (mut radio, mut now) = ready();
    let original_words = (radio.io.words[0x38], radio.io.words[0x39]);
    let mut scope = scope::Scope::new();
    scope.start(&radio, &cal(), rx(), 1250, now).unwrap();
    radio.io.events.clear();
    for _ in 0..150 {
        scope_pass(&mut radio, &mut scope, &mut now);
        if scope.plot().valid(3) {
            break;
        }
    }
    assert!(scope.plot().valid(3));
    assert_ne!(
        scope.range().unwrap().frequency_10hz(3),
        Some(rx().frequency_10hz)
    );
    scope.event(now, ScopeEvent::Accept { index: 3 });
    assert_eq!(
        scope_pass(&mut radio, &mut scope, &mut now),
        scope::Completion::None
    );
    assert_eq!(radio.state(), State::Configuring);
    assert_eq!(
        scope.state(),
        ScopeState::Restoring(StopReason::Accepted { index: 3 })
    );
    // The selected-frequency restore has already started. Cancellation must
    // replace that in-flight request, not merely suppress its current UI ACK.
    scope_pass(&mut radio, &mut scope, &mut now);
    scope.event(now, cancel);
    if cancel == ScopeEvent::Ptt(true) {
        scope.event(now, ScopeEvent::Ptt(false)); // Short press ends before RX completes.
    }
    let expected_reason = if cancel == ScopeEvent::Ptt(true) {
        StopReason::Ptt
    } else {
        StopReason::Exit
    };
    let mut completion = None;
    for _ in 0..200 {
        let result = scope_pass(&mut radio, &mut scope, &mut now);
        if matches!(result, scope::Completion::Restored { .. }) {
            completion = Some(result);
            break;
        }
    }
    assert_eq!(
        completion,
        Some(scope::Completion::Restored {
            request: rx(),
            reason: expected_reason,
        })
    );
    assert!(!scope.active());
    assert_eq!(radio.state(), State::Receiving);
    assert_eq!((radio.io.words[0x38], radio.io.words[0x39]), original_words);
    assert!(radio.io.muted);
    assert!(!radio.io.events.iter().any(|event| match event {
        Event::Write(0x33, word) => word & 0x0c != 0,
        Event::Write(0x36, word) => *word != 0x007f,
        Event::Write(0x30, word) => ![0, 0x0200, 0xbff0, 0xbff1].contains(word),
        Event::Hf(hf, band50) => *hf || *band50,
        Event::Mute(false) | Event::Indicators(_, true) => true,
        _ => false,
    }));
}

#[test]
fn scope_short_ptt_cancels_in_flight_acceptance_even_after_release() {
    scope_cancel_accepted_restore(uv98_spectrum::compact::Event::Ptt(true));
}

#[test]
fn scope_exit_cancels_in_flight_acceptance_and_restores_original_frequency() {
    scope_cancel_accepted_restore(uv98_spectrum::compact::Event::Stop);
}

#[test]
fn am_hang_bridges_short_carrier_dips_and_expires() {
    let (mut r, now) = ready();
    let mut a = audio();
    a.modulation = Modulation::Am;
    a.monitor = false;
    r.io.words[0x0c] = 2;
    assert!(r.observe(a, 0).unwrap().audio_open);
    r.io.words[0x0c] = 0;
    r.tick(now + 119, false).unwrap();
    assert!(r.observe(a, 0).unwrap().audio_open);
    r.tick(now + 120, false).unwrap();
    assert!(!r.observe(a, 0).unwrap().audio_open);
}

#[test]
fn panned_spectrum_measures_new_center_but_exit_restores_original_receiver() {
    let (mut radio, mut now) = ready();
    let original = rx();
    let center = original.frequency_10hz + 50_000;
    let window = uv98_spectrum::Window::centered(center, 12_500, 1250).unwrap();
    let mut scope = scope::Scope::new();
    scope
        .start_panned_window(&radio, &cal(), original, window, now)
        .unwrap();
    for _ in 0..240 {
        scope_pass(&mut radio, &mut scope, &mut now);
        if scope.completed_plot().valid(10) {
            break;
        }
    }
    assert!(scope.completed_plot().valid(10));
    assert_eq!(scope.window().unwrap().center_10hz(), center);
    scope.event(now, uv98_spectrum::compact::Event::Stop);
    for _ in 0..240 {
        if let scope::Completion::Restored { request, .. } =
            scope_pass(&mut radio, &mut scope, &mut now)
        {
            assert_eq!(request, original);
            return;
        }
    }
    panic!("original receiver was not restored");
}
