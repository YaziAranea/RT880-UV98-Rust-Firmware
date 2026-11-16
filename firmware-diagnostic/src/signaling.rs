//! Foreground ownership of manual DTMF and the explicitly selected ANI tail.
//! Does not start RF. Every tone is inside the existing calibrated TX session.
use uv98_handheld_ui::controller::{Slot, View};
use uv98_radio_runtime::mdc1200::{self, Action, Config, Input, Mode};
use uv98_radio_runtime::{Fault, Io, Runtime, State};
use uv98_signaling::tones::{self, BkToneSession, FiveTone, Standard, ToneRegisters};

pub fn mdc_config(view: View, slot: Slot) -> Config {
    let settings = view.mdc_for_slot(slot);
    Config {
        mode: match settings.mode {
            1 => Mode::Begin,
            2 => Mode::End,
            3 => Mode::Both,
            _ => Mode::Off,
        },
        unit: settings.id.and_then(uv98_signaling::mdc1200::UnitId::new),
    }
}

pub fn keypad(raw: u16) -> Option<u8> {
    if raw.count_ones() != 1 {
        return None;
    }
    match uv98_keys::stock_matrix_code(raw) {
        digit @ 0..=9 => Some(b'0' + digit),
        14 => Some(b'*'),
        15 => Some(b'#'),
        _ => None,
    }
}

struct Registers<'a, I: Io>(&'a mut Runtime<I>);
impl<I: Io> ToneRegisters for Registers<'_, I> {
    type Error = Fault<I::Error>;
    fn read(&mut self, register: u8) -> Result<u16, Self::Error> {
        self.0.signaling_read(register)
    }
    fn write(&mut self, register: u8, value: u16) -> Result<(), Self::Error> {
        self.0.signaling_write(register, value)
    }
}

pub struct TransmitTones {
    generator: BkToneSession,
    tail: Option<FiveTone>,
    previous_voice: bool,
    key: Option<u8>,
    key_since: u32,
    mdc: mdc1200::Session,
    mdc_config: Option<Config>,
}
impl TransmitTones {
    pub const fn new() -> Self {
        Self {
            generator: BkToneSession::new(),
            tail: None,
            previous_voice: false,
            key: None,
            key_since: 0,
            mdc: mdc1200::Session::new(),
            mdc_config: None,
        }
    }
    pub fn tail_active(&self) -> bool {
        self.tail.as_ref().is_some_and(FiveTone::active)
    }

    /// allow_tail requires clean physical release, unchanged TX configuration,
    /// and remaining ordinary TOT. Any new key/PTT or context change cancels it.
    pub fn step<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        now: u32,
        voice_held: bool,
        allow_tail: bool,
        key: Option<u8>,
        tail_kind: u8,
        id: Option<u32>,
        mdc_config: Config,
    ) -> Result<bool, Fault<I::Error>> {
        if radio.state() != State::Transmitting {
            self.generator = BkToneSession::new();
            self.tail = None;
            self.previous_voice = false;
            self.mdc = mdc1200::Session::new();
            self.mdc_config = None;
            return Ok(voice_held);
        }
        let latched = *self.mdc_config.get_or_insert(mdc_config);
        let input = if voice_held {
            Input::Voice
        } else if allow_tail {
            Input::Released
        } else {
            Input::Abort
        };
        match self.mdc.step(now, input, latched, radio.mdc_progress()) {
            Action::Start(id) => {
                // DTMF and FFSK share Tone2. Stop an active keypad tone in
                // one verified write before the end burst takes ownership.
                // Terminal TX cleanup restores the shared modulation state.
                if self.generator.active() {
                    radio.signaling_write(0x70, 0)?;
                }
                self.tail = None;
                self.previous_voice = voice_held;
                radio.start_mdc(id)?;
                return Ok(true);
            }
            Action::Keep => {
                radio.step_mdc(now)?;
                return Ok(true);
            }
            Action::RestoreVoice => {
                radio.resume_after_mdc()?;
                return Ok(true);
            }
            Action::StopTx => {
                self.tail = None;
                return Ok(false);
            }
            Action::Idle => {}
        }
        let legacy_tail = latched.mode == Mode::Off || latched.unit.is_none();
        if self.previous_voice && !voice_held && allow_tail && legacy_tail {
            let standard = match tail_kind {
                1 => Some(Standard::Zvei1),
                2 => Some(Standard::Ccir),
                _ => None,
            };
            self.tail = standard
                .zip(id)
                .and_then(|(s, id)| FiveTone::from_number(s, id));
            if let Some(tail) = &mut self.tail {
                tail.start(now);
            }
        }
        if !allow_tail {
            self.tail = None;
        }
        self.previous_voice = voice_held;
        if self.key != key {
            self.key = key;
            self.key_since = now;
        }
        let tone = if voice_held {
            key.filter(|_| now.wrapping_sub(self.key_since) >= 12)
                .and_then(tones::dtmf)
        } else {
            self.tail.as_mut().and_then(|tail| tail.tone(now))
        };
        if let Some(tone) = tone {
            if !self.generator.active() {
                radio.signaling_microphone(false)?;
            }
            self.generator.set(&mut Registers(radio), tone)?;
        } else if self.generator.active() {
            self.generator.stop(&mut Registers(radio))?;
            radio.signaling_microphone(true)?;
        }
        Ok(voice_held || self.tail_active())
    }
}
