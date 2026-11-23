use crate::{Error, GuardScan, Scan, ScanTiming, SettlePhase, COLUMN_PINS};

/// All operations complete without panic. Select receives exactly one bit;
/// release only releases the four owned open-drain column outputs.
pub(crate) trait Registers {
    fn release(&mut self);
    fn select(&mut self, column_pin: u16);
    /// Pull all four owned open-drain columns LOW for activity rejection only.
    /// This must neither drive HIGH nor change any unowned pin.
    fn select_all(&mut self);
    /// High bits in row order PC10,PC11,PC12,PD2; only low nibble is used.
    fn rows_high(&mut self) -> u8;
    fn ptt_high(&mut self) -> bool;
}

pub(crate) struct Engine<R: Registers, T: ScanTiming> {
    pub(crate) registers: R,
    timing: T,
}
impl<R: Registers, T: ScanTiming> Engine<R, T> {
    pub(crate) fn new(registers: R, timing: T) -> Self {
        Self { registers, timing }
    }
    fn released(&mut self) -> Result<(), Error> {
        self.registers.release();
        self.timing.settle(SettlePhase::Released);
        let low_rows = (!self.registers.rows_high()) & 15;
        if low_rows != 0 {
            Err(Error::RowsNotReleased { low_rows })
        } else {
            Ok(())
        }
    }
    fn ptt_matches(&mut self, expected: bool) -> Result<(), Error> {
        if self.registers.ptt_high() == expected {
            Ok(())
        } else {
            Err(Error::UnstablePtt)
        }
    }
    fn pass(&mut self, reverse: bool, ptt_high: bool) -> Result<u16, Error> {
        let mut matrix = 0u16;
        for n in 0..4 {
            let column = if reverse { 3 - n } else { n };
            // Previous iteration ended released and settled: break-before-make.
            self.registers.select(COLUMN_PINS[column]);
            self.timing.settle(SettlePhase::Selected);
            self.ptt_matches(ptt_high)?;
            let low_rows = (!self.registers.rows_high()) & 15;
            self.ptt_matches(ptt_high)?;
            matrix |= u16::from(low_rows) << (4 * column);
            self.released()?;
            self.ptt_matches(ptt_high)?;
        }
        Ok(matrix)
    }
    pub(crate) fn scan(&mut self) -> Result<Scan, Error> {
        let result = (|| {
            self.released()?;
            let ptt_high = self.registers.ptt_high();
            let forward = self.pass(false, ptt_high)?;
            let reverse = self.pass(true, ptt_high)?;
            if forward != reverse {
                return Err(Error::UnstableMatrix { forward, reverse });
            }
            Ok(Scan {
                matrix: forward,
                ptt_high,
            })
        })();
        // All return paths release columns, including errors mid-column. No
        // error/ambiguity is substituted with a successful release sample.
        self.registers.release();
        result
    }

    pub(crate) fn scan_idle_or_full(&mut self) -> Result<Scan, Error> {
        let guard = self.guard_scan()?;
        self.scan_after_guard(guard)
    }

    fn scan_after_guard(&mut self, guard: GuardScan) -> Result<Scan, Error> {
        if guard.matrix_clear() && !guard.ptt_pressed() {
            // All owned open-drain columns LOW and all settled rows HIGH
            // prove no matrix contact was observed. PB6 was also HIGH at
            // every guard observation. This shortcut can only report release;
            // it cannot manufacture a key or qualify a new PTT press.
            Ok(Scan {
                matrix: 0,
                ptt_high: true,
            })
        } else {
            // Any matrix activity OR PTT LOW retains both full passes and all
            // existing stability checks. A failed guard never reaches here.
            self.scan()
        }
    }

    pub(crate) fn scan_with_ptt_recheck(
        &mut self,
        analog_still_idle: impl FnOnce() -> bool,
    ) -> Result<Scan, Error> {
        match self.observe_guard() {
            Ok(guard) => self.scan_after_guard(guard),
            Err((error, true)) if analog_still_idle() => {
                let guard = self.guard_scan()?;
                if !guard.ptt_pressed() || !guard.matrix_clear() {
                    return Err(error);
                }
                let sample = self.scan()?;
                if sample.ptt_pressed() && sample.matrix_raw() == 0 {
                    Ok(sample)
                } else {
                    Err(error)
                }
            }
            Err((error, _)) => Err(error),
        }
    }

    pub(crate) fn guard_scan(&mut self) -> Result<GuardScan, Error> {
        self.observe_guard().map_err(|(error, _)| error)
    }

    fn observe_guard(&mut self) -> Result<GuardScan, (Error, bool)> {
        // All-low open-drain columns let any closed matrix contact pull a row
        // low. No selected column ever drives another column high. This path
        // intentionally cannot identify a key or qualify a new PTT edge.
        self.registers.release();
        let ptt_high = self.registers.ptt_high();
        self.registers.select_all();
        self.timing.settle(SettlePhase::Selected);
        let low_rows = !self.registers.rows_high() & 15;
        let ptt_selected = self.registers.ptt_high();
        // Release and settle before interpreting ANY observed error, including
        // a changing PTT. Every success/error path has the same column cleanup.
        self.registers.release();
        self.timing.settle(SettlePhase::Released);
        let idle_low = !self.registers.rows_high() & 15;
        let ptt_released = self.registers.ptt_high();
        if idle_low != 0 {
            return Err((Error::RowsNotReleased { low_rows: idle_low }, false));
        }
        if ptt_high != ptt_selected || ptt_high != ptt_released {
            // Only an isolated new press qualifies for a fresh observation.
            // A chord, release, or change back must propagate the error.
            let isolated_press = low_rows == 0 && ptt_high && !ptt_released;
            return Err((Error::UnstablePtt, isolated_press));
        }
        Ok(GuardScan {
            ptt_high,
            matrix_clear: low_rows == 0,
        })
    }
}
impl<R: Registers, T: ScanTiming> Drop for Engine<R, T> {
    fn drop(&mut self) {
        self.registers.release();
    }
}
