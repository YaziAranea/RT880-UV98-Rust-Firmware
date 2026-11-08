#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Diagnostic composition, not a reset/bootloader entry. Hardware construction
//! requires an independently prepared and qualified board. No RF shutdown,
//! clock setup, flash command, backlight, GPIO key scanning or IRQ is implied.

use uv98_diagnostic::{render, Diagnostic, LoadError, Snapshot};
use uv98_display::{Delay, Display, DisplayBus};
use uv98_keys::{ButtonEvent, Key};
use uv98_nor_read::Bus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    New,
    Ready,
    Failed,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error<N, L> {
    State,
    Read(LoadError<N>),
    Display(uv98_display::Error<L>),
}

pub struct Session<N, L, D> {
    diagnostic: Diagnostic,
    nor: N,
    display: Display<L, D>,
    state: State,
}
impl<N: Bus, L: DisplayBus, D: Delay> Session<N, L, D> {
    /// Takes already-owned bus implementations; does not access them yet.
    pub fn new(nor: N, lcd: L, delay: D) -> Self {
        Self {
            diagnostic: Diagnostic::new(),
            nor,
            display: Display::new(lcd, delay),
            state: State::New,
        }
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn snapshot(&self) -> Option<Snapshot<'_>> {
        if self.state != State::Ready {
            return None;
        }
        self.diagnostic.snapshot()
    }
    /// Reads/calibrates no RF: only parses the saved block, reads one channel,
    /// initializes LCD and draws stored fields. Any failure closes the session.
    /// Physical screen contents may remain stale on failure; callers must not
    /// treat a retained image as current state. No automatic retry is attempted.
    pub fn start(&mut self, index: usize) -> Result<(), Error<N::Error, L::Error>> {
        if self.state != State::New {
            return Err(Error::State);
        }
        self.state = State::Failed;
        self.diagnostic
            .load_calibration(&mut self.nor)
            .map_err(Error::Read)?;
        self.diagnostic
            .load_channel(&mut self.nor, index)
            .map_err(Error::Read)?;
        self.display.initialize().map_err(Error::Display)?;
        render(
            &mut self.display,
            &self.diagnostic.snapshot().expect("successful reads"),
        )
        .map_err(Error::Display)?;
        self.state = State::Ready;
        Ok(())
    }
    pub fn navigate(
        &mut self,
        key: Key,
        event: ButtonEvent,
    ) -> Result<bool, Error<N::Error, L::Error>> {
        if self.state != State::Ready {
            return Err(Error::State);
        }
        let Some(index) = self.diagnostic.key_target(key, event) else {
            return Ok(false);
        };
        self.state = State::Failed;
        self.diagnostic
            .load_channel(&mut self.nor, index)
            .map_err(Error::Read)?;
        render(
            &mut self.display,
            &self.diagnostic.snapshot().expect("successful channel read"),
        )
        .map_err(Error::Display)?;
        self.state = State::Ready;
        Ok(true)
    }
    pub fn into_parts(self) -> (N, Display<L, D>) {
        (self.nor, self.display)
    }
}

#[cfg(feature = "hardware")]
pub mod hardware {
    use super::*;
    use at32f4xx_pac::at32f423::{CRM, GPIOA, GPIOB, GPIOC, GPIOD, SPI2, SPI3};
    use core::num::NonZeroU32;
    use uv98_board_at32::hardware::Spi2Nor;
    use uv98_board_pins::{BoardPins, KeyscanPins, ReservedPins};
    use uv98_clock_profile::hardware::CycleDelay;
    use uv98_lcd_at32::{hardware::Spi3Lcd, BusTiming, TimingPhase};

    pub struct Resources {
        pub crm: CRM,
        pub gpioa: GPIOA,
        pub gpiob: GPIOB,
        pub gpioc: GPIOC,
        pub gpiod: GPIOD,
        pub spi2: SPI2,
        pub spi3: SPI3,
    }
    #[derive(Clone, Copy)]
    pub struct Timing {
        pub core_upper_hz: NonZeroU32,
        pub setup_us: NonZeroU32,
        pub hold_us: NonZeroU32,
        pub inactive_us: NonZeroU32,
    }
    pub struct MinimumBusTiming {
        delay: CycleDelay,
        timing: Timing,
    }
    impl BusTiming for MinimumBusTiming {
        fn wait(&mut self, phase: TimingPhase) {
            let duration = match phase {
                TimingPhase::Setup => self.timing.setup_us,
                TimingPhase::Hold => self.timing.hold_us,
                TimingPhase::Inactive => self.timing.inactive_us,
            };
            self.delay.wait_us(duration.get());
        }
    }
    #[derive(Debug, PartialEq, Eq)]
    pub enum ConstructionError {
        Clock(uv98_clock_profile::Error),
        UpperBoundBelowNominal,
    }
    pub type HardwareSession = Session<Spi2Nor, Spi3Lcd<MinimumBusTiming>, CycleDelay>;
    pub struct PreparedDiagnostic {
        session: HardwareSession,
        // Retaining CRM prevents safe callers from changing its clock tree.
        _crm: CRM,
        _reserved: ReservedPins,
        _keyscan: KeyscanPins,
    }
    impl PreparedDiagnostic {
        /// Compose LCD and NOR without duplicating GPIO ownership. Clock
        /// mismatch is rejected before any GPIO/peripheral configuration.
        ///
        /// # Safety
        /// All Spi2Nor/Spi3Lcd/CycleDelay constructor preconditions must hold:
        /// exact board wiring, correct panel interface, stable qualified clocks,
        /// valid timing incl. oscillator tolerance and both SPI chip limits,
        /// independent verified RF/PA inactivity, quiescent DMA/IRQ/peripherals,
        /// CS inactive on entry. No callback/interrupt/raw alias may modify the
        /// shared registers while these foreground-only backends run. Existing
        /// core/loader state and recovery must be established by other code.
        /// This function does not execute board-policy RF requests. Possessing
        /// PAC tokens or passing clock-profile checks does not prove RF-off.
        pub unsafe fn from_prepared_board(
            mut resources: Resources,
            nor: uv98_board_at32::Config,
            lcd: uv98_lcd_at32::Config,
            timing: Timing,
        ) -> Result<Self, ConstructionError> {
            let clocks = uv98_clock_profile::hardware::observe(&resources.crm)
                .map_err(ConstructionError::Clock)?;
            if timing.core_upper_hz.get() < clocks.core_hz() {
                return Err(ConstructionError::UpperBoundBelowNominal);
            }
            let pins = BoardPins::split(
                resources.gpioa,
                resources.gpiob,
                resources.gpioc,
                resources.gpiod,
            );
            let nor = unsafe {
                Spi2Nor::from_quiescent(resources.spi2, pins.nor, &mut resources.crm, nor)
            };
            let bus_delay = unsafe { CycleDelay::from_core_upper_bound(timing.core_upper_hz) };
            let lcd = unsafe {
                Spi3Lcd::from_quiescent(
                    resources.spi3,
                    pins.lcd,
                    &mut resources.crm,
                    lcd,
                    MinimumBusTiming {
                        delay: bus_delay,
                        timing,
                    },
                )
            };
            let delay = unsafe { CycleDelay::from_core_upper_bound(timing.core_upper_hz) };
            Ok(Self {
                session: Session::new(nor, lcd, delay),
                _crm: resources.crm,
                _reserved: pins.reserved,
                _keyscan: pins.keyscan,
            })
        }
        pub fn session(&mut self) -> &mut HardwareSession {
            &mut self.session
        }
    }
}
