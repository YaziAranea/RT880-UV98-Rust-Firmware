#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Nominal clock-profile check, not a clock initializer or physical measurement.
//! Reviewed stock profile: HICK -> PLL NS96/MS1/FR8/fixed2 -> 48 MHz;
//! AHB /1, APB1 /2, APB2 /1. Reference: research/startup/clock_oracle.py.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub ctrl: u32,
    pub pllcfg: u32,
    pub cfg: u32,
    pub misc1: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    OscillatorNotReady,
    ClockSource,
    PllConfiguration,
    BusDividers,
    HickDivider,
    ChangedDuringRead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NominalStockClocks {
    _private: (),
}
impl NominalStockClocks {
    pub const fn core_hz(self) -> u32 {
        48_000_000
    }
    pub const fn apb1_hz(self) -> u32 {
        24_000_000
    }
    pub const fn apb2_hz(self) -> u32 {
        48_000_000
    }
}

impl Snapshot {
    /// Accept only the selected stock profile. Unrelated CRM fields are
    /// preserved/ignored. Does not identify the MCU, voltage, flash latency,
    /// actual oscillator calibration, peripheral ownership, IRQ or RF state.
    pub fn stock_profile(self) -> Result<NominalStockClocks, Error> {
        const OSC: u32 = (1 << 0) | (1 << 1) | (1 << 24) | (1 << 25);
        if self.ctrl & OSC != OSC {
            return Err(Error::OscillatorNotReady);
        }
        if self.cfg & 0xf != 0xa {
            return Err(Error::ClockSource);
        }
        const PLL_MASK: u32 = (1 << 22) | (0x1ff << 6) | 0xf | (7 << 16);
        if self.pllcfg & PLL_MASK != 0x31801 {
            return Err(Error::PllConfiguration);
        }
        if self.cfg & ((0xf << 4) | (7 << 10) | (7 << 13)) != (4 << 10) {
            return Err(Error::BusDividers);
        }
        if self.misc1 & (1 << 12) == 0 {
            return Err(Error::HickDivider);
        }
        Ok(NominalStockClocks { _private: () })
    }
}

/// Ceiling conversion for a caller-established upper bound on core frequency.
/// u32*u32 plus the rounding term is representable in u64 for these divisors.
pub const fn cycles_for_us(upper_hz: u32, us: u32) -> u64 {
    (upper_hz as u64 * us as u64 + 999_999) / 1_000_000
}
pub const fn cycles_for_ms(upper_hz: u32, ms: u32) -> u64 {
    (upper_hz as u64 * ms as u64 + 999) / 1_000
}

#[cfg(feature = "hardware")]
pub mod hardware {
    use super::*;
    use at32f4xx_pac::at32f423::CRM;
    use core::{marker::PhantomData, num::NonZeroU32};

    fn read(crm: &CRM) -> Snapshot {
        Snapshot {
            ctrl: crm.ctrl().read().bits(),
            pllcfg: crm.pllcfg().read().bits(),
            cfg: crm.cfg().read().bits(),
            misc1: crm.misc1().read().bits(),
        }
    }
    /// Read CRM twice, rejecting visible changes. Equal reads are not a lock or
    /// proof against intervening changes. No clock or GPIO register is written.
    pub fn observe(crm: &CRM) -> Result<NominalStockClocks, Error> {
        let first = read(crm);
        let second = read(crm);
        if first != second {
            return Err(Error::ChangedDuringRead);
        }
        first.stock_profile()
    }

    /// Timer-free minimum delay. Does not own/reconfigure SysTick or enable IRQ.
    /// Upper bound must include oscillator tolerance; nominal 48 MHz alone is
    /// not a guarantee of a 48 MHz physical upper bound. Longer delays are OK
    /// for this contract; it is not a precise clock or scheduling source.
    pub struct CycleDelay {
        upper_hz: NonZeroU32,
        _single_context: PhantomData<*mut ()>,
    }
    impl CycleDelay {
        /// # Safety
        /// Cortex-M core and core frequency upper bound must be established.
        /// Keep actual core rate <= upper_hz throughout use; no uncontrolled
        /// clock changes. cortex-m::asm::delay must satisfy its architecture
        /// contract. Interrupt service may lengthen delays without a bound.
        pub unsafe fn from_core_upper_bound(upper_hz: NonZeroU32) -> Self {
            Self {
                upper_hz,
                _single_context: PhantomData,
            }
        }
        fn cycles(&mut self, mut cycles: u64) {
            while cycles > 0 {
                let chunk = cycles.min(u32::MAX as u64) as u32;
                cortex_m::asm::delay(chunk);
                cycles -= chunk as u64;
            }
        }
        pub fn wait_us(&mut self, us: u32) {
            self.cycles(cycles_for_us(self.upper_hz.get(), us));
        }
    }
    impl uv98_display::Delay for CycleDelay {
        fn delay_ms(&mut self, ms: u32) {
            self.cycles(cycles_for_ms(self.upper_hz.get(), ms));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stock() -> Snapshot {
        Snapshot {
            ctrl: 0x03000003,
            pllcfg: 0x31801,
            cfg: 0x4000100a,
            misc1: 0xf1000,
        }
    }
    #[test]
    fn stock_profile_matches_oracle_and_apb1_drives_both_spi2_spi3() {
        let clocks = stock().stock_profile().unwrap();
        assert_eq!(
            (clocks.core_hz(), clocks.apb1_hz(), clocks.apb2_hz()),
            (48_000_000, 24_000_000, 48_000_000)
        );
    }
    #[test]
    fn rejects_stopped_or_unstable_oscillators() {
        for bit in [0, 1, 24, 25] {
            let mut s = stock();
            s.ctrl &= !(1 << bit);
            assert_eq!(s.stock_profile(), Err(Error::OscillatorNotReady));
        }
    }
    #[test]
    fn rejects_switch_in_progress_wrong_divisors_and_pll_sources() {
        for cfg in [0x1008, 0x1009, 0x1002, 0x108a, 0x000a, 0x900a] {
            let mut s = stock();
            s.cfg = cfg;
            assert!(s.stock_profile().is_err());
        }
        for bit in [0, 6, 7, 14, 16, 17, 18, 22] {
            let mut s = stock();
            s.pllcfg ^= 1 << bit;
            assert_eq!(s.stock_profile(), Err(Error::PllConfiguration));
        }
        let mut s = stock();
        s.misc1 &= !(1 << 12);
        assert_eq!(s.stock_profile(), Err(Error::HickDivider));
    }
    #[test]
    fn ignores_unrelated_register_fields() {
        let mut s = stock();
        s.cfg |= 3 << 27;
        s.ctrl |= 1 << 16;
        s.misc1 |= 1 << 14;
        assert!(s.stock_profile().is_ok());
    }
    #[test]
    fn delay_conversion_rounds_up_and_never_overflows() {
        assert_eq!(cycles_for_us(48_000_001, 1), 49);
        assert_eq!(cycles_for_ms(48_000_001, 1), 48001);
        assert_eq!(cycles_for_us(u32::MAX, u32::MAX), 18_446_744_065_120);
        assert_eq!(cycles_for_ms(u32::MAX, u32::MAX), 18_446_744_065_119_618);
        assert_eq!(cycles_for_us(48_000_000, 0), 0);
    }
}
