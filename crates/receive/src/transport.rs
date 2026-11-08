//! Exclusive, borrowed A/B transport adapter. No pinmux, timing or RF startup.

use crate::{executor::Io, Chip};
use uv98_rf_bus::{Bus, Delay, Error, Pins, Profile};

/// Board-specific interlock and relay GPIO requests. This trait provides no
/// implementation. A caller must separately qualify physical wiring, pin
/// ownership (including GPIOF), timing, PTT and already-prepared receive state.
/// check_receive_preconditions is observational/interlock-only: it must not
/// change RF/relay registers or ownership and invalidate the fresh RMW word.
pub trait BoardControl {
    type Error;
    fn check_receive_preconditions(&mut self) -> Result<(), Self::Error>;
    fn set_antenna_pf0(&mut self, high: bool) -> Result<(), Self::Error>;
}

/// Optional external audio gate, separately implemented from RF bus traffic.
/// Must remain bounded and callable for cleanup after an interlock/bus failure.
pub trait AudioControl: BoardControl {
    fn set_audio_mute(&mut self, muted: bool) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileMismatch {
    pub primary: Profile,
    pub secondary: Profile,
}

/// Preserve the transport's cause AND both cleanup errors unchanged.
#[derive(Debug, Eq, PartialEq)]
pub enum PairError<A, B, C> {
    A(Error<A>),
    B(Error<B>),
    Board(C),
}

pub struct Pair<'a, AP: Pins, AD: Delay, BP: Pins, BD: Delay, C: BoardControl> {
    primary: &'a mut Bus<AP, AD>,
    secondary: &'a mut Bus<BP, BD>,
    control: &'a mut C,
}

impl<'a, AP: Pins, AD: Delay, BP: Pins, BD: Delay, C: BoardControl> Pair<'a, AP, AD, BP, BD, C> {
    /// Checks profile identity without a pin, delay, board-control or RF call.
    /// Borrowing also avoids dropping a previously used Bus on rejection.
    /// Exclusive references prevent foreground register access through another
    /// owner while the pair is retained. IRQ/raw access remains caller policy.
    pub fn new(
        primary: &'a mut Bus<AP, AD>,
        secondary: &'a mut Bus<BP, BD>,
        control: &'a mut C,
    ) -> Result<Self, ProfileMismatch> {
        if primary.profile() != Profile::A || secondary.profile() != Profile::B {
            return Err(ProfileMismatch {
                primary: primary.profile(),
                secondary: secondary.profile(),
            });
        }
        Ok(Self {
            primary,
            secondary,
            control,
        })
    }
}

impl<AP: Pins, AD: Delay, BP: Pins, BD: Delay, C: BoardControl> Io for Pair<'_, AP, AD, BP, BD, C> {
    type Error = PairError<AP::Error, BP::Error, C::Error>;

    fn check_receive_preconditions(&mut self) -> Result<(), Self::Error> {
        self.control
            .check_receive_preconditions()
            .map_err(PairError::Board)
    }
    fn read(&mut self, chip: Chip, register: u8) -> Result<u16, Self::Error> {
        match chip {
            Chip::A => self.primary.read(register).map_err(PairError::A),
            Chip::B => self.secondary.read(register).map_err(PairError::B),
        }
    }
    fn write(&mut self, chip: Chip, register: u8, value: u16) -> Result<(), Self::Error> {
        match chip {
            Chip::A => self.primary.write(register, value).map_err(PairError::A),
            Chip::B => self.secondary.write(register, value).map_err(PairError::B),
        }
    }
    fn set_antenna_pf0(&mut self, high: bool) -> Result<(), Self::Error> {
        self.control.set_antenna_pf0(high).map_err(PairError::Board)
    }
}

impl<AP: Pins, AD: Delay, BP: Pins, BD: Delay, C: AudioControl> crate::runtime::RuntimeIo
    for Pair<'_, AP, AD, BP, BD, C>
{
    fn set_audio_mute(&mut self, muted: bool) -> Result<(), Self::Error> {
        self.control.set_audio_mute(muted).map_err(PairError::Board)
    }
}
