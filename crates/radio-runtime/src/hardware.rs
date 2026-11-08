//! Exclusive foreground binding. Physical outputs remain experimental until
//! measured; this checks GPIO configuration/latches, not voltage or RF power.
use crate::gpio_control::{Engine, Port, Register, Registers};
use at32f4xx_pac::at32f423::{CRM, GPIOA, GPIOB, GPIOC, GPIOF};
use uv98_board_pins::RadioControlPins;
use uv98_diag_quiesce::RegisterIo;

pub use crate::gpio_control::ControlError;
#[derive(Debug, PartialEq, Eq)]
pub enum Error<E> {
    Bus(E),
    Control(ControlError),
}

struct Mmio;
fn base(port: Port) -> *const u8 {
    match port {
        Port::A => GPIOA::ptr().cast(),
        Port::B => GPIOB::ptr().cast(),
        Port::C => GPIOC::ptr().cast(),
        Port::F => GPIOF::ptr().cast(),
    }
}
impl Registers for Mmio {
    fn read(&self, port: Port, register: Register) -> u32 {
        // SAFETY: from_shutdown establishes clocks and exclusive serialized
        // ownership; the shared policy uses only documented aligned registers.
        unsafe { core::ptr::read_volatile(base(port).add(register as usize).cast::<u32>()) }
    }
    fn configure(&mut self, port: Port, register: Register, value: u32) {
        // The policy only passes Mode/OutputType/Drive/Pull, with unrelated fields
        // preserved. It never uses this method to write a whole output latch.
        unsafe {
            core::ptr::write_volatile(
                base(port).add(register as usize).cast_mut().cast::<u32>(),
                value,
            )
        }
    }
    fn clear(&mut self, port: Port, pins: u16) {
        // SAFETY: CLR changes only the exclusively owned pins selected by policy.
        unsafe {
            core::ptr::write_volatile(
                base(port).add(0x28).cast_mut().cast::<u32>(),
                u32::from(pins),
            )
        }
    }
    fn set(&mut self, port: Port, pins: u16) {
        // SAFETY: SCR low-half atomically sets only the selected owned pins.
        unsafe {
            core::ptr::write_volatile(
                base(port).add(0x18).cast_mut().cast::<u32>(),
                u32::from(pins),
            )
        }
    }
}

pub struct Control {
    _pins: RadioControlPins,
    engine: Engine<Mmio>,
}
impl Control {
    /// # Safety
    /// Diagnostic shutdown must have succeeded and been dropped before entry.
    /// Reference wiring, valid clocks, no LEXT/IRQ/DMA competing drivers and
    /// serialized GPIO ownership are required for this entire driver's lifetime.
    /// PA9 keypad light and PA13/PA14 indicators are included. PA13/PA14 are
    /// explicitly configured as GPIO outputs; no SWD debugger may use them.
    /// PC3 is also exclusively owned: preload LOW, then GPIO output selects
    /// U501's ordinary MICI path. Its LOW latch/configuration are checked by
    /// verify_control before runtime RF steps; external BT-MICI is not enabled.
    pub unsafe fn from_shutdown(
        pins: RadioControlPins,
        crm: &mut CRM,
    ) -> Result<Self, ControlError> {
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
        let engine = Engine::initialize(Mmio)?;
        Ok(Self {
            _pins: pins,
            engine,
        })
    }
    pub fn verify(&self) -> Result<(), ControlError> {
        self.engine.verify()
    }
    pub fn mute(&mut self, muted: bool) -> Result<(), ControlError> {
        self.engine.mute(muted)
    }
    pub fn antenna(&mut self, high: bool) -> Result<(), ControlError> {
        self.engine.antenna(high)
    }
    pub fn hf_gates(&mut self, hf: bool, band50: bool) -> Result<(), ControlError> {
        self.engine.hf_gates(hf, band50)
    }
    pub fn indicators(&mut self, rx: bool, tx: bool) -> Result<(), ControlError> {
        self.engine.indicators(rx, tx)
    }
    pub fn backlights(&mut self, display: bool, keys: bool) -> Result<(), ControlError> {
        self.engine.backlights(display, keys)
    }
}

pub struct RadioIo<A: RegisterIo> {
    primary: A,
    control: Control,
}
impl<A: RegisterIo> RadioIo<A> {
    pub fn new(primary: A, control: Control) -> Self {
        Self { primary, control }
    }
}
impl<A: RegisterIo> super::Io for RadioIo<A> {
    type Error = Error<A::Error>;
    fn read(&mut self, r: u8) -> Result<u16, Self::Error> {
        self.primary.read(r).map_err(Error::Bus)
    }
    fn write(&mut self, r: u8, v: u16) -> Result<(), Self::Error> {
        self.primary.write(r, v).map_err(Error::Bus)
    }
    fn mute(&mut self, m: bool) -> Result<(), Self::Error> {
        self.control.mute(m).map_err(Error::Control)
    }
    fn antenna(&mut self, h: bool) -> Result<(), Self::Error> {
        self.control.antenna(h).map_err(Error::Control)
    }
    fn hf_gates(&mut self, h: bool, b: bool) -> Result<(), Self::Error> {
        self.control.hf_gates(h, b).map_err(Error::Control)
    }
    fn verify_control(&mut self) -> Result<(), Self::Error> {
        self.control.verify().map_err(Error::Control)
    }
    fn indicators(&mut self, rx: bool, tx: bool) -> Result<(), Self::Error> {
        self.control.indicators(rx, tx).map_err(Error::Control)
    }
    fn backlights(&mut self, display: bool, keys: bool) -> Result<(), Self::Error> {
        self.control
            .backlights(display, keys)
            .map_err(Error::Control)
    }
}
