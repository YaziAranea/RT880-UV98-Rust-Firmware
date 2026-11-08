//! Host adapter only. RF samples and acknowledgments are deliberately synthetic.
//! No serial, PAC, file-write, calibration or transmitter backend is linked here.
use uv98_codeplug::{channels, Codeplug, CHANNELS_LEN};
use uv98_keys::{ButtonEvent, Key};
use uv98_spectrum::{Action, Engine, Event, Mode, PeakHold, Range, Timing};
use uv98_ui_core::{
    ChannelCatalog, CodeplugChannels, Controller, Direction, Effect, NavigationPolicy,
};

pub struct Simulator {
    channels: Vec<u8>,
    pub ui: Controller,
    pub spectrum: Engine,
    pub cursor: usize,
    pub now_ms: u32,
    pending: Option<Event>,
    pub inject_fault: bool,
    pub last_error: Option<String>,
    pub source: String,
}

impl Simulator {
    pub fn demo() -> Self {
        let mut bytes = vec![0xff; CHANNELS_LEN];
        for (i, (name, frequency)) in [
            (b"UHF DEMO".as_slice(), 43_350_000u32),
            (b"VHF DEMO", 14_550_000),
            (b"CALL DEMO", 43_350_000),
        ]
        .into_iter()
        .enumerate()
        {
            let r = &mut bytes[i * 48..(i + 1) * 48];
            r[2] = 1;
            r[6..10].copy_from_slice(&frequency.to_le_bytes());
            r[10..14].copy_from_slice(&frequency.to_le_bytes());
            r[32..32 + name.len()].copy_from_slice(name);
        }
        Self::from_channels(bytes, "Встроенные демонстрационные каналы".into()).unwrap()
    }
    pub fn from_codeplug(bytes: &[u8], source: String) -> Result<Self, String> {
        let parsed = Codeplug::parse(bytes).map_err(|e| e.to_string())?;
        Self::from_channels(parsed.channels().to_vec(), source)
    }
    fn from_channels(channels: Vec<u8>, source: String) -> Result<Self, String> {
        let catalog = CodeplugChannels::new(&channels).map_err(|e| format!("{e:?}"))?;
        let initial = (0..catalog.len())
            .find(|&i| catalog.is_active(i) == Some(true))
            .ok_or("Нет активных каналов")?;
        let ui = Controller::new(
            initial,
            NavigationPolicy {
                wrap: true,
                include_inactive: false,
            },
            &catalog,
        )
        .map_err(|e| format!("{e:?}"))?;
        Ok(Self {
            channels,
            ui,
            spectrum: Self::engine(43_350_000)?,
            cursor: 80,
            now_ms: 0,
            pending: None,
            inject_fault: false,
            last_error: None,
            source,
        })
    }
    fn engine(center: u32) -> Result<Engine, String> {
        let start = center
            .checked_sub(100_000)
            .filter(|x| *x != 0)
            .ok_or("Частота не подходит для демонстрационного обзора +/- 1 МГц")?;
        let end = center
            .checked_add(100_000)
            .ok_or("Переполнение диапазона")?;
        let range = Range::new(start, end, 1250).map_err(|e| format!("{e:?}"))?;
        Ok(Engine::new(
            range,
            Timing::new(1000, 1000, 8, 1000).unwrap(),
            Mode::Continuous,
            PeakHold::On,
        ))
    }
    pub fn channel(&self) -> channels::Channel<'_> {
        channels::channel(&self.channels, self.ui.view().channel_index)
            .expect("controller validates the immutable catalog")
    }
    pub fn key(&mut self, key: Key, event: ButtonEvent) {
        if key == Key::Ptt {
            self.feed(Event::Ptt(event != ButtonEvent::Released));
        }
        let catalog = CodeplugChannels::new(&self.channels).unwrap();
        match self.ui.on_key(key, event, &catalog) {
            Ok(t) => {
                self.last_error = None;
                self.effect(t.effect);
            }
            Err(e) => self.last_error = Some(format!("{e:?}")),
        }
    }
    pub fn action(&mut self, action: uv98_ui_core::Action) {
        if let uv98_ui_core::Action::Ptt(value) = action {
            self.feed(Event::Ptt(value));
        }
        let catalog = CodeplugChannels::new(&self.channels).unwrap();
        match self.ui.dispatch(action, &catalog) {
            Ok(t) => {
                self.last_error = None;
                self.effect(t.effect);
            }
            Err(e) => self.last_error = Some(format!("{e:?}")),
        }
    }
    fn effect(&mut self, effect: Option<Effect>) {
        match effect {
            Some(Effect::OpenSpectrum) => {
                // This host has no asynchronous external callbacks. Clear its
                // single synthetic event before replacing the engine lifetime.
                self.pending = None;
                match Self::engine(self.channel().rx_frequency_code()) {
                    Ok(engine) => {
                        self.spectrum = engine;
                        self.cursor = 80;
                        self.feed(Event::Start);
                    }
                    Err(error) => {
                        self.last_error = Some(error);
                        let catalog = CodeplugChannels::new(&self.channels).unwrap();
                        let _ = self
                            .ui
                            .dispatch(uv98_ui_core::Action::CloseSpectrum, &catalog);
                    }
                }
            }
            Some(Effect::CloseSpectrum) => {
                self.feed(Event::Stop);
                self.pending = None;
            }
            Some(Effect::MoveSpectrumCursor { direction }) => {
                self.cursor = match direction {
                    Direction::Previous => self.cursor.saturating_sub(1),
                    Direction::Next => (self.cursor + 1).min(self.spectrum.range().len() - 1),
                };
            }
            Some(Effect::ResetPeaks) => self.feed(Event::ResetPeaks),
            Some(Effect::SelectChannel { .. }) | None => {}
        }
    }
    pub fn toggle_sweep(&mut self) {
        if !self.ui.view().spectrum_open {
            return;
        }
        if self.spectrum.is_running() {
            self.feed(Event::Stop);
            self.pending = None;
        } else {
            self.feed(Event::Start);
        }
    }
    fn feed(&mut self, event: Event) {
        if let Some(action) = self.spectrum.handle(self.now_ms, event) {
            self.pending = match action {
                Action::PrepareReceive { ticket } => Some(Event::Ready(ticket)),
                Action::TuneReceive { ticket, .. } => Some(Event::Settled(ticket)),
                Action::ReadSignal { ticket, index } => {
                    if self.inject_fault {
                        self.inject_fault = false;
                        Some(Event::Failed { ticket, code: 1 })
                    } else {
                        Some(Event::Sample {
                            ticket,
                            raw: synthetic_signal(index, self.spectrum.sweep()),
                        })
                    }
                }
                Action::SweepComplete { .. } | Action::Cancel { .. } => None,
            };
        }
    }
    pub fn step(&mut self) {
        self.now_ms = self.now_ms.wrapping_add(4);
        let event = self.pending.take().unwrap_or(Event::Poll);
        self.feed(event);
    }
    pub fn frame(&self) -> Vec<u16> {
        let channel = self.channel();
        let model = uv98_radio_ui::Model {
            view: self.ui.view(),
            channel: Some(uv98_radio_ui::ChannelView {
                name: channel.name_bytes(),
                rx_10hz: channel.rx_frequency_code(),
                tx_10hz: channel.tx_frequency_code(),
            }),
            spectrum: Some(uv98_radio_ui::SpectrumView {
                engine: &self.spectrum,
                cursor: self.cursor,
                scale_max: 1024,
            }),
            demo: true,
        };
        let mut pixels = vec![0; 320 * 240];
        for (y, row) in pixels.chunks_exact_mut(320).enumerate() {
            uv98_radio_ui::render_row(y as u16, row.try_into().unwrap(), &model)
                .expect("bounded display model");
        }
        pixels
    }
}

/// A repeatable invented RF scene, unrelated to any antenna or receiver data.
fn synthetic_signal(index: usize, sweep: u64) -> u16 {
    let floor = 65 + ((index as u64 * 17 + sweep * 11) % 35) as u16;
    let mut signal = floor;
    for (center, width, height) in [(30usize, 9usize, 650u16), (80, 5, 850), (128, 13, 440)] {
        let distance = index.abs_diff(center);
        if distance < width {
            signal += height * (width - distance) as u16 / width as u16;
        }
    }
    signal
}

pub fn rgb(pixels: &[u16]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|v| {
            [
                ((v >> 11) as u32 * 255 / 31) as u8,
                (((v >> 5) & 63) as u32 * 255 / 63) as u8,
                ((v & 31) as u32 * 255 / 31) as u8,
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uv98_ui_core::{Action as UiAction, Screen};
    #[test]
    fn menu_keys_sweep_cursor_ptt_and_release_use_shared_cores() {
        let mut s = Simulator::demo();
        s.key(Key::Down, ButtonEvent::Pressed);
        assert_eq!(s.ui.view().channel_index, 1);
        s.key(Key::Menu, ButtonEvent::Pressed);
        s.key(Key::Menu, ButtonEvent::Pressed);
        assert_eq!(s.ui.view().screen, Screen::Spectrum);
        for _ in 0..1000 {
            s.step();
        }
        assert!(s.spectrum.bins().iter().all(|b| b.latest.is_some()));
        s.key(Key::Down, ButtonEvent::Pressed);
        assert_eq!(s.cursor, 81);
        s.key(Key::Ptt, ButtonEvent::Pressed);
        assert!(!s.spectrum.is_running());
        assert_eq!(s.ui.view().screen, Screen::Channel);
        s.key(Key::Ptt, ButtonEvent::Released);
        for _ in 0..1000 {
            s.step();
        }
        assert!(!s.spectrum.is_running());
        s.action(UiAction::OpenSpectrum);
        s.inject_fault = true;
        for _ in 0..15 {
            s.step();
        }
        assert_eq!(
            s.spectrum.state(),
            uv98_spectrum::State::Stopped(uv98_spectrum::CancelReason::Hardware { code: 1 })
        );
    }
    #[test]
    fn invalid_file_does_not_replace_working_session_and_renderer_is_bounded() {
        assert!(Simulator::from_codeplug(&[0; 10], "bad".into()).is_err());
        let mut s = Simulator::demo();
        let channel = s.frame();
        s.action(UiAction::OpenSpectrum);
        for _ in 0..700 {
            s.step();
        }
        let spectrum = s.frame();
        assert_eq!(channel.len(), 320 * 240);
        assert_eq!(spectrum.len(), channel.len());
        assert_ne!(channel, spectrum);
    }
}
