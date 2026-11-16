#![no_std]
#![cfg_attr(not(feature = "hardware"), forbid(unsafe_code))]
//! Encoders do not authorize RF or extend a transmission timeout.
//! The caller owns TX policy, calibration, cancellation and radio routing.

pub mod aprs;
pub mod cmx868;
pub mod mdc1200;
pub mod tones;

pub use tones::{dtmf, BkToneSession, FiveTone, Standard, Tone, ToneRegisters};
