#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Explicit diagnostic takeover of the related AT32F423/RT880 profile.
//!
//! The same bounded register sequence is exercised by host mocks and the ARM
//! MMIO binding. Register readback proves requested digital configuration, not
//! electrical RF-off, actual board identity, clock accuracy or recovery.

pub mod registers {
    pub const PID: u32 = 0xe004_2000;
    pub const SERIES_REVISION: u32 = 0xe004_2020;
    pub const FLASH_SIZE: u32 = 0x1fff_f7e0;
    pub const CRM: u32 = 0x4002_3800;
    pub const CRM_CTRL: u32 = CRM;
    pub const CRM_PLLCFG: u32 = CRM + 4;
    pub const CRM_CFG: u32 = CRM + 8;
    pub const CRM_CLKINT: u32 = CRM + 0x0c;
    pub const CRM_AHBRST1: u32 = CRM + 0x10;
    pub const CRM_AHBRST2: u32 = CRM + 0x14;
    pub const CRM_APB1RST: u32 = CRM + 0x20;
    pub const CRM_AHBEN1: u32 = CRM + 0x30;
    pub const CRM_AHBEN2: u32 = CRM + 0x34;
    pub const CRM_APB1EN: u32 = CRM + 0x40;
    pub const CRM_BPDC: u32 = CRM + 0x70;
    pub const CRM_CTRLSTS: u32 = CRM + 0x74;
    pub const CRM_MISC1: u32 = CRM + 0xa0;
    pub const CRM_MISC2: u32 = CRM + 0xa4;
    pub const FLASH_PSR: u32 = 0x4002_3c00;
    pub const PWC_LDOOV: u32 = 0x4000_7010;
    pub const WDT_CMD: u32 = 0x4000_3000;
    pub const WDT_DIV: u32 = WDT_CMD + 4;
    pub const WDT_RELOAD: u32 = WDT_CMD + 8;
    pub const WDT_STATUS: u32 = WDT_CMD + 12;
    pub const WDT_WINDOW: u32 = WDT_CMD + 16;
    pub const WWDT_CTRL: u32 = 0x4000_2c00;
    pub const GPIOA: u32 = 0x4002_0000;
    pub const GPIOB: u32 = 0x4002_0400;
    pub const GPIOC: u32 = 0x4002_0800;
    pub const GPIOF: u32 = 0x4002_1400;
    pub const CFGR: u32 = 0;
    pub const OMODE: u32 = 4;
    pub const ODRVR: u32 = 8;
    pub const PULL: u32 = 12;
    pub const ODT: u32 = 0x14;
    pub const SCR: u32 = 0x18;
    pub const LOCK: u32 = 0x1c;
    pub const CLR: u32 = 0x28;
    pub const ICTR: u32 = 0xe000_e004;
    pub const SYSTICK_CTRL: u32 = 0xe000_e010;
    pub const SYSTICK_RELOAD: u32 = 0xe000_e014;
    pub const SYSTICK_CURRENT: u32 = 0xe000_e018;
    pub const NVIC_ICER: u32 = 0xe000_e180;
    pub const NVIC_ICPR: u32 = 0xe000_e280;
    pub const SCB_ICSR: u32 = 0xe000_ed04;
    pub const SCB_VTOR: u32 = 0xe000_ed08;
    pub const SCB_SCR: u32 = 0xe000_ed10;
}

use registers::*;

pub const APPLICATION_BASE: u32 = 0x0800_2800;
pub const CORE_NOMINAL_HZ: u32 = 48_000_000;
/// Experimental delay-bound assumption, not a measured or qualified limit.
pub const CORE_UPPER_HZ: u32 = 50_000_000;
pub const DEFAULT_POLL_BUDGET: u32 = 100_000;
pub const DMA_CLOCKS: u32 = (1 << 22) | (1 << 24);
pub const USB_CLOCK: u32 = 1 << 7;
pub const SPI_CLOCKS: u32 = (1 << 14) | (1 << 15);
pub const GPIO_CLOCKS: u32 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 5);
/// SDK's40kHz nominal LICK, not a measurement of this unit's oscillator.
pub const WATCHDOG_NOMINAL_TIMEOUT_MS: u32 = 2000;
pub const WATCHDOG_DIVIDER_CODE: u32 = 3; // /32
pub const WATCHDOG_RELOAD_VALUE: u32 = 2499; //2500ticks *32 /40000Hz

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleTimeError {
    FrequencyTooLow,
    AmbiguousDelta,
}

/// Milliseconds accumulated from a wrapping cycle counter, with fractional
/// cycles retained. Caller must sample at intervals below 2^31 cycles; multiple
/// unobserved wraps cannot be detected. Intended for UI deadlines, not metrology.
pub struct CycleMillis {
    previous: u32,
    hz: u32,
    remainder: u64,
    milliseconds: u32,
}

impl CycleMillis {
    pub fn new(start_cycles: u32, hz: core::num::NonZeroU32) -> Result<Self, CycleTimeError> {
        if hz.get() < 1000 {
            return Err(CycleTimeError::FrequencyTooLow);
        }
        Ok(Self {
            previous: start_cycles,
            hz: hz.get(),
            remainder: 0,
            milliseconds: 0,
        })
    }

    /// On an ambiguous jump, state is unchanged and callers must stop/rebase.
    pub fn sample(&mut self, cycles: u32) -> Result<u32, CycleTimeError> {
        let delta = cycles.wrapping_sub(self.previous);
        if delta >= (1 << 31) {
            return Err(CycleTimeError::AmbiguousDelta);
        }
        let scaled = u64::from(delta) * 1000 + self.remainder;
        self.milliseconds = self
            .milliseconds
            .wrapping_add((scaled / u64::from(self.hz)) as u32);
        self.remainder = scaled % u64::from(self.hz);
        self.previous = cycles;
        Ok(self.milliseconds)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub product_id: u32,
    pub series_revision: u32,
    pub flash_kib: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    EntryContext,
    ProductId(u32),
    SeriesRevision(u32),
    FlashSize(u16),
    InterruptBanks,
    BusMastersNotStopped,
    PinLocked {
        port: u32,
        mask: u16,
    },
    LowRequestReadback,
    LowSpeedExternalClockActive,
    WindowWatchdogActive,
    IndependentWatchdogUpdating,
    IndependentWatchdogWindow,
    IndependentWatchdogBudget,
    IndependentWatchdogReadback,
    PollBudget,
    ClockTimeout {
        register: u32,
        mask: u32,
        expected: u32,
    },
    ClockProfile(uv98_clock_profile::Error),
}

/// No hidden globals or allocation. Executed in Rust only AFTER .data/.bss
/// initialization; the separate assembly pre-init handles earlier takeover.
/// A real implementation requires exclusive privileged AT32 MMIO access.
pub trait RegisterIo {
    fn mask_interrupts(&mut self);
    fn entry_context_supported(&mut self) -> bool;
    fn read32(&mut self, address: u32) -> u32;
    fn read16(&mut self, address: u32) -> u16;
    fn write32(&mut self, address: u32, value: u32);
    fn barrier(&mut self);
    /// At least the 40 core cycles used by Artery's divider transition helper.
    fn settle_hick_divider(&mut self);
}

/// Bounded terminal RF-A write timing. The concrete MCU implementation waits
/// at least 500 core cycles per phase, hence at least 10 us only under the
/// reviewed <=50 MHz clock assumption. No watchdog reload or UART is allowed.
pub trait TerminalRfTiming: RegisterIo {
    fn terminal_rf_phase_delay(&mut self);
}

fn modify(io: &mut impl RegisterIo, address: u32, clear: u32, set: u32) {
    let before = io.read32(address);
    io.write32(address, (before & !clear) | set);
}

pub fn identify(io: &mut impl RegisterIo) -> Result<Identity, Error> {
    let product_id = io.read32(PID);
    if product_id != 0x700a_3243 && product_id != 0x700a_3246 {
        return Err(Error::ProductId(product_id));
    }
    let series_revision = io.read32(SERIES_REVISION);
    if series_revision & 0xff07 != 0x1200 {
        return Err(Error::SeriesRevision(series_revision));
    }
    let flash_kib = io.read16(FLASH_SIZE);
    if flash_kib != 256 {
        return Err(Error::FlashSize(flash_kib));
    }
    Ok(Identity {
        product_id,
        series_revision,
        flash_kib,
    })
}

fn two_bit_fields(mask: u16, value: u32) -> u32 {
    let mut result = 0;
    for bit in 0..16 {
        if mask & (1 << bit) != 0 {
            result |= value << (bit * 2);
        }
    }
    result
}

fn output(
    io: &mut impl RegisterIo,
    port: u32,
    low: u16,
    high: u16,
    open_drain: bool,
) -> Result<(), Error> {
    let mask = low | high;
    let lock = io.read32(port + LOCK);
    if lock & (1 << 16) != 0 && lock & u32::from(mask) != 0 {
        return Err(Error::PinLocked { port, mask });
    }
    // Preload both latch levels before selecting output mode. No whole-port
    // reset and no ODT RMW: other pins and the antenna relay remain untouched.
    if low != 0 {
        io.write32(port + CLR, u32::from(low));
    }
    if high != 0 {
        io.write32(port + SCR, u32::from(high));
    }
    modify(
        io,
        port + OMODE,
        u32::from(mask),
        if open_drain { u32::from(mask) } else { 0 },
    );
    modify(io, port + PULL, two_bit_fields(mask, 3), 0);
    modify(
        io,
        port + ODRVR,
        two_bit_fields(mask, 3),
        two_bit_fields(mask, 2),
    );
    modify(
        io,
        port + CFGR,
        two_bit_fields(mask, 3),
        two_bit_fields(mask, 1),
    );
    Ok(())
}

/// Only related-profile digital low requests. This is not RF readback.
/// PA4 backlight is included when asserting, excluded from runtime checking.
pub fn assert_low_requests(io: &mut impl RegisterIo) -> Result<(), Error> {
    // LEXT owns PC14/PC15 regardless of apparent GPIO mode. Do not change the
    // backup domain to take those pins: reject inherited oscillator ownership.
    if io.read32(CRM_BPDC) & 1 != 0 {
        return Err(Error::LowSpeedExternalClockActive);
    }
    modify(io, CRM_AHBEN1, 0, GPIO_CLOCKS);
    let _ = io.read32(CRM_AHBEN1);
    // Physical RF gate requests first, then mute and backlight blanking.
    output(io, GPIOC, (1 << 15) | (1 << 4) | (1 << 5), 0, false)?;
    output(io, GPIOB, 1 << 11, 0, false)?;
    output(io, GPIOF, 1 << 8, 0, false)?;
    output(io, GPIOA, 1 << 4, 0, false)?;
    io.barrier();
    if !low_requests_present(io) {
        return Err(Error::LowRequestReadback);
    }
    Ok(())
}

pub fn low_requests_present(io: &mut impl RegisterIo) -> bool {
    if io.read32(CRM_BPDC) & 1 != 0 {
        return false;
    }
    if io.read32(CRM_AHBEN1) & GPIO_CLOCKS != GPIO_CLOCKS {
        return false;
    }
    for (port, mask) in [(GPIOC, 0x8030u16), (GPIOB, 0x0800), (GPIOF, 0x0100)] {
        if io.read32(port + ODT) & u32::from(mask) != 0
            || io.read32(port + CFGR) & two_bit_fields(mask, 3) != two_bit_fields(mask, 1)
            || io.read32(port + OMODE) & u32::from(mask) != 0
            || io.read32(port + PULL) & two_bit_fields(mask, 3) != 0
            || io.read32(port + ODRVR) & two_bit_fields(mask, 3) != two_bit_fields(mask, 2)
        {
            return false;
        }
    }
    true
}

fn output_present(io: &mut impl RegisterIo, port: u32, mask: u16, high: u16) -> bool {
    io.read32(port + ODT) & u32::from(mask) == u32::from(high)
        && io.read32(port + CFGR) & two_bit_fields(mask, 3) == two_bit_fields(mask, 1)
        && io.read32(port + OMODE) & u32::from(mask) == 0
        && io.read32(port + PULL) & two_bit_fields(mask, 3) == 0
        && io.read32(port + ODRVR) & two_bit_fields(mask, 3) == two_bit_fields(mask, 2)
}

/// Terminal, identity-gated RF-A shutdown REQUESTS, never electrical RF proof.
/// Caller has abandoned all GPIO/RF owners and will not resume normal work.
/// This post-RAM helper mirrors the separate stackless exception path. It
/// neither reads RF registers nor applies calibration/tuning or starts RF-B.
pub fn terminal_radio_off_requests(io: &mut impl TerminalRfTiming) -> Result<(), Error> {
    io.mask_interrupts();
    identify(io)?;
    quiesce_bus_masters(io)?;
    if io.read32(CRM_BPDC) & 1 != 0 {
        return Err(Error::LowSpeedExternalClockActive);
    }
    modify(io, CRM_AHBEN1, 0, GPIO_CLOCKS);
    io.barrier();
    if io.read32(CRM_AHBEN1) & GPIO_CLOCKS != GPIO_CLOCKS {
        return Err(Error::LowRequestReadback);
    }
    // Physical MCU gates/mutes first. Their verified output state is required
    // before touching RF-A serial pins or clocking a register request.
    assert_low_requests(io)?;
    for (port, mask) in [(GPIOA, 1u16 << 8), (GPIOF, 1 << 6), (GPIOC, 1 << 9)] {
        let lock = io.read32(port + LOCK);
        if lock & (1 << 16) != 0 && lock & u32::from(mask) != 0 {
            return Err(Error::PinLocked { port, mask });
        }
    }
    // Deselect before changing SCK/DATA ownership; preserve every other pin.
    output(io, GPIOA, 0, 1 << 8, false)?;
    output(io, GPIOF, 1 << 6, 0, false)?;
    output(io, GPIOC, 1 << 9, 0, false)?;
    io.barrier();
    if !output_present(io, GPIOA, 1 << 8, 1 << 8)
        || !output_present(io, GPIOF, 1 << 6, 0)
        || !output_present(io, GPIOC, 1 << 9, 0)
    {
        return Err(Error::LowRequestReadback);
    }
    for frame in [0x330000u32, 0x36007f, 0x300000, 0x371d00] {
        io.write32(GPIOF + CLR, 1 << 6);
        io.write32(GPIOA + CLR, 1 << 8);
        for shift in (0..24).rev() {
            io.write32(GPIOF + CLR, 1 << 6);
            io.write32(
                GPIOC + if frame & (1 << shift) != 0 { SCR } else { CLR },
                1 << 9,
            );
            io.terminal_rf_phase_delay();
            io.write32(GPIOF + SCR, 1 << 6);
            io.terminal_rf_phase_delay();
        }
        io.write32(GPIOA + SCR, 1 << 8);
        io.terminal_rf_phase_delay();
    }
    io.barrier();
    if !output_present(io, GPIOA, 1 << 8, 1 << 8)
        || !output_present(io, GPIOF, 1 << 6, 1 << 6)
        || !output_present(io, GPIOC, 1 << 9, 0)
        || !low_requests_present(io)
    {
        return Err(Error::LowRequestReadback);
    }
    Ok(())
}

/// Never starts/unlocks/reconfigures either watchdog. Reloading a restricted
/// independent-WDT window can reset the chip, so such inherited profiles fail.
/// The minimum budget is 4096 LICK ticks, not a claimed measured duration.
pub fn feed_watchdog(io: &mut impl RegisterIo) -> Result<(), Error> {
    if io.read32(CRM_APB1EN) & (1 << 11) != 0 && io.read32(WWDT_CTRL) & 0x80 != 0 {
        return Err(Error::WindowWatchdogActive);
    }
    if io.read32(WDT_STATUS) & 7 != 0 {
        return Err(Error::IndependentWatchdogUpdating);
    }
    if io.read32(WDT_WINDOW) & 0xfff != 0xfff {
        return Err(Error::IndependentWatchdogWindow);
    }
    let divider = io.read32(WDT_DIV) & 7;
    let reload = io.read32(WDT_RELOAD) & 0xfff;
    if divider > 6 || (4u32 << divider) * (reload + 1) < 4096 {
        return Err(Error::IndependentWatchdogBudget);
    }
    io.write32(WDT_CMD, 0xaaaa);
    Ok(())
}

fn watchdog_poll(
    io: &mut impl RegisterIo,
    address: u32,
    mask: u32,
    expected: u32,
    budget: u32,
) -> Result<(), Error> {
    // No reload inside the wait. An unhealthy peripheral must not keep the
    // processor alive indefinitely; each readiness stage has an explicit bound.
    for _ in 0..budget {
        if io.read32(address) & mask == expected {
            return Ok(());
        }
    }
    Err(Error::ClockTimeout {
        register: address,
        mask,
        expected,
    })
}

fn watchdog_configuration_present(io: &mut impl RegisterIo) -> bool {
    io.read32(CRM_CTRLSTS) & 3 == 3
        && io.read32(WDT_STATUS) & 7 == 0
        && io.read32(WDT_DIV) & 7 == WATCHDOG_DIVIDER_CODE
        && io.read32(WDT_RELOAD) & 0xfff == WATCHDOG_RELOAD_VALUE
        && io.read32(WDT_WINDOW) & 0xfff == 0xfff
}

/// Explicitly configure and start independent WDT before permitting RF TX.
/// Local F423 SDK: unlock5555, reloadAAAA, enableCCCC; LICK nominal40kHz.
/// /32 * (2499+1) gives nominal2s. Actual oscillator/reset timing is unmeasured.
/// Once enabled, the watchdog cannot be disabled by this API; caller must feed
/// it through bounded foreground work. A reload is never proof of progress.
pub fn arm_independent_watchdog(io: &mut impl RegisterIo, budget: u32) -> Result<(), Error> {
    io.mask_interrupts();
    if budget == 0 {
        return Err(Error::PollBudget);
    }
    if !io.entry_context_supported() {
        return Err(Error::EntryContext);
    }
    identify(io)?;
    if io.read32(CRM_APB1EN) & (1 << 11) != 0 && io.read32(WWDT_CTRL) & 0x80 != 0 {
        return Err(Error::WindowWatchdogActive);
    }
    if io.read32(WDT_WINDOW) & 0xfff != 0xfff {
        return Err(Error::IndependentWatchdogWindow);
    }
    // RSTFC is write-one-to-clear: never echo that command bit while enabling
    // LICK. Preserve reset causes and unrelated oscillator/backup-domain state.
    modify(io, CRM_CTRLSTS, 1 << 24, 1);
    watchdog_poll(io, CRM_CTRLSTS, 3, 3, budget)?;
    watchdog_poll(io, WDT_STATUS, 7, 0, budget)?;
    feed_watchdog(io)?; // permissible inherited unrestricted window only
    io.write32(WDT_CMD, 0x5555);
    io.write32(WDT_DIV, WATCHDOG_DIVIDER_CODE);
    io.write32(WDT_RELOAD, WATCHDOG_RELOAD_VALUE);
    io.write32(WDT_WINDOW, 0xfff);
    let configured = watchdog_poll(io, WDT_STATUS, 7, 0, budget).and_then(|()| {
        if watchdog_configuration_present(io) {
            Ok(())
        } else {
            Err(Error::IndependentWatchdogReadback)
        }
    });
    if let Err(error) = configured {
        io.write32(WDT_CMD, 0); // relock even when verification failed
        return Err(error);
    }
    io.write32(WDT_CMD, 0xaaaa);
    io.write32(WDT_CMD, 0xcccc); // explicit start, distinct from reload
    io.write32(WDT_CMD, 0);
    io.barrier();
    watchdog_poll(io, WDT_STATUS, 7, 0, budget)?;
    if !watchdog_configuration_present(io) {
        return Err(Error::IndependentWatchdogReadback);
    }
    // There is no SDK running-status bit. Success means enable was requested
    // with verified configuration, not that a physical timeout was measured.
    feed_watchdog(io)
}

fn quiesce_bus_masters(io: &mut impl RegisterIo) -> Result<(), Error> {
    // Do not enable inherited, clock-gated DMA channels merely to inspect them.
    // This firmware does not use DMA/OTG: gate their clocks and hold their reset
    // lines asserted. GPIO ports and the battery-backed domain are not reset.
    modify(io, CRM_AHBEN1, DMA_CLOCKS, 0);
    modify(io, CRM_AHBEN2, USB_CLOCK, 0);
    modify(io, CRM_AHBRST1, 0, DMA_CLOCKS);
    modify(io, CRM_AHBRST2, 0, USB_CLOCK);
    io.barrier();
    if io.read32(CRM_AHBEN1) & DMA_CLOCKS != 0
        || io.read32(CRM_AHBEN2) & USB_CLOCK != 0
        || io.read32(CRM_AHBRST1) & DMA_CLOCKS != DMA_CLOCKS
        || io.read32(CRM_AHBRST2) & USB_CLOCK != USB_CLOCK
    {
        return Err(Error::BusMastersNotStopped);
    }
    Ok(())
}

/// Repeat/verify the assembly takeover AFTER RAM initialization, before PAC::take().
/// Supported entry is privileged thread mode using MSP, on the selected PID.
/// No loader-independent safety is inferred from a successful result.
pub fn early_quiesce(io: &mut impl RegisterIo) -> Result<Identity, Error> {
    io.mask_interrupts();
    if !io.entry_context_supported() {
        return Err(Error::EntryContext);
    }
    let identity = identify(io)?;
    io.write32(SYSTICK_CTRL, 0);
    io.write32(SYSTICK_RELOAD, 0);
    io.write32(SYSTICK_CURRENT, 0);
    let banks = (io.read32(ICTR) & 15) + 1;
    if banks > 8 {
        return Err(Error::InterruptBanks);
    }
    for bank in 0..banks {
        io.write32(NVIC_ICER + 4 * bank, u32::MAX);
        io.write32(NVIC_ICPR + 4 * bank, u32::MAX);
    }
    io.write32(SCB_ICSR, (1 << 25) | (1 << 27));
    io.write32(SCB_VTOR, APPLICATION_BASE);
    modify(io, SCB_SCR, (1 << 1) | (1 << 2), 0);
    io.barrier();
    quiesce_bus_masters(io)?;
    assert_low_requests(io)?;
    // Deassert storage/LCD/RF CS and release passive keypad columns before
    // individual drivers take ownership. No clocks or data pulses are sent.
    output(io, GPIOA, 0, (1 << 8) | (1 << 15), false)?;
    output(io, GPIOB, 0, 1 << 12, false)?;
    output(io, GPIOC, 0, (1 << 7) | (1 << 14), false)?; // RF CS + LCD reset released.
    output(io, GPIOB, 0, 0x0407, true)?;
    io.barrier();
    quiesce_spi(io)?;
    feed_watchdog(io)?;
    Ok(identity)
}

fn quiesce_spi(io: &mut impl RegisterIo) -> Result<(), Error> {
    // CS lines must already be deasserted. Abort inherited shift engines before
    // their APB1 clock changes, then hold reset through clock initialization.
    modify(io, CRM_APB1EN, SPI_CLOCKS, 0);
    modify(io, CRM_APB1RST, 0, SPI_CLOCKS);
    io.barrier();
    if io.read32(CRM_APB1EN) & SPI_CLOCKS != 0 || io.read32(CRM_APB1RST) & SPI_CLOCKS != SPI_CLOCKS
    {
        return Err(Error::BusMastersNotStopped);
    }
    Ok(())
}

fn poll(
    io: &mut impl RegisterIo,
    address: u32,
    mask: u32,
    expected: u32,
    budget: u32,
) -> Result<(), Error> {
    for iteration in 0..budget {
        if io.read32(address) & mask == expected {
            return Ok(());
        }
        if iteration & 255 == 0 {
            feed_watchdog(io)?;
        }
    }
    Err(Error::ClockTimeout {
        register: address,
        mask,
        expected,
    })
}

/// Establish the reviewed nominal 48/24/48 MHz profile. The caller must have
/// successfully executed early_quiesce and must not have assigned GPIO/PAC
/// ownership or started peripherals yet. All readiness waits are bounded.
pub fn initialize(io: &mut impl RegisterIo, budget: u32) -> Result<Identity, Error> {
    if budget == 0 {
        return Err(Error::PollBudget);
    }
    io.mask_interrupts();
    if !io.entry_context_supported() {
        return Err(Error::EntryContext);
    }
    let identity = identify(io)?;
    if !low_requests_present(io) {
        return Err(Error::LowRequestReadback);
    }
    if io.read32(CRM_APB1EN) & SPI_CLOCKS != 0 || io.read32(CRM_APB1RST) & SPI_CLOCKS != SPI_CLOCKS
    {
        return Err(Error::BusMastersNotStopped);
    }
    quiesce_bus_masters(io)?;
    feed_watchdog(io)?;

    // FLASH_PSR changes access latency/prefetch only. No key, control, USD,
    // program, erase, option byte or protection register is written.
    modify(io, FLASH_PSR, 0x157, 0x154);
    modify(io, CRM_APB1EN, 0, 1 << 28);
    let _ = io.read32(CRM_APB1EN);
    modify(io, PWC_LDOOV, 0x13, 3);

    modify(io, CRM_MISC2, 0x30, 0x30); // Artery auto-step mode enabled.
    modify(io, CRM_CTRL, 1 << 19, 1); // HICK on, clock-failure interrupt off.
    poll(io, CRM_CTRL, 2, 2, budget)?;
    // Artery crm_hick_sclk_frequency_select sequence: /16, >=40 core
    // cycles, select HICK48/NODIV, then direct 8 MHz. PLL input untouched.
    modify(io, CRM_MISC2, 7 << 16, 4 << 16);
    io.settle_hick_divider();
    modify(io, CRM_MISC1, 0, 1 << 14);
    modify(io, CRM_MISC1, 0, 1 << 12);
    modify(io, CRM_MISC1, 1 << 14, 0);
    modify(io, CRM_CFG, 3, 0);
    poll(io, CRM_CFG, 12, 0, budget)?;
    // Once on HICK, PLL can be stopped without removing the executing clock.
    modify(io, CRM_CTRL, 1 << 24, 0);
    poll(io, CRM_CTRL, 1 << 25, 0, budget)?;
    modify(io, CRM_MISC2, 7 << 16, 0);
    modify(io, CRM_CFG, (15 << 4) | (7 << 10) | (7 << 13), 4 << 10);
    const PLL_FIELDS: u32 = (1 << 22) | (0x1ff << 6) | 15 | (7 << 16);
    modify(io, CRM_PLLCFG, PLL_FIELDS, 0x31801);
    io.write32(CRM_CLKINT, 0x009f_0000);
    modify(io, CRM_CTRL, 0, 1 << 24);
    poll(io, CRM_CTRL, 1 << 25, 1 << 25, budget)?;
    modify(io, CRM_CFG, 3, 2);
    poll(io, CRM_CFG, 12, 8, budget)?;
    modify(io, CRM_MISC2, 0x30, 0);
    io.barrier();
    let snapshot = uv98_clock_profile::Snapshot {
        ctrl: io.read32(CRM_CTRL),
        pllcfg: io.read32(CRM_PLLCFG),
        cfg: io.read32(CRM_CFG),
        misc1: io.read32(CRM_MISC1),
    };
    snapshot.stock_profile().map_err(Error::ClockProfile)?;
    feed_watchdog(io)?;
    // Both constructors perform their own selected SPI reset after enabling its
    // clock. Release our reset now while clocks remain gated and CS remains HIGH.
    modify(io, CRM_APB1RST, SPI_CLOCKS, 0);
    io.barrier();
    if io.read32(CRM_APB1RST) & SPI_CLOCKS != 0 || io.read32(CRM_APB1EN) & SPI_CLOCKS != 0 {
        return Err(Error::BusMastersNotStopped);
    }
    Ok(identity)
}

#[cfg(all(feature = "hardware", target_arch = "arm"))]
pub mod hardware;

#[cfg(test)]
mod tests;
