//! Row renderer for a new channel/menu/activity-spectrum UI.
//!
//! No heap, full framebuffer, hardware access, or measurement generation.
//! Colors are logical RGB565: physical panel RGB/BGR order is not qualified by
//! host rendering. Raw signal words are never converted to dBm or S-units.

#![no_std]
#![forbid(unsafe_code)]

use core::fmt::{self, Write};
use uv98_spectrum::{Engine, State};
use uv98_ui_core::{MenuItem, Screen, View};

mod font;

pub const WIDTH: usize = 320;
pub const HEIGHT: u16 = 240;
pub const CHART_X: usize = 40;
pub const CHART_WIDTH: usize = 240;
pub const CHART_TOP: u16 = 74;
pub const CHART_BOTTOM: u16 = 186;

pub mod color {
    pub const BACKGROUND: u16 = 0x0863;
    pub const PANEL: u16 = 0x10c6;
    pub const GRID: u16 = 0x2149;
    pub const TEXT: u16 = 0xef7d;
    pub const MUTED: u16 = 0x8c51;
    pub const CURRENT: u16 = 0x3dfb;
    pub const PREVIOUS: u16 = 0x29ad;
    pub const PEAK: u16 = 0xfde0;
    pub const CURSOR: u16 = 0xffff;
    pub const CHART: u16 = 0x0022;
}

#[derive(Clone, Copy)]
pub struct ChannelView<'a> {
    /// ASCII preview of at most 16 raw name bytes. NUL/FF ends the preview;
    /// unsupported bytes become '?'. The original record is never changed.
    pub name: &'a [u8],
    pub rx_10hz: u32,
    pub tx_10hz: u32,
}

#[derive(Clone, Copy)]
pub struct SpectrumView<'a> {
    pub engine: &'a Engine,
    pub cursor: usize,
    /// Explicit display scale. Values above it clip visually, while numerical
    /// readout remains exact. This is not a calibrated signal measurement scale.
    pub scale_max: u16,
}

#[derive(Clone, Copy)]
pub struct Model<'a> {
    pub view: View,
    pub channel: Option<ChannelView<'a>>,
    pub spectrum: Option<SpectrumView<'a>>,
    /// Must be true when measurements or acknowledgments are simulated.
    pub demo: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    RowOutOfRange { row: u16 },
    InvalidView,
    ZeroScale,
    CursorOutOfRange { cursor: usize, bins: usize },
}

#[derive(Debug, PartialEq, Eq)]
pub enum RenderError<E> {
    Model(Error),
    Display(uv98_display::Error<E>),
}

/// Stream a frame through an already initialized, exclusively owned Display.
/// This does not initialize pins or the display. Model validation precedes any
/// bus call. The one row is 640 bytes; Display additionally stages 64 bytes.
/// Transfer errors stop immediately and retain the Display driver's invalidation
/// semantics; no retry or physical-state recovery is invented here.
pub fn render<B: uv98_display::DisplayBus, D: uv98_display::Delay>(
    display: &mut uv98_display::Display<B, D>,
    model: &Model<'_>,
) -> Result<(), RenderError<B::Error>> {
    validate(0, model).map_err(RenderError::Model)?;
    display
        .begin_window(0, 0, WIDTH as u16, HEIGHT)
        .map_err(RenderError::Display)?;
    let mut row = [0; WIDTH];
    for y in 0..HEIGHT {
        render_row(y, &mut row, model).map_err(RenderError::Model)?;
        display.write_pixels(&row).map_err(RenderError::Display)?;
    }
    display.finish_pixels().map_err(RenderError::Display)
}

/// Render one complete logical RGB565 row. Validation errors preserve `out`.
/// The caller can stream 240 rows with one reusable 640-byte row buffer.
pub fn render_row(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) -> Result<(), Error> {
    validate(row, model)?;
    out.fill(color::BACKGROUND);
    rect(row, out, 0, 0, WIDTH, 28, color::PANEL);
    line(row, out, 28, 0, WIDTH, color::GRID);
    text(row, out, 12, 7, 2, b"UV98", color::TEXT);
    text(
        row,
        out,
        78,
        12,
        1,
        match model.view.screen {
            Screen::Channel => b"CHANNEL",
            Screen::Menu => b"MENU",
            Screen::Spectrum => b"SPECTRUM",
        },
        color::CURRENT,
    );
    if model.view.ptt_pressed {
        text(row, out, 224, 12, 1, b"PTT", color::PEAK);
    }
    if model.demo {
        rect(row, out, 260, 7, 48, 15, color::PEAK);
        text(row, out, 272, 11, 1, b"DEMO", color::BACKGROUND);
    }
    match model.view.screen {
        Screen::Channel => channel_page(row, out, model),
        Screen::Menu => menu_page(row, out, model),
        Screen::Spectrum => spectrum_page(row, out, model),
    }
    rect(row, out, 0, 218, WIDTH, 22, color::PANEL);
    line(row, out, 218, 0, WIDTH, color::GRID);
    let footer: &[u8] = match model.view.screen {
        Screen::Channel if model.view.entry.is_some() => b"MENU APPLY  EXIT CANCEL",
        Screen::Channel => b"UP/DN CH   MENU   0-9 SELECT",
        Screen::Menu => b"UP/DN ITEM   MENU OK   EXIT BACK",
        Screen::Spectrum => b"UP/DN CURSOR  MENU  EXIT CLOSE",
    };
    text(row, out, 12, 226, 1, footer, color::MUTED);
    Ok(())
}

fn validate(row: u16, model: &Model<'_>) -> Result<(), Error> {
    if row >= HEIGHT {
        return Err(Error::RowOutOfRange { row });
    }
    let view = model.view;
    if view.channel_count == 0
        || view.channel_count > 1024
        || view.channel_index >= view.channel_count
    {
        return Err(Error::InvalidView);
    }
    let valid_context = match view.screen {
        Screen::Channel => !view.spectrum_open,
        Screen::Spectrum => view.spectrum_open,
        Screen::Menu if view.spectrum_open => {
            matches!(view.menu_item, MenuItem::ResetPeaks | MenuItem::Back)
        }
        Screen::Menu => matches!(view.menu_item, MenuItem::Spectrum | MenuItem::Back),
    };
    if !valid_context || (view.ptt_pressed && view.spectrum_open) {
        return Err(Error::InvalidView);
    }
    if let Some(entry) = view.entry {
        if view.screen != Screen::Channel
            || !(1..=4).contains(&entry.digits)
            || u32::from(entry.value) >= 10_u32.pow(u32::from(entry.digits))
        {
            return Err(Error::InvalidView);
        }
    }
    if view.spectrum_open {
        if let Some(spectrum) = model.spectrum {
            if spectrum.scale_max == 0 {
                return Err(Error::ZeroScale);
            }
            let bins = spectrum.engine.bins().len();
            if spectrum.cursor >= bins {
                return Err(Error::CursorOutOfRange {
                    cursor: spectrum.cursor,
                    bins,
                });
            }
        }
    }
    Ok(())
}

fn channel_page(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let mut label = Text::new();
    let _ = write!(
        label,
        "CH {:04} / {:04}",
        model.view.channel_index + 1,
        model.view.channel_count
    );
    text(row, out, 14, 42, 1, label.bytes(), color::CURRENT);
    if let Some(channel) = model.channel {
        let mut name = Text::new();
        for &byte in channel.name.iter().take(16) {
            if byte == 0 || byte == 0xff {
                break;
            }
            let character = if byte.is_ascii_graphic() || byte == b' ' {
                byte.to_ascii_uppercase()
            } else {
                b'?'
            };
            let _ = name.write_char(char::from(character));
        }
        let name = if name.bytes().is_empty() {
            b"UNNAMED"
        } else {
            name.bytes()
        };
        text(row, out, 14, 61, 2, name, color::TEXT);
        text(row, out, 14, 88, 1, b"RX STORED", color::MUTED);
        let rx = frequency(channel.rx_10hz);
        text(row, out, 14, 103, 3, rx.bytes(), color::TEXT);
        text(row, out, 260, 116, 1, b"MHZ", color::CURRENT);
        line(row, out, 140, 14, 292, color::GRID);
        text(row, out, 14, 153, 1, b"TX STORED", color::MUTED);
        let tx = frequency(channel.tx_10hz);
        text(row, out, 14, 168, 2, tx.bytes(), color::MUTED);
        text(row, out, 260, 175, 1, b"MHZ", color::MUTED);
    } else {
        text(row, out, 14, 65, 2, b"NO CHANNEL DATA", color::TEXT);
        text(
            row,
            out,
            14,
            103,
            1,
            b"SELECT A STORED CHANNEL SLOT",
            color::MUTED,
        );
    }
    if let Some(entry) = model.view.entry {
        rect(row, out, 12, 191, 296, 21, color::PANEL);
        let mut number = Text::new();
        let _ = write!(
            number,
            "GO {:0width$}",
            entry.value,
            width = usize::from(entry.digits)
        );
        text(row, out, 20, 195, 2, number.bytes(), color::CURRENT);
    }
}

fn menu_page(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let spectrum = model.view.spectrum_open;
    text(
        row,
        out,
        14,
        46,
        2,
        if spectrum {
            b"SPECTRUM MENU"
        } else {
            b"CHANNEL MENU"
        },
        color::TEXT,
    );
    let items: [(MenuItem, &[u8]); 2] = [
        if spectrum {
            (MenuItem::ResetPeaks, b"RESET PEAKS")
        } else {
            (MenuItem::Spectrum, b"SPECTRUM")
        },
        (MenuItem::Back, b"BACK"),
    ];
    for (index, &(item, label)) in items.iter().enumerate() {
        let y = 89 + index as u16 * 48;
        let selected = item == model.view.menu_item;
        rect(
            row,
            out,
            12,
            y,
            296,
            36,
            if selected {
                color::CURRENT
            } else {
                color::PANEL
            },
        );
        text(
            row,
            out,
            24,
            y + 11,
            2,
            label,
            if selected {
                color::BACKGROUND
            } else {
                color::TEXT
            },
        );
    }
    text(
        row,
        out,
        14,
        193,
        1,
        if spectrum {
            b"PEAKS ARE RAW MAXIMA"
        } else {
            b"CHANNEL DATA STAYS UNCHANGED"
        },
        color::MUTED,
    );
}

fn spectrum_page(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let Some(spectrum) = model.spectrum else {
        text(row, out, 14, 52, 2, b"SPECTRUM", color::TEXT);
        text(row, out, 14, 89, 1, b"NO MEASUREMENT SOURCE", color::MUTED);
        text(
            row,
            out,
            14,
            111,
            1,
            b"NO SIGNAL VALUES GENERATED",
            color::MUTED,
        );
        return;
    };
    text(row, out, 12, 37, 1, b"RAW", color::MUTED);
    text(row, out, 84, 37, 1, b"CURRENT", color::CURRENT);
    text(row, out, 152, 37, 1, b"OLD", color::PREVIOUS);
    text(row, out, 200, 37, 1, b"PEAK", color::PEAK);
    text(
        row,
        out,
        12,
        54,
        1,
        state_label(spectrum.engine.state()),
        color::TEXT,
    );
    let mut sweep = Text::new();
    let _ = write!(sweep, "SWP {}", spectrum.engine.sweep());
    text(row, out, 164, 54, 1, sweep.bytes(), color::MUTED);

    chart(row, out, spectrum);
    let mut maximum = Text::new();
    let _ = write!(maximum, "{}", spectrum.scale_max);
    text(row, out, 4, CHART_TOP - 3, 1, maximum.bytes(), color::MUTED);
    let mut middle = Text::new();
    let _ = write!(middle, "{}", spectrum.scale_max / 2);
    text(row, out, 4, 127, 1, middle.bytes(), color::MUTED);
    text(row, out, 4, CHART_BOTTOM - 3, 1, b"0", color::MUTED);

    let range = spectrum.engine.range();
    let start = frequency(range.start_10hz());
    let end = frequency(range.end_10hz());
    text(row, out, CHART_X, 192, 1, start.bytes(), color::MUTED);
    text(
        row,
        out,
        CHART_X + CHART_WIDTH - text_width(end.bytes(), 1),
        192,
        1,
        end.bytes(),
        color::MUTED,
    );
    let bin = spectrum.engine.bins()[spectrum.cursor];
    let cursor_frequency = frequency(
        range
            .frequency_10hz(spectrum.cursor)
            .expect("validated cursor"),
    );
    let mut readout = Text::new();
    let _ = write!(readout, "{} MHZ  RAW ", cursor_frequency.as_str());
    match bin.latest {
        Some(measurement) => {
            let _ = write!(readout, "{}", measurement.raw);
        }
        None => {
            let _ = readout.write_str("--");
        }
    }
    let _ = readout.write_str("  P ");
    match bin.peak {
        Some(peak) => {
            let _ = write!(readout, "{peak}");
        }
        None => {
            let _ = readout.write_str("--");
        }
    }
    if bin
        .latest
        .is_some_and(|measurement| measurement.sweep != spectrum.engine.sweep())
    {
        let _ = readout.write_str(" OLD");
    }
    text(row, out, 12, 205, 1, readout.bytes(), color::TEXT);
}

fn chart(row: u16, out: &mut [u16; WIDTH], spectrum: SpectrumView<'_>) {
    if !(CHART_TOP..=CHART_BOTTOM).contains(&row) {
        return;
    }
    let bins = spectrum.engine.bins();
    let grid_row = (row - CHART_TOP) % 28 == 0;
    for column in 0..CHART_WIDTH {
        let x = CHART_X + column;
        let bin = bins[column * bins.len() / CHART_WIDTH];
        out[x] = if grid_row || column % 60 == 0 {
            color::GRID
        } else {
            color::CHART
        };
        if let Some(measurement) = bin.latest {
            let top = sample_y(measurement.raw, spectrum.scale_max);
            if row >= top {
                out[x] = if measurement.sweep == spectrum.engine.sweep() {
                    color::CURRENT
                } else {
                    color::PREVIOUS
                };
            }
        }
        if bin
            .peak
            .is_some_and(|value| sample_y(value, spectrum.scale_max) == row)
        {
            out[x] = color::PEAK;
        }
    }
    // Bars use floor(column * N / W), so bin i occupies the half-open interval
    // [ceil(i*W/N), ceil((i+1)*W/N)). Center that actual pixel interval: simply
    // rounding (i+0.5)*W/N can point into the preceding bin when W/N is fractional.
    let begin = (spectrum.cursor * CHART_WIDTH + bins.len() - 1) / bins.len();
    let end = ((spectrum.cursor + 1) * CHART_WIDTH + bins.len() - 1) / bins.len();
    let cursor_x = CHART_X + begin + (end - begin - 1) / 2;
    if (row - CHART_TOP) % 4 < 2 {
        out[cursor_x] = color::CURSOR;
    }
}

fn sample_y(raw: u16, scale: u16) -> u16 {
    let height = u32::from(raw.min(scale)) * u32::from(CHART_BOTTOM - CHART_TOP) / u32::from(scale);
    CHART_BOTTOM - height as u16
}

fn state_label(state: State) -> &'static [u8] {
    match state {
        State::Idle => b"IDLE",
        State::AwaitReady => b"WAIT READY",
        State::AwaitSettled => b"WAIT TUNE",
        State::Settling => b"SETTLING",
        State::AwaitSample => b"WAIT SAMPLE",
        State::SweepComplete => b"SWEEP DONE",
        State::Stopped(_) => b"STOPPED",
    }
}

struct Text {
    bytes: [u8; 64],
    len: usize,
}
impl Text {
    const fn new() -> Self {
        Self {
            bytes: [0; 64],
            len: 0,
        }
    }
    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(self.bytes()).expect("formatted ASCII")
    }
}
impl Write for Text {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.len + value.len() > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + value.len()].copy_from_slice(value.as_bytes());
        self.len += value.len();
        Ok(())
    }
}
fn frequency(code: u32) -> Text {
    let mut value = Text::new();
    let _ = write!(value, "{:03}.{:05}", code / 100_000, code % 100_000);
    value
}
fn text_width(bytes: &[u8], scale: u16) -> usize {
    bytes
        .len()
        .saturating_mul(6 * usize::from(scale))
        .saturating_sub(usize::from(scale))
}
fn rect(row: u16, out: &mut [u16; WIDTH], x: usize, y: u16, width: usize, height: u16, color: u16) {
    if row >= y && row < y.saturating_add(height) {
        let begin = x.min(WIDTH);
        let end = x.saturating_add(width).min(WIDTH);
        out[begin..end].fill(color);
    }
}
fn line(row: u16, out: &mut [u16; WIDTH], y: u16, x: usize, width: usize, color: u16) {
    rect(row, out, x, y, width, 1, color);
}
fn text(row: u16, out: &mut [u16; WIDTH], x: usize, y: u16, scale: u16, bytes: &[u8], color: u16) {
    if row < y || row >= y + 7 * scale {
        return;
    }
    let glyph_y = usize::from((row - y) / scale);
    for (index, &byte) in bytes.iter().enumerate() {
        let bits = font::glyph(byte)[glyph_y];
        for gx in 0..5 {
            if bits & (1 << (4 - gx)) == 0 {
                continue;
            }
            for dx in 0..usize::from(scale) {
                let position = x + (index * 6 + gx) * usize::from(scale) + dx;
                if position < WIDTH {
                    out[position] = color;
                }
            }
        }
    }
}
