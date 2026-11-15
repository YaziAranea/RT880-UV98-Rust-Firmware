//! AT32F423 SDK register mapping; physical key assignment needs live raw logs.
use crate::{
    engine::{Engine, Register, Registers},
    Error, PollTiming, Sample,
};
use at32f4xx_pac::at32f423::{gpioa, ADC1, ADCCOM, CRM, GPIOA, GPIOC};
use uv98_board_pins::{BatteryAdcPins, SideAdcPins, BATTERY_ADC_A_MASK, SIDE_ADC_C_MASK};

struct Mmio {
    adc: ADC1,
    _common: ADCCOM,
    _side_pins: SideAdcPins,
    _battery_pins: BatteryAdcPins,
}

impl Registers for Mmio {
    fn read(&mut self, register: Register) -> u32 {
        match register {
            Register::Status => self.adc.sts().read().bits(),
            Register::Control1 => self.adc.ctrl1().read().bits(),
            Register::Control2 => self.adc.ctrl2().read().bits(),
            Register::SampleTime1 => self.adc.spt1().read().bits(),
            Register::SampleTime2 => self.adc.spt2().read().bits(),
            Register::Sequence1 => self.adc.osq1().read().bits(),
            Register::Sequence2 => self.adc.osq2().read().bits(),
            Register::Sequence3 => self.adc.osq3().read().bits(),
            Register::Oversampling => self.adc.ovsp().read().bits(),
            Register::Data => self.adc.odt().read().bits(),
            Register::CalibrationValue => self.adc.calval().read().bits(),
            // F423 SDK adc_type::misc offsetD0 is missing from PAC0.4.0.
            Register::Misc => unsafe {
                core::ptr::read_volatile((ADC1::ptr() as usize + 0xd0) as *const u32)
            },
        }
    }
    fn write(&mut self, register: Register, value: u32) {
        match register {
            Register::Status => {
                self.adc.sts().write(|w| unsafe { w.bits(value) });
            }
            Register::Control1 => {
                self.adc.ctrl1().write(|w| unsafe { w.bits(value) });
            }
            Register::Control2 => {
                self.adc.ctrl2().write(|w| unsafe { w.bits(value) });
            }
            Register::SampleTime1 => {
                self.adc.spt1().write(|w| unsafe { w.bits(value) });
            }
            Register::SampleTime2 => {
                self.adc.spt2().write(|w| unsafe { w.bits(value) });
            }
            Register::Sequence1 => {
                self.adc.osq1().write(|w| unsafe { w.bits(value) });
            }
            Register::Sequence2 => {
                self.adc.osq2().write(|w| unsafe { w.bits(value) });
            }
            Register::Sequence3 => {
                self.adc.osq3().write(|w| unsafe { w.bits(value) });
            }
            Register::Oversampling => {
                self.adc.ovsp().write(|w| unsafe { w.bits(value) });
            }
            Register::Misc => unsafe {
                core::ptr::write_volatile((ADC1::ptr() as usize + 0xd0) as *mut u32, value)
            },
            Register::Data | Register::CalibrationValue => {
                unreachable!("read-only in this backend")
            }
        }
    }
}

pub struct SideAdc<T: PollTiming> {
    engine: Engine<Mmio, T>,
}

fn configure_analog(gpio: &gpioa::RegisterBlock, pin_mask: u16) -> Result<(), Error> {
    let lock_mask = (1 << 16) | u32::from(pin_mask);
    if gpio.wpr().read().bits() & lock_mask == lock_mask {
        return Err(Error::PinLocked);
    }
    let field = 3 << (pin_mask.trailing_zeros() * 2);
    gpio.pull()
        .modify(|r, w| unsafe { w.bits(r.bits() & !field) });
    gpio.cfgr()
        .modify(|r, w| unsafe { w.bits(r.bits() | field) });
    if gpio.cfgr().read().bits() & field != field || gpio.pull().read().bits() & field != 0 {
        return Err(Error::Configuration);
    }
    Ok(())
}

impl<T: PollTiming> SideAdc<T> {
    /// # Safety
    /// Caller verifies the MCU, PC2/PA7 wiring and voltages, quiescent ADC/DMA,
    /// stable 48 MHz HCLK and exclusive ADC1/ADCCOM/PC2/PA7 ownership. No IRQ,
    /// DMA, debugger or raw alias may access these resources. Shared GPIOA,
    /// GPIOC and CRM register updates are serialized.
    /// Reset affects only the owned ADC block. Other GPIO fields are preserved.
    /// Do not use a sample as PTT B until physical key/raw-code mapping is observed.
    pub unsafe fn from_prepared(
        side_pins: SideAdcPins,
        battery_pins: BatteryAdcPins,
        adc: ADC1,
        common: ADCCOM,
        crm: &mut CRM,
        timing: T,
        budget: u32,
    ) -> Result<Self, Error> {
        if budget == 0 || budget > 100_000 {
            return Err(Error::InvalidBudget);
        }
        crm.ahben1()
            .modify(|_, w| w.gpioa().set_bit().gpioc().set_bit());
        let _ = crm.ahben1().read().bits();
        configure_analog(unsafe { &*GPIOC::ptr() }, SIDE_ADC_C_MASK)?;
        configure_analog(unsafe { &*GPIOA::ptr() }, BATTERY_ADC_A_MASK)?;
        crm.apb2en().modify(|_, w| w.adc().set_bit());
        let _ = crm.apb2en().read().bits();
        // SDK CRM_ADC_CLOCK_SOURCE_HCLK clears MISC1.pllclk_to_adc bit15.
        crm.misc1()
            .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 15)) });
        crm.apb2rst().modify(|_, w| w.adc().set_bit());
        let _ = crm.apb2rst().read().bits();
        crm.apb2rst().modify(|_, w| w.adc().clear_bit());
        let _ = crm.apb2rst().read().bits();
        // ADCCOM adcdiv[19:16]=2 ->HCLK/4=12MHz; internal sensor/reference off.
        common
            .cctrl()
            .modify(|r, w| unsafe { w.bits((r.bits() & !((15 << 16) | (1 << 23))) | (2 << 16)) });
        if common.cctrl().read().bits() & ((15 << 16) | (1 << 23)) != 2 << 16 {
            adc.ctrl2().write(|w| unsafe { w.bits(0) });
            return Err(Error::Configuration);
        }
        let engine = Engine::prepare(
            Mmio {
                adc,
                _common: common,
                _side_pins: side_pins,
                _battery_pins: battery_pins,
            },
            timing,
            budget,
        )?;
        Ok(Self { engine })
    }
    pub fn sample(&mut self) -> Result<Sample, Error> {
        self.engine.sample()
    }
    pub fn sample_battery(&mut self) -> Result<u16, Error> {
        self.engine.sample_battery()
    }
    pub fn calibration_value(&self) -> u8 {
        self.engine.calibration_value()
    }
}
