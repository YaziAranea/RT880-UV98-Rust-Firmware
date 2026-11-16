//! DTMF frequencies follow ITU-T Q.23.
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
