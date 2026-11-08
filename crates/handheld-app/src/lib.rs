#![no_std]
#![forbid(unsafe_code)]
//! Live UI composition. NOR READ03 only; frequencies are local UI values.

use sha2::{Digest, Sha256};
use uv98_codeplug::{
    channels::{Channel, CHANNEL_COUNT, CHANNEL_LEN},
    zones::{Zone, MEMBER_COUNT, RECORD_LEN, ZONE_COUNT},
};
pub use uv98_diag_app::{StartStage, PRESERVED_CALIBRATION_SHA256};
use uv98_diagnostic::{Diagnostic, LoadError, Snapshot, CHANNELS_ADDRESS};
use uv98_display::{Delay, Display, DisplayBus};
use uv98_handheld_ui::{
    controller::{self, Change, Controller, LightMode, Mode, Request, Screen, Slot, View},
    input::{InputEvent, InputTracker, Sample},
    render::{
        self, ChannelView, Model, PeerView, RadioView, SpectrumState, SpectrumView, ZoneView,
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
    pub start_10hz: u32,
    pub step_10hz: u32,
    pub len: u8,
    pub plot: uv98_spectrum::compact::Plot,
}
impl SpectrumSnapshot {
    pub fn view(&self) -> SpectrumView<'_> {
        SpectrumView {
            state: self.state,
            start_10hz: self.start_10hz,
            step_10hz: self.step_10hz,
            len: self.len,
            plot: &self.plot,
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

// Only the 20-row radio/PTT banner is eligible for priority. Ordinary page
// work receives a batch after at most eight priority rows, even under repeated
// state changes. Telemetry never enters this queue.
const PRIORITY_BANNER_START: u16 = 28;
const PRIORITY_BANNER_END: u16 = 48;
const PRIORITY_BURST_ROWS: u16 = 8;
const PRIORITY_BANNER_MASK: u32 = (1 << (PRIORITY_BANNER_END - PRIORITY_BANNER_START)) - 1;

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
    priority_banner: u32,
    priority_run: u16,
    row_cursor: u16,
    render_page: Option<Screen>,
    radio: RadioView,
    last_radio: RadioView,
    spectrum: Option<SpectrumSnapshot>,
    last_spectrum: Option<SpectrumSnapshot>,
}
impl<N: Bus, L: DisplayBus, D: Delay> App<N, L, D> {
    pub fn new(nor: N, lcd: L, delay: D, timing: Timing, expected_sha256: [u8; 32]) -> Self {
        let bitmap = [0; 128];
        let controller = Controller::new(&bitmap, 14_550_000);
        let view = controller.view();
        Self {
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
            priority_banner: 0,
            priority_run: 0,
            row_cursor: 0,
            render_page: None,
            radio: RadioView::OFF,
            last_radio: RadioView::OFF,
            spectrum: None,
            last_spectrum: None,
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
    pub fn spectrum_view(&self) -> Option<SpectrumView<'_>> {
        if self.status.state == State::Ready {
            self.spectrum.as_ref().map(SpectrumSnapshot::view)
        } else {
            None
        }
    }
    /// False means an older immutable spectrum snapshot is still being drawn.
    /// The caller keeps its newest live data and retries; no partial replacement.
    pub fn set_spectrum(
        &mut self,
        snapshot: SpectrumSnapshot,
    ) -> Result<bool, Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
        if self.spectrum == Some(snapshot) {
            return Ok(true);
        }
        if self.status.view.screen == Screen::Spectrum
            && self.spectrum.is_some()
            && self.pending_rows.any()
        {
            return Ok(false);
        }
        self.spectrum = Some(snapshot);
        self.redraw_ready()?;
        Ok(true)
    }
    pub fn clear_spectrum(&mut self) -> Result<(), Error<N::Error, L::Error>> {
        if self.status.state != State::Ready {
            return Err(Error::State);
        }
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
        self.radio = radio;
        self.redraw_ready()
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
            InputSample::Unstable => None,
        };
        let normalized = match sample {
            InputSample::Unstable => Sample::Invalid,
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
        let change = self.controller.handle(event, &self.bitmap);
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
        self.status.view = self.controller.view();
        if change.channel_changed || change.slot_changed || change.zone_changed {
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
            if (change.slot_changed || change.zone_changed)
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
        self.pending_rows.any()
    }

    /// At most max_rows contiguous rows are sent. No NOR read or input polling
    /// is hidden in this call; the foreground must sample keys before each step.
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
                pending: self.pending_rows.any(),
            });
        }
        // Exclude queued priority bits from the ordinary cursor search. This
        // makes the fairness batch useful even if it would otherwise land on
        // the same repeatedly invalidated banner. The normal cursor is not
        // advanced by priority work.
        let mut ordinary = self.pending_rows;
        for row in PRIORITY_BANNER_START..PRIORITY_BANNER_END {
            if self.priority_banner & (1 << (row - PRIORITY_BANNER_START)) != 0 {
                ordinary.clear(row, row + 1);
            }
        }
        let normal_start = ordinary.next(self.row_cursor);
        let priority = self.priority_banner != 0
            && (normal_start.is_none() || self.priority_run < PRIORITY_BURST_ROWS);
        let (start, limit) = if priority {
            let start = PRIORITY_BANNER_START + self.priority_banner.trailing_zeros() as u16;
            let limit = if normal_start.is_some() {
                max_rows.min(PRIORITY_BURST_ROWS - self.priority_run)
            } else {
                max_rows
            };
            (Some(start), limit)
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
                end < PRIORITY_BANNER_END
                    && self.priority_banner & (1 << (end - PRIORITY_BANNER_START)) != 0
            } else {
                ordinary.has(end)
            }
        {
            end += 1;
        }
        self.status.state = State::Failed;
        self.paint_rows(start, end)?;
        self.pending_rows.clear(start, end);
        for row in start.max(PRIORITY_BANNER_START)..end.min(PRIORITY_BANNER_END) {
            self.priority_banner &= !(1 << (row - PRIORITY_BANNER_START));
        }
        if priority {
            self.priority_run = self.priority_run.saturating_add(end - start);
        } else {
            self.priority_run = 0;
            self.row_cursor = if end == render::HEIGHT { 0 } else { end };
        }
        if self.priority_banner == 0 {
            self.priority_run = 0;
        }
        if !self.pending_rows.any() {
            // One combined follow-up captures telemetry that changed while the
            // pass was rendering, including a value that changed and changed back.
            self.pending_rows.merge_take(&mut self.deferred_radio_rows);
        }
        self.status.state = State::Ready;
        Ok(RenderProgress {
            rows: end - start,
            pending: self.pending_rows.any(),
        })
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
        let slot = if controller.view().active_slot == Slot::A {
            Slot::B
        } else {
            Slot::A
        };
        let context = controller.context(slot);
        let frequency_10hz = if context.mode == Mode::Vfo {
            Some(context.vfo_10hz)
        } else {
            peer_record
                .filter(|record| Some(record.index as u16) == context.channel)
                .map(|record| {
                    let channel = record.view();
                    if context.reverse {
                        channel.tx_10hz
                    } else {
                        channel.rx_10hz
                    }
                })
        };
        PeerView {
            slot,
            mode: context.mode,
            frequency_10hz,
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
            reverse: context.reverse,
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
        // Radio-only reports cannot change a channel/zone window. Avoid the
        // catalog walks on every RSSI observation, especially in channel lists.
        if self.last_rendered != Some(self.status.view) {
            self.refresh_list()?;
            self.refresh_zones();
        }
        let page_changed = self.render_page != Some(self.status.view.screen);
        if page_changed {
            // These bitmaps describe work not yet on the physical panel, not
            // an old framebuffer. A partial page transition preserves chrome;
            // dropping prior lock/banner/startup damage could leave stale
            // pixels forever because last_* describes the last planned model.
            // Keep that damage and paint every row from the newest model.
            self.pending_rows.merge_take(&mut self.deferred_radio_rows);
            self.priority_banner = 0;
            self.priority_run = 0;
            self.row_cursor = 0;
            self.render_page = Some(self.status.view.screen);
        }
        if self.last_rendered.is_none() && !self.pending_rows.any() {
            self.row_cursor = 0;
        }
        let defer_telemetry =
            self.pending_rows.any() && radio_controls_equal(self.last_radio, self.radio);
        let snapshot = self
            .diagnostic
            .snapshot()
            .filter(|snapshot| Some(snapshot.index() as u16) == self.status.view.channel);
        let channel = snapshot.as_ref().map(|s| ChannelView {
            index: s.index(),
            name: s.channel.name_raw(),
            rx_10hz: s.channel.rx_frequency_code(),
            tx_10hz: s.channel.tx_frequency_code(),
            active: s.channel.is_active(),
            rx_tone_raw: s.channel.rx_tone_raw(),
            tx_tone_raw: s.channel.tx_tone_raw(),
            bandwidth_raw: s.channel.bandwidth_raw(),
            power_raw: s.channel.power_raw(),
        });
        let list_views: [ChannelView<'_>; 7] = core::array::from_fn(|i| self.list[i].view());
        let zone_views = zone_views(
            &self.zone_window,
            &self.zone_names,
            &self.zone_counts,
            controller::active_count(&self.all_bitmap) as u16,
        );
        let peer = self.peer_view();
        let model = Model {
            view: self.status.view,
            channel,
            list: &list_views[..self.list_len],
            calibration_matches: self.status.calibration_matches_backup,
            last_key: self.status.last_pressed,
            ptt_pressed: self.status.view.ptt_down,
            input_ready: self.status.input_ready,
            radio: self.radio,
            zone_name: self
                .status
                .view
                .zone
                .map(|zone| &self.zone_names[zone as usize][..]),
            zones: &zone_views[..self.zone_window_len],
            peer: Some(peer),
            spectrum: self.spectrum.as_ref().map(SpectrumSnapshot::view),
        };
        let damage = render::dirty_ranges(&model, self.last_rendered)
            .map_err(|e| Error::Render(render::RenderError::Model(e)))?;
        let radio_damage = render::radio_dirty_ranges(&model, self.last_radio)
            .map_err(|e| Error::Render(render::RenderError::Model(e)))?;
        let spectrum_damage = render::spectrum_dirty_ranges(
            &model,
            self.last_spectrum.as_ref().map(SpectrumSnapshot::view),
        )
        .map_err(|e| Error::Render(render::RenderError::Model(e)))?;
        for &(start, end) in damage.ranges() {
            self.pending_rows.mark(start, end);
        }
        for &(start, end) in spectrum_damage.ranges() {
            self.pending_rows.mark(start, end);
        }
        if defer_telemetry {
            for &(start, end) in radio_damage.ranges() {
                self.deferred_radio_rows.mark(start, end);
            }
        } else {
            // State, power, modulation and monitor changes are never deferred.
            self.pending_rows.merge_take(&mut self.deferred_radio_rows);
            for &(start, end) in radio_damage.ranges() {
                self.pending_rows.mark(start, end);
            }
        }
        if self.last_radio.state != self.radio.state
            || self
                .last_rendered
                .is_some_and(|old| old.ptt_down != self.status.view.ptt_down)
        {
            self.pending_rows
                .mark(PRIORITY_BANNER_START, PRIORITY_BANNER_END);
            self.priority_banner = PRIORITY_BANNER_MASK;
        }
        self.last_rendered = Some(self.status.view);
        self.last_radio = self.radio;
        self.last_spectrum = self.spectrum;
        Ok(())
    }

    fn paint_rows(&mut self, start: u16, end: u16) -> Result<(), Error<N::Error, L::Error>> {
        let snapshot = self
            .diagnostic
            .snapshot()
            .filter(|snapshot| Some(snapshot.index() as u16) == self.status.view.channel);
        let channel = snapshot.as_ref().map(|s| ChannelView {
            index: s.index(),
            name: s.channel.name_raw(),
            rx_10hz: s.channel.rx_frequency_code(),
            tx_10hz: s.channel.tx_frequency_code(),
            active: s.channel.is_active(),
            rx_tone_raw: s.channel.rx_tone_raw(),
            tx_tone_raw: s.channel.tx_tone_raw(),
            bandwidth_raw: s.channel.bandwidth_raw(),
            power_raw: s.channel.power_raw(),
        });
        let list_views: [ChannelView<'_>; 7] = core::array::from_fn(|i| self.list[i].view());
        let zone_views = zone_views(
            &self.zone_window,
            &self.zone_names,
            &self.zone_counts,
            controller::active_count(&self.all_bitmap) as u16,
        );
        let peer = Self::peer_view_from_parts(
            &self.controller,
            self.peer_record.as_ref(),
            &self.zone_names,
        );
        let model = Model {
            view: self.status.view,
            channel,
            list: &list_views[..self.list_len],
            calibration_matches: self.status.calibration_matches_backup,
            last_key: self.status.last_pressed,
            ptt_pressed: self.status.view.ptt_down,
            input_ready: self.status.input_ready,
            radio: self.radio,
            zone_name: self
                .status
                .view
                .zone
                .map(|zone| &self.zone_names[zone as usize][..]),
            zones: &zone_views[..self.zone_window_len],
            peer: Some(peer),
            spectrum: self.spectrum.as_ref().map(SpectrumSnapshot::view),
        };
        render::render_rows(&mut self.display, &model, start, end).map_err(Error::Render)
    }
}

/// RSSI and squelch observations may coalesce. Runtime state/selected settings
/// and all controller/PTT events keep their normal immediate queuing semantics.
fn radio_controls_equal(a: RadioView, b: RadioView) -> bool {
    a.state == b.state
        && a.tx_trial == b.tx_trial
        && a.power == b.power
        && a.modulation == b.modulation
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

fn zone_views<'a>(
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
mod tests;
