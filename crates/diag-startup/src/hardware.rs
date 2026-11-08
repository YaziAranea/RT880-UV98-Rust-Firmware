//! Raw access exists only during serialized startup, or terminal failure.
//! There are no retained register references and no statics before pre_init.

use crate::{Error, Identity, RegisterIo};

// cortex-m-rt 0.7.7 explicitly disallows Rust before RAM initialization.
// Startup uses a stackless assembly branch graph; Rust begins only afterwards.
// The 0.1.2 live observer identified this unit as PID 0x700a3246, series 0x1200,
// 256 KiB. Its exercised HICK8/PA11 bootstrap precedes diagnostic rejection;
// exact identity, context and pin guards still precede all RF pin requests.
core::arch::global_asm!(include_str!("early_log.s"), include_str!("pre_init.s"));

struct Mmio;

impl crate::TerminalRfTiming for Mmio {
    fn terminal_rf_phase_delay(&mut self) {
        // Minimum500 core cycles, deliberately conservative. This is a
        // bounded instruction loop, not a calibrated physical-time claim.
        // SAFETY: no stack/data memory access; flags and temporary register
        // are declared clobbered. Terminal callers already mask interrupts.
        unsafe {
            core::arch::asm!(
                "movw {remaining}, #500", "2:",
                "subs {remaining}, {remaining}, #1", "bne 2b",
                // Keep the compiler memory barrier around volatile pin writes.
                remaining = out(reg) _, options(nostack)
            );
        }
    }
}

impl RegisterIo for Mmio {
    fn mask_interrupts(&mut self) {
        cortex_m::interrupt::disable();
    }

    fn entry_context_supported(&mut self) -> bool {
        let ipsr: u32;
        let control: u32;
        // SAFETY: CPU special-register reads do not access uninitialized RAM.
        unsafe {
            core::arch::asm!("mrs {ipsr}, IPSR", "mrs {control}, CONTROL",
                ipsr = out(reg) ipsr, control = out(reg) control,
                options(nomem, nostack, preserves_flags));
        }
        ipsr == 0 && control & 3 == 0
    }

    fn read32(&mut self, address: u32) -> u32 {
        // SAFETY: wrappers require the supported privileged memory map; the
        // sequencing layer supplies aligned register addresses, identity first.
        unsafe { core::ptr::read_volatile(address as *const u32) }
    }

    fn read16(&mut self, address: u32) -> u16 {
        // SAFETY: only the documented flash-size halfword is read this way.
        unsafe { core::ptr::read_volatile(address as *const u16) }
    }

    fn write32(&mut self, address: u32, value: u32) {
        // SAFETY: startup/terminal access is serialized by the wrapper contract.
        unsafe { core::ptr::write_volatile(address as *mut u32, value) }
    }

    fn barrier(&mut self) {
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
    }

    fn settle_hick_divider(&mut self) {
        cortex_m::asm::dsb();
        cortex_m::asm::delay(40);
    }
}

/// Repeat/verify the assembly takeover after cortex-m-rt initialized RAM.
///
/// # Safety
/// Must run after .data/.bss initialization and before driver/PAC ownership,
/// except a serialized TX-only boot logger retaining no register references.
/// That logger must have drained its transmission and remain idle throughout
/// this call. Requires valid MSP, executable flash and Cortex-M4 AT32 mapping.
/// A loader must enter privileged thread mode using MSP. Register identity
/// checks cannot establish that the attached PCB has the assumed pin wiring.
pub unsafe fn early_quiesce() -> Result<Identity, Error> {
    crate::early_quiesce(&mut Mmio)
}

/// Establish clocks after successful early_quiesce, before PAC::take/split.
///
/// # Safety
/// The early_quiesce contract still holds; no transfer/driver may run concurrently,
/// and interrupts must remain masked through ownership transfer. A TX-only raw
/// boot logger may exist with TC drained; it must remain paused during all clock
/// changes and be explicitly reconfigured to the new APB2 rate before reuse.
pub unsafe fn initialize() -> Result<Identity, Error> {
    crate::initialize(&mut Mmio, crate::DEFAULT_POLL_BUDGET)
}

/// Reload only, after checking inherited watchdog window and minimum budget.
///
/// # Safety
/// A successful identity/startup sequence must precede this call. No watchdog
/// driver or interrupt may concurrently change its configuration. This helper
/// never starts or reconfigures a watchdog, and cannot prevent a deadline that
/// expired before the caller first reached it.
pub unsafe fn feed_watchdog() -> Result<(), Error> {
    crate::feed_watchdog(&mut Mmio)
}

/// Explicitly enable nominal2s independent WDT, before any RF TX request.
///
/// # Safety
/// Requires successful startup, initialized RAM, privileged thread/MSP mode,
/// exclusive CRM/WDT ownership and masked interrupts. This deliberately changes
/// watchdog configuration and issues ENABLE; it cannot subsequently be disabled
/// through this API. A failure requires terminal RF-off handling, never TX.
/// Caller must bound all work and feed regularly; nominal40kHz LICK and physical
/// reset/RF shutdown timing have not been measured on the unit.
pub unsafe fn arm_independent_watchdog() -> Result<(), Error> {
    crate::arm_independent_watchdog(&mut Mmio, crate::DEFAULT_POLL_BUDGET)
}

/// Terminal, PID-gated low/mute/backlight requests. GPIO drivers must be abandoned.
///
/// # Safety
/// Call only in a terminal serialized panic/fault path; nothing may subsequently
/// resume use of owned GPIO drivers. The CPU must permit privileged MMIO. This
/// can run in fault-handler mode, unlike normal startup. On unsupported PID it
/// performs no GPIO writes. It does not prove electrical RF-off.
pub unsafe fn terminal_low_requests() -> Result<(), Error> {
    let mut io = Mmio;
    io.mask_interrupts();
    crate::identify(&mut io)?;
    crate::quiesce_bus_masters(&mut io)?;
    crate::assert_low_requests(&mut io)
}

/// Terminal MCU gate LOW plus RF-A registers33/36/30/37 shutdown requests.
///
/// # Safety
/// Requires initialized RAM, privileged supported AT32 MMIO and <=50 MHz core.
/// All GPIO/RF owners must be abandoned; interrupts stay masked and no normal
/// driver may resume after this call. Error logging may start only AFTER this
/// bounded attempt returns. Unsupported identity performs no GPIO writes.
/// Successful GPIO readback/serial requests do not prove electrical RF-off.
pub unsafe fn terminal_radio_off_requests() -> Result<(), Error> {
    crate::terminal_radio_off_requests(&mut Mmio)
}

/// Terminal failure loop. An inherited watchdog may reset the MCU.
///
/// # Safety
/// Same exclusive terminal access contract as terminal_low_requests. The ASM
/// attempts strictly identity-gated MCU-low and RF-A shutdown frames, then stops inherited SPI,
/// establishes direct HICK8 and configures PA11/USART6 to log fault registers
/// under the broader F423 family gate. This clock/UART access is permitted even
/// if exact PID/revision/flash guards reject RF pin access.
/// Do not call this Rust wrapper before RAM initialization. GPIO readback is
/// not off proof; failed clock readiness or locked TX can leave the loop silent.
pub unsafe fn terminal_fail_stop() -> ! {
    extern "C" {
        fn __diag_terminal_fail_stop() -> !;
    }
    // SAFETY: caller promises terminal exclusive access and privileged MMIO.
    // Exception vectors enter this assembly symbol directly, without Rust.
    unsafe { __diag_terminal_fail_stop() }
}
