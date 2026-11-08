//! Pure computations observed in the related RT880 1.16 image.
//!
//! These functions perform no I/O and never repair, rewrite, or substitute
//! calibration bytes. ADC/VOX algorithms below are separate from the tuning
//! tool's similarly named general fields: those fields were not consumed in
//! the traced stock paths. Exact UV98 MCU-image identity remains unverified.

/// Decision observed at 0x08013E3C with its ordinary mode argument zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredSource {
    Primary,
    Backup,
    /// Stock would copy built-in ROM defaults into RAM. This model refuses to
    /// provide a read plan or defaults when both stored markers are invalid.
    NoValidStoredCopy,
}

/// The observed marker check is not a checksum or comprehensive validity test.
pub const fn select_stored_source(primary_marker: u8, backup_marker: u8) -> StoredSource {
    if primary_marker == 0x9A {
        StoredSource::Primary
    } else if backup_marker == 0x9A {
        StoredSource::Backup
    } else {
        StoredSource::NoValidStoredCopy
    }
}

/// A description of a read, not an operation on a device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadRange {
    pub address: u32,
    pub length: u16,
}

/// Range containing the 32 general bytes selected by the ordinary load path.
pub const fn general_read_range(source: StoredSource) -> Option<ReadRange> {
    let address = match source {
        StoredSource::Primary => 0,
        StoredSource::Backup => 0x1000,
        StoredSource::NoValidStoredCopy => return None,
    };
    Some(ReadRange {
        address,
        length: 32,
    })
}

/// Integer conversion at 0x080121D2..0x080121E4, from a DMA-sampled u16.
///
/// This returns the firmware's numeric code, not a calibrated voltage. The
/// traced path does not read tuning general[31] or general[16..31].
pub const fn battery_adc_code(dma_sample: u16) -> u16 {
    ((dma_sample as u32 * 3) / 51 + 2) as u16
}

/// Fixed indicator mapping at 0x080122AC..0x08012348.
///
/// Callers store the ADC code to u8 before this operation. This is an ordinal
/// display code, not percent capacity. In particular, output code 5 is skipped.
pub const fn battery_indicator_code(code: u8) -> u8 {
    match code {
        82..=255 => 19,
        69..=81 => code - 63,
        67..=68 => 4,
        66 => 3,
        64..=65 => 2,
        62..=63 => 1,
        _ => 0,
    }
}

/// VOX comparison at 0x08005708, with thresholds supplied by 0x080227EA.
///
/// `sample` is the raw value read from RF register 0x64. `threshold` is the
/// user setting, not the tuning tool's general[12..16] calibration fields.
/// Upper threshold wraps at 256 exactly as the caller's UXTB instruction does.
/// The upper comparison runs first, including when wrapping reverses the range.
/// No transmit state, delay, or hardware activation is implemented here.
pub const fn vox_hysteresis(sample: u16, threshold: u8, previous: bool) -> bool {
    let upper = threshold.wrapping_add(10) as u16;
    if sample >= upper {
        true
    } else if sample <= threshold as u16 {
        false
    } else {
        previous
    }
}

/// Register words assembled by stock functions 0x08004ECC and 0x08004F60.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SquelchWords {
    pub noise_4f: u16,
    pub squelch_78: u16,
}

const NOISE_LEVEL_OFFSETS: [u8; 11] = [10, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];
const SQUELCH_LEVEL_OFFSETS: [u8; 11] = [0, 0, 2, 4, 6, 10, 16, 22, 28, 34, 38];

const fn threshold_word(calibration: u8, offset: u8) -> u16 {
    let upper = calibration.wrapping_add(2).wrapping_add(offset);
    let lower = calibration.wrapping_sub(4).wrapping_add(offset);
    ((upper as u16) << 8) | lower as u16
}

/// Apply the two ROM level tables to already selected band calibration codes.
///
/// `level` must be 0..=10. Stock directly indexes the tables without checking;
/// this pure API refuses an out-of-range input. Each byte wraps modulo 256:
/// stock STRB truncation is reproduced, not changed to saturation.
pub fn squelch_words(level: u8, noise: u8, squelch: u8) -> Option<SquelchWords> {
    let noise_offset = *NOISE_LEVEL_OFFSETS.get(level as usize)?;
    let squelch_offset = *SQUELCH_LEVEL_OFFSETS.get(level as usize)?;
    Some(SquelchWords {
        noise_4f: threshold_word(noise, noise_offset),
        squelch_78: threshold_word(squelch, squelch_offset),
    })
}
