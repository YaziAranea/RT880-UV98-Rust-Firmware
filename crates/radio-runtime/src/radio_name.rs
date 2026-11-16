//! Stock RT880 1.16 fixed-length name packets on the primary BK4819.
//! See research/r41-signaling/STOCK-NAME-RU.txt for the reference trace.

use crate::{Fault, Io, Runtime, State};
use uv98_handheld_ui::controller::Slot;
use uv98_signaling::radio_name::{RadioName, NAME_BYTES};

pub(super) const PROFILE: [(u8, u16); 7] = [
    (0x70, 0x00e0),
    (0x72, 0x3065),
    (0x58, 0x37c3),
    (0x5a, 0x85cf),
    (0x5b, 0xab45),
    (0x5c, 0x5665),
    (0x5d, 0x0f00),
];
pub(super) const CONTROL: u16 = 0x0028;
pub(super) const SCRAMBLE: u16 = 1 << 13;
pub(super) const RX_FINISHED_IRQ: u16 = 1 << 13;
pub(super) const CRC_PASS: u16 = 1 << 4;

const CLEAR_RX_FIFO: u16 = 1 << 14;
const RX_ENABLE: u16 = 1 << 12;
const POPUP_MS: u32 = 3000;
const SAVED: [u8; 9] = [0x3f, 0x59, 0x70, 0x72, 0x58, 0x5a, 0x5b, 0x5c, 0x5d];

#[derive(Clone, Copy, Default)]
enum Phase {
    #[default]
    Idle,
    Capture(usize),
    Prepare(usize),
    Armed,
    Rearm,
}

#[derive(Default)]
pub(super) struct Receiver {
    phase: Phase,
    saved: [u16; SAVED.len()],
    latest: Option<(RadioName, u32)>,
}

impl Receiver {
    pub(super) fn armed(&self) -> bool {
        matches!(self.phase, Phase::Armed)
    }
}

impl<I: Io> Runtime<I> {
    /// A validated name belongs to the current FM RF/slot context only.
    pub fn received_radio_name(&self, now: u32) -> Option<(Slot, RadioName)> {
        let (name, at) = self.name_rx.latest?;
        if self.state != State::Receiving || now.wrapping_sub(at) >= POPUP_MS {
            return None;
        }
        Some((self.received_dtmf_slot()?, name))
    }

    pub(super) fn prepare_name_rx(&mut self) -> Result<(), Fault<I::Error>> {
        self.name_rx.phase = match self.name_rx.phase {
            Phase::Idle => Phase::Capture(0),
            Phase::Capture(index) => {
                self.name_rx.saved[index] = self.read(SAVED[index])?;
                if index + 1 == SAVED.len() {
                    Phase::Prepare(0)
                } else {
                    Phase::Capture(index + 1)
                }
            }
            Phase::Prepare(index) => {
                let irq = self.name_rx.saved[0];
                let (register, value) = PROFILE
                    .into_iter()
                    .chain([
                        (0x3f, irq | RX_FINISHED_IRQ),
                        (0x59, CLEAR_RX_FIFO | CONTROL),
                        (0x59, SCRAMBLE | RX_ENABLE | CONTROL),
                    ])
                    .nth(index)
                    .unwrap();
                self.write(register, value)?;
                if index + 1 == PROFILE.len() + 3 {
                    Phase::Armed
                } else {
                    Phase::Prepare(index + 1)
                }
            }
            Phase::Armed => Phase::Armed,
            Phase::Rearm => {
                self.write(0x59, CLEAR_RX_FIFO | CONTROL)?;
                self.write(0x59, SCRAMBLE | RX_ENABLE | CONTROL)?;
                Phase::Armed
            }
        };
        Ok(())
    }

    pub(super) fn finish_name_rx(&mut self, crc_ok: bool) -> Result<(), Fault<I::Error>> {
        if crc_ok {
            // A complete frame holds exactly eight words. Read as one
            // bounded batch, so another RX poll cannot mix two payloads.
            let mut payload = [0; NAME_BYTES];
            for pair in payload.chunks_exact_mut(2) {
                pair.copy_from_slice(&self.read(0x5f)?.to_be_bytes());
            }
            // A completion during the drain makes the FIFO contents
            // ambiguous. Confirm no new IRQ before publishing the bytes.
            let stable = self.read(0x0c)? & 1 == 0;
            if let (true, Some(name), Some(now)) =
                (stable, RadioName::from_payload(payload), self.last_now)
            {
                self.name_rx.latest = Some((name, now));
            }
        }
        // Disable before clearing the FIFO. The next observation services
        // any intervening IRQ with name capture disabled, then rearms.
        self.write(0x59, CONTROL)?;
        self.name_rx.phase = Phase::Rearm;
        Ok(())
    }

    pub(super) fn stop_name_rx(&mut self) -> Result<(), Fault<I::Error>> {
        let result = self.restore_name_rx();
        self.io_result(result)
    }

    pub(super) fn restore_name_rx(&mut self) -> Result<(), I::Error> {
        let previous = core::mem::take(&mut self.name_rx);
        if matches!(previous.phase, Phase::Idle | Phase::Capture(_)) {
            return Ok(());
        }
        let mut result = self.io.write(0x59, CLEAR_RX_FIFO | CONTROL);
        for (register, value) in SAVED.into_iter().zip(previous.saved).rev() {
            result = result.and(self.io.write(register, value));
        }
        result
    }
}
