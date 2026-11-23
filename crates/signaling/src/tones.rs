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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxKey {
    Dtmf(u8),
    Toneburst,
    Memory,
}

const DEBOUNCE_MS: u32 = 12;
const BURST_MS: u32 = 500;
const MARK_MS: u32 = 100;
const GAP_MS: u32 = 70;
const MEMORY_CAPACITY: usize = 12;
const BURST_TONE: Tone = Tone {
    low_hz: 1750,
    high_hz: None,
};

#[derive(Clone, Copy)]
enum Phase {
    Idle,
    Burst,
    Mark(u8),
    Gap(u8),
}

impl Phase {
    fn start_key(self) -> Option<TxKey> {
        match self {
            Self::Idle => None,
            Self::Burst => Some(TxKey::Toneburst),
            Self::Mark(_) | Self::Gap(_) => Some(TxKey::Memory),
        }
    }
}

/// Keypad and timed tones for an already qualified, physically held voice TX.
/// The caller must reset this state when TX loses that qualification.
pub struct TxTones {
    key: Option<TxKey>,
    key_since: Option<u32>,
    pressed: Option<TxKey>,
    macros_armed: bool,
    manual_blocked: bool,
    phase: Phase,
    phase_since: u32,
    memory: [u8; MEMORY_CAPACITY],
    memory_len: u8,
}

impl TxTones {
    pub const fn new() -> Self {
        Self {
            key: None,
            key_since: None,
            pressed: None,
            macros_armed: false,
            manual_blocked: false,
            phase: Phase::Idle,
            phase_since: 0,
            memory: [0; MEMORY_CAPACITY],
            memory_len: 0,
        }
    }

    /// A modem taking ownership discards the sequence and requires a fresh press.
    pub fn cancel(&mut self) {
        *self = Self::new();
        self.manual_blocked = true;
    }

    /// True while a timed sequence owns the microphone, including memory gaps.
    /// The caller also mutes the microphone whenever a manual tone is returned.
    pub const fn mute_mic(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    pub fn mark_expired(&self, now: u32) -> bool {
        let duration = match self.phase {
            Phase::Burst => BURST_MS,
            Phase::Mark(_) => MARK_MS,
            _ => return false,
        };
        now.wrapping_sub(self.phase_since) >= duration
    }

    /// Resume debouncing with the next real sample. A brief unstable scan
    /// neither confirms a new edge nor erases a previously confirmed release.
    pub fn interrupt_key_debounce(&mut self) {
        self.key_since = None;
    }

    pub fn sample(&mut self, now: u32, key: Option<TxKey>, memory: &[u8]) -> Option<Tone> {
        let manual = match key {
            Some(TxKey::Dtmf(symbol)) => dtmf(symbol),
            _ => None,
        };
        let key = match key {
            Some(TxKey::Dtmf(_)) if manual.is_none() => None,
            _ => key,
        };
        let elapsed = self.key_since.map(|since| now.wrapping_sub(since));
        if key.is_none()
            && self.key.is_none()
            && elapsed.is_some_and(|elapsed| elapsed >= DEBOUNCE_MS)
        {
            self.macros_armed = true;
            self.manual_blocked = false;
            self.pressed = None;
        }
        let stable = if self.key != key || elapsed.is_none() {
            self.key = key;
            self.key_since = Some(now);
            false
        } else {
            elapsed.is_some_and(|elapsed| elapsed >= DEBOUNCE_MS)
        };
        let new_press = stable && key.is_some() && self.pressed != key;
        let macro_pressed = new_press && self.macros_armed;
        if new_press {
            self.pressed = key;
            self.macros_armed = false;
        }

        if let Some(start_key) = self.phase.start_key() {
            if key.is_some_and(|key| key != start_key) {
                self.phase = Phase::Idle;
            } else {
                return self.advance(now);
            }
        }
        if macro_pressed {
            match key {
                Some(TxKey::Toneburst) => self.start_phase(now, Phase::Burst),
                Some(TxKey::Memory)
                    if (1..=MEMORY_CAPACITY).contains(&memory.len())
                        && memory
                            .iter()
                            .all(|&symbol| matches!(symbol, b'0'..=b'9' | b'*' | b'#')) =>
                {
                    self.memory[..memory.len()].copy_from_slice(memory);
                    self.memory_len = memory.len() as u8;
                    self.start_phase(now, Phase::Mark(0));
                }
                _ => {}
            }
        }
        if self.mute_mic() {
            return self.advance(now);
        }
        if stable && !self.manual_blocked {
            return manual;
        }
        None
    }

    /// Anchor a timed mark or gap to the successfully applied hardware step.
    pub fn applied_at(&mut self, now: u32) {
        self.phase_since = now;
    }

    fn start_phase(&mut self, now: u32, phase: Phase) {
        self.phase = phase;
        self.phase_since = now;
    }

    fn advance(&mut self, now: u32) -> Option<Tone> {
        let elapsed = now.wrapping_sub(self.phase_since);
        // Each transition starts at the observed time. A late poll must leave
        // a full gap before the next digit and never skip or bunch up digits.
        match self.phase {
            Phase::Burst if elapsed < BURST_MS => Some(BURST_TONE),
            Phase::Mark(i) if elapsed < MARK_MS => dtmf(self.memory[i as usize]),
            Phase::Mark(i) if i + 1 < self.memory_len => {
                self.start_phase(now, Phase::Gap(i + 1));
                None
            }
            Phase::Gap(i) if elapsed >= GAP_MS => {
                self.start_phase(now, Phase::Mark(i));
                dtmf(self.memory[i as usize])
            }
            Phase::Gap(_) => None,
            _ => {
                self.phase = Phase::Idle;
                None
            }
        }
    }
}

impl Default for TxTones {
    fn default() -> Self {
        Self::new()
    }
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

    #[test]
    fn manual_dtmf_waits_for_debounce_then_follows_the_held_key() {
        let mut tx = TxTones::new();
        let one = Some(TxKey::Dtmf(b'1'));
        let two = Some(TxKey::Dtmf(b'2'));
        assert_eq!(tx.sample(100, one, b""), None);
        assert_eq!(tx.sample(111, one, b""), None);
        assert_eq!(tx.sample(112, one, b""), dtmf(b'1'));
        assert_eq!(tx.sample(900, one, b""), dtmf(b'1'));
        assert_eq!(tx.sample(901, two, b""), None);
        assert_eq!(tx.sample(913, two, b""), dtmf(b'2'));
        assert_eq!(tx.sample(914, None, b""), None);
    }

    #[test]
    fn toneburst_needs_a_clean_release_and_plays_once_per_press() {
        let mut tx = TxTones::new();
        let burst = Some(TxKey::Toneburst);
        assert_eq!(tx.sample(0, burst, b""), None);
        assert_eq!(tx.sample(200, burst, b""), None);
        assert_eq!(tx.sample(201, None, b""), None);
        assert_eq!(tx.sample(212, burst, b""), None);
        assert_eq!(tx.sample(224, burst, b""), None);
        assert_eq!(tx.sample(225, None, b""), None);
        assert_eq!(tx.sample(237, None, b""), None);
        assert_eq!(tx.sample(238, burst, b""), None);
        assert_eq!(tx.sample(250, burst, b""), Some(BURST_TONE));
        assert_eq!(tx.sample(749, burst, b""), Some(BURST_TONE));
        assert_eq!(tx.sample(750, burst, b""), None);
        assert_eq!(tx.sample(1000, burst, b""), None);
        assert_eq!(tx.sample(1001, None, b""), None);
        assert_eq!(tx.sample(1013, None, b""), None);
        assert_eq!(tx.sample(1014, burst, b""), None);
        assert_eq!(tx.sample(1026, burst, b""), Some(BURST_TONE));
        assert_eq!(tx.sample(1027, None, b""), Some(BURST_TONE));
        assert_eq!(tx.sample(1526, None, b""), None);
        assert!(!tx.mute_mic());
    }

    #[test]
    fn uncertain_press_restarts_debounce_without_losing_the_armed_release() {
        let mut tx = TxTones::new();
        let up = Some(TxKey::Toneburst);
        tx.sample(0, None, b"");
        tx.sample(12, None, b"");
        tx.sample(13, up, b"");
        tx.interrupt_key_debounce();
        assert_eq!(tx.sample(40, up, b""), None);
        assert_eq!(tx.sample(51, up, b""), None);
        assert_eq!(tx.sample(52, up, b""), Some(BURST_TONE));
        assert_eq!(tx.sample(552, up, b""), None);
        tx.sample(553, None, b"");
        tx.interrupt_key_debounce();
        tx.sample(565, None, b"");
        tx.sample(566, up, b"");
        assert_eq!(tx.sample(578, up, b""), None);
    }

    #[test]
    fn memory_snapshot_survives_release_and_late_polls_across_wrap() {
        let mut tx = TxTones::new();
        let start = u32::MAX - 50;
        let mut sample = |offset: u32, key, memory: &[u8]| {
            let tone = tx.sample(start.wrapping_add(offset), key, memory);
            (tone, tx.mute_mic())
        };
        let memory = Some(TxKey::Memory);
        assert_eq!(sample(0, None, b"1#"), (None, false));
        assert_eq!(sample(12, None, b"1#"), (None, false));
        assert_eq!(sample(13, memory, b"1#"), (None, false));
        assert_eq!(sample(25, memory, b"1#"), (dtmf(b'1'), true));
        assert_eq!(sample(124, None, b"0"), (dtmf(b'1'), true));
        assert_eq!(sample(500, None, b"0"), (None, true));
        assert_eq!(sample(569, None, b"0"), (None, true));
        assert_eq!(sample(700, None, b"0"), (dtmf(b'#'), true));
        assert_eq!(sample(799, None, b"0"), (dtmf(b'#'), true));
        assert_eq!(sample(800, None, b"0"), (None, false));
    }

    #[test]
    fn another_key_cancels_a_memory_gap_before_debounce() {
        let mut tx = TxTones::new();
        let memory = Some(TxKey::Memory);
        let manual = Some(TxKey::Dtmf(b'*'));
        tx.sample(0, None, b"12");
        tx.sample(12, None, b"12");
        tx.sample(20, memory, b"12");
        assert_eq!(tx.sample(32, memory, b"12"), dtmf(b'1'));
        assert_eq!(tx.sample(132, None, b"12"), None);
        assert!(tx.mute_mic());
        assert_eq!(tx.sample(133, manual, b"12"), None);
        assert!(!tx.mute_mic());
        assert_eq!(tx.sample(145, manual, b"12"), dtmf(b'*'));
        assert_eq!(tx.sample(202, None, b"12"), None);
    }

    #[test]
    fn sliding_between_macro_keys_requires_a_neutral_matrix() {
        let mut tx = TxTones::new();
        let up = Some(TxKey::Toneburst);
        let down = Some(TxKey::Memory);
        tx.sample(0, None, b"12");
        tx.sample(12, None, b"12");
        tx.sample(13, up, b"12");
        assert_eq!(tx.sample(25, up, b"12"), Some(BURST_TONE));
        assert_eq!(tx.sample(26, down, b"12"), None);
        assert_eq!(tx.sample(38, down, b"12"), None);
        tx.sample(50, None, b"12");
        tx.sample(70, down, b"12");
        assert_eq!(tx.sample(82, down, b"12"), None);
        tx.sample(83, None, b"12");
        tx.sample(95, None, b"12");
        tx.sample(96, down, b"12");
        assert_eq!(tx.sample(108, down, b"12"), dtmf(b'1'));
    }

    #[test]
    fn cancellation_blocks_the_key_held_through_modem_ownership() {
        let mut tx = TxTones::new();
        let two = Some(TxKey::Dtmf(b'2'));
        tx.sample(0, two, b"");
        assert_eq!(tx.sample(12, two, b""), dtmf(b'2'));
        tx.cancel();
        assert!(!tx.mute_mic());
        assert_eq!(tx.sample(30, two, b""), None);
        assert_eq!(tx.sample(80, two, b""), None);
        assert_eq!(tx.sample(81, None, b""), None);
        assert_eq!(tx.sample(93, None, b""), None);
        assert_eq!(tx.sample(94, two, b""), None);
        assert_eq!(tx.sample(106, two, b""), dtmf(b'2'));
    }

    #[test]
    fn memory_rejects_invalid_symbols_and_lengths() {
        for sequence in [&b""[..], b"1A3", b"0123456789*#1"] {
            let mut tx = TxTones::new();
            let memory = Some(TxKey::Memory);
            tx.sample(0, None, sequence);
            tx.sample(12, None, sequence);
            tx.sample(13, memory, sequence);
            assert_eq!(tx.sample(25, memory, sequence), None);
            assert!(!tx.mute_mic());
            assert_eq!(tx.sample(36, memory, b"12"), None);
        }
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
