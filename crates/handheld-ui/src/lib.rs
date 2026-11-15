#![no_std]
#![forbid(unsafe_code)]
//! User-facing UI state/input/rendering without radio or storage mutations.
pub mod controller;
pub mod input;
pub mod meter;
pub mod render;
