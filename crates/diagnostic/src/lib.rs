#![no_std]
#![forbid(unsafe_code)]
//! Read-only diagnostic integration. Caller owns initialized, exclusive buses.
//! No clocks, GPIO, RF, PTT, storage writes, interrupts or boot path are started.
//! Calibration validation means layout/marker only, not checksum or RF accuracy.
//! Only one channel is buffered, with a 320-pixel row used while rendering.

use core::fmt::{self, Write};
use uv98_calibration::{Calibration, BLOCK_LEN};
use uv98_codeplug::channels::{Channel, CHANNEL_COUNT, CHANNEL_LEN};
use uv98_display::{Delay, Display, DisplayBus, HEIGHT, WIDTH};
use uv98_nor_read::{Bus, CalibrationReadError, ReadError};

mod font;

/// Physical NOR address, not the 0x1000 offset in a .880dmf file.
pub const CHANNELS_ADDRESS: u32 = 0x4000;

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError<E> {
    Calibration(CalibrationReadError<E>),
    Channel(ReadError<E>),
    IndexOutOfRange(usize),
    CalibrationNotLoaded,
}

pub struct Diagnostic {
    calibration: [u8; BLOCK_LEN],
    channel: [u8; CHANNEL_LEN],
    calibration_valid: bool,
    selected: Option<usize>,
}

pub struct Snapshot<'a> {
    pub calibration: Calibration<'a>,
    pub channel: Channel<'a>,
    index: usize,
}

impl Snapshot<'_> {
    /// Zero-based validated storage index. No zone or scan-list interpretation.
    pub fn index(&self) -> usize {
        self.index
    }
}

impl Default for Diagnostic {
    fn default() -> Self {
        Self::new()
    }
}

impl Diagnostic {
    pub const fn new() -> Self {
        Self {
            calibration: [0; BLOCK_LEN],
            channel: [0; CHANNEL_LEN],
            calibration_valid: false,
            selected: None,
        }
    }

    /// Any failed refresh invalidates both old views, including a complete read
    /// followed by a CS cleanup error. No partial or stale data is exposed.
    pub fn load_calibration<B: Bus>(&mut self, bus: &mut B) -> Result<(), LoadError<B::Error>> {
        self.calibration_valid = false;
        self.selected = None;
        uv98_nor_read::read_calibration(bus, &mut self.calibration)
            .map_err(LoadError::Calibration)?;
        self.calibration_valid = true;
        Ok(())
    }

    pub fn load_channel<B: Bus>(
        &mut self,
        bus: &mut B,
        index: usize,
    ) -> Result<(), LoadError<B::Error>> {
        self.selected = None;
        if !self.calibration_valid {
            return Err(LoadError::CalibrationNotLoaded);
        }
        if index >= CHANNEL_COUNT {
            return Err(LoadError::IndexOutOfRange(index));
        }
        let address = CHANNELS_ADDRESS + index as u32 * CHANNEL_LEN as u32;
        uv98_nor_read::read(bus, address, &mut self.channel).map_err(LoadError::Channel)?;
        self.selected = Some(index);
        Ok(())
    }

    pub fn snapshot(&self) -> Option<Snapshot<'_>> {
        if !self.calibration_valid {
            return None;
        }
        Some(Snapshot {
            calibration: Calibration::parse(&self.calibration).ok()?,
            channel: Channel::parse(&self.channel).ok()?,
            index: self.selected?,
        })
    }

    /// Immutable successfully read calibration, independent of channel loading.
    /// A failed calibration refresh invalidates this view before any bus access.
    pub fn calibration_view(&self) -> Option<Calibration<'_>> {
        if self.calibration_valid {
            Calibration::parse(&self.calibration).ok()
        } else {
            None
        }
    }

    /// Bounded navigation through raw slots, including inactive slots. No
    /// wraparound, channel activation, tuning or transmit command occurs.
    pub fn adjacent_index(&self, forward: bool) -> Option<usize> {
        let index = self.selected?;
        if forward {
            (index + 1 < CHANNEL_COUNT).then_some(index + 1)
        } else {
            index.checked_sub(1)
        }
    }

    /// Consume already-debounced events. PTT, held/released events and all other
    /// keys have no action. Timing and physical scanning belong to the caller.
    pub fn key_target(&self, key: uv98_keys::Key, event: uv98_keys::ButtonEvent) -> Option<usize> {
        if event != uv98_keys::ButtonEvent::Pressed {
            return None;
        }
        match key {
            uv98_keys::Key::Up => self.adjacent_index(false),
            uv98_keys::Key::Down => self.adjacent_index(true),
            _ => None,
        }
    }
}

/// One ASCII diagnostic line. Unsupported characters retain the '?' glyph.
#[derive(Clone, Copy)]
pub struct TextLine<'a> {
    pub x: u16,
    pub y: u16,
    pub scale: u16,
    pub text: &'a [u8],
}

/// Shared bounded row renderer for the hardware diagnostic app and host preview.
/// It uses one 640-byte row, never allocates a framebuffer, and propagates every
/// display failure. The caller must initialize the display. Scale zero is empty;
/// all coordinates/text are clipped without overflowing coordinate arithmetic.
pub fn render_text_lines<B: DisplayBus, D: Delay>(
    display: &mut Display<B, D>,
    lines: &[TextLine<'_>],
) -> Result<(), uv98_display::Error<B::Error>> {
    let mut row = [0u16; WIDTH as usize];
    display.begin_window(0, 0, WIDTH, HEIGHT)?;
    for y in 0..HEIGHT {
        row.fill(if y < 42 { 0x2104 } else { 0 });
        if y == 42 || y == 198 {
            row.fill(0x8410);
        }
        for line in lines {
            let scale = u32::from(line.scale);
            let dy = u32::from(y).wrapping_sub(u32::from(line.y));
            if scale == 0 || dy >= 7 * scale {
                continue;
            }
            let glyph_row = (dy / scale) as usize;
            for (index, &ch) in line.text.iter().take(WIDTH as usize).enumerate() {
                let x0 = u32::from(line.x) + index as u32 * 6 * scale;
                if x0 >= u32::from(WIDTH) {
                    break;
                }
                let bits = font::glyph(ch)[glyph_row];
                for gx in 0..5u32 {
                    if bits & (1 << (4 - gx)) == 0 {
                        continue;
                    }
                    let begin = (x0 + gx * scale).min(u32::from(WIDTH));
                    let end = (x0 + (gx + 1) * scale).min(u32::from(WIDTH));
                    row[begin as usize..end as usize].fill(0xffff);
                }
            }
        }
        display.write_pixels(&row)?;
    }
    display.finish_pixels()
}

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
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
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

/// Stream one frame through the real display protocol driver. Grayscale words
/// avoid claiming unqualified physical RGB/BGR color order. ASCII-only preview:
/// non-ASCII name bytes become '?'; original bytes remain in Snapshot::channel.
/// Caller initializes Display first. No heap or full framebuffer is used here.
pub fn render<B: DisplayBus, D: Delay>(
    display: &mut Display<B, D>,
    view: &Snapshot<'_>,
) -> Result<(), uv98_display::Error<B::Error>> {
    let mut slot = Text::new();
    let _ = write!(
        slot,
        "SLOT {:04} / {}",
        view.index + 1,
        if view.channel.is_active() {
            "ACTIVE"
        } else {
            "INACTIVE"
        }
    );
    let mut name = Text::new();
    for &b in view.channel.name_bytes() {
        let ch = if b.is_ascii_graphic() || b == b' ' {
            b.to_ascii_uppercase() as char
        } else {
            '?'
        };
        let _ = name.write_char(ch);
    }
    let mut rx = Text::new();
    let mut tx = Text::new();
    let mut fields = Text::new();
    if view.channel.is_active() {
        let code = view.channel.rx_frequency_code();
        let _ = write!(rx, "RX {:03}.{:05} MHZ", code / 100000, code % 100000);
        let code = view.channel.tx_frequency_code();
        let _ = write!(tx, "TX {:03}.{:05} MHZ", code / 100000, code % 100000);
        let _ = write!(
            fields,
            "RAW BW:{} PWR:{} MODE:{}",
            view.channel.bandwidth_raw(),
            view.channel.power_raw(),
            view.channel.operation_mode_raw()
        );
    } else {
        let _ = rx.write_str("NO ACTIVE CHANNEL");
        let _ = tx.write_str("STORED BYTES RETAINED");
    }
    let lines: [(u16, u16, u16, &[u8]); 10] = [
        (12, 12, 2, b"UV98 / DIAGNOSTIC"),
        (12, 38, 1, b"STORED DATA / NO RF DRIVER"),
        (12, 65, 1, b"CAL LAYOUT OK - 4096 BYTES"),
        (12, 87, 1, slot.as_bytes()),
        (12, 105, 2, name.as_bytes()),
        (12, 132, 2, rx.as_bytes()),
        (12, 156, 1, tx.as_bytes()),
        (12, 176, 1, fields.as_bytes()),
        (12, 205, 1, b"UP/DOWN: CHANNEL SLOT"),
        (12, 222, 1, b"NO RF MEASUREMENT"),
    ];
    let mut row = [0u16; WIDTH as usize];
    display.begin_window(0, 0, WIDTH, HEIGHT)?;
    for y in 0..HEIGHT {
        let background = if y < 54 { 0x2104 } else { 0x0000 };
        row.fill(background);
        if y == 54 || y == 195 {
            row.fill(0x8410);
        }
        for &(x0, y0, scale, text) in &lines {
            if y < y0 || y >= y0 + 7 * scale {
                continue;
            }
            let gy = ((y - y0) / scale) as usize;
            for (i, &ch) in text.iter().enumerate() {
                let bits = font::glyph(ch)[gy];
                for gx in 0..5 {
                    if bits & (1 << (4 - gx)) == 0 {
                        continue;
                    }
                    for dx in 0..scale {
                        let x = x0 as usize
                            + i * 6 * scale as usize
                            + gx * scale as usize
                            + dx as usize;
                        if x < row.len() {
                            row[x] = 0xffff;
                        }
                    }
                }
            }
        }
        display.write_pixels(&row)?;
    }
    display.finish_pixels()
}
