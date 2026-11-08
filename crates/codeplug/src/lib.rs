#![no_std]
#![forbid(unsafe_code)]
//! Borrowed views and explicit field patches for RT880 CPS 1.10 `.880dmf` files.
//! The same section layout was also confirmed in UV98 Plus CPS 1.07 `.98pdmf`
//! open/save handlers. These are specific examined versions, not all CPS files.
//!
//! The file concatenates CFG, Channels, FM, Time and Zones in that order.
//! Parsing checks **length only**. It does not validate field meanings, checksums,
//! model compatibility, calibration, or whether bytes came from a radio.
//! In particular, the presence of the Time section does not prove it was read
//! from the device: the examined standard CPS read does not acquire that section.
//! This format carries no established acquisition provenance.
//!
//! Views preserve every input byte; mutable helpers change only explicit fields.
//! No default substitution, allocation, file/serial access or device operations.
//! This RT880 format is not established as the format of every UV98 CPS version.

use core::fmt;

pub mod channels;
pub mod fm;
pub mod settings;
pub mod zones;

pub const CFG_LEN: usize = 4096;
pub const CHANNELS_LEN: usize = 49_152;
pub const FM_LEN: usize = 4096;
pub const TIME_LEN: usize = 32_768;
pub const ZONES_LEN: usize = 131_072;

pub const CFG_OFFSET: usize = 0;
pub const CHANNELS_OFFSET: usize = CFG_OFFSET + CFG_LEN;
pub const FM_OFFSET: usize = CHANNELS_OFFSET + CHANNELS_LEN;
pub const TIME_OFFSET: usize = FM_OFFSET + FM_LEN;
pub const ZONES_OFFSET: usize = TIME_OFFSET + TIME_LEN;
pub const FILE_LEN: usize = ZONES_OFFSET + ZONES_LEN;

/// Section names follow the examined CPS buffers, not decoded field semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Section {
    Cfg,
    Channels,
    Fm,
    Time,
    Zones,
}

pub const SECTIONS: [Section; 5] = [
    Section::Cfg,
    Section::Channels,
    Section::Fm,
    Section::Time,
    Section::Zones,
];

impl Section {
    /// Byte offset in the saved file. This is not a radio memory address.
    pub const fn offset(self) -> usize {
        match self {
            Self::Cfg => CFG_OFFSET,
            Self::Channels => CHANNELS_OFFSET,
            Self::Fm => FM_OFFSET,
            Self::Time => TIME_OFFSET,
            Self::Zones => ZONES_OFFSET,
        }
    }

    pub const fn byte_len(self) -> usize {
        match self {
            Self::Cfg => CFG_LEN,
            Self::Channels => CHANNELS_LEN,
            Self::Fm => FM_LEN,
            Self::Time => TIME_LEN,
            Self::Zones => ZONES_LEN,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    WrongLength { actual: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WrongLength { actual } => {
                write!(formatter, "expected {FILE_LEN} bytes, got {actual}")
            }
        }
    }
}

/// A view of an exact-length file, not a claim that its contents are valid.
#[derive(Clone, Copy, Debug)]
pub struct Codeplug<'a> {
    raw: &'a [u8; FILE_LEN],
}

impl<'a> Codeplug<'a> {
    /// Accept any contents of the required length without normalization.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let raw = bytes.try_into().map_err(|_| Error::WrongLength {
            actual: bytes.len(),
        })?;
        Ok(Self { raw })
    }

    pub fn raw(&self) -> &'a [u8; FILE_LEN] {
        self.raw
    }

    pub fn section(&self, section: Section) -> &'a [u8] {
        let start = section.offset();
        &self.raw[start..start + section.byte_len()]
    }

    pub fn cfg(&self) -> &'a [u8; CFG_LEN] {
        self.fixed_section(Section::Cfg)
    }

    pub fn channels(&self) -> &'a [u8; CHANNELS_LEN] {
        self.fixed_section(Section::Channels)
    }

    pub fn fm(&self) -> &'a [u8; FM_LEN] {
        self.fixed_section(Section::Fm)
    }

    /// File bytes only. This accessor cannot establish device-read provenance.
    pub fn time(&self) -> &'a [u8; TIME_LEN] {
        self.fixed_section(Section::Time)
    }

    pub fn zones(&self) -> &'a [u8; ZONES_LEN] {
        self.fixed_section(Section::Zones)
    }

    fn fixed_section<const N: usize>(&self, section: Section) -> &'a [u8; N] {
        self.section(section)
            .try_into()
            .expect("internal fixed section length")
    }
}
