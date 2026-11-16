//! Frequencies/timing: ITU DTMF, FunkTronic FT634a TRC p17 and Wavecom ZVEI1.
//! BK oscillator words use 10.32444 units/Hz with the radio's 26 MHz reference.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tone {
    pub low_hz: u16,
    pub high_hz: Option<u16>,
}

pub const fn dtmf(ascii: u8) -> Option<Tone> {
    let (row, col) = match ascii {
        b'1' => (0, 0),
        b'2' => (0, 1),
        b'3' => (0, 2),
        b'A' => (0, 3),
        b'4' => (1, 0),
        b'5' => (1, 1),
        b'6' => (1, 2),
        b'B' => (1, 3),
        b'7' => (2, 0),
        b'8' => (2, 1),
        b'9' => (2, 2),
        b'C' => (2, 3),
        b'*' => (3, 0),
        b'0' => (3, 1),
        b'#' => (3, 2),
        b'D' => (3, 3),
        _ => return None,
    };
    Some(Tone {
        low_hz: [697, 770, 852, 941][row],
        high_hz: Some([1209, 1336, 1477, 1633][col]),
    })
}

pub const fn oscillator_word(hz: u16) -> u16 {
    // Split the fixed-point product so even an out-of-range public argument
    // cannot overflow u32. Legal signaling tones are all below3kHz.
    let word = hz as u32 * 10 + (hz as u32 * 3_244 + 5_000) / 10_000;
    if word > u16::MAX as u32 {
        u16::MAX
    } else {
        word as u16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standard {
    Zvei1,
    Ccir,
}
impl Standard {
    pub const fn duration_ms(self) -> u32 {
        match self {
            Self::Zvei1 => 70,
            Self::Ccir => 100,
        }
    }
    const fn frequency(self, index: usize) -> u16 {
        match self {
            Self::Zvei1 => [
                2400, 1060, 1160, 1270, 1400, 1530, 1670, 1830, 2000, 2200, 2600,
            ][index],
            Self::Ccir => [
                1981, 1124, 1197, 1275, 1358, 1446, 1540, 1640, 1747, 1860, 2110,
            ][index],
        }
    }
}

#[derive(Clone, Copy)]
pub struct FiveTone {
    frequencies: [u16; 5],
    duration_ms: u32,
    started_ms: u32,
    active: bool,
}
impl FiveTone {
    /// Numeric ANI only. An arbitrary station name is not a standard 5-tone ID.
    /// Repeated digits alternate digit/repeat/digit, including runs of 3+ digits.
    pub fn new(standard: Standard, digits: [u8; 5]) -> Option<Self> {
        if digits.iter().any(|d| *d > 9) {
            return None;
        }
        let mut frequencies = [0; 5];
        let mut previous = 255;
        for (i, digit) in digits.into_iter().enumerate() {
            let transmitted = if digit == previous { 10 } else { digit };
            frequencies[i] = standard.frequency(transmitted as usize);
            previous = transmitted;
        }
        Some(Self {
            frequencies,
            duration_ms: standard.duration_ms(),
            started_ms: 0,
            active: false,
        })
    }
    pub fn from_number(standard: Standard, mut number: u32) -> Option<Self> {
        if number > 99_999 {
            return None;
        }
        let mut digits = [0; 5];
        for digit in digits.iter_mut().rev() {
            *digit = (number % 10) as u8;
            number /= 10;
        }
        Self::new(standard, digits)
    }
    pub fn start(&mut self, now_ms: u32) {
        self.started_ms = now_ms;
        self.active = true;
    }
    pub fn cancel(&mut self) {
        self.active = false;
    }
    pub fn tone(&mut self, now_ms: u32) -> Option<Tone> {
        let elapsed = now_ms.wrapping_sub(self.started_ms);
        if !self.active || elapsed >= self.duration_ms * 5 {
            self.active = false;
            return None;
        }
        Some(Tone {
            low_hz: self.frequencies[(elapsed / self.duration_ms) as usize],
            high_hz: None,
        })
    }
    pub const fn active(&self) -> bool {
        self.active
    }
}

pub trait ToneRegisters {
    type Error;
    fn read(&mut self, register: u8) -> Result<u16, Self::Error>;
    fn write(&mut self, register: u8, value: u16) -> Result<(), Self::Error>;
}

/// Saves only modulation registers. Never writes PA, synthesizer, relay or RF
/// register30 after the session: a fault/cancel cannot accidentally re-enable TX.
/// Call begin only inside an already qualified voice-TX state. The root runtime
/// disables MIC ADC (REG30 bit 2) during tones and restores it while TX is valid.
pub struct BkToneSession {
    saved: Option<[u16; 5]>,
    current: Option<Tone>,
}
impl BkToneSession {
    pub const fn new() -> Self {
        Self {
            saved: None,
            current: None,
        }
    }
    pub fn set<R: ToneRegisters>(&mut self, io: &mut R, tone: Tone) -> Result<(), R::Error> {
        if self.current == Some(tone) {
            return Ok(());
        }
        if self.saved.is_none() {
            // Finish all reads before making a change; failed reads leave TX intact.
            self.saved = Some([
                io.read(0x47)?,
                io.read(0x50)?,
                io.read(0x70)?,
                io.read(0x71)?,
                io.read(0x72)?,
            ]);
        }
        let saved = self.saved.unwrap();
        io.write(0x50, saved[1] | 0x8000)?;
        io.write(0x47, saved[0] & !0x0f00)?;
        io.write(0x70, 0)?;
        io.write(0x71, oscillator_word(tone.low_hz))?;
        io.write(0x72, tone.high_hz.map(oscillator_word).unwrap_or(0))?;
        // 83 is the established BK DTMF generator gain, unrelated to TX power.
        io.write(
            0x70,
            if tone.high_hz.is_some() {
                0xd3d3
            } else {
                0xd300
            },
        )?;
        io.write(0x50, saved[1] & !0x8000)?;
        self.current = Some(tone);
        Ok(())
    }
    pub fn stop<R: ToneRegisters>(&mut self, io: &mut R) -> Result<(), R::Error> {
        let Some(saved) = self.saved else {
            return Ok(());
        };
        // Attempt all restoration writes even after one fails; caller must then
        // run its ordinary fault shutdown. Keep saved values available for retry.
        let mut error = None;
        for (reg, value) in [
            (0x50, saved[1] | 0x8000),
            (0x70, 0),
            (0x71, saved[3]),
            (0x72, saved[4]),
            (0x47, saved[0]),
            (0x70, saved[2]),
            (0x50, saved[1]),
        ] {
            if let Err(e) = io.write(reg, value) {
                if error.is_none() {
                    error = Some(e);
                }
            }
        }
        self.current = None;
        if let Some(e) = error {
            return Err(e);
        }
        self.saved = None;
        Ok(())
    }
    pub const fn active(&self) -> bool {
        self.saved.is_some()
    }
}
impl Default for BkToneSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_frequencies_and_words_match_reference() {
        assert_eq!(
            dtmf(b'5'),
            Some(Tone {
                low_hz: 770,
                high_hz: Some(1336)
            })
        );
        assert_eq!(
            dtmf(b'#'),
            Some(Tone {
                low_hz: 941,
                high_hz: Some(1477)
            })
        );
        assert_eq!(dtmf(b'X'), None);
        // Integer reference words may differ by one oscillator LSB on rounding.
        assert!((i32::from(oscillator_word(697)) - 0x1c1c).abs() <= 1);
        assert!((i32::from(oscillator_word(1477)) - 0x3b91).abs() <= 1);
    }
    #[test]
    fn repeated_digits_alternate_and_deadline_wraps() {
        let mut seq = FiveTone::new(Standard::Zvei1, [2; 5]).unwrap();
        seq.start(u32::MAX - 10);
        for (i, hz) in [1160, 2600, 1160, 2600, 1160].into_iter().enumerate() {
            assert_eq!(
                seq.tone((u32::MAX - 10).wrapping_add(i as u32 * 70))
                    .unwrap()
                    .low_hz,
                hz
            );
        }
        assert_eq!(seq.tone((u32::MAX - 10).wrapping_add(350)), None);
        assert!(FiveTone::from_number(Standard::Ccir, 100000).is_none());
    }
    struct Mock {
        regs: [u16; 128],
        writes: u32,
    }
    impl ToneRegisters for Mock {
        type Error = ();
        fn read(&mut self, r: u8) -> Result<u16, ()> {
            Ok(self.regs[r as usize])
        }
        fn write(&mut self, r: u8, v: u16) -> Result<(), ()> {
            assert_ne!(r, 0x30);
            self.regs[r as usize] = v;
            self.writes += 1;
            Ok(())
        }
    }
    #[test]
    fn tone_restore_does_not_touch_rf_state() {
        let mut io = Mock {
            regs: [0x1234; 128],
            writes: 0,
        };
        let saved = io.regs;
        let mut session = BkToneSession::new();
        session.set(&mut io, dtmf(b'0').unwrap()).unwrap();
        let writes = io.writes;
        session.set(&mut io, dtmf(b'0').unwrap()).unwrap();
        assert_eq!(io.writes, writes);
        session.stop(&mut io).unwrap();
        assert_eq!(io.regs, saved);
    }
}
