//! Exclusive, muted same-path RX sweep adapter. No automatic TX or restore.
//! Root must complete full RX at QualifiedRange::origin before entering.
//! After stop, restore through a new full receive request, never an old tune.

use crate::{Error, Fault, Io, Runtime, State};
use uv98_rf_sequence::{sweep::QualifiedRange, Step};

// RSSI integration after the reset pulse. Keep RF bus timing and readbacks.
pub const SETTLE_MS: u32 = 5;
// BK4819 scan filter from fagci spectrum GetBWRegValueForScan. Full RX
// restore programs the saved channel's bandwidth again before unmuting.
const SCAN_BANDWIDTH: u16 = 0x01bc;
const SWEEP_RX: u16 = 0xbff1;
pub const SAMPLE_TIMEOUT_MS: u32 = 3000;

#[derive(Clone, Copy)]
pub(crate) struct Session {
    range: QualifiedRange,
    next_index: usize,
    pending: Option<Pending>,
}

#[derive(Clone, Copy)]
struct Pending {
    index: usize,
    started: u32,
    settle_since: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub index: usize,
    pub frequency_10hz: u32,
    /// Fresh A67 &01FF. Not dBm, power, squelch or calibrated sensitivity.
    pub raw: u16,
}

impl<I: Io> Runtime<I> {
    fn sweep_expect(&mut self, register: u8, expected: u16) -> Result<(), Fault<I::Error>> {
        let observed = self.read(register)?;
        if observed != expected {
            return Err(self.fail(Error::Readback {
                register,
                expected,
                observed,
            }));
        }
        Ok(())
    }

    fn sweep_gate_word(&mut self) -> Result<u16, Fault<I::Error>> {
        let word = self.read(0x33)?;
        if word & 0x000c != 0 {
            return Err(self.fail(Error::Readback {
                register: 0x33,
                expected: word & !0x000c,
                observed: word,
            }));
        }
        Ok(word)
    }

    /// Complete full RX at range.origin() first. This adapter verifies the
    /// corrected origin, RX control, route and PA-off words before accepting
    /// ownership. It explicitly mutes and restores fixed factory gain03BE;
    /// modulation/width/filter setup is the caller's prior full-RX contract.
    pub fn sweep_enter(&mut self, range: QualifiedRange, now: u32) -> Result<(), Fault<I::Error>> {
        if self.state != State::Receiving || !self.initialized {
            return Err(self.fail(Error::State));
        }
        self.advance_clock(now)?;
        self.stop_rx_signaling()?;
        let checked = self.io.verify_control();
        self.io_result(checked)?;
        let muted = self.io.mute(true);
        self.io_result(muted)?;
        self.audio_open = false;
        self.write(0x47, 0x6040)?;
        let gates = self.io.hf_gates(false, false);
        self.io_result(gates)?;
        let word = self.sweep_gate_word()?;
        if word & 0x0033 != range.rx_mask() {
            return Err(self.fail(Error::Readback {
                register: 0x33,
                expected: (word & !0x0033) | range.rx_mask(),
                observed: word,
            }));
        }
        self.sweep_expect(0x36, 0x007f)?;
        self.sweep_expect(0x30, 0xbff1)?;
        let (low, high) = range.origin_words();
        self.sweep_expect(0x38, low)?;
        self.sweep_expect(0x39, high)?;
        let antenna = self.io.antenna(range.antenna_high());
        self.io_result(antenna)?;
        self.write(0x43, SCAN_BANDWIDTH)?;
        self.sweep_expect(0x43, SCAN_BANDWIDTH)?;
        self.write(0x13, 0x03be)?;
        self.sweep_expect(0x13, 0x03be)?;
        let checked = self.io.verify_control();
        self.io_result(checked)?;
        self.am_agc = uv98_rf_sequence::am_agc::State::new();
        self.am_gain_word = None;
        self.sweep_session = Some(Session {
            range,
            next_index: 0,
            pending: None,
        });
        self.state = State::Sweeping;
        Ok(())
    }

    /// A synchronous bounded retune with checked control/frequency
    /// writes and fresh A33. Does not wait for settling or claim PLL lock.
    /// Only the next sequential bin is accepted; sample it before another tune.
    pub fn sweep_tune(&mut self, index: usize, now: u32) -> Result<(), Fault<I::Error>> {
        if self.state != State::Sweeping {
            return Err(self.fail(Error::State));
        }
        self.advance_clock(now)?;
        let Some(session) = self.sweep_session else {
            return Err(self.fail(Error::State));
        };
        if session.pending.is_some() || index != session.next_index {
            return Err(self.fail(Error::State));
        }
        let tune = session
            .range
            .tune(index)
            .map_err(|e| self.fail(Error::Sweep(e)))?;
        let checked = self.io.verify_control();
        self.io_result(checked)?;
        let gate_word = self.sweep_gate_word()?;
        self.sweep_expect(0x36, 0x007f)?;
        for operation in tune.operations() {
            match operation {
                Step::WriteA { register, value } => self.write(register, value)?,
                Step::UpdateA {
                    register: 0x33,
                    keep_mask,
                    value,
                } => {
                    // Same qualified RX path for every bin. Keep unrelated
                    // GPIO bits and avoid rewriting a route already selected.
                    let desired = (gate_word & keep_mask) | value;
                    if desired != gate_word {
                        let fresh = self.sweep_gate_word()?;
                        self.write(0x33, (fresh & keep_mask) | value)?;
                    }
                }
                _ => return Err(self.fail(Error::State)),
            }
        }
        // Start the RSSI integration after every frequency/route write.
        // Toggle only bit zero, keeping PA and AF gates in receive state.
        // The immediately preceding RX control write already had a readback.
        self.write(0x30, SWEEP_RX & !1)?;
        self.write(0x30, SWEEP_RX)?;
        // `now` precedes synchronous I/O. It is a timeout anchor only, never
        // treated as proof that writes completed by that instant.
        self.sweep_session = Some(Session {
            pending: Some(Pending {
                index,
                started: now,
                settle_since: None,
            }),
            ..session
        });
        Ok(())
    }

    /// First call after tune must carry a fresh clock sample. It only anchors
    /// settling AFTER the synchronous I/O and returns None without reading67.
    /// Later calls read once after SETTLE_MS; repeated reads cannot reuse that sample.
    /// Root's single foreground scheduler supplies fresh times, never catch-up
    /// events with an earlier timestamp. Clock/timeout/I/O errors clean up RF.
    pub fn sweep_read_rssi(&mut self, now: u32) -> Result<Option<Sample>, Fault<I::Error>> {
        if self.state != State::Sweeping {
            return Err(self.fail(Error::State));
        }
        self.advance_clock(now)?;
        let Some(mut session) = self.sweep_session else {
            return Err(self.fail(Error::State));
        };
        let Some(mut pending) = session.pending else {
            return Err(self.fail(Error::State));
        };
        if now.wrapping_sub(pending.started) >= SAMPLE_TIMEOUT_MS {
            return Err(self.fail(Error::ConfigurationTimeout));
        }
        let checked = self.io.verify_control();
        self.io_result(checked)?;
        let Some(since) = pending.settle_since else {
            pending.settle_since = Some(now);
            session.pending = Some(pending);
            self.sweep_session = Some(session);
            return Ok(None);
        };
        if now.wrapping_sub(since) < SETTLE_MS {
            return Ok(None);
        }
        self.sweep_gate_word()?;
        self.sweep_expect(0x36, 0x007f)?;
        let raw = self.read(0x67)? & 0x01ff;
        let frequency_10hz = session
            .range
            .tune(pending.index)
            .map_err(|e| self.fail(Error::Sweep(e)))?
            .frequency();
        session.pending = None;
        session.next_index = (pending.index + 1) % session.range.len();
        self.sweep_session = Some(session);
        Ok(Some(Sample {
            index: pending.index,
            frequency_10hz,
            raw,
        }))
    }
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
