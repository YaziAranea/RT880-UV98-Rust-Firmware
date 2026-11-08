//! Fixed-mask diagnostic GPIO policy, shared by hardware and host tests.
use crate::{Backlight, ExternalOff};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Port {
    A,
    B,
    C,
    F,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Register {
    Mode = 0,
    OutputType = 4,
    Drive = 8,
    Pull = 12,
    OutputData = 20,
}

/// Serialized bounded accesses to exclusively owned fields. OutputData is read
/// only; low/high changes must use atomic per-pin GPIO CLR/SCR requests.
pub trait Registers {
    fn read(&mut self, port: Port, register: Register) -> u32;
    fn configure(&mut self, port: Port, register: Register, value: u32);
    fn clear(&mut self, port: Port, pins: u16);
    fn set(&mut self, port: Port, pins: u16);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
const BACKLIGHT: Pin = Pin {
    port: Port::A,
    bit: 4,
};
const RF_LOW: [Pin; 5] = [
    Pin {
        port: Port::C,
        bit: 15,
    },
    Pin {
        port: Port::B,
        bit: 11,
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
        bit: 8,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Configuration {
        port: Port,
        pin: u8,
        register: Register,
        observed: u32,
    },
    HighLatch {
        port: Port,
        pin: u8,
    },
    BacklightLatch {
        requested_high: bool,
        observed_high: bool,
    },
}
pub struct Engine<R: Registers> {
    registers: R,
}
impl<R: Registers> Engine<R> {
    /// Inert ownership acquisition. The caller must already assert early LOW
    /// requests before RF bus takeover. assert_off reasserts/validates them.
    pub const fn new(registers: R) -> Self {
        Self { registers }
    }
    pub(crate) fn into_registers(self) -> R {
        self.registers
    }
    fn field(&mut self, pin: Pin, register: Register, mask: u32, value: u32) {
        let old = self.registers.read(pin.port, register);
        self.registers
            .configure(pin.port, register, (old & !mask) | value);
    }
    fn output(&mut self, pin: Pin) {
        self.field(pin, Register::OutputType, u32::from(pin.mask()), 0);
        self.field(pin, Register::Drive, pin.pair(), 2 << (pin.bit * 2));
        self.field(pin, Register::Pull, pin.pair(), 0);
        self.field(pin, Register::Mode, pin.pair(), 1 << (pin.bit * 2));
    }
    fn check_configuration(&mut self, pin: Pin) -> Result<(), Error> {
        for (register, mask, value) in [
            (Register::Mode, pin.pair(), 1 << (pin.bit * 2)),
            (Register::OutputType, u32::from(pin.mask()), 0),
            (Register::Drive, pin.pair(), 2 << (pin.bit * 2)),
            (Register::Pull, pin.pair(), 0),
        ] {
            let observed = self.registers.read(pin.port, register);
            if observed & mask != value {
                return Err(Error::Configuration {
                    port: pin.port,
                    pin: pin.bit,
                    register,
                    observed,
                });
            }
        }
        Ok(())
    }
}
impl<R: Registers> ExternalOff for Engine<R> {
    type Error = Error;
    fn assert_off(&mut self) -> Result<(), Error> {
        // Blank first. Preload ALL low latches before changing output modes.
        self.registers.clear(BACKLIGHT.port, BACKLIGHT.mask());
        for pin in RF_LOW {
            self.registers.clear(pin.port, pin.mask());
        }
        for pin in RF_LOW {
            self.output(pin);
        }
        self.output(BACKLIGHT);
        self.check_off()?;
        self.check_configuration(BACKLIGHT)?;
        let high = self.registers.read(BACKLIGHT.port, Register::OutputData)
            & u32::from(BACKLIGHT.mask())
            != 0;
        if high {
            return Err(Error::BacklightLatch {
                requested_high: false,
                observed_high: true,
            });
        }
        Ok(())
    }
    fn check_off(&mut self) -> Result<(), Error> {
        for pin in RF_LOW {
            self.check_configuration(pin)?;
            if self.registers.read(pin.port, Register::OutputData) & u32::from(pin.mask()) != 0 {
                return Err(Error::HighLatch {
                    port: pin.port,
                    pin: pin.bit,
                });
            }
        }
        Ok(())
    }
}
impl<R: Registers> Backlight for Engine<R> {
    fn set_backlight(&mut self, on: bool) -> Result<(), Error> {
        if on {
            self.check_off()?;
            self.check_configuration(BACKLIGHT)?;
            self.registers.set(BACKLIGHT.port, BACKLIGHT.mask());
        } else {
            // Blanking is always attempted, including after a failed check.
            self.registers.clear(BACKLIGHT.port, BACKLIGHT.mask());
        }
        let observed_high = self.registers.read(BACKLIGHT.port, Register::OutputData)
            & u32::from(BACKLIGHT.mask())
            != 0;
        if observed_high != on {
            return Err(Error::BacklightLatch {
                requested_high: on,
                observed_high,
            });
        }
        self.check_off()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::{cell::RefCell, rc::Rc, vec::Vec};

    #[derive(Clone)]
    struct Mock {
        state: Rc<RefCell<Memory>>,
    }
    struct Memory {
        words: [[u32; 6]; 4],
        writes: Vec<(Port, Register, u32)>,
        levels: Vec<(Port, u16, bool)>,
    }
    fn p(port: Port) -> usize {
        match port {
            Port::A => 0,
            Port::B => 1,
            Port::C => 2,
            Port::F => 3,
        }
    }
    fn r(register: Register) -> usize {
        register as usize / 4
    }
    impl Registers for Mock {
        fn read(&mut self, port: Port, register: Register) -> u32 {
            self.state.borrow().words[p(port)][r(register)]
        }
        fn configure(&mut self, port: Port, register: Register, value: u32) {
            assert_ne!(register, Register::OutputData);
            let mut s = self.state.borrow_mut();
            s.writes.push((port, register, value));
            s.words[p(port)][r(register)] = value;
        }
        fn clear(&mut self, port: Port, pins: u16) {
            let mut s = self.state.borrow_mut();
            s.levels.push((port, pins, false));
            s.words[p(port)][r(Register::OutputData)] &= !u32::from(pins);
        }
        fn set(&mut self, port: Port, pins: u16) {
            let mut s = self.state.borrow_mut();
            s.levels.push((port, pins, true));
            s.words[p(port)][r(Register::OutputData)] |= u32::from(pins);
        }
    }
    fn mock() -> Mock {
        Mock {
            state: Rc::new(RefCell::new(Memory {
                words: [[0xa5a5_5a5a; 6]; 4],
                writes: Vec::new(),
                levels: Vec::new(),
            })),
        }
    }
    #[test]
    fn all_rf_levels_are_low_and_unowned_fields_survive() {
        let m = mock();
        let shared = m.state.clone();
        let old = shared.borrow().words;
        let mut engine = Engine::new(m);
        engine.assert_off().unwrap();
        engine.check_off().unwrap();
        let s = shared.borrow();
        let masks = [1u16 << 4, 1 << 11, 0x8030, 1 << 8];
        for port in 0..4 {
            let bits = u32::from(masks[port]);
            let pairs = (0..16)
                .filter(|i| masks[port] & (1 << i) != 0)
                .fold(0, |v, i| v | (3 << (2 * i)));
            for reg in [Register::Mode, Register::Drive, Register::Pull] {
                assert_eq!(s.words[port][r(reg)] & !pairs, old[port][r(reg)] & !pairs);
            }
            for reg in [Register::OutputType, Register::OutputData] {
                assert_eq!(s.words[port][r(reg)] & !bits, old[port][r(reg)] & !bits);
            }
            assert_eq!(s.words[port][r(Register::OutputData)] & bits, 0);
        }
        assert!(s.levels.iter().all(|(_, _, high)| !high));
        assert!(s
            .levels
            .iter()
            .all(|(port, mask, _)| *port != Port::F || mask & 1 == 0));
    }
    #[test]
    fn backlight_is_the_only_high_request_and_drift_refuses_it() {
        let m = mock();
        let shared = m.state.clone();
        let mut engine = Engine::new(m);
        engine.assert_off().unwrap();
        engine.set_backlight(true).unwrap();
        engine.check_off().unwrap();
        assert_eq!(
            shared
                .borrow()
                .levels
                .iter()
                .filter(|x| x.2)
                .cloned()
                .collect::<Vec<_>>(),
            [(Port::A, 1 << 4, true)]
        );
        shared.borrow_mut().words[p(Port::B)][r(Register::OutputData)] |= 1 << 11;
        let before = shared.borrow().levels.len();
        assert_eq!(
            engine.set_backlight(true),
            Err(Error::HighLatch {
                port: Port::B,
                pin: 11
            })
        );
        assert_eq!(shared.borrow().levels.len(), before);
        engine.assert_off().unwrap();
        assert_eq!(
            shared.borrow().words[p(Port::A)][r(Register::OutputData)] & (1 << 4),
            0
        );
    }
    #[test]
    fn pull_or_mode_changes_in_owned_pins_are_detected() {
        for reg in [
            Register::Mode,
            Register::OutputType,
            Register::Drive,
            Register::Pull,
        ] {
            let m = mock();
            let shared = m.state.clone();
            let mut engine = Engine::new(m);
            engine.assert_off().unwrap();
            let bit = if reg == Register::OutputType { 15 } else { 30 };
            shared.borrow_mut().words[p(Port::C)][r(reg)] ^= 1 << bit;
            assert!(
                matches!(engine.check_off(), Err(Error::Configuration { port: Port::C, pin: 15, register, .. }) if register == reg)
            );
        }
    }
}
