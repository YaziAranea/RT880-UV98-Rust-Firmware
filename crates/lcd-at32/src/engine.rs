use crate::{BusTiming, Error, TimingPhase, Wait};
use core::num::NonZeroU32;
use uv98_display::DisplayBus;

const TDBE: u32 = 1 << 1;
const STATUS_ERRORS: u32 = 0x78;

/// Volatile operations must complete without panic or retained references.
pub(crate) trait Registers {
    fn status(&mut self) -> u32;
    fn transmit(&mut self, byte: u8);
    fn chip_select(&mut self, active: bool);
    fn data_mode(&mut self, data: bool);
    fn reset_level(&mut self, high: bool);
    fn disable(&mut self);
}

pub(crate) struct Engine<R: Registers, T: BusTiming> {
    pub(crate) registers: R,
    pub(crate) timing: T,
    budget: NonZeroU32,
    poisoned: bool,
}

impl<R: Registers, T: BusTiming> Engine<R, T> {
    pub(crate) fn new(registers: R, timing: T, budget: NonZeroU32) -> Self {
        Self {
            registers,
            timing,
            budget,
            poisoned: false,
        }
    }
    fn healthy(&self) -> Result<(), Error> {
        if self.poisoned {
            Err(Error::Poisoned)
        } else {
            Ok(())
        }
    }
    fn wait(&mut self, phase: Wait) -> Result<(), Error> {
        for _ in 0..self.budget.get() {
            let status = self.registers.status();
            if status & STATUS_ERRORS != 0 {
                return Err(Error::Status(status));
            }
            let ready = match phase {
                Wait::TransmitEmpty => status & TDBE != 0,
                Wait::NotBusy => status & 0x80 == 0,
            };
            if ready {
                return Ok(());
            }
        }
        Err(Error::Timeout(phase))
    }
    fn abort(&mut self) {
        // A failed byte may be truncated. Abort does not claim a completed
        // command; CS high prevents further clocks from entering the panel.
        self.registers.chip_select(false);
        self.registers.disable();
        self.registers.data_mode(true);
        self.timing.wait(TimingPhase::Inactive);
        self.poisoned = true;
    }
    fn write_byte(&mut self, byte: u8) -> Result<(), Error> {
        self.wait(Wait::TransmitEmpty)?;
        self.registers.transmit(byte);
        Ok(())
    }
    #[inline(always)]
    fn write_pixel_byte(&mut self, byte: u8) -> Result<(), Error> {
        // The pixel stream waits for TDBE on every byte. Keep retries local so
        // polling a busy SPI shifter takes only a few instructions per read.
        let mut polls = self.budget.get();
        loop {
            let status = self.registers.status();
            match status & (TDBE | STATUS_ERRORS) {
                TDBE => break,
                0 => {
                    polls -= 1;
                    if polls == 0 {
                        return Err(Error::Timeout(Wait::TransmitEmpty));
                    }
                }
                _ => return Err(Error::Status(status)),
            }
        }
        self.registers.transmit(byte);
        Ok(())
    }
    fn transfer_with(
        &mut self,
        data: bool,
        empty: bool,
        write: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.healthy()?;
        if empty {
            return Ok(());
        }
        let result = (|| {
            self.wait(Wait::NotBusy)?;
            self.registers.data_mode(data);
            self.registers.chip_select(true);
            self.timing.wait(TimingPhase::Setup);
            write(self)?;
            // TDBE alone does not mean the final serial bit was shifted out.
            self.wait(Wait::TransmitEmpty)?;
            self.wait(Wait::NotBusy)?;
            self.timing.wait(TimingPhase::Hold);
            self.registers.chip_select(false);
            self.registers.data_mode(true);
            self.timing.wait(TimingPhase::Inactive);
            Ok(())
        })();
        if result.is_err() {
            self.abort();
        }
        result
    }
    fn transfer(&mut self, data: bool, bytes: &[u8]) -> Result<(), Error> {
        self.transfer_with(data, bytes.is_empty(), |bus| {
            for &byte in bytes {
                bus.write_byte(byte)?;
            }
            Ok(())
        })
    }
}

impl<R: Registers, T: BusTiming> DisplayBus for Engine<R, T> {
    type Error = Error;
    fn reset(&mut self, high: bool) -> Result<(), Error> {
        self.healthy()?;
        if let Err(error) = self.wait(Wait::NotBusy) {
            self.abort();
            return Err(error);
        }
        self.registers.chip_select(false);
        self.registers.data_mode(true);
        self.registers.reset_level(high);
        self.timing.wait(TimingPhase::Inactive);
        Ok(())
    }
    fn command(&mut self, command: u8) -> Result<(), Error> {
        self.transfer(false, &[command])
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.transfer(true, bytes)
    }
    fn pixels_rgb565(&mut self, pixels: &[u16]) -> Result<(), Error> {
        // One selected transaction for the caller's row. Remain in the same
        // 8-bit SPI mode: two bounded TDBE waits and writes per RGB565 word.
        // No staging copy, per-pixel GPIO, FIFO assumption or SPI clock change.
        self.transfer_with(true, pixels.is_empty(), |bus| {
            for &pixel in pixels {
                let [high, low] = pixel.to_be_bytes();
                bus.write_pixel_byte(high)?;
                bus.write_pixel_byte(low)?;
            }
            Ok(())
        })
    }
}

impl<R: Registers, T: BusTiming> Drop for Engine<R, T> {
    fn drop(&mut self) {
        // Abortive shutdown, no reset pulse or claim of final-byte completion.
        self.registers.chip_select(false);
        self.registers.disable();
        self.registers.data_mode(true);
    }
}
