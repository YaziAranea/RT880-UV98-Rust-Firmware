//! AT32F423 PAC binding. Construction is explicitly unsafe because software
//! cannot establish the physical board, clock limits or radio quiescence.

use crate::{
    engine::{Engine, Registers},
    Config, Error, GpioRegisters,
};
use at32f4xx_pac::at32f423::{CRM, GPIOB, SPI2};
use uv98_board_pins::NorPins;
use uv98_nor_read::Bus;

const CS: u32 = 1 << 12;

struct Mmio {
    spi: SPI2,
    _pins: NorPins,
}

impl Registers for Mmio {
    fn status(&mut self) -> u32 {
        self.spi.sts().read().bits()
    }
    fn transmit(&mut self, byte: u8) {
        // The vendor library and reviewed stock helper use a 32-bit DT access.
        // PAC 0.4.0 exposes DT as u16; retain the observed access width locally.
        unsafe {
            core::ptr::write_volatile(
                (SPI2::ptr() as *mut u8).add(0x0c).cast::<u32>(),
                byte as u32,
            )
        };
    }
    fn receive(&mut self) -> u8 {
        unsafe {
            core::ptr::read_volatile((SPI2::ptr() as *const u8).add(0x0c).cast::<u32>()) as u8
        }
    }
    fn chip_select(&mut self, active: bool) {
        // NorPins owns PB12. Other disjoint GPIOB fields may be owned by LCD.
        let gpio = unsafe { &*GPIOB::ptr() };
        if active {
            gpio.clr().write(|w| unsafe { w.bits(CS) });
        } else {
            gpio.scr().write(|w| unsafe { w.bits(CS) });
        }
    }
    fn disable(&mut self) {
        self.spi
            .ctrl1()
            .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 6)) });
    }
}

/// Exclusive SPI2/PB12..15 read-only NOR transport. Owns disjoint NorPins; it
/// changes no other GPIOB pin's configuration/output latch. This capability is
/// !Send/!Sync so shared GPIO register updates remain in one execution context.
pub struct Spi2Nor {
    engine: Engine<Mmio>,
}

impl Spi2Nor {
    /// Enable GPIOB/SPI2 gates, reset SPI2 alone, configure the related board
    /// profile, and leave CS high. Does not issue a NOR command.
    ///
    /// # Safety
    /// Caller must establish AT32F423 and the PB12..15 wiring/compatible NOR;
    /// stable APB1 clock with a permitted SCK after `config.divider`; valid chip
    /// power-up and CS setup/hold/inactive timing; exclusive, quiescent SPI2 and
    /// pins; no DMA/IRQ/other core access or conflicting CRM/GPIO modification throughout
    /// use; and RF/PA inactivity independently of this driver. PB12 must already
    /// be inactive or held inactive externally before entry. This is not a
    /// validated handoff from the user's loader. Poll budget is not wall time.
    ///
    /// The caller must keep these clock/ownership/timing conditions true until
    /// this value is dropped. Resetting SPI2 can abort prior bus activity, hence
    /// the quiescence precondition. It does not reset GPIOB or other peripherals.
    /// Disjoint LCD GPIO updates are permitted only when serialized with this
    /// backend on the same foreground context; whole-port resets are forbidden.
    pub unsafe fn from_quiescent(spi: SPI2, pins: NorPins, crm: &mut CRM, config: Config) -> Self {
        let gpio = unsafe { &*GPIOB::ptr() };
        crm.ahben1().modify(|_, w| w.gpiob().set_bit());
        crm.apb1en().modify(|_, w| w.spi2().set_bit());
        let _ = crm.ahben1().read().bits();
        let _ = crm.apb1en().read().bits();

        // Preload CS high before changing its output mode. Stock writes this
        // latch after pin configuration; the deliberate order avoids a low
        // latch on the input-to-output transition under the preconditions.
        gpio.scr().write(|w| unsafe { w.bits(CS) });
        crm.apb1rst().modify(|_, w| w.spi2().set_bit());
        crm.apb1rst().modify(|_, w| w.spi2().clear_bit());
        let _ = crm.apb1rst().read().bits();

        let registers = GpioRegisters {
            cfgr: gpio.cfgr().read().bits(),
            omode: gpio.omode().read().bits(),
            odrvr: gpio.odrvr().read().bits(),
            pull: gpio.pull().read().bits(),
            muxh: gpio.muxh().read().bits(),
        }
        .with_spi2_nor();
        gpio.omode().write(|w| unsafe { w.bits(registers.omode) });
        gpio.odrvr().write(|w| unsafe { w.bits(registers.odrvr) });
        gpio.pull().write(|w| unsafe { w.bits(registers.pull) });
        gpio.muxh().write(|w| unsafe { w.bits(registers.muxh) });
        // Keep CS high and SPI disabled while changing SCK/MOSI pin modes.
        // Enable separately after mux ownership, as in the stock/SDK sequence.
        spi.ctrl2().write(|w| unsafe { w.bits(0) });
        spi.i2sctrl().write(|w| unsafe { w.bits(0) });
        let control = config.ctrl1();
        spi.ctrl1()
            .write(|w| unsafe { w.bits(control & !(1 << 6)) });
        gpio.cfgr().write(|w| unsafe { w.bits(registers.cfgr) });
        spi.ctrl1().write(|w| unsafe { w.bits(control) });
        let _ = spi.ctrl1().read().bits();
        #[cfg(target_arch = "arm")]
        // SAFETY: the supported ARM target provides DSB; no registers or stack
        // are changed, and this completes configuration before the first CS.
        unsafe {
            core::arch::asm!("dsb", options(nostack, preserves_flags));
        }
        Self {
            engine: Engine::new(Mmio { spi, _pins: pins }, config.poll_budget),
        }
    }
}

impl Bus for Spi2Nor {
    type Error = Error;
    fn select(&mut self) -> Result<(), Error> {
        self.engine.select()
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.engine.write(bytes)
    }
    fn read(&mut self, destination: &mut [u8]) -> Result<(), Error> {
        self.engine.read(destination)
    }
    fn deselect(&mut self) -> Result<(), Error> {
        self.engine.deselect()
    }
}
