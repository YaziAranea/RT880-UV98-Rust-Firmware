//! Opt-in PAC binding. All constructors require a separately qualified board.
use crate::{
    engine::{ConfigRegister, GuardedControl, PinEngine, PinError, Port, Registers},
    ControlLevels, Interlock,
};
use at32f4xx_pac::at32f423::{CRM, GPIOA, GPIOC, GPIOF};
use uv98_board_pins::{RfControlPins, RfPrimaryPins, RfSecondaryPins};
use uv98_receive::transport::{AudioControl, BoardControl};
use uv98_rf_bus::{Bus, Delay, Direction, Pins, Profile};

struct Mmio<C> {
    _capability: C,
}
fn base(port: Port) -> *mut u8 {
    match port {
        Port::A => GPIOA::ptr() as *mut u8,
        Port::C => GPIOC::ptr() as *mut u8,
        Port::F => GPIOF::ptr() as *mut u8,
    }
}
impl<C> Registers for Mmio<C> {
    fn read_config(&mut self, port: Port, register: ConfigRegister) -> u32 {
        // Constructors establish ownership, power/clock and serialization.
        unsafe { core::ptr::read_volatile(base(port).add(register as usize).cast::<u32>()) }
    }
    fn write_config(&mut self, port: Port, register: ConfigRegister, value: u32) {
        unsafe { core::ptr::write_volatile(base(port).add(register as usize).cast::<u32>(), value) }
    }
    fn set_bits(&mut self, port: Port, mask: u16) {
        // GPIO SCR +18h: set only these output latch bits; no ODT RMW.
        unsafe { core::ptr::write_volatile(base(port).add(0x18).cast::<u32>(), u32::from(mask)) }
    }
    fn clear_bits(&mut self, port: Port, mask: u16) {
        // GPIO CLR +28h: clear only these output latch bits.
        unsafe { core::ptr::write_volatile(base(port).add(0x28).cast::<u32>(), u32::from(mask)) }
    }
    fn read_input(&mut self, port: Port) -> u32 {
        unsafe { core::ptr::read_volatile(base(port).add(0x10).cast::<u32>()) }
    }
}

fn enable_acf(crm: &mut CRM) {
    crm.ahben1()
        .modify(|_, w| w.gpioa().set_bit().gpioc().set_bit().gpiof().set_bit());
    let _ = crm.ahben1().read().bits();
}
fn enable_c(crm: &mut CRM) {
    crm.ahben1().modify(|_, w| w.gpioc().set_bit());
    let _ = crm.ahben1().read().bits();
}
fn enable_f(crm: &mut CRM) {
    crm.ahben1().modify(|_, w| w.gpiof().set_bit());
    let _ = crm.ahben1().read().bits();
}

pub struct PrimaryPins {
    engine: PinEngine<Mmio<RfPrimaryPins>>,
}
pub struct SecondaryPins {
    engine: PinEngine<Mmio<RfSecondaryPins>>,
}

impl PrimaryPins {
    /// Take over the A bus only to issue the diagnostic shutdown sequence.
    /// Unlike `from_prepared_quiescent`, this does not require the RF chip to
    /// have already reached its sleep register state.
    ///
    /// # Safety
    /// The reference pin mapping, powered RF bus, oscillator/timing limits and
    /// GPIO electrical properties must be appropriate to the board. Early
    /// startup must already assert PC15/PB11/PC5/PF8 LOW, exclude inherited
    /// IRQ/DMA/peripheral pin drivers and provide exclusive serialized GPIO
    /// ownership. Those external requests are not proof of inactive V/U PA
    /// gates: A33 may still contain inherited TX bits, and the next RF operation
    /// must be the reviewed diagnostic off sequence. DATA is released before
    /// CS is driven HIGH; an inherited partial transaction is abandoned, not
    /// resumed. No command is clocked by this constructor. Stable clocks and a
    /// separately selected bounded Delay are required throughout bus use.
    /// This path cannot establish safe RF state from arbitrary damaged wiring.
    pub unsafe fn from_diagnostic_takeover(pins: RfPrimaryPins, crm: &mut CRM) -> Self {
        enable_acf(crm);
        Self {
            engine: PinEngine::prepare(Mmio { _capability: pins }, Profile::A),
        }
    }

    /// Configure only PA8 CS / PF6 SCK / PC9 DATA, with inactive CS HIGH.
    /// This is a new quiescent setup order; stock boot initially cleared CS.
    ///
    /// # Safety
    /// Caller must establish exact AT32F423 wiring, independently inactive PA/
    /// TX paths, powered and already quiescent RF chips, and CS HIGH before
    /// entry. Pins must be unlocked; no alternate peripheral may drive them.
    /// GPIO/CRM RMW must be serialized with ALL port/CRM users, including other
    /// disjoint pin backends. No IRQ/DMA/raw access may compete for these pins.
    /// Clock/power, ownership and serialization must remain valid for the
    /// lifetime and rf_bus Drop. The exact immediate SCK-high-to-IDT sampling and an external
    /// rf_bus::Delay must be qualified for the CPU/board/interrupt behavior.
    /// No delay, analog settling, RF reset, chip enable or loader takeover is
    /// established here. Initial DATA latch LOW is used only with CS inactive.
    /// Preparation releases DATA first, then preloads latches before outputs.
    /// Use into_bus() or retain Profile::A for this binding's lifetime; public
    /// Pins access does not make a manually mislabeled Bus physically valid.
    pub unsafe fn from_prepared_quiescent(pins: RfPrimaryPins, crm: &mut CRM) -> Self {
        enable_acf(crm);
        Self {
            engine: PinEngine::prepare(Mmio { _capability: pins }, Profile::A),
        }
    }
    /// Bind the known profile without a caller-supplied A/B selector.
    pub fn into_bus<D: Delay>(self, delay: D) -> Bus<Self, D> {
        Bus::new(self, delay, Profile::A)
    }
}
impl SecondaryPins {
    /// Take over B solely for diagnostic shutdown after early external LOW
    /// requests. The chip need not have already received its sleep words.
    ///
    /// # Safety
    /// All `PrimaryPins::from_diagnostic_takeover` hardware, ownership, early
    /// gate, serialization and timing requirements apply to this B pin mapping.
    /// The first subsequent RF operations must belong to diagnostic shutdown.
    /// No RX/TX init, RF reset or physical inactivity is established here.
    pub unsafe fn from_diagnostic_takeover(pins: RfSecondaryPins, crm: &mut CRM) -> Self {
        enable_c(crm);
        Self {
            engine: PinEngine::prepare(Mmio { _capability: pins }, Profile::B),
        }
    }

    /// Configure only PC7 CS / PC8 SCK / PC6 DATA, inactive CS HIGH.
    ///
    /// # Safety
    /// Same complete qualification requirements as PrimaryPins construction:
    /// exact wiring, inactive PA/TX, powered quiescent RF chip, CS already HIGH,
    /// unlocked pins, stable clocks, serialization with all GPIOC/CRM users,
    /// no IRQ/DMA competitor, qualified sample timing and external Delay.
    /// Initial DATA LOW/output is prepared; profile B subsequently releases
    /// DATA after reads and restores output at the next transaction start.
    /// Use into_bus() or retain Profile::B for this binding's lifetime.
    pub unsafe fn from_prepared_quiescent(pins: RfSecondaryPins, crm: &mut CRM) -> Self {
        enable_c(crm);
        Self {
            engine: PinEngine::prepare(Mmio { _capability: pins }, Profile::B),
        }
    }
    pub fn into_bus<D: Delay>(self, delay: D) -> Bus<Self, D> {
        Bus::new(self, delay, Profile::B)
    }
}

macro_rules! pins_impl {
    ($name:ident) => {
        impl Pins for $name {
            type Error = PinError;
            fn chip_select(&mut self, high: bool) -> Result<(), PinError> {
                self.engine.chip_select(high)
            }
            fn clock(&mut self, high: bool) -> Result<(), PinError> {
                self.engine.clock(high)
            }
            fn data_level(&mut self, high: bool) -> Result<(), PinError> {
                self.engine.data_level(high)
            }
            fn data_direction(&mut self, direction: Direction) -> Result<(), PinError> {
                self.engine.data_direction(direction)
            }
            fn read_data(&mut self) -> Result<bool, PinError> {
                self.engine.read_data()
            }
        }
    };
}
pins_impl!(PrimaryPins);
pins_impl!(SecondaryPins);

/// Owns PF0/PF8 only. PC15, PB11, PC4/5 and RF register33 are untouched.
pub struct Control<G: Interlock> {
    engine: GuardedControl<Mmio<RfControlPins>, G>,
}
impl<G: Interlock> Control<G> {
    /// Preload explicit caller-qualified latch levels, then enable outputs.
    ///
    /// # Safety
    /// Exact PF0/PF8 wiring, supply, independently inactive PA/TX, stable clocks,
    /// unlocked pins, and exclusive/serialized GPIOF/CRM access are required.
    /// `levels` must match the already qualified physical control state, so
    /// initialization does not unexpectedly move the relay or alter audio.
    /// No RF/relay timing, physical polarity or settled state is inferred.
    /// `interlock` must be observational, bounded and qualified, never default
    /// permissive. It must not access these owned registers through aliases.
    /// GPIO ownership/clock/power remain valid even if the later receive/PTT
    /// interlock fails: PF8 LOW remains available as a mute request in that
    /// state. No gate clear or physical-silence guarantee is implied.
    pub unsafe fn from_prepared_quiescent(
        pins: RfControlPins,
        crm: &mut CRM,
        levels: ControlLevels,
        interlock: G,
    ) -> Self {
        enable_f(crm);
        Self {
            engine: GuardedControl::prepare(Mmio { _capability: pins }, levels, interlock),
        }
    }

    /// Raw PF8 level, not a physical mute guarantee. Stock audio-request paths
    /// use LOW when requests clear and HIGH when active. HIGH needs the caller
    /// interlock; LOW remains available for an abortive mute request.
    pub fn set_af_mute_level(&mut self, high: bool) -> Result<(), G::Error> {
        self.engine.set_af_mute_level(high)
    }
}
impl<G: Interlock> BoardControl for Control<G> {
    type Error = G::Error;
    fn check_receive_preconditions(&mut self) -> Result<(), G::Error> {
        self.engine.check_receive_preconditions()
    }
    fn set_antenna_pf0(&mut self, high: bool) -> Result<(), G::Error> {
        self.engine.set_antenna_pf0(high)
    }
}
impl<G: Interlock> AudioControl for Control<G> {
    fn set_audio_mute(&mut self, muted: bool) -> Result<(), G::Error> {
        self.engine.set_audio_mute(muted)
    }
}
