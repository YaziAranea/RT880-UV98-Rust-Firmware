#![no_std]
#![forbid(unsafe_code)]
//! Volatile handheld UI state and input processing.

pub mod cb;
pub mod controller;
pub mod input;
pub mod text_entry;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Language {
    #[default]
    Russian = 0,
    English = 1,
    German = 2,
}

impl Language {
    pub const CURRENT: Self = if cfg!(feature = "locale-en") {
        Self::English
    } else if cfg!(feature = "locale-de") {
        Self::German
    } else {
        Self::Russian
    };
}
