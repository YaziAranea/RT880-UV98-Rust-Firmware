//! R-13 / DRX-00 capability model. This module is deliberately pure: it names
//! logical slots, the physical receiver each slot could use, and the evidence
//! status of a pair. It never writes registers, GPIO or audio hardware.
//!
//! On the related RT880 V2 schematic the secondary BK4819 receive input and
//! audio output are not connected to the antenna/audio networks. The user's
//! UV98 Plus revision is not identified, so no pair is advertised as supported.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalSlot {
    A,
    B,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalReceiver {
    /// U201 on the related schematic, existing working receive path.
    PrimaryA,
    /// U400 on the related schematic, bus/init known, RF/audio path unproven.
    SecondaryB,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Confidence {
    /// Observed on this receiver or in bounded stock execution and repeated.
    ProvenSoftware,
    /// Read from the related schematic; PCB revision of the user's radio unknown.
    SchematicRelated,
    /// Required fact is absent.
    Unverified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairStatus {
    Supported,
    Unsupported(&'static str),
    Unverified(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioCapability {
    /// One analog source reaches the speaker at a time.
    SelectOne,
    /// A real mixer feeds both signals to the speaker.
    MixBoth,
    Unverified(&'static str),
}

pub const B_RF_INPUT: Confidence = Confidence::Unverified;
pub const B_AUDIO: Confidence = Confidence::Unverified;
pub const A_RECEIVE: Confidence = Confidence::ProvenSoftware;
pub const SHARED_ANTENNA: Confidence = Confidence::SchematicRelated;

pub const B_RF_REASON: &str =
    "RT880 V2 page2: U400 LNAIN pin15 goes through L400 47nH and C461 100pF to \
     an open upper end with no antenna net; C431 is MICP->GND and C489 is \
     MICN->MOD4819B. UV98 Plus revision unverified.";
pub const B_AUDIO_REASON: &str =
    "RT880 V2 page2: EARO pin8 goes through C433 (marked NC) to AF OUT4819B. \
     U501 SW1 selects AFOUT4819 or MOD4819B by PC4; MOD4819B is a \
     modulation/mic net, not B receive audio, and there is no mixer.";
pub const DUPLICATE_REASON: &str = "A pair needs two distinct physical receivers.";

pub const fn receiver_for_slot(slot: LogicalSlot) -> PhysicalReceiver {
    // DRX-01 hypothesis only: Slot B would need the secondary receiver if its
    // RF/audio chain is ever proven. No production caller may wire this mapper
    // to RF, TX or audio before that evidence exists.
    match slot {
        LogicalSlot::A => PhysicalReceiver::PrimaryA,
        LogicalSlot::B => PhysicalReceiver::SecondaryB,
    }
}

/// TX stays on the qualified primary path. A UI label "B" never authorizes a
/// second transmit backend.
pub const fn tx_backend(receiver: PhysicalReceiver) -> Option<&'static str> {
    match receiver {
        PhysicalReceiver::PrimaryA => Some("primary channel-tx path"),
        PhysicalReceiver::SecondaryB => None,
    }
}

/// No pair is supported by the current evidence. Any pair that needs the
/// secondary receiver must stay unverified until both its RF input and its
/// audio path are proven on the exact board revision.
pub const fn pair_status(first: PhysicalReceiver, second: PhysicalReceiver) -> PairStatus {
    if matches!(
        (first, second),
        (PhysicalReceiver::PrimaryA, PhysicalReceiver::PrimaryA)
            | (PhysicalReceiver::SecondaryB, PhysicalReceiver::SecondaryB)
    ) {
        return PairStatus::Unsupported(DUPLICATE_REASON);
    }
    if matches!(
        (first, second),
        (PhysicalReceiver::SecondaryB, _) | (_, PhysicalReceiver::SecondaryB)
    ) {
        return PairStatus::Unverified(B_RF_REASON);
    }
    PairStatus::Unverified("Two-receiver operation is not required for one receiver.")
}

pub const fn audio_capability() -> AudioCapability {
    AudioCapability::Unverified(B_AUDIO_REASON)
}

/// Logical slots do not prove two physical RX chains. The current production
/// image still operates only the primary receiver.
pub const fn simultaneous_rx_supported() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_map_to_named_receivers_without_claiming_a_second_rx() {
        assert_eq!(
            receiver_for_slot(LogicalSlot::A),
            PhysicalReceiver::PrimaryA
        );
        assert_eq!(
            receiver_for_slot(LogicalSlot::B),
            PhysicalReceiver::SecondaryB
        );
        assert!(!simultaneous_rx_supported());
        assert_eq!(A_RECEIVE, Confidence::ProvenSoftware);
        assert_eq!(B_RF_INPUT, Confidence::Unverified);
        assert_eq!(B_AUDIO, Confidence::Unverified);
        assert_eq!(SHARED_ANTENNA, Confidence::SchematicRelated);
    }

    #[test]
    fn every_pair_with_secondary_stays_unverified_and_has_a_reason() {
        for pair in [
            (PhysicalReceiver::PrimaryA, PhysicalReceiver::SecondaryB),
            (PhysicalReceiver::SecondaryB, PhysicalReceiver::PrimaryA),
            (PhysicalReceiver::SecondaryB, PhysicalReceiver::SecondaryB),
        ] {
            match pair_status(pair.0, pair.1) {
                PairStatus::Supported => panic!("no supported dual pair is proven"),
                PairStatus::Unsupported(reason) | PairStatus::Unverified(reason) => {
                    assert!(!reason.is_empty());
                }
            }
            assert!(!matches!(
                pair_status(pair.0, pair.1),
                PairStatus::Supported
            ));
        }
        assert_eq!(
            pair_status(PhysicalReceiver::SecondaryB, PhysicalReceiver::PrimaryA),
            PairStatus::Unverified(B_RF_REASON)
        );
    }

    #[test]
    fn duplicate_receivers_are_rejected_not_treated_as_dual_rx() {
        assert_eq!(
            pair_status(PhysicalReceiver::PrimaryA, PhysicalReceiver::PrimaryA),
            PairStatus::Unsupported(DUPLICATE_REASON)
        );
        assert_eq!(
            pair_status(PhysicalReceiver::SecondaryB, PhysicalReceiver::SecondaryB),
            PairStatus::Unsupported(DUPLICATE_REASON)
        );
    }

    #[test]
    fn audio_is_not_a_mixer_and_tx_never_uses_secondary() {
        assert!(matches!(audio_capability(), AudioCapability::Unverified(_)));
        assert_eq!(
            audio_capability(),
            AudioCapability::Unverified(B_AUDIO_REASON)
        );
        assert!(tx_backend(PhysicalReceiver::PrimaryA).is_some());
        assert_eq!(tx_backend(PhysicalReceiver::SecondaryB), None);
    }
}
