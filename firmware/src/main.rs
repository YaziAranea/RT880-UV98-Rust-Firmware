//! Build-only Cortex-M4 startup exercise, not a radio firmware release.
//!
//! No board clocks, GPIOs, RF, storage or serial drivers are configured here.
//! An inherited bootloader state is NOT proven safe by doing nothing: this
//! executable must not be flashed until board bring-up and recovery are qualified.
#![no_std]
#![no_main]

use at32f4xx_pac as _; // Supply AT32F423 interrupt vectors to cortex-m-rt.
use core::panic::PanicInfo;
use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    cortex_m::interrupt::disable();
    idle()
}

fn idle() -> ! {
    loop {
        cortex_m::asm::wfi();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    cortex_m::interrupt::disable();
    idle()
}
