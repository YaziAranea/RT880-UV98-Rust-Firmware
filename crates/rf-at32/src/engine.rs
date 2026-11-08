//! Fixed-mask register engine. No MCU pointers or register access by default.
use crate::{ControlLevels, Interlock};
use uv98_receive::transport::{AudioControl, BoardControl};
use uv98_rf_bus::{Direction, Pins, Profile};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Port {
    A,
    C,
    F,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ConfigRegister {
    Mode = 0x00,
    OutputType = 0x04,
    Drive = 0x08,
    Pull = 0x0c,
}

/// Register operations must terminate without panic and without retaining
/// references. RMW must be serialized with every user of the same GPIO port.
/// set/clear are atomic per-pin requests, NOT whole ODT replacements.
pub trait Registers {
    fn read_config(&mut self, port: Port, register: ConfigRegister) -> u32;
    fn write_config(&mut self, port: Port, register: ConfigRegister, value: u32);
    fn set_bits(&mut self, port: Port, mask: u16);
    fn clear_bits(&mut self, port: Port, mask: u16);
    fn read_input(&mut self, port: Port) -> u32;
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
    const fn pair_mask(self) -> u32 {
        3 << (self.bit * 2)
    }
}
const A_CS: Pin = Pin {
    port: Port::A,
    bit: 8,
};
const A_DATA: Pin = Pin {
    port: Port::C,
    bit: 9,
};
const A_SCK: Pin = Pin {
    port: Port::F,
    bit: 6,
};
const B_CS: Pin = Pin {
    port: Port::C,
    bit: 7,
};
const B_DATA: Pin = Pin {
    port: Port::C,
    bit: 6,
};
const B_SCK: Pin = Pin {
    port: Port::C,
    bit: 8,
};
const ANTENNA: Pin = Pin {
    port: Port::F,
    bit: 0,
};
const AF_MUTE: Pin = Pin {
    port: Port::F,
    bit: 8,
};

fn change<R: Registers>(r: &mut R, pin: Pin, register: ConfigRegister, clear: u32, set: u32) {
    let previous = r.read_config(pin.port, register);
    r.write_config(pin.port, register, (previous & !clear) | set);
}
fn level<R: Registers>(r: &mut R, pin: Pin, high: bool) {
    if high {
        r.set_bits(pin.port, pin.mask());
    } else {
        r.clear_bits(pin.port, pin.mask());
    }
}
fn input<R: Registers>(r: &mut R, pin: Pin) {
    // High impedance FIRST. Stock default input drive=1, push-pull, no pull.
    change(r, pin, ConfigRegister::Mode, pin.pair_mask(), 0);
    change(r, pin, ConfigRegister::OutputType, u32::from(pin.mask()), 0);
    change(
        r,
        pin,
        ConfigRegister::Drive,
        pin.pair_mask(),
        1 << (pin.bit * 2),
    );
    change(r, pin, ConfigRegister::Pull, pin.pair_mask(), 0);
}
fn output<R: Registers>(r: &mut R, pin: Pin) {
    // Caller preloads the latch before this function. Enable output LAST.
    // Stock RF outputs use drive code2 (MODERATE), push-pull, no pull.
    change(r, pin, ConfigRegister::OutputType, u32::from(pin.mask()), 0);
    change(
        r,
        pin,
        ConfigRegister::Drive,
        pin.pair_mask(),
        2 << (pin.bit * 2),
    );
    change(r, pin, ConfigRegister::Pull, pin.pair_mask(), 0);
    change(
        r,
        pin,
        ConfigRegister::Mode,
        pin.pair_mask(),
        1 << (pin.bit * 2),
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PinError {
    DataIsInput,
    DataIsOutput,
    /// Turning the MCU DATA output on while the chip is selected is refused.
    OutputWhileSelected,
}

pub struct PinEngine<R: Registers> {
    registers: R,
    cs: Pin,
    data: Pin,
    sck: Pin,
    direction: Direction,
    data_latch: bool,
    selected: bool,
}
impl<R: Registers> PinEngine<R> {
    /// New quiescent initialization policy, not verbatim stock boot:
    /// release DATA; preload CS HIGH/SCK LOW/DATA LOW; enable outputs last.
    /// Physical qualification is a caller duty. Ordinary prepared use requires
    /// inactive CS before entry; a separately reviewed diagnostic takeover may
    /// abandon an inherited transaction after first asserting external LOW
    /// requests. This routine alone proves neither pre-entry CS nor RF state.
    /// A hardware binding may expose this only through an unsafe constructor.
    pub fn prepare(mut registers: R, profile: Profile) -> Self {
        let (cs, data, sck) = match profile {
            Profile::A => (A_CS, A_DATA, A_SCK),
            Profile::B => (B_CS, B_DATA, B_SCK),
        };
        input(&mut registers, data);
        level(&mut registers, cs, true);
        level(&mut registers, sck, false);
        level(&mut registers, data, false);
        output(&mut registers, cs);
        output(&mut registers, sck);
        output(&mut registers, data);
        Self {
            registers,
            cs,
            data,
            sck,
            direction: Direction::Output,
            data_latch: false,
            selected: false,
        }
    }
}
impl<R: Registers> Pins for PinEngine<R> {
    type Error = PinError;
    fn chip_select(&mut self, high: bool) -> Result<(), PinError> {
        if !high && self.direction == Direction::Input {
            return Err(PinError::DataIsInput);
        }
        level(&mut self.registers, self.cs, high);
        self.selected = !high;
        Ok(())
    }
    fn clock(&mut self, high: bool) -> Result<(), PinError> {
        level(&mut self.registers, self.sck, high);
        Ok(())
    }
    fn data_level(&mut self, high: bool) -> Result<(), PinError> {
        if self.direction != Direction::Output {
            return Err(PinError::DataIsInput);
        }
        level(&mut self.registers, self.data, high);
        self.data_latch = high;
        Ok(())
    }
    fn data_direction(&mut self, direction: Direction) -> Result<(), PinError> {
        match direction {
            Direction::Input => input(&mut self.registers, self.data),
            Direction::Output => {
                if self.selected {
                    return Err(PinError::OutputWhileSelected);
                }
                level(&mut self.registers, self.data, self.data_latch);
                output(&mut self.registers, self.data);
            }
        }
        self.direction = direction;
        Ok(())
    }
    fn read_data(&mut self) -> Result<bool, PinError> {
        if self.direction != Direction::Input {
            return Err(PinError::DataIsOutput);
        }
        // Exactly one IDT read; no guessed setup delay or input debounce.
        Ok(self.registers.read_input(self.data.port) & u32::from(self.data.mask()) != 0)
    }
}

pub struct ControlEngine<R: Registers> {
    registers: R,
}
impl<R: Registers> ControlEngine<R> {
    /// Levels must match the caller's qualified quiescent state. No high-Z
    /// interval is inserted for relay/mute control; latch is preloaded first.
    pub fn prepare(mut registers: R, levels: ControlLevels) -> Self {
        level(&mut registers, ANTENNA, levels.antenna_pf0_high);
        level(&mut registers, AF_MUTE, levels.af_mute_pf8_high);
        output(&mut registers, ANTENNA);
        output(&mut registers, AF_MUTE);
        Self { registers }
    }
    pub fn set_antenna_pf0(&mut self, high: bool) {
        level(&mut self.registers, ANTENNA, high);
    }
    pub fn set_af_mute_pf8(&mut self, high: bool) {
        level(&mut self.registers, AF_MUTE, high);
    }
}

/// Shared testable policy used by the hardware control wrapper. No default
/// interlock and no mutable access to its registers or guard are provided.
pub struct GuardedControl<R: Registers, G: Interlock> {
    engine: ControlEngine<R>,
    interlock: G,
}
impl<R: Registers, G: Interlock> GuardedControl<R, G> {
    pub fn prepare(registers: R, levels: ControlLevels, interlock: G) -> Self {
        Self {
            engine: ControlEngine::prepare(registers, levels),
            interlock,
        }
    }
    /// Raw PF8 HIGH needs the observational interlock. LOW stays available
    /// for a mute request after an interlock failure; no RF-off claim follows.
    pub fn set_af_mute_level(&mut self, high: bool) -> Result<(), G::Error> {
        if high {
            self.interlock.check_receive_preconditions()?;
        }
        self.engine.set_af_mute_pf8(high);
        Ok(())
    }
}
impl<R: Registers, G: Interlock> BoardControl for GuardedControl<R, G> {
    type Error = G::Error;
    fn check_receive_preconditions(&mut self) -> Result<(), G::Error> {
        self.interlock.check_receive_preconditions()
    }
    fn set_antenna_pf0(&mut self, high: bool) -> Result<(), G::Error> {
        self.interlock.check_receive_preconditions()?;
        self.engine.set_antenna_pf0(high);
        Ok(())
    }
}
impl<R: Registers, G: Interlock> AudioControl for GuardedControl<R, G> {
    fn set_audio_mute(&mut self, muted: bool) -> Result<(), G::Error> {
        self.set_af_mute_level(!muted)
    }
}
