#![no_std]
#![cfg_attr(not(feature = "hardware"), forbid(unsafe_code))]
//! PC2/ADC1 channel12 only. No DMA, IRQ, battery conversion or RF side effects.
//! SDK calibration is volatile ADC self-calibration, not a calibration-NOR write.
//! Auxiliary codes preserve stock thresholds; physical PTT B mapping is unproved.

pub mod engine;
#[cfg(feature = "hardware")]
pub mod hardware;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidBudget,
    PinLocked,
    Configuration,
    Timing,
    ReadyTimeout,
    CalibrationInitTimeout,
    CalibrationTimeout,
    ConversionTimeout,
    Overflow,
    InvalidSample,
    Faulted,
}

/// Must wait a bounded interval (integration uses1us), without panic/reentry.
/// The call budget remains finite even if the external clock stalls.
pub trait PollTiming {
    fn wait_poll(&mut self) -> Result<(), Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Sample {
    pub raw: u16,
    pub key: Option<uv98_keys::Key>,
}

impl Sample {
    pub fn from_raw(raw: u16) -> Result<Self, Error> {
        if raw > 4095 {
            return Err(Error::InvalidSample);
        }
        Ok(Self {
            raw,
            key: uv98_keys::auxiliary_key(raw),
        })
    }
}
