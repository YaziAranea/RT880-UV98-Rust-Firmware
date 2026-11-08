//! Lossless RT880 CPS 1.10 broadcast/shortwave receiver records.
//!
//! There are 128 records of 32 bytes. All APIs expose stored codes; frequency
//! scaling depends on the range code and no physical-unit conversion is claimed.
//! Bytes +10..+16 are opaque. Names are already-encoded GBK, not UTF-8.
//! No parser substitutes defaults, rejects unknown field values, or clears a record.

use crate::FM_LEN;

pub const CHANNEL_COUNT: usize = 128;
pub const RECORD_LEN: usize = 32;
pub const NAME_LEN: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    Range,
    Demodulation,
    Step,
    Bandwidth,
    Attenuation,
}

impl Field {
    pub const fn offset(self) -> usize {
        match self {
            Self::Range => 0,
            Self::Demodulation => 1,
            Self::Step => 2,
            Self::Bandwidth => 3,
            Self::Attenuation => 4,
        }
    }

    /// Codes actually emitted by SaveSingleFM. Its search loops cover fewer
    /// choices than the Step and ATT controls display (7 vs 8, and 6 vs 39).
    /// Reading preserves all codes; this bounded setter does not resolve that CPS bug.
    pub const fn max_writable_code(self) -> u8 {
        match self {
            Self::Range => 3,
            Self::Demodulation => 4,
            Self::Step => 6,
            Self::Bandwidth | Self::Attenuation => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    WrongLength { actual: usize },
    InvalidCode { field: Field, value: u8 },
    BfoOutsideCpsRange { value: i16 },
    NameTooLong { actual: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct FmChannels<'a> {
    raw: &'a [u8; FM_LEN],
}

impl<'a> FmChannels<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let raw = bytes.try_into().map_err(|_| Error::WrongLength {
            actual: bytes.len(),
        })?;
        Ok(Self { raw })
    }

    pub fn raw(&self) -> &'a [u8; FM_LEN] {
        self.raw
    }

    pub fn channel(&self, index: usize) -> Option<FmChannel<'a>> {
        if index >= CHANNEL_COUNT {
            return None;
        }
        let start = index * RECORD_LEN;
        Some(FmChannel::from_bytes(
            self.raw[start..start + RECORD_LEN].try_into().ok()?,
        ))
    }
}

#[derive(Debug)]
pub struct FmChannelsMut<'a> {
    raw: &'a mut [u8; FM_LEN],
}

impl<'a> FmChannelsMut<'a> {
    pub fn parse(bytes: &'a mut [u8]) -> Result<Self, Error> {
        let actual = bytes.len();
        let raw = bytes
            .try_into()
            .map_err(|_| Error::WrongLength { actual })?;
        Ok(Self { raw })
    }

    pub fn view(&self) -> FmChannels<'_> {
        FmChannels { raw: self.raw }
    }

    pub fn channel(&mut self, index: usize) -> Option<FmChannelMut<'_>> {
        if index >= CHANNEL_COUNT {
            return None;
        }
        let start = index * RECORD_LEN;
        Some(FmChannelMut::from_bytes(
            (&mut self.raw[start..start + RECORD_LEN]).try_into().ok()?,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FmChannel<'a> {
    raw: &'a [u8; RECORD_LEN],
}

impl<'a> FmChannel<'a> {
    pub fn from_bytes(raw: &'a [u8; RECORD_LEN]) -> Self {
        Self { raw }
    }

    pub fn raw(&self) -> &'a [u8; RECORD_LEN] {
        self.raw
    }

    pub fn code(&self, field: Field) -> u8 {
        self.raw[field.offset()]
    }

    /// CPS displays a record only when byte +9 is exactly 1.
    pub fn is_present(&self) -> bool {
        self.raw[9] == 1
    }

    pub fn presence_code(&self) -> u8 {
        self.raw[9]
    }

    pub fn frequency_code(&self) -> u16 {
        u16::from_le_bytes([self.raw[7], self.raw[8]])
    }

    pub fn bfo_code(&self) -> u16 {
        u16::from_le_bytes([self.raw[5], self.raw[6]])
    }

    /// The CPS display arithmetic is LE16 - 32768; units are not inferred.
    /// Raw zero decodes to -32768, even though the UI minimum is -32767.
    pub fn bfo_value(&self) -> i16 {
        (i32::from(self.bfo_code()) - 32768) as i16
    }

    pub fn name_raw(&self) -> &'a [u8; NAME_LEN] {
        self.raw[16..32].try_into().expect("fixed name length")
    }
}

#[derive(Debug)]
pub struct FmChannelMut<'a> {
    raw: &'a mut [u8; RECORD_LEN],
}

impl<'a> FmChannelMut<'a> {
    pub fn from_bytes(raw: &'a mut [u8; RECORD_LEN]) -> Self {
        Self { raw }
    }

    pub fn view(&self) -> FmChannel<'_> {
        FmChannel { raw: self.raw }
    }

    pub fn set_code(&mut self, field: Field, value: u8) -> Result<(), Error> {
        if value > field.max_writable_code() {
            return Err(Error::InvalidCode { field, value });
        }
        self.raw[field.offset()] = value;
        Ok(())
    }

    /// Patches a stored LE16 value; does not validate receiver range or units.
    pub fn set_frequency_code(&mut self, value: u16) {
        self.raw[7..9].copy_from_slice(&value.to_le_bytes());
    }

    /// Patches the raw stored code, including codes outside the CPS UI range.
    pub fn set_bfo_code(&mut self, value: u16) {
        self.raw[5..7].copy_from_slice(&value.to_le_bytes());
    }

    /// Uses the examined UI range and its offset-binary storage arithmetic.
    pub fn set_bfo_value(&mut self, value: i16) -> Result<(), Error> {
        if value == i16::MIN {
            return Err(Error::BfoOutsideCpsRange { value });
        }
        self.set_bfo_code((i32::from(value) + 32768) as u16);
        Ok(())
    }

    /// Sets only the known active marker. It does not initialize other fields.
    pub fn mark_present(&mut self) {
        self.raw[9] = 1;
    }

    pub fn set_name_raw(&mut self, value: &[u8; NAME_LEN]) {
        self.raw[16..32].copy_from_slice(value);
    }

    /// Already-encoded bytes; 0xFF padding, no truncation or conversion.
    pub fn set_name_encoded(&mut self, value: &[u8]) -> Result<(), Error> {
        if value.len() > NAME_LEN {
            return Err(Error::NameTooLong {
                actual: value.len(),
            });
        }
        self.raw[16..32].fill(0xFF);
        self.raw[16..16 + value.len()].copy_from_slice(value);
        Ok(())
    }
}
