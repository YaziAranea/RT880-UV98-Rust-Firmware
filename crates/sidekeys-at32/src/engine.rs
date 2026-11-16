//! Register protocol shared by host fault tests and MMIO backend.
use crate::{
    capture::{MAX_LATE_CYCLES, PERIOD_CYCLES},
    CaptureTiming, Error, PollTiming, Sample, RX_AUDIO_SAMPLES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Register {
    Status,
    Control1,
    Control2,
    SampleTime1,
    SampleTime2,
    Sequence1,
    Sequence2,
    Sequence3,
    Oversampling,
    Misc,
    Data,
    CalibrationValue,
}

pub trait Registers {
    fn read(&mut self, register: Register) -> u32;
    fn write(&mut self, register: Register, value: u32);
}

pub struct Engine<R: Registers, T: PollTiming> {
    registers: R,
    timing: T,
    budget: u32,
    faulted: bool,
    calibration_value: u8,
}

const SIDE_KEY_CHANNEL: u32 = 12;
const BATTERY_CHANNEL: u32 = 7;
const RX_AUDIO_CHANNEL: u32 = 5;
const SIDE_KEY_SAMPLE_TIME: u32 = 7 << (3 * (SIDE_KEY_CHANNEL - 10));
const BATTERY_SAMPLE_TIME: u32 = 7 << (3 * BATTERY_CHANNEL);
// SDK ADC_SAMPLETIME_640_5, ~53 us of acquisition at HCLK/4 = 12 MHz.
const RX_AUDIO_SAMPLE_TIME: u32 = 7 << (3 * RX_AUDIO_CHANNEL);

impl<R: Registers, T: PollTiming> Engine<R, T> {
    /// ADC reset/common-clock and disjoint analog GPIO are caller prerequisites.
    /// SDK adc_base_config MISC09 is mandatory even though PAC omits this field.
    pub fn prepare(registers: R, timing: T, budget: u32) -> Result<Self, Error> {
        if budget == 0 || budget > 100_000 {
            return Err(Error::InvalidBudget);
        }
        let mut this = Self {
            registers,
            timing,
            budget,
            faulted: false,
            calibration_value: 0,
        };
        if let Err(error) = this.initialize() {
            return Err(this.fail(error));
        }
        Ok(this)
    }

    fn initialize(&mut self) -> Result<(), Error> {
        use Register::*;
        for (register, value) in [
            (Control2, 0),
            (Control1, 0),
            (SampleTime1, SIDE_KEY_SAMPLE_TIME),
            (SampleTime2, BATTERY_SAMPLE_TIME),
            (Sequence1, 0),
            (Sequence2, 0),
            (Sequence3, SIDE_KEY_CHANNEL),
            (Oversampling, 0),
            (Misc, 9),
        ] {
            self.registers.write(register, value);
        }
        if self.registers.read(Control1) != 0
            || self.registers.read(Control2) != 0
            || self.registers.read(Sequence1) != 0
            || self.registers.read(Sequence3) != SIDE_KEY_CHANNEL
            || self.registers.read(SampleTime1) != SIDE_KEY_SAMPLE_TIME
            || self.registers.read(SampleTime2) != BATTERY_SAMPLE_TIME
            || self.registers.read(Misc) != 9
        {
            return Err(Error::Configuration);
        }
        self.registers.write(Status, !0x3f); // ready bit6 is read-only
        self.registers.write(Control2, 1);
        self.wait(Status, 1 << 6, true, Error::ReadyTimeout)?;
        self.registers.write(Control2, 1 | (1 << 3));
        self.wait(Control2, 1 << 3, false, Error::CalibrationInitTimeout)?;
        self.registers.write(Control2, 1 | (1 << 2));
        self.wait(Control2, 1 << 2, false, Error::CalibrationTimeout)?;
        self.calibration_value = (self.registers.read(CalibrationValue) & 0x7f) as u8;
        Ok(())
    }

    fn wait(
        &mut self,
        register: Register,
        mask: u32,
        set: bool,
        timeout: Error,
    ) -> Result<(), Error> {
        for poll in 0..self.budget {
            if (self.registers.read(register) & mask != 0) == set {
                return Ok(());
            }
            if poll + 1 < self.budget {
                self.timing.wait_poll()?;
            }
        }
        Err(timeout)
    }

    fn fail(&mut self, error: Error) -> Error {
        if !self.faulted {
            self.registers.write(Register::Control2, 0);
            self.faulted = true;
        }
        error
    }

    /// Exactly one software-triggered conversion. No cached sample on failure.
    pub fn sample(&mut self) -> Result<Sample, Error> {
        if self.faulted {
            return Err(Error::Faulted);
        }
        let result = self.convert(SIDE_KEY_CHANNEL).and_then(Sample::from_raw);
        result.map_err(|error| self.fail(error))
    }

    /// Read PA7 and restore PC2 before returning. Discard the first conversion
    /// after each mux switch to allow the sampling capacitor to settle.
    pub fn sample_battery(&mut self) -> Result<u16, Error> {
        if self.faulted {
            return Err(Error::Faulted);
        }
        self.battery_once().map_err(|error| {
            let error = self.fail(error);
            // Re-select PC2 only after disabling ADC on an incomplete conversion.
            self.registers.write(Register::Sequence3, SIDE_KEY_CHANNEL);
            error
        })
    }

    fn battery_once(&mut self) -> Result<u16, Error> {
        self.check_conversion_state(SIDE_KEY_CHANNEL)?;
        self.registers.write(Register::Sequence3, BATTERY_CHANNEL);
        self.convert(BATTERY_CHANNEL)?;
        let raw = self.convert(BATTERY_CHANNEL)?;
        self.registers.write(Register::Sequence3, SIDE_KEY_CHANNEL);
        self.convert(SIDE_KEY_CHANNEL)?;
        Ok(raw)
    }

    fn check_conversion_state(&mut self, channel: u32) -> Result<(), Error> {
        // Unexpected reconfiguration, continuous conversion, DMA, trigger edge
        // or calibration-in-progress is not normalized into a fresh sample.
        if self.registers.read(Register::Control2) != 1
            || self.registers.read(Register::Sequence3) != channel
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }

    fn convert(&mut self, channel: u32) -> Result<u16, Error> {
        self.prepare_conversion(channel)?;
        self.trigger_conversion();
        self.wait(Register::Status, 1 << 1, true, Error::ConversionTimeout)?;
        self.read_conversion(channel)
    }

    fn prepare_conversion(&mut self, channel: u32) -> Result<(), Error> {
        self.check_conversion_state(channel)?;
        self.registers.write(Register::Status, !0x3f);
        Ok(())
    }

    fn trigger_conversion(&mut self) {
        self.registers.write(Register::Control2, 1 | (1 << 30));
    }

    fn read_conversion(&mut self, channel: u32) -> Result<u16, Error> {
        use Register::*;
        if self.registers.read(Status) & (1 << 5) != 0 {
            return Err(Error::Overflow);
        }
        let value = self.registers.read(Data);
        if value > 4095 {
            return Err(Error::InvalidSample);
        }
        // SDK: reading ODT clears conversion-end. Trigger must self-clear.
        self.check_conversion_state(channel)?;
        Ok(value as u16)
    }

    /// Temporarily borrow idle ADC1 for one PA5 block. The caller owns the
    /// analog input. `cycle_count` reads a running 48 MHz wrapping counter
    /// without side effects, panic or reentry. The buffer is valid only on Ok.
    pub fn capture_rx_audio(
        &mut self,
        samples: &mut [u16; RX_AUDIO_SAMPLES],
        mut cycle_count: impl FnMut() -> u32,
    ) -> Result<CaptureTiming, Error> {
        use Register::*;
        if self.faulted {
            return Err(Error::Faulted);
        }

        let sample_time = self.registers.read(SampleTime2);
        let sequence = self.registers.read(Sequence3);
        if sequence != SIDE_KEY_CHANNEL
            || sample_time & BATTERY_SAMPLE_TIME != BATTERY_SAMPLE_TIME
            || self.registers.read(Control1) != 0
            || self.registers.read(Sequence1) != 0
            || self.registers.read(Sequence2) != 0
            || self.registers.read(SampleTime1) != SIDE_KEY_SAMPLE_TIME
            || self.registers.read(Oversampling) != 0
            || self.registers.read(Misc) != 9
            || self.check_conversion_state(SIDE_KEY_CHANNEL).is_err()
        {
            return Err(self.fail(Error::Configuration));
        }

        let audio_sample_time = sample_time | RX_AUDIO_SAMPLE_TIME;
        self.registers.write(SampleTime2, audio_sample_time);
        self.registers.write(Sequence3, RX_AUDIO_CHANNEL);
        let result = if self.registers.read(SampleTime2) == audio_sample_time {
            self.capture_block(samples, &mut cycle_count)
        } else {
            Err(Error::Configuration)
        };

        // Scheduling errors are only raised with no active conversion.
        // For ADC errors, stop any in-flight conversion before restoring fields.
        if let Err(error) = result {
            if error != Error::Timing {
                self.fail(error);
                if self.registers.read(Control2) != 0 {
                    return Err(Error::Configuration);
                }
            }
        }
        self.registers.write(SampleTime2, sample_time);
        self.registers.write(Sequence3, sequence);
        if self.registers.read(SampleTime2) != sample_time
            || self.registers.read(Sequence3) != sequence
        {
            return Err(self.fail(Error::Configuration));
        }
        if !self.faulted {
            // Discard mux residue after changing back from PA5 to PC2.
            let primed = self
                .prepare_conversion(SIDE_KEY_CHANNEL)
                .and_then(|()| self.capture_conversion(SIDE_KEY_CHANNEL, &mut cycle_count));
            if let Err(error) = primed {
                return Err(self.fail(error));
            }
        }
        result
    }

    fn capture_block(
        &mut self,
        samples: &mut [u16; RX_AUDIO_SAMPLES],
        cycle_count: &mut impl FnMut() -> u32,
    ) -> Result<CaptureTiming, Error> {
        self.prepare_conversion(RX_AUDIO_CHANNEL)?;
        // Discard the first PA5 conversion after selecting it.
        self.capture_conversion(RX_AUDIO_CHANNEL, cycle_count)?;
        let mut deadline = cycle_count().wrapping_add(PERIOD_CYCLES);
        let mut timing = CaptureTiming {
            first_trigger_cycle: 0,
            last_trigger_cycle: 0,
            max_late_cycles: 0,
            min_interval_cycles: u32::MAX,
            max_interval_cycles: 0,
        };
        for (index, sample) in samples.iter_mut().enumerate() {
            self.prepare_conversion(RX_AUDIO_CHANNEL)?;
            wait_until(deadline, cycle_count)?;
            let (raw, trigger) = self.capture_conversion(RX_AUDIO_CHANNEL, cycle_count)?;
            let lateness = trigger.wrapping_sub(deadline);
            if lateness > MAX_LATE_CYCLES {
                return Err(Error::Timing);
            }
            *sample = raw;
            if index == 0 {
                timing.first_trigger_cycle = trigger;
            } else {
                let interval = trigger.wrapping_sub(timing.last_trigger_cycle);
                timing.min_interval_cycles = timing.min_interval_cycles.min(interval);
                timing.max_interval_cycles = timing.max_interval_cycles.max(interval);
            }
            timing.last_trigger_cycle = trigger;
            timing.max_late_cycles = timing.max_late_cycles.max(lateness);
            deadline = deadline.wrapping_add(PERIOD_CYCLES);
        }
        Ok(timing)
    }

    fn capture_conversion(
        &mut self,
        channel: u32,
        cycle_count: &mut impl FnMut() -> u32,
    ) -> Result<(u16, u32), Error> {
        self.trigger_conversion();
        let trigger = cycle_count();
        for _ in 0..PERIOD_CYCLES {
            if self.registers.read(Register::Status) & (1 << 1) != 0 {
                return Ok((self.read_conversion(channel)?, trigger));
            }
            if cycle_count().wrapping_sub(trigger) >= PERIOD_CYCLES {
                break;
            }
        }
        Err(Error::ConversionTimeout)
    }

    pub const fn calibration_value(&self) -> u8 {
        self.calibration_value
    }
}

fn wait_until(deadline: u32, cycle_count: &mut impl FnMut() -> u32) -> Result<(), Error> {
    // Iteration cap also bounds the wait if the cycle counter stops.
    for _ in 0..PERIOD_CYCLES {
        let lateness = cycle_count().wrapping_sub(deadline) as i32;
        if lateness >= 0 {
            return if lateness as u32 <= MAX_LATE_CYCLES {
                Ok(())
            } else {
                Err(Error::Timing)
            };
        }
    }
    Err(Error::Timing)
}

impl<R: Registers, T: PollTiming> Drop for Engine<R, T> {
    fn drop(&mut self) {
        if !self.faulted {
            self.registers.write(Register::Control2, 0);
        }
    }
}
