//! Connect the timed scan scheduler to operator requests and the channel catalog.
//! The foreground still owns physical input sampling and RF tuning.

use uv98_display::{Delay, DisplayBus};
use uv98_handheld_app::App;
use uv98_handheld_ui::controller::{Request, ScanState, SideAction};
use uv98_nor_read::Bus;
use uv98_radio_runtime::{policy, ptt, scan, State as RadioState};

pub use scan::Action;

type UiError<N, L> = uv98_handheld_app::Error<<N as Bus>::Error, <L as DisplayBus>::Error>;

/// The sole clean side input bound to Scan is the scan command itself. A
/// physical PTT, chord or another side input still cancels before any tune.
pub fn cancels_scan(input: ptt::Input, adc: u16, bindings: [SideAction; 2]) -> bool {
    if input.clean
        && (input.upper ^ input.lower)
        && bindings[usize::from(input.lower)] == SideAction::Scan
    {
        return false;
    }
    input.upper || input.lower || !ptt::side_idle(adc)
}

pub struct Scanner {
    scheduler: scan::Scheduler,
    zone_order: Option<scan::ZoneOrder>,
    ignore_current: bool,
}

impl Scanner {
    pub const fn new() -> Self {
        Self {
            scheduler: scan::Scheduler::new(),
            zone_order: None,
            ignore_current: false,
        }
    }

    pub fn active(&self) -> bool {
        self.scheduler.active()
    }

    pub fn paused(&self) -> bool {
        self.scheduler.paused()
    }

    pub fn awaiting_tune(&self) -> bool {
        self.scheduler.status().phase == scan::Phase::AwaitingTune
    }

    pub fn start<N: Bus, L: DisplayBus, D: Delay>(
        &mut self,
        app: &mut App<N, L, D>,
        now: u32,
    ) -> Result<Action, UiError<N, L>> {
        let view = app.view_ref();
        self.zone_order = app
            .scan_zone_priority()
            .map(|priority| scan::ZoneOrder::new(priority, view.channel));
        if let Some(action) = policy::scan_target_with_range(view, app.scan_range())
            .ok()
            .and_then(|target| self.scheduler.start(target, now).ok())
        {
            Ok(action)
        } else {
            app.reject_radio_request(Request::StartScan)?;
            Ok(Action::None)
        }
    }

    pub fn stop<N: Bus, L: DisplayBus, D: Delay>(
        &mut self,
        app: &mut App<N, L, D>,
    ) -> Result<Action, UiError<N, L>> {
        let action = self.scheduler.cancel();
        app.acknowledge_scan_stopped()?;
        Ok(action)
    }

    #[inline(never)]
    pub fn cancel<N: Bus, L: DisplayBus, D: Delay>(
        &mut self,
        app: &mut App<N, L, D>,
    ) -> Result<Action, UiError<N, L>> {
        let action = self.scheduler.cancel();
        app.cancel_scan()?;
        app.acknowledge_scan_stopped()?;
        Ok(action)
    }

    fn reject<N: Bus, L: DisplayBus, D: Delay>(
        &mut self,
        app: &mut App<N, L, D>,
    ) -> Result<(), UiError<N, L>> {
        self.cancel(app)?;
        app.reject_radio_request(Request::StartScan)
    }

    /// Cancel on raw side input before honoring skip. Sample the scan clock only if a
    /// paused session actually reaches the skip path.
    pub fn finish_input<N: Bus, L: DisplayBus, D: Delay>(
        &mut self,
        app: &mut App<N, L, D>,
        mut action: Action,
        cancel_input: bool,
        skip: bool,
        sample_time: impl FnOnce() -> u32,
    ) -> Result<bool, UiError<N, L>> {
        if self.active()
            && (cancel_input
                || matches!(
                    app.view_ref().scan,
                    ScanState::Stopped | ScanState::Stopping
                ))
        {
            action = self.cancel(app)?;
        } else if skip && self.paused() {
            action = self
                .scheduler
                .skip_current(sample_time(), app.view_ref().channel)
                .unwrap_or(Action::Stop);
        }
        if action == Action::None {
            return Ok(false);
        }
        let retune = self.apply(app, action, skip)?;
        if !retune && action != Action::Stop {
            self.scheduler.cancel();
        }
        Ok(retune)
    }

    /// Feed the confirmed RF state to the scheduler. A selected target requires
    /// the foreground to stop RX before it publishes the new pause state.
    pub fn advance<N: Bus, L: DisplayBus, D: Delay>(
        &mut self,
        app: &mut App<N, L, D>,
        now: u32,
        radio: RadioState,
        audio_open: bool,
        sample: Option<scan::FindSample>,
    ) -> Result<bool, UiError<N, L>> {
        if let Some(ticket) = self.scheduler.status().ticket {
            match self.scheduler.tune_ack(ticket, now, radio) {
                Ok(true) => {
                    app.acknowledge_scan_started()?;
                }
                Ok(false) => {}
                Err(_) => self.reject(app)?,
            }
        }
        if let Some(sample) = sample {
            let hz = sample.frequency_10hz;
            self.ignore_current = audio_open && app.scan_frequency_ignored(hz);
            if let Some((raw, opened)) = self.scheduler.observe_find(sample) {
                app.record_scan_find(hz, raw, opened, now);
            }
        }
        match self.scheduler.tick_with_cursor_hold(
            now,
            audio_open && !self.ignore_current,
            app.scan_cursor_held(),
        ) {
            Ok(Action::None) => {}
            Ok(action) => {
                if self.apply(app, action, false)? {
                    return Ok(true);
                }
                self.scheduler.cancel();
            }
            Err(_) => self.reject(app)?,
        }
        Ok(false)
    }

    #[inline(never)]
    fn apply<N: Bus, L: DisplayBus, D: Delay>(
        &mut self,
        app: &mut App<N, L, D>,
        action: Action,
        force_ordinary: bool,
    ) -> Result<bool, UiError<N, L>> {
        self.ignore_current = false;
        let action = self
            .scheduler
            .omit_marked_vfo(action, |hz| app.scan_frequency_ignored(hz));
        match action {
            Action::NextChannel => {
                let skipped = self.scheduler.skipped_channels();
                let target = if let Some(order) = &mut self.zone_order {
                    order.next(
                        app.view_ref().channel,
                        skipped,
                        force_ordinary,
                        |cursor, priority| app.next_scan_channel_after(cursor, skipped, priority),
                    )
                } else {
                    app.next_scan_channel(skipped)
                        .filter(|&channel| Some(channel) != app.view_ref().channel)
                };
                if let Some(index) = target {
                    app.select_scan_channel(index)?;
                    Ok(true)
                } else {
                    if self.zone_order.is_none() && app.view_ref().scan == ScanState::Requested {
                        app.reject_radio_request(Request::StartScan)?;
                    } else {
                        app.acknowledge_scan_stopped()?;
                    }
                    Ok(false)
                }
            }
            Action::TuneFrequency(frequency) => {
                app.select_scan_frequency(frequency)?;
                Ok(true)
            }
            Action::Stop => {
                app.acknowledge_scan_stopped()?;
                Ok(false)
            }
            Action::None => Ok(false),
        }
    }
}
