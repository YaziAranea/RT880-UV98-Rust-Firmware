#![no_std]
#![no_main]
//! Minimal application observer, not a replacement for the internal loader.
//! Rust supplies assembly/linking only. Reset does not call Rust or use RAM.
//! The register map is the one used by the stock image that ran on this radio;
//! this is an explicit board-profile assumption, not an identity-derived fact.

core::arch::global_asm!(include_str!("probe.s"));

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // No Rust code is reachable from Reset; retain a bounded terminal fallback.
    unsafe { core::arch::asm!("b ProbeStop", options(noreturn, nostack)) }
}
