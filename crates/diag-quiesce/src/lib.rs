#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Diagnostic shutdown after early loader takeover, never an RX/TX initializer.
//!
//! All results describe requested/read-back register state. They do not certify
//! physical PA inactivity, RF silence, board identity, or safe-to-transmit state.
//! External LOW requests precede RF transactions. Any error is terminal and
//! makes one additional external-off request, including backlight blanking.

pub mod control;
#[cfg(feature = "hardware")]
pub mod hardware;

use uv98_board_policy::{Change, Chip};
use uv98_rf_bus::{Bus, Delay, Pins, Profile};

/// Bounded synchronous I/O, with no hidden retries or RX/TX initialization.
pub trait RegisterIo {
    type Error;
    fn profile(&self) -> Profile;
    fn read(&mut self, address: u8) -> Result<u16, Self::Error>;
    fn write(&mut self, address: u8, value: u16) -> Result<(), Self::Error>;
}
impl<P: Pins, D: Delay> RegisterIo for Bus<P, D> {
    type Error = uv98_rf_bus::Error<P::Error>;
    fn profile(&self) -> Profile {
        Bus::profile(self)
    }
    fn read(&mut self, address: u8) -> Result<u16, Self::Error> {
        Bus::read(self, address)
    }
    fn write(&mut self, address: u8, value: u16) -> Result<(), Self::Error> {
        Bus::write(self, address, value)
    }
}

/// Owner of the diagnostic output lines, not a receive permission provider.
/// `assert_off` requests external LOW gates/mute AND blanks the backlight;
/// `check_off` observes configuration/latches, excluding backlight brightness.
/// Implementations must be bounded and callable for cleanup after bus failure.
pub trait ExternalOff {
    type Error;
    fn assert_off(&mut self) -> Result<(), Self::Error>;
    fn check_off(&mut self) -> Result<(), Self::Error>;
}
pub trait Backlight: ExternalOff {
    /// Must never change any RF/mute/relay pin. HIGH requires an off check.
    fn set_backlight(&mut self, on: bool) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    New,
    RegisterVerified,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileMismatch {
    pub primary: Profile,
    pub secondary: Profile,
}
#[derive(Debug, Eq, PartialEq)]
pub enum Cause<A, B, C> {
    State,
    External(C),
    Primary(A),
    Secondary(B),
    Readback {
        chip: Chip,
        address: u8,
        mask: u16,
        expected: u16,
        observed: u16,
    },
}
#[derive(Debug, Eq, PartialEq)]
pub struct Failure<A, B, C> {
    pub cause: Cause<A, B, C>,
    pub external_cleanup: Option<C>,
}

/// Eight sequential readbacks, not an atomic snapshot or a physical-off token.
/// Order: A33,A36,A30,A37,B33,B36,B30,B37.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterState {
    pub words: [u16; 8],
}

#[derive(Clone, Copy)]
struct Request {
    chip: Chip,
    address: u8,
    change: Change,
    mask: u16,
    expected: u16,
}
// A combines stock TX mask FFF3 and RX mask FFCC. Applying the combined mask
// once prevents a second fresh read from retaining newly observed TX bits.
// These are newly composed shutdown operations, not a verbatim stock routine.
const REQUESTS: [Request; 8] = [
    Request {
        chip: Chip::A,
        address: 0x33,
        change: Change::Keep(0xffc0),
        mask: 0x003f,
        expected: 0,
    },
    Request {
        chip: Chip::A,
        address: 0x36,
        change: Change::Write(0x007f),
        mask: 0xffff,
        expected: 0x007f,
    },
    Request {
        chip: Chip::A,
        address: 0x30,
        change: Change::Write(0),
        mask: 0xffff,
        expected: 0,
    },
    Request {
        chip: Chip::A,
        address: 0x37,
        change: Change::Write(0x1d00),
        mask: 0xffff,
        expected: 0x1d00,
    },
    Request {
        chip: Chip::B,
        address: 0x33,
        change: Change::Keep(0xffe0),
        mask: 0x001f,
        expected: 0,
    },
    Request {
        chip: Chip::B,
        address: 0x36,
        change: Change::Write(0x007f),
        mask: 0xffff,
        expected: 0x007f,
    },
    Request {
        chip: Chip::B,
        address: 0x30,
        change: Change::Write(0),
        mask: 0xffff,
        expected: 0,
    },
    Request {
        chip: Chip::B,
        address: 0x37,
        change: Change::Write(0x1d00),
        mask: 0xffff,
        expected: 0x1d00,
    },
];

pub struct Shutdown<'a, A: RegisterIo, B: RegisterIo, C: ExternalOff> {
    a: &'a mut A,
    b: &'a mut B,
    control: &'a mut C,
    state: State,
}
impl<'a, A: RegisterIo, B: RegisterIo, C: ExternalOff> Shutdown<'a, A, B, C> {
    /// Inert, borrowed composition: rejected profiles do not drop owned buses.
    pub fn new(a: &'a mut A, b: &'a mut B, control: &'a mut C) -> Result<Self, ProfileMismatch> {
        if a.profile() != Profile::A || b.profile() != Profile::B {
            return Err(ProfileMismatch {
                primary: a.profile(),
                secondary: b.profile(),
            });
        }
        Ok(Self {
            a,
            b,
            control,
            state: State::New,
        })
    }
    pub const fn state(&self) -> State {
        self.state
    }
    fn fail(
        &mut self,
        cause: Cause<A::Error, B::Error, C::Error>,
    ) -> Failure<A::Error, B::Error, C::Error> {
        self.state = State::Failed;
        Failure {
            cause,
            external_cleanup: self.control.assert_off().err(),
        }
    }
    fn wrong_state(&mut self) -> Failure<A::Error, B::Error, C::Error> {
        if self.state == State::RegisterVerified {
            self.fail(Cause::State)
        } else {
            self.state = State::Failed;
            Failure {
                cause: Cause::State,
                external_cleanup: None,
            }
        }
    }
    fn guard(&mut self) -> Result<(), Cause<A::Error, B::Error, C::Error>> {
        self.control.check_off().map_err(Cause::External)
    }
    fn read(
        &mut self,
        chip: Chip,
        address: u8,
    ) -> Result<u16, Cause<A::Error, B::Error, C::Error>> {
        self.guard()?;
        let value = match chip {
            Chip::A => self.a.read(address).map_err(Cause::Primary),
            Chip::B => self.b.read(address).map_err(Cause::Secondary),
        }?;
        self.guard()?;
        Ok(value)
    }
    fn write(
        &mut self,
        chip: Chip,
        address: u8,
        word: u16,
    ) -> Result<(), Cause<A::Error, B::Error, C::Error>> {
        self.guard()?;
        match chip {
            Chip::A => self.a.write(address, word).map_err(Cause::Primary),
            Chip::B => self.b.write(address, word).map_err(Cause::Secondary),
        }?;
        self.guard()
    }
    fn check_word(&mut self, request: Request) -> Result<u16, Cause<A::Error, B::Error, C::Error>> {
        let observed = self.read(request.chip, request.address)?;
        if observed & request.mask != request.expected {
            return Err(Cause::Readback {
                chip: request.chip,
                address: request.address,
                mask: request.mask,
                expected: request.expected,
                observed,
            });
        }
        Ok(observed)
    }
    /// Execute once. No RF reset, RX start, TX enable, retry or NOR write.
    pub fn run_once(&mut self) -> Result<RegisterState, Failure<A::Error, B::Error, C::Error>> {
        if self.state != State::New {
            return Err(self.wrong_state());
        }
        self.state = State::Failed;
        let result = (|| {
            self.control.assert_off().map_err(Cause::External)?;
            let mut words = [0; 8];
            for (index, request) in REQUESTS.iter().copied().enumerate() {
                let word = match request.change {
                    Change::Write(word) => word,
                    Change::Keep(mask) => self.read(request.chip, request.address)? & mask,
                };
                self.write(request.chip, request.address, word)?;
                words[index] = self.check_word(request)?;
            }
            Ok(RegisterState { words })
        })();
        match result {
            Ok(readback) => {
                self.state = State::RegisterVerified;
                Ok(readback)
            }
            Err(cause) => Err(self.fail(cause)),
        }
    }
    /// Observational recheck while diagnostics run; no RF register writes.
    /// The first error poisons this instance and requests external LOW/blank.
    pub fn verify_off(&mut self) -> Result<RegisterState, Failure<A::Error, B::Error, C::Error>> {
        if self.state != State::RegisterVerified {
            return Err(self.wrong_state());
        }
        let result = (|| {
            let mut words = [0; 8];
            for (index, request) in REQUESTS.iter().copied().enumerate() {
                words[index] = self.check_word(request)?;
            }
            Ok(RegisterState { words })
        })();
        result.map_err(|cause| self.fail(cause))
    }
    /// Explicit terminal stop. External requests only, no additional RF writes.
    pub fn stop(&mut self) -> Result<(), C::Error> {
        if self.state == State::Failed {
            return Ok(());
        }
        self.state = State::Failed;
        self.control.assert_off()
    }
}
impl<A: RegisterIo, B: RegisterIo, C: Backlight> Shutdown<'_, A, B, C> {
    pub fn set_backlight(&mut self, on: bool) -> Result<(), Failure<A::Error, B::Error, C::Error>> {
        if self.state != State::RegisterVerified {
            return Err(self.wrong_state());
        }
        let result = (|| {
            self.guard()?;
            self.control.set_backlight(on).map_err(Cause::External)?;
            self.guard()
        })();
        result.map_err(|cause| self.fail(cause))
    }
}
impl<A: RegisterIo, B: RegisterIo, C: ExternalOff> Drop for Shutdown<'_, A, B, C> {
    fn drop(&mut self) {
        if self.state == State::RegisterVerified {
            self.state = State::Failed;
            let _ = self.control.assert_off();
        }
    }
}

#[cfg(test)]
mod tests;
