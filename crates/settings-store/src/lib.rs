#![no_std]
#![cfg_attr(
    not(all(feature = "hardware", target_arch = "arm")),
    forbid(unsafe_code)
)]
//! Two-page journal for explicitly saved station name, Roger setting and microphone gain.
//! It uses reserved internal flash. External NOR is outside its address space.
//! A full firmware update clears the journal.

#[cfg(all(feature = "hardware", target_arch = "arm"))]
pub mod hardware;
#[cfg(any(test, all(feature = "hardware", target_arch = "arm")))]
mod journal;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Settings {
    /// Exactly the stock name field width. The caller selects encoding/padding.
    pub name: [u8; 16],
    pub roger_enabled: bool,
    /// None preserves a legacy record's factory microphone setting.
    pub microphone_gain: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Layout,
    Clock,
    Protection,
    Controller,
    Operation,
    Verify,
    Conflict,
    InvalidSettings,
}
