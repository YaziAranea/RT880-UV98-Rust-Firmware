//! Lossless views and field patches for RT880 CPS 1.10 zone records.
//!
//! The examined CPS uses 256 records of 512 bytes, with 200 little-endian
//! channel indices per record. Bytes +2..+4 and +420..+512 remain opaque.
//! Names are 16 encoded GBK bytes, not UTF-8. No encoding conversion occurs.
//! This module does not remove dangling references or compact membership.

use crate::ZONES_LEN;

pub const ZONE_COUNT: usize = 256;
pub const RECORD_LEN: usize = 512;
pub const MEMBER_COUNT: usize = 200;
pub const EMPTY_MEMBER: u16 = 0xFFFF;
pub const NAME_LEN: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    WrongLength { actual: usize },
    MemberOutOfRange { slot: usize },
    InvalidMemberCode { value: u16 },
    NameTooLong { actual: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct Zones<'a> {
    raw: &'a [u8; ZONES_LEN],
}

impl<'a> Zones<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let raw = bytes.try_into().map_err(|_| Error::WrongLength {
            actual: bytes.len(),
        })?;
        Ok(Self { raw })
    }

    pub fn raw(&self) -> &'a [u8; ZONES_LEN] {
        self.raw
    }

    pub fn zone(&self, index: usize) -> Option<Zone<'a>> {
        if index >= ZONE_COUNT {
            return None;
        }
        let start = index * RECORD_LEN;
        Some(Zone::from_bytes(
            self.raw[start..start + RECORD_LEN].try_into().ok()?,
        ))
    }
}

#[derive(Debug)]
pub struct ZonesMut<'a> {
    raw: &'a mut [u8; ZONES_LEN],
}

impl<'a> ZonesMut<'a> {
    pub fn parse(bytes: &'a mut [u8]) -> Result<Self, Error> {
        let actual = bytes.len();
        let raw = bytes
            .try_into()
            .map_err(|_| Error::WrongLength { actual })?;
        Ok(Self { raw })
    }

    pub fn view(&self) -> Zones<'_> {
        Zones { raw: self.raw }
    }

    pub fn zone(&mut self, index: usize) -> Option<ZoneMut<'_>> {
        if index >= ZONE_COUNT {
            return None;
        }
        let start = index * RECORD_LEN;
        Some(ZoneMut::from_bytes(
            (&mut self.raw[start..start + RECORD_LEN]).try_into().ok()?,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Zone<'a> {
    raw: &'a [u8; RECORD_LEN],
}

impl<'a> Zone<'a> {
    pub fn from_bytes(raw: &'a [u8; RECORD_LEN]) -> Self {
        Self { raw }
    }

    pub fn raw(&self) -> &'a [u8; RECORD_LEN] {
        self.raw
    }

    pub fn name_raw(&self) -> &'a [u8; NAME_LEN] {
        self.raw[4..20].try_into().expect("fixed name length")
    }

    /// The CPS writes the same count to both bytes. These are NOT decoded as
    /// a conventional little-endian count, and reading never repairs them.
    pub fn cps_count_bytes(&self) -> [u8; 2] {
        [self.raw[0], self.raw[1]]
    }

    /// Returns every stored code unchanged; None means an invalid slot index.
    /// Codes 0..=1023 are zero-based channel indices; 0xFFFF is the CPS sentinel.
    pub fn member_code(&self, slot: usize) -> Option<u16> {
        if slot >= MEMBER_COUNT {
            return None;
        }
        let offset = 20 + slot * 2;
        Some(u16::from_le_bytes([self.raw[offset], self.raw[offset + 1]]))
    }

    /// Counts stored indices below 1024, without checking whether channels exist.
    pub fn referenced_channel_count(&self) -> u8 {
        let mut count = 0;
        for slot in 0..MEMBER_COUNT {
            if self.member_code(slot).is_some_and(|code| code < 1024) {
                count += 1;
            }
        }
        count
    }
}

#[derive(Debug)]
pub struct ZoneMut<'a> {
    raw: &'a mut [u8; RECORD_LEN],
}

impl<'a> ZoneMut<'a> {
    pub fn from_bytes(raw: &'a mut [u8; RECORD_LEN]) -> Self {
        Self { raw }
    }

    pub fn view(&self) -> Zone<'_> {
        Zone { raw: self.raw }
    }

    /// Patches precisely the two member bytes. Header count bytes are unchanged;
    /// call refresh_cps_count explicitly if the examined CPS convention is wanted.
    pub fn set_member_code(&mut self, slot: usize, value: u16) -> Result<(), Error> {
        if slot >= MEMBER_COUNT {
            return Err(Error::MemberOutOfRange { slot });
        }
        if value >= 1024 && value != EMPTY_MEMBER {
            return Err(Error::InvalidMemberCode { value });
        }
        let offset = 20 + slot * 2;
        self.raw[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    /// Mirrors only the CPS's two identical count-byte writes. No membership
    /// entries, unknown codes, ordering, or unrelated header bytes are changed.
    pub fn refresh_cps_count(&mut self) -> u8 {
        let count = self.view().referenced_channel_count();
        self.raw[0] = count;
        self.raw[1] = count;
        count
    }

    /// Replaces exactly 16 bytes, preserving encoding, embedded NULs and padding.
    pub fn set_name_raw(&mut self, value: &[u8; NAME_LEN]) {
        self.raw[4..20].copy_from_slice(value);
    }

    /// Copies already-encoded bytes and pads the remaining name with 0xFF.
    /// Rejects oversize names rather than splitting a multibyte character.
    pub fn set_name_encoded(&mut self, value: &[u8]) -> Result<(), Error> {
        if value.len() > NAME_LEN {
            return Err(Error::NameTooLong {
                actual: value.len(),
            });
        }
        self.raw[4..20].fill(0xFF);
        self.raw[4..4 + value.len()].copy_from_slice(value);
        Ok(())
    }
}
