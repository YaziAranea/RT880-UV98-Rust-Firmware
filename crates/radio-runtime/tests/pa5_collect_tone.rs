//! Test the same fixed-point detector used by the isolated PA5 image.

#[path = "../../../firmware-diagnostic/src/collect_tone.rs"]
mod collect_tone;

const SAMPLES: usize = 2048;
const RATE: f64 = collect_tone::SAMPLE_RATE as f64;

fn tone(hz: f64, amplitude: f64, samples: usize) -> [u16; SAMPLES] {
    core::array::from_fn(|i| {
        let value = if i < samples {
            amplitude * (core::f64::consts::TAU * hz * i as f64 / RATE + 0.23).sin()
        } else {
            0.0
        };
        (2035.0 + value).round() as u16
    })
}

#[test]
fn accepts_a_sustained_1400_hz_tone_with_bias_and_noise() {
    let mut samples = tone(1400.0, 460.0, SAMPLES);
    let mut random = 29u32;
    for sample in &mut samples {
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *sample = (i32::from(*sample) + (random >> 24) as i32 - 128) as u16;
    }
    assert!(collect_tone::candidate(&samples));
    assert!(collect_tone::candidate(&samples[151..]));
}

#[test]
fn rejects_neighboring_frequencies_and_quiet_short_or_corrupt_samples() {
    for hz in [1350.0, 1375.0, 1380.0, 1420.0, 1425.0, 1450.0] {
        assert!(!collect_tone::candidate(&tone(hz, 500.0, SAMPLES)), "{hz}");
    }
    assert!(!collect_tone::candidate(&tone(1400.0, 500.0, 640)));
    assert!(!collect_tone::candidate(&tone(1400.0, 5.0, SAMPLES)));
    assert!(!collect_tone::candidate(&[2250; SAMPLES]));
    assert!(!collect_tone::candidate(&[u16::MAX; SAMPLES]));
    assert!(!collect_tone::candidate(&[]));
}

#[test]
fn rejects_noise_and_does_not_join_separate_short_tones() {
    let mut random = 53u32;
    let noise: [u16; SAMPLES] = core::array::from_fn(|_| {
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (1700 + (random >> 22)) as u16
    });
    assert!(!collect_tone::candidate(&noise));

    let mut interrupted = tone(1400.0, 800.0, SAMPLES);
    interrupted[640..1440].fill(2035);
    assert!(!collect_tone::candidate(&interrupted));
}
