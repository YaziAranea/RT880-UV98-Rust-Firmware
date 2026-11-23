//! RX-only PA5 laboratory screen, compiled out of the ordinary firmware.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AudioCaptureStatus {
    #[default]
    Ready,
    Captured,
    TimingRejected,
    RxClosed,
    QuietOpen,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioCaptureView {
    pub sequence: u32,
    pub samples: u16,
    pub minimum: u16,
    pub maximum: u16,
    pub dc: u16,
    pub low_clips: u16,
    pub high_clips: u16,
    pub late_cycles: u32,
    pub status: AudioCaptureStatus,
    pub quiet: bool,
    /// 0 not exported, 1 sending, 2 sent, 3 UART failed.
    pub export: u8,
}

pub fn render_audio_rows<B: DisplayBus, D: Delay>(
    display: &mut Display<B, D>,
    view: AudioCaptureView,
    language: Language,
    start: u16,
    end: u16,
) -> Result<(), RenderError<B::Error>> {
    if start > end || end > HEIGHT {
        return Err(RenderError::Model(Error::InvalidRange));
    }
    if start == end {
        return Ok(());
    }
    display
        .begin_window(0, start, WIDTH as u16, end - start)
        .map_err(RenderError::Display)?;
    let mut version = Text::new();
    let _ = write!(version, "{} / ADC IN5 / DAC2 OFF", VERSION);
    let mut pixels = [0; WIDTH];
    for y in start..end {
        let out = &mut Row {
            pixels: &mut pixels,
            language,
        };
        out.fill(color::BACKGROUND);
        centered(y, out, 12, 2, "PA5 AUDIO / RX ONLY", color::ACCENT);
        centered(y, out, 44, 1, version.as_str(), color::MUTED);
        centered(y, out, 64, 1, "8000 Hz / 2048 / 256 ms", color::TEXT);
        let mut value = Text::new();
        let _ = write!(value, "#{}  N={}", view.sequence, view.samples);
        text(y, out, 12, 94, 2, value.as_str(), color::TEXT);
        let mut value = Text::new();
        let _ = write!(value, "MIN {}  MAX {}", view.minimum, view.maximum);
        text(y, out, 12, 124, 1, value.as_str(), color::TEXT);
        let mut value = Text::new();
        let _ = write!(
            value,
            "DC {}  P-P {}",
            view.dc,
            view.maximum.saturating_sub(view.minimum)
        );
        text(y, out, 12, 145, 1, value.as_str(), color::TEXT);
        let mut value = Text::new();
        let _ = write!(value, "CLIP 0:{}  4095:{}", view.low_clips, view.high_clips);
        text(y, out, 12, 166, 1, value.as_str(), color::TEXT);
        let mut value = Text::new();
        let _ = write!(value, "LATE {} cycles", view.late_cycles);
        text(y, out, 12, 187, 1, value.as_str(), color::TEXT);
        use AudioCaptureStatus::*;
        centered(
            y,
            out,
            213,
            1,
            match view.status {
                Ready => "READY",
                Captured if view.quiet => "QUIET CONTROL CAPTURED",
                Captured => "OPEN RX CAPTURED",
                TimingRejected => "TIMING ERROR / RETRY",
                RxClosed => "RX AUDIO CLOSED / USE MONITOR",
                QuietOpen => "AUDIO OPEN / CLOSE SQL FOR QUIET",
            },
            if matches!(view.status, Ready | Captured) {
                color::ACCENT
            } else {
                color::BAD
            },
        );
        centered(
            y,
            out,
            235,
            1,
            match view.export {
                0 if view.status == Captured => "UART: PRESS # TO EXPORT",
                0 => "UART: NO BLOCK",
                1 => "UART: SENDING",
                2 => "UART: SENT / CHECK PC",
                _ => "UART: FAILED / RETRY #",
            },
            color::MUTED,
        );
        centered(
            y,
            out,
            269,
            1,
            "5: OPEN RX   0: QUIET   #: UART",
            color::ACCENT,
        );
        centered(y, out, 290, 1, "EXIT: BACK / NO AUTO TX", color::TEXT);
        display
            .write_pixels(&pixels)
            .map_err(RenderError::Display)?;
    }
    display.finish_pixels().map_err(RenderError::Display)
}
