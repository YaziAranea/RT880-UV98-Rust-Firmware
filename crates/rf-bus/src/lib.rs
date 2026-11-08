//! Three-wire, 8-bit address / 16-bit word transport from related RT880 stock.
//!
//! Only logical pin operations are provided. No MMIO, automatic GPIO setup,
//! RF initialization, reset, tuning, or transmission policy is present.
//! Successful calls reproduce the observed A/B operation order. Errors use a
//! new abort policy: attempt CS high and DATA input, then permanently poison.
//! Neither successful transport nor cleanup proves any physical RF-off state.
//!
//! See research/rf-bus/EVIDENCE-RU.txt for timing and board preconditions.

#![no_std]
#![forbid(unsafe_code)]

/// Each successful register transaction has exactly 24 rising clock requests.
pub const TRANSACTION_CLOCKS: usize = 24;
pub const MAX_TRANSACTION_DELAYS: usize = 49;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    /// U201: CS PA8, SCK PF6, DATA PC9 in the related schematic.
    /// DATA must initially be output; a successful read restores output.
    A,
    /// U400: CS PC7, SCK PC8, DATA PC6 in the related schematic.
    /// Each transaction sets output; a successful read leaves DATA input.
    B,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Input,
    Output,
}

/// Qualified, synchronous, exclusively owned logical pins for ONE RF chip.
///
/// Methods must terminate within an independently established bound, not panic,
/// retain references, or silently schedule DMA. An error may mean the requested
/// level/direction was partially applied. `chip_select(true)` means HIGH
/// (deselected); all other bools are also physical pin levels, not active flags.
/// `read_data` must return a valid sample after the immediately preceding SCK
/// high request. The stock has no explicit sample-setup delay at that point.
///
/// GPIO clock, pinmux, electrical mode, ownership, interrupts and competing
/// accesses are the adapter's responsibility; this trait configures none.
pub trait Pins {
    type Error;
    fn chip_select(&mut self, high: bool) -> Result<(), Self::Error>;
    fn clock(&mut self, high: bool) -> Result<(), Self::Error>;
    fn data_level(&mut self, high: bool) -> Result<(), Self::Error>;
    fn data_direction(&mut self, direction: Direction) -> Result<(), Self::Error>;
    fn read_data(&mut self) -> Result<bool, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    WriteSetup,
    WriteHigh,
    ReadTurnaround,
    ReadHigh,
    ReadLow,
}

/// Caller-supplied bounded delay, corresponding to stock delay(10) calls.
///
/// The integer 10 is NOT declared to be microseconds or CPU cycles here.
/// Implementations must establish timing for the actual clock/board, terminate
/// without panic and preserve minimum setup/high/low times under interrupts.
/// ReadTurnaround occurs after the address, BEFORE DATA switches to input.
/// ReadHigh occurs AFTER sampling DATA; it cannot supply sample-setup time.
pub trait Delay {
    fn wait(&mut self, phase: Phase);
}

#[derive(Debug, Eq, PartialEq)]
pub struct Fault<E> {
    pub cause: E,
    /// None means the cleanup request returned success, not verified CS voltage.
    pub chip_select_cleanup: Option<E>,
    /// Input/high impedance request attempted even if CS cleanup failed.
    pub data_release_cleanup: Option<E>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Error<E> {
    InvalidRegister(u8),
    Pin(Fault<E>),
    Poisoned,
}

pub struct Bus<P: Pins, D: Delay> {
    pins: P,
    delay: D,
    profile: Profile,
    poisoned: bool,
    started: bool,
}

impl<P: Pins, D: Delay> Bus<P, D> {
    /// Inert construction: no pin/delay call, no stock RF initialization.
    ///
    /// Before the first operation the caller must qualify wiring, pinmux,
    /// power/clock, CS high and exclusive ownership. Profile A additionally
    /// requires DATA output. Delay and read sampling contracts must hold.
    /// Reconstructing a Bus after a fault is not an automatic recovery method;
    /// the physical bus/device state first needs independent requalification.
    pub const fn new(pins: P, delay: D, profile: Profile) -> Self {
        Self {
            pins,
            delay,
            profile,
            poisoned: false,
            started: false,
        }
    }

    pub const fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    pub const fn profile(&self) -> Profile {
        self.profile
    }

    fn validate(&self, register: u8) -> Result<(), Error<P::Error>> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        if register & 0x80 != 0 {
            return Err(Error::InvalidRegister(register));
        }
        Ok(())
    }

    fn begin(&mut self) -> Result<(), P::Error> {
        self.started = true;
        if matches!(self.profile, Profile::B) {
            self.pins.data_direction(Direction::Output)?;
        }
        self.pins.clock(false)?;
        self.pins.chip_select(false)
    }

    fn write_byte(&mut self, byte: u8) -> Result<(), P::Error> {
        for shift in (0..8).rev() {
            self.pins.clock(false)?;
            self.pins.data_level(byte & (1 << shift) != 0)?;
            self.delay.wait(Phase::WriteSetup);
            self.pins.clock(true)?;
            self.delay.wait(Phase::WriteHigh);
        }
        Ok(())
    }

    fn failed(&mut self, cause: P::Error) -> Error<P::Error> {
        self.poisoned = true;
        // Do not clock anything else after a fault: CS may itself be broken.
        let chip_select_cleanup = self.pins.chip_select(true).err();
        let data_release_cleanup = self.pins.data_direction(Direction::Input).err();
        Error::Pin(Fault {
            cause,
            chip_select_cleanup,
            data_release_cleanup,
        })
    }

    /// Transmit register, word high byte, word low byte, all MSB first.
    /// An error may have partially written a register; no retry is performed.
    pub fn write(&mut self, register: u8, word: u16) -> Result<(), Error<P::Error>> {
        self.validate(register)?;
        let result = (|| {
            self.begin()?;
            self.write_byte(register)?;
            self.write_byte((word >> 8) as u8)?;
            self.write_byte(word as u8)?;
            self.pins.chip_select(true)
        })();
        result.map_err(|cause| self.failed(cause))
    }

    /// Send register|80h, turn DATA around, sample 16 bits MSB first.
    /// Every returned error discards the partial word.
    pub fn read(&mut self, register: u8) -> Result<u16, Error<P::Error>> {
        self.validate(register)?;
        let result = (|| {
            self.begin()?;
            self.write_byte(register | 0x80)?;
            self.delay.wait(Phase::ReadTurnaround);
            self.pins.data_direction(Direction::Input)?;
            self.pins.clock(false)?;
            let mut word = 0u16;
            for _ in 0..16 {
                self.pins.clock(true)?;
                word = (word << 1) | u16::from(self.pins.read_data()?);
                self.delay.wait(Phase::ReadHigh);
                self.pins.clock(false)?;
                self.delay.wait(Phase::ReadLow);
            }
            self.pins.chip_select(true)?;
            if matches!(self.profile, Profile::A) {
                self.pins.data_direction(Direction::Output)?;
            }
            Ok(word)
        })();
        result.map_err(|cause| self.failed(cause))
    }
}

impl<P: Pins, D: Delay> Drop for Bus<P, D> {
    fn drop(&mut self) {
        if self.started {
            // Best effort only. No clock or RF shutdown command is issued.
            let _ = self.pins.chip_select(true);
            let _ = self.pins.data_direction(Direction::Input);
        }
    }
}
