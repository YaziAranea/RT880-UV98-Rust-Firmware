//! Fixed-point detector for a documented 1400 Hz tone in saved PA5 samples.

pub const SAMPLE_RATE: u32 = 8_000;
const WINDOW_SAMPLES: usize = 160;
const REQUIRED_WINDOWS: usize = 5;
const MIN_RMS_COUNTS: i64 = 12;
const Q8: i64 = 256;

// round(256 * (cos, sin)(2π * 7 * n / 40)). The 1400 Hz projection at
// 8000 samples/s repeats every 40 samples. A window contains four periods
// of the table, with exactly zero sum in each quadrature to cancel DC.
#[rustfmt::skip]
const PHASORS: [(i16, i16); 40] = [
    (256, 0), (116, 228), (-150, 207), (-253, -40), (-79, -243),
    (181, -181), (243, 79), (40, 253), (-207, 150), (-228, -116),
    (0, -256), (228, -116), (207, 150), (-40, 253), (-243, 79),
    (-181, -181), (79, -243), (253, -40), (150, 207), (-116, 228),
    (-256, 0), (-116, -228), (150, -207), (253, 40), (79, 243),
    (-181, 181), (-243, -79), (-40, -253), (207, -150), (228, 116),
    (0, 256), (-228, 116), (-207, -150), (40, -253), (243, -79),
    (181, 181), (-79, 243), (-253, 40), (-150, -207), (116, -228),
];

pub fn candidate(samples: &[u16]) -> bool {
    let mut consecutive = 0;
    for window in samples.chunks_exact(WINDOW_SAMPLES) {
        if qualified(window) {
            consecutive += 1;
            if consecutive == REQUIRED_WINDOWS {
                return true;
            }
        } else {
            consecutive = 0;
        }
    }
    false
}

fn qualified(window: &[u16]) -> bool {
    let (mut i, mut q, mut sum, mut sum_squares) = (0i32, 0i32, 0u32, 0u32);
    for section in window.chunks_exact(PHASORS.len()) {
        for (&sample, &(cosine, sine)) in section.iter().zip(&PHASORS) {
            if sample > 4095 {
                return false;
            }
            let sample = i32::from(sample);
            i += sample * i32::from(cosine);
            q += sample * i32::from(sine);
            sum += sample as u32;
            sum_squares += (sample * sample) as u32;
        }
    }
    // n * (sum of squared AC samples), with no truncation of the mean.
    let n = WINDOW_SAMPLES as i64;
    let sum = i64::from(sum);
    let energy = n * i64::from(sum_squares) - sum * sum;
    if energy < (n * MIN_RMS_COUNTS).pow(2) {
        return false;
    }
    let (i, q) = (i64::from(i), i64::from(q));
    // Require 2 * projection / (Q8² * energy) >= 0.75.
    8 * (i * i + q * q) >= 3 * Q8 * Q8 * energy
}
