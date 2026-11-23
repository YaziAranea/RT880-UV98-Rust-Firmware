#![no_std]
#![forbid(unsafe_code)]
//! Live UI composition. NOR READ03 only; frequencies are local UI values.

use sha2::{Digest, Sha256};
use uv98_codeplug::{
    channels::{Channel, Tone, CHANNEL_COUNT, CHANNEL_LEN},
    zones::{Zone, MEMBER_COUNT, RECORD_LEN, ZONE_COUNT},
};
pub use uv98_diag_app::{StartStage, PRESERVED_CALIBRATION_SHA256};
use uv98_diagnostic::{Diagnostic, LoadError, Snapshot, CHANNELS_ADDRESS};
use uv98_display::{Delay, Display, DisplayBus};
use uv98_handheld_ui::{
    controller::{
        self, Change, Controller, LightMode, Mode, Request, RxToneSource, Screen, Slot, View,
    },
    input::{InputEvent, InputTracker, Sample},
    render::{
        self, ChannelView, ManualTxStatus, Model, PeerView, RadioView, SpectrumError,
        SpectrumState, SpectrumView, ZoneView,
    },
};
use uv98_keys::{Key, Timing};
use uv98_keyscan_at32::{classify_matrix, MatrixState};
use uv98_nor_read::{Bus, ReadError};

pub const ZONES_ADDRESS: u32 = 0x1c000;
pub const CONFIG_ADDRESS: u32 = 0x2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSample {
    Stable {
        matrix: u16,
        ptt_pressed: bool,
    },
    /// Aux16 is a virtual second side input; the board owns ADC normalization.
    WithAux {
        matrix: u16,
        ptt_pressed: bool,
        aux: Option<Key>,
    },
    Unstable,
    MatrixTransition,
}
impl From<uv98_diag_app::InputSample> for InputSample {
    fn from(value: uv98_diag_app::InputSample) -> Self {
        match value {
            uv98_diag_app::InputSample::Stable {
                matrix,
                ptt_pressed,
            } => Self::Stable {
                matrix,
                ptt_pressed,
            },
            uv98_diag_app::InputSample::Unstable => Self::Unstable,
        }
    }
}

/// Unmodified stock CFG bytes. Validation and RF application belong to runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadioConfig {
    pub squelch: u8,
    pub mic_gain: u8,
    pub fm_volume: [u8; 2],
    pub am_volume: [u8; 2],
    pub squelch_delay: u8,
}

/// Immutable display publication, separate from the live sweep engine's plot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpectrumSnapshot {
    pub state: SpectrumState,
    /// Monotonic scope window identity; a restart creates a new id even when
    /// start/step/len happen to match the old grid.
    pub window_id: u32,
    pub start_10hz: u32,
    /// Exact bin at the middle of the published window, not an approximation.
    pub center_10hz: u32,
    pub step_10hz: u32,
    pub len: u8,
    pub plot: uv98_spectrum::compact::Plot,
}
impl SpectrumSnapshot {
    pub fn view(&self) -> SpectrumView<'_> {
        SpectrumView {
            state: self.state,
            window_id: self.window_id,
            start_10hz: self.start_10hz,
            center_10hz: self.center_10hz,
            step_10hz: self.step_10hz,
            len: self.len,
            plot: &self.plot,
            waterfall: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    New,
    Ready,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    pub view: View,
    pub calibration_sha256: Option<[u8; 32]>,
    pub calibration_matches_backup: Option<bool>,
    pub raw_sample: Option<(u16, bool)>,
    pub last_pressed: Option<Key>,
    pub presses: u32,
    pub input_ready: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error<N, L> {
    State,
    Display(uv98_display::Error<L>),
    Render(render::RenderError<L>),
    Read(LoadError<N>),
    Catalog(ReadError<N>),
    ZoneLayout(u8),
    Clock,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Update {
    /// A redraw was queued; call render_step between input observations.
    pub rendered: bool,
    pub pressed: Option<Key>,
    pub channel_changed: bool,
    pub request: Option<Request>,
    pub cancel_temporary: bool,
    pub ptt_tx_requested: bool,
    pub ptt_slot: Option<Slot>,
    pub zone_changed: bool,
    pub slot_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderProgress {
    pub rows: u16,
    pub pending: bool,
}

// Confirmed state banners and Home TX badges can preempt a page repaint.
// Ordinary work receives a batch after at most eight priority rows.
const PRIORITY_BURST_ROWS: u16 = 8;
const MENU_BURST_ROWS: u16 = 67;

#[derive(Clone, Copy, Default)]
struct PendingRows([u32; 10]);
impl PendingRows {
    fn mark(&mut self, start: u16, end: u16) {
        for row in start..end.min(render::HEIGHT) {
            self.0[row as usize / 32] |= 1 << (row % 32);
        }
    }
    fn any(&self) -> bool {
        self.0.iter().any(|v| *v != 0)
    }
    fn next(&self, cursor: u16) -> Option<u16> {
        let cursor = cursor.min(render::HEIGHT - 1);
        let word = usize::from(cursor / 32);
        let offset = cursor % 32;
        let tail = self.0[word] & (u32::MAX << offset);
        if tail != 0 {
            return Some(word as u16 * 32 + tail.trailing_zeros() as u16);
        }
        for index in word + 1..self.0.len() {
            if self.0[index] != 0 {
                return Some(index as u16 * 32 + self.0[index].trailing_zeros() as u16);
            }
        }
        for index in 0..=word {
            if self.0[index] != 0 {
                return Some(index as u16 * 32 + self.0[index].trailing_zeros() as u16);
            }
        }
        None
    }
    fn merge_take(&mut self, other: &mut Self) {
        for (target, source) in self.0.iter_mut().zip(other.0.iter_mut()) {
            *target |= *source;
            *source = 0;
        }
    }
    fn without(mut self, other: &Self) -> Self {
        for (rows, excluded) in self.0.iter_mut().zip(other.0) {
            *rows &= !excluded;
        }
        self
    }
    fn has(&self, row: u16) -> bool {
        row < render::HEIGHT && self.0[row as usize / 32] & (1 << (row % 32)) != 0
    }
    fn clear(&mut self, start: u16, end: u16) {
        for row in start..end {
            self.0[row as usize / 32] &= !(1 << (row % 32));
        }
    }
}

#[derive(Clone, Copy)]
struct Record {
    index: usize,
    raw: [u8; CHANNEL_LEN],
}
impl Record {
    const EMPTY: Self = Self {
        index: 0,
        raw: [0xff; CHANNEL_LEN],
    };
    fn view(&self) -> ChannelView<'_> {
        let channel = Channel::parse(&self.raw).expect("fixed size record");
        ChannelView {
            index: self.index,
            name: channel.name_raw(),
            rx_10hz: channel.rx_frequency_code(),
            tx_10hz: channel.tx_frequency_code(),
            active: channel.is_active(),
            rx_tone_raw: channel.rx_tone_raw(),
            tx_tone_raw: channel.tx_tone_raw(),
            bandwidth_raw: channel.bandwidth_raw(),
            power_raw: channel.power_raw(),
        }
    }
}

pub struct App<N, L, D> {
    #[cfg(feature = "pa5-audio")]
    audio_capture: render::AudioCaptureView,
    nor: N,
    display: Display<L, D>,
    diagnostic: Diagnostic,
    bitmap: [u8; 128],
    all_bitmap: [u8; 128],
    zone_bitmap: [u8; 32],
    zone_names: [[u8; 16]; ZONE_COUNT],
    zone_counts: [u16; ZONE_COUNT],
    zone_window: [Option<u8>; 7],
    zone_window_len: usize,
    peer_record: Option<Record>,
    config: Option<RadioConfig>,
    rx_tones: [Option<(RxToneSource, u16)>; 2],
    controller: Controller,
    input: InputTracker,
    expected_sha256: [u8; 32],
    status: Status,
    list: [Record; 7],
    list_len: usize,
    list_anchor: Option<u16>,
    last_rendered: Option<View>,
    pending_rows: PendingRows,
    // Telemetry coalesces behind an in-flight pass instead of re-queuing rows
    // already painted near its beginning. Payloads always use the latest model.
    deferred_radio_rows: PendingRows,
    priority_rows: PendingRows,
    pending_outlines: u8,
    priority_run: u16,
    menu_priority: [(u16, u16); 3],
    menu_run: u16,
    menu_cache: render::MenuCache,
    row_cursor: u16,
    render_page: Option<Screen>,
    radio: RadioView,
    render_radio: RadioView,
    last_radio: RadioView,
    manual_tx: Option<ManualTxStatus>,
    battery_mv: Option<u16>,
    spectrum: Option<SpectrumSnapshot>,
    waterfall: render::Waterfall,
    shortwave_scope: render::ShortwaveScope,
    shortwave_scope_selection: Option<(controller::ShortwaveMode, u16, i16, u8, bool, u8)>,
    last_spectrum: Option<SpectrumSnapshot>,
    /// Physical lower side-key input identification, independent of its
    /// editable assignment. False until the board traces a real source.
    lower_side_mapped: bool,
    spectrum_error: SpectrumError,
}
impl<N: Bus, L: DisplayBus, D: Delay> App<N, L, D> {
    #[cfg(feature = "pa5-audio")]
    pub fn set_audio_capture(&mut self, view: render::AudioCaptureView) {
        if self.audio_capture != view {
            self.audio_capture = view;
            if self.status.view.screen == Screen::About {
                self.pending_rows.mark(0, render::HEIGHT);
            }
        }
    }

    pub fn new(nor: N, lcd: L, delay: D, timing: Timing, expected_sha256: [u8; 32]) -> Self {
        let bitmap = [0; 128];
        let controller = Controller::new(&bitmap, 14_550_000);
        let view = controller.view();
        Self {
            #[cfg(feature = "pa5-audio")]
            audio_capture: render::AudioCaptureView::default(),
            nor,
            display: Display::new_portrait(lcd, delay),
            diagnostic: Diagnostic::new(),
            bitmap,
            all_bitmap: bitmap,
            zone_bitmap: [0; 32],
            zone_names: [[0xff; 16]; ZONE_COUNT],
            zone_counts: [0; ZONE_COUNT],
            zone_window: [None; 7],
            zone_window_len: 0,
            peer_record: None,
            config: None,
            rx_tones: [None; 2],
            controller,
            input: InputTracker::new(timing),
            expected_sha256,
            status: Status {
                state: State::New,
                view,
                calibration_sha256: None,
                calibration_matches_backup: None,
                raw_sample: None,
                last_pressed: None,
                presses: 0,
                input_ready: false,
            },
            list: [Record::EMPTY; 7],
            list_len: 0,
            list_anchor: None,
            last_rendered: None,
            pending_rows: PendingRows::default(),
            deferred_radio_rows: PendingRows::default(),
            priority_rows: PendingRows::default(),
            pending_outlines: 0,
            priority_run: 0,
            menu_priority: [(0, 0); 3],
            menu_run: 0,
            menu_cache: render::MenuCache::new(),
            row_cursor: 0,
            render_page: None,
            radio: RadioView::OFF,
            render_radio: RadioView::OFF,
            last_radio: RadioView::OFF,
            manual_tx: None,
            battery_mv: None,
            spectrum: None,
            waterfall: render::Waterfall::new(),
            shortwave_scope: render::ShortwaveScope::new(),
            shortwave_scope_selection: None,
            last_spectrum: None,
            lower_side_mapped: false,
            spectrum_error: SpectrumError::None,
        }
    }
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn snapshot(&self) -> Option<Snapshot<'_>> {
        if self.status.state != State::Ready {
            return None;
        }
        self.diagnostic
            .snapshot()
            .filter(|snapshot| Some(snapshot.index() as u16) == self.status.view.channel)
    }
    pub fn calibration_view(&self) -> Option<uv98_calibration::Calibration<'_>> {
        if self.status.state == State::Ready {
            self.diagnostic.calibration_view()
        } else {
            None
        }
    }
    pub fn selected_channel(&self) -> Option<Channel<'_>> {
        if self.status.view.mode != Mode::Channel {
            return None;
        }
        self.snapshot().map(|snapshot| snapshot.channel)
    }

    pub fn positions(&self) -> [u32; 2] {
        self.controller.positions()
    }

    pub fn dtmf_memory(&self) -> &[u8] {
        self.controller.dtmf_memory()
    }

    #[cold]
    #[cfg_attr(target_arch = "arm", inline(never))]
    pub fn restore_positions(
        &mut self,
        mut words: [u32; 2],
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let active = if words[0] & controller::POSITION_ACTIVE_B != 0 {
            Slot::B
        } else {
            Slot::A
        };
        words[0] &= !controller::POSITION_ACTIVE_B;
        for slot in [active.other(), active] {
            let mut change = self
                .controller
                .restore_position(slot, words[usize::from(slot == Slot::B)]);
            if !change.slot_changed && !change.zone_changed {
                let fixed = self.controller.reconcile_channels(&self.bitmap);
                change.channel_changed |= fixed.channel_changed;
                change.redraw |= fixed.redraw;
            }
            self.apply_change(change, None)?;
        }
        Ok(())
    }

    pub fn rx_tone_override(&self, view: View) -> Option<u16> {
        let (source, raw) = self.rx_tones[usize::from(view.active_slot == Slot::B)]?;
        (RxToneSource::for_view(view) == Some(source)).then_some(raw)
    }

    #[cfg_attr(target_arch = "arm", inline(never))]
    pub fn set_rx_tone_search_result(
        &mut self,
        result: Option<u16>,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.controller.set_rx_tone_search_result(result) {
            self.redraw_ready()?;
        }
        Ok(())
    }

    #[cfg_attr(target_arch = "arm", inline(never))]
    pub fn reject_rx_tone_search(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        self.controller.reject_rx_tone_search();
        self.redraw_ready()
    }

    pub fn peer_radio_selection(&self) -> (controller::View, Option<Channel<'_>>) {
        let mut view = self.status.view;
        view.active_slot = if view.active_slot == Slot::A {
            Slot::B
        } else {
            Slot::A
        };
        let context = self.controller.context(view.active_slot);
        view.mode = context.mode;
        view.channel = context.channel;
        view.vfo_10hz = context.vfo_10hz;
        view.shift_index = context.shift_index;
        view.tx_tone_index = context.tx_tone_index;
        view.reverse = context.reverse;
        view.zone = context.zone;
        view.step_index = context.step_index;
        let record = self.peer_record.as_ref().filter(|record| {
            context.mode == Mode::Channel && context.channel == Some(record.index as u16)
        });
        (
            view,
            record.and_then(|record| Channel::parse(&record.raw).ok()),
        )
    }
    pub fn radio_config(&self) -> Option<RadioConfig> {
        if self.status.state == State::Ready {
            self.config
        } else {
            None
        }
    }
    pub fn radio_view(&self) -> RadioView {
        self.radio
    }
    pub fn battery_voltage(&self) -> Option<u16> {
        self.battery_mv
    }
    pub fn spectrum_view(&self) -> Option<SpectrumView<'_>> {
        if self.status.state == State::Ready {
            self.spectrum.as_ref().map(SpectrumSnapshot::view)
        } else {
            None
        }
    }
    /// False means an older immutable spectrum snapshot is still being drawn.
    /// The caller keeps its newest live data and retries; no partial replacement.
    pub fn set_shortwave_status(
        &mut self,
        status: controller::ShortwaveStatus,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.view.shortwave_status != status {
            self.controller.set_shortwave_status(status);
            self.status.view = self.controller.view();
            self.redraw_ready()?;
        }
        Ok(())
    }

    pub fn set_gps_status(
        &mut self,
        status: controller::GpsStatus,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.view.gps_status == status {
            return Ok(());
        }
        self.controller.set_gps_status(status);
        self.status.view = self.controller.view();
        if self.status.view.screen == Screen::Gps {
            self.redraw_ready()?;
        }
        Ok(())
    }
    pub fn apply_radio_name_settings(
        &mut self,
        name: Option<uv98_handheld_ui::controller::RadioName>,
        enabled: bool,
    ) {
        self.controller.apply_radio_name_settings(name, enabled);
        self.status.view = self.controller.view();
    }
    pub fn apply_microphone_gain(&mut self, gain: u8) {
        self.controller.apply_microphone_gain(gain);
        self.status.view = self.controller.view();
    }
    pub fn report_name_save(&mut self, confirmed: bool) -> Result<(), Error<N::Error, L::Error>> {
        self.controller.report_name_save(confirmed);
        self.status.view = self.controller.view();
        self.redraw_ready()
    }

    pub fn set_aprs_identity(&mut self, call: [u8; 6], ssid: u8) {
        self.controller.set_aprs_identity(call, ssid);
        self.status.view = self.controller.view();
    }

    pub fn set_aprs_status(
        &mut self,
        status: controller::AprsStatus,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.view.aprs_status == status {
            return Ok(());
        }
        self.controller.set_aprs_status(status);
        self.status.view = self.controller.view();
        if self.status.view.screen == Screen::Aprs {
            self.redraw_ready()?;
        }
        Ok(())
    }
    pub fn set_aprs_auto_available(
        &mut self,
        available: bool,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.view.aprs_auto_available != available {
            self.controller.set_aprs_auto_available(available);
            self.status.view = self.controller.view();
            if self.status.view.screen == Screen::Aprs {
                self.redraw_ready()?;
            }
        }
        Ok(())
    }

    /// Clear the operator opt-in after a backend refusal or source change.
    pub fn disarm_aprs_auto(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.view.aprs_auto_armed {
            self.controller.disarm_aprs_auto();
            self.status.view = self.controller.view();
            self.redraw_ready()?;
        }
        Ok(())
    }

    pub fn set_aprs_rx_available(
        &mut self,
        available: bool,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.view.aprs_rx_available != available {
            self.controller.set_aprs_rx_available(available);
            self.status.view = self.controller.view();
            if self.status.view.screen == Screen::AprsRx {
                self.redraw_ready()?;
            }
        }
        Ok(())
    }

    /// Add a decoded AX.25 UI packet to the volatile APRS history.
    pub fn push_aprs_packet(
        &mut self,
        source: &[u8],
        information: &[u8],
    ) -> Result<(), Error<N::Error, L::Error>> {
        self.controller.push_aprs_packet(source, information);
        self.status.view = self.controller.view();
        if self.status.view.screen == Screen::AprsRx {
            self.redraw_ready()?;
        }
        Ok(())
    }

    pub fn shortwave_scope_sample(
        &mut self,
        first_hz: u32,
        step_hz: u32,
        points: u8,
        bin: u8,
        rssi: u8,
        snr: u8,
        complete: bool,
    ) -> Result<(), Error<N::Error, L::Error>> {
        let view = &self.status.view;
        if !matches!(points, 8 | 16 | 32) || points != view.shortwave_scope_points || bin >= points
        {
            return Ok(());
        }
        let selection = (
            view.shortwave_mode,
            view.shortwave_fraction_hz,
            view.shortwave_bfo,
            view.shortwave_filter,
            view.shortwave_soft_mute,
            points,
        );
        let reset = self.shortwave_scope.first_hz != first_hz
            || self.shortwave_scope.step_hz != step_hz
            || self.shortwave_scope.point_count() != points
            || self.shortwave_scope_selection != Some(selection);
        if reset {
            self.shortwave_scope.reset(first_hz, step_hz, points);
            self.shortwave_scope_selection = Some(selection);
        }
        let damage = self.shortwave_scope.sample(bin, rssi, snr, complete);
        if self.status.view.screen == Screen::Shortwave && self.status.view.shortwave_scope {
            if reset {
                self.pending_rows.mark(55, 283);
            } else {
                for (start, end) in damage {
                    self.pending_rows.mark(start, end);
                }
            }
        }
        Ok(())
    }
    /// Read only the documented CFG range for auxiliary feature initialization.
    pub fn read_auxiliary_config(&mut self, offset: u16, bytes: &mut [u8]) -> bool {
        if usize::from(offset) + bytes.len() > 4096 {
            return false;
        }
        uv98_nor_read::read(&mut self.nor, CONFIG_ADDRESS + u32::from(offset), bytes).is_ok()
    }

    pub fn set_watch_rx(&mut self, slot: Option<Slot>) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.view.watch_rx != slot {
            self.controller.set_watch_rx(slot);
            self.redraw_ready()?;
        }
        Ok(())
    }

    pub fn set_crossband_probe(
        &mut self,
        status: controller::CrossbandProbe,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.view.crossband_probe != status {
            self.controller.set_crossband_probe(status);
            self.redraw_ready()?;
        }
        Ok(())
    }

    pub fn continue_shortwave_scan(
        &mut self,
        rssi: u8,
        snr: u8,
    ) -> Result<bool, Error<N::Error, L::Error>> {
        let before = self.controller.view();
        let next = self.controller.continue_shortwave_scan(rssi, snr);
        if !self.controller.view().same_view(&before) {
            self.redraw_ready()?;
        }
        Ok(next)
    }

    /// Stock SSB payload in external NOR. This path has no write opcode.
    pub fn read_shortwave_patch(&mut self, offset: u16, output: &mut [u8]) -> bool {
        const PATCH_BYTES: usize = 0x7bb * 8;
        if usize::from(offset) + output.len() > PATCH_BYTES {
            return false;
        }
        uv98_nor_read::read(&mut self.nor, 0x126000 + u32::from(offset), output).is_ok()
    }

    pub fn set_spectrum(
        &mut self,
        snapshot: SpectrumSnapshot,
    ) -> Result<bool, Error<N::Error, L::Error>> {
        self.set_spectrum_with_cursor(snapshot, None)
    }

    pub fn set_spectrum_with_cursor(
        &mut self,
        snapshot: SpectrumSnapshot,
        cursor: Option<u8>,
    ) -> Result<bool, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        if self.spectrum == Some(snapshot) && cursor.is_none() {
            return Ok(true);
        }
        if self.status.view.screen == Screen::Spectrum
            && self.spectrum.is_some()
            && self.pending_rows.any()
        {
            return Ok(false);
        }
        if snapshot.len > 0 {
            let new_len = snapshot.len;
            let old_cursor = self.status.view.spectrum_cursor;
            self.controller.clamp_spectrum_cursor(new_len - 1);
            let new_last = snapshot
                .start_10hz
                .saturating_add(snapshot.step_10hz.saturating_mul(u32::from(new_len - 1)));
            let previous = self.spectrum;
            let window_changed = previous.is_some_and(|old| {
                old.start_10hz != snapshot.start_10hz
                    || old.step_10hz != snapshot.step_10hz
                    || old.len != snapshot.len
            });
            if let (Some(old), true) = (previous, window_changed) {
                if old.len > 0 {
                    let cursor = old_cursor.min(old.len - 1);
                    let frequency = old
                        .start_10hz
                        .saturating_add(old.step_10hz.saturating_mul(u32::from(cursor)));
                    let index = if frequency < snapshot.start_10hz {
                        0
                    } else if frequency > new_last {
                        new_len - 1
                    } else {
                        let delta = frequency - snapshot.start_10hz;
                        ((delta.saturating_add(snapshot.step_10hz / 2)) / snapshot.step_10hz)
                            .min(u32::from(new_len - 1)) as u8
                    };
                    self.controller.place_spectrum_cursor(index);
                }
            } else if previous.is_none() {
                self.controller.place_spectrum_cursor((new_len - 1) / 2);
            }
        }
        let previous = self.spectrum.as_ref();
        let geometry_changed = previous.is_none_or(|old| {
            old.start_10hz != snapshot.start_10hz
                || old.step_10hz != snapshot.step_10hz
                || old.len != snapshot.len
        });
        let new_window =
            geometry_changed || previous.is_some_and(|old| old.window_id != snapshot.window_id);
        if geometry_changed {
            self.waterfall = render::Waterfall::new();
        }
        if snapshot.plot.sweep != 0
            && (new_window || previous.is_some_and(|old| old.plot.sweep != snapshot.plot.sweep))
        {
            self.waterfall.push(&snapshot.plot, snapshot.len);
        }
        if let Some(bin) = snapshot.plot.held_bin {
            if new_window || previous.is_some_and(|old| old.plot.held_bin != Some(bin)) {
                self.controller.place_spectrum_cursor(bin);
            }
        }
        if let Some(cursor) = cursor {
            self.controller
                .place_spectrum_cursor(cursor.min(snapshot.len.saturating_sub(1)));
        }
        self.status.view = self.controller.view();
        self.spectrum = Some(snapshot);
        if self.spectrum_error.is_failure() {
            self.last_rendered = None;
        }
        self.spectrum_error = SpectrumError::None;
        self.redraw_ready()?;
        Ok(true)
    }
    /// Refuse a start/restart and keep the spectrum page usable: overlay,
    /// window controls and the exact reason stay visible without a snapshot.
    pub fn reject_spectrum_start(
        &mut self,
        reason: SpectrumError,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        self.controller.reject_spectrum_start();
        self.spectrum_error = reason;
        // The refusal label is external model state, not a View field.
        self.last_rendered = None;
        self.redraw_ready()
    }
    /// Runtime owns temporary listening; the UI only needs its EXIT contract.
    pub fn set_spectrum_listening(
        &mut self,
        listening: bool,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        if self.controller.set_spectrum_listening(listening) {
            self.redraw_ready()?;
        }
        Ok(())
    }
    /// Main callback after the exact restore ACK: bind the displayed bin to the
    /// RF-confirmed bin before claiming that audio is being monitored.
    pub fn begin_spectrum_peak_listen(
        &mut self,
        bin: u8,
    ) -> Result<bool, Error<N::Error, L::Error>> {
        if self.controller.begin_peak_listen(bin) {
            self.redraw_ready()?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn confirm_spectrum_listen(&mut self, bin: u8) -> Result<bool, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        // Coordinator calls can change slots/pages outside the keyboard path.
        // A late restore callback on that new page is cancellation, never an
        // audio authorization, even if an older pending bin still matches.
        if self.status.view.screen != Screen::Spectrum {
            self.cancel_spectrum_pending()?;
            return Ok(false);
        }
        let accepted = self.controller.confirm_spectrum_listen(bin);
        if accepted {
            self.redraw_ready()?;
        }
        Ok(accepted)
    }
    /// Cancel a pending Listen before its restore ACK without changing RF.
    pub fn cancel_spectrum_pending(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        if self.controller.cancel_spectrum_pending() {
            self.redraw_ready()?;
        }
        Ok(())
    }
    /// Keep an existing snapshot but show why a new action was refused.
    pub fn note_spectrum_error(
        &mut self,
        reason: SpectrumError,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        if self.spectrum_error != reason {
            self.spectrum_error = reason;
            self.last_rendered = None;
            self.redraw_ready()?;
        }
        Ok(())
    }
    pub fn clear_spectrum(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        self.controller.cancel_spectrum_pending();
        self.spectrum = None;
        self.redraw_ready()
    }
    /// Only call after successful backend restoration and range/bin validation.
    pub fn accept_spectrum_frequency(
        &mut self,
        frequency_10hz: u32,
    ) -> Result<Update, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let change = self.controller.accept_spectrum_frequency(frequency_10hz);
        self.apply_change(change, None)
    }
    pub fn next_catalog_index(&self, current: Option<u16>, forward: bool) -> Option<u16> {
        if self.status.state != State::Ready {
            return None;
        }
        self.controller
            .ordered_channel(&self.bitmap, current, forward)
    }
    /// Apply RAM-only exclusions for one scan without changing the catalog or
    /// its zone order. The ordinary bounded channel traversal can wrap once.
    pub fn next_scan_channel(&self, skipped: &[u8; 128]) -> Option<u16> {
        self.next_scan_channel_after(self.status.view.channel, skipped, None)
    }

    /// A priority visit is temporarily outside the ordinary zone traversal.
    pub fn next_scan_channel_after(
        &self,
        cursor: Option<u16>,
        skipped: &[u8; 128],
        priority: Option<u16>,
    ) -> Option<u16> {
        if self.status.state != State::Ready {
            return None;
        }
        let mut eligible = core::array::from_fn(|index| self.bitmap[index] & !skipped[index]);
        if let Some(priority) = priority {
            if let Some(byte) = eligible.get_mut(usize::from(priority) / 8) {
                *byte &= !(1 << (priority % 8));
            }
        }
        self.controller.ordered_channel(&eligible, cursor, true)
    }

    pub fn scan_zone_priority(&self) -> Option<u16> {
        if self.status.state != State::Ready {
            return None;
        }
        self.controller.scan_zone_priority(&self.bitmap)
    }
    pub fn set_radio_view(&mut self, radio: RadioView) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        self.controller.set_radio_choices(
            radio.power,
            radio.monitor,
            radio.modulation == render::Modulation::Am,
        );
        if self.radio == radio {
            return Ok(());
        }
        let old_slot = self.radio.transmitting_slot();
        if old_slot != radio.transmitting_slot() {
            self.update_tx_status(old_slot, None);
        }
        self.radio = radio;
        self.redraw_ready()
    }

    /// Publish telemetry for the confirmed manual PTT session.
    pub fn set_tx_status(
        &mut self,
        remaining_seconds: Option<u8>,
        sag_mv: Option<u16>,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let slot = self.radio.transmitting_slot();
        let status = slot
            .and(remaining_seconds)
            .map(|remaining_seconds| ManualTxStatus {
                remaining_seconds,
                sag_mv,
            });
        self.update_tx_status(slot, status);
        Ok(())
    }

    fn update_tx_status(&mut self, slot: Option<Slot>, status: Option<ManualTxStatus>) {
        if self.manual_tx == status {
            return;
        }
        self.manual_tx = status;
        if let Some(slot) = slot.filter(|_| self.status.view.screen == Screen::Home) {
            let (start, end) = render::home_tx_status_rows(slot);
            self.pending_rows.mark(start, end);
        }
    }

    /// Select the locale for a synthetic host preview.
    #[cfg(feature = "locale-all")]
    pub fn set_preview_language(
        &mut self,
        language: uv98_handheld_ui::locale::Language,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        self.controller.set_preview_language(language);
        self.redraw_ready()
    }

    /// Publish pack millivolts without redrawing unrelated rows or touching NOR.
    pub fn set_battery_voltage(
        &mut self,
        millivolts: Option<u16>,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        if render::battery_display(self.battery_mv) != render::battery_display(millivolts) {
            let (start, end) = render::battery_rows();
            self.pending_rows.mark(start, end);
        }
        self.battery_mv = millivolts;
        Ok(())
    }

    pub fn enable_fast_matrix_input(&mut self) {
        self.input.enable_fast_matrix();
    }

    /// Use the shorter PTT debounce with isolated physical input and an RF guard.
    pub fn enable_fast_ptt_input(&mut self) {
        self.input.enable_fast_ptt();
    }

    /// Physical identification of the lower side key, separate from its
    /// assigned action. Callers pass true only after a traced input works.
    pub fn set_lower_side_mapped(&mut self, mapped: bool) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        self.input.set_aux16_ptt(mapped);
        if self.lower_side_mapped != mapped {
            self.lower_side_mapped = mapped;
            self.redraw_ready()?;
        }
        Ok(())
    }

    /// Confirm runtime acceptance of global RAM lighting policy; no storage IO.
    pub fn set_lighting_modes(
        &mut self,
        display: LightMode,
        keys: LightMode,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let change = self.controller.set_lighting_modes(display, keys);
        self.apply_change(change, None)?;
        Ok(())
    }
    pub fn acknowledge_scan_started(&mut self) -> Result<bool, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let accepted = self.controller.acknowledge_scan_started();
        self.redraw_ready()?;
        Ok(accepted)
    }
    pub fn acknowledge_scan_stopped(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        self.controller.acknowledge_scan_stopped();
        self.redraw_ready()
    }
    pub fn set_scan_paused(&mut self, paused: bool) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        if self.controller.set_scan_paused(paused) {
            self.redraw_ready()?;
        }
        Ok(())
    }
    pub fn scan_cursor_held(&self) -> bool {
        self.status.view.screen == Screen::Home && self.input.short_star_pending()
    }
    pub fn reject_radio_request(
        &mut self,
        request: Request,
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        self.controller.reject_radio_request(request);
        self.redraw_ready()
    }
    pub fn reject_tx(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let change = self.controller.reject_tx();
        self.apply_change(change, None)?;
        Ok(())
    }
    pub fn cancel_scan(&mut self) -> Result<Update, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let change = self.controller.cancel_scan();
        self.apply_change(change, None)
    }
    pub fn select_slot(&mut self, slot: Slot) -> Result<Update, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let change = self.controller.select_slot(slot);
        self.apply_change(change, None)
    }
    pub fn select_scan_channel(&mut self, index: u16) -> Result<Update, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let change = self.controller.select_scan_channel(index, &self.bitmap);
        self.apply_change(change, None)
    }
    pub fn select_scan_frequency(
        &mut self,
        frequency_10hz: u32,
    ) -> Result<Update, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let change = self.controller.select_scan_frequency(frequency_10hz);
        self.apply_change(change, None)
    }
    fn redraw_ready(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        self.status.state = State::Failed;
        self.status.view = self.controller.view();
        self.draw()?;
        self.status.state = State::Ready;
        Ok(())
    }
    pub fn start(&mut self, slot: usize) -> Result<(), Error<N::Error, L::Error>> {
        self.start_observed(slot, |_| {})
    }
    /// Catalog is a 128-byte active bitmap, never a full mutable codeplug copy.
    /// Any failed read is terminal; partial catalogs and stale channels are hidden.
    pub fn start_observed(
        &mut self,
        _slot: usize,
        mut observe: impl FnMut(StartStage),
    ) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::New {
            return Err(Error::State);
        }
        self.status.state = State::Failed;
        observe(StartStage::DisplayBegin);
        self.display.initialize().map_err(Error::Display)?;
        observe(StartStage::CalibrationBegin);
        self.diagnostic
            .load_calibration(&mut self.nor)
            .map_err(Error::Read)?;
        let digest: [u8; 32] = Sha256::digest(
            self.diagnostic
                .calibration_view()
                .expect("validated calibration")
                .raw(),
        )
        .into();
        self.status.calibration_sha256 = Some(digest);
        self.status.calibration_matches_backup = Some(digest == self.expected_sha256);
        observe(StartStage::ChannelBegin);
        let mut raw = [0; CHANNEL_LEN];
        for index in 0..CHANNEL_COUNT {
            uv98_nor_read::read(
                &mut self.nor,
                CHANNELS_ADDRESS + (index * CHANNEL_LEN) as u32,
                &mut raw,
            )
            .map_err(Error::Catalog)?;
            if Channel::parse(&raw).expect("fixed record").is_active() {
                self.bitmap[index / 8] |= 1 << (index % 8);
            }
        }
        self.all_bitmap = self.bitmap;
        self.controller = Controller::new(&self.bitmap, 14_550_000);
        self.read_config()?;
        self.read_zone_catalog()?;
        self.controller.set_zone_catalog(&self.zone_bitmap);
        if let Some(index) = self.controller.view().channel {
            self.diagnostic
                .load_channel(&mut self.nor, index as usize)
                .map_err(Error::Read)?;
        }
        self.refresh_peer()?;
        self.status.view = self.controller.view();
        observe(StartStage::RenderBegin);
        self.draw()?;
        self.status.state = State::Ready;
        while self.pending_rows.any() {
            self.render_step(4)?;
        }
        observe(StartStage::Complete);
        Ok(())
    }
    pub fn sample<I: Into<InputSample>>(
        &mut self,
        now_ms: u32,
        sample: I,
    ) -> Result<Update, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        let sample = sample.into();
        self.status.raw_sample = match sample {
            InputSample::Stable {
                matrix,
                ptt_pressed,
            }
            | InputSample::WithAux {
                matrix,
                ptt_pressed,
                ..
            } => Some((matrix, ptt_pressed)),
            InputSample::Unstable | InputSample::MatrixTransition => None,
        };
        let normalized = match sample {
            InputSample::Unstable => Sample::Invalid,
            InputSample::MatrixTransition => Sample::UncertainMatrix,
            InputSample::Stable {
                matrix,
                ptt_pressed,
            } => normalize_input(matrix, ptt_pressed, None),
            InputSample::WithAux {
                matrix,
                ptt_pressed,
                aux,
            } => normalize_input(matrix, ptt_pressed, aux),
        };
        let event = match self.input.sample(now_ms, normalized) {
            Ok(e) => e,
            Err(_) => {
                self.status.state = State::Failed;
                return Err(Error::Clock);
            }
        };
        let ready_changed = self.status.input_ready != self.input.is_ready();
        self.status.input_ready = self.input.is_ready();
        let Some(event) = event else {
            let rendered = ready_changed && self.status.view.screen == Screen::Keys;
            if rendered {
                // Readiness is model metadata, not part of View's dirty comparison.
                self.last_rendered = None;
                self.status.state = State::Failed;
                self.draw()?;
                self.status.state = State::Ready;
            }
            return Ok(Update {
                rendered,
                pressed: None,
                channel_changed: false,
                request: None,
                cancel_temporary: false,
                ptt_tx_requested: false,
                ptt_slot: None,
                zone_changed: false,
                slot_changed: false,
            });
        };
        let pressed = match event {
            InputEvent::Short(k) | InputEvent::Hold(k) | InputEvent::Repeat(k) => Some(k),
            InputEvent::Ptt(true) => Some(Key::Ptt),
            InputEvent::PttB(true) => Some(Key::Aux16),
            InputEvent::Ptt(false) | InputEvent::PttB(false) => None,
        };
        if let Some(key) = pressed {
            self.status.last_pressed = Some(key);
            self.status.presses = self.status.presses.saturating_add(1);
        }
        let view = self.controller.view();
        if event == InputEvent::Short(Key::Digit0)
            && view.screen == Screen::Spectrum
            && !view.spectrum_menu
            && !view.spectrum_listening
            && view.spectrum_pending_listen.is_none()
        {
            if let Some((index, _)) = self
                .spectrum
                .as_ref()
                .filter(|snapshot| snapshot.plot.held_bin.is_none())
                .and_then(|snapshot| snapshot.plot.strongest())
            {
                if self.controller.place_spectrum_cursor(index) {
                    self.redraw_ready()?;
                }
            }
        }
        let change = self.controller.handle_at(event, &self.bitmap, now_ms);
        self.apply_change(change, pressed)
    }

    fn apply_change(
        &mut self,
        mut change: Change,
        pressed: Option<Key>,
    ) -> Result<Update, Error<N::Error, L::Error>> {
        self.status.state = State::Failed;
        if change.request == Some(Request::StartSpectrum) {
            // A newly requested grid cannot display a previous session's bins.
            self.spectrum = None;
            self.waterfall = render::Waterfall::new();
        }
        if let Some(Request::ApplyRxToneOverride { source, tone_raw }) = change.request {
            let view = self.controller.view();
            if RxToneSource::for_view(view) == Some(source)
                && matches!(
                    Tone::decode(tone_raw),
                    Some(Tone::Ctcss { .. } | Tone::Dcs { .. })
                )
            {
                self.rx_tones[usize::from(view.active_slot == Slot::B)] = Some((source, tone_raw));
            }
        }
        if change.zone_changed || change.slot_changed {
            self.load_current_zone()?;
            let reconciled = self.controller.reconcile_channels(&self.bitmap);
            change.channel_changed |= reconciled.channel_changed;
            change.redraw |= reconciled.redraw;
            change.cancel_temporary |= reconciled.cancel_temporary;
            if reconciled.request == Some(Request::StopScan) || change.request.is_none() {
                change.request = reconciled.request;
            }
            self.list_anchor = None;
            self.list_len = 0;
        }
        let view = self.controller.view();
        let peer_cb_changed = view.active_slot == Slot::B && self.status.view.cb != view.cb;
        self.status.view = view;
        if change.channel_changed || change.slot_changed || change.zone_changed || peer_cb_changed {
            if let Some(index) = self.status.view.channel {
                if self
                    .diagnostic
                    .snapshot()
                    .is_none_or(|snapshot| snapshot.index() != index as usize)
                {
                    self.diagnostic
                        .load_channel(&mut self.nor, index as usize)
                        .map_err(Error::Read)?;
                }
            }
            self.refresh_peer()?;
            if (change.slot_changed || change.zone_changed || peer_cb_changed)
                && self.status.view.screen == Screen::Home
            {
                // The peer record/name is external model data, not a View field.
                // Refresh its card without discarding an unrelated page pass.
                let (start, end) = render::home_slot_rows(self.status.view.active_slot.other());
                self.pending_rows.mark(start, end);
            }
        }
        if change.redraw {
            self.draw()?;
        }
        self.status.state = State::Ready;
        Ok(Update {
            rendered: change.redraw,
            pressed,
            channel_changed: change.channel_changed,
            request: change.request,
            cancel_temporary: change.cancel_temporary,
            ptt_tx_requested: change.ptt_tx_requested,
            ptt_slot: change.ptt_slot,
            zone_changed: change.zone_changed,
            slot_changed: change.slot_changed,
        })
    }
    pub fn into_parts(self) -> (N, Display<L, D>) {
        (self.nor, self.display)
    }
    pub fn render_pending(&self) -> bool {
        self.pending_outlines != 0 || self.pending_rows.any()
    }

    /// Send up to max_rows full-row equivalents from the current model.
    /// Check admission before each window of at most six rows or one outline
    /// edge. A transfer already started runs synchronously. The caller still
    /// polls physical inputs before the next batch.
    pub fn render_batch(
        &mut self,
        max_rows: u16,
        mut admit: impl FnMut() -> bool,
    ) -> Result<RenderProgress, Error<N::Error, L::Error>> {
        let mut progress = self.render_step(0)?;
        while progress.pending && progress.rows < max_rows && admit() {
            let next = self.render_step((max_rows - progress.rows).min(6))?;
            progress.rows += next.rows;
            progress.pending = next.pending;
        }
        Ok(progress)
    }

    /// Send at most max_rows contiguous rows, or one narrow outline edge that
    /// costs one row of the budget. No NOR read or input polling is hidden in
    /// this call. Use render_batch to combine windows under one polling budget.
    pub fn render_step(
        &mut self,
        max_rows: u16,
    ) -> Result<RenderProgress, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        if max_rows == 0 {
            return Ok(RenderProgress {
                rows: 0,
                pending: self.render_pending(),
            });
        }
        // Exclude queued priority bits from the ordinary cursor search. This
        // makes the fairness batch useful even if it would otherwise land on
        // the same repeatedly invalidated banner. The normal cursor is not
        // advanced by priority work.
        let ordinary = self.pending_rows.without(&self.priority_rows);
        let normal_start = ordinary.next(self.row_cursor);
        let admit_priority = normal_start.is_none() || self.priority_run < PRIORITY_BURST_ROWS;
        let priority = admit_priority && self.priority_rows.any();
        // One narrow transfer makes confirmed TX visible even when an older
        // pop-up spent the priority budget. Everything else keeps its quota.
        let tx_top = self
            .render_radio
            .transmitting_slot()
            .map(render::HomeOutlineEdge::top)
            .filter(|edge| self.pending_outlines & edge.bit() != 0);
        let edge = tx_top.or_else(|| {
            if !priority && admit_priority {
                render::HomeOutlineEdge::next(self.pending_outlines)
            } else {
                None
            }
        });
        if let Some(edge) = edge {
            self.status.state = State::Failed;
            render::render_home_outline_edge(
                &mut self.display,
                edge,
                self.status.view.active_slot,
                self.render_radio,
            )
            .map_err(Error::Render)?;
            self.pending_outlines &= !edge.bit();
            self.priority_run = self.priority_run.saturating_add(1);
            return Ok(self.finish_render_step(1));
        }
        let menu = if !priority && self.menu_run < MENU_BURST_ROWS {
            self.menu_priority.iter().find_map(|&(start, end)| {
                if start >= end {
                    return None;
                }
                ordinary
                    .next(start)
                    .filter(|row| (start..end).contains(row))
                    .map(|row| (row, end - row))
            })
        } else {
            None
        };
        let (start, limit) = if priority {
            let limit = if normal_start.is_some() {
                max_rows.min(PRIORITY_BURST_ROWS - self.priority_run)
            } else {
                max_rows
            };
            (self.priority_rows.next(0), limit)
        } else if let Some((start, remaining)) = menu {
            (
                Some(start),
                max_rows.min(remaining).min(MENU_BURST_ROWS - self.menu_run),
            )
        } else {
            (normal_start, max_rows)
        };
        let Some(start) = start else {
            return Ok(RenderProgress {
                rows: 0,
                pending: false,
            });
        };
        let mut end = start + 1;
        while end - start < limit
            && if priority {
                self.priority_rows.has(end)
            } else {
                ordinary.has(end)
            }
        {
            end += 1;
        }
        self.status.state = State::Failed;
        self.paint_rows(start, end)?;
        self.pending_rows.clear(start, end);
        self.priority_rows.clear(start, end);
        if priority {
            self.priority_run = self.priority_run.saturating_add(end - start);
        } else {
            self.priority_run = 0;
            if menu.is_some() {
                self.menu_run += end - start;
            } else {
                self.menu_run = 0;
                self.row_cursor = if end == render::HEIGHT { 0 } else { end };
            }
        }
        Ok(self.finish_render_step(end - start))
    }

    fn finish_render_step(&mut self, rows: u16) -> RenderProgress {
        if self.pending_outlines == 0 && !self.priority_rows.any() {
            self.priority_run = 0;
        }
        if !self.render_pending() {
            // One combined follow-up captures telemetry that changed while the
            // pass was rendering, including a value that changed and changed back.
            self.pending_rows.merge_take(&mut self.deferred_radio_rows);
            self.render_radio = self.radio;
            self.menu_run = 0;
        }
        self.status.state = State::Ready;
        RenderProgress {
            rows,
            pending: self.render_pending(),
        }
    }

    fn read_config(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        let mut raw = [0; 15];
        uv98_nor_read::read(&mut self.nor, CONFIG_ADDRESS + 0xa4, &mut raw)
            .map_err(Error::Catalog)?;
        self.config = Some(RadioConfig {
            squelch: raw[0],
            mic_gain: raw[9],
            fm_volume: [raw[10], raw[11]],
            am_volume: [raw[12], raw[13]],
            squelch_delay: raw[14],
        });
        Ok(())
    }

    fn read_zone_catalog(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        let mut raw = [0; RECORD_LEN];
        for index in 0..ZONE_COUNT {
            uv98_nor_read::read(
                &mut self.nor,
                ZONES_ADDRESS + (index * RECORD_LEN) as u32,
                &mut raw,
            )
            .map_err(Error::Catalog)?;
            let zone = Zone::from_bytes(&raw);
            if !zone_header_valid(zone) {
                continue;
            }
            self.zone_bitmap[index / 8] |= 1 << (index % 8);
            self.zone_names[index].copy_from_slice(zone.name_raw());
            let mut members = [0; MEMBER_COUNT];
            let mut membership = [0; 128];
            self.zone_counts[index] =
                collect_zone_members(zone, &self.all_bitmap, &mut members, &mut membership) as u16;
        }
        Ok(())
    }

    fn load_current_zone(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        let Some(index) = self.controller.view().zone else {
            self.bitmap = self.all_bitmap;
            self.controller.set_channel_order(&[]);
            return Ok(());
        };
        let mut raw = [0; RECORD_LEN];
        uv98_nor_read::read(
            &mut self.nor,
            ZONES_ADDRESS + u32::from(index) * RECORD_LEN as u32,
            &mut raw,
        )
        .map_err(Error::Catalog)?;
        let zone = Zone::from_bytes(&raw);
        if !zone_header_valid(zone) {
            return Err(Error::ZoneLayout(index));
        }
        let mut order = [0; MEMBER_COUNT];
        let len = collect_zone_members(zone, &self.all_bitmap, &mut order, &mut self.bitmap);
        self.controller.set_channel_order(&order[..len]);
        self.zone_counts[index as usize] = len as u16;
        self.zone_names[index as usize].copy_from_slice(zone.name_raw());
        Ok(())
    }

    fn refresh_peer(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        let other = if self.controller.view().active_slot == Slot::A {
            Slot::B
        } else {
            Slot::A
        };
        let context = self.controller.context(other);
        let selected = context.channel.filter(|&index| {
            context.mode == Mode::Channel && controller::channel_active(&self.all_bitmap, index)
        });
        let Some(index) = selected else {
            self.peer_record = None;
            return Ok(());
        };
        if self
            .peer_record
            .is_some_and(|record| record.index == index as usize)
        {
            return Ok(());
        }
        let mut record = Record::EMPTY;
        record.index = index as usize;
        if let Some(snapshot) = self
            .diagnostic
            .snapshot()
            .filter(|snapshot| snapshot.index() == record.index)
        {
            record.raw.copy_from_slice(snapshot.channel.raw());
            self.peer_record = Some(record);
            return Ok(());
        }
        uv98_nor_read::read(
            &mut self.nor,
            CHANNELS_ADDRESS + u32::from(index) * CHANNEL_LEN as u32,
            &mut record.raw,
        )
        .map_err(Error::Catalog)?;
        self.peer_record = Some(record);
        Ok(())
    }

    fn peer_view(&self) -> PeerView<'_> {
        Self::peer_view_from_parts(
            &self.controller,
            self.peer_record.as_ref(),
            &self.zone_names,
        )
    }

    fn peer_view_from_parts<'a>(
        controller: &Controller,
        peer_record: Option<&'a Record>,
        zone_names: &'a [[u8; 16]; ZONE_COUNT],
    ) -> PeerView<'a> {
        let view = controller.view_ref();
        let slot = if view.active_slot == Slot::A {
            Slot::B
        } else {
            Slot::A
        };
        let context = controller.context(slot);
        let (rx, tx) = if context.mode == Mode::Vfo {
            let shifted = controller::vfo_tx_frequency_for(
                context.vfo_10hz,
                context.shift_index,
                view.cb_for_slot(slot),
            );
            (Some(context.vfo_10hz), shifted)
        } else {
            peer_record
                .filter(|record| Some(record.index as u16) == context.channel)
                .map_or((None, None), |record| {
                    let channel = record.view();
                    (Some(channel.rx_10hz), Some(channel.tx_10hz))
                })
        };
        let reverse = context.reverse && view.cb_for_slot(slot).is_none();
        let (frequency_10hz, tx_frequency_10hz) = if reverse { (tx, rx) } else { (rx, tx) };
        PeerView {
            slot,
            mode: context.mode,
            frequency_10hz,
            tx_frequency_10hz,
            channel: context.channel,
            name: if context.mode == Mode::Channel {
                peer_record
                    .filter(|record| Some(record.index as u16) == context.channel)
                    .map_or(&[], |record| &record.raw[32..48])
            } else {
                &[]
            },
            zone: context.zone,
            zone_name: context.zone.map(|zone| &zone_names[zone as usize][..]),
            reverse,
        }
    }

    fn refresh_zones(&mut self) {
        if self.status.view.screen != Screen::Zones {
            return;
        }
        let cursor = self.status.view.zone_cursor;
        if self.zone_window[..self.zone_window_len].contains(&cursor) {
            return;
        }
        let ordinal = cursor.map_or(0, |selected| {
            1 + (0..selected)
                .filter(|&index| self.zone_bitmap[index as usize / 8] & (1 << (index % 8)) != 0)
                .count()
        });
        let start = ordinal / 7 * 7;
        let mut indices = [None; 7];
        let mut count = 0;
        let mut seen = 0;
        for candidate in 0..=ZONE_COUNT {
            let index = if candidate == 0 {
                None
            } else {
                Some((candidate - 1) as u8)
            };
            if index
                .is_some_and(|index| self.zone_bitmap[index as usize / 8] & (1 << (index % 8)) == 0)
            {
                continue;
            }
            if seen >= start && count < indices.len() {
                indices[count] = index;
                count += 1;
            }
            seen += 1;
            if count == indices.len() {
                break;
            }
        }
        if self.zone_window != indices || self.zone_window_len != count {
            self.zone_window = indices;
            self.zone_window_len = count;
            self.pending_rows.mark(54, 278);
        }
    }

    fn refresh_list(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.view.screen != Screen::Channels {
            return Ok(());
        }
        // Page by the CPS membership order, preserving sparse stored numbering.
        let selected = self.status.view.list_channel;
        if selected.is_some_and(|index| {
            self.list[..self.list_len]
                .iter()
                .any(|record| record.index == index as usize)
        }) {
            return Ok(());
        }
        let mut ordinal = 0usize;
        let first = self.controller.first_ordered_channel(&self.bitmap);
        let mut cursor = first;
        for _ in 0..CHANNEL_COUNT {
            if cursor.is_none() || cursor == selected {
                break;
            }
            ordinal += 1;
            cursor = self.controller.ordered_channel(&self.bitmap, cursor, true);
            if cursor == first {
                break;
            }
        }
        let first_ordinal = ordinal / 7 * 7;
        let mut seen = 0usize;
        let mut indices = [0u16; 7];
        let mut count = 0usize;
        let mut cursor = first;
        for _ in 0..CHANNEL_COUNT {
            let Some(index) = cursor else {
                break;
            };
            if seen >= first_ordinal && count < 7 {
                indices[count] = index;
                count += 1;
            }
            seen += 1;
            if count == 7 {
                break;
            }
            cursor = self.controller.ordered_channel(&self.bitmap, cursor, true);
            if cursor == first {
                break;
            }
        }
        let anchor = if count > 0 { Some(indices[0]) } else { None };
        if anchor == self.list_anchor && self.list_len == count {
            return Ok(());
        }
        self.list_len = 0;
        for &index in &indices[..count] {
            let mut record = Record::EMPTY;
            record.index = index as usize;
            uv98_nor_read::read(
                &mut self.nor,
                CHANNELS_ADDRESS + index as u32 * CHANNEL_LEN as u32,
                &mut record.raw,
            )
            .map_err(Error::Catalog)?;
            self.list[self.list_len] = record;
            self.list_len += 1;
        }
        self.list_anchor = anchor;
        self.pending_rows.mark(54, 278);
        Ok(())
    }
    fn draw(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        let view = self.status.view;
        let selection = (
            view.shortwave_mode,
            view.shortwave_fraction_hz,
            view.shortwave_bfo,
            view.shortwave_filter,
            view.shortwave_soft_mute,
            view.shortwave_scope_points,
        );
        if self
            .shortwave_scope_selection
            .is_some_and(|old| old != selection)
        {
            self.shortwave_scope = render::ShortwaveScope::new();
            self.shortwave_scope_selection = None;
            if view.screen == Screen::Shortwave && view.shortwave_scope {
                self.pending_rows.mark(55, 283);
            }
        }
        // Radio-only reports cannot change a channel/zone window. Avoid the
        // catalog walks on every RSSI observation, especially in channel lists.
        if self
            .last_rendered
            .as_ref()
            .is_none_or(|last| !last.same_view(&view))
        {
            self.refresh_list()?;
            self.refresh_zones();
        }
        let page_changed = self.render_page != Some(self.status.view.screen);
        #[cfg(feature = "pa5-audio")]
        if page_changed
            && (self.render_page == Some(Screen::About) || self.status.view.screen == Screen::About)
        {
            self.pending_rows.mark(0, render::HEIGHT);
        }
        if page_changed {
            // These bitmaps describe work not yet on the physical panel, not
            // an old framebuffer. A partial page transition preserves chrome;
            // dropping prior lock/banner/startup damage could leave stale
            // pixels forever because last_* describes the last planned model.
            // Keep that damage and paint every row from the newest model.
            self.pending_rows.merge_take(&mut self.deferred_radio_rows);
            self.priority_rows = PendingRows::default();
            self.pending_outlines = 0;
            self.priority_run = 0;
            self.menu_priority = [(0, 0); 3];
            self.menu_run = 0;
            self.row_cursor = 0;
            self.render_page = Some(self.status.view.screen);
        }
        if self.last_rendered.is_none() && !self.pending_rows.any() {
            self.row_cursor = 0;
        }
        let defer_telemetry = !page_changed
            && self.render_pending()
            && radio_controls_equal(self.last_radio, self.radio);
        let screen = self.status.view.screen;
        let snapshot = matches!(screen, Screen::Home | Screen::ChannelDetails)
            .then(|| self.diagnostic.snapshot())
            .flatten()
            .filter(|snapshot| Some(snapshot.index() as u16) == self.status.view.channel);
        let channel = snapshot.as_ref().map(|s| ChannelView {
            index: s.index(),
            name: s.channel.name_raw(),
            rx_10hz: s.channel.rx_frequency_code(),
            tx_10hz: s.channel.tx_frequency_code(),
            active: s.channel.is_active(),
            rx_tone_raw: self
                .rx_tone_override(self.status.view)
                .unwrap_or_else(|| s.channel.rx_tone_raw()),
            tx_tone_raw: s.channel.tx_tone_raw(),
            bandwidth_raw: s.channel.bandwidth_raw(),
            power_raw: s.channel.power_raw(),
        });
        let list_views: [ChannelView<'_>; 7];
        let list = if screen == Screen::Channels {
            list_views = core::array::from_fn(|i| self.list[i].view());
            &list_views[..self.list_len]
        } else {
            &[]
        };
        let zone_views;
        let zones = if screen == Screen::Zones {
            zone_views = zone_views_from_parts(
                &self.zone_window,
                &self.zone_names,
                &self.zone_counts,
                controller::active_count(&self.all_bitmap) as u16,
            );
            &zone_views[..self.zone_window_len]
        } else {
            &[]
        };
        let peer = (screen == Screen::Home).then(|| self.peer_view());
        let model = Model {
            view: self.status.view,
            channel,
            list,
            calibration_matches: self.status.calibration_matches_backup,
            last_key: self.status.last_pressed,
            ptt_pressed: self.status.view.ptt_down,
            input_ready: self.status.input_ready,
            radio: self.radio,
            manual_tx: self.manual_tx,
            battery_mv: self.battery_mv,
            zone_name: self
                .status
                .view
                .zone
                .map(|zone| &self.zone_names[zone as usize][..]),
            zones,
            peer,
            spectrum: self.spectrum.as_ref().map(|snapshot| SpectrumView {
                waterfall: Some(&self.waterfall),
                ..snapshot.view()
            }),
            lower_side_mapped: self.lower_side_mapped,
            spectrum_error: self.spectrum_error,
            shortwave_scope: Some(&self.shortwave_scope),
        };
        let damage = render::plan_damage(
            &model,
            self.last_rendered.as_ref(),
            self.last_radio,
            self.last_spectrum.as_ref().map(SpectrumSnapshot::view),
        )
        .map_err(|e| Error::Render(render::RenderError::Model(e)))?;
        for &(start, end) in damage.controller.ranges() {
            self.pending_rows.mark(start, end);
        }
        if screen == Screen::Menu
            && self.last_rendered.as_ref().is_none_or(|old| {
                old.screen != Screen::Menu
                    || old.menu_group != view.menu_group
                    || old.menu_index != view.menu_index
            })
        {
            let previous = self
                .last_rendered
                .as_ref()
                .filter(|old| {
                    old.screen == Screen::Menu
                        && old.menu_group == view.menu_group
                        && usize::from(old.menu_index) / render::LIST_ROWS
                            == usize::from(view.menu_index) / render::LIST_ROWS
                })
                .map_or((0, 0), |old| render::menu_item_rows(old.menu_index));
            // Keep the title of a new group and the latest selected item
            // ahead of older body damage. Clear a same-page previous selection
            // next. New input replaces the ordering, retaining all damage.
            self.menu_priority = [(11, 18), render::menu_item_rows(view.menu_index), previous];
        }
        for &(start, end) in damage.spectrum.ranges() {
            self.pending_rows.mark(start, end);
        }
        self.pending_outlines |= damage.radio.home_outline_edges();
        if defer_telemetry {
            for &(start, end) in damage.radio.ranges() {
                self.deferred_radio_rows.mark(start, end);
            }
        } else {
            // Control changes and visible signaling are never deferred.
            self.render_radio = self.radio;
            self.pending_rows.merge_take(&mut self.deferred_radio_rows);
            for &(start, end) in damage.radio.ranges() {
                self.pending_rows.mark(start, end);
            }
        }
        if self.radio.state == render::RadioState::Receiving {
            if self.last_radio.radio_name != self.radio.radio_name
                && self.radio.signaling_popup_damage(self.last_radio).is_some()
            {
                self.mark_priority(render::NAME_POPUP_PRIORITY_ROWS);
            }
        } else if self.last_radio.state == render::RadioState::Receiving {
            // Remaining popup damage stays in the ordinary queue. Preserve
            // the shared fairness budget when radio states change rapidly.
            let (start, end) = render::NAME_POPUP_PRIORITY_ROWS;
            self.priority_rows.clear(start, end);
        }
        let old_tx = self.last_radio.transmitting_slot();
        let new_tx = self.radio.transmitting_slot();
        let badge_changed = screen == Screen::Home && old_tx != new_tx;
        if badge_changed {
            if let Some(slot) = new_tx {
                self.pending_outlines |= render::HomeOutlineEdge::top(slot).bit();
            }
            for slot in [old_tx, new_tx].into_iter().flatten() {
                let start = render::home_slot_rows(slot).0;
                let end = render::home_ptt_badge_rows(slot).1;
                self.mark_priority((start, end));
            }
        }
        let off_ptt_changed = self.radio.state == render::RadioState::Off
            && self
                .last_rendered
                .is_some_and(|old| old.ptt_down != self.status.view.ptt_down);
        let state_changed = self.last_radio.state != self.radio.state;
        let fault = matches!(
            self.radio.state,
            render::RadioState::Unavailable | render::RadioState::Failed
        );
        if off_ptt_changed || state_changed && (!badge_changed || fault) {
            self.mark_priority(render::radio_banner_rows(screen));
        }
        self.last_rendered = Some(self.status.view);
        self.last_radio = self.radio;
        self.last_spectrum = self.spectrum;
        Ok(())
    }

    fn mark_priority(&mut self, (start, end): (u16, u16)) {
        self.pending_rows.mark(start, end);
        self.priority_rows.mark(start, end);
    }

    fn paint_rows(&mut self, start: u16, end: u16) -> Result<(), Error<N::Error, L::Error>> {
        let screen = self.status.view.screen;
        #[cfg(feature = "pa5-audio")]
        if screen == Screen::About {
            return render::render_audio_rows(
                &mut self.display,
                self.audio_capture,
                self.status.view.language(),
                start,
                end,
            )
            .map_err(Error::Render);
        }
        let snapshot = matches!(screen, Screen::Home | Screen::ChannelDetails)
            .then(|| self.diagnostic.snapshot())
            .flatten()
            .filter(|snapshot| Some(snapshot.index() as u16) == self.status.view.channel);
        let channel = snapshot.as_ref().map(|s| ChannelView {
            index: s.index(),
            name: s.channel.name_raw(),
            rx_10hz: s.channel.rx_frequency_code(),
            tx_10hz: s.channel.tx_frequency_code(),
            active: s.channel.is_active(),
            rx_tone_raw: self
                .rx_tone_override(self.status.view)
                .unwrap_or_else(|| s.channel.rx_tone_raw()),
            tx_tone_raw: s.channel.tx_tone_raw(),
            bandwidth_raw: s.channel.bandwidth_raw(),
            power_raw: s.channel.power_raw(),
        });
        let list_views: [ChannelView<'_>; 7];
        let list = if screen == Screen::Channels {
            list_views = core::array::from_fn(|i| self.list[i].view());
            &list_views[..self.list_len]
        } else {
            &[]
        };
        let zone_views;
        let zones = if screen == Screen::Zones {
            zone_views = zone_views_from_parts(
                &self.zone_window,
                &self.zone_names,
                &self.zone_counts,
                controller::active_count(&self.all_bitmap) as u16,
            );
            &zone_views[..self.zone_window_len]
        } else {
            &[]
        };
        let peer = (screen == Screen::Home).then(|| {
            Self::peer_view_from_parts(
                &self.controller,
                self.peer_record.as_ref(),
                &self.zone_names,
            )
        });
        let model = Model {
            view: self.status.view,
            channel,
            list,
            calibration_matches: self.status.calibration_matches_backup,
            last_key: self.status.last_pressed,
            ptt_pressed: self.status.view.ptt_down,
            input_ready: self.status.input_ready,
            radio: self.render_radio,
            manual_tx: self.manual_tx,
            battery_mv: self.battery_mv,
            zone_name: self
                .status
                .view
                .zone
                .map(|zone| &self.zone_names[zone as usize][..]),
            zones,
            peer,
            spectrum: self.spectrum.as_ref().map(|snapshot| SpectrumView {
                waterfall: Some(&self.waterfall),
                ..snapshot.view()
            }),
            lower_side_mapped: self.lower_side_mapped,
            spectrum_error: self.spectrum_error,
            shortwave_scope: Some(&self.shortwave_scope),
        };
        render::render_rows_cached(&mut self.display, &model, start, end, &mut self.menu_cache)
            .map_err(Error::Render)
    }
}

/// RSSI and squelch observations may coalesce. Runtime state/selected settings
/// and all controller/PTT events keep their normal immediate queuing semantics.
fn radio_controls_equal(a: RadioView, b: RadioView) -> bool {
    a.state == b.state
        && a.signaling_popup_damage(b).is_none()
        && a.tx_slot == b.tx_slot
        && a.receiving_slot() == b.receiving_slot()
        && a.tx_trial == b.tx_trial
        && a.power == b.power
        && a.modulation == b.modulation
        && a.vfo_am_override == b.vfo_am_override
        && a.route == b.route
        && a.monitor == b.monitor
}

fn normalize_input(matrix: u16, ptt_pressed: bool, aux: Option<Key>) -> Sample {
    match (classify_matrix(matrix), ptt_pressed, aux) {
        (MatrixState::Released, false, None) => Sample::Released,
        (MatrixState::Released, true, None) => Sample::Key(Key::Ptt),
        (MatrixState::Single(key), false, None) => Sample::Key(key),
        (MatrixState::Released, false, Some(key @ (Key::Aux16 | Key::Aux17 | Key::Aux18))) => {
            Sample::Key(key)
        }
        _ => Sample::Invalid,
    }
}

fn zone_header_valid(zone: Zone<'_>) -> bool {
    let counts = zone.cps_count_bytes();
    counts[0] == counts[1]
        && counts[0] <= MEMBER_COUNT as u8
        && (counts[0] != 0
            || zone
                .name_raw()
                .first()
                .is_some_and(|byte| !matches!(*byte, 0 | 0xff)))
}

fn collect_zone_members(
    zone: Zone<'_>,
    all: &[u8; 128],
    order: &mut [u16; MEMBER_COUNT],
    bitmap: &mut [u8; 128],
) -> usize {
    bitmap.fill(0);
    // A stock empty record may have 200 zero words, not 200 channel-zero links.
    if zone.cps_count_bytes()[0] == 0 {
        return 0;
    }
    let mut len = 0;
    for slot in 0..MEMBER_COUNT {
        let index = zone.member_code(slot).expect("bounded member slot");
        if !controller::channel_active(all, index) || controller::channel_active(bitmap, index) {
            continue;
        }
        order[len] = index;
        len += 1;
        bitmap[index as usize / 8] |= 1 << (index % 8);
    }
    len
}

fn zone_views_from_parts<'a>(
    window: &[Option<u8>; 7],
    names: &'a [[u8; 16]; ZONE_COUNT],
    counts: &[u16; ZONE_COUNT],
    all_count: u16,
) -> [ZoneView<'a>; 7] {
    core::array::from_fn(|index| {
        let zone = window[index];
        ZoneView {
            index: zone,
            name: zone.map_or(&[][..], |zone| &names[zone as usize][..]),
            channel_count: zone.map_or(all_count, |zone| counts[zone as usize]),
        }
    })
}

/// Stable cable-log key identifiers, independent of Rust enum discriminants.
pub const fn key_code(key: Key) -> u32 {
    match key {
        Key::Digit0 => 0,
        Key::Digit1 => 1,
        Key::Digit2 => 2,
        Key::Digit3 => 3,
        Key::Digit4 => 4,
        Key::Digit5 => 5,
        Key::Digit6 => 6,
        Key::Digit7 => 7,
        Key::Digit8 => 8,
        Key::Digit9 => 9,
        Key::Menu => 10,
        Key::Up => 11,
        Key::Down => 12,
        Key::Exit => 13,
        Key::Star => 14,
        Key::Hash => 15,
        Key::Ptt => 16,
        Key::Aux16 => 17,
        Key::Aux17 => 18,
        Key::Aux18 => 19,
    }
}

#[cfg(test)]
extern crate self as uv98_handheld_app;

#[cfg(test)]
mod tests;
