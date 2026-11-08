#![no_std]
#![forbid(unsafe_code)]
//! Pure, offline calibration computations reverse-engineered from one related
//! RT880-labelled 1.16 image. They are not established as the exact UV98 firmware.
//!
//! No serial or device I/O, Flash writes, fallback defaults, or radio activation.
//! Outputs are stored/register codes, not measured physical RF quantities.
//! See CALIBRATION-PORT-RU.txt for evidence, test scope and unresolved behavior.

pub const REFERENCE_IMAGE_SHA256: &str =
    "eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc";

pub mod frequency;
pub mod general;
pub mod rf;
