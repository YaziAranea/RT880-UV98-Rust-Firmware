extern crate std;
use super::*;
use std::vec::Vec;
use uv98_calibration::Calibration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Read(u8),
    Write(u8, u16),
    Mute(bool),
    Hf(bool, bool),
    Antenna(bool),
    Indicators(bool, bool),
    Lights(bool, bool),
    Verify,
}
struct Mock {
    words: [u16; 128],
    events: Vec<Event>,
    calls: usize,
    fail_at: Option<usize>,
    mismatch: Option<u8>,
}
impl Mock {
    fn event(&mut self, event: Event) -> Result<(), &'static str> {
        self.events.push(event);
        let index = self.calls;
        self.calls += 1;
        if self.fail_at == Some(index) {
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
    fn write(&mut self, r: u8, value: u16) -> Result<(), Self::Error> {
        self.event(Event::Write(r, value))?;
        self.words[r as usize] = value;
        Ok(())
    }
    fn mute(&mut self, muted: bool) -> Result<(), Self::Error> {
        self.event(Event::Mute(muted))
    }
    fn antenna(&mut self, high: bool) -> Result<(), Self::Error> {
        self.event(Event::Antenna(high))
    }
    fn hf_gates(&mut self, hf: bool, b50: bool) -> Result<(), Self::Error> {
        self.event(Event::Hf(hf, b50))
    }
    fn indicators(&mut self, rx: bool, tx: bool) -> Result<(), Self::Error> {
        self.event(Event::Indicators(rx, tx))
    }
    fn backlights(&mut self, display: bool, keys: bool) -> Result<(), Self::Error> {
        self.event(Event::Lights(display, keys))
    }
    fn verify_control(&mut self) -> Result<(), Self::Error> {
        self.event(Event::Verify)
    }
}

fn calibration() -> Calibration<'static> {
    Calibration::parse(include_bytes!(
        "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
    ))
    .unwrap()
}
fn ready() -> (Runtime<Mock>, QualifiedRange) {
    let range = QualifiedRange::new(&calibration(), 12_640_000, 12_639_999, 12_640_001, 1).unwrap();
    let mut words = [0; 128];
    let (low, high) = range.origin_words();
    words[0x38] = low;
    words[0x39] = high;
    words[0x33] = 0xa510;
    words[0x36] = 0x007f;
    words[0x30] = 0xbff1;
    words[0x67] = 0xa55a;
    let mut runtime = Runtime::new(Mock {
        words,
        events: Vec::new(),
        calls: 0,
        fail_at: None,
        mismatch: None,
    });
    runtime.initialized = true;
    runtime.state = State::Receiving;
    runtime.audio_open = true;
    (runtime, range)
}
fn clear(runtime: &mut Runtime<Mock>) {
    runtime.io.events.clear();
    runtime.io.calls = 0;
}
fn assert_no_tx(runtime: &Runtime<Mock>) {
    assert!(!runtime.io.events.iter().any(|e| match e {
        Event::Write(0x36, v) => *v != 0x007f,
        Event::Write(0x33, v) => *v & 0x0c != 0,
        Event::Write(0x30, v) => ![0, 0x0200, 0xbff1].contains(v),
        Event::Hf(a, b) => *a || *b,
        Event::Mute(false) => true,
        Event::Indicators(_, true) => true,
        _ => false,
    }));
}
fn assert_failed_off(runtime: &Runtime<Mock>) {
    assert_eq!(runtime.state(), State::Failed);
    assert!(runtime.sweep_session.is_none());
    assert_eq!(runtime.io.words[0x33], 0);
    assert_eq!(runtime.io.words[0x36], 0x007f);
    assert_eq!(runtime.io.words[0x30], 0);
    assert_no_tx(runtime);
}

#[test]
fn enter_verifies_origin_mutes_and_pins_fixed_gain() {
    let (mut runtime, range) = ready();
    runtime.am_gain_word = Some(0x1111);
    runtime.sweep_enter(range, 1).unwrap();
    assert_eq!(runtime.state(), State::Sweeping);
    assert!(!runtime.audio_open());
    assert_eq!(runtime.io.words[0x13], 0x03be);
    assert_eq!(runtime.io.words[0x47], 0x6040);
    assert_eq!(runtime.am_gain_word, None);
    assert!(runtime.io.events.contains(&Event::Antenna(false)));
    assert_no_tx(&runtime);
    assert!(core::mem::size_of::<Session>() <= 64);
}

#[test]
fn six_writes_use_fresh_route_word_and_settle_starts_after_io_ack() {
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, 1).unwrap();
    runtime.io.words[0x33] = 0xa550; // Fresh unrelated GPIO bits must survive.
    clear(&mut runtime);
    runtime.sweep_tune(0, 10).unwrap();
    let writes: Vec<_> = runtime
        .io
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Write(r, v) => Some((*r, *v)),
            _ => None,
        })
        .collect();
    assert_eq!(
        writes,
        [
            (0x38, 0xdeff),
            (0x39, 0x00c0),
            (0x37, 0x1f0f),
            (0x30, 0x0200),
            (0x30, 0xbff1),
            (0x33, 0xa550)
        ]
    );
    clear(&mut runtime);
    // Suppose tuning consumed90ms: no early sample using pre-I/O time10.
    assert_eq!(runtime.sweep_read_rssi(100).unwrap(), None);
    assert_eq!(runtime.sweep_read_rssi(109).unwrap(), None);
    assert!(!runtime.io.events.contains(&Event::Read(0x67)));
    assert_eq!(
        runtime.sweep_read_rssi(110).unwrap(),
        Some(Sample {
            index: 0,
            frequency_10hz: 12_639_999,
            raw: 0x015a,
        })
    );
    assert_eq!(
        runtime
            .io
            .events
            .iter()
            .filter(|e| **e == Event::Read(0x67))
            .count(),
        1
    );
    assert_no_tx(&runtime);
    runtime.sweep_tune(1, 111).unwrap();
}

#[test]
fn new_session_restarts_index_and_stale_or_duplicate_calls_fail_closed() {
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, 1).unwrap();
    runtime.sweep_tune(0, 10).unwrap();
    assert!(runtime.sweep_tune(1, 11).is_err()); // Pending sample cannot be replaced.
    assert_failed_off(&runtime);
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, 1).unwrap();
    assert!(runtime.sweep_tune(1, 2).is_err()); // Out-of-order firstbin.
    assert_failed_off(&runtime);
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, 1).unwrap();
    runtime.sweep_tune(0, 2).unwrap();
    runtime.sweep_read_rssi(3).unwrap();
    runtime.sweep_read_rssi(13).unwrap();
    assert!(runtime.sweep_read_rssi(14).is_err());
    assert_failed_off(&runtime);
}

#[test]
fn entry_rejects_every_unprepared_gate_control_frequency_or_gain_readback() {
    for (register, value) in [
        (0x33, 0xa514),
        (0x33, 0xa520),
        (0x36, 0x5cff),
        (0x30, 0xc1fe),
        (0x38, 0),
        (0x39, 0),
    ] {
        let (mut runtime, range) = ready();
        runtime.io.words[register] = value;
        assert!(runtime.sweep_enter(range, 1).is_err());
        assert_failed_off(&runtime);
    }
    let (mut runtime, range) = ready();
    runtime.io.mismatch = Some(0x13);
    assert!(runtime.sweep_enter(range, 1).is_err());
    assert_failed_off(&runtime);
}

#[test]
fn any_io_failure_in_entry_or_tune_attempts_terminal_off() {
    let (mut complete, range) = ready();
    complete.sweep_enter(range, 1).unwrap();
    let entry_calls = complete.io.calls;
    clear(&mut complete);
    complete.sweep_tune(0, 10).unwrap();
    let tune_calls = complete.io.calls;
    for at in 0..entry_calls {
        let (mut runtime, range) = ready();
        runtime.io.fail_at = Some(at);
        let fault = runtime.sweep_enter(range, 1).unwrap_err();
        assert!(fault.cleanup_attempted);
        assert_failed_off(&runtime);
    }
    for at in 0..tune_calls {
        let (mut runtime, range) = ready();
        runtime.sweep_enter(range, 1).unwrap();
        clear(&mut runtime);
        runtime.io.fail_at = Some(at);
        let fault = runtime.sweep_tune(0, 10).unwrap_err();
        assert!(fault.cleanup_attempted);
        assert_failed_off(&runtime);
    }
}

#[test]
fn control_readback_failure_or_new_tx_bits_abort_tune_before_enable() {
    for bad in [0x38, 0x39, 0x30] {
        let (mut runtime, range) = ready();
        runtime.sweep_enter(range, 1).unwrap();
        runtime.io.mismatch = Some(bad);
        assert!(runtime.sweep_tune(0, 2).is_err());
        assert_failed_off(&runtime);
    }
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, 1).unwrap();
    runtime.io.words[0x33] |= 4;
    clear(&mut runtime);
    assert!(runtime.sweep_tune(0, 2).is_err());
    assert_failed_off(&runtime);
    assert!(!runtime
        .io
        .events
        .iter()
        .any(|e| matches!(e, Event::Write(0x38 | 0x39, _))));
}

#[test]
fn clock_wrap_timeout_cancel_and_no_transmit_in_sweep() {
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, u32::MAX - 12).unwrap();
    runtime.sweep_tune(0, u32::MAX - 11).unwrap();
    assert_eq!(runtime.sweep_read_rssi(u32::MAX - 9).unwrap(), None);
    assert!(runtime.sweep_read_rssi(0).unwrap().is_some());
    runtime.sweep_tune(1, 1).unwrap();
    assert!(runtime.sweep_read_rssi(3001).is_err());
    assert_failed_off(&runtime);
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, 1).unwrap();
    runtime.sweep_tune(0, 2).unwrap();
    runtime.stop().unwrap(); // Root's EXIT/PTT cancellation uses this path.
    assert!(runtime.sweep_session.is_none());
    assert_eq!(runtime.state(), State::Off);
    clear(&mut runtime);
    assert!(runtime.sweep_read_rssi(3).is_err());
    assert!(!runtime.io.events.contains(&Event::Read(0x67)));
    assert_failed_off(&runtime);
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, 1).unwrap();
    let tx = uv98_rf_sequence::TxRequest {
        frequency_10hz: 43_297_500,
        width: uv98_rf_sequence::Width::Wide,
        tone: 0x1302,
        tone_mode: 0,
        option_nibble: 0,
        power: 0,
        mic_gain: 31,
    };
    assert!(runtime.transmit(&calibration(), tx, 2, true).is_err());
    assert_failed_off(&runtime);
}

#[test]
fn sample_anchor_and_every_fresh_rssi_io_failure_clean_up_without_a_sample() {
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, 1).unwrap();
    runtime.sweep_tune(0, 2).unwrap();
    clear(&mut runtime);
    runtime.io.fail_at = Some(0); // GPIO verification at the settle anchor.
    let fault = runtime.sweep_read_rssi(3).unwrap_err();
    assert_eq!(fault.reason, Error::Io("injected"));
    assert_failed_off(&runtime);
    assert!(!runtime.io.events.contains(&Event::Read(0x67)));

    // Four independent dependencies: control pins, fresh TX gates, PA-off
    // readback, and the RSSI transaction itself. None may yield a stale sample.
    for at in 0..4 {
        let (mut runtime, range) = ready();
        runtime.sweep_enter(range, 1).unwrap();
        runtime.sweep_tune(0, 2).unwrap();
        assert_eq!(runtime.sweep_read_rssi(3).unwrap(), None);
        clear(&mut runtime);
        runtime.io.fail_at = Some(at);
        let fault = runtime.sweep_read_rssi(13).unwrap_err();
        assert_eq!(fault.reason, Error::Io("injected"));
        assert!(fault.cleanup_attempted);
        assert_failed_off(&runtime);
        let expected = [
            Event::Verify,
            Event::Read(0x33),
            Event::Read(0x36),
            Event::Read(0x67),
        ];
        assert_eq!(&runtime.io.events[..at + 1], &expected[..at + 1]);
        assert_eq!(
            runtime
                .io
                .events
                .iter()
                .filter(|event| **event == Event::Read(0x67))
                .count(),
            usize::from(at == 3)
        );
    }
}

#[test]
fn sample_gate_pa_readback_and_backward_clock_fail_before_rssi() {
    for (register, word) in [(0x33, 0xa514), (0x33, 0xa518), (0x36, 0x5cff)] {
        let (mut runtime, range) = ready();
        runtime.sweep_enter(range, 1).unwrap();
        runtime.sweep_tune(0, 2).unwrap();
        runtime.sweep_read_rssi(3).unwrap();
        runtime.io.words[register] = word;
        clear(&mut runtime);
        let fault = runtime.sweep_read_rssi(13).unwrap_err();
        assert!(matches!(fault.reason, Error::Readback { .. }));
        assert_failed_off(&runtime);
        assert!(!runtime.io.events.contains(&Event::Read(0x67)));
    }
    let (mut runtime, range) = ready();
    runtime.sweep_enter(range, 1).unwrap();
    runtime.sweep_tune(0, 2).unwrap();
    runtime.sweep_read_rssi(10).unwrap();
    clear(&mut runtime);
    assert_eq!(runtime.sweep_read_rssi(9).unwrap_err().reason, Error::Clock);
    assert_failed_off(&runtime);
    assert!(!runtime.io.events.contains(&Event::Read(0x67)));
}
