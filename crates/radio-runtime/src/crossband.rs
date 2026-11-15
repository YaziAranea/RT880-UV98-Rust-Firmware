//! Cross-band relay admission and carrier tracking. This module has no I/O.
//! The caller must own both RF paths and prove the secondary is off before
//! arming. Only a fresh, settled primary squelch/tone result is a carrier.

use crate::{policy::Profile, tx_policy};
use uv98_band_plan::{select_routes, SelectorMode};
use uv98_handheld_ui::controller::Slot;
use uv98_rf_sequence::{Modulation, RxRequest, TxRequest};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pair {
    incoming: Profile,
    receive: RxRequest,
    transmit: TxRequest,
}

impl Pair {
    /// Keep the primary on the incoming RX frequency and use the destination's
    /// actual TX frequency, including its offset. Profiles must be committed.
    pub fn from_profiles(incoming: Profile, outgoing: Profile) -> Option<Self> {
        let receive = incoming.receive?;
        let transmit = outgoing.transmit?;
        if receive.modulation != Modulation::Fm
            || outgoing.audio.modulation != Modulation::Fm
            || receive.level == 0
            || incoming.audio.monitor
        {
            return None;
        }
        let rx_band = vhf_uhf_path(receive.frequency_10hz, SelectorMode::Primary)?;
        let tx_band = vhf_uhf_path(transmit.frequency_10hz, SelectorMode::Secondary)?;
        (rx_band != tx_band).then_some(Self {
            incoming,
            receive,
            transmit,
        })
    }

    pub const fn receive(self) -> RxRequest {
        self.receive
    }

    pub const fn transmit(self) -> TxRequest {
        self.transmit
    }
}

// First integration scope: the VHF/UHF antenna paths, excluding CB and 800 MHz.
fn vhf_uhf_path(frequency: u32, mode: SelectorMode) -> Option<u8> {
    match frequency {
        13_600_000..=17_400_000 | 40_000_000..=48_000_000 => {
            select_routes(frequency, mode).map(|route| route.tx_code())
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Carrier {
    Closed,
    PendingTone,
    Open,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    None,
    Start(TxRequest),
    Stop,
}

const DWELL_MS: u32 = 100;
const TONE_DWELL_MS: u32 = 250;
const HANDOFF_MS: u32 = 220;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Listening(Option<u32>),
    Active(u32),
    Holdoff(u32),
    Disarmed,
}

pub struct Session {
    pairs: [Pair; 2],
    slot: Slot,
    quiet_seen: [bool; 2],
    state: State,
}

fn index(slot: Slot) -> usize {
    usize::from(slot == Slot::B)
}

impl Session {
    pub fn arm(a: Profile, b: Profile, first: Slot) -> Option<Self> {
        let a_to_b = Pair::from_profiles(a, b)?;
        let b_to_a = Pair::from_profiles(b, a)?;
        if vhf_uhf_path(a_to_b.receive().frequency_10hz, SelectorMode::Primary)
            == vhf_uhf_path(b_to_a.receive().frequency_10hz, SelectorMode::Primary)
        {
            return None;
        }
        Some(Self {
            pairs: [a_to_b, b_to_a],
            slot: first,
            quiet_seen: [false; 2],
            state: State::Listening(None),
        })
    }

    pub const fn slot(&self) -> Slot {
        self.slot
    }

    pub fn pair(&self, slot: Slot) -> Pair {
        self.pairs[index(slot)]
    }

    pub fn listening_profile(&self) -> Profile {
        self.pair(self.slot).incoming
    }

    pub fn disarmed(&self) -> bool {
        self.state == State::Disarmed
    }

    /// A dwell starts with the first fresh sample after tuning. The caller
    /// must clear the previous slot's sample when retuning.
    pub fn tick(&mut self, now: u32, carrier: Option<Carrier>) -> Action {
        let slot = index(self.slot);
        match self.state {
            State::Listening(since) => {
                let Some(carrier) = carrier else {
                    return Action::None;
                };
                if carrier == Carrier::Closed {
                    self.quiet_seen[slot] = true;
                } else if carrier == Carrier::Open && self.quiet_seen[slot] {
                    self.state = State::Active(now);
                    return Action::Start(self.pair(self.slot).transmit());
                }
                let since = since.unwrap_or(now);
                let dwell = if self.pair(self.slot).receive().tone == 0 {
                    DWELL_MS
                } else {
                    TONE_DWELL_MS
                };
                if now.wrapping_sub(since) >= dwell {
                    self.next_slot();
                } else {
                    self.state = State::Listening(Some(since));
                }
            }
            State::Active(start) => match carrier {
                None => {
                    self.state = State::Disarmed;
                    return Action::Stop;
                }
                Some(Carrier::Closed) => return self.end_transmission(now, true),
                Some(Carrier::PendingTone) => return self.end_transmission(now, false),
                Some(Carrier::Open) if !tx_policy::hold_within_limit(start, now) => {
                    return self.end_transmission(now, false);
                }
                Some(Carrier::Open) => {}
            },
            State::Holdoff(since) if now.wrapping_sub(since) >= HANDOFF_MS => self.next_slot(),
            State::Holdoff(_) | State::Disarmed => {}
        }
        Action::None
    }

    fn end_transmission(&mut self, now: u32, input_is_quiet: bool) -> Action {
        // Require a clear destination before accepting a reverse call. A
        // lingering carrier from a remote repeater must not key us back.
        self.quiet_seen[index(self.slot.other())] = false;
        self.quiet_seen[index(self.slot)] = input_is_quiet;
        self.state = State::Holdoff(now);
        Action::Stop
    }

    fn next_slot(&mut self) {
        self.slot = self.slot.other();
        self.state = State::Listening(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Audio, TX_TIMEOUT_MS};
    use uv98_rf_sequence::Width;

    fn profile(rx: u32, tx: u32) -> Profile {
        Profile {
            receive: Some(RxRequest {
                frequency_10hz: rx,
                width: Width::Wide,
                level: 1,
                tone: 0,
                tone_mode: 0,
                option_nibble: 0,
                squelch_delay: 0,
                modulation: Modulation::Fm,
            }),
            transmit: Some(TxRequest {
                frequency_10hz: tx,
                width: Width::Wide,
                tone: 0,
                tone_mode: 0,
                option_nibble: 0,
                power: 0,
                mic_gain: 0,
            }),
            audio: Audio {
                modulation: Modulation::Fm,
                gain: 0,
                dac: 0,
                squelch: 1,
                tone_required: false,
                monitor: false,
            },
            frequency: rx,
            route: 3,
            power: 0,
        }
    }

    #[test]
    fn uses_the_actual_destination_tx_band_and_requires_a_tx_profile() {
        let vhf = profile(14_550_000, 14_550_000);
        let split = profile(14_560_000, 43_350_000);
        let pair = Pair::from_profiles(vhf, split).unwrap();
        assert_eq!(pair.receive().frequency_10hz, 14_550_000);
        assert_eq!(pair.transmit().frequency_10hz, 43_350_000);
        assert!(Pair::from_profiles(vhf, vhf).is_none());
        let mut monitor = vhf;
        monitor.audio.monitor = true;
        assert!(Pair::from_profiles(monitor, split).is_none());
        assert!(Pair::from_profiles(
            vhf,
            Profile {
                transmit: None,
                ..split
            }
        )
        .is_none());
        assert!(Pair::from_profiles(profile(43_350_000, 43_350_000), vhf).is_some());
    }

    #[test]
    fn follows_both_inputs_with_their_actual_tx_frequencies() {
        let a = profile(14_550_000, 14_490_000);
        let b = profile(43_350_000, 43_850_000);
        let mut s = Session::arm(a, b, Slot::A).unwrap();
        let closed = Some(Carrier::Closed);
        let open = Some(Carrier::Open);
        assert_eq!(s.tick(0, open), Action::None);
        assert_eq!(s.tick(1, closed), Action::None);
        assert_eq!(s.tick(2, open), Action::Start(b.transmit.unwrap()));
        assert_eq!(s.tick(2 + DWELL_MS, open), Action::None);
        assert_eq!(s.slot(), Slot::A);
        assert_eq!(s.tick(110, closed), Action::Stop);
        assert_eq!(s.tick(110 + HANDOFF_MS - 1, open), Action::None);
        assert_eq!(s.slot(), Slot::A);
        assert_eq!(s.tick(110 + HANDOFF_MS, open), Action::None);
        assert_eq!(s.slot(), Slot::B);
        assert_eq!(s.listening_profile(), b);
        assert_eq!(s.tick(335, None), Action::None);
        assert_eq!(s.tick(360, Some(Carrier::PendingTone)), Action::None);
        assert_eq!(s.tick(365, open), Action::None);
        assert_eq!(s.tick(370, closed), Action::None);
        assert_eq!(s.tick(375, open), Action::Start(a.transmit.unwrap()));
        assert_eq!(s.tick(380, None), Action::Stop);
        assert!(s.disarmed());
        assert_eq!(s.tick(385, open), Action::None);
    }

    #[test]
    fn idle_dwell_waits_for_tuning_and_timeout_requires_quiet_on_original_input() {
        let a = profile(14_550_000, 14_550_000);
        let b = profile(43_350_000, 43_350_000);
        let mut s = Session::arm(a, b, Slot::B).unwrap();
        let closed = Some(Carrier::Closed);
        let open = Some(Carrier::Open);
        assert_eq!(s.tick(500, None), Action::None);
        assert_eq!(s.slot(), Slot::B);
        s.tick(505, closed);
        s.tick(505 + DWELL_MS, closed);
        assert_eq!(s.slot(), Slot::A);
        s.tick(610, closed);
        assert_eq!(s.tick(620, open), Action::Start(b.transmit.unwrap()));
        let timeout = 620 + TX_TIMEOUT_MS;
        assert_eq!(s.tick(timeout, open), Action::Stop);
        s.tick(timeout + HANDOFF_MS, open);
        s.tick(timeout + HANDOFF_MS + 1, closed);
        s.tick(timeout + HANDOFF_MS + DWELL_MS + 1, closed);
        assert_eq!(s.slot(), Slot::A);
        assert_eq!(s.tick(timeout + 400, open), Action::None);
        s.tick(timeout + 410, closed);
        assert_eq!(
            s.tick(timeout + 420, open),
            Action::Start(b.transmit.unwrap())
        );
    }
}
