//! Bounded RGB565 renderer for the physical handheld UI. No framebuffer, heap,
//! measurements, frequency tuning, or storage writes. Radio activity is shown
//! only from an explicit backend report, never inferred from a key or request.

use core::fmt::{self, Write};
use uv98_codeplug::channels::{Bandwidth, Power, Tone};
use uv98_display::{Delay, Display, DisplayBus};
use uv98_keys::Key;
use uv98_spectrum::compact::{Plot, BINS as SPECTRUM_BINS};

#[cfg(test)]
use crate::controller::MENU_ITEMS;
use crate::controller::{
    binding_digit, menu_entries, EntryKind, LightMode, MenuEntry, MenuGroup, MenuItem, Mode,
    Notice, ScanState, Screen, Setting, ShortcutAction, SideAction, Slot, View, LIGHT_MODES,
    SHORTCUT_ACTIONS, SIDE_ACTIONS, STEP_10HZ,
};

#[path = "font.rs"]
mod font;

pub const WIDTH: usize = 240;
pub const HEIGHT: u16 = 320;
pub const LIST_ROWS: usize = 7;
const LIST_Y: u16 = 54;
const LIST_ROW_HEIGHT: u16 = 32;
const ITEM_PAINT_HEIGHT: u16 = 30;
const SETTING_Y: u16 = 112;
const SETTING_ROW_HEIGHT: u16 = 44;
const SETTING_PAINT_HEIGHT: u16 = 36;

pub mod color {
    pub const BACKGROUND: u16 = 0x0842;
    pub const PANEL: u16 = 0x10c5;
    pub const LINE: u16 = 0x2949;
    pub const TEXT: u16 = 0xf7be;
    pub const MUTED: u16 = 0xa554;
    pub const ACCENT: u16 = 0x4e9f;
    pub const GOOD: u16 = 0x7f2d;
    pub const WARNING: u16 = 0xfde7;
    pub const WARNING_BACKGROUND: u16 = 0x2901;
    pub const BAD: u16 = 0xfa08;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RadioState {
    Off,
    Starting,
    Receiving,
    Transmitting,
    Unavailable,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Modulation {
    Fm,
    Am,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RadioRoute {
    Hf,
    Vhf,
    Uhf,
    Uhf800,
}

/// Confirmed runtime status. None means unavailable, not a fabricated reading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RadioView {
    pub state: RadioState,
    pub rssi: Option<u16>,
    pub squelch_open: bool,
    pub monitor: bool,
    /// Confirmed power selection: 0 low, 1 medium, 2 high; not measured watts.
    pub power: Option<u8>,
    pub modulation: Modulation,
    pub route: Option<RadioRoute>,
    /// Build-profile warning only. This never means TX is active or eligible.
    pub tx_trial: bool,
}

impl RadioView {
    pub const OFF: Self = Self {
        state: RadioState::Off,
        rssi: None,
        squelch_open: false,
        monitor: false,
        power: None,
        modulation: Modulation::Fm,
        route: None,
        tx_trial: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpectrumState {
    Preparing,
    Sweeping,
    Restoring,
    Stopped,
    Failed,
}

/// Borrow an immutable published snapshot, never the actively changing sweep
/// buffer. The caller publishes a new snapshot only after queued rows drain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpectrumView<'a> {
    pub state: SpectrumState,
    pub start_10hz: u32,
    pub step_10hz: u32,
    pub len: u8,
    pub plot: &'a Plot,
}

impl SpectrumView<'_> {
    pub fn frequency_10hz(self, index: u8) -> Option<u32> {
        if index >= self.len {
            return None;
        }
        self.step_10hz
            .checked_mul(u32::from(index))?
            .checked_add(self.start_10hz)
    }
}

#[derive(Clone, Copy)]
pub struct ChannelView<'a> {
    pub index: usize,
    /// At most the first 16 stored bytes are displayed. NUL/FF terminate the
    /// name. Stock names use GBK: ASCII and its Cyrillic block are supported;
    /// other characters become '?', without guessing a different encoding.
    /// Display conversion never changes the original channel record.
    pub name: &'a [u8],
    pub rx_10hz: u32,
    pub tx_10hz: u32,
    pub active: bool,
    pub rx_tone_raw: u16,
    pub tx_tone_raw: u16,
    pub bandwidth_raw: u8,
    pub power_raw: u8,
}

#[derive(Clone, Copy)]
pub struct ZoneView<'a> {
    /// None is the unfiltered All row; other indices refer to stored zones.
    pub index: Option<u8>,
    pub name: &'a [u8],
    pub channel_count: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerView<'a> {
    pub slot: Slot,
    pub mode: Mode,
    pub frequency_10hz: Option<u32>,
    pub channel: Option<u16>,
    pub name: &'a [u8],
    pub zone: Option<u8>,
    pub zone_name: Option<&'a [u8]>,
    pub reverse: bool,
}

#[derive(Clone, Copy)]
pub struct Model<'a> {
    pub view: View,
    pub channel: Option<ChannelView<'a>>,
    /// Visible window of at most seven stored channels, ordered by the caller.
    pub list: &'a [ChannelView<'a>],
    pub calibration_matches: Option<bool>,
    pub last_key: Option<Key>,
    pub ptt_pressed: bool,
    pub input_ready: bool,
    pub radio: RadioView,
    /// None means all channels, not a fabricated stored zone name.
    pub zone_name: Option<&'a [u8]>,
    pub zones: &'a [ZoneView<'a>],
    /// Saved other slot, not a second receiver or evidence of dual watch.
    pub peer: Option<PeerView<'a>>,
    pub spectrum: Option<SpectrumView<'a>>,
}

/// Fixed physical positions: selecting B never swaps the cards on screen.
pub const fn home_slot_rows(slot: Slot) -> (u16, u16) {
    match slot {
        Slot::A => (52, 142),
        Slot::B => (148, 238),
    }
}

pub const fn home_frequency_rows(slot: Slot) -> (u16, u16) {
    let top = home_slot_rows(slot).0;
    (top + 39, top + 67)
}

fn home_ham_rows(slot: Slot) -> (u16, u16) {
    let top = home_slot_rows(slot).0;
    (top + 70, top + 81)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    RowOutOfRange { row: u16 },
    InvalidRange,
    InvalidView,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RenderError<E> {
    Model(Error),
    Display(uv98_display::Error<E>),
}

/// At most four sorted, disjoint half-open row intervals. This is a render
/// plan, not a pixel buffer: the caller can merge it into a pending row bitmap
/// and render short slices between input samples using the latest model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirtyRanges {
    ranges: [(u16, u16); 4],
    len: usize,
}

impl DirtyRanges {
    pub fn ranges(&self) -> &[(u16, u16)] {
        &self.ranges[..self.len]
    }

    fn new() -> Self {
        Self {
            ranges: [(0, 0); 4],
            len: 0,
        }
    }

    fn push(&mut self, mut start: u16, mut end: u16) {
        if start >= end {
            return;
        }
        let mut index = 0;
        while index < self.len {
            let (a, b) = self.ranges[index];
            if start <= b && a <= end {
                start = start.min(a);
                end = end.max(b);
                self.ranges.copy_within(index + 1..self.len, index);
                self.len -= 1;
            } else {
                index += 1;
            }
        }
        // Five independent changes need not invalidate the whole display.
        // Merge the nearest pair; extra rows are harmless, dropped dirty rows
        // are not. This matters when RSSI, squelch and power change together.
        if self.len == self.ranges.len() {
            let mut expanded = [(0, 0); 5];
            let insert = self.ranges.partition_point(|&(a, _)| a < start);
            expanded[..insert].copy_from_slice(&self.ranges[..insert]);
            expanded[insert] = (start, end);
            expanded[insert + 1..].copy_from_slice(&self.ranges[insert..]);
            let mut closest = 0;
            for index in 1..4 {
                if expanded[index + 1].0 - expanded[index].1
                    < expanded[closest + 1].0 - expanded[closest].1
                {
                    closest = index;
                }
            }
            expanded[closest].1 = expanded[closest + 1].1;
            expanded.copy_within(closest + 2..5, closest + 1);
            self.ranges.copy_from_slice(&expanded[..4]);
            return;
        }
        let index = self.ranges[..self.len].partition_point(|&(a, _)| a < start);
        self.ranges.copy_within(index..self.len, index + 1);
        self.ranges[index] = (start, end);
        self.len += 1;
    }
}

pub fn render<B: DisplayBus, D: Delay>(
    display: &mut Display<B, D>,
    model: &Model<'_>,
) -> Result<(), RenderError<B::Error>> {
    render_rows(display, model, 0, HEIGHT)
}

/// Render the half-open row interval. Validation precedes every display access.
/// A caller must use a complete frame after startup or account separately for
/// externally changed data not represented by the controller's View. Known
/// page transitions are covered by `dirty_ranges`.
pub fn render_rows<B: DisplayBus, D: Delay>(
    display: &mut Display<B, D>,
    model: &Model<'_>,
    start_y: u16,
    end_y: u16,
) -> Result<(), RenderError<B::Error>> {
    validate(model).map_err(RenderError::Model)?;
    if start_y > end_y || end_y > HEIGHT {
        return Err(RenderError::Model(Error::InvalidRange));
    }
    if start_y == end_y {
        return Ok(());
    }
    display
        .begin_window(0, start_y, WIDTH as u16, end_y - start_y)
        .map_err(RenderError::Display)?;
    let mut pixels = [0u16; WIDTH];
    for y in start_y..end_y {
        // The immutable model and complete row interval were checked before
        // begin_window. Do not walk cached lists or recheck numeric entry for
        // every row in an incremental batch.
        render_validated_row(y, &mut pixels, model);
        display
            .write_pixels(&pixels)
            .map_err(RenderError::Display)?;
    }
    display.finish_pixels().map_err(RenderError::Display)
}

/// Conservative incremental updates for controller-only changes. The caller
/// passes None after a channel-list window/cache or other external model change.
/// Old and new menu/list rows are redrawn together so no stale selection remains.
pub fn render_changed<B: DisplayBus, D: Delay>(
    display: &mut Display<B, D>,
    model: &Model<'_>,
    previous: Option<View>,
) -> Result<(), RenderError<B::Error>> {
    let dirty = dirty_ranges(model, previous).map_err(RenderError::Model)?;
    for &(start, end) in dirty.ranges() {
        render_rows(display, model, start, end)?;
    }
    Ok(())
}

/// Plan the same updates as `render_changed`, without accessing a display.
/// Passing None invalidates the whole frame, including external model fields.
pub fn dirty_ranges(model: &Model<'_>, previous: Option<View>) -> Result<DirtyRanges, Error> {
    validate(model)?;
    let mut dirty = DirtyRanges::new();
    let Some(old) = previous else {
        dirty.push(0, HEIGHT);
        return Ok(dirty);
    };
    let new = model.view;
    if old == new {
        return Ok(dirty);
    }
    // Account for shared chrome before any navigation early return. Its
    // unchanged brand, panel fills and separators need not cross SPI again.
    if old.locked != new.locked {
        dirty.push(0, 27);
        dirty.push(296, HEIGHT);
    }
    if old.notice != new.notice {
        // Home's first shortcut glyph starts one row above the notice band.
        // Redraw it too when hiding or restoring the underlying hints.
        dirty.push(269, 295);
    }
    if old.ptt_down != new.ptt_down && model.radio.state == RadioState::Off {
        dirty.push(28, 48);
    }
    if old.screen != new.screen || new.screen == Screen::Setting && old.setting != new.setting {
        dirty.push(11, 18); // Page title, including erasing a longer old title.
        dirty.push(48, 296); // All old body content, padding and notice band.
        dirty.push(305, 312); // Footer labels; its fill and separator are fixed.
                              // A confirmed spectrum snapshot overrides the ordinary radio banner.
        if old.screen == Screen::Spectrum || new.screen == Screen::Spectrum {
            dirty.push(28, 48);
        }
        return Ok(dirty);
    }
    if new.screen == Screen::Menu && old.menu_group != new.menu_group {
        dirty.push(11, 18);
        // Include the old occupied rows when entering a shorter category.
        // Padding belongs to the list too; clearing only new items leaves
        // stale entries below Scanning and Keys.
        let visible_rows = menu_entries(old.menu_group)
            .len()
            .max(menu_entries(new.menu_group).len())
            .min(LIST_ROWS);
        dirty.push(LIST_Y, LIST_Y + visible_rows as u16 * LIST_ROW_HEIGHT);
        dirty.push(283, 290);
        return Ok(dirty);
    }
    if old.mode != new.mode || old.active_slot != new.active_slot || old.zone != new.zone {
        if new.screen != Screen::Home {
            dirty.push(0, HEIGHT);
            return Ok(dirty);
        }
        if old.active_slot != new.active_slot {
            dirty.push(52, 238);
        } else {
            let (start, end) = home_slot_rows(new.active_slot);
            dirty.push(start, end);
        }
        dirty.push(245, 265);
    }
    match new.screen {
        Screen::Menu if menu_start(old.menu_index) == menu_start(new.menu_index) => {
            if old.menu_index == new.menu_index {
                return Ok(dirty);
            }
            let first = menu_start(new.menu_index);
            for index in [old.menu_index, new.menu_index] {
                let y = LIST_Y + (usize::from(index) - first) as u16 * LIST_ROW_HEIGHT;
                dirty.push(y, y + ITEM_PAINT_HEIGHT);
            }
            dirty.push(283, 290);
        }
        Screen::Channels => {
            if old.list_channel == new.list_channel {
                return Ok(dirty);
            }
            let old_row = model
                .list
                .iter()
                .position(|channel| Some(channel.index as u16) == old.list_channel);
            let new_row = model
                .list
                .iter()
                .position(|channel| Some(channel.index as u16) == new.list_channel);
            if let (Some(a), Some(b)) = (old_row, new_row) {
                for index in [a, b] {
                    let y = LIST_Y + index as u16 * LIST_ROW_HEIGHT;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
            } else {
                dirty.push(LIST_Y, LIST_Y + LIST_ROWS as u16 * LIST_ROW_HEIGHT);
            }
        }
        Screen::Zones => {
            if old.zone_cursor == new.zone_cursor {
                return Ok(dirty);
            }
            let a = model
                .zones
                .iter()
                .position(|zone| zone.index == old.zone_cursor);
            let b = model
                .zones
                .iter()
                .position(|zone| zone.index == new.zone_cursor);
            if let (Some(a), Some(b)) = (a, b) {
                for index in [a, b] {
                    let y = LIST_Y + index as u16 * LIST_ROW_HEIGHT;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
            } else {
                dirty.push(LIST_Y, LIST_Y + LIST_ROWS as u16 * LIST_ROW_HEIGHT);
            }
        }
        Screen::SideKeys | Screen::SideAction => {
            let (a, b) = if new.screen == Screen::SideKeys {
                (old.side_index, new.side_index)
            } else {
                (old.side_action_index, new.side_action_index)
            };
            if old.side_bindings != new.side_bindings {
                dirty.push(48, HEIGHT);
            } else if a != b {
                for index in [a, b] {
                    let y = LIST_Y + u16::from(index) * LIST_ROW_HEIGHT;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
            }
            if new.screen == Screen::SideAction && old.side_index != new.side_index {
                dirty.push(283, 290);
            }
        }
        Screen::Shortcuts | Screen::ShortcutAction => {
            let (a, b) = if new.screen == Screen::Shortcuts {
                (old.binding_index, new.binding_index)
            } else {
                (old.binding_action_index, new.binding_action_index)
            };
            if menu_start(a) != menu_start(b) || old.bindings != new.bindings {
                dirty.push(48, HEIGHT);
            } else if a != b {
                for index in [a, b] {
                    let y = LIST_Y + (usize::from(index) - menu_start(b)) as u16 * LIST_ROW_HEIGHT;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
                dirty.push(283, 290);
            }
        }
        Screen::Home => {
            let top = home_slot_rows(new.active_slot).0;
            if old.scan != new.scan && new.entry.is_none() {
                dirty.push(242, 265);
                dirty.push(296, HEIGHT);
            }
            if old.reverse != new.reverse && new.entry.is_none() {
                let (a, b) = home_frequency_rows(new.active_slot);
                dirty.push(a, b);
                let (a, b) = home_ham_rows(new.active_slot);
                dirty.push(a, b);
            }
            if old.entry != new.entry {
                if let (Some(a), Some(b)) = (old.entry, new.entry) {
                    if a.kind == b.kind {
                        dirty.push(top + 42, top + 63);
                        return Ok(dirty);
                    }
                }
                let (a, b) = home_slot_rows(new.active_slot);
                dirty.push(a, b);
                dirty.push(242, 265);
                dirty.push(296, HEIGHT);
                return Ok(dirty);
            }
            if old.channel != new.channel {
                dirty.push(top + 7, top + 14);
                dirty.push(top + 25, top + 32);
                let (a, b) = home_frequency_rows(new.active_slot);
                dirty.push(a, b);
                let (a, b) = home_ham_rows(new.active_slot);
                dirty.push(a, b);
            }
            if old.vfo_10hz != new.vfo_10hz && new.mode == Mode::Vfo && new.entry.is_none() {
                let (a, b) = home_frequency_rows(new.active_slot);
                dirty.push(a, b);
            }
            if old.step_index != new.step_index {
                dirty.push(245, 252);
                if new.mode == Mode::Vfo && new.entry.is_none() {
                    dirty.push(top + 72, top + 79);
                }
            }
        }
        Screen::Step => {
            if old.step_index != new.step_index {
                for index in [old.step_index, new.step_index] {
                    let y = 62 + u16::from(index) * 33;
                    dirty.push(y, y + ITEM_PAINT_HEIGHT);
                }
            }
        }
        Screen::Spectrum => {
            if old.spectrum_cursor != new.spectrum_cursor {
                dirty.push(56, 77);
                dirty.push(225, 232);
                dirty.push(240, 247);
            }
        }
        Screen::Setting => {
            if old.setting_index != new.setting_index {
                for index in [old.setting_index, new.setting_index] {
                    let (start, pitch, height) = setting_geometry(new.setting.unwrap());
                    let y = start + u16::from(index) * pitch;
                    dirty.push(y, y + height);
                }
            }
            if old.reverse != new.reverse
                || old.scan != new.scan
                || old.display_light != new.display_light
                || old.key_light != new.key_light
            {
                dirty.push(84, 91);
            }
        }
        Screen::ChannelDetails => {
            if old.channel != new.channel || old.reverse != new.reverse {
                dirty.push(48, 295);
            }
        }
        Screen::Keys => dirty.push(52, 270),
        Screen::Calibration | Screen::Help | Screen::About => {}
        // Menu window changed. Its header and footer remain unchanged.
        Screen::Menu => {
            dirty.push(LIST_Y, LIST_Y + LIST_ROWS as u16 * LIST_ROW_HEIGHT);
            dirty.push(283, 290);
        }
    }
    Ok(dirty)
}

/// Metadata changes use much smaller regions than screen navigation. In
/// particular a live RSSI sample does not invalidate the frequency or footer.
pub fn radio_dirty_ranges(model: &Model<'_>, previous: RadioView) -> Result<DirtyRanges, Error> {
    validate(model)?;
    let new = model.radio;
    let mut dirty = DirtyRanges::new();
    if model.view.screen == Screen::Home
        && model.view.entry.is_none()
        && ((previous.tx_trial != new.tx_trial && !new.monitor)
            || (previous.monitor != new.monitor && (previous.tx_trial || new.tx_trial)))
    {
        dirty.push(258, 265);
    }
    if model.view.screen == Screen::Home
        && model.view.entry.is_none()
        && !model.view.locked
        && previous.monitor != new.monitor
    {
        dirty.push(296, HEIGHT);
    }
    if previous.state != new.state {
        dirty.push(28, 48);
        match model.view.screen {
            Screen::Home if model.view.entry.is_none() => {
                let (a, b) = home_frequency_rows(model.view.active_slot);
                dirty.push(a, b);
                let (a, b) = home_ham_rows(model.view.active_slot);
                dirty.push(a, b);
                dirty.push(245, 265);
            }
            Screen::Keys => dirty.push(211, 218),
            Screen::Setting if setting_uses_radio(model.view.setting) => dirty.push(84, 91),
            _ => {}
        }
        return Ok(dirty);
    }
    if new.state == RadioState::Receiving && previous.squelch_open != new.squelch_open {
        dirty.push(28, 48);
    }
    if model.view.screen == Screen::Setting {
        let changed = match model.view.setting {
            Some(Setting::Power) => previous.power != new.power,
            Some(Setting::Monitor) => previous.monitor != new.monitor,
            Some(Setting::Modulation) => previous.modulation != new.modulation,
            _ => false,
        };
        if changed {
            dirty.push(84, 91);
        }
        return Ok(dirty);
    }
    if model.view.screen != Screen::Home
        || model.view.entry.is_some()
        || !matches!(new.state, RadioState::Receiving | RadioState::Transmitting)
    {
        return Ok(dirty);
    }
    if previous.rssi != new.rssi
        && new.state == RadioState::Receiving
        && (!new.tx_trial || new.monitor)
        || previous.monitor != new.monitor
    {
        dirty.push(258, 265);
    }
    if previous.modulation != new.modulation || previous.route != new.route {
        dirty.push(245, 252);
    }
    if previous.power != new.power {
        let (a, b) = home_ham_rows(model.view.active_slot);
        dirty.push(a, b);
        if new.state == RadioState::Transmitting {
            dirty.push(258, 265);
        }
    }
    Ok(dirty)
}

/// Damage from replacing one stable spectrum snapshot. Cursor-only movement
/// is handled by dirty_ranges and does not repaint the 114-row graph.
pub fn spectrum_dirty_ranges(
    model: &Model<'_>,
    previous: Option<SpectrumView<'_>>,
) -> Result<DirtyRanges, Error> {
    validate(model)?;
    let mut dirty = DirtyRanges::new();
    if model.view.screen != Screen::Spectrum {
        return Ok(dirty);
    }
    match (previous, model.spectrum) {
        (Some(old), Some(new))
            if old.start_10hz == new.start_10hz
                && old.step_10hz == new.step_10hz
                && old.len == new.len =>
        {
            if old.plot != new.plot {
                dirty.push(96, 224);
                dirty.push(240, 261);
            }
            if old.state != new.state {
                dirty.push(28, 48);
                dirty.push(269, 290);
            }
        }
        (None, None) => {}
        _ => dirty.push(28, 296),
    }
    Ok(dirty)
}

/// Deterministic row renderer. Invalid input preserves the caller's pixel row.
pub fn render_row(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) -> Result<(), Error> {
    if row >= HEIGHT {
        return Err(Error::RowOutOfRange { row });
    }
    validate(model)?;
    render_validated_row(row, out, model);
    Ok(())
}

/// Internal fast path: caller has validated the immutable model and row range.
fn render_validated_row(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    out.fill(color::BACKGROUND);
    if row < 48 {
        header(row, out, model);
    } else if row < 296 {
        match model.view.screen {
            Screen::Home => home(row, out, model),
            Screen::Menu => menu(row, out, model),
            Screen::Channels => channels(row, out, model),
            Screen::ChannelDetails => details(row, out, model),
            Screen::Step => steps(row, out, model),
            Screen::Calibration => calibration(row, out, model),
            Screen::Keys => keys(row, out, model),
            Screen::Help => help(row, out),
            Screen::About => about(row, out),
            Screen::Shortcuts => shortcuts(row, out, model),
            Screen::ShortcutAction => shortcut_actions(row, out, model),
            Screen::Zones => zones(row, out, model),
            Screen::SideKeys => side_keys(row, out, model),
            Screen::SideAction => side_actions(row, out, model),
            Screen::Setting => setting(row, out, model),
            Screen::Spectrum => spectrum(row, out, model),
        }
        notice(row, out, model.view.notice);
    } else {
        footer(row, out, model);
    }
}

fn validate(model: &Model<'_>) -> Result<(), Error> {
    let view = model.view;
    if view.spectrum_cursor as usize >= SPECTRUM_BINS {
        return Err(Error::InvalidView);
    }
    if view.screen == Screen::Spectrum {
        if let Some(spectrum) = model.spectrum {
            if spectrum.len == 0
                || usize::from(spectrum.len) > SPECTRUM_BINS
                || spectrum.start_10hz == 0
                || spectrum.step_10hz == 0
                || spectrum
                    .frequency_10hz(spectrum.len - 1)
                    .is_none_or(|f| f > 99_999_999)
            {
                return Err(Error::InvalidView);
            }
            for index in 0..usize::from(spectrum.len) {
                if spectrum.plot.valid(index) && spectrum.plot.latest[index] > 511
                    || spectrum.plot.peak_valid(index) && spectrum.plot.peaks[index] > 511
                {
                    return Err(Error::InvalidView);
                }
            }
        }
    }
    if view.screen == Screen::Setting
        && !view
            .setting
            .is_some_and(|setting| usize::from(view.setting_index) < setting_options(setting).len())
    {
        return Err(Error::InvalidView);
    }
    if usize::from(view.menu_index) >= menu_entries(view.menu_group).len()
        || usize::from(view.step_index) >= STEP_10HZ.len()
        || usize::from(view.binding_index) >= view.bindings.len()
        || usize::from(view.binding_action_index) >= SHORTCUT_ACTIONS.len()
        || usize::from(view.side_index) >= view.side_bindings.len()
        || usize::from(view.side_action_index) >= SIDE_ACTIONS.len()
        || view.channel.is_some_and(|index| index >= 1024)
        || view.list_channel.is_some_and(|index| index >= 1024)
        || model.channel.is_some_and(|channel| channel.index >= 1024)
        || model.list.len() > LIST_ROWS
        || model.zones.len() > LIST_ROWS
        || model.zones.iter().any(|zone| zone.channel_count > 1024)
        || model
            .peer
            .is_some_and(|peer| peer.channel.is_some_and(|channel| channel >= 1024))
        || model.list.iter().any(|channel| channel.index >= 1024)
    {
        return Err(Error::InvalidView);
    }
    if let Some(entry) = view.entry {
        let maximum_digits = match entry.kind {
            EntryKind::Channel => 4,
            EntryKind::Frequency => 8,
        };
        if entry.digits == 0
            || entry.digits > maximum_digits
            || entry.value >= 10u32.pow(u32::from(entry.digits))
        {
            return Err(Error::InvalidView);
        }
    }
    Ok(())
}

fn header(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    rect(row, out, 0, 0, WIDTH, 27, color::PANEL);
    text(row, out, 12, 6, 2, "UV98", color::TEXT);
    text(row, out, 72, 11, 1, page_title(model.view), color::ACCENT);
    if model.view.locked {
        rect(row, out, 178, 5, 52, 18, color::WARNING);
        symbol(row, out, 182, 6, Symbol::Lock, color::BACKGROUND);
        text(row, out, 204, 10, 1, "БЛОК", color::BACKGROUND);
    } else {
        text(row, out, 182, 11, 1, "0.1.8-r4", color::MUTED);
    }
    let (label, foreground, background) = match model.radio.state {
        RadioState::Off if model.ptt_pressed || model.view.ptt_down => (
            "PTT: ПЕРЕДАЧА ОТКЛЮЧЕНА",
            color::WARNING,
            color::WARNING_BACKGROUND,
        ),
        RadioState::Off => ("РАДИО ВЫКЛЮЧЕНО", color::WARNING, color::WARNING_BACKGROUND),
        RadioState::Starting => ("РАДИО: ЗАПУСК", color::WARNING, color::WARNING_BACKGROUND),
        RadioState::Receiving if model.radio.squelch_open => {
            ("RX: ПРИЁМ СИГНАЛА", color::BACKGROUND, color::GOOD)
        }
        RadioState::Receiving => ("RX: ОЖИДАНИЕ СИГНАЛА", color::GOOD, color::PANEL),
        RadioState::Transmitting => ("TX: ПЕРЕДАЧА", color::TEXT, color::BAD),
        RadioState::Unavailable => (
            "РАДИО НЕДОСТУПНО",
            color::WARNING,
            color::WARNING_BACKGROUND,
        ),
        RadioState::Failed => ("ОШИБКА РАДИО", color::TEXT, color::BAD),
    };
    // Confirmed sweep publication is more precise than the generic Starting
    // state used during retunes. Never hide a real TX or terminal RF fault.
    let (label, foreground, background) = if model.view.screen == Screen::Spectrum
        && !matches!(
            model.radio.state,
            RadioState::Transmitting | RadioState::Failed
        ) {
        match model.spectrum.map(|s| s.state) {
            Some(SpectrumState::Sweeping) => ("ОБЗОР: ЗВУК ВЫКЛ", color::ACCENT, color::PANEL),
            Some(SpectrumState::Preparing) => (
                "ОБЗОР: ПОДГОТОВКА",
                color::WARNING,
                color::WARNING_BACKGROUND,
            ),
            Some(SpectrumState::Restoring) => (
                "ВОЗВРАТ К ПРИЁМУ",
                color::WARNING,
                color::WARNING_BACKGROUND,
            ),
            Some(SpectrumState::Failed) => ("ОШИБКА ОБЗОРА", color::TEXT, color::BAD),
            _ => (label, foreground, background),
        }
    } else {
        (label, foreground, background)
    };
    rect(row, out, 0, 28, WIDTH, 20, background);
    centered(row, out, 34, 1, label, foreground);
}

fn page_title(view: View) -> &'static str {
    match view.screen {
        Screen::Home => "СЛОТЫ A / B",
        Screen::Menu => view.menu_group.map(menu_group_label).unwrap_or("МЕНЮ"),
        Screen::Channels => "СПИСОК КАНАЛОВ",
        Screen::ChannelDetails => "ДАННЫЕ КАНАЛА",
        Screen::Step => "ШАГ ЧАСТОТЫ",
        Screen::Calibration => "КАЛИБРОВКИ",
        Screen::Keys => "ПРОВЕРКА КНОПОК",
        Screen::Help => "УПРАВЛЕНИЕ",
        Screen::About => "О ПРОШИВКЕ",
        Screen::Shortcuts => "ДОЛГИЕ КЛАВИШИ",
        Screen::ShortcutAction => "НАЗНАЧИТЬ ДЕЙСТВИЕ",
        Screen::Zones => "ЗОНЫ КАНАЛОВ",
        Screen::SideKeys => "БОКОВЫЕ КНОПКИ",
        Screen::SideAction => "ДЕЙСТВИЕ КНОПКИ",
        Screen::Setting => view.setting.map(setting_title).unwrap_or("НАСТРОЙКА"),
        Screen::Spectrum => "СПЕКТР ДИАПАЗОНА",
    }
}

const GRAPH_X: usize = 12;
const GRAPH_WIDTH: usize = 216;
const GRAPH_TOP: u16 = 110;
const GRAPH_BOTTOM: u16 = 223;

fn spectrum_bin_x(index: usize, len: usize) -> usize {
    GRAPH_X + index * (GRAPH_WIDTH - 2) / len.saturating_sub(1).max(1)
}

fn spectrum_level_y(raw: u16) -> u16 {
    GRAPH_BOTTOM - ((u32::from(raw) * u32::from(GRAPH_BOTTOM - GRAPH_TOP)) / 511) as u16
}

fn spectrum(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let Some(view) = model.spectrum else {
        centered(row, out, 116, 2, "НЕТ ИЗМЕРЕНИЙ", color::WARNING);
        centered(row, out, 147, 1, "ОЖИДАНИЕ ПРИЁМНИКА", color::MUTED);
        centered(row, out, 183, 1, "EXIT: ВЕРНУТЬ ЧАСТОТУ", color::MUTED);
        return;
    };
    let cursor = model.view.spectrum_cursor.min(view.len - 1);
    if (56..77).contains(&row) {
        let value = frequency(view.frequency_10hz(cursor).unwrap());
        centered(row, out, 56, 3, value.as_str(), color::TEXT);
    }
    if (84..91).contains(&row) {
        let mut range = Text::new();
        let _ = write!(
            range,
            "{} - {}",
            frequency(view.start_10hz).as_str(),
            frequency(view.frequency_10hz(view.len - 1).unwrap()).as_str()
        );
        centered(row, out, 84, 1, range.as_str(), color::MUTED);
    }
    text(row, out, 12, 96, 1, "RAW 0..511", color::MUTED);
    text(row, out, 138, 96, 1, "ПИК", color::WARNING);
    text(row, out, 180, 96, 1, "ОБЗОР", color::ACCENT);
    if (GRAPH_TOP..=GRAPH_BOTTOM).contains(&row) {
        rect(
            row,
            out,
            GRAPH_X,
            GRAPH_TOP,
            GRAPH_WIDTH,
            GRAPH_BOTTOM - GRAPH_TOP + 1,
            color::PANEL,
        );
        for y in [GRAPH_TOP, 138, 166, 194, GRAPH_BOTTOM] {
            rect(row, out, GRAPH_X, y, GRAPH_WIDTH, 1, color::LINE);
        }
        for index in 0..usize::from(view.len) {
            let x = spectrum_bin_x(index, usize::from(view.len));
            if view.plot.valid(index) {
                let y = spectrum_level_y(view.plot.latest[index]);
                if row >= y {
                    let foreground = if view.plot.fresh(index) {
                        color::ACCENT
                    } else {
                        color::MUTED
                    };
                    out[x..x + 2].fill(foreground);
                }
            }
            if view.plot.peak_valid(index) && row == spectrum_level_y(view.plot.peaks[index]) {
                out[x..x + 2].fill(color::WARNING);
            }
        }
    }
    // A marker below the plot avoids repainting all its rows on every arrow.
    if (225..232).contains(&row) {
        let x = spectrum_bin_x(usize::from(cursor), usize::from(view.len));
        let spread = usize::from(row - 225) / 2;
        let start = x.saturating_sub(spread);
        let end = (x + 2 + spread).min(WIDTH);
        out[start..end].fill(color::ACCENT);
    }
    if (240..247).contains(&row) {
        let index = usize::from(cursor);
        let mut reading = Text::new();
        if view.plot.valid(index) {
            let _ = write!(
                reading,
                "RAW {:3}  {}",
                view.plot.latest[index],
                if view.plot.fresh(index) {
                    "ТЕКУЩИЙ ПРОХОД"
                } else {
                    "ПРОШЛЫЙ ПРОХОД"
                }
            );
        } else {
            let _ = reading.write_str("RAW ---  ЕЩЁ НЕ ИЗМЕРЕНО");
        }
        centered(row, out, 240, 1, reading.as_str(), color::TEXT);
    }
    if (254..261).contains(&row) {
        let mut info = Text::new();
        let step = view.step_10hz;
        let _ = write!(
            info,
            "{} ТОЧЕК  ШАГ {}.{:02}кГц",
            view.len,
            step / 100,
            step % 100
        );
        centered(row, out, 254, 1, info.as_str(), color::MUTED);
    }
    if model.view.notice == Notice::None {
        let (status, foreground) = match view.state {
            SpectrumState::Preparing => ("ПОДГОТОВКА ПРИЁМНИКА", color::WARNING),
            SpectrumState::Sweeping => ("↑↓ КУРСОР   ЗВУК ВЫКЛ", color::ACCENT),
            SpectrumState::Restoring => ("ВОССТАНОВЛЕНИЕ ПРИЁМА", color::WARNING),
            SpectrumState::Stopped => ("ОБЗОР ОСТАНОВЛЕН", color::MUTED),
            SpectrumState::Failed => ("ОШИБКА ОБЗОРА", color::BAD),
        };
        centered(row, out, 269, 1, status, foreground);
        centered(row, out, 283, 1, "PTT: СТОП, БЕЗ ПЕРЕДАЧИ", color::MUTED);
    }
}

fn home(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    slot_card(row, out, model, Slot::A);
    slot_card(row, out, model, Slot::B);
    radio_information(row, out, model);
    if model.view.notice == Notice::None {
        text(
            row,
            out,
            16,
            269,
            1,
            "ДОЛГО *: A/B   ↑↓: ВЫБОР",
            color::MUTED,
        );
        text(
            row,
            out,
            16,
            283,
            1,
            "ЦИФРЫ: ВВОД   EXIT: ОТМЕНА",
            color::MUTED,
        );
    }
}

fn slot_content<'a>(model: &Model<'a>, slot: Slot) -> PeerView<'a> {
    if slot == model.view.active_slot {
        PeerView {
            slot,
            mode: model.view.mode,
            frequency_10hz: home_frequency(model),
            channel: model.view.channel,
            name: model.channel.map(|channel| channel.name).unwrap_or(&[]),
            zone: model.view.zone,
            zone_name: model.zone_name,
            reverse: model.view.reverse,
        }
    } else {
        model
            .peer
            .filter(|peer| peer.slot == slot)
            .unwrap_or(PeerView {
                slot,
                mode: Mode::Vfo,
                frequency_10hz: None,
                channel: None,
                name: &[],
                zone: None,
                zone_name: None,
                reverse: false,
            })
    }
}

fn slot_card(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>, slot: Slot) {
    let (top, end) = home_slot_rows(slot);
    if !(top..end).contains(&row) {
        return;
    }
    let active = slot == model.view.active_slot;
    let content = slot_content(model, slot);
    let border = if active { color::ACCENT } else { color::LINE };
    rect(
        row,
        out,
        6,
        top,
        228,
        end - top,
        if active {
            color::PANEL
        } else {
            color::BACKGROUND
        },
    );
    rect(row, out, 6, top, 228, 1, border);
    rect(row, out, 6, end - 1, 228, 1, border);
    rect(row, out, 6, top, 1, end - top, border);
    rect(row, out, 233, top, 1, end - top, border);
    if active {
        rect(row, out, 14, top + 5, 16, 16, color::ACCENT);
    }
    symbol(
        row,
        out,
        14,
        top + 5,
        if slot == Slot::A {
            Symbol::SlotA
        } else {
            Symbol::SlotB
        },
        if active {
            color::BACKGROUND
        } else {
            color::MUTED
        },
    );
    if (top + 7..top + 14).contains(&row) {
        let mut mode = Text::new();
        match content.mode {
            Mode::Channel => {
                if let Some(index) = content.channel {
                    let _ = write!(mode, "MR {:04}", index + 1);
                } else {
                    let _ = mode.write_str("MR ----");
                }
            }
            Mode::Vfo => {
                let _ = mode.write_str("VFO");
            }
        }
        text(row, out, 38, top + 7, 1, mode.as_str(), color::TEXT);
        let mut zone = Text::new();
        if content.zone.is_none() {
            let _ = zone.write_str("ЗОНА: ВСЕ");
        } else if let Some(name) = content.zone_name {
            let decoded = channel_name(name);
            let _ = write!(zone, "ЗОНА: {}", decoded.as_str());
        } else {
            let _ = write!(zone, "ЗОНА: {}", u16::from(content.zone.unwrap()) + 1);
        }
        text(row, out, 96, top + 7, 1, zone.as_str(), color::MUTED);
    }
    let entry = if active { model.view.entry } else { None };
    if (top + 25..top + 32).contains(&row) {
        let decoded = channel_name(content.name);
        let label = if let Some(entry) = entry {
            match entry.kind {
                EntryKind::Channel => "ВВОД НОМЕРА КАНАЛА",
                EntryKind::Frequency => "ВВОД ЧАСТОТЫ",
            }
        } else if content.mode == Mode::Vfo {
            "РУЧНАЯ ЧАСТОТА"
        } else if content.channel.is_none() {
            "НЕТ КАНАЛА"
        } else {
            decoded.as_str()
        };
        text(row, out, 16, top + 25, 1, label, color::TEXT);
        if entry.is_none() {
            text(row, out, 206, top + 25, 1, "МГЦ", color::MUTED);
        }
    }
    if let Some(entry) = entry {
        if (top + 42..top + 63).contains(&row) {
            let value = entry_text(entry.value, entry.digits, entry.kind);
            centered(row, out, top + 42, 3, value.as_str(), color::ACCENT);
        }
        text(
            row,
            out,
            16,
            top + 73,
            1,
            "MENU: ПРИНЯТЬ  EXIT: ОТМЕНА",
            color::MUTED,
        );
        return;
    }
    if (top + 39..top + 67).contains(&row) {
        let value = content.frequency_10hz.map(frequency);
        centered(
            row,
            out,
            top + 39,
            4,
            value
                .as_ref()
                .map(FrequencyText::as_str)
                .unwrap_or("---.-----"),
            if active { color::ACCENT } else { color::TEXT },
        );
    }
    if active {
        if content.mode == Mode::Vfo && (top + 72..top + 79).contains(&row) {
            let mut step = Text::new();
            let _ = write!(step, "ШАГ {} КГЦ", step_label(model.view.step_index));
            text(row, out, 16, top + 72, 1, step.as_str(), color::MUTED);
        }
        if (top + 70..top + 81).contains(&row) {
            ham_status(104 + row - (top + 70), out, model);
        }
    } else {
        text(
            row,
            out,
            16,
            top + 73,
            1,
            if content.reverse {
                "R / ДОЛГО *: ВЫБРАТЬ"
            } else {
                "ДОЛГО *: ВЫБРАТЬ"
            },
            color::MUTED,
        );
    }
}

fn home_frequency(model: &Model<'_>) -> Option<u32> {
    match model.view.mode {
        Mode::Channel => model
            .channel
            .filter(|channel| channel.active)
            .map(|channel| {
                let (rx, tx, _, _) = displayed_channel(channel, model.view.reverse);
                if model.radio.state == RadioState::Transmitting {
                    tx
                } else {
                    rx
                }
            }),
        Mode::Vfo => Some(model.view.vfo_10hz),
    }
}

fn slot_label(slot: Slot) -> &'static str {
    match slot {
        Slot::A => "A",
        Slot::B => "B",
    }
}

fn radio_information(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    if !(242..265).contains(&row) {
        return;
    }
    if model.view.entry.is_some() {
        text(
            row,
            out,
            16,
            245,
            1,
            "MENU: ПОДТВЕРДИТЬ ВВОД",
            color::ACCENT,
        );
        text(
            row,
            out,
            16,
            258,
            1,
            "EXIT: ОТМЕНИТЬ БЕЗ ИЗМЕНЕНИЙ",
            color::MUTED,
        );
        return;
    }
    if (245..252).contains(&row) {
        let mut status = Text::new();
        match model.view.scan {
            ScanState::Requested => {
                let _ = status.write_str("СКАН: ЗАПРОС ЗАПУСКА");
            }
            ScanState::Running => {
                let _ = status.write_str("СКАН: РАБОТАЕТ");
            }
            ScanState::Stopping => {
                let _ = status.write_str("СКАН: ОСТАНОВКА");
            }
            ScanState::Stopped => {
                if matches!(
                    model.radio.state,
                    RadioState::Receiving | RadioState::Transmitting
                ) {
                    let _ = write!(
                        status,
                        "{}  {}/{}  ШАГ {}К",
                        slot_label(model.view.active_slot),
                        if model.radio.modulation == Modulation::Am {
                            "AM"
                        } else {
                            "FM"
                        },
                        match model.radio.route {
                            Some(RadioRoute::Hf) => "HF",
                            Some(RadioRoute::Vhf) => "VHF",
                            Some(RadioRoute::Uhf) => "UHF",
                            Some(RadioRoute::Uhf800) => "800",
                            None => "?",
                        },
                        step_label(model.view.step_index)
                    );
                } else {
                    let _ = write!(
                        status,
                        "ВЫБРАН {} / ШАГ {} КГЦ",
                        slot_label(model.view.active_slot),
                        step_label(model.view.step_index)
                    );
                }
            }
        }
        text(row, out, 16, 245, 1, status.as_str(), color::TEXT);
    }
    if (258..265).contains(&row) {
        let mut status = Text::new();
        if model.radio.tx_trial && !model.radio.monitor {
            let _ = status.write_str("TX TEST: CH002 / A / 5S");
        } else if model.radio.state == RadioState::Transmitting {
            let _ = write!(
                status,
                "TX МОЩНОСТЬ {}",
                model.radio.power.map(power_label).unwrap_or("?")
            );
        } else if model.radio.state == RadioState::Receiving {
            match model.radio.rssi {
                Some(raw) => {
                    let _ = write!(status, "RSSI RAW {}", raw);
                }
                None => {
                    let _ = status.write_str("RSSI: НЕТ ДАННЫХ");
                }
            }
            if model.radio.monitor {
                let _ = status.write_str(" / МОНИТОР ВКЛ");
            }
        } else {
            let _ = status.write_str("↑↓: ВЫБОР  ЦИФРЫ: ВВОД");
        }
        text(
            row,
            out,
            16,
            258,
            1,
            status.as_str(),
            if (model.radio.tx_trial && !model.radio.monitor)
                || (model.radio.state == RadioState::Receiving && model.radio.monitor)
            {
                color::WARNING
            } else {
                color::MUTED
            },
        );
    }
    if model.view.scan == ScanState::Running {
        symbol(row, out, 208, 242, Symbol::Scan, color::GOOD);
    }
}

/// Small code-native symbols inspired by conventional radio notation. They
/// identify available menu functions; live indicators still use actual state.
#[derive(Clone, Copy)]
enum Symbol {
    Bank,
    Channels,
    Scan,
    Monitor,
    Power,
    Reverse,
    Wave,
    Settings,
    Lock,
    SlotA,
    SlotB,
}

fn symbol(row: u16, out: &mut [u16; WIDTH], x: usize, y: u16, symbol: Symbol, color: u16) {
    if !(y..y + 16).contains(&row) {
        return;
    }
    if matches!(symbol, Symbol::SlotA | Symbol::SlotB) {
        rect(row, out, x, y, 16, 1, color);
        rect(row, out, x, y + 15, 16, 1, color);
        rect(row, out, x, y, 1, 16, color);
        rect(row, out, x + 15, y, 1, 16, color);
        text(
            row,
            out,
            x + 5,
            y + 5,
            1,
            if matches!(symbol, Symbol::SlotA) {
                "A"
            } else {
                "B"
            },
            color,
        );
        return;
    }
    let rows: [u16; 16] = match symbol {
        Symbol::Bank => [
            0, 0x7c00, 0x4200, 0x41fc, 0x4004, 0x7ffe, 0x4002, 0x4002, 0x4002, 0x4002, 0x4002,
            0x4002, 0x4002, 0x7ffe, 0, 0,
        ],
        Symbol::Channels => [
            0, 0x7ffe, 0x4002, 0x5ffa, 0x4002, 0x4002, 0x5ffa, 0x4002, 0x4002, 0x5ffa, 0x4002,
            0x4002, 0x5ffa, 0x4002, 0x7ffe, 0,
        ],
        Symbol::Scan => [
            0, 0x07c0, 0x1830, 0x2008, 0x4004, 0x4004, 0x401c, 0x0014, 0x2800, 0x3802, 0x2002,
            0x2002, 0x1004, 0x0c18, 0x03e0, 0,
        ],
        Symbol::Monitor => [
            0, 0x0100, 0x0308, 0x0510, 0x7912, 0x4124, 0x4124, 0x4124, 0x4124, 0x4124, 0x7912,
            0x0510, 0x0308, 0x0100, 0, 0,
        ],
        Symbol::Power => [
            0, 0x0380, 0x0300, 0x0600, 0x0c00, 0x1800, 0x3ff0, 0x0070, 0x00e0, 0x01c0, 0x0380,
            0x0700, 0x0e00, 0x0c00, 0x0800, 0,
        ],
        Symbol::Reverse => [
            0, 0, 0x0010, 0x0018, 0x7ffc, 0x0018, 0x0010, 0, 0, 0x0800, 0x1800, 0x3ffe, 0x1800,
            0x0800, 0, 0,
        ],
        Symbol::Wave => [
            0, 0, 0, 0, 0x0c00, 0x1200, 0x2100, 0x4081, 0x8042, 0x0024, 0x0018, 0, 0, 0, 0, 0,
        ],
        Symbol::Settings => [
            0, 0x0100, 0x0100, 0x7ffc, 0x0100, 0, 0x0020, 0x0020, 0x7ffc, 0x0020, 0, 0x0800,
            0x0800, 0x7ffc, 0x0800, 0,
        ],
        Symbol::Lock => [
            0, 0x03c0, 0x0660, 0x0420, 0x0420, 0x0420, 0x1ff8, 0x1008, 0x1008, 0x1188, 0x1188,
            0x1088, 0x1008, 0x1ff8, 0, 0,
        ],
        Symbol::SlotA | Symbol::SlotB => [0; 16],
    };
    let bits = rows[usize::from(row - y)];
    for column in 0..16 {
        if bits & (1 << (15 - column)) != 0 && x + column < WIDTH {
            out[x + column] = color;
        }
    }
}

fn menu_start(index: u8) -> usize {
    usize::from(index) / LIST_ROWS * LIST_ROWS
}

fn setting_title(setting: Setting) -> &'static str {
    match setting {
        Setting::Mode => "РЕЖИМ КАНАЛ / VFO",
        Setting::Power => "МОЩНОСТЬ",
        Setting::Monitor => "МОНИТОР",
        Setting::Modulation => "РЕЖИМ FM / AM",
        Setting::Reverse => "РЕВЕРС RX / TX",
        Setting::ActiveSlot => "АКТИВНЫЙ СЛОТ",
        Setting::Scan => "СКАНИРОВАНИЕ",
        Setting::DisplayLight => "ПОДСВЕТКА ЭКРАНА",
        Setting::KeyLight => "ПОДСВЕТКА КНОПОК",
    }
}

fn setting_options(setting: Setting) -> &'static [&'static str] {
    match setting {
        Setting::Mode => &["КАНАЛЫ", "ЧАСТОТА / VFO"],
        Setting::Power => &["НИЗКАЯ (L)", "СРЕДНЯЯ (M)", "ВЫСОКАЯ (H)"],
        Setting::Monitor | Setting::Reverse => &["ВЫКЛЮЧЕН", "ВКЛЮЧЕН"],
        Setting::Modulation => &["FM", "AM"],
        Setting::ActiveSlot => &["СЛОТ A", "СЛОТ B"],
        Setting::Scan => &["ОСТАНОВИТЬ", "ЗАПУСТИТЬ"],
        Setting::DisplayLight | Setting::KeyLight => &[
            "ВЫКЛЮЧЕНА",
            "ВКЛЮЧЕНА",
            "5 СЕКУНД",
            "10 СЕКУНД",
            "20 СЕКУНД",
            "30 СЕКУНД",
            "60 СЕКУНД",
        ],
    }
}

fn setting_geometry(setting: Setting) -> (u16, u16, u16) {
    if matches!(setting, Setting::DisplayLight | Setting::KeyLight) {
        (100, 26, 22)
    } else {
        (SETTING_Y, SETTING_ROW_HEIGHT, SETTING_PAINT_HEIGHT)
    }
}

fn light_index(mode: LightMode) -> usize {
    LIGHT_MODES
        .iter()
        .position(|&candidate| candidate == mode)
        .unwrap()
}

fn setting_uses_radio(setting: Option<Setting>) -> bool {
    matches!(
        setting,
        Some(Setting::Power | Setting::Monitor | Setting::Modulation)
    )
}

/// This is the committed value, never the highlighted draft. For RF settings
/// the backend must report an active state before the UI calls it confirmed.
fn current_setting_label(model: &Model<'_>, setting: Setting) -> &'static str {
    const UNKNOWN: &str = "НЕТ ПОДТВЕРЖДЕНИЯ";
    if setting_uses_radio(Some(setting))
        && !matches!(
            model.radio.state,
            RadioState::Receiving | RadioState::Transmitting
        )
    {
        return UNKNOWN;
    }
    let index = match setting {
        Setting::Mode => usize::from(model.view.mode == Mode::Vfo),
        Setting::Power => match model.radio.power {
            Some(power) => usize::from(power),
            None => return UNKNOWN,
        },
        Setting::Monitor => usize::from(model.radio.monitor),
        Setting::Modulation => usize::from(model.radio.modulation == Modulation::Am),
        Setting::Reverse => usize::from(model.view.reverse),
        Setting::ActiveSlot => usize::from(model.view.active_slot == Slot::B),
        Setting::DisplayLight => light_index(model.view.display_light),
        Setting::KeyLight => light_index(model.view.key_light),
        Setting::Scan => {
            return match model.view.scan {
                ScanState::Stopped => "ОСТАНОВЛЕН",
                ScanState::Requested => "ЗАПРОС ЗАПУСКА",
                ScanState::Running => "СКАНИРУЕТ",
                ScanState::Stopping => "ОСТАНОВКА",
            }
        }
    };
    setting_options(setting)
        .get(index)
        .copied()
        .unwrap_or(UNKNOWN)
}

fn setting(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let Some(setting) = model.view.setting else {
        return;
    };
    text(row, out, 16, 62, 1, "ВЫБЕРИТЕ ЗНАЧЕНИЕ", color::ACCENT);
    if (84..91).contains(&row) {
        let mut current = Text::new();
        let _ = write!(current, "СЕЙЧАС: {}", current_setting_label(model, setting));
        text(row, out, 16, 84, 1, current.as_str(), color::MUTED);
    }
    let (start, pitch, height) = setting_geometry(setting);
    for (index, &label) in setting_options(setting).iter().enumerate() {
        let y = start + index as u16 * pitch;
        if !(y..y + height).contains(&row) {
            continue;
        }
        let selected = index == usize::from(model.view.setting_index);
        rect(
            row,
            out,
            12,
            y,
            216,
            height,
            if selected {
                color::ACCENT
            } else {
                color::PANEL
            },
        );
        text(
            row,
            out,
            24,
            y + (height - 14) / 2,
            2,
            label,
            item_color(selected),
        );
    }
    if setting.choice_count() > 3 {
        return;
    }
    text(
        row,
        out,
        16,
        252,
        1,
        "ПРИМЕНЕНИЕ: ТОЛЬКО MENU",
        color::MUTED,
    );
}

pub fn menu_label(item: MenuItem) -> &'static str {
    match item {
        MenuItem::Mode => "КАНАЛ / ЧАСТОТА",
        MenuItem::Channels => "СПИСОК КАНАЛОВ",
        MenuItem::ChannelDetails => "ДАННЫЕ КАНАЛА",
        MenuItem::Step => "ШАГ ЧАСТОТЫ",
        MenuItem::Calibration => "КАЛИБРОВКИ",
        MenuItem::Keys => "ПРОВЕРКА КНОПОК",
        MenuItem::Help => "УПРАВЛЕНИЕ",
        MenuItem::About => "О ПРОШИВКЕ",
        MenuItem::Reverse => "РЕВЕРС RX / TX",
        MenuItem::Scan => "СКАНИРОВАНИЕ",
        MenuItem::Shortcuts => "ДОЛГИЕ КЛАВИШИ",
        MenuItem::Power => "МОЩНОСТЬ L/M/H",
        MenuItem::Monitor => "МОНИТОР",
        MenuItem::Modulation => "РЕЖИМ FM / AM",
        MenuItem::Zones => "ЗОНЫ КАНАЛОВ",
        MenuItem::ActiveSlot => "ВЫБОР A / B",
        MenuItem::SideKeys => "БОКОВЫЕ КНОПКИ",
        MenuItem::DisplayLight => "СВЕТ ЭКРАНА",
        MenuItem::KeyLight => "СВЕТ КНОПОК",
        MenuItem::Spectrum => "СПЕКТР ЧАСТОТ",
    }
}

pub fn menu_group_label(group: MenuGroup) -> &'static str {
    match group {
        MenuGroup::Mode => "КАНАЛЫ / VFO",
        MenuGroup::Receive => "ПРИЁМ И TX",
        MenuGroup::Scanning => "ПОИСК / СПЕКТР",
        MenuGroup::Keys => "КНОПКИ",
        MenuGroup::System => "СВЕТ / СИСТЕМА",
    }
}

fn menu(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let start = menu_start(model.view.menu_index);
    let entries = menu_entries(model.view.menu_group);
    for (offset, &entry) in entries.iter().skip(start).take(LIST_ROWS).enumerate() {
        let y = LIST_Y + offset as u16 * LIST_ROW_HEIGHT;
        if row < y || row >= y + LIST_ROW_HEIGHT {
            continue;
        }
        let selected = start + offset == usize::from(model.view.menu_index);
        item_background(row, out, y, selected);
        let item = match entry {
            MenuEntry::Item(item) => Some(item),
            MenuEntry::Group(_) => None,
        };
        let glyph = match item {
            Some(MenuItem::Mode | MenuItem::Modulation) => Symbol::Wave,
            Some(MenuItem::Channels | MenuItem::ChannelDetails) => Symbol::Channels,
            Some(MenuItem::Zones) => Symbol::Bank,
            Some(MenuItem::Scan | MenuItem::Spectrum) => Symbol::Scan,
            Some(MenuItem::Monitor) => Symbol::Monitor,
            Some(MenuItem::Power) => Symbol::Power,
            Some(MenuItem::Reverse) => Symbol::Reverse,
            Some(MenuItem::ActiveSlot) => {
                if model.view.active_slot == Slot::A {
                    Symbol::SlotA
                } else {
                    Symbol::SlotB
                }
            }
            None => match entry {
                MenuEntry::Group(MenuGroup::Mode) => Symbol::Channels,
                MenuEntry::Group(MenuGroup::Receive) => Symbol::Monitor,
                MenuEntry::Group(MenuGroup::Scanning) => Symbol::Scan,
                _ => Symbol::Settings,
            },
            _ => Symbol::Settings,
        };
        symbol(
            row,
            out,
            20,
            y + 7,
            glyph,
            if selected {
                color::BACKGROUND
            } else {
                color::ACCENT
            },
        );
        text(
            row,
            out,
            44,
            y + 8,
            2,
            match entry {
                MenuEntry::Item(item) => menu_label(item),
                MenuEntry::Group(group) => menu_group_label(group),
            },
            item_color(selected),
        );
        if matches!(entry, MenuEntry::Group(_)) {
            text(row, out, 216, y + 8, 2, ">", item_color(selected));
        }
    }
    if (283..290).contains(&row) {
        let mut position = Text::new();
        let _ = write!(
            position,
            "{} / {}",
            model.view.menu_index + 1,
            entries.len()
        );
        text(row, out, 192, 283, 1, position.as_str(), color::MUTED);
    }
}

pub fn shortcut_label(action: ShortcutAction) -> &'static str {
    match action {
        ShortcutAction::None => "НЕ НАЗНАЧЕНО",
        ShortcutAction::Power => "МОЩНОСТЬ L/M/H",
        ShortcutAction::Monitor => "МОНИТОР",
        ShortcutAction::Scan => "СКАНИРОВАНИЕ",
        ShortcutAction::Reverse => "РЕВЕРС RX / TX",
        ShortcutAction::Mode => "КАНАЛ / VFO",
        ShortcutAction::Step => "ШАГ ЧАСТОТЫ",
        ShortcutAction::Channels => "СПИСОК КАНАЛОВ",
        ShortcutAction::Keys => "ПРОВЕРКА КНОПОК",
        ShortcutAction::Lock => "БЛОКИРОВКА",
        ShortcutAction::Modulation => "РЕЖИМ FM / AM",
        ShortcutAction::Zones => "ЗОНЫ КАНАЛОВ",
        ShortcutAction::SwitchSlot => "ВЫБОР A / B",
        ShortcutAction::Spectrum => "СПЕКТР ЧАСТОТ",
    }
}

fn shortcuts(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let index = model.view.binding_index;
    let start = menu_start(index);
    for (offset, &action) in model
        .view
        .bindings
        .iter()
        .skip(start)
        .take(LIST_ROWS)
        .enumerate()
    {
        let y = LIST_Y + offset as u16 * LIST_ROW_HEIGHT;
        if row < y || row >= y + LIST_ROW_HEIGHT {
            continue;
        }
        let selected = start + offset == usize::from(index);
        item_background(row, out, y, selected);
        let mut number = Text::new();
        let _ = write!(
            number,
            "{}",
            binding_digit((start + offset) as u8).unwrap_or(0)
        );
        text(
            row,
            out,
            18,
            y + 8,
            2,
            number.as_str(),
            item_color(selected),
        );
        text(
            row,
            out,
            48,
            y + 8,
            2,
            shortcut_label(action),
            item_color(selected),
        );
    }
    if model.view.notice == Notice::None && (283..290).contains(&row) {
        text(row, out, 16, 283, 1, "ДО ВЫКЛЮЧЕНИЯ", color::MUTED);
        let mut count = Text::new();
        let _ = write!(count, "{} / {}", index + 1, model.view.bindings.len());
        text(row, out, 182, 283, 1, count.as_str(), color::MUTED);
    }
}

fn shortcut_actions(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let index = model.view.binding_action_index;
    let start = menu_start(index);
    for (offset, &action) in SHORTCUT_ACTIONS
        .iter()
        .skip(start)
        .take(LIST_ROWS)
        .enumerate()
    {
        let y = LIST_Y + offset as u16 * LIST_ROW_HEIGHT;
        if row < y || row >= y + LIST_ROW_HEIGHT {
            continue;
        }
        let selected = start + offset == usize::from(index);
        item_background(row, out, y, selected);
        text(
            row,
            out,
            21,
            y + 8,
            2,
            shortcut_label(action),
            item_color(selected),
        );
    }
    if model.view.notice == Notice::None && (283..290).contains(&row) {
        let mut key = Text::new();
        let _ = write!(
            key,
            "ДОЛГО {}: ВЫБЕРИТЕ ФУНКЦИЮ",
            binding_digit(model.view.binding_index).unwrap_or(0)
        );
        text(row, out, 16, 283, 1, key.as_str(), color::MUTED);
    }
}

fn zones(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    if model.zones.is_empty() {
        centered(row, out, 128, 2, "НЕТ СПИСКА ЗОН", color::TEXT);
        return;
    }
    for (offset, zone) in model.zones.iter().enumerate() {
        let y = LIST_Y + offset as u16 * LIST_ROW_HEIGHT;
        if row < y || row >= y + LIST_ROW_HEIGHT {
            continue;
        }
        let selected = zone.index == model.view.zone_cursor;
        item_background(row, out, y, selected);
        let name = channel_name(zone.name);
        text(
            row,
            out,
            16,
            y + 4,
            2,
            if zone.index.is_none() {
                "ВСЕ КАНАЛЫ"
            } else {
                name.as_str()
            },
            item_color(selected),
        );
        let mut count = Text::new();
        let _ = write!(count, "КАНАЛОВ: {}", zone.channel_count);
        text(
            row,
            out,
            16,
            y + 22,
            1,
            count.as_str(),
            item_color(selected),
        );
    }
    if model.view.notice == Notice::None {
        text(
            row,
            out,
            16,
            283,
            1,
            "ЗОНА ФИЛЬТРУЕТ СПИСОК КАНАЛОВ",
            color::MUTED,
        );
    }
}

pub fn side_action_label(action: SideAction) -> &'static str {
    match action {
        SideAction::PttA => "PTT A",
        SideAction::PttB => "PTT B",
        SideAction::Active => "PTT ТЕКУЩИЙ A/B",
        SideAction::Monitor => "МОНИТОР",
        SideAction::Scan => "СКАНИРОВАНИЕ",
        SideAction::Power => "МОЩНОСТЬ L/M/H",
        SideAction::None => "НЕ НАЗНАЧЕНО",
    }
}

fn side_keys(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    for (index, &action) in model.view.side_bindings.iter().enumerate() {
        let y = LIST_Y + index as u16 * LIST_ROW_HEIGHT;
        if row < y || row >= y + LIST_ROW_HEIGHT {
            continue;
        }
        let selected = usize::from(model.view.side_index) == index;
        item_background(row, out, y, selected);
        text(
            row,
            out,
            16,
            y + 4,
            2,
            if index == 0 {
                "ВЕРХНЯЯ КНОПКА"
            } else {
                "НИЖНЯЯ КНОПКА"
            },
            item_color(selected),
        );
        text(
            row,
            out,
            16,
            y + 22,
            1,
            side_action_label(action),
            item_color(selected),
        );
    }
    for (y, label) in [
        (154, "PTT A / PTT B: СЛОТ ПЕРЕДАЧИ"),
        (178, "PTT ТЕКУЩИЙ: АКТИВНЫЙ A/B"),
        (210, "ДОЛГО *: ПЕРЕКЛЮЧИТЬ A/B"),
        (248, "ИЗМЕНЕНИЯ ДО ВЫКЛЮЧЕНИЯ"),
    ] {
        text(row, out, 16, y, 1, label, color::MUTED);
    }
}

fn side_actions(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    for (index, &action) in SIDE_ACTIONS.iter().enumerate() {
        let y = LIST_Y + index as u16 * LIST_ROW_HEIGHT;
        if row < y || row >= y + LIST_ROW_HEIGHT {
            continue;
        }
        let selected = usize::from(model.view.side_action_index) == index;
        item_background(row, out, y, selected);
        text(
            row,
            out,
            21,
            y + 8,
            2,
            side_action_label(action),
            item_color(selected),
        );
    }
    if model.view.notice == Notice::None {
        text(
            row,
            out,
            16,
            283,
            1,
            if model.view.side_index == 0 {
                "НАЗНАЧЕНИЕ ВЕРХНЕЙ КНОПКИ"
            } else {
                "НАЗНАЧЕНИЕ НИЖНЕЙ КНОПКИ"
            },
            color::MUTED,
        );
    }
}

fn channels(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    if model.list.is_empty() {
        centered(row, out, 128, 2, "СПИСОК ПУСТ", color::TEXT);
        centered(row, out, 163, 1, "СОХРАНЁННЫХ КАНАЛОВ НЕТ", color::MUTED);
        return;
    }
    for (offset, channel) in model.list.iter().enumerate() {
        let y = LIST_Y + offset as u16 * LIST_ROW_HEIGHT;
        if row < y || row >= y + LIST_ROW_HEIGHT {
            continue;
        }
        let selected = model.view.list_channel == Some(channel.index as u16);
        item_background(row, out, y, selected);
        if (y + 7..y + 14).contains(&row) {
            let mut number = Text::new();
            let _ = write!(number, "{:04}", channel.index + 1);
            text(
                row,
                out,
                15,
                y + 7,
                1,
                number.as_str(),
                item_color(selected),
            );
        }
        if (y + 3..y + 17).contains(&row) {
            let name = channel_name(channel.name);
            let scale = if text_width(name.as_str(), 2) <= 184 {
                2
            } else {
                1
            };
            text(
                row,
                out,
                44,
                y + 3,
                scale,
                name.as_str(),
                item_color(selected),
            );
        }
        if (y + 22..y + 29).contains(&row) {
            let value = frequency(channel.rx_10hz);
            text(
                row,
                out,
                44,
                y + 22,
                1,
                value.as_str(),
                item_color(selected),
            );
            text(row, out, 108, y + 22, 1, "МГЦ", item_color(selected));
        }
    }
}

fn details(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    let Some(channel) = model.channel else {
        centered(row, out, 128, 2, "КАНАЛ НЕ ВЫБРАН", color::TEXT);
        return;
    };
    let reversed = model.view.mode == Mode::Channel && model.view.reverse;
    let (rx, tx, rx_tone, tx_tone) = displayed_channel(channel, reversed);
    if (60..67).contains(&row) {
        let mut number = Text::new();
        let _ = write!(
            number,
            "КАНАЛ {:04}{}",
            channel.index + 1,
            if reversed { " / R РЕВЕРС" } else { "" }
        );
        text(row, out, 16, 60, 1, number.as_str(), color::MUTED);
    }
    if (80..94).contains(&row) {
        let name = channel_name(channel.name);
        text(row, out, 16, 80, 2, name.as_str(), color::TEXT);
    }
    text(row, out, 16, 112, 1, "ПРИЁМ / МГЦ", color::MUTED);
    if (130..151).contains(&row) {
        text(row, out, 16, 130, 3, frequency(rx).as_str(), color::ACCENT);
    }
    if (158..165).contains(&row) {
        text(
            row,
            out,
            16,
            158,
            1,
            tone_text(rx_tone).as_str(),
            color::TEXT,
        );
    }
    text(row, out, 16, 185, 1, "ПЕРЕДАЧА / МГЦ", color::MUTED);
    if (203..224).contains(&row) {
        text(row, out, 16, 203, 3, frequency(tx).as_str(), color::TEXT);
    }
    if (231..238).contains(&row) {
        text(
            row,
            out,
            16,
            231,
            1,
            tone_text(tx_tone).as_str(),
            color::TEXT,
        );
    }
    if (260..267).contains(&row) {
        let offset = offset_text(rx, tx);
        text(row, out, 16, 260, 1, offset.as_str(), color::MUTED);
    }
    if (281..288).contains(&row) {
        let mut flags = Text::new();
        let _ = write!(
            flags,
            "ПОЛОСА {} / МОЩНОСТЬ {}",
            bandwidth_label(channel.bandwidth_raw),
            power_label(channel.power_raw)
        );
        text(row, out, 16, 281, 1, flags.as_str(), color::MUTED);
    }
}

pub fn step_label(index: u8) -> &'static str {
    match index {
        0 => "2.5",
        1 => "5",
        2 => "6.25",
        3 => "10",
        4 => "12.5",
        5 => "25",
        _ => "?",
    }
}

fn steps(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    for index in 0..STEP_10HZ.len() {
        let y = 62 + index as u16 * 33;
        if row < y || row >= y + LIST_ROW_HEIGHT {
            continue;
        }
        let selected = usize::from(model.view.step_index) == index;
        item_background(row, out, y, selected);
        if row < y + 8 || row >= y + 22 {
            continue;
        }
        let mut label = Text::new();
        let _ = write!(label, "{} КГЦ", step_label(index as u8));
        text(row, out, 24, y + 8, 2, label.as_str(), item_color(selected));
    }
    text(row, out, 16, 281, 1, "ШАГ РУЧНОГО РЕЖИМА VFO", color::MUTED);
}

fn calibration(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    text(row, out, 16, 68, 2, "ПРОВЕРКА БЭКАПА", color::TEXT);
    text(row, out, 16, 107, 1, "4096 БАЙТ / SHA-256", color::MUTED);
    let (label, foreground) = match model.calibration_matches {
        Some(true) => ("ДАННЫЕ СОВПАЛИ", color::GOOD),
        Some(false) => ("НЕ СОВПАДАЕТ", color::BAD),
        None => ("НЕ ПРОВЕРЕНО", color::WARNING),
    };
    rect(row, out, 12, 132, 216, 52, color::PANEL);
    centered(row, out, 151, 2, label, foreground);
    text(
        row,
        out,
        16,
        214,
        1,
        "СРАВНЕНИЕ С ИСХОДНЫМ БЭКАПОМ",
        color::MUTED,
    );
    text(row, out, 16, 249, 1, "КАЛИБРОВКИ НЕ", color::MUTED);
    text(row, out, 16, 268, 1, "ПЕРЕЗАПИСЫВАЮТСЯ", color::MUTED);
}

pub fn key_label(key: Key) -> &'static str {
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
        Key::Up => "ВВЕРХ",
        Key::Down => "ВНИЗ",
        Key::Exit => "EXIT",
        Key::Star => "*",
        Key::Hash => "#",
        Key::Ptt => "PTT",
        Key::Aux16 => "БОКОВАЯ 1",
        Key::Aux17 => "БОКОВАЯ 2",
        Key::Aux18 => "БОКОВАЯ 3",
    }
}

fn keys(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    text(row, out, 16, 68, 1, "ПОСЛЕДНЯЯ КНОПКА", color::MUTED);
    let key = model.last_key.or(model.view.last_key);
    centered(
        row,
        out,
        101,
        3,
        key.map(key_label).unwrap_or("НАЖМИТЕ"),
        color::ACCENT,
    );
    rect(row, out, 16, 147, 208, 1, color::LINE);
    text(row, out, 16, 170, 2, "PTT", color::TEXT);
    text(
        row,
        out,
        74,
        170,
        2,
        if model.ptt_pressed || model.view.ptt_down {
            "НАЖАТА"
        } else {
            "ОТПУЩЕНА"
        },
        color::TEXT,
    );
    for (index, &pressed) in model.view.side_down.iter().enumerate() {
        let y = 191 + index as u16 * 11;
        if row < y || row >= y + 7 {
            continue;
        }
        let mut label = Text::new();
        let _ = write!(
            label,
            "{}: {}",
            if index == 0 {
                "ВЕРХНЯЯ"
            } else {
                "НИЖНЯЯ"
            },
            if pressed {
                "НАЖАТА"
            } else {
                "ОТПУЩЕНА"
            }
        );
        text(row, out, 16, y, 1, label.as_str(), color::MUTED);
    }
    text(
        row,
        out,
        16,
        211,
        1,
        if model.radio.state == RadioState::Transmitting {
            "TX ПОДТВЕРЖДЕН РАДИО"
        } else {
            "PTT: КНОПКА, НЕ СТАТУС TX"
        },
        color::MUTED,
    );
    text(
        row,
        out,
        16,
        248,
        1,
        if model.input_ready {
            "ГОТОВО К НАЖАТИЮ"
        } else {
            "ОТПУСТИТЕ ВСЕ КНОПКИ"
        },
        color::MUTED,
    );
}

fn help(row: u16, out: &mut [u16; WIDTH]) {
    for (index, (key, meaning)) in [
        ("↑ / ↓", "ВЫБОР / ЧАСТОТА"),
        ("MENU", "МЕНЮ / ПОДТВЕРДИТЬ"),
        ("EXIT", "НАЗАД / ОТМЕНА ВВОДА"),
        ("*", "CH/VFO; ДОЛГО: A/B"),
        ("#", "ШАГ; ДОЛГО: БЛОК"),
        ("0...9", "НОМЕР / ЧАСТОТА"),
        ("0-9", "ДОЛГО: БЫСТРАЯ ФУНКЦИЯ"),
    ]
    .iter()
    .enumerate()
    {
        let y = 56 + index as u16 * 24;
        text(row, out, 16, y, 2, key, color::ACCENT);
        text(row, out, 88, y + 4, 1, meaning, color::TEXT);
    }
    text(
        row,
        out,
        16,
        216,
        1,
        "ДОЛГО 0: СПЕКТР (МОЖНО ИЗМЕНИТЬ)",
        color::ACCENT,
    );
    text(row, out, 16, 232, 1, "R РЕВЕРС  +/- СДВИГ TX", color::MUTED);
    text(
        row,
        out,
        16,
        248,
        1,
        "CT CTCSS / DCS ЦИФРОВОЙ ТОН",
        color::MUTED,
    );
    text(
        row,
        out,
        16,
        264,
        1,
        "W/N ПОЛОСА / L/M/H МОЩНОСТЬ",
        color::MUTED,
    );
    text(
        row,
        out,
        16,
        284,
        1,
        "ДОЛГО EXIT: ДОМОЙ / ОТМЕНА",
        color::MUTED,
    );
}

fn about(row: u16, out: &mut [u16; WIDTH]) {
    text(row, out, 16, 68, 3, "UV98 RUST", color::ACCENT);
    text(row, out, 16, 112, 2, "ВЕРСИЯ 0.1.8-r4", color::TEXT);
    for (y, label) in [
        (156, "КАНАЛЫ / ЧАСТОТЫ / МЕНЮ"),
        (180, "СТАТУС РАДИО В ВЕРХНЕЙ СТРОКЕ"),
        (204, "КАЛИБРОВКИ ТОЛЬКО ЧТЕНИЕ"),
        (248, "НАСТРОЙКИ СБРОСЯТСЯ"),
        (270, "ПРИ ВЫКЛЮЧЕНИИ"),
    ] {
        text(row, out, 16, y, 1, label, color::MUTED);
    }
}

fn notice(row: u16, out: &mut [u16; WIDTH], value: Notice) {
    let label = match value {
        Notice::None => return,
        Notice::InvalidChannel => "НОМЕР КАНАЛА: ОТ 1 ДО 1024",
        Notice::InactiveChannel => "В ЭТОМ КАНАЛЕ НЕТ ДАННЫХ",
        Notice::InvalidFrequency => "ЧАСТОТА: 000.10000 - 999.99999",
        Notice::EntryFull => "ЛИМИТ ЦИФР / MENU: ПРИНЯТЬ",
        Notice::Locked => "БЛОКИРОВКА: УДЕРЖИВАЙТЕ #",
        Notice::TxDisabled => "ПЕРЕДАЧА ВЫКЛЮЧЕНА",
        Notice::NoChannels => "СОХРАНЁННЫХ КАНАЛОВ НЕТ",
        Notice::ChannelOnly => "РЕВЕРС: ВЫБЕРИТЕ КАНАЛ",
        Notice::RadioUnavailable => "РАДИОФУНКЦИЯ НЕДОСТУПНА",
    };
    rect(row, out, 0, 270, WIDTH, 25, color::WARNING_BACKGROUND);
    centered(row, out, 279, 1, label, color::WARNING);
}

fn footer(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    rect(row, out, 0, 296, WIDTH, 24, color::PANEL);
    rect(row, out, 0, 296, WIDTH, 1, color::LINE);
    let (left, right) = if model.view.locked {
        ("УДЕРЖИВАТЬ #", "РАЗБЛОКИРОВАТЬ")
    } else if model.view.entry.is_some() {
        ("MENU ПРИНЯТЬ", "EXIT ОТМЕНА")
    } else {
        match model.view.screen {
            Screen::Home if model.view.scan != ScanState::Stopped => ("MENU МЕНЮ", "EXIT СТОП"),
            Screen::Home if model.radio.monitor => ("MENU МЕНЮ", "EXIT ВЫКЛ"),
            Screen::Home => ("MENU МЕНЮ", "# ШАГ   * РЕЖИМ"),
            Screen::Menu | Screen::Channels | Screen::Step => ("MENU ВЫБРАТЬ", "EXIT НАЗАД"),
            Screen::Shortcuts => ("MENU ИЗМЕНИТЬ", "EXIT НАЗАД"),
            Screen::ShortcutAction => ("MENU ПРИНЯТЬ", "EXIT ОТМЕНА"),
            Screen::Zones => ("MENU ВЫБРАТЬ", "EXIT НАЗАД"),
            Screen::SideKeys => ("MENU ИЗМЕНИТЬ", "EXIT НАЗАД"),
            Screen::SideAction => ("MENU ПРИНЯТЬ", "EXIT ОТМЕНА"),
            Screen::Setting => ("MENU ПОДТВЕРДИТЬ", "EXIT ОТМЕНА"),
            Screen::Spectrum => ("MENU ЧАСТОТА", "EXIT ВОЗВРАТ"),
            _ => ("MENU НАЗАД", "EXIT НАЗАД"),
        }
    };
    text(row, out, 12, 305, 1, left, color::ACCENT);
    text(
        row,
        out,
        WIDTH - 12 - text_width(right, 1),
        305,
        1,
        right,
        color::TEXT,
    );
}

fn displayed_channel(channel: ChannelView<'_>, reverse: bool) -> (u32, u32, u16, u16) {
    if reverse {
        (
            channel.tx_10hz,
            channel.rx_10hz,
            channel.tx_tone_raw,
            channel.rx_tone_raw,
        )
    } else {
        (
            channel.rx_10hz,
            channel.tx_10hz,
            channel.rx_tone_raw,
            channel.tx_tone_raw,
        )
    }
}

fn valid_frequency(value: u32) -> bool {
    value > 0 && value <= 99_999_999
}

fn ham_status(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) {
    if !(104..115).contains(&row) {
        return;
    }
    let active = matches!(
        model.radio.state,
        RadioState::Receiving | RadioState::Transmitting
    );
    if model.view.mode != Mode::Channel {
        if active {
            text(
                row,
                out,
                204,
                106,
                1,
                model.radio.power.map(power_label).unwrap_or("?"),
                color::MUTED,
            );
        }
        return;
    }
    let Some(channel) = model.channel.filter(|channel| channel.active) else {
        return;
    };
    let (rx, tx, rx_tone, tx_tone) = displayed_channel(channel, model.view.reverse);
    if model.view.reverse {
        icon(
            row,
            out,
            16,
            104,
            11,
            &[8, 4, 2047, 4, 8, 256, 512, 2047, 512, 256],
            color::ACCENT,
        );
        text(row, out, 32, 106, 1, "R", color::ACCENT);
    }
    if valid_frequency(rx) && valid_frequency(tx) && rx != tx {
        icon(
            row,
            out,
            57,
            104,
            10,
            &[31, 3, 5, 9, 17, 32, 64, 128, 256, 512],
            color::ACCENT,
        );
        text(
            row,
            out,
            71,
            106,
            1,
            if tx > rx { "+" } else { "-" },
            color::ACCENT,
        );
    }
    let tones = [Tone::decode(rx_tone), Tone::decode(tx_tone)];
    if tones
        .iter()
        .any(|tone| matches!(tone, Some(Tone::Ctcss { .. })))
    {
        text(row, out, 94, 106, 1, "CT", color::ACCENT);
    }
    if tones
        .iter()
        .any(|tone| matches!(tone, Some(Tone::Dcs { .. })))
    {
        text(row, out, 115, 106, 1, "DCS", color::ACCENT);
    }
    if tones.iter().any(Option::is_none) {
        text(row, out, 140, 106, 1, "?", color::WARNING);
    }
    text(
        row,
        out,
        164,
        106,
        1,
        bandwidth_label(channel.bandwidth_raw),
        color::MUTED,
    );
    text(
        row,
        out,
        204,
        106,
        1,
        if active {
            model.radio.power.map(power_label).unwrap_or("?")
        } else {
            power_label(channel.power_raw)
        },
        color::MUTED,
    );
}

fn bandwidth_label(raw: u8) -> &'static str {
    match Bandwidth::decode(raw) {
        Some(Bandwidth::Wide) => "W",
        Some(Bandwidth::Narrow) => "N",
        None => "?",
    }
}

fn power_label(raw: u8) -> &'static str {
    match Power::decode(raw) {
        Some(Power::Low) => "L",
        Some(Power::Medium) => "M",
        Some(Power::High) => "H",
        None => "?",
    }
}

fn tone_text(raw: u16) -> Text {
    let mut value = Text::new();
    match Tone::decode(raw) {
        Some(Tone::None) => {
            let _ = value.write_str("ТОН ВЫКЛЮЧЕН");
        }
        Some(Tone::Ctcss { tenths_hz }) => {
            let _ = write!(value, "CTCSS {}.{} ГЦ", tenths_hz / 10, tenths_hz % 10);
        }
        Some(Tone::Dcs { code, inverted }) => {
            let _ = write!(
                value,
                "DCS D{:03o}{}",
                code,
                if inverted { 'I' } else { 'N' }
            );
        }
        None => {
            let _ = write!(value, "ТОН ? / {:04X}", raw);
        }
    }
    value
}

fn offset_text(rx: u32, tx: u32) -> Text {
    let mut value = Text::new();
    if !valid_frequency(rx) || !valid_frequency(tx) {
        let _ = value.write_str("СДВИГ TX: НЕ ОПРЕДЕЛЁН");
    } else if rx == tx {
        let _ = value.write_str("СИМПЛЕКС / БЕЗ СДВИГА");
    } else {
        let delta = rx.abs_diff(tx);
        let _ = write!(
            value,
            "СДВИГ TX {}{}.{:05} МГЦ",
            if tx > rx { '+' } else { '-' },
            delta / 100_000,
            delta % 100_000
        );
    }
    value
}

fn icon(
    row: u16,
    out: &mut [u16; WIDTH],
    x: usize,
    y: u16,
    width: usize,
    rows: &[u16],
    color: u16,
) {
    let Some(bits) = row.checked_sub(y).and_then(|dy| rows.get(usize::from(dy))) else {
        return;
    };
    for column in 0..width {
        if bits & (1 << (width - 1 - column)) != 0 && x + column < WIDTH {
            out[x + column] = color;
        }
    }
}

fn item_background(row: u16, out: &mut [u16; WIDTH], y: u16, selected: bool) {
    rect(
        row,
        out,
        12,
        y,
        216,
        ITEM_PAINT_HEIGHT,
        if selected {
            color::ACCENT
        } else {
            color::PANEL
        },
    );
}

fn item_color(selected: bool) -> u16 {
    if selected {
        color::BACKGROUND
    } else {
        color::TEXT
    }
}

struct Text {
    bytes: [u8; 128],
    len: usize,
}

impl Text {
    const fn new() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
        }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl Write for Text {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > self.bytes.len() - self.len {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + value.len()].copy_from_slice(value.as_bytes());
        self.len += value.len();
        Ok(())
    }
}

fn channel_name(bytes: &[u8]) -> Text {
    let mut value = Text::new();
    let bytes = &bytes[..bytes.len().min(16)];
    let end = bytes
        .iter()
        .position(|&byte| byte == 0 || byte == 0xff)
        .unwrap_or(bytes.len());
    let bytes = &bytes[..end];
    let mut index = 0;
    while index < bytes.len() {
        let first = bytes[index];
        let mut character = '?';
        if first.is_ascii_graphic() || first == b' ' {
            character = first as char;
        } else if (0x81..=0xfe).contains(&first)
            && index + 1 < bytes.len()
            && (0x40..=0xfe).contains(&bytes[index + 1])
            && bytes[index + 1] != 0x7f
        {
            if first == 0xa7 {
                character = gbk_cyrillic(bytes[index + 1]).unwrap_or('?');
            }
            index += 1;
        }
        let _ = value.write_char(character);
        index += 1;
    }
    if value.len == 0 {
        let _ = value.write_str("БЕЗ ИМЕНИ");
    }
    value
}

fn gbk_cyrillic(trail: u8) -> Option<char> {
    let (position, base, yo) = match trail {
        0xa1..=0xc1 => (u32::from(trail - 0xa1), 0x0410, 'Ё'),
        0xd1..=0xf1 => (u32::from(trail - 0xd1), 0x0430, 'ё'),
        _ => return None,
    };
    if position == 6 {
        Some(yo)
    } else {
        char::from_u32(base + position - u32::from(position > 6))
    }
}

struct FrequencyText {
    bytes: [u8; 9],
}
impl FrequencyText {
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes).unwrap_or("---.-----")
    }
}

fn frequency(value: u32) -> FrequencyText {
    let mut result = FrequencyText {
        bytes: *b"---.-----",
    };
    if value == 0 || value > 99_999_999 {
        return result;
    } else {
        // Fixed-width ASCII avoids the general formatting machinery for the
        // largest text on every frequency row; exact same nine characters.
        let mut digits = value;
        for index in (0..9).rev() {
            if index == 3 {
                result.bytes[index] = b'.';
            } else {
                result.bytes[index] = b'0' + (digits % 10) as u8;
                digits /= 10;
            }
        }
    }
    result
}

fn entry_text(value: u32, digits: u8, kind: EntryKind) -> Text {
    let mut raw = Text::new();
    let _ = write!(raw, "{:0width$}", value, width = usize::from(digits));
    let mut result = Text::new();
    for (index, ch) in raw.as_str().chars().enumerate() {
        if kind == EntryKind::Frequency && index == 3 {
            let _ = result.write_char('.');
        }
        let _ = result.write_char(ch);
    }
    if kind == EntryKind::Frequency {
        for index in usize::from(digits)..8 {
            if index == 3 {
                let _ = result.write_char('.');
            }
            let _ = result.write_char('_');
        }
    } else {
        let _ = result.write_char('_');
    }
    result
}

fn text_width(value: &str, scale: u16) -> usize {
    value
        .chars()
        .count()
        .saturating_mul(6 * usize::from(scale))
        .saturating_sub(usize::from(scale))
}

fn centered(row: u16, out: &mut [u16; WIDTH], y: u16, scale: u16, value: &str, color: u16) {
    if row < y || row >= y + 7 * scale {
        return;
    }
    text(
        row,
        out,
        WIDTH.saturating_sub(text_width(value, scale)) / 2,
        y,
        scale,
        value,
        color,
    );
}

fn rect(row: u16, out: &mut [u16; WIDTH], x: usize, y: u16, width: usize, height: u16, color: u16) {
    if row >= y && row < y.saturating_add(height) {
        out[x.min(WIDTH)..x.saturating_add(width).min(WIDTH)].fill(color);
    }
}

fn text(row: u16, out: &mut [u16; WIDTH], x: usize, y: u16, scale: u16, value: &str, color: u16) {
    if row < y || row >= y + 7 * scale {
        return;
    }
    let glyph_row = usize::from((row - y) / scale);
    for (index, ch) in value.chars().enumerate() {
        let origin = x + index * 6 * usize::from(scale);
        if origin >= WIDTH {
            break;
        }
        let bits = font::glyph(ch)[glyph_row];
        for column in 0..5 {
            if bits & (1 << (4 - column)) == 0 {
                continue;
            }
            let start = origin + column * usize::from(scale);
            if start < WIDTH {
                out[start..(start + usize::from(scale)).min(WIDTH)].fill(color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::controller::Entry;
    use std::{cell::RefCell, rc::Rc, vec, vec::Vec};

    fn view(screen: Screen) -> View {
        View {
            screen,
            mode: Mode::Channel,
            channel: Some(0),
            list_channel: Some(0),
            vfo_10hz: 14_550_000,
            step_index: 4,
            menu_index: 0,
            menu_group: None,
            entry: None,
            locked: false,
            notice: Notice::None,
            last_key: None,
            ptt_down: false,
            reverse: false,
            scan: ScanState::Stopped,
            bindings: crate::controller::DEFAULT_BINDINGS,
            binding_index: 0,
            binding_action_index: 0,
            active_slot: Slot::A,
            zone: None,
            zone_cursor: None,
            side_bindings: [SideAction::PttA, SideAction::PttB],
            side_index: 0,
            side_action_index: 0,
            side_down: [false; 2],
            ptt_slot: None,
            setting: if screen == Screen::Setting {
                Some(Setting::Mode)
            } else {
                None
            },
            setting_index: 0,
            display_light: LightMode::On,
            key_light: LightMode::On,
            spectrum_cursor: 40,
        }
    }

    fn model(screen: Screen) -> Model<'static> {
        Model {
            view: view(screen),
            channel: Some(ChannelView {
                index: 0,
                name: b"LOCAL CHANNEL",
                rx_10hz: 43_180_000,
                tx_10hz: 43_180_000,
                active: true,
                rx_tone_raw: 0,
                tx_tone_raw: 0x1375,
                bandwidth_raw: 0,
                power_raw: 2,
            }),
            list: &[],
            calibration_matches: Some(true),
            last_key: None,
            ptt_pressed: false,
            input_ready: true,
            radio: RadioView::OFF,
            zone_name: None,
            zones: &[],
            peer: None,
            spectrum: None,
        }
    }

    #[test]
    fn invalid_rows_and_views_preserve_pixels() {
        let mut pixels = [0xdead; WIDTH];
        let mut m = model(Screen::Home);
        assert_eq!(
            render_row(HEIGHT, &mut pixels, &m),
            Err(Error::RowOutOfRange { row: HEIGHT })
        );
        assert_eq!(pixels, [0xdead; WIDTH]);
        for invalid in [
            View {
                menu_index: menu_entries(m.view.menu_group).len() as u8,
                ..m.view
            },
            View {
                binding_index: m.view.bindings.len() as u8,
                ..m.view
            },
            View {
                binding_action_index: SHORTCUT_ACTIONS.len() as u8,
                ..m.view
            },
            View {
                step_index: 6,
                ..m.view
            },
            View {
                channel: Some(1024),
                ..m.view
            },
            View {
                entry: Some(Entry {
                    kind: EntryKind::Channel,
                    value: 10,
                    digits: 1,
                }),
                ..m.view
            },
            View {
                entry: Some(Entry {
                    kind: EntryKind::Frequency,
                    value: 0,
                    digits: 0,
                }),
                ..m.view
            },
        ] {
            m.view = invalid;
            assert_eq!(render_row(0, &mut pixels, &m), Err(Error::InvalidView));
            assert_eq!(pixels, [0xdead; WIDTH]);
        }
    }

    #[test]
    fn all_pages_initialize_every_pixel_and_keep_radio_disabled_banner() {
        let reference = model(Screen::Home);
        let mut banner = [0; WIDTH];
        render_row(35, &mut banner, &reference).unwrap();
        for screen in [
            Screen::Home,
            Screen::Menu,
            Screen::Channels,
            Screen::ChannelDetails,
            Screen::Step,
            Screen::Calibration,
            Screen::Keys,
            Screen::Help,
            Screen::About,
            Screen::Shortcuts,
            Screen::ShortcutAction,
            Screen::Zones,
            Screen::SideKeys,
            Screen::SideAction,
            Screen::Setting,
        ] {
            let m = model(screen);
            for y in 0..HEIGHT {
                let mut a = [0xdead; WIDTH];
                let mut b = [0xbeef; WIDTH];
                render_row(y, &mut a, &m).unwrap();
                render_row(y, &mut b, &m).unwrap();
                assert_eq!(a, b);
                if y == 35 {
                    assert_eq!(a, banner);
                }
            }
        }
    }

    #[test]
    fn gbk_cyrillic_name_conversion_preserves_encoding_boundaries() {
        for (start, alphabet) in [
            (0xa1, "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ"),
            (0xd1, "абвгдеёжзийклмнопрстуфхцчшщъыьэюя"),
        ] {
            for (offset, expected) in alphabet.chars().enumerate() {
                assert_eq!(gbk_cyrillic(start + offset as u8), Some(expected));
            }
        }
        assert_eq!(
            channel_name(&[b'A', 0xa7, 0xa7, 0xa7, 0xf1, 0, b'Z']).as_str(),
            "AЁя"
        );
        assert_eq!(channel_name(&[0x81, 0x40, b'A']).as_str(), "?A");
        assert_eq!(channel_name(&[0xa7]).as_str(), "?");
        assert_eq!(channel_name(&[0xff]).as_str(), "БЕЗ ИМЕНИ");
        assert_eq!(
            channel_name(b"12345678901234567890").as_str(),
            "1234567890123456"
        );
    }

    #[test]
    fn entries_retain_leading_zeroes_and_show_decimal_at_mhz_boundary() {
        assert_eq!(
            entry_text(145500, 6, EntryKind::Frequency).as_str(),
            "145.500__"
        );
        assert_eq!(
            entry_text(18000, 6, EntryKind::Frequency).as_str(),
            "018.000__"
        );
        assert_eq!(entry_text(1, 4, EntryKind::Channel).as_str(), "0001_");
        assert_eq!(frequency(14_551_250).as_str(), "145.51250");
        assert_eq!(frequency(u32::MAX).as_str(), "---.-----");
    }

    #[test]
    fn reverse_swaps_displayed_frequencies_and_tones_without_mutating_storage() {
        let raw = ChannelView {
            index: 0,
            name: b"REPEATER",
            rx_10hz: 43_180_000,
            tx_10hz: 43_020_000,
            active: true,
            rx_tone_raw: 0x1302,
            tx_tone_raw: 0x2013,
            bandwidth_raw: 1,
            power_raw: 2,
        };
        assert_eq!(
            displayed_channel(raw, false),
            (43_180_000, 43_020_000, 0x1302, 0x2013)
        );
        assert_eq!(
            displayed_channel(raw, true),
            (43_020_000, 43_180_000, 0x2013, 0x1302)
        );
        assert_eq!(raw.rx_10hz, 43_180_000);
        assert_eq!(tone_text(0x1302).as_str(), "CTCSS 77.0 ГЦ");
        assert_eq!(tone_text(0x2013).as_str(), "DCS D023N");
        assert_eq!(tone_text(0x3013).as_str(), "DCS D023I");
        assert_eq!(tone_text(0).as_str(), "ТОН ВЫКЛЮЧЕН");
        assert_eq!(tone_text(0xffff).as_str(), "ТОН ? / FFFF");
        assert_eq!(
            offset_text(raw.rx_10hz, raw.tx_10hz).as_str(),
            "СДВИГ TX -1.60000 МГЦ"
        );
        assert_eq!(
            offset_text(raw.tx_10hz, raw.rx_10hz).as_str(),
            "СДВИГ TX +1.60000 МГЦ"
        );
        let mut a = model(Screen::Home);
        a.channel = Some(raw);
        let mut b = a;
        b.view.reverse = true;
        compare_incremental(a, b);
    }

    struct Panel {
        frame: Vec<u16>,
        command: u8,
        arguments: Vec<u8>,
        x0: usize,
        x1: usize,
        y0: usize,
        y1: usize,
        x: usize,
        y: usize,
        high: Option<u8>,
        pixels_written: usize,
    }
    impl Panel {
        fn new() -> Self {
            Self {
                frame: vec![0; WIDTH * usize::from(HEIGHT)],
                command: 0,
                arguments: Vec::new(),
                x0: 0,
                x1: WIDTH - 1,
                y0: 0,
                y1: usize::from(HEIGHT) - 1,
                x: 0,
                y: 0,
                high: None,
                pixels_written: 0,
            }
        }
    }
    #[derive(Clone)]
    struct Bus(Rc<RefCell<Panel>>);
    impl DisplayBus for Bus {
        type Error = ();
        fn reset(&mut self, _: bool) -> Result<(), ()> {
            Ok(())
        }
        fn command(&mut self, command: u8) -> Result<(), ()> {
            let p = &mut *self.0.borrow_mut();
            p.command = command;
            p.arguments.clear();
            p.high = None;
            if command == 0x2c {
                p.x = p.x0;
                p.y = p.y0;
            }
            Ok(())
        }
        fn data(&mut self, bytes: &[u8]) -> Result<(), ()> {
            let p = &mut *self.0.borrow_mut();
            if p.command == 0x2a || p.command == 0x2b {
                p.arguments.extend_from_slice(bytes);
                if p.arguments.len() == 4 {
                    let start = usize::from(u16::from_be_bytes([p.arguments[0], p.arguments[1]]));
                    let end = usize::from(u16::from_be_bytes([p.arguments[2], p.arguments[3]]));
                    if p.command == 0x2a {
                        p.x0 = start;
                        p.x1 = end;
                    } else {
                        p.y0 = start;
                        p.y1 = end;
                    }
                }
            } else if p.command == 0x2c {
                for &byte in bytes {
                    if let Some(high) = p.high.take() {
                        assert!(p.x < WIDTH && p.y < usize::from(HEIGHT));
                        p.frame[p.y * WIDTH + p.x] = u16::from_be_bytes([high, byte]);
                        p.pixels_written += 1;
                        p.x += 1;
                        if p.x > p.x1 {
                            p.x = p.x0;
                            p.y += 1;
                        }
                    } else {
                        p.high = Some(byte);
                    }
                }
            }
            Ok(())
        }
    }
    struct NoDelay;
    impl Delay for NoDelay {
        fn delay_ms(&mut self, _: u32) {}
    }

    fn compare_incremental(mut initial: Model<'_>, updated: Model<'_>) -> usize {
        let panel = Rc::new(RefCell::new(Panel::new()));
        let mut display = Display::new_portrait(Bus(panel.clone()), NoDelay);
        display.initialize().unwrap();
        render(&mut display, &initial).unwrap();
        panel.borrow_mut().pixels_written = 0;
        render_changed(&mut display, &updated, Some(initial.view)).unwrap();
        let incremental_count = panel.borrow().pixels_written;
        for y in 0..HEIGHT {
            let mut expected = [0; WIDTH];
            render_row(y, &mut expected, &updated).unwrap();
            assert_eq!(
                &panel.borrow().frame[usize::from(y) * WIDTH..usize::from(y + 1) * WIDTH],
                expected.as_slice(),
                "row {y}"
            );
        }
        // Identical state must perform no display I/O.
        initial.view = updated.view;
        panel.borrow_mut().pixels_written = 0;
        render_changed(&mut display, &updated, Some(initial.view)).unwrap();
        assert_eq!(panel.borrow().pixels_written, 0);
        incremental_count
    }

    const ALL_SCREENS: [Screen; 16] = [
        Screen::Home,
        Screen::Menu,
        Screen::Channels,
        Screen::ChannelDetails,
        Screen::Step,
        Screen::Calibration,
        Screen::Keys,
        Screen::Help,
        Screen::About,
        Screen::Shortcuts,
        Screen::ShortcutAction,
        Screen::Zones,
        Screen::SideKeys,
        Screen::SideAction,
        Screen::Setting,
        Screen::Spectrum,
    ];

    const ALL_GROUPS: [Option<MenuGroup>; 6] = [
        None,
        Some(MenuGroup::Mode),
        Some(MenuGroup::Receive),
        Some(MenuGroup::Scanning),
        Some(MenuGroup::Keys),
        Some(MenuGroup::System),
    ];

    const ALL_SETTINGS: [Setting; 9] = [
        Setting::Mode,
        Setting::Power,
        Setting::Monitor,
        Setting::Modulation,
        Setting::Reverse,
        Setting::ActiveSlot,
        Setting::Scan,
        Setting::DisplayLight,
        Setting::KeyLight,
    ];

    /// Pixel oracle, independent of the planner's hard-coded geometry. Cache
    /// reference rows only in this host test; production still holds one row.
    /// All pairs include navigation and simultaneous backend publication, as
    /// the App merges these three independent damage sources into one bitmap.
    fn assert_all_transition_pairs(models: &[Model<'_>]) {
        let frames: Vec<Vec<[u16; WIDTH]>> = models
            .iter()
            .map(|model| {
                validate(model).unwrap();
                (0..HEIGHT)
                    .map(|y| {
                        let mut row = [0; WIDTH];
                        render_validated_row(y, &mut row, model);
                        row
                    })
                    .collect()
            })
            .collect();
        for (a, old) in models.iter().enumerate() {
            for (b, new) in models.iter().enumerate() {
                let plans = [
                    dirty_ranges(new, Some(old.view)).unwrap(),
                    radio_dirty_ranges(new, old.radio).unwrap(),
                    spectrum_dirty_ranges(new, old.spectrum).unwrap(),
                ];
                for y in 0..HEIGHT {
                    if frames[a][usize::from(y)] != frames[b][usize::from(y)] {
                        assert!(
                            plans.iter().any(|plan| plan
                                .ranges()
                                .iter()
                                .any(|&(start, end)| (start..end).contains(&y))),
                            "stale row {y}: {:?}/{:?}/{:?} -> {:?}/{:?}/{:?}",
                            old.view,
                            old.radio,
                            old.spectrum.map(|s| s.state),
                            new.view,
                            new.radio,
                            new.spectrum.map(|s| s.state)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_screen_pair_preserves_pixels_with_shared_flags_and_radio_changes() {
        let plot = Plot::EMPTY;
        let mut models = Vec::new();
        for screen in ALL_SCREENS {
            for profile in 0..4 {
                let mut m = model(screen);
                m.spectrum = Some(plot_view(&plot));
                match profile {
                    1 => {
                        m.view.locked = true;
                        m.view.notice = Notice::Locked;
                        m.view.ptt_down = true;
                        m.view.side_down[0] = true;
                    }
                    2 => {
                        m.view.notice = Notice::RadioUnavailable;
                        m.view.mode = Mode::Vfo;
                        m.view.active_slot = Slot::B;
                        m.view.scan = ScanState::Running;
                        m.radio.state = RadioState::Receiving;
                        m.radio.squelch_open = true;
                        m.radio.monitor = true;
                        m.radio.rssi = Some(321);
                    }
                    3 => {
                        m.radio.state = RadioState::Transmitting;
                        m.radio.power = Some(2);
                        m.view.ptt_down = true;
                        if screen == Screen::Home {
                            m.view.entry = Some(Entry {
                                kind: EntryKind::Frequency,
                                value: 145500,
                                digits: 6,
                            });
                        }
                        m.spectrum.as_mut().unwrap().state = SpectrumState::Restoring;
                    }
                    _ => {}
                }
                models.push(m);
            }
        }
        assert_all_transition_pairs(&models);

        // Exercise the actual display window path, not only damage coverage.
        let home = model(Screen::Home);
        let menu = model(Screen::Menu);
        assert_eq!(compare_incremental(home, menu), WIDTH * 262);
        assert_eq!(compare_incremental(menu, home), WIDTH * 262);
        assert_eq!(dirty_ranges(&home, None).unwrap().ranges(), &[(0, HEIGHT)]);
        let mut group = menu;
        group.view.menu_group = Some(MenuGroup::Scanning);
        assert_eq!(compare_incremental(menu, group), WIDTH * 206);
        assert_eq!(compare_incremental(group, menu), WIDTH * 206);
    }

    #[test]
    fn every_menu_group_cursor_and_setting_choice_clears_longer_old_content() {
        let mut menus = Vec::new();
        for group in ALL_GROUPS {
            for index in 0..menu_entries(group).len() {
                for flags in 0..3 {
                    let mut m = model(Screen::Menu);
                    m.view.menu_group = group;
                    m.view.menu_index = index as u8;
                    m.view.locked = flags == 1;
                    m.view.notice = if flags == 2 {
                        Notice::RadioUnavailable
                    } else {
                        Notice::None
                    };
                    menus.push(m);
                }
            }
        }
        assert_all_transition_pairs(&menus);

        let mut settings = Vec::new();
        for setting in ALL_SETTINGS {
            for index in 0..setting.choice_count() {
                for confirmed in [false, true] {
                    let mut m = model(Screen::Setting);
                    m.view.setting = Some(setting);
                    m.view.setting_index = index;
                    if confirmed {
                        m.radio.state = RadioState::Receiving;
                        m.radio.power = Some(2);
                        m.radio.monitor = true;
                        m.radio.modulation = Modulation::Am;
                        m.view.mode = Mode::Vfo;
                        m.view.active_slot = Slot::B;
                        m.view.reverse = true;
                        m.view.scan = ScanState::Running;
                        m.view.display_light = LightMode::Seconds10;
                        m.view.key_light = LightMode::Off;
                    }
                    settings.push(m);
                }
            }
        }
        assert_all_transition_pairs(&settings);
    }

    #[test]
    fn every_page_notice_lock_radio_and_spectrum_banner_transition_is_covered() {
        let plot = Plot::EMPTY;
        for screen in ALL_SCREENS {
            let mut states = Vec::new();
            for notice in [
                Notice::None,
                Notice::InvalidChannel,
                Notice::InactiveChannel,
                Notice::InvalidFrequency,
                Notice::EntryFull,
                Notice::Locked,
                Notice::TxDisabled,
                Notice::NoChannels,
                Notice::ChannelOnly,
                Notice::RadioUnavailable,
            ] {
                for locked in [false, true] {
                    let mut m = model(screen);
                    m.view.notice = notice;
                    m.view.locked = locked;
                    m.view.ptt_down = locked;
                    states.push(m);
                }
            }
            for state in [
                RadioState::Off,
                RadioState::Starting,
                RadioState::Receiving,
                RadioState::Transmitting,
                RadioState::Unavailable,
                RadioState::Failed,
            ] {
                let mut m = model(screen);
                m.radio.state = state;
                m.radio.squelch_open = true;
                m.radio.power = Some(1);
                m.radio.rssi = Some(271);
                states.push(m);
            }
            if screen == Screen::Spectrum {
                for state in [
                    SpectrumState::Preparing,
                    SpectrumState::Sweeping,
                    SpectrumState::Restoring,
                    SpectrumState::Stopped,
                    SpectrumState::Failed,
                ] {
                    let mut m = model(screen);
                    m.radio.state = RadioState::Starting;
                    m.spectrum = Some(SpectrumView {
                        state,
                        ..plot_view(&plot)
                    });
                    states.push(m);
                }
            }
            assert_all_transition_pairs(&states);
        }
    }

    #[test]
    fn dirty_rows_match_complete_frame_and_avoid_full_frame_for_navigation() {
        let old = model(Screen::Menu);
        let mut new = old;
        new.view.menu_index = 1;
        assert!(compare_incremental(old, new) < WIDTH * usize::from(HEIGHT));
        let mut old = model(Screen::Home);
        old.view.mode = Mode::Vfo;
        let mut new = old;
        new.view.vfo_10hz += 1250;
        assert!(compare_incremental(old, new) < WIDTH * usize::from(HEIGHT));
        let mut new = old;
        new.view.entry = Some(Entry {
            kind: EntryKind::Frequency,
            value: 145500,
            digits: 6,
        });
        compare_incremental(old, new);
        let mut after = new;
        after.view.entry = None;
        compare_incremental(new, after);
        let mut more = new;
        more.view.entry = Some(Entry {
            kind: EntryKind::Frequency,
            value: 1455000,
            digits: 7,
        });
        assert!(compare_incremental(new, more) < WIDTH * 50);
        let mut old = model(Screen::Menu);
        old.view.menu_index = 5;
        let mut new = old;
        new.view.menu_index = 0;
        compare_incremental(old, new);
        let old = model(Screen::Home);
        let mut new = old;
        new.view.locked = true;
        compare_incremental(old, new);
        let old = model(Screen::Home);
        let mut new = old;
        new.view.channel = Some(1);
        new.channel = Some(ChannelView {
            index: 1,
            name: b"SECOND CHANNEL",
            rx_10hz: 14_550_000,
            tx_10hz: 14_550_000,
            active: true,
            rx_tone_raw: 0x2013,
            tx_tone_raw: 0,
            bandwidth_raw: 1,
            power_raw: 0,
        });
        assert_eq!(compare_incremental(old, new), WIDTH * 53);
    }

    #[test]
    fn home_notices_hide_whole_shortcut_glyphs_and_transitions_leave_no_fragments() {
        let normal = model(Screen::Home);
        let mut normal_shortcut_row = [0; WIDTH];
        render_row(269, &mut normal_shortcut_row, &normal).unwrap();
        assert!(normal_shortcut_row
            .iter()
            .any(|&pixel| pixel == color::MUTED));
        for notice in [
            Notice::Locked,
            Notice::NoChannels,
            Notice::TxDisabled,
            Notice::InvalidChannel,
        ] {
            let mut shown = normal;
            shown.view.notice = notice;
            shown.view.locked = notice == Notice::Locked;
            for y in 265..270 {
                let mut row = [0; WIDTH];
                render_row(y, &mut row, &shown).unwrap();
                assert_eq!(
                    row,
                    [color::BACKGROUND; WIDTH],
                    "notice {notice:?}, row {y}"
                );
            }
            // Notice changes include the top row of the old shortcut above
            // the banner. Lock additionally changes the header and footer.
            let rows = if shown.view.locked { 77 } else { 26 };
            assert_eq!(compare_incremental(normal, shown), WIDTH * rows);
            assert_eq!(compare_incremental(shown, normal), WIDTH * rows);
        }
    }

    #[test]
    fn dirty_plan_is_bounded_sorted_and_preserves_four_row_streaming() {
        let mut before = model(Screen::Menu);
        before.view.menu_index = 1;
        let mut after = before;
        after.view.menu_index = 5;
        after.view.ptt_down = true;
        let plan = dirty_ranges(&after, Some(before.view)).unwrap();
        assert_eq!(plan.ranges().len(), 4);
        for (index, &(start, end)) in plan.ranges().iter().enumerate() {
            assert!(start < end && end <= HEIGHT);
            if index != 0 {
                assert!(plan.ranges()[index - 1].1 < start);
            }
        }
        let panel = Rc::new(RefCell::new(Panel::new()));
        let mut display = Display::new_portrait(Bus(panel.clone()), NoDelay);
        display.initialize().unwrap();
        render(&mut display, &before).unwrap();
        for &(start, end) in plan.ranges() {
            let mut y = start;
            while y < end {
                let next = (y + 4).min(end);
                render_rows(&mut display, &after, y, next).unwrap();
                y = next;
            }
        }
        for y in 0..HEIGHT {
            let mut expected = [0; WIDTH];
            render_row(y, &mut expected, &after).unwrap();
            assert_eq!(
                &panel.borrow().frame[usize::from(y) * WIDTH..usize::from(y + 1) * WIDTH],
                expected.as_slice(),
                "four-row slice y={y}"
            );
        }
        assert!(dirty_ranges(&after, Some(after.view))
            .unwrap()
            .ranges()
            .is_empty());
        assert_eq!(dirty_ranges(&after, None).unwrap().ranges(), &[(0, HEIGHT)]);

        let mut merged = DirtyRanges::new();
        merged.push(20, 40);
        merged.push(0, 10);
        merged.push(10, 20);
        assert_eq!(merged.ranges(), &[(0, 40)]);
        for start in [50, 70, 90, 110] {
            merged.push(start, start + 5);
        }
        assert_eq!(merged.ranges(), &[(0, 55), (70, 75), (90, 95), (110, 115)]);
        let mut expected = [false; HEIGHT as usize];
        let mut plan = DirtyRanges::new();
        for index in 0..80 {
            let start = (index * 37 % 313) as u16;
            let end = start + 7;
            expected[usize::from(start)..usize::from(end)].fill(true);
            plan.push(start, end);
            assert!(plan.ranges().len() <= 4);
            for (row, &needed) in expected.iter().enumerate() {
                assert!(
                    !needed
                        || plan
                            .ranges()
                            .iter()
                            .any(|&(a, b)| { row >= usize::from(a) && row < usize::from(b) })
                );
            }
        }
    }

    #[test]
    fn shortcut_navigation_assignment_and_scan_updates_match_full_frames() {
        for screen in [Screen::Shortcuts, Screen::ShortcutAction] {
            for (a, b) in [(0, 1), (1, 5), (5, 6), (6, 7), (7, 8)] {
                let mut old = model(screen);
                let mut new = old;
                if screen == Screen::Shortcuts {
                    old.view.binding_index = a;
                    new.view.binding_index = b;
                } else {
                    old.view.binding_action_index = a;
                    new.view.binding_action_index = b;
                }
                compare_incremental(old, new);
            }
        }
        let old = model(Screen::Shortcuts);
        let mut changed = old;
        changed.view.bindings[0] = ShortcutAction::Modulation;
        compare_incremental(old, changed);
        for scan in [
            ScanState::Requested,
            ScanState::Running,
            ScanState::Stopping,
        ] {
            let old = model(Screen::Home);
            let mut new = old;
            new.view.scan = scan;
            compare_incremental(old, new);
            compare_incremental(new, old);
        }
        for &action in &SHORTCUT_ACTIONS {
            assert!(text_width(shortcut_label(action), 2) <= WIDTH - 48);
        }
    }

    #[test]
    fn zone_slot_and_side_selection_updates_match_full_frames() {
        let rows = [
            ZoneView {
                index: None,
                name: b"",
                channel_count: 144,
            },
            ZoneView {
                index: Some(0),
                name: b"LOCAL",
                channel_count: 5,
            },
            ZoneView {
                index: Some(7),
                name: b"REPEATER",
                channel_count: 3,
            },
        ];
        let mut old = model(Screen::Zones);
        old.zones = &rows;
        for cursor in [Some(0), Some(7), None] {
            let mut new = old;
            new.view.zone_cursor = cursor;
            compare_incremental(old, new);
            old = new;
        }
        let old = model(Screen::Home);
        let mut new = old;
        new.view.active_slot = Slot::B;
        new.view.zone = Some(0);
        new.zone_name = Some(b"LOCAL");
        new.peer = Some(PeerView {
            slot: Slot::A,
            mode: Mode::Channel,
            frequency_10hz: Some(43_180_000),
            channel: Some(0),
            name: b"LOCAL CHANNEL",
            zone: None,
            zone_name: None,
            reverse: false,
        });
        compare_incremental(old, new);
        for screen in [Screen::SideKeys, Screen::SideAction] {
            let old = model(screen);
            let mut new = old;
            if screen == Screen::SideKeys {
                new.view.side_index = 1;
            } else {
                new.view.side_action_index = 6;
            }
            compare_incremental(old, new);
        }
        let old = model(Screen::SideKeys);
        let mut new = old;
        new.view.side_bindings[0] = SideAction::Active;
        compare_incremental(old, new);
        for &action in &SIDE_ACTIONS {
            assert!(text_width(side_action_label(action), 2) <= WIDTH - 42);
        }
    }

    fn compare_radio_incremental(initial: Model<'_>, updated: Model<'_>) -> usize {
        let panel = Rc::new(RefCell::new(Panel::new()));
        let mut display = Display::new_portrait(Bus(panel.clone()), NoDelay);
        display.initialize().unwrap();
        render(&mut display, &initial).unwrap();
        panel.borrow_mut().pixels_written = 0;
        let ranges = radio_dirty_ranges(&updated, initial.radio).unwrap();
        for &(start, end) in ranges.ranges() {
            render_rows(&mut display, &updated, start, end).unwrap();
        }
        for y in 0..HEIGHT {
            let mut expected = [0; WIDTH];
            render_row(y, &mut expected, &updated).unwrap();
            assert_eq!(
                &panel.borrow().frame[usize::from(y) * WIDTH..usize::from(y + 1) * WIDTH],
                expected.as_slice(),
                "RF metadata row {y}"
            );
        }
        let count = panel.borrow().pixels_written;
        count
    }

    #[test]
    fn live_rssi_uses_only_seven_rows_and_rf_updates_leave_no_stale_fields() {
        let mut old = model(Screen::Home);
        old.radio.state = RadioState::Receiving;
        let mut new = old;
        for value in [Some(1), Some(u16::MAX), None] {
            new.radio.rssi = value;
            assert_eq!(compare_radio_incremental(old, new), WIDTH * 7);
            old = new;
        }
        new.radio = RadioView {
            state: RadioState::Receiving,
            rssi: None,
            squelch_open: true,
            monitor: true,
            power: Some(0),
            modulation: Modulation::Am,
            route: Some(RadioRoute::Hf),
            tx_trial: false,
        };
        compare_radio_incremental(old, new);
        old = new;
        new.radio.power = Some(2);
        assert_eq!(compare_radio_incremental(old, new), WIDTH * 11);
        old = new;
        new.radio.state = RadioState::Transmitting;
        assert_eq!(compare_radio_incremental(old, new), WIDTH * 79);
        old = new;
        new.radio.power = Some(0);
        assert_eq!(compare_radio_incremental(old, new), WIDTH * 18);
        old = model(Screen::Home);
        old.radio.state = RadioState::Receiving;
        old.view.scan = ScanState::Running;
        new = old;
        new.radio.rssi = Some(789);
        assert_eq!(compare_radio_incremental(old, new), WIDTH * 7);
    }

    #[test]
    fn radio_state_transitions_only_repaint_visible_radio_regions() {
        let states = [
            RadioState::Off,
            RadioState::Starting,
            RadioState::Receiving,
            RadioState::Transmitting,
            RadioState::Unavailable,
            RadioState::Failed,
        ];
        for screen in [Screen::Home, Screen::Menu, Screen::Keys] {
            for &before in &states {
                for &after in &states {
                    let mut old = model(screen);
                    old.radio.state = before;
                    old.channel.as_mut().unwrap().tx_10hz = 43_020_000;
                    let mut new = old;
                    new.radio = RadioView {
                        state: after,
                        rssi: Some(987),
                        squelch_open: true,
                        monitor: true,
                        power: Some(2),
                        modulation: Modulation::Am,
                        route: Some(RadioRoute::Hf),
                        tx_trial: false,
                    };
                    let pixels = compare_radio_incremental(old, new);
                    assert!(pixels <= WIDTH * 113, "{screen:?}: {before:?} -> {after:?}");
                    if screen == Screen::Home {
                        old.view.entry = Some(Entry {
                            kind: EntryKind::Frequency,
                            value: 145500,
                            digits: 6,
                        });
                        new.view.entry = old.view.entry;
                        assert!(compare_radio_incremental(old, new) <= WIDTH * 20);
                    }
                }
            }
        }
    }

    #[test]
    fn ordinary_navigation_avoids_repainting_unchanged_panels() {
        let mut old = model(Screen::Home);
        old.view.mode = Mode::Vfo;
        let mut new = old;
        new.view.vfo_10hz += 1250;
        assert_eq!(compare_incremental(old, new), WIDTH * 28);
        new = old;
        new.view.step_index = 0;
        assert_eq!(compare_incremental(old, new), WIDTH * 14);
        old = model(Screen::Step);
        new = old;
        new.view.step_index = 0;
        assert_eq!(compare_incremental(old, new), WIDTH * 60);

        old = model(Screen::Menu);
        new = old;
        new.view.menu_index = 1;
        assert_eq!(compare_incremental(old, new), WIDTH * 67);
        for screen in [Screen::SideKeys, Screen::SideAction] {
            old = model(screen);
            new = old;
            if screen == Screen::SideKeys {
                new.view.side_index = 1;
            } else {
                new.view.side_action_index = 1;
            }
            assert_eq!(compare_incremental(old, new), WIDTH * 60);
        }

        for screen in [
            Screen::Help,
            Screen::About,
            Screen::Calibration,
            Screen::Step,
            Screen::Channels,
            Screen::Zones,
        ] {
            old = model(screen);
            new = old;
            new.view.last_key = Some(Key::Up);
            assert_eq!(compare_incremental(old, new), 0, "{screen:?}");
        }

        for state in [RadioState::Off, RadioState::Receiving] {
            old = model(Screen::Home);
            old.radio.state = state;
            for scan in [
                ScanState::Requested,
                ScanState::Running,
                ScanState::Stopping,
            ] {
                new = old;
                new.view.scan = scan;
                assert_eq!(compare_incremental(old, new), WIDTH * 47);
                assert_eq!(compare_incremental(new, old), WIDTH * 47);
            }
            new = old;
            new.view.reverse = true;
            assert_eq!(compare_incremental(old, new), WIDTH * 39);
        }
    }

    #[test]
    fn menu_groups_keep_cursor_updates_small_and_fm_am_direct() {
        assert_eq!(menu_entries(None)[0], MenuEntry::Item(MenuItem::Modulation));
        for group in [
            None,
            Some(MenuGroup::Mode),
            Some(MenuGroup::Receive),
            Some(MenuGroup::Scanning),
            Some(MenuGroup::Keys),
            Some(MenuGroup::System),
        ] {
            let entries = menu_entries(group);
            for index in 0..entries.len() - 1 {
                let mut old = model(Screen::Menu);
                old.view.menu_group = group;
                old.view.menu_index = index as u8;
                let mut new = old;
                new.view.menu_index += 1;
                let expected = if index / 7 == (index + 1) / 7 {
                    67
                } else {
                    231
                };
                assert_eq!(
                    compare_incremental(old, new),
                    WIDTH * expected,
                    "{group:?} menu {index}"
                );
                assert_eq!(
                    compare_incremental(new, old),
                    WIDTH * expected,
                    "{group:?} reverse {index}"
                );
            }
            let old = model(Screen::Menu);
            let mut new = old;
            new.view.menu_group = group;
            compare_incremental(old, new);
        }
    }

    #[test]
    fn channel_and_zone_page_replacement_preserves_header_and_footer() {
        let base = model(Screen::Home).channel.unwrap();
        let first: [ChannelView<'_>; 7] = core::array::from_fn(|index| ChannelView {
            index,
            name: b"PAGE ONE",
            ..base
        });
        let second: [ChannelView<'_>; 7] = core::array::from_fn(|index| ChannelView {
            index: index + 7,
            name: b"PAGE TWO",
            ..base
        });
        let mut old = model(Screen::Channels);
        old.list = &first;
        old.view.list_channel = Some(6);
        let mut new = old;
        new.list = &second;
        new.view.list_channel = Some(7);
        assert_eq!(compare_incremental(old, new), WIDTH * 224);

        let first: [ZoneView<'_>; 7] = core::array::from_fn(|index| ZoneView {
            index: Some(index as u8),
            name: b"ZONE ONE",
            channel_count: 1,
        });
        let second: [ZoneView<'_>; 7] = core::array::from_fn(|index| ZoneView {
            index: Some(index as u8 + 7),
            name: b"ZONE TWO",
            channel_count: 3,
        });
        let mut old = model(Screen::Zones);
        old.zones = &first;
        old.view.zone_cursor = Some(6);
        let mut new = old;
        new.zones = &second;
        new.view.zone_cursor = Some(7);
        assert_eq!(compare_incremental(old, new), WIDTH * 224);
    }

    #[test]
    fn both_slots_use_full_size_frequency_and_stay_in_fixed_positions() {
        let mut a = model(Screen::Home);
        a.peer = Some(PeerView {
            slot: Slot::B,
            mode: Mode::Vfo,
            frequency_10hz: Some(14_552_500),
            channel: None,
            name: &[],
            zone: Some(3),
            zone_name: Some(b"B LOCAL"),
            reverse: false,
        });
        assert_eq!(slot_content(&a, Slot::A).frequency_10hz, Some(43_180_000));
        assert_eq!(slot_content(&a, Slot::B).frequency_10hz, Some(14_552_500));
        for slot in [Slot::A, Slot::B] {
            let (start, end) = home_frequency_rows(slot);
            assert_eq!(end - start, 28);
            let color = if slot == Slot::A {
                color::ACCENT
            } else {
                color::TEXT
            };
            let mut painted_rows = 0;
            for y in start..end {
                let mut row = [0; WIDTH];
                render_row(y, &mut row, &a).unwrap();
                if row[14..226].contains(&color) {
                    painted_rows += 1;
                }
            }
            assert!(
                painted_rows >= 24,
                "frequency lost full-height glyphs: {slot:?}"
            );
        }
        let mut b = a;
        b.view.active_slot = Slot::B;
        b.view.mode = Mode::Vfo;
        b.view.channel = None;
        b.view.vfo_10hz = 14_552_500;
        b.view.zone = Some(3);
        b.zone_name = Some(b"B LOCAL");
        b.peer = Some(slot_content(&a, Slot::A));
        assert_eq!(slot_content(&b, Slot::A).frequency_10hz, Some(43_180_000));
        assert_eq!(slot_content(&b, Slot::B).frequency_10hz, Some(14_552_500));
        assert_eq!(slot_content(&b, Slot::A).mode, Mode::Channel);
        assert_eq!(slot_content(&b, Slot::B).zone, Some(3));
        assert!(compare_incremental(a, b) < WIDTH * usize::from(HEIGHT));
        let mut edit = b;
        edit.view.entry = Some(Entry {
            kind: EntryKind::Frequency,
            value: 145700,
            digits: 6,
        });
        compare_incremental(b, edit);
        for y in home_slot_rows(Slot::A).0..home_slot_rows(Slot::A).1 {
            let mut before = [0; WIDTH];
            let mut after = [0; WIDTH];
            render_row(y, &mut before, &b).unwrap();
            render_row(y, &mut after, &edit).unwrap();
            assert_eq!(before, after, "editing B changed saved A pixels");
        }
    }

    #[test]
    fn lighting_policy_ack_updates_only_current_value_and_keeps_other_light_independent() {
        for setting in [Setting::DisplayLight, Setting::KeyLight] {
            let mut old = model(Screen::Setting);
            old.view.setting = Some(setting);
            old.view.setting_index = 6;
            let mut new = old;
            if setting == Setting::DisplayLight {
                new.view.display_light = LightMode::Seconds10;
            } else {
                new.view.key_light = LightMode::Seconds10;
            }
            assert_eq!(compare_incremental(old, new), WIDTH * 7);
            assert_eq!(new.view.setting_index, 6, "backend ack moved draft cursor");
            assert_eq!(
                if setting == Setting::DisplayLight {
                    new.view.key_light
                } else {
                    new.view.display_light
                },
                LightMode::On
            );
        }
    }

    #[test]
    fn setting_drafts_move_only_the_highlight_and_keep_committed_value_visible() {
        for setting in [
            Setting::Mode,
            Setting::Power,
            Setting::Monitor,
            Setting::Modulation,
            Setting::Reverse,
            Setting::ActiveSlot,
            Setting::Scan,
            Setting::DisplayLight,
            Setting::KeyLight,
        ] {
            assert_eq!(
                setting_options(setting).len(),
                usize::from(setting.choice_count())
            );
            for &label in setting_options(setting) {
                assert!(text_width(label, 2) <= WIDTH - 48);
            }
            let mut old = model(Screen::Setting);
            old.view.setting = Some(setting);
            old.radio.state = RadioState::Receiving;
            old.radio.power = Some(0);
            let current = current_setting_label(&old, setting);
            for index in 1..setting.choice_count() {
                let mut new = old;
                new.view.setting_index = index;
                let expected_rows = if matches!(setting, Setting::DisplayLight | Setting::KeyLight)
                {
                    44
                } else {
                    72
                };
                assert_eq!(compare_incremental(old, new), WIDTH * expected_rows);
                assert_eq!(current_setting_label(&new, setting), current);
                for y in 84..91 {
                    let mut before = [0; WIDTH];
                    let mut after = [0; WIDTH];
                    render_row(y, &mut before, &old).unwrap();
                    render_row(y, &mut after, &new).unwrap();
                    assert_eq!(before, after, "draft changed committed label: {setting:?}");
                }
            }
            old.view.setting_index = setting.choice_count();
            let mut row = [0x1234; WIDTH];
            assert_eq!(render_row(112, &mut row, &old), Err(Error::InvalidView));
            assert_eq!(row, [0x1234; WIDTH]);
        }
        let mut absent = model(Screen::Setting);
        absent.view.setting = None;
        assert_eq!(dirty_ranges(&absent, None), Err(Error::InvalidView));
    }

    #[test]
    fn native_menu_symbols_fit_and_scan_symbol_requires_running_state() {
        for item in MENU_ITEMS {
            assert!(text_width(menu_label(item), 2) <= WIDTH - 44 - 12);
        }
        for group in [
            MenuGroup::Mode,
            MenuGroup::Receive,
            MenuGroup::Scanning,
            MenuGroup::Keys,
            MenuGroup::System,
        ] {
            assert!(
                text_width(menu_group_label(group), 2) <= 170,
                "category label must leave room for its right arrow"
            );
        }
        let mut m = model(Screen::Home);
        m.radio.state = RadioState::Receiving;
        for state in [
            ScanState::Stopped,
            ScanState::Requested,
            ScanState::Stopping,
            ScanState::Running,
        ] {
            m.view.scan = state;
            let mut active_icon_pixels = 0;
            for y in 242..258 {
                let mut row = [0; WIDTH];
                render_row(y, &mut row, &m).unwrap();
                active_icon_pixels += row[208..224].iter().filter(|&&p| p == color::GOOD).count();
            }
            assert_eq!(active_icon_pixels > 0, state == ScanState::Running);
        }
    }

    #[test]
    fn settings_show_backend_confirmation_without_replacing_pending_choice() {
        for setting in [Setting::Power, Setting::Monitor, Setting::Modulation] {
            let mut old = model(Screen::Setting);
            old.view.setting = Some(setting);
            old.view.setting_index = 1;
            assert_eq!(current_setting_label(&old, setting), "НЕТ ПОДТВЕРЖДЕНИЯ");
            let mut new = old;
            new.radio.state = RadioState::Receiving;
            new.radio.power = Some(0);
            assert_eq!(compare_radio_incremental(old, new), WIDTH * 27);
            old = new;
            new.radio.power = Some(2);
            new.radio.monitor = true;
            new.radio.modulation = Modulation::Am;
            assert_eq!(compare_radio_incremental(old, new), WIDTH * 7);
            assert_eq!(new.view.setting_index, 1);
            old = new;
            new.radio.state = RadioState::Starting;
            assert_eq!(compare_radio_incremental(old, new), WIDTH * 27);
            assert_eq!(current_setting_label(&new, setting), "НЕТ ПОДТВЕРЖДЕНИЯ");
        }
    }

    #[test]
    fn ptt_intent_never_claims_tx_or_changes_frequency_without_backend_ack() {
        let mut m = model(Screen::Home);
        m.channel.as_mut().unwrap().tx_10hz = 43_020_000;
        assert_eq!(home_frequency(&m), Some(43_180_000));
        m.ptt_pressed = true;
        m.view.ptt_down = true;
        assert_eq!(home_frequency(&m), Some(43_180_000));
        m.radio.state = RadioState::Receiving;
        assert_eq!(home_frequency(&m), Some(43_180_000));
        m.radio.state = RadioState::Transmitting;
        assert_eq!(home_frequency(&m), Some(43_020_000));
        m.view.reverse = true;
        assert_eq!(home_frequency(&m), Some(43_180_000));
        m.radio.state = RadioState::Receiving;
        assert_eq!(home_frequency(&m), Some(43_020_000));
        for state in [
            RadioState::Off,
            RadioState::Starting,
            RadioState::Receiving,
            RadioState::Transmitting,
            RadioState::Unavailable,
            RadioState::Failed,
        ] {
            m.radio = RadioView {
                state,
                rssi: Some(u16::MAX),
                squelch_open: true,
                monitor: true,
                power: Some(255),
                modulation: Modulation::Am,
                route: Some(RadioRoute::Hf),
                tx_trial: false,
            };
            for y in 0..HEIGHT {
                render_row(y, &mut [0; WIDTH], &m).unwrap();
            }
        }
    }

    #[test]
    fn tx_trial_marker_is_a_build_warning_not_a_transmit_acknowledgement() {
        assert!(!RadioView::OFF.tx_trial);
        for state in [
            RadioState::Off,
            RadioState::Starting,
            RadioState::Receiving,
            RadioState::Transmitting,
            RadioState::Unavailable,
            RadioState::Failed,
        ] {
            let mut normal = model(Screen::Home);
            normal.radio.state = state;
            normal.channel.as_mut().unwrap().tx_10hz = 43_020_000;
            let mut trial = normal;
            trial.radio.tx_trial = true;
            assert_eq!(trial.radio.state, normal.radio.state);
            assert_eq!(home_frequency(&trial), home_frequency(&normal));
            assert_eq!(compare_radio_incremental(normal, trial), WIDTH * 7);
            assert_eq!(compare_radio_incremental(trial, normal), WIDTH * 7);
            let mut with_monitor = trial;
            with_monitor.radio.monitor = true;
            compare_radio_incremental(trial, with_monitor);
            let mut no_marker = with_monitor;
            no_marker.radio.tx_trial = false;
            assert_eq!(compare_radio_incremental(with_monitor, no_marker), 0);
        }
        let mut old = model(Screen::Home);
        old.radio.state = RadioState::Receiving;
        old.radio.tx_trial = true;
        let mut new = old;
        new.radio.rssi = Some(321);
        assert_eq!(
            compare_radio_incremental(old, new),
            0,
            "hidden RSSI need not repaint the build warning"
        );
    }

    fn plot_view(plot: &Plot) -> SpectrumView<'_> {
        SpectrumView {
            state: SpectrumState::Sweeping,
            start_10hz: 43_000_000,
            step_10hz: 1250,
            len: 81,
            plot,
        }
    }

    fn compare_spectrum_incremental(old: Model<'_>, new: Model<'_>) -> usize {
        let plan = spectrum_dirty_ranges(&new, old.spectrum).unwrap();
        for row in 0..HEIGHT {
            let mut a = [0; WIDTH];
            let mut b = [0; WIDTH];
            render_row(row, &mut a, &old).unwrap();
            render_row(row, &mut b, &new).unwrap();
            if a != b {
                assert!(
                    plan.ranges()
                        .iter()
                        .any(|&(start, end)| (start..end).contains(&row)),
                    "unpainted spectrum row {row}"
                );
            }
        }
        plan.ranges().iter().map(|&(a, b)| usize::from(b - a)).sum()
    }

    #[test]
    fn spectrum_unknown_points_do_not_paint_fake_measurements() {
        let plot = Plot::EMPTY;
        let mut m = model(Screen::Spectrum);
        m.spectrum = Some(plot_view(&plot));
        for y in GRAPH_TOP..GRAPH_BOTTOM {
            let mut row = [0; WIDTH];
            render_row(y, &mut row, &m).unwrap();
            assert!(!row.contains(&color::ACCENT));
            assert!(!row.contains(&color::WARNING));
        }
        for state in [
            SpectrumState::Preparing,
            SpectrumState::Sweeping,
            SpectrumState::Restoring,
            SpectrumState::Stopped,
            SpectrumState::Failed,
        ] {
            m.spectrum.as_mut().unwrap().state = state;
            for y in 0..HEIGHT {
                render_row(y, &mut [0; WIDTH], &m).unwrap();
            }
        }
    }

    #[test]
    fn spectrum_cursor_repaints_readout_and_marker_not_the_graph() {
        let mut plot = Plot::EMPTY;
        plot.valid.fill(0xff);
        plot.fresh.fill(0xff);
        plot.peak_valid.fill(0xff);
        for index in 0..SPECTRUM_BINS {
            plot.latest[index] = (index * 6) as u16;
            plot.peaks[index] = 511;
        }
        let mut old = model(Screen::Spectrum);
        old.spectrum = Some(plot_view(&plot));
        for index in [0, 1, 40, 79, 80] {
            let mut new = old;
            new.view.spectrum_cursor = index;
            let count = compare_incremental(old, new);
            assert!(count <= WIDTH * 35, "cursor must not repaint the graph");
            old = new;
        }
    }

    #[test]
    fn spectrum_snapshot_damage_matches_full_pixels_including_stale_and_peak() {
        let empty = Plot::EMPTY;
        let mut measured = empty;
        measured.valid[5] = 1;
        measured.fresh[5] = 1;
        measured.peak_valid[5] = 1;
        measured.latest[40] = 0;
        measured.peaks[40] = 511;
        measured.sweep = 1;
        let mut old = model(Screen::Spectrum);
        old.spectrum = Some(plot_view(&empty));
        let mut new = old;
        new.spectrum = Some(plot_view(&measured));
        assert!(compare_spectrum_incremental(old, new) < 160);
        let mut stale = measured;
        stale.fresh.fill(0);
        old = new;
        new.spectrum = Some(plot_view(&stale));
        compare_spectrum_incremental(old, new);
        old = new;
        new.spectrum.as_mut().unwrap().state = SpectrumState::Restoring;
        assert_eq!(compare_spectrum_incremental(old, new), 41);
        old = new;
        new.spectrum.as_mut().unwrap().start_10hz += 1250;
        compare_spectrum_incremental(old, new);
        old = new;
        new.spectrum = None;
        compare_spectrum_incremental(old, new);
    }

    #[test]
    fn malformed_spectrum_snapshot_preserves_row_and_is_not_a_measurement() {
        let mut plot = Plot::EMPTY;
        plot.valid[0] = 1;
        plot.latest[0] = 512;
        let mut m = model(Screen::Spectrum);
        m.spectrum = Some(plot_view(&plot));
        let mut row = [0xabcd; WIDTH];
        assert_eq!(render_row(120, &mut row, &m), Err(Error::InvalidView));
        assert_eq!(row, [0xabcd; WIDTH]);
        let empty = Plot::EMPTY;
        m.spectrum = Some(plot_view(&empty));
        for len in [0, 82, 255] {
            m.spectrum.as_mut().unwrap().len = len;
            assert_eq!(render_row(120, &mut row, &m), Err(Error::InvalidView));
        }
    }

    #[test]
    fn confirmed_sweep_header_shows_muted_review_but_never_hides_real_tx_or_fault() {
        let plot = Plot::EMPTY;
        let mut m = model(Screen::Spectrum);
        m.spectrum = Some(plot_view(&plot));
        m.radio.state = RadioState::Starting;
        let mut preparing = [0; WIDTH];
        render_row(34, &mut preparing, &m).unwrap();
        m.radio.state = RadioState::Receiving;
        m.radio.squelch_open = true;
        let mut receiving = [0; WIDTH];
        render_row(34, &mut receiving, &m).unwrap();
        assert_eq!(
            preparing, receiving,
            "confirmed sweep header must not claim audible channel RX"
        );
        for state in [RadioState::Transmitting, RadioState::Failed] {
            m.radio.state = state;
            let mut row = [0; WIDTH];
            render_row(28, &mut row, &m).unwrap();
            assert_eq!(row, [color::BAD; WIDTH]);
        }
        for state in [
            SpectrumState::Preparing,
            SpectrumState::Sweeping,
            SpectrumState::Restoring,
            SpectrumState::Stopped,
            SpectrumState::Failed,
        ] {
            let old = model(Screen::Spectrum);
            let mut new = old;
            new.spectrum = Some(SpectrumView {
                state,
                ..plot_view(&plot)
            });
            compare_spectrum_incremental(old, new);
            compare_spectrum_incremental(new, old);
        }
    }

    #[test]
    fn fixed_frequency_digits_match_decimal_reference_at_edges_and_samples() {
        for value in [
            1, 9, 10, 99, 100, 99_999, 100_000, 14_550_000, 43_297_500, 99_999_999,
        ] {
            assert_eq!(
                frequency(value).as_str(),
                std::format!("{:03}.{:05}", value / 100_000, value % 100_000)
            );
        }
        for value in (1..99_999_999).step_by(97_531) {
            assert_eq!(
                frequency(value).as_str(),
                std::format!("{:03}.{:05}", value / 100_000, value % 100_000)
            );
        }
        assert_eq!(frequency(0).as_str(), "---.-----");
        assert_eq!(frequency(100_000_000).as_str(), "---.-----");
    }

    #[test]
    fn tenth_binding_is_zero_and_new_header_revision_fits_longest_title() {
        assert_eq!(binding_digit(9), Some(0));
        assert_eq!(shortcut_label(ShortcutAction::Spectrum), "СПЕКТР ЧАСТОТ");
        assert!(182 + text_width("0.1.8-r4", 1) <= WIDTH - 10);
        let mut old = model(Screen::Shortcuts);
        old.view.binding_index = 8;
        let mut new = old;
        new.view.binding_index = 9;
        compare_incremental(old, new);
        assert_eq!(new.view.bindings[9], ShortcutAction::Spectrum);
        for screen in [Screen::ShortcutAction, Screen::Help, Screen::About] {
            let m = model(screen);
            assert!(72 + text_width(page_title(m.view), 1) < 182);
            for row in 0..HEIGHT {
                render_row(row, &mut [0; WIDTH], &m).unwrap();
            }
        }
    }

    /// CPU-only host observation: excludes LCD, SPI waits, PNG and file I/O.
    /// No timing threshold; a desktop measurement does not predict MCU cycles.
    #[test]
    #[ignore = "manual renderer profiling; run release with --ignored --nocapture"]
    fn profile_renderer_rows() {
        const FRAMES: u32 = 2000;
        for screen in [
            Screen::Home,
            Screen::Menu,
            Screen::Channels,
            Screen::ChannelDetails,
            Screen::Shortcuts,
            Screen::ShortcutAction,
        ] {
            let mut m = model(screen);
            let mut list = [m.channel.unwrap(); LIST_ROWS];
            for (index, channel) in list.iter_mut().enumerate() {
                channel.index = index;
            }
            m.list = &list;
            let mut row = [0; WIDTH];
            let started = std::time::Instant::now();
            for _ in 0..FRAMES {
                for y in 0..HEIGHT {
                    render_row(y, &mut row, std::hint::black_box(&m)).unwrap();
                    std::hint::black_box(&row);
                }
            }
            let elapsed = started.elapsed();
            std::println!(
                "{screen:?}: {} frames; {:.3} us/frame; {:.1} ns/row; host CPU only",
                FRAMES,
                elapsed.as_secs_f64() * 1e6 / f64::from(FRAMES),
                elapsed.as_secs_f64() * 1e9 / f64::from(FRAMES) / f64::from(HEIGHT)
            );
        }
    }
}
