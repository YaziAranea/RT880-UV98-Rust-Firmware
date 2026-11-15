//! Register protocol shared by host fault tests and MMIO backend.
use crate::{Error, PollTiming, Sample};

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
const SIDE_KEY_SAMPLE_TIME: u32 = 7 << (3 * (SIDE_KEY_CHANNEL - 10));
const BATTERY_SAMPLE_TIME: u32 = 7 << (3 * BATTERY_CHANNEL);

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
        use Register::*;
        self.check_conversion_state(channel)?;
        self.registers.write(Status, !0x3f);
        self.registers.write(Control2, 1 | (1 << 30));
        self.wait(Status, 1 << 1, true, Error::ConversionTimeout)?;
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

    pub const fn calibration_value(&self) -> u8 {
        self.calibration_value
    }
}

impl<R: Registers, T: PollTiming> Drop for Engine<R, T> {
    fn drop(&mut self) {
        if !self.faulted {
            self.registers.write(Register::Control2, 0);
        }
    }
}
