//! Lossless views and explicit field patches for the examined CPS 48-byte channels.
//!
//! Offsets follow RT880 CPS 1.10 `SaveSingle` / `SetChInfo`. A channel section
//! contains 1024 records. Parsing preserves unknown values; it neither activates
//! records nor normalizes them. Patching changes only the requested fields.
//! This module has no file or radio I/O and makes no hardware validity claim.

use core::fmt;

use crate::CHANNELS_LEN;

pub const CHANNEL_LEN: usize = 48;
pub const CHANNEL_COUNT: usize = 1024;
pub const NAME_LEN: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelError {
    WrongSectionLength { actual: usize },
    WrongRecordLength { actual: usize },
    IndexOutOfRange { index: usize },
    NameTooLong { actual: usize },
    CtcssOutOfRange { tenths_hz: u16 },
    DcsOutOfRange { code: u16 },
}

impl fmt::Display for ChannelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WrongSectionLength { actual } => {
                write!(
                    formatter,
                    "expected {CHANNELS_LEN} channel bytes, got {actual}"
                )
            }
            Self::WrongRecordLength { actual } => {
                write!(
                    formatter,
                    "expected {CHANNEL_LEN} record bytes, got {actual}"
                )
            }
            Self::IndexOutOfRange { index } => {
                write!(
                    formatter,
                    "channel index {index} is outside 0..{CHANNEL_COUNT}"
                )
            }
            Self::NameTooLong { actual } => {
                write!(formatter, "name has {actual} bytes; maximum is {NAME_LEN}")
            }
            Self::CtcssOutOfRange { tenths_hz } => {
                write!(formatter, "CTCSS code {tenths_hz} exceeds the 12-bit field")
            }
            Self::DcsOutOfRange { code } => {
                write!(formatter, "DCS code {code} exceeds three octal digits")
            }
        }
    }
}

/// CPS byte 1, independent of the stored TX frequency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationMode {
    ReceiveTransmit,
    ReceiveOnly,
    TransmitOnly,
}

impl OperationMode {
    pub const fn decode(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::ReceiveTransmit),
            1 => Some(Self::ReceiveOnly),
            2 => Some(Self::TransmitOnly),
            _ => None,
        }
    }

    pub const fn encode(self) -> u8 {
        match self {
            Self::ReceiveTransmit => 0,
            Self::ReceiveOnly => 1,
            Self::TransmitOnly => 2,
        }
    }
}

/// CPS byte 3. Labels are preserved without assuming a filter implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bandwidth {
    Wide,
    Narrow,
}

impl Bandwidth {
    pub const fn decode(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Wide),
            1 => Some(Self::Narrow),
            _ => None,
        }
    }

    pub const fn encode(self) -> u8 {
        match self {
            Self::Wide => 0,
            Self::Narrow => 1,
        }
    }
}

/// CPS byte 16. These labels do not specify output power in watts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Power {
    Low,
    Medium,
    High,
}

impl Power {
    pub const fn decode(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Low),
            1 => Some(Self::Medium),
            2 => Some(Self::High),
            _ => None,
        }
    }

    pub const fn encode(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }
}

/// Canonical CPS tone encodings. Unknown raw words remain accessible separately.
///
/// CTCSS payload is frequency in tenths of a hertz (88.5 -> 885). The 12-bit
/// limit is a storage constraint, not a list of radio-supported tones. DCS code
/// is the numeric value of three octal digits (D023 -> 0o023 -> decimal 19).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tone {
    None,
    Ctcss { tenths_hz: u16 },
    Dcs { code: u16, inverted: bool },
}

impl Tone {
    /// Decode canonical words only; do not normalize malformed/unknown words.
    pub const fn decode(raw: u16) -> Option<Self> {
        let payload = raw & 0x0fff;
        match raw {
            0 => Some(Self::None),
            0x1000..=0x1fff => Some(Self::Ctcss { tenths_hz: payload }),
            0x2000..=0x21ff => Some(Self::Dcs {
                code: payload,
                inverted: false,
            }),
            0x3000..=0x31ff => Some(Self::Dcs {
                code: payload,
                inverted: true,
            }),
            _ => None,
        }
    }

    /// Reject overflow rather than silently masking the caller's value.
    pub const fn encode(self) -> Result<u16, ChannelError> {
        match self {
            Self::None => Ok(0),
            Self::Ctcss { tenths_hz } if tenths_hz <= 0x0fff => Ok(0x1000 | tenths_hz),
            Self::Ctcss { tenths_hz } => Err(ChannelError::CtcssOutOfRange { tenths_hz }),
            Self::Dcs { code, inverted } if code <= 0x01ff => {
                Ok((if inverted { 0x3000 } else { 0x2000 }) | code)
            }
            Self::Dcs { code, .. } => Err(ChannelError::DcsOutOfRange { code }),
        }
    }
}

/// Borrowed record, including inactive records and unrecognized field values.
#[derive(Clone, Copy, Debug)]
pub struct Channel<'a> {
    raw: &'a [u8; CHANNEL_LEN],
}

impl<'a> Channel<'a> {
    pub fn parse(record: &'a [u8]) -> Result<Self, ChannelError> {
        let raw = record
            .try_into()
            .map_err(|_| ChannelError::WrongRecordLength {
                actual: record.len(),
            })?;
        Ok(Self { raw })
    }

    pub fn raw(&self) -> &'a [u8; CHANNEL_LEN] {
        self.raw
    }

    pub fn active_marker(&self) -> u8 {
        self.raw[2]
    }

    /// Exact CPS presence check. It does not validate the other fields.
    pub fn is_active(&self) -> bool {
        self.active_marker() == 1
    }

    pub fn operation_mode_raw(&self) -> u8 {
        self.raw[1]
    }
    pub fn operation_mode(&self) -> Option<OperationMode> {
        OperationMode::decode(self.operation_mode_raw())
    }

    /// Unsigned stored code. CPS displays ordinary values as code / 100000 MHz
    /// (one code unit = 10 Hz). No frequency-range validation is implied.
    pub fn rx_frequency_code(&self) -> u32 {
        self.word(6)
    }
    pub fn tx_frequency_code(&self) -> u32 {
        self.word(10)
    }
    pub fn rx_tone_raw(&self) -> u16 {
        self.halfword(4)
    }
    pub fn tx_tone_raw(&self) -> u16 {
        self.halfword(14)
    }
    pub fn rx_tone(&self) -> Option<Tone> {
        Tone::decode(self.rx_tone_raw())
    }
    pub fn tx_tone(&self) -> Option<Tone> {
        Tone::decode(self.tx_tone_raw())
    }
    pub fn bandwidth_raw(&self) -> u8 {
        self.raw[3]
    }
    pub fn bandwidth(&self) -> Option<Bandwidth> {
        Bandwidth::decode(self.bandwidth_raw())
    }
    pub fn power_raw(&self) -> u8 {
        self.raw[16]
    }
    pub fn power(&self) -> Option<Power> {
        Power::decode(self.power_raw())
    }

    /// Byte 19 bit 7: clear means included in scan, set means excluded.
    pub fn scan_add(&self) -> bool {
        self.raw[19] & 0x80 == 0
    }

    /// Raw GBK field, including padding and any unknown bytes.
    pub fn name_raw(&self) -> &'a [u8; NAME_LEN] {
        self.raw[32..48].try_into().expect("fixed name field")
    }

    /// Bytes before the first FF padding byte. NULs are retained, and GBK is not
    /// decoded here. The original CPS additionally strips NULs for display.
    pub fn name_bytes(&self) -> &'a [u8] {
        let raw = self.name_raw();
        let end = raw
            .iter()
            .position(|&byte| byte == 0xff)
            .unwrap_or(NAME_LEN);
        &raw[..end]
    }

    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.raw[offset..offset + 4]
                .try_into()
                .expect("fixed u32 field"),
        )
    }

    fn halfword(&self, offset: usize) -> u16 {
        u16::from_le_bytes([self.raw[offset], self.raw[offset + 1]])
    }
}

/// Parse one zero-based record from the exact-length Channels section.
pub fn channel(section: &[u8], index: usize) -> Result<Channel<'_>, ChannelError> {
    let start = record_start(section.len(), index)?;
    Channel::parse(&section[start..start + CHANNEL_LEN])
}

/// Requested changes only. `None` preserves the exact original field.
///
/// No patch activates, deletes, or reinitializes a record. Width, power, mode,
/// tone, frequency and name replace their entire explicitly selected fields.
/// Scan edits only bit 7; all lower bits of byte 19 remain unchanged.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChannelPatch {
    pub rx_frequency_code: Option<u32>,
    pub tx_frequency_code: Option<u32>,
    pub name_raw: Option<[u8; NAME_LEN]>,
    pub operation_mode: Option<OperationMode>,
    pub bandwidth: Option<Bandwidth>,
    pub power: Option<Power>,
    pub scan_add: Option<bool>,
    pub rx_tone: Option<Tone>,
    pub tx_tone: Option<Tone>,
}

/// Build FF-padded name bytes without truncation. The host must encode/validate
/// GBK before calling this helper; no character encoding is guessed here.
pub fn padded_name(encoded: &[u8]) -> Result<[u8; NAME_LEN], ChannelError> {
    if encoded.len() > NAME_LEN {
        return Err(ChannelError::NameTooLong {
            actual: encoded.len(),
        });
    }
    let mut result = [0xff; NAME_LEN];
    result[..encoded.len()].copy_from_slice(encoded);
    Ok(result)
}

/// Apply a patch to one record. Validation completes before any byte changes.
pub fn patch_record(record: &mut [u8], patch: &ChannelPatch) -> Result<(), ChannelError> {
    if record.len() != CHANNEL_LEN {
        return Err(ChannelError::WrongRecordLength {
            actual: record.len(),
        });
    }
    let rx_tone = patch.rx_tone.map(Tone::encode).transpose()?;
    let tx_tone = patch.tx_tone.map(Tone::encode).transpose()?;

    if let Some(value) = patch.rx_frequency_code {
        record[6..10].copy_from_slice(&value.to_le_bytes());
    }
    if let Some(value) = patch.tx_frequency_code {
        record[10..14].copy_from_slice(&value.to_le_bytes());
    }
    if let Some(value) = patch.name_raw {
        record[32..48].copy_from_slice(&value);
    }
    if let Some(value) = patch.operation_mode {
        record[1] = value.encode();
    }
    if let Some(value) = patch.bandwidth {
        record[3] = value.encode();
    }
    if let Some(value) = patch.power {
        record[16] = value.encode();
    }
    if let Some(value) = patch.scan_add {
        record[19] = (record[19] & 0x7f) | if value { 0 } else { 0x80 };
    }
    if let Some(value) = rx_tone {
        record[4..6].copy_from_slice(&value.to_le_bytes());
    }
    if let Some(value) = tx_tone {
        record[14..16].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

/// Patch one zero-based record in an exact-length Channels section.
pub fn patch_channel(
    section: &mut [u8],
    index: usize,
    patch: &ChannelPatch,
) -> Result<(), ChannelError> {
    let start = record_start(section.len(), index)?;
    patch_record(&mut section[start..start + CHANNEL_LEN], patch)
}

fn record_start(section_len: usize, index: usize) -> Result<usize, ChannelError> {
    if section_len != CHANNELS_LEN {
        return Err(ChannelError::WrongSectionLength {
            actual: section_len,
        });
    }
    // Check before multiplication, including when index is usize::MAX.
    if index >= CHANNEL_COUNT {
        return Err(ChannelError::IndexOutOfRange { index });
    }
    Ok(index * CHANNEL_LEN)
}
