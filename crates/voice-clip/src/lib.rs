#![no_std]
#![forbid(unsafe_code)]
//! One short mono IMA ADPCM clip. The fixed-size patchable slot belongs to the
//! application image, outside its vectors, calibration profile and settings journal.
#[cfg(feature = "wav")]
extern crate std;
#[cfg(feature = "wav")]
pub mod wav;

pub const MAGIC: &[u8; 16] = b"UV98VOICECLIP01\0";
pub const RATE: usize = 4000;
pub const MAX_FRAMES: usize = 6000;
pub const SLOT_BYTES: usize = MAGIC.len() + 4 + MAX_FRAMES / 2;
pub const CONFIG_MAGIC: &[u8; 16] = b"UV98VOICECONF01\0";
pub const CONFIG_BYTES: usize = CONFIG_MAGIC.len() + 6;
pub const MAX_DELAY_MS: u16 = 3000;

/// Defaults embedded by CPS. A saved keypad assignment on the radio takes
/// precedence until the next application image is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoiceConfig {
    pub key: Option<u8>,
    pub delay_ms: u16,
}

impl VoiceConfig {
    pub const UNASSIGNED: Self = Self {
        key: None,
        delay_ms: 0,
    };

    /// Body: digit (0xff means unassigned), reserved zero, LE milliseconds,
    /// then CRC-16/CCITT-FALSE over those four bytes.
    pub fn from_body(body: &[u8]) -> Option<Self> {
        let &[digit, 0, low, high, crc_low, crc_high] = body else {
            return None;
        };
        let delay_ms = u16::from_le_bytes([low, high]);
        if delay_ms > MAX_DELAY_MS || crc16(&body[..4]) != u16::from_le_bytes([crc_low, crc_high]) {
            return None;
        }
        let key = match digit {
            0..=9 => Some(digit),
            0xff => None,
            _ => return None,
        };
        Some(Self { key, delay_ms })
    }

    pub const fn encode(self) -> Option<[u8; CONFIG_BYTES]> {
        let digit = match self.key {
            Some(digit @ 0..=9) => digit,
            None => 0xff,
            _ => return None,
        };
        if self.delay_ms > MAX_DELAY_MS {
            return None;
        }
        let [low, high] = self.delay_ms.to_le_bytes();
        let body = [digit, 0, low, high];
        let crc = crc16(&body).to_le_bytes();
        let mut record = [0u8; CONFIG_BYTES];
        let (magic, rest) = record.split_at_mut(CONFIG_MAGIC.len());
        magic.copy_from_slice(CONFIG_MAGIC);
        rest.copy_from_slice(&[digit, 0, low, high, crc[0], crc[1]]);
        Some(record)
    }
}
const STEP: [i16; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const ADJUST: [i8; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

const fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    let mut i = 0;
    while i < bytes.len() {
        crc ^= (bytes[i] as u16) << 8;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
            bit += 1;
        }
        i += 1;
    }
    crc
}

#[derive(Clone, Default)]
struct Ima {
    sample: i32,
    index: i32,
}
impl Ima {
    fn advance(&mut self, code: u8) -> i32 {
        let step = i32::from(STEP[self.index as usize]);
        let mut difference = step >> 3;
        if code & 4 != 0 {
            difference += step;
        }
        if code & 2 != 0 {
            difference += step >> 1;
        }
        if code & 1 != 0 {
            difference += step >> 2;
        }
        self.sample = (self.sample
            + if code & 8 != 0 {
                -difference
            } else {
                difference
            })
        .clamp(-32768, 32767);
        self.index = (self.index + i32::from(ADJUST[usize::from(code & 7)])).clamp(0, 88);
        self.sample
    }

    fn encode(&mut self, value: i16) -> u8 {
        let delta = i32::from(value) - self.sample;
        let mut code = if delta < 0 { 8 } else { 0 };
        let mut remaining = delta.abs();
        let step = i32::from(STEP[self.index as usize]);
        for (mask, amount) in [(4, step), (2, step >> 1), (1, step >> 2)] {
            if remaining >= amount {
                code |= mask;
                remaining -= amount;
            }
        }
        self.advance(code);
        code
    }
}

/// Layout: magic, CRC16-CCITT-FALSE, sample count (both LE), low-nibble-first
/// IMA codes, unused padding. CRC covers the count and all used code bytes.
pub fn encode(pcm: &[i16]) -> Option<[u8; SLOT_BYTES]> {
    if pcm.is_empty() || pcm.len() > MAX_FRAMES {
        return None;
    }
    let mut slot = [0; SLOT_BYTES];
    slot[..16].copy_from_slice(MAGIC);
    slot[18..20].copy_from_slice(&(pcm.len() as u16).to_le_bytes());
    let mut state = Ima::default();
    for (i, &sample) in pcm.iter().enumerate() {
        slot[20 + i / 2] |= state.encode(sample) << (4 * (i & 1));
    }
    let crc = crc16(&slot[18..20 + pcm.len().div_ceil(2)]);
    slot[16..18].copy_from_slice(&crc.to_le_bytes());
    Some(slot)
}

#[derive(Clone)]
pub struct Samples<'a> {
    codes: &'a [u8],
    state: Ima,
    at: usize,
    end: usize,
}

impl<'a> Samples<'a> {
    /// The caller selects the slot by symbol (firmware) or by magic (CPS).
    pub fn from_body(body: &'a [u8]) -> Option<Self> {
        if body.len() != SLOT_BYTES - MAGIC.len() {
            return None;
        }
        let count = u16::from_le_bytes([body[2], body[3]]) as usize;
        if !(1..=MAX_FRAMES).contains(&count) {
            return None;
        }
        let end = 4 + count.div_ceil(2);
        if crc16(&body[2..end]) != u16::from_le_bytes([body[0], body[1]]) {
            return None;
        }
        Some(Self {
            codes: &body[4..end],
            state: Ima::default(),
            at: 0,
            end: count * 2,
        })
    }
}

impl Iterator for Samples<'_> {
    type Item = u8;
    /// DAC runs at 8 kHz; interpolate adjacent 4 kHz decoded samples.
    fn next(&mut self) -> Option<u8> {
        if self.at == self.end {
            return None;
        }
        let index = self.at / 2;
        let previous = self.state.sample;
        let sample = if self.at & 1 == 0 {
            let packed = self.codes[index / 2];
            let code = (packed >> ((index & 1) * 4)) & 15;
            (previous + self.state.advance(code)) / 2
        } else {
            previous
        };
        self.at += 1;
        Some(((sample + 32768) >> 8) as u8)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end - self.at;
        (remaining, Some(remaining))
    }
}
impl ExactSizeIterator for Samples<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voice_configuration_checks_crc_ranges_and_reserved_byte() {
        for key in (0..10).map(Some).chain(core::iter::once(None)) {
            for delay_ms in [0, 1, 437, MAX_DELAY_MS] {
                let config = VoiceConfig { key, delay_ms };
                let raw = config.encode().unwrap();
                assert_eq!(&raw[..16], CONFIG_MAGIC);
                assert_eq!(VoiceConfig::from_body(&raw[16..]), Some(config));
                for index in 16..CONFIG_BYTES {
                    let mut broken = raw;
                    broken[index] ^= 1;
                    assert_eq!(VoiceConfig::from_body(&broken[16..]), None);
                }
            }
        }
        for invalid in [[10, 0, 0, 0], [0, 1, 0, 0], [0, 0, 0xb9, 0x0b]] {
            let mut body = [0; 6];
            body[..4].copy_from_slice(&invalid);
            body[4..].copy_from_slice(&crc16(&invalid).to_le_bytes());
            assert_eq!(VoiceConfig::from_body(&body), None);
        }
        assert!(VoiceConfig {
            key: Some(10),
            delay_ms: 0
        }
        .encode()
        .is_none());
        assert!(VoiceConfig {
            key: None,
            delay_ms: MAX_DELAY_MS + 1
        }
        .encode()
        .is_none());
    }
    #[test]
    fn known_nibbles_and_crc() {
        assert_eq!(crc16(b"123456789"), 0x29b1);
        let mut decoder = Ima::default();
        assert_eq!(
            [7, 7, 0, 15, 8].map(|n| decoder.advance(n)),
            [11, 41, 45, -11, -19]
        );
    }
    #[test]
    fn validates_length_checksum_and_replays_odd_clip() {
        let mut slot = encode(&[12000; 51]).unwrap();
        let playback = Samples::from_body(&slot[16..]).unwrap();
        assert_eq!(playback.len(), 102);
        assert_eq!(playback.clone().last(), playback.last());
        slot[21] ^= 1;
        assert!(Samples::from_body(&slot[16..]).is_none());
        assert!(encode(&[]).is_none());
        assert!(encode(&[0; MAX_FRAMES + 1]).is_none());
        let longest = encode(&[0; MAX_FRAMES]).unwrap();
        assert!(Samples::from_body(&longest[16..])
            .unwrap()
            .all(|v| v == 128));
    }
}
