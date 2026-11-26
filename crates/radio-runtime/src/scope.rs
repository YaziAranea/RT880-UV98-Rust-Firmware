//! Integration of the compact sweep protocol and the qualified stock RF
//! fragment. Preparation and restoration are complete normal RX plans.
//! This adapter has no transmit operation and never acknowledges work by time
//! alone. The caller continues Runtime::tick and supplies a fresh clock.

use core::num::NonZeroU32;

use crate::spectrum_flow::AfterListen;
use crate::{Error, Fault, Io, Runtime, State};
use uv98_calibration::Calibration;
use uv98_handheld_ui::controller::SpectrumModulation;
use uv98_rf_sequence::{self as sequence, Modulation, RxRequest, Width};
use uv98_spectrum::{compact, Range, Ticket, Timing, Window, WindowError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartError {
    NotReceiving,
    Busy,
    Grid,
    /// The requested window is invalid or does not fit the compact engine.
    Window(WindowError),
    /// A sweep must stay centred on the receive plan it prepared.
    CenterMismatch,
    Qualification(sequence::sweep::Error),
    Receive(sequence::Error),
    Protocol(compact::StartError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    None,
    /// `request` is the complete normal RX plan whose completion the runtime
    /// acknowledged. The caller may cache it as applied, but only after
    /// comparing it with the policy it still wants; the frequency alone does
    /// not describe width, tone, option or squelch.
    Restored {
        request: RxRequest,
        reason: compact::StopReason,
    },
}

#[derive(Clone, Copy)]
enum Waiting {
    Prepare(Ticket),
    Restore {
        ticket: Ticket,
        request: RxRequest,
        reason: compact::StopReason,
    },
}

/// Bounded temporary listening on one measured bin. The adapter keeps the
/// original receive plan for the return and never commits UI context.
pub const LISTEN_TIMEOUT_MS: u32 = 15_000;
const INTERLEAVED_LISTEN_MS: u32 = 1_000;
pub(crate) const LISTEN_SAMPLE_INTERVAL_MS: u32 = 125;

#[derive(Clone, Copy)]
enum Listen {
    Active {
        started: u32,
        sampled_at: u32,
        bin: u8,
        automatic: bool,
    },
    /// Original RX plan is already submitted; wait for its full completion.
    Restoring { original: RxRequest },
}

pub struct Scope {
    engine: compact::Engine,
    completed_plot: compact::Plot,
    trigger: Option<u16>,
    trigger_listens: bool,
    skipped_bin: Option<u8>,
    preferred_bin: Option<u8>,
    sweep_started_at: u32,
    qualified: Option<sequence::sweep::QualifiedRange>,
    original: Option<RxRequest>,
    window: Option<Window>,
    generation: u32,
    listen_modulation: Option<SpectrumModulation>,
    listen: Option<Listen>,
    pending: Option<compact::Action>,
    waiting: Option<Waiting>,
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Scope {
    pub const fn new() -> Self {
        Self {
            engine: compact::Engine::new(),
            completed_plot: compact::Plot::EMPTY,
            trigger: None,
            trigger_listens: false,
            skipped_bin: None,
            preferred_bin: None,
            sweep_started_at: 0,
            qualified: None,
            original: None,
            window: None,
            generation: 0,
            listen_modulation: None,
            listen: None,
            pending: None,
            waiting: None,
        }
    }
    pub fn active(&self) -> bool {
        self.engine.running()
            || self.engine.restoring()
            || self.listen.is_some()
            || matches!(self.pending, Some(compact::Action::Terminal { .. }))
    }
    pub fn listening(&self) -> bool {
        matches!(self.listen, Some(Listen::Active { .. }))
    }
    pub fn listen_restoring(&self) -> bool {
        matches!(self.listen, Some(Listen::Restoring { .. }))
    }

    /// The RF-confirmed bin remains bound until its original RX is restored.
    pub fn listened_bin(&self) -> Option<u8> {
        if let (Some(_), compact::State::Restored(compact::StopReason::Accepted { index })) =
            (self.listen, self.engine.state())
        {
            Some(index)
        } else {
            None
        }
    }
    /// A bin action is accepted only while the compact engine is actually
    /// sweeping. During Restoring an `Accept` is ignored by the engine, so
    /// accepting a new UI intent then would let an old completion drive it.
    pub fn can_accept_bin_action(&self) -> bool {
        self.engine.running() && self.listen.is_none()
    }

    /// Snapshot the temporary demodulator when the engine accepts this bin.
    pub fn listen_bin(&mut self, now: u32, bin: u8, modulation: SpectrumModulation) -> bool {
        if !self.can_accept_bin_action() {
            return false;
        }
        self.event(now, compact::Event::Accept { index: bin });
        if self.engine.state()
            == compact::State::Restoring(compact::StopReason::Accepted { index: bin })
        {
            self.listen_modulation = Some(modulation);
            true
        } else {
            false
        }
    }

    /// Ordinary VFO acceptance uses the saved receiver's policy, even when
    /// the active Listen used a different demodulator.
    pub fn vfo_modulation(&self, frequency_10hz: u32) -> Modulation {
        if crate::policy::aviation_band(frequency_10hz) {
            Modulation::Am
        } else if let Some(original) = self.original {
            if crate::policy::aviation_band(original.frequency_10hz) {
                Modulation::Fm
            } else {
                original.modulation
            }
        } else {
            Modulation::Fm
        }
    }

    /// Restart may cancel only a running sweep, never an in-flight restore.
    pub fn can_restart(&self) -> bool {
        self.engine.running() && self.listen.is_none()
    }
    /// Time the audio interval from confirmed RX, using the wrapping clock.
    pub fn listen_due(&self, now: u32, interleaved: bool) -> Option<AfterListen> {
        let Some(Listen::Active {
            started,
            bin,
            automatic,
            ..
        }) = self.listen
        else {
            return None;
        };
        let elapsed = now.wrapping_sub(started);
        if automatic && interleaved && elapsed >= INTERLEAVED_LISTEN_MS {
            Some(AfterListen::Refresh(bin))
        } else {
            (elapsed >= LISTEN_TIMEOUT_MS).then_some(AfterListen::Resume)
        }
    }
    /// Only a confirmed restore to a selected bin can enter temporary listening.
    pub fn begin_listen(&mut self, now: u32, automatic: bool) -> bool {
        let compact::State::Restored(compact::StopReason::Accepted { index }) = self.engine.state()
        else {
            return false;
        };
        if self.listen.is_some() || self.original.is_none() {
            return false;
        }
        self.listen = Some(Listen::Active {
            started: now,
            sampled_at: now.wrapping_sub(LISTEN_SAMPLE_INTERVAL_MS),
            bin: index,
            automatic,
        });
        true
    }

    /// Use the ordinary RX observation of the accepted frequency. Throttle
    /// publication without adding RF transactions or changing the wide sweep.
    pub fn observe_listen_rssi(&mut self, now: u32, frequency_10hz: u32, raw: u16) {
        let Some(Listen::Active {
            started,
            sampled_at,
            bin,
            automatic,
        }) = self.listen
        else {
            return;
        };
        if now.wrapping_sub(started) >= LISTEN_TIMEOUT_MS
            || now.wrapping_sub(sampled_at) < LISTEN_SAMPLE_INTERVAL_MS
            || raw > compact::MAX_RAW
            || self
                .range()
                .and_then(|r| r.frequency_10hz(usize::from(bin)))
                != Some(frequency_10hz)
        {
            return;
        }
        let sequence = self
            .completed_plot
            .held_sample
            .and_then(|sample| sample.sequence.checked_add(1))
            .unwrap_or(NonZeroU32::MIN);
        self.completed_plot.held_sample = Some(compact::HeldSample { sequence, bin, raw });
        self.listen = Some(Listen::Active {
            started,
            sampled_at: now,
            bin,
            automatic,
        });
    }

    /// Submit the original full RX plan once, then report completion exactly
    /// when the runtime is Receiving again.
    pub fn end_listen<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        calibration: &Calibration<'_>,
        now: u32,
    ) -> Result<Option<RxRequest>, Fault<I::Error>> {
        match self.listen {
            Some(Listen::Active { .. }) => {
                let original = self.original.ok_or_else(|| radio.fail(Error::State))?;
                radio.stop()?;
                radio.receive(calibration, original, now)?;
                self.completed_plot.held_sample = None;
                self.listen = Some(Listen::Restoring { original });
                Ok(None)
            }
            Some(Listen::Restoring { original }) => {
                if radio.state() == State::Receiving {
                    self.listen = None;
                    Ok(Some(original))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
    /// Drop stale range/plot/window after a refused start so the UI cannot
    /// present previous measurements as the new window.
    #[inline(never)]
    pub fn discard(&mut self) {
        self.engine = compact::Engine::new();
        self.completed_plot = compact::Plot::EMPTY;
        self.skipped_bin = None;
        self.preferred_bin = None;
        self.sweep_started_at = 0;
        self.qualified = None;
        self.original = None;
        self.window = None;
        self.listen_modulation = None;
        self.listen = None;
        self.pending = None;
        self.waiting = None;
    }
    pub fn state(&self) -> compact::State {
        self.engine.state()
    }
    pub fn plot(&self) -> &compact::Plot {
        self.engine.plot()
    }
    pub fn completed_plot(&self) -> &compact::Plot {
        &self.completed_plot
    }
    pub fn range(&self) -> Option<Range> {
        self.engine.config().map(|c| c.range)
    }
    pub fn window(&self) -> Option<Window> {
        self.window
    }
    /// Identity of the current or last started window. Increments only after a
    /// successful start, so every Restart invalidates older published grids.
    pub const fn generation(&self) -> u32 {
        self.generation
    }

    /// Backwards-compatible entry used by existing callers: 81 points around
    /// the original frequency, spaced by the ordinary VFO step.
    pub fn start<I: Io>(
        &mut self,
        radio: &Runtime<I>,
        calibration: &Calibration<'_>,
        original: RxRequest,
        step_10hz: u32,
        now: u32,
    ) -> Result<(), StartError> {
        let span = step_10hz.checked_mul(80).ok_or(StartError::Grid)?;
        let window = Window::centered(original.frequency_10hz, span, step_10hz)
            .map_err(StartError::Window)?;
        self.start_window(radio, calibration, original, window, now)
    }

    /// Only validation and protocol state change occur here; bad windows leave
    /// the working receiver alone. The window must be centred on `original` and
    /// every point must share its qualified band/route/factory offset.
    pub fn start_window<I: Io>(
        &mut self,
        radio: &Runtime<I>,
        calibration: &Calibration<'_>,
        original: RxRequest,
        window: Window,
        now: u32,
    ) -> Result<(), StartError> {
        if self.active() {
            return Err(StartError::Busy);
        }
        if radio.state() != State::Receiving {
            return Err(StartError::NotReceiving);
        }
        if window.center_10hz() != original.frequency_10hz {
            return Err(StartError::CenterMismatch);
        }
        self.start_panned_window(radio, calibration, original, window, now)
    }

    pub fn qualify_window(
        calibration: &Calibration<'_>,
        original: RxRequest,
        window: Window,
    ) -> Result<sequence::sweep::QualifiedRange, StartError> {
        let range = window.range();
        let mut prepare = original;
        prepare.frequency_10hz = window.center_10hz();
        sequence::receive(calibration, prepare).map_err(StartError::Receive)?;
        sequence::receive(calibration, original).map_err(StartError::Receive)?;
        sequence::sweep::QualifiedRange::new(
            calibration,
            prepare.frequency_10hz,
            range.start_10hz(),
            range.end_10hz(),
            range.step_10hz(),
        )
        .map_err(StartError::Qualification)
    }

    /// Prepare at the viewport center and retain the original receiver for EXIT.
    pub fn start_panned_window<I: Io>(
        &mut self,
        radio: &Runtime<I>,
        calibration: &Calibration<'_>,
        original: RxRequest,
        window: Window,
        now: u32,
    ) -> Result<(), StartError> {
        if self.active() {
            return Err(StartError::Busy);
        }
        if radio.state() != State::Receiving {
            return Err(StartError::NotReceiving);
        }
        let range = window.range();
        let qualified = Self::qualify_window(calibration, original, window)?;
        let timing =
            Timing::new(3000, 1000, crate::sweep::SETTLE_MS, 1000).map_err(|_| StartError::Grid)?;
        let config = compact::Config::new(range, original.frequency_10hz, timing, 3000)
            .map_err(|_| StartError::Grid)?;
        let action = self
            .engine
            .start(now, config)
            .map_err(StartError::Protocol)?;
        self.completed_plot = compact::Plot::EMPTY;
        self.skipped_bin = None;
        self.preferred_bin = None;
        self.sweep_started_at = now;
        self.qualified = Some(qualified);
        self.original = Some(original);
        self.window = Some(window);
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.generation = 1;
        }
        self.waiting = None;
        self.listen_modulation = None;
        self.pending = Some(action);
        Ok(())
    }

    /// Repeat the qualified grid after restoring the original RX plan. Keep
    /// the last complete plot visible until real samples replace it.
    pub fn restart_held_window(&mut self, now: u32, bin: u8, skip: bool) -> Result<(), StartError> {
        let reason = if skip {
            compact::StopReason::Exit
        } else {
            compact::StopReason::Accepted { index: bin }
        };
        if self.listen.is_some() || self.engine.state() != compact::State::Restored(reason) {
            return Err(StartError::Busy);
        }
        let config = self.engine.config().ok_or(StartError::Grid)?;
        if usize::from(bin) >= config.range.len() {
            return Err(StartError::Grid);
        }
        let action = self
            .engine
            .start(now, config)
            .map_err(StartError::Protocol)?;
        self.completed_plot.held_bin = None;
        self.preferred_bin = (!skip).then_some(bin);
        if skip {
            self.skipped_bin = Some(bin);
        }
        self.pending = Some(action);
        Ok(())
    }

    pub fn set_trigger(&mut self, trigger: Option<u16>, listen: bool) {
        if !listen || trigger.is_none() {
            self.completed_plot.held_bin = None;
        }
        self.trigger = trigger;
        self.trigger_listens = listen;
    }

    pub fn resume(&mut self) {
        let preferred = self.preferred_bin.take();
        if let Some(bin) = self.completed_plot.held_bin.take().or(preferred) {
            self.skipped_bin = Some(bin);
        }
    }

    pub fn suppress_bin_until_below(&mut self, bin: u8) {
        if self
            .range()
            .is_some_and(|range| usize::from(bin) < range.len())
        {
            self.skipped_bin = Some(bin);
        }
    }

    fn finish_plot(&mut self, now: u32) {
        let mut plot = *self.engine.plot();
        let len = self
            .window
            .expect("active scope has a window")
            .range()
            .len();
        let sweep = self.completed_plot.sweep.wrapping_add(1).max(1);
        if sweep != plot.sweep {
            // Retain real same-grid peaks. ClearPeaks clears both bitmaps.
            for index in 0..len {
                if self.completed_plot.peak_valid(index) && plot.peak_valid(index) {
                    plot.peaks[index] = plot.peaks[index].max(self.completed_plot.peaks[index]);
                }
            }
        }
        plot.sweep = sweep;
        plot.elapsed_ms = now
            .wrapping_sub(self.sweep_started_at)
            .min(u32::from(u16::MAX)) as u16;
        // Use the lower quartile so a strong signal or one low bin cannot
        // stretch the scale. Limit changes to four counts per complete sweep.
        let floor = lower_quartile(&plot.latest[..len]);
        let target = floor.saturating_sub(8).min(447) & !3;
        plot.display_floor = Some(
            self.completed_plot
                .display_floor
                .map_or(target, |previous| {
                    target.clamp(previous.saturating_sub(4), previous + 4)
                }),
        );
        let preferred = self.preferred_bin.take().filter(|_| self.trigger_listens);
        if let Some(delta) = self.trigger {
            let threshold = floor.saturating_add(delta);
            if self
                .skipped_bin
                .is_some_and(|bin| plot.latest[usize::from(bin)] < threshold)
            {
                self.skipped_bin = None;
            }
            plot.trigger_bin = (0..len as u8)
                .filter(|&index| {
                    let i = usize::from(index);
                    (!self.trigger_listens || Some(index) != self.skipped_bin)
                        && plot.fresh(i)
                        && plot.latest[i] >= threshold
                })
                .max_by_key(|&index| {
                    let priority = u16::from(Some(index) == preferred) * (compact::MAX_RAW + 1);
                    (
                        priority + plot.latest[usize::from(index)],
                        core::cmp::Reverse(index),
                    )
                });
            if self.trigger_listens {
                plot.held_bin = plot.trigger_bin;
            }
        }
        self.completed_plot = plot;
    }

    /// Stop/PTT/accept override queued work before the next RF operation.
    /// A false PTT observation must also be forwarded after cancellation so a
    /// future sweep is not blocked by an obsolete held state.
    pub fn event(&mut self, now: u32, event: compact::Event) {
        if event == compact::Event::ClearPeaks {
            self.completed_plot.peak_valid.fill(0);
        }
        if let compact::Event::Accept { index } = event {
            if !self.engine.plot().valid(usize::from(index)) {
                return;
            }
        }
        if let Some(action) = self.engine.handle(now, event) {
            if matches!(
                action,
                compact::Action::Restore { .. } | compact::Action::Terminal { .. }
            ) {
                self.waiting = None;
            }
            self.pending = Some(action);
        }
    }

    /// At most one sweep command is performed per call. Full receive plans
    /// advance independently through Runtime::tick. `clock` must be sampled
    /// after synchronous writes, never a captured pre-transaction timestamp.
    pub fn step<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        calibration: &Calibration<'_>,
        mut clock: impl FnMut() -> u32,
    ) -> Result<Completion, Fault<I::Error>> {
        if !self.active() {
            return Ok(Completion::None);
        }
        let now = clock();
        if self.completed_plot.held_bin.is_some()
            && self.engine.state() == compact::State::SweepComplete
        {
            return Ok(Completion::None);
        }
        if let Some(waiting) = self.waiting {
            if radio.state() == State::Receiving {
                self.waiting = None;
                match waiting {
                    Waiting::Prepare(ticket) => {
                        let qualified = self.qualified.ok_or_else(|| radio.fail(Error::State))?;
                        radio.sweep_enter(qualified, now)?;
                        self.pending = self
                            .engine
                            .handle(clock(), compact::Event::Prepared(ticket));
                    }
                    Waiting::Restore {
                        ticket,
                        request,
                        reason,
                    } => {
                        self.pending = self
                            .engine
                            .handle(clock(), compact::Event::Restored(ticket));
                        if !matches!(self.engine.state(), compact::State::Restored(_)) {
                            return Err(radio.fail(Error::State));
                        }
                        return Ok(Completion::Restored { request, reason });
                    }
                }
            } else {
                self.event(now, compact::Event::Poll);
            }
        } else {
            // Check deadlines even if the caller has not yet executed a queued
            // command. Poll emits no acknowledgement and cannot replace an
            // outstanding action except with a timeout restoration/terminal.
            self.event(now, compact::Event::Poll);
        }
        let Some(action) = self.pending.take() else {
            return Ok(Completion::None);
        };
        match action {
            compact::Action::Prepare { ticket } => {
                radio.stop()?;
                let mut request = self.original.ok_or_else(|| radio.fail(Error::State))?;
                request.frequency_10hz = self
                    .window
                    .ok_or_else(|| radio.fail(Error::State))?
                    .center_10hz();
                request.tone = 0;
                request.tone_mode = 0;
                request.option_nibble = 0;
                request.level = 0;
                radio.receive(calibration, request, clock())?;
                self.waiting = Some(Waiting::Prepare(ticket));
            }
            compact::Action::Tune { ticket, index, .. } => {
                let started = clock();
                if index == 0 {
                    self.sweep_started_at = started;
                }
                radio.sweep_tune(usize::from(index), started)?;
                let completed_at = clock();
                // Establish the runtime's independent post-write settle anchor.
                if radio.sweep_read_rssi(completed_at)?.is_some() {
                    return Err(radio.fail(Error::State));
                }
                self.pending = self
                    .engine
                    .handle(completed_at, compact::Event::Tuned(ticket));
            }
            compact::Action::Read { ticket, index } => {
                if let Some(sample) = radio.sweep_read_rssi(clock())? {
                    if sample.index != usize::from(index) {
                        return Err(radio.fail(Error::State));
                    }
                    self.pending = self.engine.handle(
                        clock(),
                        compact::Event::Sample {
                            ticket,
                            raw: sample.raw,
                        },
                    );
                    // Capture before the next Poll clears freshness and starts
                    // rewriting bins. UI publication may lag several RF steps.
                    if matches!(self.pending, Some(compact::Action::SweepComplete { .. })) {
                        self.finish_plot(clock());
                    }
                } else {
                    self.pending = Some(compact::Action::Read { ticket, index });
                }
            }
            compact::Action::Restore {
                ticket,
                frequency_10hz,
                reason,
            } => {
                radio.stop()?;
                let mut request = self.original.ok_or_else(|| radio.fail(Error::State))?;
                let listen_modulation = self.listen_modulation.take();
                if matches!(reason, compact::StopReason::Accepted { .. }) {
                    request.modulation = match listen_modulation {
                        Some(mode) if mode.is_am(frequency_10hz) => Modulation::Am,
                        Some(_) => Modulation::Fm,
                        None => self.vfo_modulation(frequency_10hz),
                    };
                    request.frequency_10hz = frequency_10hz;
                    request.width = Width::Wide;
                    request.tone = 0;
                    request.tone_mode = 0;
                    request.option_nibble = 0;
                }
                radio.receive(calibration, request, clock())?;
                self.waiting = Some(Waiting::Restore {
                    ticket,
                    request,
                    reason,
                });
            }
            compact::Action::SweepComplete { .. } => {}
            compact::Action::Terminal { .. } => return Err(radio.fail(Error::State)),
        }
        Ok(Completion::None)
    }
}

// A completed sweep holds at most 81 nine-bit samples. Two histogram
// passes select the exact order statistic without sorting or copying the plot.
fn lower_quartile(levels: &[u16]) -> u16 {
    const { assert!(compact::BINS <= u8::MAX as usize && compact::MAX_RAW < 1024) };
    let mut rank = (levels.len() / 4) as u8;
    let mut prefix = 0;
    for shift in [5, 0] {
        let mut counts = [0u8; 32];
        for &level in levels {
            let part = level >> shift;
            if part >> 5 == prefix {
                counts[usize::from(part & 31)] += 1;
            }
        }
        for (digit, count) in counts.into_iter().enumerate() {
            if rank < count {
                prefix = (prefix << 5) | digit as u16;
                break;
            }
            rank -= count;
        }
    }
    prefix
}

#[cfg(test)]
mod quartile_tests {
    use super::*;

    #[test]
    fn interleaved_listen_deadline_wraps_at_one_second() {
        let started = u32::MAX - 500;
        let mut scope = Scope::new();
        scope.listen = Some(Listen::Active {
            started,
            sampled_at: started,
            bin: 3,
            automatic: true,
        });
        assert_eq!(scope.listen_due(started.wrapping_add(999), true), None);
        assert_eq!(
            scope.listen_due(started.wrapping_add(1000), true),
            Some(AfterListen::Refresh(3))
        );
        assert_eq!(scope.listen_due(started.wrapping_add(1000), false), None);
    }

    #[test]
    fn selected_floor_matches_sorted_sweeps() {
        for seed in [0, 1, 31, 32, 127, 511] {
            let mut levels = core::array::from_fn::<_, { compact::BINS }, _>(|i| {
                ((i as u16 * 7) ^ seed) % (seed + 1)
            });
            for order in 0..3 {
                if order == 1 {
                    levels.sort_unstable();
                } else if order == 2 {
                    levels.reverse();
                }
                for len in 1..=levels.len() {
                    let mut sorted = levels;
                    sorted[..len].sort_unstable();
                    assert_eq!(lower_quartile(&levels[..len]), sorted[len / 4]);
                }
            }
        }
    }
}

const _: () = assert!(core::mem::size_of::<Scope>() < 1568);
