//! Opt-in PAC binding. Construction is unsafe until the board is qualified.
use crate::{
    engine::{Engine, Registers},
    BusTiming, Config, Error, GpioRegisters,
};
use at32f4xx_pac::at32f423::{CRM, GPIOA, GPIOB, GPIOC, SPI3};
use uv98_board_pins::LcdPins;
use uv98_display::DisplayBus;

struct Mmio {
    spi: SPI3,
    _pins: LcdPins,
}
impl Registers for Mmio {
    fn status(&mut self) -> u32 {
        self.spi.sts().read().bits()
    }
    fn transmit(&mut self, byte: u8) {
        // Reviewed stock helper uses a 32-bit DT store at offset0x0C.
        unsafe {
            core::ptr::write_volatile(
                (SPI3::ptr() as *mut u8).add(0x0C).cast::<u32>(),
                u32::from(byte),
            )
        };
    }
    fn chip_select(&mut self, active: bool) {
        let gpio = unsafe { &*GPIOA::ptr() };
        if active {
            gpio.clr().write(|w| unsafe { w.bits(1 << 15) });
        } else {
            gpio.scr().write(|w| unsafe { w.bits(1 << 15) });
        }
    }
    fn data_mode(&mut self, data: bool) {
        let gpio = unsafe { &*GPIOC::ptr() };
        if data {
            gpio.scr().write(|w| unsafe { w.bits(1 << 13) });
        } else {
            gpio.clr().write(|w| unsafe { w.bits(1 << 13) });
        }
    }
    fn reset_level(&mut self, high: bool) {
        let gpio = unsafe { &*GPIOC::ptr() };
        if high {
            gpio.scr().write(|w| unsafe { w.bits(1 << 14) });
        } else {
            gpio.clr().write(|w| unsafe { w.bits(1 << 14) });
        }
    }
    fn disable(&mut self) {
        self.spi
            .ctrl1()
            .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 6)) });
    }
}

/// Owns SPI3 and the disjoint PA15/PB3/PB5/PC13/PC14 capability.
/// Backlight PA4, PB4 and all NOR pins are untouched. Faults poison this value;
/// reset() does not repair transport faults. Recover only by a newly qualified
/// backend construction or reboot, not by blindly retrying display initialize.
pub struct Spi3Lcd<T: BusTiming> {
    engine: Engine<Mmio, T>,
}

impl<T: BusTiming> Spi3Lcd<T> {
    /// Configure a quiescent bus; no LCD command, power or backlight operation.
    ///
    /// # Safety
    /// Caller must establish the AT32F423 and listed LCD wiring, a compatible
    /// 4-line8-bit panel interface (including actual IM straps and E7 behavior),
    /// and the selected clock-mode waveform/SCK from stable APB1/divider. Establish
    /// RF/PA inactivity separately. SPI3 must be quiescent, CS inactive and RESET
    /// high before entry. No DMA/IRQ/other execution context may access SPI3 or
    /// these pins. GPIO configuration RMW and CRM access require serialization
    /// against ALL other users of the same port/CRM, even disjoint pin owners.
    /// Gates/clocks/power must remain valid for the lifetime and Drop. `timing`
    /// must meet the actual panel's DC/CS timing and must not panic. Reset pulse
    /// timing is supplied separately by uv98_display::Delay. This is not a
    /// validated loader handoff and is not called by Stage0.
    pub unsafe fn from_quiescent(
        spi: SPI3,
        pins: LcdPins,
        crm: &mut CRM,
        config: Config,
        timing: T,
    ) -> Self {
        crm.ahben1()
            .modify(|_, w| w.gpioa().set_bit().gpiob().set_bit().gpioc().set_bit());
        crm.apb1en().modify(|_, w| w.spi3().set_bit());
        let _ = crm.ahben1().read().bits();
        let _ = crm.apb1en().read().bits();
        let a = unsafe { &*GPIOA::ptr() };
        let b = unsafe { &*GPIOB::ptr() };
        let c = unsafe { &*GPIOC::ptr() };
        a.scr().write(|w| unsafe { w.bits(1 << 15) });
        c.scr().write(|w| unsafe { w.bits((1 << 13) | (1 << 14)) });
        let read_a = GpioRegisters {
            cfgr: a.cfgr().read().bits(),
            omode: a.omode().read().bits(),
            odrvr: a.odrvr().read().bits(),
            pull: a.pull().read().bits(),
            muxl: a.muxl().read().bits(),
            muxh: a.muxh().read().bits(),
        }
        .with_cs();
        let read_b = GpioRegisters {
            cfgr: b.cfgr().read().bits(),
            omode: b.omode().read().bits(),
            odrvr: b.odrvr().read().bits(),
            pull: b.pull().read().bits(),
            muxl: b.muxl().read().bits(),
            muxh: b.muxh().read().bits(),
        }
        .with_spi3();
        let read_c = GpioRegisters {
            cfgr: c.cfgr().read().bits(),
            omode: c.omode().read().bits(),
            odrvr: c.odrvr().read().bits(),
            pull: c.pull().read().bits(),
            muxl: c.muxl().read().bits(),
            muxh: c.muxh().read().bits(),
        }
        .with_control();
        a.omode().write(|w| unsafe { w.bits(read_a.omode) });
        a.odrvr().write(|w| unsafe { w.bits(read_a.odrvr) });
        a.pull().write(|w| unsafe { w.bits(read_a.pull) });
        a.cfgr().write(|w| unsafe { w.bits(read_a.cfgr) });
        c.omode().write(|w| unsafe { w.bits(read_c.omode) });
        c.odrvr().write(|w| unsafe { w.bits(read_c.odrvr) });
        c.pull().write(|w| unsafe { w.bits(read_c.pull) });
        c.cfgr().write(|w| unsafe { w.bits(read_c.cfgr) });
        crm.apb1rst().modify(|_, w| w.spi3().set_bit());
        crm.apb1rst().modify(|_, w| w.spi3().clear_bit());
        let _ = crm.apb1rst().read().bits();
        b.omode().write(|w| unsafe { w.bits(read_b.omode) });
        b.odrvr().write(|w| unsafe { w.bits(read_b.odrvr) });
        b.pull().write(|w| unsafe { w.bits(read_b.pull) });
        b.muxl().write(|w| unsafe { w.bits(read_b.muxl) });
        spi.ctrl2().write(|w| unsafe { w.bits(0) });
        spi.i2sctrl().write(|w| unsafe { w.bits(0) });
        // Establish clock mode while disabled and CS is high. Expose the mux
        // before enabling SPI; do not change SCK ownership on an enabled bus.
        let control = config.ctrl1();
        spi.ctrl1()
            .write(|w| unsafe { w.bits(control & !(1 << 6)) });
        b.cfgr().write(|w| unsafe { w.bits(read_b.cfgr) });
        spi.ctrl1().write(|w| unsafe { w.bits(control) });
        let _ = spi.ctrl1().read().bits();
        #[cfg(target_arch = "arm")]
        // SAFETY: supported ARM target; complete peripheral configuration
        // before any LCD reset or command, without touching memory or stack.
        unsafe {
            core::arch::asm!("dsb", options(nostack, preserves_flags));
        }
        Self {
            engine: Engine::new(Mmio { spi, _pins: pins }, timing, config.poll_budget),
        }
    }
}

impl<T: BusTiming> DisplayBus for Spi3Lcd<T> {
    type Error = Error;
    fn reset(&mut self, high: bool) -> Result<(), Error> {
        self.engine.reset(high)
    }
    fn command(&mut self, command: u8) -> Result<(), Error> {
        self.engine.command(command)
    }
    fn data(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.engine.data(bytes)
    }
    fn pixels_rgb565(&mut self, pixels: &[u16]) -> Result<(), Error> {
        self.engine.pixels_rgb565(pixels)
    }
}
