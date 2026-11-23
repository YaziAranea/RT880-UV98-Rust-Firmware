//! One operator-authorized APRS packet on the selected calibrated FM TX path.
//! The modem owns PB7/audio only; Runtime alone owns RF, PA and antenna gates.
#[cfg(target_arch = "arm")]
use uv98_board_pins::ModemPins;
use uv98_calibration::Calibration;
use uv98_handheld_ui::controller::AprsStatus;
use uv98_radio_runtime::{policy::Profile, Fault, Io, Runtime, State};
use uv98_rf_sequence::Modulation;
#[cfg(target_arch = "arm")]
use uv98_signaling::cmx868::hardware::CBus;
use uv98_signaling::{
    aprs::{Config, Wire},
    cmx868::{Bus, Modem, Progress},
};

const REQUEST_TIMEOUT_MS: u32 = 5_000;
const RESULT_DISPLAY_MS: u32 = 2_000;
const QUIET_CHANNEL_MS: u32 = 200;
const MENU_MATRIX: u16 = 0x1000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    ModemStartup,
    RadioStartup,
    Sending,
}

pub struct Beacon<B: Bus> {
    modem: Option<Modem<B>>,
    automatic: bool,
    last_carrier_check: u32,
    config: Option<Config>,
    level: u8,
    phase: Phase,
    status: AprsStatus,
    packet: Option<Wire>,
    profile: Option<Profile>,
    requested_at: u32,
    finished_at: Option<u32>,
    menu_released: bool,
    quiet_since: Option<u32>,
    owns_rf: bool,
    finished: bool,
}

#[cfg(target_arch = "arm")]
impl Beacon<CBus> {
    /// # Safety
    /// Verified 48 MHz AT32F423, exclusive ModemPins/CRM configuration, and
    /// serialized foreground access. An unsupported PB4 strap does not create
    /// a modem or enable any RF backend.
    pub unsafe fn new(pins: ModemPins, config: Option<Config>, level: u8) -> Self {
        let modem = unsafe { CBus::new(pins) }.ok().map(Modem::new);
        Self::from_modem(modem, config, level)
    }
}

impl<B: Bus> Beacon<B> {
    pub fn from_modem(modem: Option<Modem<B>>, config: Option<Config>, level: u8) -> Self {
        let status = if config.is_none() {
            AprsStatus::Unconfigured
        } else if modem.is_none() {
            AprsStatus::Unsupported
        } else {
            AprsStatus::NoFix
        };
        Self {
            modem,
            config,
            level: level.min(7),
            phase: Phase::Idle,
            automatic: false,
            last_carrier_check: 0,
            status,
            packet: None,
            profile: None,
            requested_at: 0,
            finished_at: None,
            menu_released: false,
            quiet_since: None,
            owns_rf: false,
            finished: false,
        }
    }

    pub fn active(&self) -> bool {
        self.phase != Phase::Idle
    }

    pub fn idle_modem(&mut self) -> Option<&mut Modem<B>> {
        if self.active() {
            None
        } else {
            self.modem.as_mut()
        }
    }

    pub const fn status(&self) -> AprsStatus {
        self.status
    }

    /// Let the root invalidate its applied RX cache after stop(). Otherwise an
    /// unchanged Profile could leave RF off because no retune appears needed.
    pub fn take_finished(&mut self) -> bool {
        let finished = self.finished;
        self.finished = false;
        finished
    }

    /// Keep terminal result visible briefly, then permit another explicit send.
    /// The caller supplies a fresh GPS fix decision; stale fixes never create
    /// readiness, and this method never starts a transmission.
    pub fn refresh(&mut self, has_fix: bool, now: u32) {
        if self.active() {
            return;
        }
        if self.config.is_none() {
            self.status = AprsStatus::Unconfigured;
        } else if self.modem.is_none() {
            self.status = AprsStatus::Unsupported;
        } else if !has_fix {
            self.status = AprsStatus::NoFix;
        } else if self
            .finished_at
            .is_none_or(|at| now.wrapping_sub(at) >= RESULT_DISPLAY_MS)
        {
            self.finished_at = None;
            self.status = AprsStatus::Ready;
        }
    }

    /// Validate the manual or explicitly armed automatic request against the exact
    /// selected TX plan before modem startup. Callsign/path/position come from
    /// the user's configuration and a current GPS fix, never placeholders.
    #[cfg_attr(target_arch = "arm", inline(never))]
    pub fn request(
        &mut self,
        calibration: &Calibration<'_>,
        profile: Profile,
        fix: Option<(i32, i32)>,
        now: u32,
        automatic: bool,
    ) -> bool {
        if self.active() {
            return false;
        }
        let Some(config) = self.config else {
            self.status = AprsStatus::Unconfigured;
            return false;
        };
        let Some((latitude, longitude)) = fix else {
            self.status = AprsStatus::NoFix;
            return false;
        };
        let valid = profile.audio.modulation == Modulation::Fm
            && profile.receive.is_some()
            && profile
                .transmit
                .is_some_and(|tx| uv98_rf_sequence::transmit(calibration, tx).is_ok());
        let packet = config
            .position(latitude, longitude)
            .and_then(|frame| Wire::encode(&frame));
        if !valid
            || (automatic && !auto_channel(&profile))
            || packet.is_err()
            || self.modem.is_none()
        {
            self.status = AprsStatus::Failed;
            self.finished_at = Some(now);
            return false;
        }
        let modem = self.modem.as_mut().unwrap();
        if modem.begin(now).is_err() {
            let _ = modem.off();
            self.status = AprsStatus::Failed;
            self.finished_at = Some(now);
            return false;
        }
        self.automatic = automatic;
        self.last_carrier_check = now;
        self.packet = packet.ok();
        self.profile = Some(profile);
        self.requested_at = now;
        self.finished_at = None;
        self.menu_released = false;
        self.quiet_since = None;
        self.owns_rf = false;
        self.finished = false;
        self.phase = Phase::ModemStartup;
        self.status = AprsStatus::Sending;
        true
    }

    fn mark_finished(&mut self, now: u32, status: AprsStatus) {
        self.phase = Phase::Idle;
        self.owns_rf = false;
        self.packet = None;
        self.profile = None;
        self.quiet_since = None;
        self.status = status;
        self.finished_at = Some(now);
        self.finished = true;
    }

    fn finish<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        now: u32,
        mut status: AprsStatus,
    ) -> Result<bool, Fault<I::Error>> {
        // Remove modem audio before removing RF. Always attempt RF stop even
        // when a modem register write fails; it has an independent shutdown.
        if self
            .modem
            .as_mut()
            .is_some_and(|modem| modem.off().is_err())
        {
            status = AprsStatus::Failed;
        }
        let owned = self.owns_rf;
        self.mark_finished(now, status);
        if owned {
            radio.stop()?;
        }
        Ok(false)
    }

    fn radio_fault<E>(&mut self, fault: Fault<E>, now: u32) -> Fault<E> {
        // Runtime already attempted independent RF cleanup on its I/O fault.
        if let Some(modem) = self.modem.as_mut() {
            let _ = modem.off();
        }
        self.mark_finished(now, AprsStatus::Failed);
        fault
    }

    /// Run before Runtime::tick and before LCD/storage/logging. During Sending
    /// call at least once per <6.67 ms byte; modem underflow aborts the packet.
    /// No busy wait, frame loop, hidden re-key or automatic retry is performed.
    #[allow(clippy::too_many_arguments)]
    pub fn step<I: Io>(
        &mut self,
        radio: &mut Runtime<I>,
        calibration: &Calibration<'_>,
        now: u32,
        current: Option<Profile>,
        screen_open: bool,
        upper: bool,
        lower: bool,
        aux_clear: bool,
        matrix: Option<u16>,
        mut clear_at_keying: impl FnMut() -> bool,
    ) -> Result<bool, Fault<I::Error>> {
        if !self.active() {
            return Ok(false);
        }
        let key_ok = match matrix {
            Some(0) => {
                self.menu_released = true;
                true
            }
            Some(MENU_MATRIX) => !self.automatic && !self.menu_released,
            _ => false,
        };
        if !screen_open
            || upper
            || lower
            || !aux_clear
            || !key_ok
            || current != self.profile
            || now.wrapping_sub(self.requested_at) >= REQUEST_TIMEOUT_MS
            || (!self.owns_rf && radio.tx_active_or_starting())
        {
            return self.finish(radio, now, AprsStatus::Failed);
        }
        match self.phase {
            Phase::Idle => Ok(false),
            Phase::ModemStartup => {
                let progress = self.modem.as_mut().unwrap().poll(now);
                match progress {
                    Err(_) => self.finish(radio, now, AprsStatus::Failed),
                    Ok(Progress::Ready)
                        if self.menu_released && radio.state() == State::Receiving =>
                    {
                        let profile = self.profile.unwrap();
                        let rx = profile.receive.unwrap();
                        // A tone-mismatched carrier still owns the channel.
                        // Inspect carrier SQL, never the decoded audio gate.
                        let carrier = if self.automatic {
                            // Keep RX audio and tone detection running while deferring.
                            if now.wrapping_sub(self.last_carrier_check) > 50 {
                                self.quiet_since = None;
                            }
                            self.last_carrier_check = now;
                            radio.carrier_present()
                        } else {
                            radio
                                .observe(profile.audio, rx.tone)
                                .map(|s| s.squelch_open)
                        };
                        let busy = match carrier {
                            Ok(busy) => busy,
                            Err(fault) => return Err(self.radio_fault(fault, now)),
                        };
                        if busy {
                            self.quiet_since = None;
                            return Ok(false);
                        }
                        let quiet_since = *self.quiet_since.get_or_insert(now);
                        if now.wrapping_sub(quiet_since) < QUIET_CHANNEL_MS {
                            return Ok(false);
                        }
                        let tx = profile.transmit.unwrap();
                        // A second qualification immediately precedes keying.
                        if uv98_rf_sequence::transmit(calibration, tx).is_err() {
                            return self.finish(radio, now, AprsStatus::Failed);
                        }
                        if self.automatic {
                            if !clear_at_keying() {
                                return self.finish(radio, now, AprsStatus::Failed);
                            }
                            match radio.carrier_present() {
                                Ok(false) => {}
                                Ok(true) => {
                                    self.quiet_since = None;
                                    return Ok(false);
                                }
                                Err(fault) => return Err(self.radio_fault(fault, now)),
                            }
                        }
                        self.owns_rf = true;
                        if let Err(fault) = radio.stop() {
                            return Err(self.radio_fault(fault, now));
                        }
                        if let Err(fault) = radio.transmit(calibration, tx, now, true) {
                            return Err(self.radio_fault(fault, now));
                        }
                        self.phase = Phase::RadioStartup;
                        Ok(true)
                    }
                    Ok(_) if radio.state() == State::Failed => {
                        self.finish(radio, now, AprsStatus::Failed)
                    }
                    Ok(_) => {
                        self.quiet_since = None;
                        Ok(false)
                    }
                }
            }
            Phase::RadioStartup => match radio.state() {
                State::Configuring => Ok(true),
                State::Transmitting => {
                    // PB7 carries the external modem into the normal MIC path.
                    // Unlike internal DTMF, MIC ADC must remain enabled here.
                    if self
                        .modem
                        .as_mut()
                        .unwrap()
                        .start_tx(self.packet.as_ref().unwrap(), now, self.level)
                        .is_err()
                    {
                        return self.finish(radio, now, AprsStatus::Failed);
                    }
                    self.phase = Phase::Sending;
                    Ok(true)
                }
                _ => self.finish(radio, now, AprsStatus::Failed),
            },
            Phase::Sending => {
                if radio.state() != State::Transmitting {
                    return self.finish(radio, now, AprsStatus::Failed);
                }
                match self.modem.as_mut().unwrap().poll(now) {
                    Ok(Progress::Sending) => Ok(true),
                    Ok(Progress::Complete) => self.finish(radio, now, AprsStatus::Sent),
                    _ => self.finish(radio, now, AprsStatus::Failed),
                }
            }
        }
    }
}

/// Carrier sensing is meaningful only on the exact frequency about to be keyed.
pub fn auto_channel(profile: &Profile) -> bool {
    profile
        .receive
        .zip(profile.transmit)
        .is_some_and(|(rx, tx)| {
            rx.modulation == Modulation::Fm
                && rx.frequency_10hz == tx.frequency_10hz
                && rx.level != 0
                && !profile.audio.monitor
                && profile.audio.squelch != 0
        })
}
