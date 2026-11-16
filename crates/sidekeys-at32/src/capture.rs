pub const RX_AUDIO_HZ: u32 = 8_000;
pub const RX_AUDIO_SAMPLES: usize = 2_048;
pub(crate) const PERIOD_CYCLES: u32 = 48_000_000 / RX_AUDIO_HZ;
pub(crate) const MAX_LATE_CYCLES: u32 = PERIOD_CYCLES / 20; // 6.25 us at 48 MHz

/// DWT cycle counts sampled immediately after writing each software trigger.
/// Timestamps may wrap. A successful block met every trigger deadline within
/// 5% of a sample period. The nominal 8 kHz assumes a 48 MHz HCLK.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureTiming {
    pub first_trigger_cycle: u32,
    pub last_trigger_cycle: u32,
    pub max_late_cycles: u32,
    pub min_interval_cycles: u32,
    pub max_interval_cycles: u32,
}
use crate::Error;

/// ADC-code moments for one complete block. DC is `sum / RX_AUDIO_SAMPLES`.
/// AC RMS is the square root of the mean square minus the squared DC.
/// Clip counts record exact endpoint codes, 0 and 4095.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureStats {
    pub min: u16,
    pub max: u16,
    pub sum: u32,
    pub sum_squares: u64,
    pub clip_low: u16,
    pub clip_high: u16,
}

impl CaptureStats {
    pub fn from_samples(samples: &[u16; RX_AUDIO_SAMPLES]) -> Result<Self, Error> {
        let mut stats = Self {
            min: 4095,
            max: 0,
            sum: 0,
            sum_squares: 0,
            clip_low: 0,
            clip_high: 0,
        };
        for &sample in samples {
            if sample > 4095 {
                return Err(Error::InvalidSample);
            }
            let code = u32::from(sample);
            stats.min = stats.min.min(sample);
            stats.max = stats.max.max(sample);
            stats.sum += code;
            stats.sum_squares += u64::from(code * code);
            stats.clip_low += u16::from(sample == 0);
            stats.clip_high += u16::from(sample == 4095);
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_counts_and_moments_are_exact_at_full_scale() {
        let mut samples = [4095; RX_AUDIO_SAMPLES];
        samples[0] = 0;
        samples[1] = 2048;
        assert_eq!(
            CaptureStats::from_samples(&samples),
            Ok(CaptureStats {
                min: 0,
                max: 4095,
                sum: (RX_AUDIO_SAMPLES as u32 - 2) * 4095 + 2048,
                sum_squares: (RX_AUDIO_SAMPLES as u64 - 2) * 4095 * 4095 + 2048 * 2048,
                clip_low: 1,
                clip_high: RX_AUDIO_SAMPLES as u16 - 2,
            })
        );
        samples[0] = 4096;
        assert_eq!(
            CaptureStats::from_samples(&samples),
            Err(Error::InvalidSample)
        );
    }
}
