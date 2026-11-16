#[path = "../../../firmware-diagnostic/src/signaling.rs"]
mod signaling;

use signaling::{mdc_config, TransmitTones};
use std::{cell::RefCell, convert::Infallible, rc::Rc};
use uv98_calibration::Calibration;
use uv98_handheld_ui::controller::{Controller, MdcSettings, RadioName, Slot};
use uv98_radio_runtime::{
    mdc1200::{Config, Progress},
    Io, Poll, Runtime, State, TX_TIMEOUT_MS,
};
use uv98_rf_sequence::{TxRequest, Width};

const MIC_ADC: u16 = 4;
const TX_DSP: u16 = 2;
const TX_DONE: u16 = 0x8000;

struct Chip {
    registers: [u16; 128],
    writes: Vec<(u8, u16)>,
    irq: u16,
    transactions: usize,
}

#[derive(Clone)]
struct Bus(Rc<RefCell<Chip>>);

impl Bus {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(Chip {
            registers: [0; 128],
            writes: Vec::new(),
            irq: 0,
            transactions: 0,
        })))
    }

    fn reg(&self, register: u8) -> u16 {
        self.0.borrow().registers[usize::from(register)]
    }

    fn fifo_words(&self) -> usize {
        self.0
            .borrow()
            .writes
            .iter()
            .filter(|(register, _)| *register == 0x5f)
            .count()
    }

    fn finish(&self) {
        self.0.borrow_mut().irq = TX_DONE;
    }
}

impl Io for Bus {
    type Error = Infallible;

    fn read(&mut self, register: u8) -> Result<u16, Self::Error> {
        self.0.borrow_mut().transactions += 1;
        Ok(match register {
            0x02 => std::mem::take(&mut self.0.borrow_mut().irq),
            0x0c => u16::from(self.0.borrow().irq != 0),
            _ => self.reg(register),
        })
    }

    fn write(&mut self, register: u8, value: u16) -> Result<(), Self::Error> {
        let mut chip = self.0.borrow_mut();
        chip.transactions += 1;
        chip.registers[usize::from(register)] = value;
        chip.writes.push((register, value));
        Ok(())
    }

    fn mute(&mut self, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn antenna(&mut self, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn hf_gates(&mut self, _: bool, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn indicators(&mut self, _: bool, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn backlights(&mut self, _: bool, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn verify_control(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn config(mode: u8) -> Config {
    let mut view = Controller::new(&[255; 128], 14_550_000).view();
    assert_eq!(mdc_config(view, Slot::A), Config::default());
    view.mdc[1] = MdcSettings {
        mode,
        id: Some(0x1234),
    };
    assert_eq!(view.active_slot, Slot::A);
    assert_eq!(mdc_config(view, Slot::A), Config::default());
    mdc_config(view, Slot::B)
}

struct Harness {
    radio: Runtime<Bus>,
    bus: Bus,
    foreground: TransmitTones,
    config: Config,
    name: Option<RadioName>,
    now: u32,
}

impl Harness {
    fn keyed(mode: u8) -> Self {
        let bus = Bus::new();
        let mut h = Self {
            radio: Runtime::new(bus.clone()),
            bus,
            foreground: TransmitTones::new(),
            config: config(mode),
            name: None,
            now: 0,
        };
        let cal = Calibration::parse(include_bytes!(
            "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        ))
        .unwrap();
        h.radio.cold_start(&cal, h.now).unwrap();
        h.complete_configuration(false, State::Off);
        h.radio
            .transmit_with_mdc(
                &cal,
                TxRequest {
                    frequency_10hz: 14_550_000,
                    width: Width::Wide,
                    tone: 0,
                    tone_mode: 0,
                    option_nibble: 0,
                    power: 0,
                    mic_gain: 31,
                },
                h.now,
                true,
                h.config,
            )
            .unwrap();
        h.complete_configuration(true, State::Transmitting);
        h
    }

    fn complete_configuration(&mut self, held: bool, state: State) {
        for _ in 0..300 {
            if self.radio.state() != State::Configuring {
                assert_eq!(self.radio.state(), state);
                return;
            }
            self.tick(2, held, false, None);
        }
        panic!("configuration did not finish");
    }

    fn tick(&mut self, elapsed: u32, held: bool, allow_tail: bool, key: Option<u8>) -> Poll {
        self.now += elapsed;
        let keep = self
            .foreground
            .step(
                &mut self.radio,
                self.now,
                held,
                allow_tail,
                key,
                self.config,
                self.name,
            )
            .unwrap();
        self.radio.tick(self.now, keep).unwrap()
    }

    fn assert_off(&self) {
        assert_eq!(self.radio.state(), State::Off);
        assert_eq!(self.bus.reg(0x30), 0);
        assert_eq!(self.bus.reg(0x36), 0x007f);
        assert_eq!(self.bus.reg(0x58) & 1, 0);
        assert_eq!(self.bus.reg(0x59) & 0x1800, 0);
    }

    /// Only call for an authorized TX. Terminal shutdown deliberately issues
    /// all independent off requests and uses a different transaction budget.
    fn step_authorized(&mut self, held: bool, tail: bool) {
        let before = self.bus.0.borrow().transactions;
        self.tick(1, held, tail, None);
        assert!(self.bus.0.borrow().transactions - before <= 2);
    }

    fn advance_until_air(&mut self, held: bool, tail: bool) {
        for _ in 0..300 {
            self.step_authorized(held, tail);
            if matches!(self.radio.mdc_progress(), Some(Progress::OnAir { .. })) {
                return;
            }
            assert_eq!(self.bus.reg(0x59) & 0x0800, 0);
        }
        panic!("MDC preparation did not finish");
    }

    fn finish_air(&mut self, elapsed: u32, held: bool, tail: bool) -> Poll {
        self.bus.finish();
        self.tick(elapsed, held, tail, None); // See the IRQ.
        self.step_authorized(held, tail); // Acknowledge and decode its flags.
        self.tick(1, held, tail, None) // Start restoration or unkey.
    }
}

#[test]
fn both_edges_are_once_per_real_tx() {
    let mut h = Harness::keyed(3);
    assert_eq!(h.bus.reg(0x30) & (MIC_ADC | TX_DSP), TX_DSP);
    assert_eq!(h.bus.fifo_words(), 0);
    h.tick(1, true, false, None);
    assert_eq!(h.bus.fifo_words(), 0);
    h.advance_until_air(true, false);
    assert_eq!(h.bus.reg(0x59), 0x08f8);
    assert_eq!(h.bus.fifo_words(), 11);
    h.tick(1, true, false, Some(b'5'));
    assert_eq!(h.bus.fifo_words(), 11);
    h.finish_air(279, true, false);
    assert_eq!(h.radio.mdc_progress(), Some(Progress::Restoring));
    for _ in 0..16 {
        h.step_authorized(true, false);
    }
    assert_eq!(h.bus.reg(0x30) & (MIC_ADC | TX_DSP), MIC_ADC | TX_DSP);

    h.tick(1, false, true, None);
    h.advance_until_air(false, true);
    assert_eq!(h.bus.reg(0x59), 0x0838);
    assert_eq!(h.bus.fifo_words(), 22);
    assert!(h.finish_air(200, false, true).tx_ended);
    h.assert_off();
}

#[test]
fn release_during_dtmf_moves_directly_to_fsk_and_a_new_ptt_aborts_it() {
    let mut h = Harness::keyed(2);
    h.tick(1, true, false, Some(b'5'));
    h.tick(12, true, false, Some(b'5'));
    assert_eq!(h.bus.reg(0x30) & (MIC_ADC | TX_DSP), TX_DSP);
    assert_eq!(h.bus.reg(0x70), 0xd3d3);
    let before = h.bus.0.borrow().writes.len();

    h.tick(1, false, true, None);
    assert_eq!(h.bus.reg(0x70), 0);
    h.advance_until_air(false, true);
    assert_eq!(h.bus.reg(0x70), 0x00e0);
    assert!(!h.bus.0.borrow().writes[before..]
        .iter()
        .any(|&(reg, word)| reg == 0x30 && word & MIC_ADC != 0));
    assert_eq!(h.bus.fifo_words(), 11);
    assert!(h.tick(1, false, false, None).tx_ended);
    h.assert_off();
}

#[test]
fn release_or_chord_during_preparation_never_arms_fsk() {
    // 13 capture steps, 26 FIFO/setup operations and one arm transition.
    // Even a cancellation at the final guard must precede the FSK enable.
    for (steps, clean_release) in [(0, true), (5, false), (18, true), (40, false)] {
        let mut h = Harness::keyed(3);
        h.tick(1, true, false, None);
        for _ in 0..steps {
            h.step_authorized(true, false);
        }
        assert_eq!(h.radio.mdc_progress(), Some(Progress::Preparing));
        assert!(h.tick(1, false, clean_release, None).tx_ended);
        assert!(!h
            .bus
            .0
            .borrow()
            .writes
            .iter()
            .any(|&(reg, value)| { reg == 0x59 && value & 0x0800 != 0 }));
        h.assert_off();
    }
}

#[test]
fn release_or_tot_at_last_restore_guard_does_not_reenable_mic() {
    for elapsed in [1, TX_TIMEOUT_MS] {
        let mut h = Harness::keyed(3);
        h.tick(1, true, false, None);
        h.advance_until_air(true, false);
        h.finish_air(279, true, false);
        assert_eq!(h.radio.mdc_progress(), Some(Progress::Restoring));
        for _ in 0..14 {
            h.step_authorized(true, false);
            assert_eq!(h.bus.reg(0x30) & MIC_ADC, 0);
        }
        assert!(h.tick(elapsed, false, false, None).tx_ended);
        assert!(!h
            .bus
            .0
            .borrow()
            .writes
            .iter()
            .any(|&(reg, value)| { reg == 0x30 && value & MIC_ADC != 0 }));
        h.assert_off();
    }
}

#[test]
fn unsafe_release_and_tot_terminate_without_an_end_id() {
    for elapsed in [1, TX_TIMEOUT_MS] {
        let mut h = Harness::keyed(2);
        h.tick(1, true, false, None);
        assert!(h.tick(elapsed, false, false, None).tx_ended);
        assert_eq!(h.bus.fifo_words(), 0);
        h.assert_off();
    }
}

#[test]
fn mdc_off_releases_active_dtmf_without_an_end_burst() {
    let mut h = Harness::keyed(0);
    assert_eq!(signaling::keypad(0), None);
    h.tick(1, true, false, Some(b'5'));
    h.tick(12, true, false, Some(b'5'));
    assert_eq!(h.bus.reg(0x70), 0xd3d3);
    let before = h.bus.0.borrow().writes.len();
    assert!(h.tick(1, false, true, None).tx_ended);
    assert_eq!(h.bus.reg(0x70) & 0x8080, 0);
    assert!(!h.bus.0.borrow().writes[before..]
        .iter()
        .any(|&(reg, word)| reg == 0x30 && word & MIC_ADC != 0));
    assert_eq!(h.bus.fifo_words(), 0);
    h.assert_off();
}

#[test]
fn stock_name_tail_sends_the_original_fifo_words_once_after_clean_release() {
    let name = RadioName::from_payload(*b"AZ QX R0ABC\xFF\xFF\xFF\xFF\xFF").unwrap();
    // Numeric PTT-ID end is configured too. The name takes that end slot.
    let mut h = Harness::keyed(2);
    h.name = Some(name);
    h.tick(1, true, false, None);
    assert_eq!(h.bus.fifo_words(), 0);
    assert_eq!(h.bus.reg(0x30) & MIC_ADC, MIC_ADC);

    // The accepted name belongs to this voice session, even if UI changes.
    h.name = None;
    h.tick(1, false, true, None);
    h.advance_until_air(false, true);
    for (reg, word) in [
        (0x58, 0x37c3),
        (0x59, 0x2828),
        (0x5a, 0x85cf),
        (0x5b, 0xab45),
        (0x5c, 0x5665),
        (0x5d, 0x0f00),
        (0x70, 0x00e0),
        (0x72, 0x3065),
    ] {
        assert_eq!(h.bus.reg(reg), word, "register {reg:#x}");
    }
    assert_eq!(h.bus.reg(0x30) & MIC_ADC, 0);
    let words: Vec<_> = h
        .bus
        .0
        .borrow()
        .writes
        .iter()
        .filter_map(|&(reg, word)| (reg == 0x5f).then_some(word))
        .collect();
    assert_eq!(
        words,
        [0x415a, 0x2051, 0x5820, 0x5230, 0x4142, 0x43ff, 0xffff, 0xffff]
    );

    assert!(h.finish_air(167, false, true).tx_ended);
    h.assert_off();
    h.tick(1, false, true, None);
    assert_eq!(h.bus.fifo_words(), 8);
}

#[test]
fn name_tail_aborts_on_renewed_ptt_or_missing_completion() {
    for (elapsed, held, tail) in [(1, true, false), (220, false, true)] {
        let mut h = Harness::keyed(0);
        h.name = RadioName::from_payload(*b"QRST UVWXYZ 0123");
        h.tick(1, true, false, None);
        h.tick(1, false, true, None);
        h.advance_until_air(false, true);
        assert!(h.tick(elapsed, held, tail, None).tx_ended);
        h.assert_off();
    }
}

#[test]
fn name_tail_requires_clean_release_and_remaining_tot() {
    for elapsed in [1, TX_TIMEOUT_MS] {
        let mut h = Harness::keyed(0);
        h.name = RadioName::from_payload(*b"QRST UVWXYZ 0123");
        h.tick(1, true, false, None);
        assert!(h.tick(elapsed, false, false, None).tx_ended);
        h.assert_off();
        assert_eq!(h.bus.fifo_words(), 0);
    }
}
