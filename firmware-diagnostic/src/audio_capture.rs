//! Manual, passive PA5 experiment. This module is excluded from TX images.
use uv98_boot_log::{hardware::TxLogger, AudioExportBudget, Frame};
use uv98_handheld_ui::render::AudioCaptureView;
use uv98_keys::Key;
use uv98_sidekeys_at32::{
    hardware::{RxAudioInput, SideAdc},
    CaptureStats, CaptureTiming, Error, PollTiming, RX_AUDIO_HZ, RX_AUDIO_SAMPLES,
};

pub struct Capture {
    input: RxAudioInput,
    samples: [u16; RX_AUDIO_SAMPLES],
    timing: Option<CaptureTiming>,
    view: AudioCaptureView,
    export: Option<(u16, AudioExportBudget)>,
    checksum: u32,
}

impl Capture {
    pub fn new(input: RxAudioInput) -> Self {
        Self {
            input,
            samples: [0; RX_AUDIO_SAMPLES],
            timing: None,
            view: AudioCaptureView::default(),
            export: None,
            checksum: 0,
        }
    }
    pub fn view(&self) -> AudioCaptureView {
        self.view
    }

    /// Returns true after an acquisition attempt so main resamples time/keys.
    pub fn key<T: PollTiming>(
        &mut self,
        key: Option<Key>,
        allowed: bool,
        adc: &mut SideAdc<T>,
    ) -> Result<bool, Error> {
        if !allowed || self.export.is_some() {
            return Ok(false);
        }
        match key {
            Some(Key::Digit5) => {
                self.timing = None;
                self.view = AudioCaptureView {
                    sequence: self.view.sequence.wrapping_add(1),
                    ..AudioCaptureView::default()
                };
                match adc.capture_rx_audio(
                    &mut self.input,
                    &mut self.samples,
                    cortex_m::peripheral::DWT::cycle_count,
                ) {
                    Ok(timing) => {
                        let stats = CaptureStats::from_samples(&self.samples)?;
                        self.checksum = pcm_crc32(&self.samples);
                        self.view = AudioCaptureView {
                            sequence: self.view.sequence,
                            samples: RX_AUDIO_SAMPLES as u16,
                            minimum: stats.min,
                            maximum: stats.max,
                            dc: (stats.sum / RX_AUDIO_SAMPLES as u32) as u16,
                            low_clips: stats.clip_low,
                            high_clips: stats.clip_high,
                            late_cycles: timing.max_late_cycles,
                            status: 1,
                            export: 0,
                        };
                        self.timing = Some(timing);
                    }
                    Err(Error::Timing) => self.view.status = 2,
                    Err(error) => {
                        self.view.status = 3;
                        return Err(error);
                    }
                }
                Ok(true)
            }
            Some(Key::Hash) if self.timing.is_some() => {
                self.export = Some((0, AudioExportBudget::default()));
                self.view.export = 1;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// One bounded UART frame per foreground pass. Never writes to the RF chip.
    pub fn export_step(&mut self, log: &mut TxLogger) {
        let Some((index, budget)) = self.export.as_mut() else {
            return;
        };
        let Some(timing) = self.timing else {
            self.export = None;
            return;
        };
        let frame = match *index {
            0 => Frame::new(
                0xA510_0000,
                [self.view.sequence, RX_AUDIO_HZ, RX_AUDIO_SAMPLES as u32],
            ),
            1 => Frame::new(
                0xA510_0001,
                [
                    timing.first_trigger_cycle,
                    timing.last_trigger_cycle,
                    timing.max_late_cycles,
                ],
            ),
            2 => Frame::new(
                0xA510_0002,
                [
                    timing.min_interval_cycles,
                    timing.max_interval_cycles,
                    48_000_000,
                ],
            ),
            3..=344 => {
                let chunk = usize::from(*index - 3);
                let mut words = [0u32; 3];
                for (word_index, word) in words.iter_mut().enumerate() {
                    let sample = chunk * 6 + word_index * 2;
                    *word = u32::from(self.samples.get(sample).copied().unwrap_or(0))
                        | (u32::from(self.samples.get(sample + 1).copied().unwrap_or(0)) << 16);
                }
                Frame::new(0xA520_0000 | chunk as u32, words)
            }
            _ => Frame::new(
                0xA51F_0000,
                [self.view.sequence, self.checksum, RX_AUDIO_SAMPLES as u32],
            ),
        };
        if log.emit_audio(frame, budget).is_err() {
            self.export = None;
            self.view.export = 3;
        } else if *index == 345 {
            self.export = None;
            self.view.export = 2;
        } else {
            *index += 1;
        }
    }
}

fn pcm_crc32(samples: &[u16; RX_AUDIO_SAMPLES]) -> u32 {
    let mut crc = !0u32;
    for byte in samples.iter().flat_map(|value| value.to_le_bytes()) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
