//! PAC capability binding for diagnostic external LOW requests and backlight.
use crate::{
    control::{Engine, Error, Port, Register, Registers},
    Backlight, ExternalOff,
};
use at32f4xx_pac::at32f423::{CRM, GPIOA, GPIOB, GPIOC, GPIOF};
use uv98_board_pins::DiagnosticControlPins;

struct Mmio {
    _pins: DiagnosticControlPins,
}
fn base(port: Port) -> *mut u8 {
    match port {
        Port::A => GPIOA::ptr() as *mut u8,
        Port::B => GPIOB::ptr() as *mut u8,
        Port::C => GPIOC::ptr() as *mut u8,
        Port::F => GPIOF::ptr() as *mut u8,
    }
}
impl Registers for Mmio {
    fn read(&mut self, port: Port, register: Register) -> u32 {
        unsafe { core::ptr::read_volatile(base(port).add(register as usize).cast::<u32>()) }
    }
    fn configure(&mut self, port: Port, register: Register, value: u32) {
        // Engine only calls this for mode/type/drive/pull, never whole ODT.
        unsafe { core::ptr::write_volatile(base(port).add(register as usize).cast::<u32>(), value) }
    }
    fn clear(&mut self, port: Port, pins: u16) {
        unsafe { core::ptr::write_volatile(base(port).add(0x28).cast::<u32>(), u32::from(pins)) }
    }
    fn set(&mut self, port: Port, pins: u16) {
        unsafe { core::ptr::write_volatile(base(port).add(0x18).cast::<u32>(), u32::from(pins)) }
    }
}
pub struct Control {
    engine: Engine<Mmio>,
}
impl Control {
    /// Consumes this driver after Shutdown has ended. No output is changed;
    /// a subsequent operational driver must revalidate and acquire the pins.
    pub fn into_pins(self) -> DiagnosticControlPins {
        self.engine.into_registers()._pins
    }
    /// Construct a persistent owner after the raw early-startup phase ends.
    /// No output is enabled here; run_once/assert_off reasserts the LOW requests.
    ///
    /// # Safety
    /// Early startup must already assert PA4/PB11/PC4/PC5/PC15/PF8 LOW with
    /// appropriate GPIO configuration. Exact reference wiring/polarities and
    /// board power remain hardware assumptions, not proved by this token.
    /// LEXT/alternate functions, IRQ/DMA and any competing raw pin driver must
    /// be stopped; GPIO RMW must remain serialized with all other port users.
    /// In particular no early-startup raw GPIO helper may be used while this
    /// owner lives. Clock gates must remain on and pin configuration unlocked.
    /// PF0 is deliberately not touched; this owner does not move the relay.
    pub unsafe fn from_early_low_requests(pins: DiagnosticControlPins, crm: &mut CRM) -> Self {
        crm.ahben1().modify(|_, w| {
            w.gpioa()
                .set_bit()
                .gpiob()
                .set_bit()
                .gpioc()
                .set_bit()
                .gpiof()
                .set_bit()
        });
        let _ = crm.ahben1().read().bits();
        Self {
            engine: Engine::new(Mmio { _pins: pins }),
        }
    }
}
impl ExternalOff for Control {
    type Error = Error;
    fn assert_off(&mut self) -> Result<(), Error> {
        self.engine.assert_off()
    }
    fn check_off(&mut self) -> Result<(), Error> {
        self.engine.check_off()
    }
}
impl Backlight for Control {
    fn set_backlight(&mut self, on: bool) -> Result<(), Error> {
        self.engine.set_backlight(on)
    }
}
