//! Desktop WAV import. Accept uncompressed integer PCM and downmix before
//! low-pass resampling to the voice slot rate. Never truncate a message.
use super::{encode, MAX_FRAMES, RATE, SLOT_BYTES};
use std::{f64::consts::PI, vec::Vec};

#[cfg(feature = "mp3")]
mod mp3;
#[cfg(feature = "mp3")]
pub use mp3::decode_mp3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Duration,
    TooLarge,
}

// Bound desktop memory independently of the compressed file size.
const MAX_SOURCE_FRAMES: usize = 16_000_000;

pub struct Recording {
    rate: usize,
    mono: Vec<i16>,
}

impl Recording {
    pub fn duration_ms(&self) -> u32 {
        (self.mono.len() as u64 * 1000).div_ceil(self.rate as u64) as u32
    }

    fn window_frames(&self) -> usize {
        (MAX_FRAMES * self.rate / RATE).min(self.mono.len())
    }

    pub fn last_start_ms(&self) -> u32 {
        let last = self.mono.len() - self.window_frames();
        (last as u64 * 1000).div_ceil(self.rate as u64) as u32
    }

    /// The last slider position includes the final sample, even at fractional milliseconds.
    pub fn clip(&self, start_ms: u32) -> Result<[u8; SLOT_BYTES], Error> {
        if start_ms > self.last_start_ms() {
            return Err(Error::Duration);
        }
        let frames = self.window_frames();
        let start = (u64::from(start_ms) * self.rate as u64 / 1000)
            .min((self.mono.len() - frames) as u64) as usize;
        encode_window(&self.mono[start..start + frames], self.rate)
    }
}

fn word(header: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([header[at], header[at + 1]])
}
fn dword(header: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(header[at..at + 4].try_into().unwrap())
}

pub fn import(bytes: &[u8]) -> Result<[u8; SLOT_BYTES], Error> {
    let recording = decode(bytes)?;
    if recording.last_start_ms() != 0 {
        return Err(Error::Duration);
    }
    recording.clip(0)
}

pub fn decode(bytes: &[u8]) -> Result<Recording, Error> {
    if bytes.get(..4) != Some(b"RIFF")
        || bytes.get(8..12) != Some(b"WAVE")
        || usize::try_from(dword(bytes, 4)).ok() != Some(bytes.len() - 8)
    {
        return Err(Error::Format);
    }
    let mut fmt = None;
    let mut data = None;
    let mut chunks = &bytes[12..];
    while !chunks.is_empty() {
        let header = chunks.get(..8).ok_or(Error::Format)?;
        let size = dword(header, 4) as usize;
        let tail = &chunks[8..];
        let part = tail.get(..size).ok_or(Error::Format)?;
        match &header[..4] {
            b"fmt " if fmt.is_none() => fmt = Some(part),
            b"data" if data.is_none() => data = Some(part),
            b"fmt " | b"data" => return Err(Error::Format),
            _ => {}
        }
        chunks = if size == tail.len() {
            &[]
        } else {
            tail.get(size.checked_add(size & 1).ok_or(Error::Format)?..)
                .ok_or(Error::Format)?
        };
    }
    let fmt = fmt.filter(|f| f.len() >= 16).ok_or(Error::Format)?;
    let channels = word(fmt, 2) as usize;
    let rate = dword(fmt, 4) as usize;
    let bits = word(fmt, 14) as usize;
    if word(fmt, 0) != 1
        || !(1..=2).contains(&channels)
        || !(4000..=192000).contains(&rate)
        || !matches!(bits, 8 | 16 | 24 | 32)
    {
        return Err(Error::Format);
    }
    let width = bits / 8;
    let block = channels * width;
    let data = data.ok_or(Error::Format)?;
    if usize::from(word(fmt, 12)) != block
        || dword(fmt, 8) as usize != rate * block
        || !data.len().is_multiple_of(block)
    {
        return Err(Error::Format);
    }
    let frames = data.len() / block;
    if frames == 0 {
        return Err(Error::Duration);
    }
    if frames > MAX_SOURCE_FRAMES {
        return Err(Error::TooLarge);
    }

    let mono = data
        .chunks_exact(block)
        .map(|frame| {
            (frame
                .chunks_exact(width)
                .map(|v| match width {
                    1 => (i32::from(v[0]) - 128) << 8,
                    2 => i32::from(i16::from_le_bytes([v[0], v[1]])),
                    3 => i32::from_le_bytes([0, v[0], v[1], v[2]]) >> 16,
                    _ => i32::from_le_bytes(v.try_into().unwrap()) >> 16,
                })
                .sum::<i32>()
                / channels as i32) as i16
        })
        .collect();
    Ok(Recording { rate, mono })
}

fn encode_window(mono: &[i16], rate: usize) -> Result<[u8; SLOT_BYTES], Error> {
    let frames = mono.len();
    let count = (frames as u64 * RATE as u64).div_ceil(rate as u64);
    // Hann-windowed sinc at 0.44 of the output rate, with the kernel width
    // following the decimation ratio (e.g. 48 kHz input).
    let ratio = rate as f64 / RATE as f64;
    let radius = (6.0 * ratio).ceil() as isize;
    let mut pcm = Vec::with_capacity(count as usize);
    for i in 0..count {
        let position = i as f64 * ratio;
        let center = position.floor() as isize;
        let mut sum = 0.0;
        let mut weight = 0.0;
        for j in (center - radius + 1).max(0)..=(center + radius).min(frames as isize - 1) {
            let d = position - j as f64;
            let x = PI * 0.88 * d / ratio;
            let sinc = if x.abs() < 1e-9 { 1.0 } else { x.sin() / x };
            let w = sinc * (0.5 + 0.5 * (PI * d / radius as f64).cos());
            sum += mono[j as usize] as f64 * w;
            weight += w;
        }
        pcm.push((sum / weight).round().clamp(-32768.0, 32767.0) as i16);
    }
    encode(&pcm).ok_or(Error::Duration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Samples;
    use std::vec;

    pub fn wave(rate: u32, values: &[i16], channels: u16) -> Vec<u8> {
        let length = values.len() as u32 * 2;
        let mut b = b"RIFF".to_vec();
        b.extend((length + 36).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(channels.to_le_bytes());
        b.extend(rate.to_le_bytes());
        b.extend((rate * 2 * u32::from(channels)).to_le_bytes());
        b.extend((channels * 2).to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend(length.to_le_bytes());
        for v in values {
            b.extend(v.to_le_bytes());
        }
        b
    }

    #[test]
    fn stereo_downmix_cancels_and_rejects_broken_or_long_wav() {
        let mut wave = wave(11025, &[10000, -10000].repeat(11025), 2);
        let slot = import(&wave).unwrap();
        let samples = Samples::from_body(&slot[16..]).unwrap();
        assert_eq!(samples.len(), 8000);
        assert!(samples.clone().all(|v| v == 128));
        wave.pop();
        assert_eq!(import(&wave), Err(Error::Format));
        assert_eq!(
            import(&self::wave(8000, &vec![0; 12001], 1)),
            Err(Error::Duration)
        );
    }

    #[test]
    fn honors_integer_width_sign_and_final_unpadded_pcm() {
        for (bits, sample) in [
            (8u16, vec![192]),
            (16, vec![0, 64]),
            (24, vec![0, 0, 64]),
            (32, vec![0, 0, 0, 64]),
        ] {
            let mut bytes = wave(4000, &[], 1);
            let width = bits / 8;
            bytes[28..32].copy_from_slice(&(4000u32 * u32::from(width)).to_le_bytes());
            bytes[32..34].copy_from_slice(&width.to_le_bytes());
            bytes[34..36].copy_from_slice(&bits.to_le_bytes());
            let payload = sample.repeat(101);
            bytes[4..8].copy_from_slice(&(36 + payload.len() as u32).to_le_bytes());
            bytes[40..44].copy_from_slice(&(payload.len() as u32).to_le_bytes());
            bytes.extend(payload);
            let slot = import(&bytes).unwrap();
            let decoded = Samples::from_body(&slot[16..]).unwrap();
            assert_eq!(decoded.len(), 202);
            assert!(decoded.skip(120).all(|s| (190..=194).contains(&s)));
        }
    }

    #[test]
    fn speech_band_survives_but_out_of_band_tone_is_filtered() {
        let rms = |hz: f64| {
            let input: Vec<_> = (0..4800)
                .map(|i| (12000.0 * (2.0 * PI * hz * i as f64 / 48000.0).sin()) as i16)
                .collect();
            let slot = import(&wave(48000, &input, 1)).unwrap();
            Samples::from_body(&slot[16..])
                .unwrap()
                .skip(100)
                .take(600)
                .map(|v| (f64::from(v) - 128.0).powi(2))
                .sum::<f64>()
        };
        assert!(rms(500.0) > 100.0 * rms(6500.0));
    }
}
