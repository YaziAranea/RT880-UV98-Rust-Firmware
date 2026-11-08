//! Fixed GPIO field policy shared by MMIO and host tests. Readback verifies
//! register state only; it cannot prove external voltage or RF inactivity.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    Locked,
    Configuration,
    Latch,
    IndicatorConflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Port {
    A,
    B,
    C,
    F,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Register {
    Mode = 0,
    OutputType = 4,
    Drive = 8,
    Pull = 12,
    OutputData = 20,
    Lock = 28,
}

/// GPIO output data is read-only through this interface: latch changes must
/// use atomic per-pin CLR/SCR, never a whole-port read/modify/write.
pub(crate) trait Registers {
    fn read(&self, port: Port, register: Register) -> u32;
    fn configure(&mut self, port: Port, register: Register, value: u32);
    fn clear(&mut self, port: Port, pins: u16);
    fn set(&mut self, port: Port, pins: u16);
}

#[derive(Clone, Copy)]
struct Pin {
    port: Port,
    bit: u8,
}
impl Pin {
    const fn mask(self) -> u16 {
        1 << self.bit
    }
    const fn pair(self) -> u32 {
        3 << (self.bit * 2)
    }
}

// LCD backlight, two HF gates, RXTS/FM-off, antenna relay, audio gate,
// keypad backlight, green/red indicators and ordinary MICI mux selection.
// PA13/PA14 replace SWD functions. PC3 remains LOW (stock11C34); no method
// enables the external BT-MICI/digital path selected by PC3 HIGH.
const PINS: [Pin; 11] = [
    Pin {
        port: Port::A,
        bit: 4,
    },
    Pin {
        port: Port::B,
        bit: 11,
    },
    Pin {
        port: Port::C,
        bit: 15,
    },
    Pin {
        port: Port::C,
        bit: 4,
    },
    Pin {
        port: Port::C,
        bit: 5,
    },
    Pin {
        port: Port::F,
        bit: 0,
    },
    Pin {
        port: Port::F,
        bit: 8,
    },
    Pin {
        port: Port::A,
        bit: 9,
    },
    Pin {
        port: Port::A,
        bit: 13,
    },
    Pin {
        port: Port::A,
        bit: 14,
    },
    Pin {
        port: Port::C,
        bit: 3,
    },
];

pub(crate) struct Engine<R: Registers> {
    registers: R,
    levels: [bool; 11],
}

impl<R: Registers> Engine<R> {
    pub(crate) fn initialize(mut registers: R) -> Result<Self, ControlError> {
        // Request every LOW before validating or configuring any individual pin.
        // A stuck first latch must not prevent later independent off requests.
        for pin in PINS {
            registers.clear(pin.port, pin.mask());
        }
        let mut engine = Self {
            registers,
            levels: [false; 11],
        };
        for pin in PINS {
            if engine.high(pin) {
                return Err(ControlError::Latch);
            }
            let lock_mask = u32::from(pin.mask()) | 0x10000;
            if engine.registers.read(pin.port, Register::Lock) & lock_mask == lock_mask {
                return Err(ControlError::Locked);
            }
        }
        for pin in PINS {
            engine.output(pin)?;
        }
        engine.verify()?;
        engine.set_level(0, true)?; // Backlight only after all outputs verified.
        engine.set_level(7, true)?; // PA9 keypad backlight, not either RF indicator.
        Ok(engine)
    }

    fn high(&self, pin: Pin) -> bool {
        self.registers.read(pin.port, Register::OutputData) & u32::from(pin.mask()) != 0
    }

    fn output(&mut self, pin: Pin) -> Result<(), ControlError> {
        for (register, mask, bits) in [
            (Register::OutputType, u32::from(pin.mask()), 0),
            (Register::Drive, pin.pair(), 2 << (pin.bit * 2)),
            (Register::Pull, pin.pair(), 0),
            (Register::Mode, pin.pair(), 1 << (pin.bit * 2)),
        ] {
            let old = self.registers.read(pin.port, register);
            self.registers
                .configure(pin.port, register, (old & !mask) | bits);
        }
        self.check(pin, false)
    }

    fn check(&self, pin: Pin, high: bool) -> Result<(), ControlError> {
        for (register, mask, bits) in [
            (Register::Mode, pin.pair(), 1 << (pin.bit * 2)),
            (Register::OutputType, u32::from(pin.mask()), 0),
            (Register::Drive, pin.pair(), 2 << (pin.bit * 2)),
            (Register::Pull, pin.pair(), 0),
        ] {
            if self.registers.read(pin.port, register) & mask != bits {
                return Err(ControlError::Configuration);
            }
        }
        if self.high(pin) != high {
            return Err(ControlError::Latch);
        }
        Ok(())
    }

    fn set_level(&mut self, index: usize, high: bool) -> Result<(), ControlError> {
        let pin = PINS[index];
        if high {
            self.registers.set(pin.port, pin.mask());
        } else {
            self.registers.clear(pin.port, pin.mask());
        }
        if self.high(pin) != high {
            return Err(ControlError::Latch);
        }
        self.levels[index] = high;
        Ok(())
    }

    pub(crate) fn verify(&self) -> Result<(), ControlError> {
        for (pin, high) in PINS.iter().zip(self.levels) {
            self.check(*pin, high)?;
        }
        Ok(())
    }
    pub(crate) fn mute(&mut self, muted: bool) -> Result<(), ControlError> {
        self.set_level(6, !muted)
    }
    pub(crate) fn backlights(&mut self, display: bool, keys: bool) -> Result<(), ControlError> {
        // Atomic per-pin writes cannot modify PTT, UART, LEDs or any RF gate.
        // Validate GPIO ownership even when a requested level is unchanged.
        for (index, high) in [(0, display), (7, keys)] {
            self.check(PINS[index], self.levels[index])?;
            if self.levels[index] != high {
                self.set_level(index, high)?;
            }
        }
        Ok(())
    }
    pub(crate) fn antenna(&mut self, high: bool) -> Result<(), ControlError> {
        self.set_level(5, high)
    }
    pub(crate) fn hf_gates(&mut self, hf: bool, band50: bool) -> Result<(), ControlError> {
        // Evaluate both results before propagating either failure. No enable is
        // attempted unless both independent LOW requests passed their readback.
        let first = self.set_level(2, false);
        let second = self.set_level(1, false);
        first?;
        second?;
        if hf {
            self.set_level(2, true)?;
        }
        if band50 {
            self.set_level(1, true)?;
        }
        Ok(())
    }

    pub(crate) fn indicators(&mut self, rx: bool, tx: bool) -> Result<(), ControlError> {
        // Repeated observations do not pulse a lit indicator. Verify both pins,
        // including their GPIO modes, before accepting the unchanged state.
        if (rx || tx) && !(rx && tx) && self.levels[8] == rx && self.levels[9] == tx {
            self.check(PINS[8], rx)?;
            return self.check(PINS[9], tx);
        }
        // Both LOW requests are independent, including during cleanup after a
        // failure. Neither failed clear permits the other LED to be enabled.
        let green = self.set_level(8, false);
        let red = self.set_level(9, false);
        green?;
        red?;
        if rx && tx {
            return Err(ControlError::IndicatorConflict);
        }
        self.check(PINS[8], false)?;
        self.check(PINS[9], false)?;
        if rx {
            self.set_level(8, true)?;
        } else if tx {
            self.set_level(9, true)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::{cell::RefCell, rc::Rc, vec::Vec};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Event {
        Configure(Port, Register, u32),
        Clear(Port, u16),
        Set(Port, u16),
    }
    struct Memory {
        registers: [[u32; 8]; 4],
        events: Vec<Event>,
        stuck_high: [u32; 4],
        stuck_low: [u32; 4],
        ignore_config: Option<(Port, Register)>,
    }
    #[derive(Clone)]
    struct Mock(Rc<RefCell<Memory>>);
    impl Mock {
        fn new() -> Self {
            let mut registers = [[0xa5a5_6c3c; 8]; 4];
            for words in &mut registers {
                words[Register::Lock as usize / 4] = 0;
            }
            Self(Rc::new(RefCell::new(Memory {
                registers,
                events: Vec::new(),
                stuck_high: [0; 4],
                stuck_low: [0; 4],
                ignore_config: None,
            })))
        }
    }
    impl Registers for Mock {
        fn read(&self, port: Port, register: Register) -> u32 {
            let memory = self.0.borrow();
            let value = memory.registers[port as usize][register as usize / 4];
            if register == Register::OutputData {
                (value | memory.stuck_high[port as usize]) & !memory.stuck_low[port as usize]
            } else {
                value
            }
        }
        fn configure(&mut self, port: Port, register: Register, value: u32) {
            assert!(matches!(
                register,
                Register::Mode | Register::OutputType | Register::Drive | Register::Pull
            ));
            let mut memory = self.0.borrow_mut();
            memory.events.push(Event::Configure(port, register, value));
            if memory.ignore_config != Some((port, register)) {
                memory.registers[port as usize][register as usize / 4] = value;
            }
        }
        fn clear(&mut self, port: Port, pins: u16) {
            let mut memory = self.0.borrow_mut();
            memory.events.push(Event::Clear(port, pins));
            memory.registers[port as usize][Register::OutputData as usize / 4] &= !u32::from(pins);
        }
        fn set(&mut self, port: Port, pins: u16) {
            let mut memory = self.0.borrow_mut();
            memory.events.push(Event::Set(port, pins));
            memory.registers[port as usize][Register::OutputData as usize / 4] |= u32::from(pins);
        }
    }

    fn assert_off_prefix(events: &[Event]) {
        let expected = [
            Event::Clear(Port::A, 1 << 4),
            Event::Clear(Port::B, 1 << 11),
            Event::Clear(Port::C, 1 << 15),
            Event::Clear(Port::C, 1 << 4),
            Event::Clear(Port::C, 1 << 5),
            Event::Clear(Port::F, 1),
            Event::Clear(Port::F, 1 << 8),
            Event::Clear(Port::A, 1 << 9),
            Event::Clear(Port::A, 1 << 13),
            Event::Clear(Port::A, 1 << 14),
            Event::Clear(Port::C, 1 << 3),
        ];
        assert_eq!(&events[..11], &expected);
    }

    #[test]
    fn initialization_preloads_all_low_and_preserves_unowned_configuration_and_latches() {
        let mock = Mock::new();
        let before = mock.0.borrow().registers;
        let engine = Engine::initialize(mock.clone()).unwrap();
        engine.verify().unwrap();
        let memory = mock.0.borrow();
        assert_off_prefix(&memory.events);
        assert_eq!(
            &memory.events[memory.events.len() - 2..],
            &[Event::Set(Port::A, 1 << 4), Event::Set(Port::A, 1 << 9)]
        );
        // Independent board contract masks, deliberately not derived from PINS.
        let singles = [0x6210u32, 0x0800, 0x8038, 0x0101];
        let pairs = [0x3c0c_0300u32, 0x00c0_0000, 0xc000_0fc0, 0x0003_0003];
        for port in 0..4 {
            for (register, mask) in [
                (Register::Mode, pairs[port]),
                (Register::OutputType, singles[port]),
                (Register::Drive, pairs[port]),
                (Register::Pull, pairs[port]),
                (Register::OutputData, singles[port]),
            ] {
                let index = register as usize / 4;
                assert_eq!(
                    memory.registers[port][index] & !mask,
                    before[port][index] & !mask
                );
            }
            assert_eq!(
                memory.registers[port][Register::Lock as usize / 4],
                before[port][Register::Lock as usize / 4]
            );
        }
    }

    #[test]
    fn ordinary_microphone_mux_stays_low_and_faults_prevent_preparation() {
        let mock = Mock::new();
        mock.0.borrow_mut().stuck_high[Port::C as usize] = 1 << 3;
        assert_eq!(
            Engine::initialize(mock.clone()).err(),
            Some(ControlError::Latch)
        );
        assert_off_prefix(&mock.0.borrow().events);
        assert_eq!(mock.0.borrow().events.len(), 11);

        let mock = Mock::new();
        let mut engine = Engine::initialize(mock.clone()).unwrap();
        for enabled in [true, false] {
            engine.hf_gates(enabled, enabled).unwrap();
            engine.antenna(enabled).unwrap();
            engine.mute(enabled).unwrap();
            engine.indicators(enabled, !enabled).unwrap();
            engine.backlights(enabled, !enabled).unwrap();
            engine.verify().unwrap();
        }
        let memory = mock.0.borrow();
        assert_eq!(memory.registers[Port::C as usize][5] & 8, 0);
        assert!(!memory
            .events
            .iter()
            .any(|e| matches!(e, Event::Set(Port::C, mask) if mask & 8 != 0)));
    }

    #[test]
    fn microphone_mux_tampering_rejects_tx_tick_before_any_rf_enable() {
        struct Radio {
            gpio: Engine<Mock>,
            writes: Vec<(u8, u16)>,
        }
        impl crate::Io for Radio {
            type Error = ControlError;
            fn read(&mut self, _: u8) -> Result<u16, Self::Error> {
                panic!("A rejected first TX tick must not read RF registers")
            }
            fn write(&mut self, r: u8, value: u16) -> Result<(), Self::Error> {
                self.writes.push((r, value));
                Ok(())
            }
            fn mute(&mut self, muted: bool) -> Result<(), Self::Error> {
                self.gpio.mute(muted)
            }
            fn antenna(&mut self, high: bool) -> Result<(), Self::Error> {
                self.gpio.antenna(high)
            }
            fn hf_gates(&mut self, hf: bool, band50: bool) -> Result<(), Self::Error> {
                self.gpio.hf_gates(hf, band50)
            }
            fn indicators(&mut self, rx: bool, tx: bool) -> Result<(), Self::Error> {
                self.gpio.indicators(rx, tx)
            }
            fn backlights(&mut self, display: bool, keys: bool) -> Result<(), Self::Error> {
                self.gpio.backlights(display, keys)
            }
            fn verify_control(&mut self) -> Result<(), Self::Error> {
                self.gpio.verify()
            }
        }
        let calibration = uv98_calibration::Calibration::parse(include_bytes!(
            "../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
        ))
        .unwrap();
        for register in [Register::Mode, Register::OutputData] {
            let mock = Mock::new();
            let mut runtime = crate::Runtime::new(Radio {
                gpio: Engine::initialize(mock.clone()).unwrap(),
                writes: Vec::new(),
            });
            // Isolate the already initialized -> first TX tick boundary.
            runtime.initialized = true;
            runtime
                .transmit(
                    &calibration,
                    uv98_rf_sequence::TxRequest {
                        frequency_10hz: 43_297_500,
                        width: uv98_rf_sequence::Width::Wide,
                        tone: 0x1302,
                        tone_mode: 0,
                        option_nibble: 0,
                        power: 0,
                        mic_gain: 31,
                    },
                    0,
                    true,
                )
                .unwrap();
            mock.0.borrow_mut().registers[Port::C as usize][register as usize / 4] ^=
                if register == Register::Mode {
                    3 << 6
                } else {
                    8
                };
            let fault = runtime.tick(1, true).unwrap_err();
            assert_eq!(runtime.state(), crate::State::Failed);
            assert_eq!(
                fault.reason,
                crate::Error::Io(if register == Register::Mode {
                    ControlError::Configuration
                } else {
                    ControlError::Latch
                })
            );
            assert!(fault.cleanup_attempted);
            assert_eq!(fault.cleanup_failures, 0);
            assert_eq!(
                runtime.into_io().writes,
                [(0x33, 0), (0x36, 0x007f), (0x30, 0), (0x47, 0x6040)]
            );
        }
    }

    #[test]
    fn locked_owned_pin_blocks_configuration_after_all_off_requests_but_foreign_lock_does_not() {
        for (port, bit) in [
            (Port::A, 4),
            (Port::B, 11),
            (Port::C, 15),
            (Port::C, 4),
            (Port::C, 5),
            (Port::F, 0),
            (Port::F, 8),
            (Port::A, 9),
            (Port::A, 13),
            (Port::A, 14),
            (Port::C, 3),
        ] {
            let mock = Mock::new();
            mock.0.borrow_mut().registers[port as usize][Register::Lock as usize / 4] =
                0x10000 | (1 << bit);
            assert_eq!(
                Engine::initialize(mock.clone()).err(),
                Some(ControlError::Locked)
            );
            let memory = mock.0.borrow();
            assert_off_prefix(&memory.events);
            assert_eq!(memory.events.len(), 11);
        }
        let mock = Mock::new();
        mock.0.borrow_mut().registers[Port::C as usize][Register::Lock as usize / 4] =
            0x10000 | (1 << 2);
        Engine::initialize(mock).unwrap().verify().unwrap();
    }

    #[test]
    fn stuck_latch_or_ignored_configuration_never_claims_success_or_turns_on_backlight() {
        let mock = Mock::new();
        mock.0.borrow_mut().stuck_high[Port::A as usize] = 1 << 4;
        assert_eq!(
            Engine::initialize(mock.clone()).err(),
            Some(ControlError::Latch)
        );
        assert_eq!(mock.0.borrow().events.len(), 11);
        assert_off_prefix(&mock.0.borrow().events);
        let mock = Mock::new();
        {
            let mut memory = mock.0.borrow_mut();
            memory.registers[Port::C as usize][Register::Mode as usize / 4] = 0;
            memory.ignore_config = Some((Port::C, Register::Mode));
        }
        assert_eq!(
            Engine::initialize(mock.clone()).err(),
            Some(ControlError::Configuration)
        );
        assert!(!mock
            .0
            .borrow()
            .events
            .iter()
            .any(|event| matches!(event, Event::Set(_, _))));
    }

    #[test]
    fn either_failed_hf_clear_still_attempts_both_and_prevents_all_high_requests() {
        for broken in [Port::C, Port::B] {
            let mock = Mock::new();
            let mut engine = Engine::initialize(mock.clone()).unwrap();
            engine.hf_gates(true, true).unwrap();
            {
                let mut memory = mock.0.borrow_mut();
                memory.events.clear();
                memory.stuck_high[broken as usize] =
                    if broken == Port::C { 1 << 15 } else { 1 << 11 };
            }
            assert_eq!(engine.hf_gates(true, true), Err(ControlError::Latch));
            assert_eq!(
                mock.0.borrow().events.as_slice(),
                &[
                    Event::Clear(Port::C, 1 << 15),
                    Event::Clear(Port::B, 1 << 11)
                ]
            );
        }
    }

    #[test]
    fn successful_hf_routing_clears_both_before_enabling_and_control_checks_detect_tampering() {
        let mock = Mock::new();
        let mut engine = Engine::initialize(mock.clone()).unwrap();
        mock.0.borrow_mut().events.clear();
        engine.hf_gates(true, true).unwrap();
        assert_eq!(
            mock.0.borrow().events.as_slice(),
            &[
                Event::Clear(Port::C, 1 << 15),
                Event::Clear(Port::B, 1 << 11),
                Event::Set(Port::C, 1 << 15),
                Event::Set(Port::B, 1 << 11)
            ]
        );
        engine.antenna(true).unwrap();
        engine.mute(false).unwrap();
        engine.verify().unwrap();
        mock.0.borrow_mut().registers[Port::C as usize][Register::Pull as usize / 4] ^= 1 << 30;
        assert_eq!(engine.verify(), Err(ControlError::Configuration));
        mock.0.borrow_mut().registers[Port::C as usize][Register::Pull as usize / 4] ^= 1 << 30;
        mock.0.borrow_mut().stuck_low[Port::F as usize] = 1 << 8;
        assert_eq!(engine.verify(), Err(ControlError::Latch));
        engine.mute(true).unwrap();
        engine.verify().unwrap();
    }

    #[test]
    fn backlights_only_change_the_two_owned_latches() {
        let mock = Mock::new();
        let mut engine = Engine::initialize(mock.clone()).unwrap();
        engine.antenna(true).unwrap();
        engine.mute(false).unwrap();
        engine.indicators(true, false).unwrap();
        let before = mock.0.borrow().registers;
        mock.0.borrow_mut().events.clear();
        engine.backlights(false, true).unwrap();
        engine.backlights(false, false).unwrap();
        engine.backlights(false, false).unwrap();
        engine.verify().unwrap();
        assert_eq!(
            mock.0.borrow().events.as_slice(),
            &[Event::Clear(Port::A, 1 << 4), Event::Clear(Port::A, 1 << 9)]
        );
        let after = mock.0.borrow().registers;
        for port in 0..4 {
            for register in 0..8 {
                let lights =
                    if port == Port::A as usize && register == Register::OutputData as usize / 4 {
                        (1 << 4) | (1 << 9)
                    } else {
                        0
                    };
                assert_eq!(
                    before[port][register] & !lights,
                    after[port][register] & !lights
                );
            }
        }
        engine.backlights(true, false).unwrap();
        engine.verify().unwrap();
    }

    #[test]
    fn backlight_pin_fault_is_detected_before_any_enable() {
        let mock = Mock::new();
        let mut engine = Engine::initialize(mock.clone()).unwrap();
        engine.backlights(false, false).unwrap();
        mock.0.borrow_mut().events.clear();
        mock.0.borrow_mut().registers[Port::A as usize][Register::Mode as usize / 4] ^= 3 << 8;
        assert_eq!(
            engine.backlights(true, false),
            Err(ControlError::Configuration)
        );
        assert!(mock.0.borrow().events.is_empty());
    }

    #[test]
    fn indicators_break_before_make_and_never_touch_rf_or_backlight_latches() {
        let mock = Mock::new();
        let mut engine = Engine::initialize(mock.clone()).unwrap();
        for (rx, tx, bit) in [(true, false, 13), (false, true, 14)] {
            mock.0.borrow_mut().events.clear();
            engine.indicators(rx, tx).unwrap();
            assert_eq!(
                mock.0.borrow().events.as_slice(),
                &[
                    Event::Clear(Port::A, 1 << 13),
                    Event::Clear(Port::A, 1 << 14),
                    Event::Set(Port::A, 1 << bit),
                ]
            );
            engine.verify().unwrap();
            mock.0.borrow_mut().events.clear();
            engine.indicators(rx, tx).unwrap();
            assert!(mock.0.borrow().events.is_empty());
        }
        mock.0.borrow_mut().events.clear();
        assert_eq!(
            engine.indicators(true, true),
            Err(ControlError::IndicatorConflict)
        );
        assert_eq!(
            mock.0.borrow().events.as_slice(),
            &[
                Event::Clear(Port::A, 1 << 13),
                Event::Clear(Port::A, 1 << 14)
            ]
        );
        engine.verify().unwrap();
    }

    #[test]
    fn each_failed_indicator_clear_attempts_both_and_prevents_reenable() {
        for bit in [13, 14] {
            let mock = Mock::new();
            let mut engine = Engine::initialize(mock.clone()).unwrap();
            {
                let mut memory = mock.0.borrow_mut();
                memory.events.clear();
                memory.stuck_high[Port::A as usize] = 1 << bit;
            }
            assert_eq!(engine.indicators(true, false), Err(ControlError::Latch));
            assert_eq!(
                mock.0.borrow().events.as_slice(),
                &[
                    Event::Clear(Port::A, 1 << 13),
                    Event::Clear(Port::A, 1 << 14)
                ]
            );
        }
    }

    #[test]
    fn indicator_mode_tampering_is_detected_on_unchanged_and_changed_requests() {
        let mock = Mock::new();
        let mut engine = Engine::initialize(mock.clone()).unwrap();
        engine.indicators(true, false).unwrap();
        {
            let mut memory = mock.0.borrow_mut();
            memory.events.clear();
            memory.registers[Port::A as usize][Register::Mode as usize / 4] ^= 3 << 26;
        }
        assert_eq!(
            engine.indicators(true, false),
            Err(ControlError::Configuration)
        );
        assert!(mock.0.borrow().events.is_empty());
        assert_eq!(
            engine.indicators(false, true),
            Err(ControlError::Configuration)
        );
        assert_eq!(
            mock.0.borrow().events.as_slice(),
            &[
                Event::Clear(Port::A, 1 << 13),
                Event::Clear(Port::A, 1 << 14)
            ]
        );
    }
}
