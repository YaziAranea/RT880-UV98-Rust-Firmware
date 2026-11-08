#![no_std]
#![forbid(unsafe_code)]
//! Read-only views of a **saved** tuning block. No allocator, serial access,
//! hardware access, writers, interpolation, or default substitution.
//!
//! The layout comes from `ReadDataToUI` and `FillBand` in Radio Tuning Software
//! 241219 and has been checked against one UV98 Plus stock 1.16 backup. This is
//! not a verified RF calibration algorithm or a promise of revision compatibility.
//! Values exposed here are stored codes, **not Hz, watts, or measured volts**.
//!
//! Parsing validates length and the observed `0x9A` marker only. No checksum is
//! known for this block: arbitrary corruption after the marker is not detected.
//! Keep and independently verify the original backup and its SHA-256 manifest.
//! Every input byte, including unknown bits, unused table points, and the tail,
//! remains available through borrowed raw accessors.

use core::fmt;

pub const BLOCK_LEN: usize = 4096;
pub const MARKER: u8 = 0x9A;
pub const GENERAL_LEN: usize = 32;
pub const BAND_COUNT: usize = 15;
pub const BAND_LEN: usize = 112;
pub const TABLE_LEN: usize = 16;
pub const KNOWN_LAYOUT_END: usize = GENERAL_LEN + BAND_COUNT * BAND_LEN;
pub const TAIL_LEN: usize = BLOCK_LEN - KNOWN_LAYOUT_END;

const VALID_POINTS: [usize; BAND_COUNT] =
    [8, 16, 10, 16, 15, 16, 16, 16, 16, 16, 16, 16, 16, 16, 12];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    WrongLength { actual: usize },
    InvalidMarker { found: u8 },
    BandOutOfRange { index: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WrongLength { actual } => write!(f, "expected {BLOCK_LEN} bytes, got {actual}"),
            Self::InvalidMarker { found } => {
                write!(f, "expected marker 0x{MARKER:02X}, got 0x{found:02X}")
            }
            Self::BandOutOfRange { index } => {
                write!(f, "band index {index} is outside 0..{BAND_COUNT}")
            }
        }
    }
}

/// The four filter fields, named as in the tuning tool, without physical units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Filter {
    Tx300,
    Tx3k,
    Rx300,
    Rx3k,
}

/// Stored table rows. Power rows contain control codes, not measured watts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Table {
    PowerHigh,
    PowerLow,
    NoiseWide,
    SquelchWide,
    NoiseNarrow,
    SquelchNarrow,
}

/// An immutable borrowed view of exactly 4096 bytes.
#[derive(Clone, Copy, Debug)]
pub struct Calibration<'a> {
    raw: &'a [u8; BLOCK_LEN],
}

impl<'a> Calibration<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let raw: &'a [u8; BLOCK_LEN] = data
            .try_into()
            .map_err(|_| Error::WrongLength { actual: data.len() })?;
        if raw[0] != MARKER {
            return Err(Error::InvalidMarker { found: raw[0] });
        }
        Ok(Self { raw })
    }

    /// The original bytes, without normalization or copies.
    pub fn raw(&self) -> &'a [u8; BLOCK_LEN] {
        self.raw
    }

    pub fn first_byte(&self) -> u8 {
        self.raw[0]
    }

    pub fn general(&self) -> General<'a> {
        General {
            raw: fixed_slice(self.raw, 0),
        }
    }

    /// `index` is zero-based. Checked before arithmetic, including `usize::MAX`.
    pub fn band(&self, index: usize) -> Result<Band<'a>, Error> {
        if index >= BAND_COUNT {
            return Err(Error::BandOutOfRange { index });
        }
        Ok(Band {
            raw: fixed_slice(self.raw, GENERAL_LEN + index * BAND_LEN),
            index,
        })
    }

    /// Bytes 1712..4096 have no interpretation in this parser.
    pub fn uninterpreted_tail(&self) -> &'a [u8; TAIL_LEN] {
        fixed_slice(self.raw, KNOWN_LAYOUT_END)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct General<'a> {
    raw: &'a [u8; GENERAL_LEN],
}

impl<'a> General<'a> {
    pub fn raw(&self) -> &'a [u8; GENERAL_LEN] {
        self.raw
    }
    /// Test-mode flag. The source control is named ComboBoxScreen, but its
    /// choices are "Stop Test" / "Start Test", not display hardware variants.
    pub fn test_mode_raw(&self) -> u8 {
        self.raw[1]
    }
    pub fn test_mode_ui_index(&self) -> u8 {
        u8::from(self.raw[1] != 0)
    }
    #[deprecated(note = "Source control name was misleading; use test_mode_raw")]
    pub fn screen_raw(&self) -> u8 {
        self.test_mode_raw()
    }
    #[deprecated(note = "Source control name was misleading; use test_mode_ui_index")]
    pub fn screen_ui_index(&self) -> u8 {
        self.test_mode_ui_index()
    }
    pub fn reg_3e_raw(&self) -> u16 {
        u16le(self.raw, 2)
    }
    pub fn reg_3e_ui_index(&self) -> u8 {
        u8::from(self.reg_3e_raw() != 0xAAAA)
    }
    pub fn expander(&self) -> Compander {
        Compander {
            raw: u16le(self.raw, 4),
        }
    }
    pub fn compressor(&self) -> Compander {
        Compander {
            raw: u16le(self.raw, 6),
        }
    }
    pub fn filter_raw(&self, filter: Filter) -> u8 {
        self.raw[8 + filter as usize]
    }
    pub fn filter_code(&self, filter: Filter) -> u8 {
        self.filter_raw(filter) & 7
    }
    pub fn vox_on_raw(&self) -> u16 {
        u16le(self.raw, 12)
    }
    pub fn vox_off_raw(&self) -> u16 {
        u16le(self.raw, 14)
    }

    /// Fifteen stored bytes. The source UI divides these by ten; this parser
    /// deliberately makes no voltage or measurement interpretation.
    pub fn battery_points_raw(&self) -> &'a [u8; 15] {
        fixed_slice(self.raw, 16)
    }
    pub fn adc_correction_raw(&self) -> u8 {
        self.raw[31]
    }
    pub fn adc_correction_code(&self) -> i16 {
        i16::from(self.raw[31]) - 128
    }
}

/// A decoded 16-bit word. The complete raw word remains available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Compander {
    raw: u16,
}

impl Compander {
    pub fn raw(&self) -> u16 {
        self.raw
    }
    pub fn mode_code(&self) -> u8 {
        (self.raw >> 14) as u8
    }
    pub fn af_code(&self) -> u8 {
        ((self.raw >> 7) & 0x7F) as u8
    }
    pub fn noise_code(&self) -> u8 {
        (self.raw & 0x7F) as u8
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Band<'a> {
    raw: &'a [u8; BAND_LEN],
    index: usize,
}

impl<'a> Band<'a> {
    pub fn raw(&self) -> &'a [u8; BAND_LEN] {
        self.raw
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn byte_offset(&self) -> usize {
        GENERAL_LEN + self.index * BAND_LEN
    }

    /// Number of columns displayed by the source tool. This does not validate
    /// the stored points or establish which frequencies they represent.
    pub fn valid_points(&self) -> usize {
        VALID_POINTS[self.index]
    }
    pub fn frequency_offset_raw(&self) -> u16 {
        u16le(self.raw, 0)
    }

    /// Offset-binary stored code (`uint16_le - 32768`), not assumed Hz.
    pub fn frequency_offset_code(&self) -> i32 {
        i32::from(self.frequency_offset_raw()) - 32768
    }

    pub fn wide(&self) -> Width<'a> {
        Width {
            raw: fixed_slice(self.raw, 2),
        }
    }
    pub fn narrow(&self) -> Width<'a> {
        Width {
            raw: fixed_slice(self.raw, 9),
        }
    }

    /// All sixteen bytes, including points not displayed by the source tool.
    pub fn table_raw(&self, table: Table) -> &'a [u8; TABLE_LEN] {
        fixed_slice(self.raw, 16 + table as usize * TABLE_LEN)
    }

    /// A raw point in 0..16. Points beyond `valid_points()` are preserved too.
    pub fn point_raw(&self, table: Table, index: usize) -> Option<u8> {
        self.table_raw(table).get(index).copied()
    }
}

/// A seven-byte wide/narrow group, with both full raw words and UI bit fields.
#[derive(Clone, Copy, Debug)]
pub struct Width<'a> {
    raw: &'a [u8; 7],
}

impl<'a> Width<'a> {
    pub fn raw(&self) -> &'a [u8; 7] {
        self.raw
    }
    pub fn mic_gain_raw(&self) -> u8 {
        self.raw[0]
    }
    pub fn tx_deviation_raw(&self) -> u16 {
        u16le(self.raw, 1)
    }
    pub fn tx_deviation_code(&self) -> u16 {
        self.tx_deviation_raw() & 0xFFF
    }
    pub fn ctcss_deviation_raw(&self) -> u8 {
        self.raw[3]
    }
    pub fn ctcss_deviation_code(&self) -> u8 {
        self.raw[3] & 0x7F
    }
    pub fn dcs_deviation_raw(&self) -> u8 {
        self.raw[4]
    }
    pub fn dcs_deviation_code(&self) -> u8 {
        self.raw[4] & 0x7F
    }
    pub fn volume_raw(&self) -> u16 {
        u16le(self.raw, 5)
    }
    pub fn volume_gain_code(&self) -> u8 {
        ((self.volume_raw() >> 4) & 0x3F) as u8
    }
    pub fn volume_dac_code(&self) -> u8 {
        (self.volume_raw() & 0xF) as u8
    }
}

fn u16le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

// Only fixed layout offsets or previously bounded band indices reach here.
fn fixed_slice<const N: usize>(data: &[u8], offset: usize) -> &[u8; N] {
    data[offset..offset + N]
        .try_into()
        .expect("internal fixed layout bounds")
}
