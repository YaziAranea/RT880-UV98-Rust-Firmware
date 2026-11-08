#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Experimental related-RT880 RF GPIO transport; hardware is explicitly opt-in.
//! Default build contains only a register engine usable with host mocks.
//! Constructors neither establish RF/PA inactivity nor initialize the RF chip.
//! No RF delay implementation, TX-gate write, PLL, startup, or measured timing.
//! Atomic SCR/CLR writes change only owned output latches. GPIO configuration
//! RMW requires serialization even against other owners of disjoint port pins.

pub mod engine;
#[cfg(feature = "hardware")]
pub mod hardware;

/// Required external observational interlock. No permissive default is given.
/// It must inspect qualified board/PTT/prepared-RX conditions in bounded time,
/// without changing RF/relay registers or ownership. It must not panic.
/// PTT must be a fresh physical observation through its existing owner; a
/// stale GUI/key latch is insufficient. This crate does not duplicate PB6.
pub trait Interlock {
    type Error;
    fn check_receive_preconditions(&mut self) -> Result<(), Self::Error>;
}

/// Raw latch levels, not a claim about a relay contact or audio voltage.
/// Hardware construction requires these levels to match the separately
/// qualified initial state; initialization must not unexpectedly move a relay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlLevels {
    pub antenna_pf0_high: bool,
    pub af_mute_pf8_high: bool,
}
