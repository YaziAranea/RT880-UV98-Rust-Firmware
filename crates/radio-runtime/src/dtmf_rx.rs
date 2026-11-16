//! BK4819 DTMF decoder on the primary FM receive path.

use crate::{Fault, Io, Runtime, State};
use uv98_signaling::dtmf_rx::DtmfRx;

// Beken BK4819 V3 register list, REG09/24/3F. Filter words and the setup
// sequence were traced in stock RT880 1.16 at 08004770..08004848.
const COEFFICIENTS: [u8; 16] = [
    0x6f, 0x6b, 0x67, 0x62, 0x50, 0x47, 0x3a, 0x2c, 0x41, 0x37, 0x25, 0x17, 0xe4, 0xcb, 0xb5, 0x9f,
];
// Native RT880 CPS calls CFG[519] the decode threshold. The examined stock
// configuration exports use 22. CFG[518] is the separate TX tone gain.
const DETECTOR_THRESHOLD: u16 = 22;
const DETECTOR_CONTROL: u16 = 0x807e | (DETECTOR_THRESHOLD << 7);
const DTMF_IRQ: u16 = 1 << 11;

// The stock IRQ consumer at 0801D990 reads REG0B before acknowledging.
// Bit 12 identifies a new DTMF symbol, bits 11:8 give its value.
pub(super) const SYMBOL_FOUND: u16 = 1 << 12;

struct Baseline {
    reg21: u16,
    reg24: u16,
    irq_mask: u16,
}

#[derive(Default)]
pub(super) struct Receiver {
    pub(super) text: DtmfRx,
    coefficients_written: usize,
    baseline: Option<Baseline>,
}

impl<I: Io> Runtime<I> {
    /// Current decoded symbols, empty after expiry or when leaving receive.
    /// Use the same monotonic millisecond clock as `tick`.
    pub fn received_dtmf(&self, now: u32) -> &str {
        if self.state == State::Receiving {
            self.dtmf_rx.text.text(now)
        } else {
            ""
        }
    }

    pub(super) fn prepare_dtmf_rx(&mut self) -> Result<bool, Fault<I::Error>> {
        if self.dtmf_rx.baseline.is_some() {
            return Ok(true);
        }
        let index = self.dtmf_rx.coefficients_written;
        if let Some(&coefficient) = COEFFICIENTS.get(index) {
            // Coefficients survive ordinary RX/TX reconfiguration. Load one
            // word per observation; no long register loop in the foreground.
            self.write(0x09, ((index as u16) << 12) | u16::from(coefficient))?;
            self.dtmf_rx.coefficients_written += 1;
            return Ok(false);
        }
        let baseline = Baseline {
            reg21: self.read(0x21)?,
            reg24: self.read(0x24)?,
            irq_mask: self.read(0x3f)?,
        };
        let irq_mask = baseline.irq_mask;
        self.dtmf_rx.baseline = Some(baseline);
        self.write(0x21, 0x06d8)?;
        self.write(0x02, 0)?;
        self.write(0x3f, irq_mask | DTMF_IRQ)?;
        self.write(0x24, DETECTOR_CONTROL)?;
        Ok(true)
    }

    pub(super) fn stop_dtmf_rx(&mut self) -> Result<(), Fault<I::Error>> {
        let result = self.restore_dtmf_rx();
        self.io_result(result)
    }

    pub(super) fn restore_dtmf_rx(&mut self) -> Result<(), I::Error> {
        self.dtmf_rx.text.clear();
        let Some(saved) = self.dtmf_rx.baseline.take() else {
            return Ok(());
        };
        let mut result = Ok(());
        for (reg, word) in [
            (0x24, saved.reg24),
            (0x3f, saved.irq_mask),
            (0x02, 0),
            (0x21, saved.reg21),
        ] {
            // Attempt each independent restore even after a bus error.
            result = result.and(self.io.write(reg, word));
        }
        result
    }
}
