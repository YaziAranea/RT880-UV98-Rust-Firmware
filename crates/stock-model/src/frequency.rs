//! Frequency-record selection and register-word correction from RT880 1.16.
//!
//! Evidence: selector `0x08012A30`, setters `0x08003CCC` / `0x08004B88` in the
//! hash-pinned related RT880 image. Inputs and outputs are stock integer codes;
//! they do not establish measured frequency accuracy or UV98 compatibility.
//! Selection is an ordered, inclusive lookup with a floored table index, not
//! interpolation. Routing flags, record caching and fallback loading are outside
//! this pure API. No default record is substituted for an unrecognised code.

use uv98_calibration::{BAND_LEN, GENERAL_LEN};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    band_index: usize,
    table_index: usize,
}

impl Selection {
    pub const fn band_index(self) -> usize {
        self.band_index
    }

    pub const fn table_index(self) -> usize {
        self.table_index
    }

    /// Offset in the primary 4096-byte calibration block, not an I/O operation.
    pub const fn byte_offset(self) -> usize {
        GENERAL_LEN + self.band_index * BAND_LEN
    }
}

// Branch order is significant at shared endpoints. The last two windows
// intentionally preserve the observed repeated index 13; this is not corrected
// to index 14 without evidence for a different firmware image.
const WINDOWS: [(u32, u32, u32, usize); 15] = [
    (13_600_000, 17_400_000, 13_500_000, 0),
    (40_000_000, 48_000_000, 40_000_000, 1),
    (1_800_000, 6_400_000, 1_500_000, 2),
    (6_400_000, 13_600_000, 6_000_000, 3),
    (17_400_000, 24_000_000, 17_000_000, 4),
    (24_000_000, 32_000_000, 24_000_000, 5),
    (32_000_000, 40_000_000, 32_000_000, 6),
    (48_000_000, 56_000_000, 48_000_000, 7),
    (56_000_000, 66_000_000, 56_000_000, 8),
    (84_000_000, 92_000_000, 84_000_000, 9),
    (92_000_000, 100_000_000, 92_000_000, 10),
    (100_000_000, 108_000_000, 100_000_000, 11),
    (108_000_000, 116_000_000, 108_000_000, 12),
    (116_000_000, 124_000_000, 116_000_000, 13),
    (124_000_000, 130_000_000, 124_000_000, 13),
];

/// Select the stored band and table point using the reference's branch order.
///
/// `None` means this routine recognises no window. The reference then leaves its
/// cached calibration untouched; this API exposes the absence rather than
/// silently borrowing a prior band's record.
pub fn select(frequency_code: u32) -> Option<Selection> {
    for (lower, upper, origin, band_index) in WINDOWS {
        if lower <= frequency_code && frequency_code <= upper {
            let stock_index = ((frequency_code - origin) / 500_000) as u8;
            return Some(Selection {
                band_index,
                table_index: usize::from(stock_index.min(15)),
            });
        }
    }
    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrequencyWords {
    pub code: u32,
    pub reg38: u16,
    pub reg39: u16,
}

/// Compute the two register arguments, without writing either register.
///
/// The reference uses a band-wide u16 offset, biased by 32768. ARM 32-bit
/// addition/subtraction wraps; preserve that behavior even for synthetic
/// boundary inputs. This function does not select or validate an operating band.
pub const fn correct_code(frequency_code: u32, raw_offset: u16) -> FrequencyWords {
    let code = frequency_code
        .wrapping_add(raw_offset as u32)
        .wrapping_sub(32_768);
    FrequencyWords {
        code,
        reg38: code as u16,
        reg39: (code >> 16) as u16,
    }
}
