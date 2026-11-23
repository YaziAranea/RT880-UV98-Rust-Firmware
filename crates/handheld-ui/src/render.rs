//! Bounded RGB565 renderer for the physical handheld UI. No framebuffer, heap,
//! measurements, frequency tuning, or storage writes. Radio activity is shown
//! only from an explicit backend report, never inferred from a key or request.

use crate::cb::{CbGrid, CHANNEL_COUNT as CB_CHANNEL_COUNT};
use crate::locale::{self, Language};
use core::fmt::{self, Write};
use uv98_codeplug::channels::{Bandwidth, Power, Tone};
use uv98_display::{Delay, Display, DisplayBus};
use uv98_keys::Key;
use uv98_spectrum::compact::{Plot, BINS as SPECTRUM_BINS};

#[cfg(test)]
use crate::controller::{
    binding_digit, vfo_tx_frequency_for, LightMode, MenuEntry, MenuGroup, MenuItem, ShortcutAction,
    SideAction, MENU_ITEMS,
};
use crate::controller::{
    menu_entries, CrossbandProbe, EntryKind, MdcDraft, Mode, Notice, ScanState, Screen, Setting,
    Slot, View, SHORTCUT_ACTIONS, SIDE_ACTIONS, SPECTRUM_INTERVALS, STEP_10HZ, TX_CTCSS,
};
use crate::meter::{receive_segments, transmit_segments, SEGMENTS as METER_SEGMENTS};

#[path = "signaling_popup.rs"]
mod signaling_popup;
pub use signaling_popup::{DtmfReception, NameReception};
pub const NAME_POPUP_PRIORITY_ROWS: (u16, u16) = signaling_popup::NAME_PRIORITY_ROWS;

#[path = "damage.rs"]
mod damage;
pub use damage::{
    dirty_ranges, plan_damage, radio_dirty_ranges, spectrum_dirty_ranges, DamagePlan, DirtyRanges,
};
#[path = "navigation.rs"]
mod navigation;
use navigation::{
    actions, channels, menu, menu_start, setting, setting_geometry, setting_title,
    setting_uses_radio, shift_tx_preview, shortcuts, side_keys, zones,
};
#[cfg(test)]
use navigation::{current_setting_label, setting_options, MENU_LABEL_X, SHIFT_TX_X};
pub use navigation::{
    menu_group_label, menu_item_rows, menu_label, shortcut_label, side_action_label, MenuCache,
};
#[path = "font.rs"]
mod font;
#[path = "frequency_font.rs"]
mod frequency_font;
#[path = "utility.rs"]
mod utility;
pub use utility::ShortwaveScope;
#[cfg(feature = "pa5-audio")]
#[path = "audio_capture.rs"]
mod audio_capture;
#[cfg(feature = "pa5-audio")]
pub use audio_capture::{render_audio_rows, AudioCaptureView};
use utility::{shortwave_processing_label, shortwave_scope_page, utility_page};

pub const WIDTH: usize = 240;
pub const HEIGHT: u16 = 320;

struct Row<'a> {
    pixels: &'a mut [u16; WIDTH],
    language: Language,
}
impl core::ops::Deref for Row<'_> {
    type Target = [u16; WIDTH];
    fn deref(&self) -> &Self::Target {
        self.pixels
    }
}
impl core::ops::DerefMut for Row<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.pixels
    }
}

pub const LIST_ROWS: usize = 7;
const HOME_HEADER_END: u16 = 28;
const HOME_NAME_Y: u16 = 30;
const HOME_FREQUENCY_Y: u16 = 54;
const HOME_DIGIT_HEIGHT: u16 = 48;
pub const HOME_METER_Y: u16 = 112;
const HOME_META_Y: u16 = 130;
const LIST_Y: u16 = 54;
const LIST_ROW_HEIGHT: u16 = 32;
const ITEM_PAINT_HEIGHT: u16 = 30;

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
    pub(super) const TX: u16 = 0x319f;
    // Pure red in the portrait panel's native BGR565 order.
    pub(super) const TX_OUTLINE: u16 = 0x001f;
    pub(super) const RX_OUTLINE: u16 = 0x07e0;
    // Portrait panel uses MADCTL BGR. These two badges are authored in
    // panel order; the established palette and the S-meter stay as before.
    pub const AM_BADGE: u16 = 0x421f;
    pub const FM_BADGE: u16 = 0x27e4;
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
    /// Confirmed transmitter context, including a tail after key release.
    pub tx_slot: Option<Slot>,
    /// Confirmed receive context. Scope or relay audio has no home VFO.
    pub rx_slot: Option<Slot>,
    pub dtmf: Option<DtmfReception>,
    pub radio_name: Option<NameReception>,
    pub rssi: Option<u16>,
    pub meter_floor: u16,
    pub squelch_open: bool,
    pub monitor: bool,
    /// Confirmed power selection: 0 low, 1 medium, 2 high; not measured watts.
    pub power: Option<u8>,
    pub modulation: Modulation,
    /// Explicit AM on the selected slot, independent of automatic airband RX.
    pub vfo_am_override: bool,
    pub route: Option<RadioRoute>,
    /// Build-profile warning only. This never means TX is active or eligible.
    pub tx_trial: bool,
}

impl RadioView {
    pub const OFF: Self = Self {
        state: RadioState::Off,
        tx_slot: None,
        rx_slot: None,
        dtmf: None,
        radio_name: None,
        rssi: None,
        meter_floor: 0,
        squelch_open: false,
        monitor: false,
        power: None,
        modulation: Modulation::Fm,
        vfo_am_override: false,
        route: None,
        tx_trial: false,
    };

    pub fn visible_dtmf(self) -> Option<DtmfReception> {
        if self.state == RadioState::Receiving {
            self.dtmf
        } else {
            None
        }
    }

    fn signaling_popup(self) -> Option<signaling_popup::Reception> {
        if self.state != RadioState::Receiving {
            return None;
        }
        self.radio_name
            .map(signaling_popup::Reception::Name)
            .or_else(|| self.visible_dtmf().map(signaling_popup::Reception::Dtmf))
    }

    pub fn signaling_popup_damage(self, previous: Self) -> Option<(u16, u16)> {
        (self.signaling_popup() != previous.signaling_popup()).then_some(signaling_popup::ROWS)
    }

    pub fn receiving_slot(self) -> Option<Slot> {
        if self.state == RadioState::Receiving && self.squelch_open {
            self.rx_slot
        } else {
            None
        }
    }

    pub fn transmitting_slot(self) -> Option<Slot> {
        if self.state == RadioState::Transmitting {
            self.tx_slot
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpectrumState {
    Preparing,
    Sweeping,
    Listening,
    Restoring,
    Stopped,
    Failed,
}

/// Why the last spectrum start/restart was refused. Kept separately from the
/// snapshot so the overlay and window controls still render without data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpectrumError {
    None,
    Receiver,
    Window,
    Route,
    Busy,
    Restore,
    /// The displayed grid belongs to an older scope window than RF.
    Stale,
}

impl SpectrumError {
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "\x1f0B4",
            Self::Receiver => "\x1f0F2",
            Self::Window => "\x1f0CF",
            Self::Route => "\x1f063",
            Self::Busy => "\x1f0C6",
            Self::Restore => "\x1f055",
            Self::Stale => "\x1f064",
        }
    }
    pub const fn is_failure(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Borrow an immutable published snapshot, never the actively changing sweep
/// buffer. The caller publishes a new snapshot only after queued rows drain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpectrumView<'a> {
    pub state: SpectrumState,
    pub window_id: u32,
    pub start_10hz: u32,
    /// Middle bin of the published window.
    pub center_10hz: u32,
    pub step_10hz: u32,
    pub len: u8,
    pub plot: &'a Plot,
    pub waterfall: Option<&'a Waterfall>,
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
    pub fn span_10hz(self) -> Option<u32> {
        self.step_10hz
            .checked_mul(u32::from(self.len).checked_sub(1)?)
    }
    pub fn middle_index(self) -> u8 {
        self.len / 2
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SpectrumPhase {
    Pending,
    Preparing,
    Sweeping,
    Listening,
    Restoring,
    Stopped,
    Failed,
}

/// Single live phase for labels and audio claims. An immutable plot may lag a
/// restart, but a stale `Listening` snapshot must never claim audio after the
/// live listening flag was cleared.
fn spectrum_phase(model: &Model<'_>) -> Option<SpectrumPhase> {
    if model.view.screen != Screen::Spectrum {
        return None;
    }
    if model.view.spectrum_listening {
        return Some(SpectrumPhase::Listening);
    }
    if model.view.spectrum_pending_listen.is_some() {
        return Some(SpectrumPhase::Pending);
    }
    match model.spectrum?.state {
        SpectrumState::Preparing => Some(SpectrumPhase::Preparing),
        SpectrumState::Sweeping => Some(SpectrumPhase::Sweeping),
        // The live flag is authoritative; old Listening publication is stale.
        SpectrumState::Listening => Some(SpectrumPhase::Stopped),
        SpectrumState::Restoring => Some(SpectrumPhase::Restoring),
        SpectrumState::Stopped => Some(SpectrumPhase::Stopped),
        SpectrumState::Failed => Some(SpectrumPhase::Failed),
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
    pub tx_frequency_10hz: Option<u32>,
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
    pub manual_tx: Option<ManualTxStatus>,
    /// Pack voltage in millivolts, or None while the measurement is unavailable.
    pub battery_mv: Option<u16>,
    /// None means all channels, not a fabricated stored zone name.
    pub zone_name: Option<&'a [u8]>,
    pub zones: &'a [ZoneView<'a>],
    /// Saved other slot, not a second receiver or evidence of dual watch.
    pub peer: Option<PeerView<'a>>,
    pub spectrum: Option<SpectrumView<'a>>,
    pub shortwave_scope: Option<&'a ShortwaveScope>,
    pub spectrum_error: SpectrumError,
    /// False while the lower side-key electrical source is not identified.
    /// Its assignment is editable but not evidence that the input works.
    pub lower_side_mapped: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManualTxStatus {
    pub remaining_seconds: u8,
    /// Receive minus transmit pack voltage on the display scale.
    pub sag_mv: Option<u16>,
}

/// Fixed physical positions: selecting B never swaps the cards on screen.
pub const fn home_slot_rows(slot: Slot) -> (u16, u16) {
    match slot {
        Slot::A => (28, 170),
        Slot::B => (176, 318),
    }
}

/// The badge and top outline are serviced before the rest of the card.
pub const fn home_ptt_badge_rows(slot: Slot) -> (u16, u16) {
    let top = home_slot_rows(slot).0;
    (top + 5, top + 21)
}

/// Countdown and voltage drop occupy unused space between TX meter labels.
pub const fn home_tx_status_rows(slot: Slot) -> (u16, u16) {
    let start = home_slot_rows(slot).0 + HOME_METER_Y + 8;
    (start, start + 7)
}

/// The same header location is used on Home and secondary screens.
pub const fn battery_rows() -> (u16, u16) {
    (3, 15)
}

fn home_frame_ink(active_slot: Slot, slot: Slot) -> u16 {
    if active_slot == slot {
        color::ACCENT
    } else {
        color::LINE
    }
}

/// One disjoint LCD rectangle per edge. Each sends at most one full-width
/// row's worth of pixels, even for a tall side. The rest of the card stays put.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HomeOutlineEdge(u8);

impl HomeOutlineEdge {
    pub const fn top(slot: Slot) -> Self {
        Self(match slot {
            Slot::A => 0,
            Slot::B => 4,
        })
    }

    pub fn next(pending: u8) -> Option<Self> {
        (pending != 0).then(|| Self(pending.trailing_zeros() as u8))
    }

    pub fn bit(self) -> u8 {
        1 << self.0
    }

    fn slot(self) -> Slot {
        if self.0 < 4 {
            Slot::A
        } else {
            Slot::B
        }
    }

    fn rect(self) -> (u16, u16, u16, u16) {
        let (top, end) = home_slot_rows(self.slot());
        match self.0 % 4 {
            0 => (4, top, 232, 1),
            1 => (4, top + 1, 1, end - top - 2),
            2 => (235, top + 1, 1, end - top - 2),
            _ => (4, end - 1, 232, 1),
        }
    }
}

pub fn render_home_outline_edge<B: DisplayBus, D: Delay>(
    display: &mut Display<B, D>,
    edge: HomeOutlineEdge,
    active_slot: Slot,
    radio: RadioView,
) -> Result<(), RenderError<B::Error>> {
    let slot = edge.slot();
    let ink = if radio.transmitting_slot() == Some(slot) {
        color::TX_OUTLINE
    } else if radio.receiving_slot() == Some(slot) {
        color::RX_OUTLINE
    } else {
        home_frame_ink(active_slot, slot)
    };
    let (x, y, width, height) = edge.rect();
    // The popup is above the VFO cards. Narrow direct LCD writes must obey
    // the same composition order as the full-row painter.
    if height == 1
        && radio.signaling_popup().is_some()
        && (signaling_popup::ROWS.0..signaling_popup::ROWS.1).contains(&y)
    {
        let (left, right) = signaling_popup::COLUMNS;
        display
            .fill_rect(x, y, left - x, 1, ink)
            .map_err(RenderError::Display)?;
        return display
            .fill_rect(right, y, x + width - right, 1, ink)
            .map_err(RenderError::Display);
    }
    display
        .fill_rect(x, y, width, height, ink)
        .map_err(RenderError::Display)
}

pub const fn home_frequency_rows(slot: Slot) -> (u16, u16) {
    let top = home_slot_rows(slot).0;
    (
        top + HOME_FREQUENCY_Y,
        top + HOME_FREQUENCY_Y + HOME_DIGIT_HEIGHT,
    )
}

fn home_ham_rows(slot: Slot) -> (u16, u16) {
    let top = home_slot_rows(slot).0;
    (top + HOME_META_Y - 3, top + HOME_META_Y + 11)
}

fn home_meter_rows(slot: Slot) -> (u16, u16) {
    let top = home_slot_rows(slot).0;
    (top + HOME_METER_Y, top + HOME_METER_Y + 15)
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
    render_rows_cached(display, model, start_y, end_y, &mut MenuCache::new())
}

/// The caller may retain decoded menu glyphs across synchronous LCD windows.
pub fn render_rows_cached<B: DisplayBus, D: Delay>(
    display: &mut Display<B, D>,
    model: &Model<'_>,
    start_y: u16,
    end_y: u16,
    menu_cache: &mut MenuCache,
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
        render_validated_row(y, &mut pixels, model, menu_cache);
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

/// Deterministic row renderer. Invalid input preserves the caller's pixel row.
pub fn render_row(row: u16, out: &mut [u16; WIDTH], model: &Model<'_>) -> Result<(), Error> {
    if row >= HEIGHT {
        return Err(Error::RowOutOfRange { row });
    }
    validate(model)?;
    render_validated_row(row, out, model, &mut MenuCache::new());
    Ok(())
}

/// Internal fast path: caller has validated the immutable model and row range.
#[inline(never)]
fn render_validated_row(
    row: u16,
    pixels: &mut [u16; WIDTH],
    model: &Model<'_>,
    menu_cache: &mut MenuCache,
) {
    let mut row_context = Row {
        pixels,
        language: model.view.language(),
    };
    let out = &mut row_context;
    out.fill(color::BACKGROUND);
    if model.view.screen == Screen::Home {
        if row < HOME_HEADER_END {
            home_header(row, out, model);
        } else {
            home(row, out, model);
        }
    } else if row < 48 {
        header(row, out, model);
    } else if row < 296 {
        match model.view.screen {
            Screen::Home => unreachable!(),
            Screen::Menu => menu(row, out, model, menu_cache),
            Screen::Channels => channels(row, out, model),
            Screen::ChannelDetails => details(row, out, model),
            Screen::Step => steps(row, out, model),
            Screen::Calibration => calibration(row, out, model),
            Screen::Keys => keys(row, out, model),
            Screen::Help => help(row, out),
            Screen::About => about(row, out),
            Screen::Shortcuts => shortcuts(row, out, model),
            Screen::Zones => zones(row, out, model),
            Screen::SideKeys => side_keys(row, out, model),
            Screen::ShortcutAction | Screen::SideAction => actions(row, out, model),
            Screen::Setting => setting(row, out, model),
            Screen::Spectrum => spectrum(row, out, model),
            Screen::Shortwave => shortwave(row, out, model),
            Screen::Crossband => crossband(row, out, &model.view),
            Screen::Gps | Screen::Aprs | Screen::AprsRx => utility_page(row, out, model),
            Screen::Cb => cb_grid(row, out, &model.view),
            Screen::Mdc => mdc_page(row, out, &model.view),
            Screen::RadioName => radio_name_page(row, out, &model.view),
            Screen::RxToneSearch => rx_tone_search_page(row, out, &model.view),
            Screen::DtmfMemory => dtmf_memory_page(row, out, &model.view),
        }
        notice(row, out, model.view.notice);
    } else {
        footer(row, out, model);
    }
    if let Some(report) = model.radio.signaling_popup() {
        signaling_popup::paint(row, out, report);
    }
}

fn validate(model: &Model<'_>) -> Result<(), Error> {
    let view = model.view;
    let invalid_dtmf_page = view.screen == Screen::DtmfMemory
        && view.dtmf_memory_page.is_none_or(|page| {
            usize::from(page.slot) >= crate::controller::DTMF_MEMORY_SLOTS
                || usize::from(page.len) > crate::controller::DTMF_MEMORY_LENGTH
                || page.bytes[..usize::from(page.len)]
                    .iter()
                    .any(|byte| !byte.is_ascii_digit() && !matches!(byte, b'*' | b'#'))
        });
    if view.spectrum_cursor as usize >= SPECTRUM_BINS
        || (view.screen == Screen::Cb && view.cb_draft.is_none())
        || (view.screen == Screen::Mdc && view.mdc_option > 1)
        || invalid_dtmf_page
    {
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
            if spectrum
                .frequency_10hz(spectrum.middle_index())
                .is_none_or(|middle| middle != spectrum.center_10hz)
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
            .is_some_and(|setting| view.setting_index < setting.choice_count())
    {
        return Err(Error::InvalidView);
    }
    if usize::from(view.spectrum_span_index) >= crate::controller::SPECTRUM_INTERVALS.len()
        || usize::from(view.spectrum_step_index) >= STEP_10HZ.len()
        || view.spectrum_option >= crate::controller::SPECTRUM_MENU_LEN
        || usize::from(view.menu_index) >= menu_entries(view.menu_group).len()
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
        let maximum_digits = entry.kind.maximum_digits();
        if entry.digits == 0
            || entry.digits > maximum_digits
            || entry.value >= 10u32.pow(u32::from(entry.digits))
        {
            return Err(Error::InvalidView);
        }
    }
    Ok(())
}

fn header(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    rect(row, out, 0, 0, WIDTH, 27, color::PANEL);
    text(row, out, 12, 11, 1, page_title(model.view), color::ACCENT);
    if model.view.locked {
        rect(row, out, 173, 16, 60, 10, color::WARNING);
        text(row, out, 187, 18, 1, "\x1f041", color::BACKGROUND);
    } else {
        text(row, out, 212, 18, 1, VERSION_SHORT, color::MUTED);
    }
    battery(row, out, model.battery_mv);
    radio_banner(row, out, model);
}

pub fn radio_banner_rows(screen: Screen) -> (u16, u16) {
    if screen == Screen::Home {
        (0, 27)
    } else {
        (28, 48)
    }
}

/// Rounded decivolts and approximate voltage bands for a two-cell pack.
/// The four segments are voltage levels, not a state-of-charge estimate.
pub fn battery_display(millivolts: Option<u16>) -> Option<(u16, u8)> {
    millivolts.map(|mv| {
        let decivolts = ((u32::from(mv) + 50) / 100) as u16;
        let bars = match mv {
            0..6600 => 0,
            6600..7200 => 1,
            7200..7600 => 2,
            7600..8000 => 3,
            _ => 4,
        };
        (decivolts, bars)
    })
}

fn battery(row: u16, out: &mut Row<'_>, millivolts: Option<u16>) {
    let (start, end) = battery_rows();
    if !(start..end).contains(&row) {
        return;
    }
    let display = battery_display(millivolts);
    let ink = match display {
        Some((_, 0 | 1)) => color::TX,
        Some(_) => color::TEXT,
        None => color::MUTED,
    };
    rect(row, out, 173, start, 25, 1, ink);
    rect(row, out, 173, end - 1, 25, 1, ink);
    rect(row, out, 173, start, 1, end - start, ink);
    rect(row, out, 197, start, 1, end - start, ink);
    rect(row, out, 198, start + 4, 3, 4, ink);
    if let Some((decivolts, bars)) = display {
        for bar in 0..bars {
            rect(row, out, 176 + usize::from(bar) * 5, start + 3, 4, 6, ink);
        }
        if (6..13).contains(&row) {
            let mut volts = Text::new();
            let _ = write!(volts, "{}.{}V", decivolts / 10, decivolts % 10);
            text(row, out, 205, 6, 1, volts.as_str(), ink);
        }
    } else {
        text(row, out, 183, 6, 1, "?", ink);
        text(row, out, 205, 6, 1, "--.-V", ink);
    }
}

fn home_header(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    rect(row, out, 0, 0, WIDTH, 27, color::PANEL);
    match model.radio.state {
        RadioState::Starting | RadioState::Receiving
            if model.view.dual_watch && model.view.scan != ScanState::Paused =>
        {
            text(row, out, 12, 6, 1, "DUAL WATCH", color::ACCENT);
        }
        RadioState::Receiving if !model.view.locked => home_status(row, out, model),
        RadioState::Receiving | RadioState::Transmitting => {}
        _ => radio_banner(row + 28, out, model),
    }
    if let Some(label) = notice_label(model.view.notice) {
        rect(row, out, 0, 17, WIDTH, 10, color::WARNING_BACKGROUND);
        centered(row, out, 18, 1, label, color::WARNING);
    }
    if model.view.locked {
        rect(row, out, 146, 1, 20, 16, color::WARNING);
        symbol(row, out, 148, 1, Symbol::Lock, color::BACKGROUND);
    }
    battery(row, out, model.battery_mv);
}

fn radio_banner(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    if !(28..48).contains(&row) || model.view.screen == Screen::Shortwave {
        return;
    }
    let (label, foreground, background) = match model.radio.state {
        RadioState::Off if model.ptt_pressed || model.view.ptt_down => {
            ("\x1f035", color::WARNING, color::WARNING_BACKGROUND)
        }
        RadioState::Off => ("\x1f0FA", color::WARNING, color::WARNING_BACKGROUND),
        RadioState::Starting => ("\x1f0FC", color::WARNING, color::WARNING_BACKGROUND),
        RadioState::Receiving => ("", color::GOOD, color::PANEL),
        RadioState::Transmitting => ("TX", color::TEXT, color::BAD),
        RadioState::Unavailable => ("\x1f0FB", color::WARNING, color::WARNING_BACKGROUND),
        RadioState::Failed => ("\x1f0D9", color::TEXT, color::BAD),
    };
    // Confirmed sweep publication is more precise than the generic Starting
    // state used during retunes. Never hide a real TX or terminal RF fault.
    let (label, foreground, background) = if model.view.screen == Screen::Spectrum
        && model.view.spectrum_listening
        && !matches!(
            model.radio.state,
            RadioState::Transmitting | RadioState::Failed
        ) {
        ("\x1f0CC", color::BACKGROUND, color::GOOD)
    } else if model.view.screen == Screen::Spectrum
        && model.view.spectrum_menu
        && !matches!(
            model.radio.state,
            RadioState::Transmitting | RadioState::Failed
        )
    {
        ("\x1f0C9", color::ACCENT, color::PANEL)
    } else if model.view.screen == Screen::Spectrum
        && !matches!(
            model.radio.state,
            RadioState::Transmitting | RadioState::Failed
        )
    {
        match spectrum_phase(model) {
            Some(SpectrumPhase::Pending) => ("\x1f0CA", color::WARNING, color::WARNING_BACKGROUND),
            Some(SpectrumPhase::Sweeping) => ("\x1f0C7", color::ACCENT, color::PANEL),
            Some(SpectrumPhase::Listening) => ("\x1f0CC", color::BACKGROUND, color::GOOD),
            Some(SpectrumPhase::Preparing) => {
                ("\x1f0CB", color::WARNING, color::WARNING_BACKGROUND)
            }
            Some(SpectrumPhase::Restoring) => {
                ("\x1f052", color::WARNING, color::WARNING_BACKGROUND)
            }
            Some(SpectrumPhase::Stopped) => ("\x1f0C5", color::MUTED, color::PANEL),
            Some(SpectrumPhase::Failed) => ("\x1f0D7", color::TEXT, color::BAD),
            None => (label, foreground, background),
        }
    } else {
        (label, foreground, background)
    };
    let width = if model.view.screen == Screen::Home {
        if model.view.locked {
            146
        } else {
            170
        }
    } else {
        WIDTH
    };
    rect(row, out, 0, 28, width, 20, background);
    centered_within(row, out, 34, 1, label, foreground, width);
}

fn page_title(view: View) -> &'static str {
    match view.screen {
        Screen::Home => "\x1f11F",
        Screen::Menu => view.menu_group.map(menu_group_label).unwrap_or("\x1f09C"),
        Screen::Channels => "\x1f126",
        Screen::ChannelDetails => "\x1f066",
        Screen::Step => "\x1f13F",
        Screen::Calibration => "\x1f082",
        Screen::Keys => "\x1f0F6",
        Screen::Help => "\x1f136",
        Screen::About => "\x1f0C3",
        Screen::Shortcuts => "\x1f06D",
        Screen::ShortcutAction => "\x1f0AB",
        Screen::Zones => "\x1f080",
        Screen::SideKeys => "\x1f047",
        Screen::SideAction => "\x1f068",
        Screen::Setting => view.setting.map(setting_title).unwrap_or("\x1f0AC"),
        Screen::Spectrum => "\x1f124",
        Screen::Shortwave => "\x1f0FD",
        Screen::Crossband => "\x1f097",
        Screen::Gps => "\x1f019",
        Screen::Aprs => "\x1f00A",
        Screen::AprsRx => "\x1f183",
        Screen::Cb => cb_label(),
        Screen::Mdc if view.active_slot == Slot::A => "MDC1200 A",
        Screen::Mdc => "MDC1200 B",
        Screen::RadioName => "\x1f16D",
        Screen::RxToneSearch => "\x1f17B",
        Screen::DtmfMemory => "\x1f17E",
    }
}

fn cb_label() -> &'static str {
    "\x1f15B"
}

fn cb_grid_label(grid: CbGrid) -> &'static str {
    match grid {
        CbGrid::Zeros => "\x1f15C",
        CbGrid::Cept => "\x1f15D",
    }
}

const CB_COLUMNS: u8 = 5;
const CB_CHANNEL_HEIGHT: u16 = 18;

fn cb_channel_rows(number: u8) -> (u16, u16) {
    let y = 108 + u16::from((number - 1) / CB_COLUMNS) * 20;
    (y, y + CB_CHANNEL_HEIGHT)
}

fn cb_grid(row: u16, out: &mut Row<'_>, view: &View) {
    let Some(cb) = view.cb_draft else {
        return;
    };
    if (54..70).contains(&row) {
        let entered = view
            .entry
            .map(|entry| entry_text(entry.value, entry.digits, entry.kind));
        let frequency = frequency(cb.frequency_10hz());
        let value = entered.as_ref().map_or(frequency.as_str(), Text::as_str);
        text(row, out, 14, 54, 2, value, color::ACCENT);
        text(row, out, 131, 54, 1, cb_grid_label(cb.grid), color::TEXT);
        text(
            row,
            out,
            131,
            63,
            1,
            if cb.am { "AM" } else { "FM" },
            if cb.am {
                color::AM_BADGE
            } else {
                color::FM_BADGE
            },
        );
    } else {
        text(row, out, 14, 74, 1, "\x1f15E", color::MUTED);
        if cb.am {
            text(row, out, 14, 84, 1, "\x1f160", color::TEXT);
        }
        if view.cb.is_some() {
            text(row, out, 14, 94, 1, "\x1f15F", color::MUTED);
        }
    }
    if !(108..268).contains(&row) {
        return;
    }
    let first = 1 + ((row - 108) / 20) as u8 * CB_COLUMNS;
    for number in first..(first + CB_COLUMNS).min(CB_CHANNEL_COUNT + 1) {
        let (top, end) = cb_channel_rows(number);
        let x = 11 + usize::from((number - 1) % CB_COLUMNS) * 44;
        let selected = number == cb.number();
        rect(
            row,
            out,
            x,
            top,
            42,
            end - top,
            if selected {
                color::ACCENT
            } else {
                color::PANEL
            },
        );
        if (top + 2..top + 16).contains(&row) {
            let mut label = Text::new();
            let _ = write!(label, "{:02}", number);
            text(
                row,
                out,
                x + 10,
                top + 2,
                2,
                label.as_str(),
                if selected {
                    color::BACKGROUND
                } else {
                    color::TEXT
                },
            );
        }
    }
}

const fn mdc_option_top(option: u8) -> u16 {
    57 + option as u16 * 45
}

fn mdc_option_label(option: u8) -> &'static str {
    match option {
        0 => "\x1f163",
        _ => "UNIT ID",
    }
}

fn mdc_timing_label(timing: u8) -> &'static str {
    match timing {
        1 => "\x1f165",
        2 => "\x1f166",
        3 => "\x1f167",
        _ => "\x1f05E",
    }
}

const MDC_NAME_HINT_ROW: u16 = 209;
const MDC_NAME_HINT: &str = "\x1f171";

fn mdc_end_replaced_by_name(view: &View) -> bool {
    let settings = view.mdc_for_slot(view.active_slot);
    let timing = match view.mdc_draft {
        Some(MdcDraft::Timing(timing)) => timing,
        _ => settings.mode,
    };
    view.roger_name().is_some() && matches!(timing, 2 | 3)
}

fn mdc_page(row: u16, out: &mut Row<'_>, view: &View) {
    if (MDC_NAME_HINT_ROW..MDC_NAME_HINT_ROW + 7).contains(&row) && mdc_end_replaced_by_name(view) {
        text(
            row,
            out,
            16,
            MDC_NAME_HINT_ROW,
            1,
            MDC_NAME_HINT,
            color::MUTED,
        );
    }
    let mut value = Text::new();
    if let Some(draft) = view.mdc_draft {
        text(
            row,
            out,
            16,
            56,
            2,
            mdc_option_label(view.mdc_option),
            color::TEXT,
        );
        match draft {
            MdcDraft::Timing(timing) => {
                text(
                    row,
                    out,
                    16,
                    105,
                    1,
                    mdc_timing_label(timing),
                    color::ACCENT,
                );
            }
            MdcDraft::UnitId(editor) => {
                let _ = write!(value, "{}_", editor.value());
                text(row, out, 16, 105, 2, value.as_str(), color::ACCENT);
                text(row, out, 16, 138, 1, "\x1f168", color::MUTED);
                text(row, out, 16, 153, 1, "\x1f169", color::MUTED);
                text(row, out, 16, 168, 1, "\x1f16B", color::MUTED);
            }
        }
        return;
    }
    let settings = view.mdc_for_slot(view.active_slot);
    for option in 0..2 {
        let top = mdc_option_top(option);
        if !(top..top + 37).contains(&row) {
            continue;
        }
        let selected = option == view.mdc_option;
        rect(row, out, 12, top, 216, 37, color::PANEL);
        if selected {
            rect(row, out, 12, top, 3, 37, color::ACCENT);
        }
        text(
            row,
            out,
            21,
            top + 7,
            1,
            mdc_option_label(option),
            color::TEXT,
        );
        match option {
            0 => {
                let _ = value.write_str(mdc_timing_label(settings.mode));
            }
            _ => {
                if let Some(id) = settings.id {
                    let _ = write!(value, "{:04X}", id);
                } else {
                    let _ = value.write_str("----");
                }
            }
        }
        text(row, out, 21, top + 22, 1, value.as_str(), color::ACCENT);
    }
}

fn radio_name_page(row: u16, out: &mut Row<'_>, view: &View) {
    let Some(editor) = view.radio_name_draft else {
        return;
    };
    if (96..110).contains(&row) {
        let mut value = Text::new();
        let _ = write!(value, "{}_", editor.value());
        let scale = fitting_scale(value.as_str(), 2, 208);
        text(row, out, 16, 96, scale, value.as_str(), color::ACCENT);
    }
    text(row, out, 16, 138, 1, "\x1f16F", color::MUTED);
    text(row, out, 16, 153, 1, "\x1f169", color::MUTED);
}

const WAITING_FOR_SUBTONE: &str = "\x1f0CE";
const SUBTONE_DETECTED: &str = "\x1f12D";

fn rx_tone_search_page(row: u16, out: &mut Row<'_>, view: &View) {
    let Some(search) = view.rx_tone_search else {
        return;
    };
    let Some(raw) = search.result_raw else {
        centered(row, out, 120, 2, WAITING_FOR_SUBTONE, color::ACCENT);
        return;
    };
    centered(row, out, 92, 1, SUBTONE_DETECTED, color::GOOD);
    let value = tone_text(raw);
    centered(row, out, 130, 2, value.as_str(), color::ACCENT);
    if matches!(Tone::decode(raw), Some(Tone::Dcs { .. })) {
        centered(row, out, 182, 1, "\x1f17C", color::MUTED);
    }
    centered(row, out, 222, 1, "\x1f17D", color::MUTED);
}

fn dtmf_memory_page(row: u16, out: &mut Row<'_>, view: &View) {
    let Some(page) = view.dtmf_memory_page else {
        return;
    };
    if (78..94).contains(&row) {
        let mut label = Text::new();
        let _ = write!(label, "{} {}/3", "\x1f17F", page.slot + 1);
        centered(row, out, 78, 1, label.as_str(), color::MUTED);
    }
    if page.active && !page.editing {
        centered(row, out, 101, 1, "\x1f180", color::GOOD);
    }
    if (126..154).contains(&row) {
        let mut value = Text::new();
        if page.len == 0 {
            let _ = value.write_str("----");
        } else {
            for &byte in &page.bytes[..usize::from(page.len)] {
                let _ = value.write_char(byte as char);
            }
        }
        centered(row, out, 126, 2, value.as_str(), color::ACCENT);
    }
    if page.editing {
        centered(row, out, 190, 1, "\x1f181", color::MUTED);
    }
}

/// One user-visible build identity for the header and the About page.
#[cfg(not(feature = "pa5-audio"))]
pub const VERSION: &str = "0.1.8-r54";
#[cfg(feature = "pa5-audio")]
pub const VERSION: &str = "R42-PA5";
/// Header has a fixed 42 px slot; About shows the full string.
#[cfg(not(feature = "pa5-audio"))]
pub const VERSION_SHORT: &str = "R54";
#[cfg(feature = "pa5-audio")]
pub const VERSION_SHORT: &str = "PA5";

fn crossband(row: u16, out: &mut Row<'_>, view: &View) {
    let (status, tint) = match view.crossband_probe {
        CrossbandProbe::Starting => ("\x1f0F7", color::ACCENT),
        CrossbandProbe::InvalidPair => ("\x1f0C2", color::WARNING),
        CrossbandProbe::InvalidProbeChannels => ("\x1f058", color::WARNING),
        CrossbandProbe::BusError => ("\x1f0D6", color::BAD),
        CrossbandProbe::Stopped => ("\x1f106", color::MUTED),
        CrossbandProbe::Measuring {
            a_rx,
            b_rx,
            rssi,
            status,
            tone_match,
        } => {
            centered(row, out, 65, 2, "\x1f12E", color::ACCENT);
            for (y, label, value) in [(124, "A RX", a_rx), (152, "B RX", b_rx)] {
                let mut line = Text::new();
                let _ = write!(line, "{label}  {} \x1f09A", frequency(value).as_str());
                centered(row, out, y, 1, line.as_str(), color::TEXT);
            }
            let mut line = Text::new();
            let _ = write!(line, "B RSSI {rssi}   0C {status:04X}");
            centered(row, out, 169, 1, line.as_str(), color::TEXT);
            match tone_match {
                Some(true) => (SUBTONE_DETECTED, color::GOOD),
                Some(false) => (WAITING_FOR_SUBTONE, color::MUTED),
                None => ("\x1f12C", color::MUTED),
            }
        }
        CrossbandProbe::Ready {
            a_rx,
            b_rx,
            traffic,
            live,
        } => {
            let (direction, left, right) = match traffic {
                Some((Slot::A, rx, tx)) => ("A → B", ("\x1f057", rx), ("\x1f061", tx)),
                Some((Slot::B, rx, tx)) => ("B → A", ("\x1f057", rx), ("\x1f061", tx)),
                None => ("A / B", ("A RX", a_rx), ("B RX", b_rx)),
            };
            centered(row, out, 65, 2, direction, color::ACCENT);
            if !live {
                centered(row, out, 93, 1, "\x1f12F", color::WARNING);
            }
            for (y, (label, value)) in [(124, left), (152, right)] {
                let freq = frequency(value);
                let mut line = Text::new();
                let _ = write!(line, "{label}  {} \x1f09A", freq.as_str());
                centered(row, out, y, 1, line.as_str(), color::TEXT);
            }
            if traffic.is_some() && live {
                ("\x1f107", color::GOOD)
            } else if traffic.is_some() {
                ("\x1f113", color::GOOD)
            } else {
                ("\x1f0CD", color::MUTED)
            }
        }
    };
    centered(row, out, 190, 1, status, tint);
}

const WATERFALL_ROWS: usize = 16;
const WATERFALL_TOP: u16 = 190;
const WATERFALL_BOTTOM: u16 = WATERFALL_TOP + 4 * WATERFALL_ROWS as u16;
const WATERFALL_COLORS: [u16; 8] = [
    color::BACKGROUND,
    0x6000,
    0xf800,
    0xfde0,
    0x87e0,
    0x07e0,
    0x07ff,
    0x001f,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Waterfall {
    rows: [[u8; SPECTRUM_BINS]; WATERFALL_ROWS],
    newest: usize,
}

impl Default for Waterfall {
    fn default() -> Self {
        Self::new()
    }
}

impl Waterfall {
    pub const fn new() -> Self {
        Self {
            rows: [[0; SPECTRUM_BINS]; WATERFALL_ROWS],
            newest: 0,
        }
    }

    pub fn push(&mut self, plot: &Plot, len: u8) {
        let Some((floor, _)) = spectrum_bounds(plot) else {
            return;
        };
        self.newest = (self.newest + 1) % WATERFALL_ROWS;
        self.rows[self.newest].fill(0);
        for index in 0..usize::from(len).min(SPECTRUM_BINS) {
            if plot.valid(index) {
                let level = plot.latest[index].saturating_sub(floor);
                self.rows[self.newest][index] = 1 + (level / 8).min(6) as u8;
            }
        }
    }

    fn row(&self, age: usize) -> &[u8; SPECTRUM_BINS] {
        &self.rows[(self.newest + WATERFALL_ROWS - age) % WATERFALL_ROWS]
    }
}

const GRAPH_X: usize = 12;
const GRAPH_WIDTH: usize = 216;
const GRAPH_TOP: u16 = 116;
const GRAPH_BOTTOM: u16 = 175;
const SPECTRUM_MENU_LABELS: [&str; 8] = [
    "\x1f121", "\x1f0F4", "\x1f048", "\x1f09B", "\x1f13B", "\x1f13C", "\x1f10B", "\x1f07A",
];

fn spectrum_bin_x(index: usize, len: usize) -> usize {
    GRAPH_X + (2 * index + 1) * GRAPH_WIDTH / (2 * len.max(1))
}

fn spectrum_bounds(plot: &Plot) -> Option<(u16, u16)> {
    let low = plot.display_floor.or_else(|| {
        plot.raw_range()
            .map(|(min, _)| min.saturating_sub(8).min(447))
    })?;
    Some((low, low + 64))
}

fn spectrum_level_y(raw: u16, low: u16, high: u16) -> u16 {
    let level = raw.clamp(low, high) - low;
    GRAPH_BOTTOM
        - ((u32::from(level) * u32::from(GRAPH_BOTTOM - GRAPH_TOP)) / u32::from(high - low)) as u16
}

fn khz(value_10hz: u32) -> Text {
    let mut text = Text::new();
    let _ = write!(text, "{}.{:02}\x1f142", value_10hz / 100, value_10hz % 100);
    text
}

fn spectrum_menu_value(model: &Model<'_>, view: Option<SpectrumView<'_>>, option: u8) -> Text {
    let step = STEP_10HZ
        .get(usize::from(model.view.spectrum_step_index))
        .copied()
        .unwrap_or(0);
    let intervals = u32::from(
        *SPECTRUM_INTERVALS
            .get(usize::from(model.view.spectrum_span_index))
            .unwrap_or(&0),
    );
    match option {
        0 | 1 => {
            let mut text = Text::new();
            let frequency = view
                .and_then(|view| view.frequency_10hz(model.view.spectrum_cursor.min(view.len - 1)))
                .map(|value| frequency(value));
            match frequency {
                Some(value) => {
                    let _ = text.write_str(value.as_str());
                }
                None => {
                    let _ = text.write_str("---.-----");
                }
            }
            text
        }
        2 | 3 => khz(step.saturating_mul(intervals)),
        4 | 5 => khz(step),
        _ => Text::new(),
    }
}

fn spectrum_menu_overlay(
    row: u16,
    out: &mut Row<'_>,
    view: Option<SpectrumView<'_>>,
    model: &Model<'_>,
) -> bool {
    const STEP: u16 = 17;
    const BAND: u16 = 15;
    let total = STEP * SPECTRUM_MENU_LABELS.len() as u16 - 2;
    if row < GRAPH_TOP || row >= GRAPH_TOP + total {
        return false;
    }
    let option = model.view.spectrum_option;
    rect(
        row,
        out,
        GRAPH_X,
        GRAPH_TOP,
        GRAPH_WIDTH,
        total,
        color::PANEL,
    );
    for index in 0..SPECTRUM_MENU_LABELS.len() as u16 {
        let y = GRAPH_TOP + index * STEP;
        let selected = index as u8 == option;
        if selected && row >= y && row < y + BAND {
            rect(row, out, GRAPH_X, y, GRAPH_WIDTH, BAND, color::ACCENT);
        }
        let foreground = if selected {
            color::BACKGROUND
        } else {
            color::TEXT
        };
        text(
            row,
            out,
            18,
            y + 4,
            1,
            SPECTRUM_MENU_LABELS[index as usize],
            foreground,
        );
        let value = spectrum_menu_value(model, view, index as u8);
        text(row, out, 148, y + 4, 1, value.as_str(), foreground);
    }
    true
}

fn spectrum_geometry(row: u16, out: &mut Row<'_>, step_10hz: u32, span_10hz: u32) {
    let (top, label, value) = if (71..85).contains(&row) {
        (71, "\x1f13A", step_10hz)
    } else if (87..101).contains(&row) {
        (87, "\x1f0EC", span_10hz)
    } else {
        return;
    };
    let mut line = Text::new();
    let (whole, hundredths) = (value / 100, value % 100);
    if hundredths == 0 {
        let _ = write!(line, "{label} {whole} \x1f08E");
    } else {
        let _ = write!(line, "{label} {whole}.{hundredths:02} \x1f08E");
    }
    centered(row, out, top, 2, line.as_str(), color::TEXT);
}

fn spectrum(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    let menu = model.view.spectrum_menu;
    let Some(view) = model.spectrum else {
        let step = STEP_10HZ
            .get(usize::from(model.view.spectrum_step_index))
            .copied()
            .unwrap_or(0);
        let intervals = SPECTRUM_INTERVALS
            .get(usize::from(model.view.spectrum_span_index))
            .copied()
            .unwrap_or(0);
        spectrum_geometry(row, out, step, step * u32::from(intervals));
        // Overlay and window controls must stay available before the first
        // measured point and after any refused start.
        if (103..110).contains(&row) {
            centered(
                row,
                out,
                103,
                1,
                model.spectrum_error.label(),
                color::WARNING,
            );
        }
        if !menu && (110..117).contains(&row) {
            centered(row, out, 110, 1, "\x1f02C", color::MUTED);
        }
        if menu {
            spectrum_menu_overlay(row, out, None, model);
        }
        if model.view.notice == Notice::None {
            if menu {
                centered(row, out, 269, 1, "\x1f02D", color::ACCENT);
            } else {
                centered(row, out, 269, 1, "\x1f015", color::MUTED);
            }
        }
        return;
    };
    let cursor = model.view.spectrum_cursor.min(view.len - 1);
    if (48..69).contains(&row) {
        let value = frequency(view.frequency_10hz(cursor).unwrap_or(0));
        centered(row, out, 48, 3, value.as_str(), color::TEXT);
    }
    spectrum_geometry(row, out, view.step_10hz, view.span_10hz().unwrap_or(0));
    if (104..111).contains(&row) {
        let mut line = Text::new();
        match view.plot.strongest() {
            Some((index, raw)) => {
                let at = view.frequency_10hz(index).unwrap_or(0);
                let _ = write!(
                    line,
                    "\x1f0F8 {}  \x1f114 {} {raw:3}",
                    view.plot.sweep,
                    frequency(at).as_str()
                );
            }
            None => {
                let _ = write!(line, "\x1f0F8 {}  \x1f115", view.plot.sweep);
            }
        }
        centered(row, out, 104, 1, line.as_str(), color::WARNING);
    }
    if !menu && (112..116).contains(&row) {
        if let Some(bin) = view.plot.trigger_bin.filter(|&bin| bin < view.len) {
            let x = spectrum_bin_x(usize::from(bin), usize::from(view.len));
            out[x - 2..x + 3].fill(color::WARNING);
        }
    }
    if !menu && (GRAPH_TOP..=GRAPH_BOTTOM).contains(&row) {
        rect(
            row,
            out,
            GRAPH_X,
            GRAPH_TOP,
            GRAPH_WIDTH,
            GRAPH_BOTTOM - GRAPH_TOP + 1,
            color::PANEL,
        );
        for tick in 0..=8 {
            let x = GRAPH_X + tick * (GRAPH_WIDTH - 1) / 8;
            out[x] = color::LINE;
        }
        for y in [GRAPH_TOP, 136, 155, GRAPH_BOTTOM] {
            rect(row, out, GRAPH_X, y, GRAPH_WIDTH, 1, color::LINE);
        }
        let (low, high) = spectrum_bounds(view.plot).unwrap_or((0, 64));
        let bins = usize::from(view.len);
        for index in 0..bins {
            let x = GRAPH_X + index * GRAPH_WIDTH / bins;
            let next = GRAPH_X + (index + 1) * GRAPH_WIDTH / bins;
            let end = next - usize::from(next - x >= 3);
            if view.plot.valid(index) {
                let tip = spectrum_level_y(view.plot.latest[index], low, high);
                if row >= tip {
                    let foreground = if row < tip + 2 && view.plot.fresh(index) {
                        color::ACCENT
                    } else {
                        0xfce0
                    };
                    out[x..end].fill(foreground);
                }
            }
            if view.plot.peak_valid(index)
                && row == spectrum_level_y(view.plot.peaks[index], low, high)
            {
                out[x..end].fill(color::WARNING);
            }
        }
    }
    if !menu && (176..183).contains(&row) {
        let x = spectrum_bin_x(usize::from(cursor), usize::from(view.len));
        let spread = usize::from(row - 176) / 2;
        out[x.saturating_sub(spread)..(x + 2 + spread).min(WIDTH)].fill(color::ACCENT);
    }
    if !menu && (183..190).contains(&row) {
        for (bin, align) in [(0, 0), ((view.len - 1) / 2, 1), (view.len - 1, 2)] {
            let khz = view.frequency_10hz(bin).unwrap_or(0) / 100;
            let mut label = Text::new();
            let _ = write!(label, "{}.{:03}", khz / 1000, khz % 1000);
            let label = label.as_str();
            let width = localized_width(label, 1, out.language);
            let x = match align {
                0 => GRAPH_X,
                1 => (WIDTH - width) / 2,
                _ => GRAPH_X + GRAPH_WIDTH - width,
            };
            text(row, out, x, 183, 1, label, color::TEXT);
        }
    }
    if !menu && (WATERFALL_TOP..WATERFALL_BOTTOM).contains(&row) {
        if let Some(history) = view.waterfall {
            let levels = history.row(usize::from((row - WATERFALL_TOP) / 4));
            let bins = usize::from(view.len);
            for (index, &level) in levels[..bins].iter().enumerate() {
                let start = GRAPH_X + index * GRAPH_WIDTH / bins;
                let end = GRAPH_X + (index + 1) * GRAPH_WIDTH / bins;
                out[start..end].fill(WATERFALL_COLORS[usize::from(level)]);
            }
        }
    }
    if !menu && (255..262).contains(&row) {
        let index = usize::from(cursor);
        let mut reading = Text::new();
        if view.plot.valid(index) {
            let _ = write!(
                reading,
                "RSSI {:3}   {} \x1f0A2   {} \x1f134",
                view.plot.latest[index], view.plot.elapsed_ms, view.len
            );
        } else {
            let _ = reading.write_str("\x1f038");
        }
        centered(row, out, 255, 1, reading.as_str(), color::TEXT);
    }
    if !menu && (264..271).contains(&row) {
        let mut info = Text::new();
        let mode = if model.view.spectrum_auto_listen {
            "\x1f03C"
        } else {
            "\x1f03D"
        };
        let _ = write!(info, "{mode} \x1f000{}  4+ 5−", model.view.spectrum_trigger);
        centered(row, out, 264, 1, info.as_str(), color::MUTED);
    }
    if menu {
        spectrum_menu_overlay(row, out, Some(view), model);
    }
    if model.view.notice == Notice::None {
        if menu {
            centered(row, out, 269, 1, "\x1f02D", color::ACCENT);
            centered(
                row,
                out,
                283,
                1,
                if model.view.spectrum_listening {
                    "\x1f146"
                } else {
                    "\x1f147"
                },
                color::MUTED,
            );
        } else {
            let (status, foreground) = match spectrum_phase(model) {
                Some(SpectrumPhase::Pending) => ("\x1f0E4", color::WARNING),
                Some(SpectrumPhase::Preparing) => ("\x1f0E3", color::WARNING),
                Some(SpectrumPhase::Sweeping) if view.plot.held_bin.is_some() => {
                    ("\x1f0E0", color::WARNING)
                }
                Some(SpectrumPhase::Sweeping) => ("\x1f143", color::ACCENT),
                Some(SpectrumPhase::Listening) => ("\x1f120", color::GOOD),
                Some(SpectrumPhase::Restoring) => ("\x1f054", color::WARNING),
                Some(SpectrumPhase::Stopped) => ("\x1f0C5", color::MUTED),
                Some(SpectrumPhase::Failed) => ("\x1f0D7", color::BAD),
                None => ("\x1f0C5", color::MUTED),
            };
            centered(row, out, 274, 1, status, foreground);
            let keys = if spectrum_phase(model) == Some(SpectrumPhase::Sweeping) {
                "\x1f006"
            } else if spectrum_phase(model) == Some(SpectrumPhase::Listening) {
                "\x1f001"
            } else {
                "\x1f036"
            };
            centered(row, out, 283, 1, keys, color::MUTED);
        }
    }
}

fn shortwave(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    use crate::controller::{ShortwaveMode, ShortwaveStatus, SW_BANDS};
    let view = model.view;
    if let Some(draft) = view.shortwave_band_draft {
        centered(row, out, 52, 2, "\x1f06A", color::TEXT);
        for (index, &(name, low, high, _)) in SW_BANDS.iter().enumerate() {
            let y = 74 + index as u16 * 20;
            if !(y..y + 17).contains(&row) {
                continue;
            }
            let selected = usize::from(draft) == index;
            let foreground = if selected { color::ACCENT } else { color::TEXT };
            rect(
                row,
                out,
                10,
                y,
                220,
                17,
                if selected {
                    color::PANEL
                } else {
                    color::BACKGROUND
                },
            );
            let mut line = Text::new();
            if index == usize::from(crate::controller::SW_FM_BAND) {
                let _ = write!(line, "{name}  {}-{} \x1f09A", low / 1000, high / 1000);
            } else {
                let _ = write!(line, "{name}  {low}-{high}");
            }
            text(row, out, 20, y + 5, 1, line.as_str(), foreground);
        }
        return;
    }

    if view.shortwave_scope && view.shortwave_entry.is_none() {
        shortwave_scope_page(row, out, model);
        return;
    }
    rect(row, out, 12, 55, 216, 178, color::PANEL);
    let mut band = Text::new();
    let _ = write!(
        band,
        "VFO   {}   {}",
        view.shortwave_mode.label(),
        SW_BANDS[usize::from(view.shortwave_band)].0
    );
    centered(row, out, 63, 1, band.as_str(), color::ACCENT);
    let mut line = Text::new();
    let khz = view.shortwave_entry.unwrap_or(view.shortwave_khz);
    let fm = view.shortwave_mode == ShortwaveMode::Fm;
    if fm && view.shortwave_entry.is_none() {
        let _ = write!(line, "{}.{:03}", khz / 1000, khz % 1000);
    } else if view.shortwave_mode.is_ssb() && view.shortwave_entry.is_none() {
        let _ = write!(line, "{khz}.{:03}", view.shortwave_fraction_hz);
    } else {
        let _ = write!(line, "{khz}");
    }
    centered(row, out, 86, 4, line.as_str(), color::TEXT);
    centered(
        row,
        out,
        132,
        2,
        if fm && view.shortwave_entry.is_none() {
            "\x1f09A"
        } else {
            "\x1f08E"
        },
        color::MUTED,
    );
    let mut hint = Text::new();
    if view.shortwave_entry.is_some() {
        let _ = hint.write_str("\x1f025");
    } else {
        let step = view.shortwave_step_hz(view.shortwave_scan);
        let label = if view.shortwave_scan {
            "\x1f13E"
        } else {
            "\x1f145"
        };
        if step < 1000 {
            let _ = write!(hint, "{label} {step} \x1f065");
        } else if step % 1000 == 0 {
            let _ = write!(hint, "{label} {} \x1f08E", step / 1000);
        } else {
            let _ = write!(
                hint,
                "{label} {}.{} \x1f08E",
                step / 1000,
                step % 1000 / 100
            );
        }
    }
    centered(row, out, 155, 1, hint.as_str(), color::ACCENT);
    if view.shortwave_mode.is_ssb() {
        let mut bfo = Text::new();
        let _ = write!(bfo, "BFO {:+} \x1f065", view.shortwave_bfo);
        centered(row, out, 167, 1, bfo.as_str(), color::MUTED);
    }
    let mut signal = Text::new();
    let (state, foreground) = match view.shortwave_status {
        ShortwaveStatus::Recovering(code) => {
            let _ = write!(signal, "\x1f096 {code:02X}");
            ("\x1f0E2", color::WARNING)
        }
        ShortwaveStatus::Patching(percent) => {
            let _ = write!(signal, "{percent}%   SI4732");
            ("\x1f079", color::WARNING)
        }
        ShortwaveStatus::Starting | ShortwaveStatus::Listening { .. } if view.shortwave_scan => {
            ("\x1f118", color::ACCENT)
        }
        ShortwaveStatus::Starting => ("\x1f0AD", color::MUTED),
        ShortwaveStatus::Listening { part, rssi, snr } => {
            let _ = write!(signal, "ID {part:02X}   RSSI {rssi}   SNR {snr}");
            ("\x1f0EF", color::GOOD)
        }
        ShortwaveStatus::Fault(code) => {
            let _ = write!(signal, "\x1f096 {code:02X}   \x1f023");
            ("\x1f0DA", color::BAD)
        }
    };
    centered(row, out, 184, 2, state, foreground);
    centered(row, out, 213, 1, signal.as_str(), color::MUTED);
    shortwave_processing_label(row, out, view, 238);
    centered(row, out, 252, 1, "\x1f002", color::ACCENT);
    centered(
        row,
        out,
        266,
        1,
        if view.shortwave_mode == ShortwaveMode::Am {
            "\x1f070"
        } else if view.shortwave_mode.is_ssb() {
            "\x1f071"
        } else {
            "\x1f072"
        },
        color::MUTED,
    );
    centered(row, out, 280, 1, "\x1f093", color::ACCENT);
}

fn home(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    let slot = if row < home_slot_rows(Slot::B).0 {
        Slot::A
    } else {
        Slot::B
    };
    let (top, end) = home_slot_rows(slot);
    let transmitting = model.radio.transmitting_slot() == Some(slot);
    if transmitting && ((top..top + 3).contains(&row) || (end - 3..end).contains(&row)) {
        rect(row, out, 4, row, 232, 1, color::TX_OUTLINE);
        return;
    }
    slot_card(row, out, model, slot);
    if transmitting {
        if let Some(status) = model.manual_tx {
            tx_status(row, out, slot, status);
        }
        rect(row, out, 4, top, 3, end - top, color::TX_OUTLINE);
        rect(row, out, 233, top, 3, end - top, color::TX_OUTLINE);
    } else if model.radio.receiving_slot() == Some(slot) && (top..end).contains(&row) {
        if row == top || row == end - 1 {
            out[4..236].fill(color::RX_OUTLINE);
        } else {
            out[4] = color::RX_OUTLINE;
            out[235] = color::RX_OUTLINE;
        }
    }
}

#[inline(never)]
fn tx_status(row: u16, out: &mut Row<'_>, slot: Slot, status: ManualTxStatus) {
    let (start, end) = home_tx_status_rows(slot);
    if !(start..end).contains(&row) {
        return;
    }
    let mut value = Text::new();
    let _ = write!(value, "{} \x1f10A", status.remaining_seconds);
    text(row, out, 46, start, 1, value.as_str(), color::TEXT);
    if let Some(sag) = status.sag_mv {
        let mut value = Text::new();
        if sag != 0 {
            let _ = value.write_char('-');
        }
        let _ = write!(value, "{} mV", sag);
        text(row, out, 140, start, 1, value.as_str(), color::MUTED);
    }
}

fn slot_content<'a>(model: &Model<'a>, slot: Slot) -> PeerView<'a> {
    if slot == model.view.active_slot {
        PeerView {
            slot,
            mode: model.view.mode,
            frequency_10hz: home_frequency(model),
            tx_frequency_10hz: None,
            channel: model.view.channel,
            name: model.channel.map(|channel| channel.name).unwrap_or(&[]),
            zone: model.view.zone,
            zone_name: model.zone_name,
            reverse: model.view.reverse,
        }
    } else {
        let mut peer = model
            .peer
            .filter(|peer| peer.slot == slot)
            .unwrap_or(PeerView {
                slot,
                mode: Mode::Vfo,
                frequency_10hz: None,
                tx_frequency_10hz: None,
                channel: None,
                name: &[],
                zone: None,
                zone_name: None,
                reverse: false,
            });
        if model.radio.transmitting_slot() == Some(slot) {
            peer.frequency_10hz = peer.tx_frequency_10hz;
        }
        peer
    }
}

#[inline(never)]
fn slot_card(row: u16, out: &mut Row<'_>, model: &Model<'_>, slot: Slot) {
    let (top, end) = home_slot_rows(slot);
    if !(top..end).contains(&row) {
        return;
    }
    let active = slot == model.view.active_slot;
    let transmitting = model.radio.transmitting_slot() == Some(slot);
    let cb = model.view.cb_for_slot(slot);
    let content = slot_content(model, slot);
    let border = home_frame_ink(model.view.active_slot, slot);
    let background = if active {
        color::PANEL
    } else {
        color::BACKGROUND
    };
    rect(row, out, 4, top, 232, end - top, background);
    rect(row, out, 4, top, 232, 1, border);
    rect(row, out, 4, end - 1, 232, 1, border);
    rect(
        row,
        out,
        4,
        top,
        if border == color::LINE { 1 } else { 3 },
        end - top,
        border,
    );
    rect(row, out, 235, top, 1, end - top, border);
    rect(row, out, 14, top + 25, 212, 1, color::LINE);
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
                if let Some(cb) = cb {
                    let _ = write!(mode, "CB {:02}", cb.number());
                } else {
                    let _ = mode.write_str("VFO");
                }
            }
        }
        text(row, out, 38, top + 7, 1, mode.as_str(), color::TEXT);
        let mut zone = Text::new();
        if let Some(cb) = cb {
            let _ = zone.write_str(cb_grid_label(cb.grid));
        } else if content.zone.is_none() {
            let _ = zone.write_str("\x1f07F");
        } else if let Some(name) = content.zone_name {
            let decoded = channel_name(name);
            let _ = write!(zone, "\x1f07E {}", decoded.as_str());
        } else {
            let _ = write!(zone, "\x1f07E {}", u16::from(content.zone.unwrap()) + 1);
        }
        text(row, out, 96, top + 7, 1, zone.as_str(), color::MUTED);
    }
    if transmitting {
        let (badge_start, badge_end) = home_ptt_badge_rows(slot);
        rect(
            row,
            out,
            172,
            badge_start,
            63,
            badge_end - badge_start,
            background,
        );
        rect(
            row,
            out,
            176,
            badge_start,
            50,
            badge_end - badge_start,
            color::TX,
        );
        text(
            row,
            out,
            187,
            badge_start + 5,
            1,
            if slot == Slot::A { "PTT A" } else { "PTT B" },
            color::TEXT,
        );
    }
    let entry = if active { model.view.entry } else { None };
    let name_y = top + HOME_NAME_Y;
    if (name_y..name_y + 14).contains(&row) {
        let decoded = channel_name(content.name);
        let label = match entry.map(|entry| entry.kind) {
            Some(EntryKind::Channel | EntryKind::CbChannel) => "\x1f0C0",
            Some(EntryKind::Frequency) => "\x1f04B",
            None if content.mode == Mode::Vfo => "",
            None if content.channel.is_none() => "\x1f0B5",
            None => decoded.as_str(),
        };
        text(row, out, 14, name_y, 2, label, color::TEXT);
        if entry.is_none() {
            text(row, out, 208, name_y + 6, 1, "\x1f09A", color::MUTED);
        }
    }
    let (frequency_start, frequency_end) = home_frequency_rows(slot);
    if (frequency_start..frequency_end).contains(&row) {
        let value = content.frequency_10hz.map(frequency);
        let entered = entry.map(|entry| entry_text(entry.value, entry.digits, entry.kind));
        let digits = entered
            .as_ref()
            .map(Text::as_str)
            .or_else(|| value.as_ref().map(FrequencyText::as_str))
            .unwrap_or("---.-----");
        home_digits(
            row,
            out,
            frequency_start,
            digits,
            if active { color::ACCENT } else { color::TEXT },
            background,
        );
    }
    if entry.is_some() {
        if transmitting {
            slot_meter(
                row,
                out,
                top + HOME_METER_Y,
                home_meter(&model.view, model.radio, slot),
            );
        } else {
            text(row, out, 16, top + HOME_METER_Y, 1, "\x1f02E", color::MUTED);
        }
        return;
    }
    let live_metadata = home_radio_metadata(&model.view, model.radio, slot);
    if active {
        if cb.is_none() && (top + 104..top + 111).contains(&row) {
            let setup = match content.mode {
                Mode::Channel => model.channel.filter(|ch| ch.active).and_then(|ch| {
                    let (rx, tx, _, tone) = displayed_channel(ch, model.view.reverse);
                    (valid_frequency(tx) && (rx != tx || tone != 0)).then_some((tx, tone))
                }),
                Mode::Vfo if model.view.shift_index != 0 || model.view.tx_tone_index != 0 => {
                    let tone = if model.view.reverse {
                        0
                    } else {
                        TX_CTCSS
                            .get(usize::from(model.view.tx_tone_index))
                            .copied()
                            .filter(|tone| *tone != 0)
                            .map_or(0, |tone| 0x1000 | tone)
                    };
                    let tx = if model.view.reverse {
                        Some(model.view.vfo_10hz)
                    } else {
                        crate::controller::vfo_tx_frequency(&model.view)
                    };
                    tx.map(|tx| (tx, tone))
                }
                Mode::Vfo => None,
            };
            if let Some((tx, tone)) = setup {
                let mut line = Text::new();
                let _ = write!(line, "TX {}", frequency(tx).as_str());
                if tone != 0 {
                    let _ = write!(line, "  {}", tone_text(tone).as_str());
                }
                text(row, out, 14, top + 104, 1, line.as_str(), color::MUTED);
            }
        }
        if cb.is_none()
            && content.mode == Mode::Vfo
            && (top + HOME_META_Y..top + HOME_META_Y + 7).contains(&row)
        {
            let mut info = Text::new();
            let _ = write!(
                info,
                "\x1f13A {} \x1f08E",
                step_label(model.view.step_index)
            );
            text(
                row,
                out,
                16,
                top + HOME_META_Y,
                1,
                info.as_str(),
                color::MUTED,
            );
        }
        if (top + HOME_META_Y..top + HOME_META_Y + 11).contains(&row) {
            ham_status(104 + row - (top + HOME_META_Y), out, model, live_metadata);
        }
    }
    if live_metadata {
        let (label, ink) = match model.radio.modulation {
            Modulation::Am if !(transmitting && cb.is_some()) => ("AM", color::AM_BADGE),
            _ => ("FM", color::FM_BADGE),
        };
        let x = if content.mode == Mode::Vfo { 108 } else { 148 };
        text(row, out, x, top + HOME_META_Y + 2, 1, label, ink);
    }
    if live_metadata || active {
        let power = if live_metadata {
            model.radio.power
        } else {
            None
        };
        let ink = match power.and_then(Power::decode) {
            Some(Power::Low) => 0x001f,    // Native panel BGR565: red.
            Some(Power::Medium) => 0x07ff, // Yellow.
            Some(Power::High) => 0x07e0,   // Green.
            None => color::MUTED,
        };
        // The acknowledged setting belongs to the actual RF slot, including
        // lower-PTT transmission while the editing focus remains on A.
        text(
            row,
            out,
            210,
            top + HOME_META_Y - 3,
            2,
            power.map(power_label).unwrap_or("?"),
            ink,
        );
    }
    slot_meter(
        row,
        out,
        top + HOME_METER_Y,
        home_meter(&model.view, model.radio, slot),
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MeterReading {
    Inactive,
    Unavailable,
    Receive(Option<u8>),
    Transmit(Option<u8>),
}

impl MeterReading {
    fn from_radio(radio: RadioView) -> Self {
        match radio.state {
            RadioState::Receiving => Self::Receive(receive_segments(radio.rssi, radio.meter_floor)),
            RadioState::Transmitting => Self::Transmit(transmit_segments(radio.power)),
            _ => Self::Unavailable,
        }
    }
}

fn home_meter(view: &View, radio: RadioView, slot: Slot) -> MeterReading {
    let meter_slot = if radio.state == RadioState::Transmitting {
        radio.transmitting_slot()
    } else {
        radio.receiving_slot().or(Some(view.active_slot))
    };
    if meter_slot == Some(slot) {
        MeterReading::from_radio(radio)
    } else {
        MeterReading::Inactive
    }
}

fn home_radio_metadata(view: &View, radio: RadioView, slot: Slot) -> bool {
    if radio.state == RadioState::Receiving {
        view.active_slot == slot
    } else {
        radio.transmitting_slot() == Some(slot)
    }
}

/// Shared ruled 10+2 geometry for RX relative level and confirmed TX setting.
/// A saved peer slot has no live RF report, so its bar stays visibly empty.
fn slot_meter(row: u16, out: &mut Row<'_>, top: u16, reading: MeterReading) {
    if !(top..top + 15).contains(&row) {
        return;
    }
    const LEFT: usize = 46;
    const PITCH: usize = 15;
    let (label, unit, lit, enabled) = match reading {
        MeterReading::Receive(value) => (
            "RX",
            if value.is_some() { "REL" } else { "--" },
            value,
            true,
        ),
        MeterReading::Transmit(value) => {
            ("TX", if value.is_some() { "SET" } else { "?" }, value, true)
        }
        MeterReading::Inactive => ("--", "--", None, false),
        MeterReading::Unavailable => ("RX", "--", None, false),
    };
    text(
        row,
        out,
        16,
        top,
        1,
        label,
        if matches!(reading, MeterReading::Transmit(_)) {
            color::TX
        } else {
            color::MUTED
        },
    );
    text(row, out, 16, top + 8, 1, unit, color::MUTED);
    for index in 0..METER_SEGMENTS {
        // Native BGR565. Dim shades show the same boundaries at low signal.
        let (bright, dim) = match index {
            0..=1 => (0x319f, 0x0848), // red: 0–2
            2..=5 => (0x075f, 0x01c8), // yellow: 2–6
            6..=7 => (0xfac6, 0x40a2), // blue: 6–8
            _ => (0x37e5, 0x09e1),     // green: 8–10, +1 and +2
        };
        let fill = if !enabled {
            color::LINE
        } else if lit.is_some_and(|count| index < count) {
            bright
        } else {
            dim
        };
        rect(
            row,
            out,
            LEFT + usize::from(index) * PITCH,
            top,
            10,
            7,
            fill,
        );
    }
    for tick in 0..=METER_SEGMENTS {
        let x = LEFT + usize::from(tick) * PITCH;
        rect(
            row,
            out,
            x,
            top + 7,
            1,
            if tick % 2 == 0 || tick > 10 { 1 } else { 0 },
            color::LINE,
        );
    }
    if matches!(reading, MeterReading::Transmit(_)) {
        for (mark, label) in [(3, "L"), (6, "M"), (10, "H")] {
            text(
                row,
                out,
                LEFT + mark * PITCH - 2,
                top + 8,
                1,
                label,
                color::MUTED,
            );
        }
    } else {
        // Relative divisions only: +1/+2 are extra cells, NOT dB over S9.
        for (x, label) in [
            (44, "0"),
            (74, "2"),
            (104, "4"),
            (134, "6"),
            (164, "8"),
            (190, "10"),
            (205, "+1"),
            (220, "+2"),
        ] {
            text(row, out, x, top + 8, 1, label, color::MUTED);
        }
    }
}

fn home_frequency(model: &Model<'_>) -> Option<u32> {
    let transmitting = model.radio.transmitting_slot() == Some(model.view.active_slot);
    match model.view.mode {
        Mode::Channel => model
            .channel
            .filter(|channel| channel.active)
            .map(|channel| {
                let (rx, tx, _, _) = displayed_channel(channel, model.view.reverse);
                if transmitting {
                    tx
                } else {
                    rx
                }
            }),
        Mode::Vfo => {
            let reverse =
                model.view.reverse && model.view.cb_for_slot(model.view.active_slot).is_none();
            let shifted = reverse ^ transmitting;
            if shifted {
                crate::controller::vfo_tx_frequency(&model.view)
            } else {
                Some(model.view.vfo_10hz)
            }
        }
    }
}

fn home_status(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    if model.view.scan != ScanState::Stopped {
        let label = match model.view.scan {
            ScanState::Requested => "\x1f119",
            ScanState::Running => "\x1f118",
            ScanState::Paused => "\x1f175",
            ScanState::Stopping => "\x1f11A",
            ScanState::Stopped => "",
        };
        centered_within(row, out, 6, 1, label, color::GOOD, 170);
        if model.view.scan == ScanState::Running {
            symbol(row, out, 148, 1, Symbol::Scan, color::GOOD);
        }
    } else if model.radio.monitor {
        centered_within(row, out, 6, 1, "\x1f030", color::GOOD, 170);
    } else if model.radio.tx_trial {
        centered_within(
            row,
            out,
            6,
            1,
            "TX TEST: CH002 / A / 5S",
            color::WARNING,
            170,
        );
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

fn symbol(row: u16, out: &mut Row<'_>, x: usize, y: u16, symbol: Symbol, color: u16) {
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

fn details(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    let Some(channel) = model.channel else {
        centered(row, out, 128, 2, "\x1f088", color::TEXT);
        return;
    };
    let reversed = model.view.mode == Mode::Channel && model.view.reverse;
    let (rx, tx, rx_tone, tx_tone) = displayed_channel(channel, reversed);
    if (60..67).contains(&row) {
        let mut number = Text::new();
        let _ = write!(
            number,
            "\x1f085 {:04}{}",
            channel.index + 1,
            if reversed { " \x1f003" } else { "" }
        );
        text(row, out, 16, 60, 1, number.as_str(), color::MUTED);
    }
    if (80..94).contains(&row) {
        let name = channel_name(channel.name);
        text(row, out, 16, 80, 2, name.as_str(), color::TEXT);
    }
    text(row, out, 16, 112, 1, "\x1f0F0", color::MUTED);
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
    text(row, out, 16, 185, 1, "\x1f0DC", color::MUTED);
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
            "\x1f0EC {} \x1f004 {}",
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

fn steps(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
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
        let _ = write!(label, "{} \x1f08E", step_label(index as u8));
        text(row, out, 24, y + 8, 2, label.as_str(), item_color(selected));
    }
    text(row, out, 16, 281, 1, "\x1f13D", color::MUTED);
}

fn calibration(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    text(row, out, 16, 68, 2, "\x1f0F5", color::TEXT);
    text(row, out, 16, 107, 1, "\x1f007", color::MUTED);
    let (label, foreground) = match model.calibration_matches {
        Some(true) => ("\x1f067", color::GOOD),
        Some(false) => ("\x1f0B3", color::BAD),
        None => ("\x1f0B2", color::WARNING),
    };
    rect(row, out, 12, 132, 216, 52, color::PANEL);
    centered(row, out, 151, 2, label, foreground);
    text(row, out, 16, 214, 1, "\x1f129", color::MUTED);
    text(row, out, 16, 249, 1, "\x1f083", color::MUTED);
    text(row, out, 16, 268, 1, "\x1f0DF", color::MUTED);
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
        Key::Up => "\x1f04A",
        Key::Down => "\x1f051",
        Key::Exit => "EXIT",
        Key::Star => "*",
        Key::Hash => "#",
        Key::Ptt => "PTT",
        Key::Aux16 => "\x1f044",
        Key::Aux17 => "\x1f045",
        Key::Aux18 => "\x1f046",
    }
}

fn keys(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    text(row, out, 16, 68, 1, "\x1f0ED", color::MUTED);
    let key = model.last_key.or(model.view.last_key);
    centered(
        row,
        out,
        101,
        3,
        key.map(key_label).unwrap_or("\x1f0A4"),
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
            "\x1f0A3"
        } else {
            "\x1f0D5"
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
            if index == 0 { "\x1f04D" } else { "\x1f0BB" },
            if pressed { "\x1f0A3" } else { "\x1f0D5" }
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
            "\x1f03A"
        } else {
            "\x1f034"
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
            "\x1f062"
        } else {
            "\x1f0D3"
        },
        color::MUTED,
    );
}

fn help(row: u16, out: &mut Row<'_>) {
    for (index, (key, meaning)) in [
        ("↑ / ↓", "\x1f05A"),
        ("MENU", "\x1f09D"),
        ("EXIT", "\x1f0A8"),
        ("*", "\x1f00C"),
        ("#", "\x1f140"),
        ("0...9", "\x1f0BE"),
        ("0-9", "\x1f076"),
    ]
    .iter()
    .enumerate()
    {
        let y = 56 + index as u16 * 24;
        text(row, out, 16, y, 2, key, color::ACCENT);
        text(row, out, 88, y + 4, 1, meaning, color::TEXT);
    }
    text(row, out, 16, 216, 1, "\x1f073", color::ACCENT);
    text(row, out, 16, 232, 1, "\x1f037", color::MUTED);
    text(row, out, 16, 248, 1, "\x1f00D", color::MUTED);
    text(row, out, 16, 264, 1, "\x1f03B", color::MUTED);
    text(row, out, 16, 284, 1, "\x1f074", color::MUTED);
}

fn about(row: u16, out: &mut Row<'_>) {
    text(row, out, 16, 68, 3, "UV98 RUST", color::ACCENT);
    text(row, out, 16, 112, 2, "\x1f04C", color::TEXT);
    text(
        row,
        out,
        16 + localized_width("\x1f04C ", 2, out.language),
        112,
        2,
        VERSION,
        color::TEXT,
    );
    for (y, label) in [
        (156, "\x1f08C"),
        (180, "\x1f12B"),
        (204, "\x1f084"),
        (248, "\x1f0AE"),
        (270, "\x1f0EE"),
    ] {
        text(row, out, 16, y, 1, label, color::MUTED);
    }
}

fn notice_label(value: Notice) -> Option<&'static str> {
    Some(match value {
        Notice::None => return None,
        Notice::AuxBinding => "\x1f0A7",
        Notice::InvalidChannel => "\x1f0C1",
        Notice::InvalidCbChannel => "\x1f161",
        Notice::InvalidMdcId => "\x1f16B",
        Notice::NameSaved => "\x1f172",
        Notice::NameSaveUnconfirmed => "\x1f173",
        Notice::InactiveChannel => "\x1f049",
        Notice::InvalidFrequency => "\x1f139",
        Notice::EntryFull => "\x1f098",
        Notice::Locked => "\x1f043",
        Notice::TxDisabled => "\x1f0DD",
        Notice::NoChannels => "\x1f122",
        Notice::ChannelOnly => "\x1f103",
        Notice::RadioUnavailable => "\x1f0FE",
        Notice::SpectrumUnavailable => "\x1f0C4",
        Notice::ScanPrioritySet => "\x1f177",
        Notice::ScanPriorityCleared => "\x1f178",
        Notice::ScanPriorityUnavailable => "\x1f179",
        Notice::ScanPriorityStopScan => "\x1f17A",
    })
}

fn notice(row: u16, out: &mut Row<'_>, value: Notice) {
    let Some(label) = notice_label(value) else {
        return;
    };
    rect(row, out, 0, 270, WIDTH, 25, color::WARNING_BACKGROUND);
    centered(row, out, 279, 1, label, color::WARNING);
}

fn footer(row: u16, out: &mut Row<'_>, model: &Model<'_>) {
    rect(row, out, 0, 296, WIDTH, 24, color::PANEL);
    rect(row, out, 0, 296, WIDTH, 1, color::LINE);
    let (left, right) = if model.view.locked {
        ("\x1f135", "\x1f0FF")
    } else if model.view.entry.is_some() {
        ("\x1f027", "\x1f013")
    } else {
        match model.view.screen {
            Screen::Home if model.view.scan != ScanState::Stopped => ("\x1f020", "\x1f014"),
            Screen::Home if model.radio.monitor => ("\x1f020", "\x1f011"),
            Screen::Home => ("", ""),
            Screen::Menu | Screen::Channels | Screen::Cb => ("\x1f01E", "\x1f012"),
            Screen::RxToneSearch
                if model
                    .view
                    .rx_tone_search
                    .is_some_and(|search| search.result_raw.is_none()) =>
            {
                ("", "\x1f012")
            }
            Screen::RxToneSearch => ("\x1f01E", "\x1f012"),
            Screen::DtmfMemory if model.view.dtmf_memory_page.is_some_and(|page| page.editing) => {
                ("\x1f027", "\x1f013")
            }
            Screen::DtmfMemory => ("\x1f01F", "\x1f012"),
            Screen::Step => ("\x1f026", "\x1f013"),
            Screen::Shortcuts => ("\x1f01F", "\x1f012"),
            Screen::ShortcutAction => ("\x1f027", "\x1f013"),
            Screen::Zones => ("\x1f01E", "\x1f012"),
            Screen::SideKeys => ("\x1f01F", "\x1f012"),
            Screen::SideAction => ("\x1f027", "\x1f013"),
            Screen::Setting => ("\x1f024", "\x1f013"),
            Screen::Spectrum if model.view.spectrum_menu => ("\x1f027", "\x1f012"),
            Screen::Spectrum if model.view.spectrum_pending_listen.is_some() => {
                ("\x1f022", "\x1f013")
            }
            Screen::Spectrum if model.view.spectrum_listening => ("\x1f01D", "\x1f014"),
            Screen::Spectrum => ("\x1f01D", "\x1f010"),
            Screen::Shortwave if model.view.shortwave_band_draft.is_some() => {
                ("\x1f01E", "\x1f012")
            }
            Screen::Shortwave => ("\x1f028", "\x1f012"),
            Screen::Crossband => ("\x1f075", "EXIT"),
            Screen::Aprs => ("MENU", "\x1f012"),
            Screen::AprsRx => ("", "\x1f012"),
            Screen::RadioName => ("\x1f027", "\x1f013"),
            Screen::Mdc if model.view.mdc_draft.is_some() => ("\x1f027", "\x1f013"),
            Screen::Mdc => ("\x1f01F", "\x1f012"),
            _ => ("\x1f021", "\x1f012"),
        }
    };
    text(row, out, 12, 305, 1, left, color::ACCENT);
    text(
        row,
        out,
        WIDTH.saturating_sub(12 + localized_width(right, 1, out.language)),
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

fn ham_status(row: u16, out: &mut Row<'_>, model: &Model<'_>, _live_metadata: bool) {
    if !(104..115).contains(&row) {
        return;
    }
    if model.view.mode != Mode::Channel {
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
            let _ = value.write_str("\x1f131");
        }
        Some(Tone::Ctcss { tenths_hz }) => {
            let _ = write!(value, "CTCSS {}.{} \x1f065", tenths_hz / 10, tenths_hz % 10);
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
            let _ = write!(value, "\x1f130 {:04X}", raw);
        }
    }
    value
}

fn offset_text(rx: u32, tx: u32) -> Text {
    let mut value = Text::new();
    if !valid_frequency(rx) || !valid_frequency(tx) {
        let _ = value.write_str("\x1f10F");
    } else if rx == tx {
        let _ = value.write_str("\x1f116");
    } else {
        let delta = rx.abs_diff(tx);
        let _ = write!(
            value,
            "\x1f10E {}{}.{:05} \x1f09A",
            if tx > rx { '+' } else { '-' },
            delta / 100_000,
            delta % 100_000
        );
    }
    value
}

fn icon(row: u16, out: &mut Row<'_>, x: usize, y: u16, width: usize, rows: &[u16], color: u16) {
    let Some(bits) = row.checked_sub(y).and_then(|dy| rows.get(usize::from(dy))) else {
        return;
    };
    for column in 0..width {
        if bits & (1 << (width - 1 - column)) != 0 && x + column < WIDTH {
            out[x + column] = color;
        }
    }
}

fn item_background(row: u16, out: &mut Row<'_>, y: u16, selected: bool) {
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
        let _ = value.write_str("\x1f040");
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

#[inline(never)]
fn home_digits(row: u16, out: &mut Row<'_>, top: u16, value: &str, ink: u16, base: u16) {
    // Chakra Petch SemiBold, SIL OFL 1.1. Four pixels per byte.
    const INK_WIDTH: usize = frequency_font::INK_BYTES * 4;
    let blend = |opacity: u16| {
        let component = |shift: u32, mask: u16| {
            (((ink >> shift) & mask) * opacity + ((base >> shift) & mask) * (3 - opacity) + 1) / 3
                << shift
        };
        component(11, 31) | component(5, 63) | component(0, 31)
    };
    let palette = [base, blend(1), blend(2), ink];
    let glyph_row = usize::from(row - top);
    let width: usize = value.bytes().map(|b| if b == b'.' { 12 } else { 27 }).sum();
    let mut x = (WIDTH - width) / 2;
    for byte in value.bytes() {
        let (glyph, left, count, advance) = match byte {
            b'0'..=b'9' => (usize::from(byte - b'0'), 0, INK_WIDTH, 27),
            b'.' => (10, 7, 12, 12),
            b'-' => (11, 0, INK_WIDTH, 27),
            _ => (12, 0, INK_WIDTH, 27),
        };
        let bitmap_row = frequency_font::row(glyph, glyph_row);
        for column in 0..count {
            let bit = column + left;
            let alpha = bitmap_row[bit / 4] >> ((bit % 4) * 2) & 3;
            if alpha != 0 {
                out[x + column] = palette[usize::from(alpha)];
            }
        }
        x += advance;
    }
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
    } else if kind != EntryKind::CbChannel || digits < kind.maximum_digits() {
        let _ = result.write_char('_');
    }
    result
}

fn text_width(value: &str, scale: u16) -> usize {
    localized_width(value, scale, Language::Russian)
}
fn localized_width(value: &str, scale: u16, language: Language) -> usize {
    if value.contains('\x1f') {
        raw_width(locale::resolve(value, language).as_str(), scale)
    } else {
        raw_width(value, scale)
    }
}
fn fitting_scale(value: &str, requested: u16, available: usize) -> u16 {
    let mut scale = requested;
    while scale > 1 && raw_width(value, scale) > available {
        scale -= 1;
    }
    scale
}
fn raw_width(value: &str, scale: u16) -> usize {
    value
        .chars()
        .count()
        .saturating_mul(6 * usize::from(scale))
        .saturating_sub(usize::from(scale))
}

#[inline(never)]
fn centered(row: u16, out: &mut Row<'_>, y: u16, scale: u16, value: &str, color: u16) {
    centered_within(row, out, y, scale, value, color, WIDTH);
}

#[inline(never)]
fn centered_within(
    row: u16,
    out: &mut Row<'_>,
    y: u16,
    scale: u16,
    value: &str,
    color: u16,
    width: usize,
) {
    if row < y || row >= y + 7 * scale {
        return;
    }
    let translated;
    let value = if value.contains('\x1f') {
        translated = locale::resolve(value, out.language);
        translated.as_str()
    } else {
        value
    };
    let scale = fitting_scale(value, scale, width);
    raw_text(
        row,
        out,
        width.saturating_sub(raw_width(value, scale)) / 2,
        y,
        scale,
        value,
        color,
    );
}

#[inline(never)]
fn rect(row: u16, out: &mut Row<'_>, x: usize, y: u16, width: usize, height: u16, color: u16) {
    if row >= y && row < y.saturating_add(height) {
        out[x.min(WIDTH)..x.saturating_add(width).min(WIDTH)].fill(color);
    }
}

#[inline(never)]
fn text(row: u16, out: &mut Row<'_>, x: usize, y: u16, scale: u16, value: &str, color: u16) {
    if row < y || row >= y + 7 * scale {
        return;
    }
    let translated;
    let value = if value.contains('\x1f') {
        translated = locale::resolve(value, out.language);
        translated.as_str()
    } else {
        value
    };
    let scale = fitting_scale(value, scale, WIDTH.saturating_sub(x));
    raw_text(row, out, x, y, scale, value, color);
}
#[inline(never)]
fn raw_text(row: u16, out: &mut Row<'_>, x: usize, y: u16, scale: u16, value: &str, color: u16) {
    if row < y || row >= y + 7 * scale {
        return;
    }
    let glyph_row = usize::from((row - y) / scale);
    let scale = usize::from(scale);
    for (index, ch) in value.chars().enumerate() {
        let origin = x + index * 6 * scale;
        if origin >= WIDTH {
            break;
        }
        draw_glyph_row(out, origin, glyph_rows(ch)[glyph_row], scale, color);
    }
}

// Keep one copy of the glyph dispatch for cached and uncached text.
#[inline(never)]
fn glyph_rows(ch: char) -> [u8; 7] {
    font::glyph(ch)
}

fn draw_glyph_row(out: &mut Row<'_>, origin: usize, bits: u8, scale: usize, color: u16) {
    for column in 0..5 {
        if bits & (1 << (4 - column)) == 0 {
            continue;
        }
        let start = origin + column * scale;
        if start < WIDTH {
            out[start..(start + scale).min(WIDTH)].fill(color);
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::controller::Entry;
    use std::{cell::RefCell, rc::Rc, vec, vec::Vec};

    #[test]
    fn menu_cache_matches_label_pixels_and_reuses_glyphs_across_windows() {
        let mut cache = MenuCache::new();
        let mut expected_resolutions = 0;
        for language in [Language::Russian, Language::English, Language::German] {
            for group in
                core::iter::once(None).chain(menu_entries(None).iter().filter_map(|entry| {
                    if let MenuEntry::Group(group) = entry {
                        Some(Some(*group))
                    } else {
                        None
                    }
                }))
            {
                for &entry in menu_entries(group) {
                    if cache.key != Some((entry, language)) {
                        expected_resolutions += 1;
                    }
                    let value = match entry {
                        MenuEntry::Item(item) => menu_label(item),
                        MenuEntry::Group(group) => menu_group_label(group),
                    };
                    let label = locale::resolve(value, language);
                    let scale = fitting_scale(label.as_str(), 2, WIDTH - MENU_LABEL_X);
                    // Start and stop in the middle of the label, as separate
                    // LCD windows do. Color can change without new glyphs.
                    for span in [0..4, 4..10, 10..14] {
                        for row in span {
                            let ink = if row < 4 {
                                color::TEXT
                            } else {
                                color::BACKGROUND
                            };
                            let mut actual = [color::ACCENT; WIDTH];
                            let mut expected = actual;
                            cache.paint(
                                entry,
                                row,
                                &mut Row {
                                    pixels: &mut actual,
                                    language,
                                },
                                0,
                                ink,
                            );
                            raw_text(
                                row,
                                &mut Row {
                                    pixels: &mut expected,
                                    language,
                                },
                                MENU_LABEL_X,
                                0,
                                scale,
                                label.as_str(),
                                ink,
                            );
                            assert_eq!(actual, expected, "{entry:?} {language:?} row {row}");
                        }
                    }
                    assert_eq!(cache.resolutions, expected_resolutions);
                }
            }
        }
    }

    fn view(screen: Screen) -> View {
        View {
            language: Language::Russian,
            cb: None,
            cb_draft: (screen == Screen::Cb).then(crate::cb::CbChannel::default),
            mdc: [crate::controller::MdcSettings { mode: 0, id: None }; 2],
            radio_name: None,
            name_roger_enabled: false,
            microphone_gain: 16,
            tx_timeout: crate::controller::TxTimeout::default(),
            radio_name_draft: None,
            mdc_option: 0,
            mdc_draft: None,
            gps_enabled: false,
            gps_baud: 1,
            gps_status: crate::controller::GpsStatus::default(),
            aprs_status: crate::controller::AprsStatus::Unconfigured,
            aprs_auto_available: false,
            aprs_auto_armed: false,
            aprs_rx_available: false,
            aprs_rx_stats: Default::default(),
            aprs_rx_packet: crate::controller::AprsPacket::EMPTY,
            aprs_rx_count: 0,
            aprs_rx_index: 0,
            aprs_callsign: [b' '; 6],
            aprs_ssid: 0,
            dtmf_enabled: true,
            shortwave_scope: false,
            shortwave_scope_points: 8,
            shortwave_filter: 1,
            shortwave_soft_mute: false,
            shortwave_high_pass: false,
            screen,
            mode: Mode::Channel,
            channel: Some(0),
            list_channel: Some(0),
            vfo_10hz: 14_550_000,
            shift_index: 0,
            tx_tone_index: 0,
            top_key: None,
            top_draft: None,
            top_action: 0,
            bottom_key: None,
            bottom_action: 0,
            step_index: 4,
            menu_index: 0,
            menu_group: None,
            spectrum_cursor: 40,
            spectrum_span_index: 3,
            spectrum_trigger: 24,
            spectrum_auto_listen: true,
            shortwave_khz: 6070,
            shortwave_fraction_hz: 0,
            shortwave_step: crate::controller::ShortwaveMode::Am.default_step_index(),
            shortwave_band_draft: None,
            shortwave_mode: crate::controller::ShortwaveMode::Am,
            shortwave_band: 0,
            shortwave_bfo: 0,
            shortwave_scan: false,
            shortwave_entry: None,
            shortwave_status: crate::controller::ShortwaveStatus::Starting,
            crossband_probe: CrossbandProbe::Starting,
            dual_watch: false,
            watch_rx: None,
            spectrum_step_index: 4,
            spectrum_menu: false,
            spectrum_option: 0,
            spectrum_listening: false,
            spectrum_pending_listen: None,
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
            scan_priority_enabled: false,
            rx_tone_search: None,
            dtmf_memory_page: (screen == Screen::DtmfMemory)
                .then(crate::controller::DtmfMemoryPage::default),
            display_light: LightMode::On,
            key_light: LightMode::On,
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
            manual_tx: None,
            battery_mv: None,
            zone_name: None,
            zones: &[],
            peer: None,
            spectrum: None,
            shortwave_scope: None,
            spectrum_error: SpectrumError::None,
            lower_side_mapped: false,
        }
    }

    #[test]
    fn tone_search_refreshes_the_footer_when_a_result_becomes_actionable() {
        use crate::controller::{RxToneSearch, RxToneSource};
        assert_eq!(
            locale::resolve(WAITING_FOR_SUBTONE, Language::Russian).as_str(),
            "ОЖИДАНИЕ СУБТОНА"
        );
        assert_eq!(
            locale::resolve(SUBTONE_DETECTED, Language::Russian).as_str(),
            "СУБТОН ОБНАРУЖЕН"
        );
        let mut model = model(Screen::RxToneSearch);
        model.view.rx_tone_search = Some(RxToneSearch {
            source: RxToneSource::Channel {
                slot: Slot::A,
                channel: 0,
            },
            result_raw: None,
        });
        let previous = model.view;
        model.view.rx_tone_search.as_mut().unwrap().result_raw = Some(0x1302);
        let dirty = dirty_ranges(&model, Some(previous)).unwrap();
        for row in 305..312 {
            assert!(dirty
                .ranges()
                .iter()
                .any(|&(start, end)| (start..end).contains(&row)));
        }
    }

    #[test]
    fn dtmf_memory_page_renders_localized_slot_and_incremental_edits() {
        for (language, expected) in [
            (Language::Russian, "ПАМЯТЬ DTMF"),
            (Language::English, "DTMF MEMORY"),
            (Language::German, "DTMF SPEICHER"),
        ] {
            assert_eq!(
                locale::resolve(menu_label(MenuItem::DtmfMemory), language).as_str(),
                expected
            );
            let mut before = model(Screen::DtmfMemory);
            before.view.language = language;
            let mut after = before;
            after.view.dtmf_memory_page = Some(crate::controller::DtmfMemoryPage {
                slot: 1,
                bytes: *b"12*#00000000",
                len: 4,
                active: true,
                editing: true,
            });
            compare_incremental(before, after);
        }
    }

    #[test]
    fn aprs_auto_status_renders_incrementally_in_each_language() {
        for language in [Language::Russian, Language::English, Language::German] {
            let mut off = model(Screen::Aprs);
            off.view.language = language;
            off.view.aprs_status = crate::controller::AprsStatus::Ready;
            off.view.aprs_auto_available = true;
            let mut on = off;
            on.view.aprs_auto_armed = true;
            let mut waiting = on;
            waiting.view.aprs_auto_available = false;
            let mut unavailable = waiting;
            unavailable.view.aprs_auto_armed = false;
            for label in ["* \x1f188", "\x1f189", "\x1f18A", "\x1f18B"] {
                assert!((1..WIDTH).contains(&localized_width(label, 1, language)));
            }
            compare_incremental(off, on);
            compare_incremental(on, waiting);
            compare_incremental(waiting, unavailable);
            compare_incremental(unavailable, off);
        }
    }

    #[test]
    fn tx_timeout_setting_labels_and_draft_render_in_each_locale() {
        for (language, title, unit) in [
            (Language::Russian, "ТАЙМЕР ПЕРЕДАЧИ", "С"),
            (Language::English, "TX TIMEOUT", "S"),
            (Language::German, "SENDEZEITLIMIT", "S"),
        ] {
            assert_eq!(
                locale::resolve(setting_title(Setting::TxTimeout), language).as_str(),
                title
            );
            assert_eq!(locale::resolve("\x1f10A", language).as_str(), unit);
            let mut before = model(Screen::Setting);
            before.view.language = language;
            before.view.setting = Some(Setting::TxTimeout);
            before.view.setting_index = 2;
            let mut after = before;
            after.view.setting_index = 0;
            assert_eq!(
                current_setting_label(&after, Setting::TxTimeout),
                setting_options(Setting::TxTimeout)[2]
            );
            compare_incremental(before, after);
        }
    }

    #[test]
    fn ctcss_and_last_gps_baud_pages_render_real_choices() {
        for (setting, index) in [(Setting::TxTone, 50), (Setting::GpsBaud, 9)] {
            let mut m = model(Screen::Setting);
            m.view.setting = Some(setting);
            m.view.setting_index = index;
            for row in 0..HEIGHT {
                render_row(row, &mut [0; WIDTH], &m).unwrap();
            }
            let mut changed = m;
            changed.view.setting_index = index - 3;
            compare_incremental(m, changed);
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
        let reference = model(Screen::Menu);
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
                if y == radio_banner_rows(screen).0 + 7 {
                    if screen == Screen::Home {
                        assert_eq!(a[0], color::WARNING_BACKGROUND);
                        assert_eq!(a[172], color::PANEL);
                        assert!(a[..170].contains(&color::WARNING));
                    } else {
                        assert_eq!(a, banner);
                    }
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
        assert_eq!(
            locale::resolve(channel_name(&[0xff]).as_str(), Language::Russian).as_str(),
            "БЕЗ ИМЕНИ"
        );
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
        assert_eq!(
            locale::resolve(tone_text(0x1302).as_str(), Language::Russian).as_str(),
            "CTCSS 77.0 ГЦ"
        );
        assert_eq!(tone_text(0x2013).as_str(), "DCS D023N");
        assert_eq!(tone_text(0x3013).as_str(), "DCS D023I");
        assert_eq!(
            locale::resolve(tone_text(0).as_str(), Language::Russian).as_str(),
            "ТОН ВЫКЛЮЧЕН"
        );
        assert_eq!(
            locale::resolve(tone_text(0xffff).as_str(), Language::Russian).as_str(),
            "ТОН ? / FFFF"
        );
        assert_eq!(
            locale::resolve(
                offset_text(raw.rx_10hz, raw.tx_10hz).as_str(),
                Language::Russian
            )
            .as_str(),
            "СДВИГ TX -1.60000 МГЦ"
        );
        assert_eq!(
            locale::resolve(
                offset_text(raw.tx_10hz, raw.rx_10hz).as_str(),
                Language::Russian
            )
            .as_str(),
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

    const ALL_SCREENS: [Screen; 17] = [
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
        Screen::Shortwave,
    ];

    const ALL_GROUPS: [Option<MenuGroup>; 6] = [
        None,
        Some(MenuGroup::Mode),
        Some(MenuGroup::Receive),
        Some(MenuGroup::Scanning),
        Some(MenuGroup::Keys),
        Some(MenuGroup::System),
    ];

    const ALL_SETTINGS: [Setting; 10] = [
        Setting::Mode,
        Setting::Power,
        Setting::Monitor,
        Setting::Modulation,
        Setting::Reverse,
        Setting::ActiveSlot,
        Setting::Scan,
        Setting::ScanPriority,
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
                        render_validated_row(y, &mut row, model, &mut MenuCache::new());
                        row
                    })
                    .collect()
            })
            .collect();
        for (a, old) in models.iter().enumerate() {
            for (b, new) in models.iter().enumerate() {
                let damage = plan_damage(new, Some(&old.view), old.radio, old.spectrum).unwrap();
                let plans = [damage.controller, damage.radio, damage.spectrum];
                let edges = damage.radio.home_outline_edges();
                for y in 0..HEIGHT {
                    let old_row = &frames[a][usize::from(y)];
                    let new_row = &frames[b][usize::from(y)];
                    if old_row != new_row {
                        let row_dirty = plans.iter().any(|plan| {
                            plan.ranges()
                                .iter()
                                .any(|&(start, end)| (start..end).contains(&y))
                        });
                        let edges_cover = || {
                            old_row
                                .iter()
                                .zip(new_row)
                                .enumerate()
                                .all(|(x, (old, new))| {
                                    old == new
                                        || (0..8).any(|index| {
                                            let edge = HomeOutlineEdge(index);
                                            let (left, top, width, height) = edge.rect();
                                            edges & edge.bit() != 0
                                                && (left..left + width).contains(&(x as u16))
                                                && (top..top + height).contains(&y)
                                        })
                                })
                        };
                        assert!(
                            row_dirty || edges_cover(),
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
        assert_eq!(compare_incremental(home, menu), WIDTH * usize::from(HEIGHT));
        assert_eq!(compare_incremental(menu, home), WIDTH * usize::from(HEIGHT));
        assert_eq!(dirty_ranges(&home, None).unwrap().ranges(), &[(0, HEIGHT)]);
        let mut group = menu;
        group.view.menu_group = Some(MenuGroup::Scanning);
        assert_eq!(compare_incremental(menu, group), WIDTH * 238);
        assert_eq!(compare_incremental(group, menu), WIDTH * 238);
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
                Notice::ScanPrioritySet,
                Notice::ScanPriorityCleared,
                Notice::ScanPriorityUnavailable,
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
        assert_eq!(
            compare_incremental(old, new),
            WIDTH * (7 + 14 + 48 + 9 + 14)
        );
    }

    #[test]
    fn cb_grid_incremental_matches_full_frame_in_each_language() {
        for language in [Language::Russian, Language::English, Language::German] {
            let mut old = model(Screen::Cb);
            old.view.language = language;
            let cb = old.view.cb_draft.unwrap();
            let mut new = old;
            new.view.cb_draft = Some(cb.adjacent(true)); // Wrap to the next row.
            assert_eq!(compare_incremental(old, new), WIDTH * (16 + 18 * 2));
            old = new;
            new.view.cb_draft.as_mut().unwrap().grid = CbGrid::Zeros;
            new.view.cb_draft.as_mut().unwrap().am = false;
            assert_eq!(compare_incremental(old, new), WIDTH * (16 + 7));
            old = new;
            new.view.entry = Some(Entry {
                kind: EntryKind::CbChannel,
                value: 9,
                digits: 1,
            });
            compare_incremental(old, new);
            new.view.entry.as_mut().unwrap().value = 99;
            new.view.entry.as_mut().unwrap().digits = 2;
            new.view.notice = Notice::InvalidCbChannel;
            compare_incremental(old, new);
            compare_incremental(new, old);
            let mut committed = model(Screen::Home);
            committed.view.language = language;
            committed.view.mode = Mode::Vfo;
            committed.view.cb = old.view.cb_draft;
            committed.view.vfo_10hz = committed.view.cb.unwrap().frequency_10hz();
            compare_incremental(new, committed);
            let mut stepped = committed;
            stepped.view.cb = stepped.view.cb.map(|cb| cb.adjacent(false));
            stepped.view.vfo_10hz = stepped.view.cb.unwrap().frequency_10hz();
            assert_eq!(compare_incremental(committed, stepped), WIDTH * (7 + 48));
        }
    }

    #[test]
    fn mdc_end_conflict_hint_tracks_draft_in_each_locale() {
        use uv98_signaling::radio_name::RadioName;

        for language in [Language::Russian, Language::English, Language::German] {
            let mut begin = model(Screen::Mdc);
            begin.view.language = language;
            begin.view.radio_name = RadioName::from_payload(*b"QRST UVWXYZ 0123");
            begin.view.name_roger_enabled = true;
            begin.view.mdc_draft = Some(MdcDraft::Timing(1));
            let mut end = begin;
            end.view.mdc_draft = Some(MdcDraft::Timing(2));
            assert!(!mdc_end_replaced_by_name(&begin.view));
            assert!(mdc_end_replaced_by_name(&end.view));
            assert_eq!(compare_incremental(begin, end), WIDTH * (44 + 7));
            assert_eq!(compare_incremental(end, begin), WIDTH * (44 + 7));
        }
    }

    #[test]
    fn mdc_and_name_edits_match_complete_frames() {
        use crate::text_entry::{Editor, InputMode, TextValue};
        for language in [Language::Russian, Language::English, Language::German] {
            let mut old = model(Screen::Mdc);
            old.view.language = language;
            old.view.active_slot = Slot::B;
            assert_eq!(page_title(old.view), "MDC1200 B");
            let mut new = old;
            new.view.mdc_option = 1;
            assert_eq!(compare_incremental(old, new), WIDTH * 74);
            old = new;
            new.view.mdc_draft = Some(MdcDraft::UnitId(Editor::new(
                InputMode::Hex,
                TextValue::empty(),
            )));
            compare_incremental(old, new);

            old = model(Screen::RadioName);
            old.view.language = language;
            let mut editor = Editor::new(InputMode::RadioName, TextValue::empty());
            old.view.radio_name_draft = Some(editor);
            new = old;
            editor.digit(2, false);
            editor.digit(3, false);
            new.view.radio_name_draft = Some(editor);
            assert_eq!(compare_incremental(old, new), WIDTH * 14);
        }
    }

    #[test]
    fn home_notices_render_on_clean_background() {
        let normal = model(Screen::Home);
        for notice in [
            Notice::Locked,
            Notice::NoChannels,
            Notice::TxDisabled,
            Notice::InvalidChannel,
            Notice::ScanPrioritySet,
            Notice::ScanPriorityCleared,
            Notice::ScanPriorityUnavailable,
        ] {
            let mut shown = normal;
            shown.view.notice = notice;
            shown.view.locked = notice == Notice::Locked;
            for y in home_slot_rows(Slot::B).0..home_slot_rows(Slot::B).1 {
                let mut before = [0; WIDTH];
                let mut after = [0; WIDTH];
                render_row(y, &mut before, &normal).unwrap();
                render_row(y, &mut after, &shown).unwrap();
                assert_eq!(before, after, "notice covered VFO B at row {y}");
            }
            let rows = 27;
            assert_eq!(compare_incremental(normal, shown), WIDTH * rows);
            assert_eq!(compare_incremental(shown, normal), WIDTH * rows);
        }
    }

    #[test]
    fn combined_damage_validates_before_planning_the_initial_frame() {
        let mut home = model(Screen::Home);
        let initial = plan_damage(&home, None, home.radio, None).unwrap();
        assert_eq!(initial.controller.ranges(), &[(0, HEIGHT)]);
        assert!(initial.radio.ranges().is_empty());
        assert!(initial.spectrum.ranges().is_empty());
        assert_eq!(initial.radio.home_outline_edges(), 0);

        home.view.spectrum_cursor = u8::MAX;
        assert_eq!(
            plan_damage(&home, None, home.radio, None),
            Err(Error::InvalidView)
        );
        assert_eq!(
            plan_damage(&home, Some(&home.view), home.radio, None),
            Err(Error::InvalidView)
        );
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
    fn scan_skip_hint_changes_only_the_receiving_home_header() {
        for language in [Language::Russian, Language::English, Language::German] {
            assert!(localized_width("\x1f175", 1, language) <= 170);
            for dual_watch in [false, true] {
                let mut running = model(Screen::Home);
                running.view.language = language;
                running.view.dual_watch = dual_watch;
                running.radio.state = RadioState::Receiving;
                running.view.scan = ScanState::Running;
                let mut paused = running;
                paused.view.scan = ScanState::Paused;
                assert_eq!(compare_incremental(running, paused), WIDTH * 27);
                assert_eq!(compare_incremental(paused, running), WIDTH * 27);
            }
        }
    }

    #[test]
    fn receive_frame_follows_confirmed_slot_and_restores_focus_frame() {
        for focus in [Slot::A, Slot::B] {
            let mut model = model(Screen::Home);
            model.view.active_slot = focus;
            model.view.dual_watch = true;
            model.radio.state = RadioState::Receiving;
            for rx in [Some(focus.other()), Some(focus), None] {
                let before = model;
                model.radio.rx_slot = rx;
                model.radio.squelch_open = rx.is_some();
                for row in 0..HOME_HEADER_END {
                    let mut old = [0; WIDTH];
                    let mut new = [0; WIDTH];
                    render_row(row, &mut old, &before).unwrap();
                    render_row(row, &mut new, &model).unwrap();
                    assert_eq!(old, new, "RX must never change the header");
                }
                for slot in [Slot::A, Slot::B] {
                    let (top, end) = home_slot_rows(slot);
                    let expected = if rx == Some(slot) {
                        color::RX_OUTLINE
                    } else {
                        home_frame_ink(focus, slot)
                    };
                    for row in [top, top + 1, end - 1] {
                        let mut pixels = [0; WIDTH];
                        render_row(row, &mut pixels, &model).unwrap();
                        assert_eq!(pixels[4], expected, "{slot:?} {rx:?} row {row}");
                        assert_eq!(pixels[235], expected, "{slot:?} {rx:?} row {row}");
                    }
                }
                assert_eq!(model.view.active_slot, focus);
                assert!(compare_radio_incremental(before, model) < WIDTH * 70);
            }
        }
        for screen in [Screen::Menu, Screen::Setting] {
            let mut idle = model(screen);
            idle.view.setting = Some(Setting::Monitor);
            idle.radio.state = RadioState::Receiving;
            let mut receiving = idle;
            receiving.radio.rx_slot = Some(Slot::B);
            receiving.radio.squelch_open = true;
            assert_eq!(compare_radio_incremental(idle, receiving), 0);
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
            tx_frequency_10hz: Some(43_020_000),
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
        let mut edges = ranges.home_outline_edges();
        while let Some(edge) = HomeOutlineEdge::next(edges) {
            render_home_outline_edge(&mut display, edge, updated.view.active_slot, updated.radio)
                .unwrap();
            edges &= !edge.bit();
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
    fn dtmf_overlay_survives_carrier_edges_on_both_slots() {
        for slot in [Slot::A, Slot::B] {
            let mut old = model(Screen::Home);
            old.radio.state = RadioState::Receiving;
            old.radio.squelch_open = true;
            old.radio.rx_slot = Some(slot);
            old.radio.dtmf = DtmfReception::new(slot, "1122A*#");
            let mut new = old;
            new.radio.squelch_open = false;
            new.radio.rx_slot = None;
            compare_radio_incremental(old, new);
            compare_radio_incremental(new, old);
        }
    }

    #[test]
    fn live_meter_samples_repaint_only_changed_card_rows() {
        let mut old = model(Screen::Home);
        old.radio.state = RadioState::Receiving;
        let mut new = old;
        for (value, rows) in [(Some(1), 15), (Some(u16::MAX), 15), (None, 0)] {
            new.radio.rssi = value;
            assert_eq!(compare_radio_incremental(old, new), WIDTH * rows);
            old = new;
        }
        new.radio = RadioView {
            state: RadioState::Receiving,
            rssi: None,
            meter_floor: 0,
            squelch_open: true,
            monitor: true,
            power: Some(0),
            modulation: Modulation::Am,
            vfo_am_override: false,
            route: Some(RadioRoute::Hf),
            tx_slot: None,
            rx_slot: None,
            dtmf: None,
            radio_name: None,
            tx_trial: false,
        };
        compare_radio_incremental(old, new);
        old = new;
        new.radio.power = Some(2);
        assert_eq!(compare_radio_incremental(old, new), WIDTH * 14);
        old = new;
        new.radio.state = RadioState::Transmitting;
        new.radio.tx_slot = Some(Slot::A);
        assert_eq!(
            radio_dirty_ranges(&new, old.radio).unwrap().ranges(),
            &[(0, 27), home_slot_rows(Slot::A)]
        );
        assert_eq!(compare_radio_incremental(old, new), WIDTH * (27 + 142));
        old = new;
        new.radio.power = Some(0);
        assert_eq!(compare_radio_incremental(old, new), WIDTH * 21);
        old = model(Screen::Home);
        old.radio.state = RadioState::Receiving;
        old.view.scan = ScanState::Running;
        new = old;
        new.radio.rssi = Some(789);
        assert_eq!(compare_radio_incremental(old, new), 0);
    }

    #[test]
    fn rx_and_tx_scales_use_the_same_four_color_boundaries() {
        for reading in [
            MeterReading::Receive(Some(12)),
            MeterReading::Transmit(Some(12)),
        ] {
            let mut row = [0; WIDTH];
            slot_meter(
                110,
                &mut Row {
                    pixels: &mut row,
                    language: Language::Russian,
                },
                110,
                reading,
            );
            let cells: [u16; 12] = core::array::from_fn(|index| row[46 + index * 15]);
            assert_eq!(&cells[0..2], &[0x319f; 2]);
            assert_eq!(&cells[2..6], &[0x075f; 4]);
            assert_eq!(&cells[6..8], &[0xfac6; 2]);
            assert_eq!(&cells[8..12], &[0x37e5; 4]);
        }
    }

    #[test]
    fn rx_bar_updates_do_not_repaint_ruler_and_inactive_slot_never_gets_rssi() {
        for slot in [Slot::A, Slot::B] {
            let mut old = model(Screen::Home);
            old.view.active_slot = slot;
            old.radio.state = RadioState::Receiving;
            old.radio.rssi = Some(1);
            let mut new = old;
            new.radio.rssi = Some(400);
            assert_eq!(compare_radio_incremental(old, new), WIDTH * 7);
            let peer = if slot == Slot::A { Slot::B } else { Slot::A };
            let (a, b) = home_slot_rows(peer);
            for y in a..b {
                let mut before = [0; WIDTH];
                let mut after = [0; WIDTH];
                render_row(y, &mut before, &old).unwrap();
                render_row(y, &mut after, &new).unwrap();
                assert_eq!(before, after, "live {slot:?} RSSI leaked into peer");
            }
            old = new;
            new.radio.rssi = Some(401);
            assert_eq!(compare_radio_incremental(old, new), 0);
        }
    }

    #[test]
    fn large_power_follows_actual_b_tx_while_a_has_an_entry() {
        let mut old = model(Screen::Home);
        old.view.active_slot = Slot::A;
        old.view.entry = Some(crate::controller::Entry {
            kind: EntryKind::Frequency,
            value: 14550000,
            digits: 8,
        });
        old.radio.state = RadioState::Transmitting;
        old.radio.tx_slot = Some(Slot::B);
        old.radio.power = Some(0);
        for (power, ink) in [(0, 0x001f), (1, 0x07ff), (2, 0x07e0)] {
            let mut new = old;
            new.radio.power = Some(power);
            compare_radio_incremental(old, new);
            let top = home_slot_rows(Slot::B).0 + HOME_META_Y - 3;
            let mut painted_rows = 0;
            for y in top..top + 14 {
                let mut pixels = [0; WIDTH];
                render_row(y, &mut pixels, &new).unwrap();
                if pixels[210..222].contains(&ink) {
                    painted_rows += 1;
                }
            }
            assert!(painted_rows > 7, "power glyph must use the large font");
            old = new;
        }
    }

    #[test]
    fn tx_meter_ignores_rx_samples_and_ptt_intent_without_tx_ack() {
        let mut m = model(Screen::Home);
        m.radio.state = RadioState::Receiving;
        m.radio.rssi = Some(400);
        m.radio.power = Some(0);
        m.ptt_pressed = true;
        assert_eq!(
            MeterReading::from_radio(m.radio),
            MeterReading::Receive(Some(12))
        );
        let mut tx = m;
        tx.radio.state = RadioState::Transmitting;
        tx.radio.tx_slot = Some(m.view.active_slot);
        assert_eq!(
            MeterReading::from_radio(tx.radio),
            MeterReading::Transmit(Some(3))
        );
        compare_radio_incremental(m, tx);
        let mut changed = tx;
        changed.radio.rssi = Some(0);
        assert_eq!(compare_radio_incremental(tx, changed), 0);
        changed.radio.power = None;
        compare_radio_incremental(tx, changed);
    }

    #[test]
    fn only_confirmed_slot_gets_ptt_badge_frame_meter_and_tx_frequency() {
        for focus in [Slot::A, Slot::B] {
            let tx_slot = focus.other();
            let mut receiving = model(Screen::Home);
            receiving.view.active_slot = focus;
            receiving.radio.rx_slot = Some(focus);
            receiving.channel.as_mut().unwrap().tx_10hz = 43_020_000;
            receiving.peer = Some(PeerView {
                slot: tx_slot,
                mode: Mode::Vfo,
                frequency_10hz: Some(14_550_000),
                tx_frequency_10hz: Some(14_610_000),
                channel: None,
                name: &[],
                zone: None,
                zone_name: None,
                reverse: false,
            });
            receiving.radio.state = RadioState::Receiving;
            receiving.radio.squelch_open = true;
            receiving.radio.tx_slot = Some(tx_slot); // Ignored while receiving.
            receiving.view.ptt_down = true;
            let mut transmitting = receiving;
            transmitting.radio.state = RadioState::Transmitting;
            transmitting.radio.power = Some(2);
            transmitting.view.ptt_down = false; // Explicit tail after release.
            assert_eq!(home_frequency(&transmitting), Some(43_180_000));
            assert_eq!(
                slot_content(&transmitting, tx_slot).frequency_10hz,
                Some(14_610_000)
            );
            assert_eq!(
                home_meter(&transmitting.view, transmitting.radio, focus),
                MeterReading::Inactive
            );
            assert_eq!(
                home_meter(&transmitting.view, transmitting.radio, tx_slot),
                MeterReading::Transmit(Some(10))
            );
            let mut row = [0; WIDTH];
            let (start, _) = home_ptt_badge_rows(tx_slot);
            render_row(start, &mut row, &receiving).unwrap();
            assert_ne!(row[176], color::TX);
            render_row(start, &mut row, &transmitting).unwrap();
            assert_eq!(row[176], color::TX);
            assert_eq!(&row[4..7], &[color::TX_OUTLINE; 3]);
            assert_eq!(&row[233..236], &[color::TX_OUTLINE; 3]);
            let (top, end) = home_slot_rows(tx_slot);
            for y in [top, top + 2, end - 3, end - 1] {
                render_row(y, &mut row, &transmitting).unwrap();
                assert_eq!(&row[4..236], &[0x001f; 232]);
            }
            render_row(home_slot_rows(focus).0, &mut row, &transmitting).unwrap();
            assert_eq!(row[4], color::ACCENT);
            render_row(6, &mut row, &transmitting).unwrap();
            assert!(row[..170].iter().all(|&pixel| pixel == color::PANEL));
            compare_radio_incremental(receiving, transmitting);
            compare_radio_incremental(transmitting, receiving);
        }
    }

    #[test]
    fn cb_am_display_uses_nominal_rx_and_acknowledged_offset_fm_tx() {
        let cb = crate::cb::CbChannel::default();
        const B_RX: u32 = 43_350_000;
        for focus in [Slot::A, Slot::B] {
            let mut receiving = model(Screen::Home);
            receiving.view.cb = Some(cb);
            receiving.view.active_slot = focus;
            receiving.view.mode = Mode::Vfo;
            receiving.view.vfo_10hz = if focus == Slot::A {
                cb.frequency_10hz()
            } else {
                B_RX
            };
            receiving.view.shift_index = 0;
            let peer = focus.other();
            let (peer_rx, peer_tx) = if peer == Slot::A {
                (cb.frequency_10hz(), cb.tx_frequency_10hz())
            } else {
                (B_RX, B_RX)
            };
            receiving.peer = Some(PeerView {
                slot: peer,
                mode: Mode::Vfo,
                frequency_10hz: Some(peer_rx),
                tx_frequency_10hz: Some(peer_tx),
                channel: None,
                name: &[],
                zone: None,
                zone_name: None,
                reverse: false,
            });
            receiving.radio.state = RadioState::Receiving;
            receiving.radio.modulation = Modulation::Am;
            receiving.radio.tx_slot = Some(Slot::A);
            receiving.view.ptt_down = true;
            if focus == Slot::A {
                let clean = receiving;
                receiving.view.reverse = true;
                receiving.view.shift_index = 1;
                receiving.view.tx_tone_index = 1;
                let y = home_slot_rows(Slot::A).0 + 104;
                let mut clean_row = [0; WIDTH];
                let mut stale_row = [0; WIDTH];
                render_row(y, &mut clean_row, &clean).unwrap();
                render_row(y, &mut stale_row, &receiving).unwrap();
                assert_eq!(clean_row, stale_row);
            }
            let nominal = slot_content(&receiving, Slot::A).frequency_10hz.unwrap();
            assert_eq!(frequency(nominal).as_str(), "027.13500");

            let mut transmitting = receiving;
            transmitting.radio.state = RadioState::Transmitting;
            let carrier = slot_content(&transmitting, Slot::A).frequency_10hz.unwrap();
            assert_eq!(frequency(carrier).as_str(), "027.13700");
            assert_eq!(
                slot_content(&transmitting, Slot::B).frequency_10hz,
                Some(B_RX)
            );
            let badge_top = home_slot_rows(Slot::A).0 + HOME_META_Y + 2;
            let mut saw_fm = false;
            for y in badge_top..badge_top + 7 {
                let mut row = [0; WIDTH];
                render_row(y, &mut row, &transmitting).unwrap();
                saw_fm |= row[108..120].contains(&color::FM_BADGE);
                assert!(!row[108..120].contains(&color::AM_BADGE));
            }
            assert!(saw_fm);
            compare_radio_incremental(receiving, transmitting);
            compare_radio_incremental(transmitting, receiving);
        }
    }

    #[test]
    fn battery_bands_and_rounding_use_pack_millivolts() {
        assert_eq!(battery_display(None), None);
        for (mv, decivolts, bars) in [
            (6550, 66, 0),
            (6600, 66, 1),
            (7199, 72, 1),
            (7200, 72, 2),
            (7600, 76, 3),
            (8000, 80, 4),
            (8250, 83, 4),
            (u16::MAX, 655, 4),
        ] {
            assert_eq!(battery_display(Some(mv)), Some((decivolts, bars)));
        }
    }

    #[test]
    fn name_popup_updates_only_its_rows_and_yields_to_tx() {
        use uv98_signaling::radio_name::RadioName;

        let mut dtmf = model(Screen::Home);
        dtmf.radio.state = RadioState::Receiving;
        dtmf.radio.dtmf = DtmfReception::new(Slot::A, "1234");
        let name = RadioName::from_payload(*b"QRST UVWXYZ 0123").unwrap();
        let mut shown = dtmf;
        shown.radio.radio_name = Some(NameReception::new(Slot::B, name));
        assert_eq!(compare_radio_incremental(dtmf, shown), WIDTH * 58);

        let mut changed_slot = shown;
        changed_slot.radio.radio_name = Some(NameReception::new(Slot::A, name));
        assert_eq!(compare_radio_incremental(shown, changed_slot), WIDTH * 58);
        assert_eq!(compare_radio_incremental(changed_slot, dtmf), WIDTH * 58);

        let mut tx = shown;
        tx.radio.state = RadioState::Transmitting;
        tx.radio.tx_slot = Some(Slot::A);
        assert!(tx.radio.signaling_popup().is_none());
        compare_radio_incremental(shown, tx);
    }

    #[test]
    fn dtmf_popup_changes_and_tx_removal_match_a_complete_frame() {
        for screen in [Screen::Home, Screen::Menu, Screen::Keys] {
            let mut before = model(screen);
            before.radio.state = RadioState::Receiving;
            let mut shown = before;
            shown.radio.dtmf = DtmfReception::new(Slot::B, "0011ABCD*#");
            assert_eq!(compare_radio_incremental(before, shown), WIDTH * 58);
            let mut changed = shown;
            changed.radio.dtmf = DtmfReception::new(Slot::B, "0011ABCD*#22");
            assert_eq!(compare_radio_incremental(shown, changed), WIDTH * 58);
            assert_eq!(compare_radio_incremental(changed, before), WIDTH * 58);
            let mut tx = shown;
            tx.radio.state = RadioState::Transmitting;
            tx.radio.tx_slot = Some(Slot::A);
            assert!(tx.radio.visible_dtmf().is_none());
            compare_radio_incremental(shown, tx);
            compare_radio_incremental(tx, shown);
        }
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
                        meter_floor: 0,
                        squelch_open: true,
                        monitor: true,
                        power: Some(2),
                        modulation: Modulation::Am,
                        vfo_am_override: false,
                        route: Some(RadioRoute::Hf),
                        tx_slot: None,
                        rx_slot: None,
                        dtmf: None,
                        radio_name: None,
                        tx_trial: false,
                    };
                    let pixels = compare_radio_incremental(old, new);
                    assert!(pixels <= WIDTH * 128, "{screen:?}: {before:?} -> {after:?}");
                    if screen == Screen::Home {
                        old.view.entry = Some(Entry {
                            kind: EntryKind::Frequency,
                            value: 145500,
                            digits: 6,
                        });
                        new.view.entry = old.view.entry;
                        assert!(compare_radio_incremental(old, new) <= WIDTH * 27);
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
        assert_eq!(
            compare_incremental(old, new),
            WIDTH * usize::from(HOME_DIGIT_HEIGHT)
        );
        new = old;
        new.view.step_index = 0;
        assert_eq!(compare_incremental(old, new), WIDTH * 7);
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
                assert_eq!(compare_incremental(old, new), WIDTH * 27);
                assert_eq!(compare_incremental(new, old), WIDTH * 27);
            }
            new = old;
            new.view.reverse = true;
            assert_eq!(compare_incremental(old, new), WIDTH * (48 + 9 + 14));
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
                let y = LIST_Y + (index % LIST_ROWS) as u16 * LIST_ROW_HEIGHT;
                for row in y..y + ITEM_PAINT_HEIGHT {
                    let mut selected = [0; WIDTH];
                    render_row(row, &mut selected, &old).unwrap();
                    assert_eq!(selected[12], color::ACCENT);
                    assert_eq!(selected[14], color::ACCENT);
                }
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
            tx_frequency_10hz: Some(14_612_500),
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
            assert_eq!(end - start, HOME_DIGIT_HEIGHT);
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
                painted_rows >= 44,
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
            Setting::ScanPriority,
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
    fn shift_editor_previews_the_active_b_draft_before_commit() {
        use crate::controller::Controller;
        use crate::input::InputEvent;

        let bitmap = [0; 128];
        let mut controller = Controller::new(&bitmap, 43_350_000);
        controller.select_slot(Slot::B);
        let mut press = |key| {
            controller.handle(InputEvent::Short(key), &bitmap);
            controller.view()
        };
        press(Key::Up);
        press(Key::Menu);
        press(Key::Down);
        press(Key::Down);
        press(Key::Menu);
        for _ in 0..5 {
            press(Key::Down);
        }
        let simplex = press(Key::Menu);
        let minus = press(Key::Down);
        let plus = press(Key::Down);
        for view in [simplex, minus, plus] {
            assert_eq!(view.screen, Screen::Setting);
            assert_eq!(view.setting, Some(Setting::Shift));
            assert_eq!(view.active_slot, Slot::B);
            assert_eq!(view.vfo_10hz, 43_351_250);
            assert_eq!(view.shift_index, 0, "draft must not commit");
        }
        let committed = press(Key::Menu);
        assert_eq!(committed.shift_index, 2);
        assert_eq!(
            vfo_tx_frequency_for(committed.vfo_10hz, 2, None),
            Some(43_411_250)
        );
        assert_eq!(controller.context(Slot::A).vfo_10hz, 43_350_000);
        assert_eq!(controller.context(Slot::A).shift_index, 0);

        for language in [Language::Russian, Language::English, Language::German] {
            let render = |view: View| {
                let mut m = model(Screen::Setting);
                m.view = view;
                m.view.language = language;
                m
            };
            let simplex = render(simplex);
            let minus = render(minus);
            let plus = render(plus);
            let mut absent = plus;
            absent.view.vfo_10hz = 0;
            let mut am = plus;
            am.radio.modulation = Modulation::Am;
            am.radio.vfo_am_override = true;
            let mut auto_am = plus;
            auto_am.radio.modulation = Modulation::Am;
            for (model, label, ink) in [
                (simplex, "TX 433.51250", color::TEXT),
                (minus, "TX 432.91250", color::TEXT),
                (plus, "TX 434.11250", color::TEXT),
                (absent, "TX ---.-----", color::MUTED),
                (am, "TX ---.-----", color::MUTED),
                (auto_am, "TX 434.11250", color::TEXT),
            ] {
                for y in 84..91 {
                    let mut actual = [0; WIDTH];
                    render_row(y, &mut actual, &model).unwrap();
                    let mut expected = [color::BACKGROUND; WIDTH];
                    raw_text(
                        y,
                        &mut Row {
                            pixels: &mut expected,
                            language,
                        },
                        SHIFT_TX_X,
                        84,
                        1,
                        label,
                        ink,
                    );
                    assert_eq!(actual[SHIFT_TX_X - 1..], expected[SHIFT_TX_X - 1..]);
                }
            }
            assert_eq!(compare_incremental(simplex, minus), WIDTH * 51);
            assert_eq!(compare_incremental(minus, plus), WIDTH * 51);
            assert_eq!(compare_incremental(plus, absent), WIDTH * 7);
            assert_eq!(compare_incremental(absent, plus), WIDTH * 7);
            assert_eq!(compare_radio_incremental(auto_am, am), WIDTH * 7);
        }
    }

    #[test]
    fn shift_preview_applies_existing_frequency_bounds_and_reverse() {
        let mut view = model(Screen::Setting).view;
        view.mode = Mode::Vfo;
        for (base, draft, reverse, expected) in [
            (0, 0, false, None),
            (1, 1, true, None), // Checked offset underflows.
            (1_800_000, 0, false, Some(1_800_000)),
            (1_799_999, 2, false, None), // Invalid RX even with valid TX.
            (1_860_000, 1, false, Some(1_800_000)),
            (1_859_999, 1, true, None), // Reverse would make RX invalid.
            (11_739_999, 2, false, Some(11_799_999)),
            (11_740_000, 2, false, None), // Shifted TX enters airband.
            (11_800_000, 1, false, None), // Airband RX blocks ordinary FM TX.
            (13_700_000, 0, false, Some(13_700_000)),
            (13_699_999, 0, false, None),
            (66_000_000, 0, false, Some(66_000_000)),
            (65_940_001, 2, false, None), // TX falls between RF bands.
            (83_999_999, 0, false, None),
            (84_000_000, 0, false, Some(84_000_000)),
            (99_939_999, 2, false, Some(99_999_999)),
            (99_940_000, 2, false, None),
            (43_351_251, 2, false, Some(43_411_251)), // Retain full precision.
            (43_351_251, 2, true, Some(43_351_251)),
            (43_351_251, 7, false, None),
        ] {
            view.vfo_10hz = base;
            view.setting_index = draft;
            view.reverse = reverse;
            assert_eq!(
                shift_tx_preview(&view),
                expected,
                "{base} {draft} {reverse}"
            );
        }
    }

    #[test]
    fn languages_redraw_the_complete_screen_and_keep_menu_labels_inside_the_panel() {
        for language in [Language::Russian, Language::English, Language::German] {
            for item in MENU_ITEMS {
                let label = locale::resolve(menu_label(item), language);
                assert!(
                    raw_width(label.as_str(), 2) <= WIDTH - 44 - 12,
                    "menu label too long: {language:?} {item:?}: {}",
                    label.as_str()
                );
            }
            for group in [
                MenuGroup::Mode,
                MenuGroup::Receive,
                MenuGroup::Scanning,
                MenuGroup::Keys,
                MenuGroup::System,
            ] {
                let width = localized_width(menu_group_label(group), 2, language);
                assert!(
                    width <= 170,
                    "group {group:?}, {language:?}: {width}px > 170px"
                );
            }
            for screen in [
                Screen::Home,
                Screen::Menu,
                Screen::Spectrum,
                Screen::Shortwave,
                Screen::Gps,
                Screen::Aprs,
                Screen::AprsRx,
                Screen::About,
                Screen::Help,
            ] {
                let old = model(screen);
                let mut new = old;
                new.view.language = language;
                if language != old.view.language {
                    assert_eq!(compare_incremental(old, new), WIDTH * usize::from(HEIGHT));
                }
                for y in 0..HEIGHT {
                    render_row(y, &mut [0; WIDTH], &new).unwrap();
                }
            }
        }
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
            for y in 1..17 {
                let mut row = [0; WIDTH];
                render_row(y, &mut row, &m).unwrap();
                active_icon_pixels += row[148..164].iter().filter(|&&p| p == color::GOOD).count();
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
            assert_eq!(
                locale::resolve(current_setting_label(&old, setting), Language::Russian).as_str(),
                "НЕТ ПОДТВЕРЖДЕНИЯ"
            );
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
            assert_eq!(
                locale::resolve(current_setting_label(&new, setting), Language::Russian).as_str(),
                "НЕТ ПОДТВЕРЖДЕНИЯ"
            );
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
        m.radio.tx_slot = Some(m.view.active_slot);
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
                meter_floor: 0,
                squelch_open: true,
                monitor: true,
                power: Some(255),
                modulation: Modulation::Am,
                vfo_am_override: false,
                route: Some(RadioRoute::Hf),
                tx_slot: None,
                rx_slot: None,
                dtmf: None,
                radio_name: None,
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
            assert_eq!(compare_radio_incremental(normal, trial), WIDTH * 27);
            assert_eq!(compare_radio_incremental(trial, normal), WIDTH * 27);
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
            WIDTH * 15,
            "visible meter updates without repainting the build warning"
        );
    }

    fn plot_view(plot: &Plot) -> SpectrumView<'_> {
        SpectrumView {
            state: SpectrumState::Sweeping,
            window_id: 1,
            start_10hz: 43_000_000,
            center_10hz: 43_050_000,
            step_10hz: 1250,
            len: 81,
            plot,
            waterfall: None,
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
    fn listening_banner_overrides_stale_sweeping_state_including_overlay() {
        let plot = Plot::EMPTY;
        let mut m = model(Screen::Spectrum);
        m.spectrum = Some(plot_view(&plot));
        m.view.spectrum_listening = true;
        let mut row = [0; WIDTH];
        render_row(34, &mut row, &m).unwrap();
        assert!(
            row.contains(&color::GOOD),
            "listening banner must be audible"
        );
        m.view.spectrum_menu = true;
        render_row(34, &mut row, &m).unwrap();
        assert!(
            row.contains(&color::GOOD),
            "overlay must not say sound is off"
        );
        let mut audible = [0; WIDTH];
        render_row(283, &mut audible, &m).unwrap();
        m.view.spectrum_listening = false;
        let mut muted = [0; WIDTH];
        render_row(283, &mut muted, &m).unwrap();
        assert_ne!(audible, muted, "overlay footer must reflect actual audio");
        m.view.spectrum_menu = false;
        render_row(34, &mut row, &m).unwrap();
        assert!(!row.contains(&color::GOOD));
    }

    #[test]
    fn spectrum_without_snapshot_keeps_reason_and_overlay_visible() {
        let mut m = model(Screen::Spectrum);
        m.spectrum = None;
        m.spectrum_error = SpectrumError::Window;
        let mut row = [0; WIDTH];
        render_row(103, &mut row, &m).unwrap();
        assert!(
            row.contains(&color::WARNING),
            "refusal reason must be visible"
        );
        m.view.spectrum_menu = true;
        m.view.spectrum_option = crate::controller::SPECTRUM_MENU_SPAN_NARROWER;
        render_row(GRAPH_TOP + 3 * 17 + 4, &mut row, &m).unwrap();
        assert!(
            row.contains(&color::ACCENT),
            "menu must render without a snapshot"
        );
        for y in 0..HEIGHT {
            render_row(y, &mut row, &m).unwrap();
        }
    }

    #[test]
    fn spectrum_menu_option_move_repaints_only_the_two_bands() {
        let plot = Plot::EMPTY;
        let mut old = model(Screen::Spectrum);
        old.spectrum = Some(plot_view(&plot));
        old.view.spectrum_menu = true;
        old.view.spectrum_option = 0;
        let mut new = old;
        new.view.spectrum_option = 3;
        assert!(new.view.spectrum_menu);
        let mut readout = [0; WIDTH];
        render_row(252, &mut readout, &new).unwrap();
        assert!(
            !readout.contains(&color::TEXT),
            "menu must hide the cursor readout area"
        );
        let count = compare_incremental(old, new);
        assert!(
            count <= WIDTH * 32,
            "menu selection moved {count} pixels, expected only two 15-row bands"
        );
        old = new;
        new.view.spectrum_menu = false;
        let count = compare_incremental(old, new);
        assert!(
            count <= WIDTH * (20 + 320 - 94),
            "closing repaints the menu area"
        );
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
        assert_eq!(compare_spectrum_incremental(old, new), 71);
        old = new;
        new.spectrum.as_mut().unwrap().start_10hz += 1250;
        new.spectrum.as_mut().unwrap().center_10hz += 1250;
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
        assert_eq!(
            locale::resolve(shortcut_label(ShortcutAction::Spectrum), Language::Russian).as_str(),
            "СПЕКТР ЧАСТОТ"
        );
        #[cfg(not(feature = "pa5-audio"))]
        assert_eq!(VERSION.rsplit_once("-r").unwrap().1, &VERSION_SHORT[1..]);
        assert!(212 + text_width(VERSION_SHORT, 1) <= WIDTH - 10);
        assert!(16 + text_width("ВЕРСИЯ ", 2) + text_width(VERSION, 2) <= WIDTH - 8);
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

    #[test]
    fn phase_transitions_footer_and_overlay_incremental_match_full_frame() {
        let plot = Plot::EMPTY;
        let mut old = model(Screen::Spectrum);
        old.spectrum = Some(plot_view(&plot));
        let mut pending = old;
        pending.view.spectrum_pending_listen = Some(40);
        compare_incremental(old, pending);
        let mut listening = pending;
        listening.view.spectrum_pending_listen = None;
        listening.view.spectrum_listening = true;
        compare_incremental(pending, listening);
        let mut restoring = listening;
        restoring.view.spectrum_listening = false;
        restoring.spectrum.as_mut().unwrap().state = SpectrumState::Restoring;
        compare_spectrum_incremental(listening, restoring);
        let mut stopped = restoring;
        stopped.spectrum.as_mut().unwrap().state = SpectrumState::Stopped;
        compare_spectrum_incremental(restoring, stopped);
        let mut menu = stopped;
        menu.view.spectrum_menu = true;
        compare_incremental(stopped, menu);
        compare_incremental(menu, stopped);
    }

    #[test]
    fn codex_listen_transition_incremental_matches_full_frame() {
        let plot = Plot::EMPTY;
        let mut old = model(Screen::Spectrum);
        old.spectrum = Some(plot_view(&plot));
        let mut new = old;
        new.view.spectrum_listening = true;
        compare_incremental(old, new);
        compare_incremental(new, old);
    }

    #[test]
    fn codex_stopped_listen_does_not_claim_audio_from_stale_snapshot() {
        let plot = Plot::EMPTY;
        let mut m = model(Screen::Spectrum);
        m.spectrum = Some(plot_view(&plot));
        m.spectrum.as_mut().unwrap().state = SpectrumState::Listening;
        m.view.spectrum_listening = false;
        let mut row = [0; WIDTH];
        render_row(34, &mut row, &m).unwrap();
        assert!(
            !row.contains(&color::GOOD),
            "stale Listening snapshot must not override current stop/restore state"
        );
    }
}
