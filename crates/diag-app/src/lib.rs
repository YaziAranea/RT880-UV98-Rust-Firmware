#![no_std]
#![forbid(unsafe_code)]
//! Foreground diagnostic application, shared by the real image and host preview.
//! It only sends NOR READ03 transactions and display traffic. PTT is an input
//! label; no RF or flash writer exists here. Hardware startup remains the caller's
//! responsibility. Accepted samples and a real monotonic clock must be supplied.

use core::fmt::{self, Write};
use sha2::{Digest, Sha256};
use uv98_calibration::{Table, BAND_COUNT};
use uv98_diagnostic::{render_text_lines, Diagnostic, LoadError, Snapshot, TextLine};
use uv98_display::{Delay, Display, DisplayBus};
use uv98_keys::{ClockError, Key, Timing};
use uv98_keyscan_at32::{classify_matrix, MatrixState};
use uv98_nor_read::Bus;

/// SHA256 of the user's preserved stock 1.16 calibration, not embedded defaults.
pub const PRESERVED_CALIBRATION_SHA256: [u8; 32] = [
    0x89, 0x06, 0xc3, 0xd9, 0x84, 0x7e, 0xee, 0xef, 0xe0, 0xee, 0x6a, 0xad, 0x6a, 0xa4, 0x30, 0x29,
    0x49, 0xbd, 0x5e, 0x59, 0x84, 0xf3, 0xc6, 0x4e, 0xd0, 0x2e, 0xdb, 0x8c, 0xf6, 0xfb, 0x0f, 0xf7,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    New,
    Ready,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Overview,
    Calibration,
    Keys,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputState {
    WaitingForRelease,
    Ready,
    Unstable,
}

/// Ordered startup checkpoints. Observer receives no calibration/channel data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartStage {
    DisplayBegin,
    CalibrationBegin,
    ChannelBegin,
    RenderBegin,
    Complete,
}

/// Stable means accepted by the board scanner, not physically debounce-proven.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSample {
    Stable { matrix: u16, ptt_pressed: bool },
    Unstable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    pub page: Page,
    pub band: usize,
    pub calibration_sha256: Option<[u8; 32]>,
    pub calibration_matches_backup: Option<bool>,
    pub input_state: InputState,
    pub raw_sample: Option<(u16, bool)>,
    pub last_pressed: Option<Key>,
    pub presses: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error<N, L> {
    State,
    Display(uv98_display::Error<L>),
    Read {
        source: LoadError<N>,
        fault_display_error: Option<uv98_display::Error<L>>,
    },
    Clock {
        fault_display_error: Option<uv98_display::Error<L>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Update {
    pub rendered: bool,
    pub pressed: Option<Key>,
    pub channel_changed: bool,
}

/// Release-to-arm policy prevents boot-held keys and ambiguous/failed scans
/// from becoming navigation. A new press requires debounce_ms of matching
/// observations; every release or invalid sample requires a fresh clean release.
/// A repeated held key does not autorepeat. Timing::hold_ms is unused by design.
struct InputTracker {
    debounce_ms: u32,
    last_ms: Option<u32>,
    release_since: Option<u32>,
    candidate: Option<(Key, u32, bool)>,
    state: InputState,
}
impl InputTracker {
    const fn new(timing: Timing) -> Self {
        Self {
            debounce_ms: timing.debounce_ms(),
            last_ms: None,
            release_since: None,
            candidate: None,
            state: InputState::WaitingForRelease,
        }
    }
    fn update(&mut self, now_ms: u32, sample: InputSample) -> Result<Option<Key>, ClockError> {
        if self
            .last_ms
            .is_some_and(|previous| now_ms.wrapping_sub(previous) >= (1 << 31))
        {
            return Err(ClockError);
        }
        self.last_ms = Some(now_ms);
        let key = match sample {
            InputSample::Unstable => {
                self.invalidate(InputState::Unstable);
                return Ok(None);
            }
            InputSample::Stable {
                matrix,
                ptt_pressed,
            } => match (classify_matrix(matrix), ptt_pressed) {
                (MatrixState::Released, false) => None,
                (MatrixState::Released, true) => Some(Key::Ptt),
                (MatrixState::Single(key), false) => Some(key),
                _ => {
                    self.invalidate(InputState::Unstable);
                    return Ok(None);
                }
            },
        };
        if self.state != InputState::Ready {
            if key.is_some() {
                self.release_since = None;
            } else if let Some(since) = self.release_since {
                if now_ms.wrapping_sub(since) >= self.debounce_ms {
                    self.state = InputState::Ready;
                    self.release_since = None;
                }
            } else {
                self.release_since = Some(now_ms);
                self.state = InputState::WaitingForRelease;
            }
            return Ok(None);
        }
        match (self.candidate, key) {
            (None, Some(key)) => self.candidate = Some((key, now_ms, false)),
            (Some((candidate, _, _)), Some(key)) if candidate != key => {
                self.invalidate(InputState::WaitingForRelease);
            }
            (Some((candidate, since, false)), Some(_))
                if now_ms.wrapping_sub(since) >= self.debounce_ms =>
            {
                self.candidate = Some((candidate, since, true));
                return Ok(Some(candidate));
            }
            (Some(_), None) => {
                self.invalidate(InputState::WaitingForRelease);
                self.release_since = Some(now_ms);
            }
            _ => {}
        }
        Ok(None)
    }
    fn invalidate(&mut self, state: InputState) {
        self.state = state;
        self.release_since = None;
        self.candidate = None;
    }
}

pub struct App<N, L, D> {
    diagnostic: Diagnostic,
    nor: N,
    display: Display<L, D>,
    expected_sha256: [u8; 32],
    input: InputTracker,
    status: Status,
}
impl<N: Bus, L: DisplayBus, D: Delay> App<N, L, D> {
    /// Inert construction. Pin state, clocks, RF inhibition and bus setup are
    /// provided by the board, not inferred from possession of these adapters.
    pub fn new(nor: N, lcd: L, delay: D, timing: Timing, expected_sha256: [u8; 32]) -> Self {
        Self {
            diagnostic: Diagnostic::new(),
            nor,
            display: Display::new(lcd, delay),
            expected_sha256,
            input: InputTracker::new(timing),
            status: Status {
                state: State::New,
                page: Page::Overview,
                band: 0,
                calibration_sha256: None,
                calibration_matches_backup: None,
                input_state: InputState::WaitingForRelease,
                raw_sample: None,
                last_pressed: None,
                presses: 0,
            },
        }
    }
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn snapshot(&self) -> Option<Snapshot<'_>> {
        if self.status.state == State::Ready {
            self.diagnostic.snapshot()
        } else {
            None
        }
    }
    /// One initialization attempt. Display initializes before NOR reads so a
    /// read/layout failure can draw one terminal fault frame. If that also fails,
    /// both errors are retained. SHA mismatch is a visible diagnostic fact, never
    /// permission to replace the read bytes or apply RF calibration.
    pub fn start(&mut self, slot: usize) -> Result<(), Error<N::Error, L::Error>> {
        self.start_observed(slot, |_| {})
    }
    pub fn start_observed(
        &mut self,
        slot: usize,
        mut observe: impl FnMut(StartStage),
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::New {
            return Err(Error::State);
        }
        self.status.state = State::Failed;
        observe(StartStage::DisplayBegin);
        self.display.initialize().map_err(Error::Display)?;
        observe(StartStage::CalibrationBegin);
        if let Err(source) = self.diagnostic.load_calibration(&mut self.nor) {
            let fault_display_error = self.fault_frame(b"CALIBRATION READ / LAYOUT").err();
            return Err(Error::Read {
                source,
                fault_display_error,
            });
        }
        let calibration = self
            .diagnostic
            .calibration_view()
            .expect("successful calibration read");
        let digest: [u8; 32] = Sha256::digest(calibration.raw()).into();
        self.status.calibration_sha256 = Some(digest);
        self.status.calibration_matches_backup = Some(digest == self.expected_sha256);
        observe(StartStage::ChannelBegin);
        if let Err(source) = self.diagnostic.load_channel(&mut self.nor, slot) {
            let fault_display_error = self.fault_frame(b"CHANNEL READ / INDEX").err();
            return Err(Error::Read {
                source,
                fault_display_error,
            });
        }
        observe(StartStage::RenderBegin);
        self.render().map_err(Error::Display)?;
        self.status.state = State::Ready;
        observe(StartStage::Complete);
        Ok(())
    }
    /// Process one completed scan using a measured monotonic timestamp.
    /// Invalid/ambiguous scans cancel pending input; they are never fabricated
    /// releases. A clock discontinuity or storage/display failure is terminal.
    pub fn sample(
        &mut self,
        now_ms: u32,
        sample: InputSample,
    ) -> Result<Update, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let before = self.status;
        let pressed = match self.input.update(now_ms, sample) {
            Ok(pressed) => pressed,
            Err(_) => {
                self.status.state = State::Failed;
                self.status.raw_sample = None;
                self.status.input_state = InputState::Unstable;
                let fault_display_error = self.fault_frame(b"MONOTONIC CLOCK FAILURE").err();
                return Err(Error::Clock {
                    fault_display_error,
                });
            }
        };
        self.status.input_state = self.input.state;
        self.status.raw_sample = match sample {
            InputSample::Stable {
                matrix,
                ptt_pressed,
            } => Some((matrix, ptt_pressed)),
            InputSample::Unstable => None,
        };
        let mut channel_changed = false;
        if let Some(key) = pressed {
            self.status.last_pressed = Some(key);
            self.status.presses = self.status.presses.saturating_add(1);
            match key {
                Key::Menu => {
                    self.status.page = match self.status.page {
                        Page::Overview => Page::Calibration,
                        Page::Calibration => Page::Keys,
                        Page::Keys => Page::Overview,
                    }
                }
                Key::Exit => self.status.page = Page::Overview,
                Key::Up | Key::Down => match self.status.page {
                    Page::Overview => {
                        if let Some(slot) = self.diagnostic.adjacent_index(key == Key::Down) {
                            self.status.state = State::Failed;
                            if let Err(source) = self.diagnostic.load_channel(&mut self.nor, slot) {
                                let fault_display_error =
                                    self.fault_frame(b"CHANNEL READ FAILURE").err();
                                return Err(Error::Read {
                                    source,
                                    fault_display_error,
                                });
                            }
                            self.status.state = State::Ready;
                            channel_changed = true;
                        }
                    }
                    Page::Calibration => {
                        if key == Key::Up {
                            self.status.band = self.status.band.saturating_sub(1);
                        } else {
                            self.status.band = (self.status.band + 1).min(BAND_COUNT - 1);
                        }
                    }
                    Page::Keys => {}
                },
                // PTT and all digits/star/hash are labels only.
                _ => {}
            }
        }
        let rendered = self.status != before || channel_changed;
        if rendered {
            self.status.state = State::Failed;
            self.render().map_err(Error::Display)?;
            self.status.state = State::Ready;
        }
        Ok(Update {
            rendered,
            pressed,
            channel_changed,
        })
    }
    pub fn into_parts(self) -> (N, Display<L, D>) {
        (self.nor, self.display)
    }

    fn fault_frame(&mut self, reason: &[u8]) -> Result<(), uv98_display::Error<L::Error>> {
        render_text_lines(
            &mut self.display,
            &[
                line(8, 8, 2, b"UV98 DIAGNOSTIC FAULT"),
                line(8, 30, 1, b"STOPPED / NO AUTO RETRY"),
                line(8, 65, 1, reason),
                line(8, 95, 1, b"NO VALID CURRENT SNAPSHOT"),
                line(8, 120, 1, b"ORIGINAL STORAGE NOT WRITTEN"),
                line(8, 210, 1, b"POWER OFF TO END DIAGNOSTIC"),
            ],
        )
    }
    fn render(&mut self) -> Result<(), uv98_display::Error<L::Error>> {
        let view = self
            .diagnostic
            .snapshot()
            .expect("render after successful loads");
        let mut rows = [Text::new(); 12];
        rows[0].text("UV98 RUST DIAGNOSTIC");
        rows[1].text("READ ONLY / NO TRANSMIT COMMANDS");
        rows[2].text(if self.status.calibration_matches_backup == Some(true) {
            "CAL SHA256: BACKUP MATCH"
        } else {
            "CAL SHA256: BACKUP MISMATCH"
        });
        let general = view.calibration.general();
        match self.status.page {
            Page::Overview => {
                let _ = write!(
                    rows[3],
                    "ADC CODE:{}  VOX ON:{} OFF:{}",
                    general.adc_correction_code(),
                    general.vox_on_raw(),
                    general.vox_off_raw()
                );
                let _ = write!(
                    rows[4],
                    "SLOT {:04} / {}",
                    view.index() + 1,
                    if view.channel.is_active() {
                        "ACTIVE"
                    } else {
                        "INACTIVE"
                    }
                );
                for &byte in view.channel.name_bytes() {
                    let _ = rows[5].write_char(if byte.is_ascii_graphic() || byte == b' ' {
                        byte.to_ascii_uppercase() as char
                    } else {
                        '?'
                    });
                }
                if view.channel.is_active() {
                    let frequency = view.channel.rx_frequency_code();
                    let _ = write!(
                        rows[6],
                        "RX {:03}.{:05} MHZ",
                        frequency / 100000,
                        frequency % 100000
                    );
                } else {
                    rows[6].text("NO ACTIVE CHANNEL");
                }
            }
            Page::Calibration => {
                let band = view
                    .calibration
                    .band(self.status.band)
                    .expect("bounded band index");
                let _ = write!(
                    rows[3],
                    "ADC:{} REG3E:{:04X} MARKER:9A",
                    general.adc_correction_code(),
                    general.reg_3e_raw()
                );
                let _ = write!(
                    rows[4],
                    "BAND {:02}/15 / {} POINTS",
                    self.status.band + 1,
                    band.valid_points()
                );
                let _ = write!(
                    rows[5],
                    "FREQ OFFSET CODE: {}",
                    band.frequency_offset_code()
                );
                let _ = write!(
                    rows[6],
                    "PWR HI/LO[0]: {} / {}",
                    band.table_raw(Table::PowerHigh)[0],
                    band.table_raw(Table::PowerLow)[0]
                );
            }
            Page::Keys => {
                rows[3].text("PRESS ONE MATRIX KEY OR PTT");
                rows[4].text("BOOT-HELD KEYS REQUIRE RELEASE");
                rows[5].text("AMBIGUOUS SCANS CANCEL INPUT");
                rows[6].text("PTT IS A TEST INPUT ONLY");
            }
        }
        if let Some((matrix, ptt)) = self.status.raw_sample {
            let _ = write!(
                rows[7],
                "MATRIX {:04X} / PTT {}",
                matrix,
                if ptt { "DOWN" } else { "UP" }
            );
        } else {
            rows[7].text("MATRIX / PTT: NO VALID SAMPLE");
        }
        let _ = write!(
            rows[8],
            "LAST {} / PRESSES {}",
            self.status.last_pressed.map(key_name).unwrap_or("NONE"),
            self.status.presses
        );
        rows[9].text(match self.status.input_state {
            InputState::Ready => "INPUT READY",
            InputState::WaitingForRelease => "INPUT WAITING FOR CLEAN RELEASE",
            InputState::Unstable => "INPUT UNSTABLE / RELEASE KEYS",
        });
        rows[10].text(match self.status.page {
            Page::Overview => "UP/DOWN SLOT / MENU CALIBRATION",
            Page::Calibration => "RAW CODES / UP/DOWN BAND / MENU KEYS",
            Page::Keys => "MENU OR EXIT: OVERVIEW",
        });
        rows[11].text("ADC SIDE KEYS / ENCODER: NOT SAMPLED");
        let positions = [
            (8, 8, 2),
            (8, 30, 1),
            (8, 52, 1),
            (8, 70, 1),
            (8, 88, 1),
            (8, 106, 1),
            (8, 124, 1),
            (8, 146, 1),
            (8, 164, 1),
            (8, 182, 1),
            (8, 210, 1),
            (8, 226, 1),
        ];
        let mut lines = [line(0, 0, 0, b""); 12];
        for ((target, &(x, y, scale)), text) in lines.iter_mut().zip(&positions).zip(&rows) {
            *target = line(x, y, scale, text.bytes());
        }
        render_text_lines(&mut self.display, &lines)
    }
}

const fn line(x: u16, y: u16, scale: u16, text: &[u8]) -> TextLine<'_> {
    TextLine { x, y, scale, text }
}
pub const fn key_name(key: Key) -> &'static str {
    match key {
        Key::Digit0 => "0",
        Key::Digit1 => "1",
        Key::Digit2 => "2",
        Key::Digit3 => "3",
        Key::Digit4 => "4",
        Key::Digit5 => "5",
        Key::Digit6 => "6",
        Key::Digit7 => "7",
        Key::Digit8 => "8",
        Key::Digit9 => "9",
        Key::Menu => "MENU",
        Key::Up => "UP",
        Key::Down => "DOWN",
        Key::Exit => "EXIT",
        Key::Star => "STAR",
        Key::Hash => "HASH",
        Key::Ptt => "PTT",
        Key::Aux16 => "AUX16",
        Key::Aux17 => "AUX17",
        Key::Aux18 => "AUX18",
    }
}
#[derive(Clone, Copy)]
struct Text {
    bytes: [u8; 48],
    len: usize,
}
impl Text {
    const fn new() -> Self {
        Self {
            bytes: [0; 48],
            len: 0,
        }
    }
    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    fn text(&mut self, text: &str) {
        let _ = self.write_str(text);
    }
}
impl Write for Text {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.len + text.len() > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + text.len()].copy_from_slice(text.as_bytes());
        self.len += text.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
