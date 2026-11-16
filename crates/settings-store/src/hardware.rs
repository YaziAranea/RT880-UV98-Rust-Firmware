//! AT32F423xC main flash, 2 KiB sectors 126 and 127.
//!
//! Artery RM rev 2.03 §5.2 and §5.8 define the flash controller sequence.
//! A same-bank flash fetch can stall during program or erase, so the
//! operation below runs in SRAM.

use crate::{
    journal::{self, Page, Storage, PAGE_SIZE, WORDS},
    Error, Settings,
};
use at32f4xx_pac::at32f423::{CRM, FLASH};
use core::{arch::global_asm, ptr};

const START: usize = 0x0803_f000;
const END: usize = 0x0804_0000;
const SRAM_START: usize = 0x2000_0000;
const SRAM_END: usize = 0x2000_c000;
const FLASH_SIZE_KIB: *const u16 = 0x1fff_f7e0 as *const u16;
const DWT_CTRL: *const u32 = 0xe000_1000 as *const u32;
const DWT_CYCCNT: *const u32 = 0xe000_1004 as *const u32;
const ERASE_DEADLINE_CYCLES: u32 = 48_000 * 50; // 50 ms, DS max 16 ms.
const PROGRAM_DEADLINE_CYCLES: u32 = 48_000 * 2; // 2 ms, DS max 42 us.

global_asm!(include_str!("flash.s"), options(raw));

extern "C" {
    static __settings_start: u8;
    static __settings_end: u8;
    static __sdata: u8;
    static __edata: u8;
    static __uv98_flash_op_end: u8;
    fn __uv98_flash_op(address: u32, data: u32, mode: u32, deadline_cycles: u32) -> u32;
}

pub struct FlashStore {
    flash: FLASH,
}

impl FlashStore {
    /// Open the journal owned by this firmware image.
    ///
    /// # Safety
    /// Startup must identify an AT32F423xC with 256 KiB, initialize .data
    /// (including SRAM instructions), establish 48 MHz HCLK and HICK, start
    /// DWT and arm the independent watchdog. The linker must exclude both
    /// pages from all executable and loadable sections. No other access to
    /// the flash controller, journal pages or raw aliases may run concurrently.
    pub unsafe fn new(flash: FLASH) -> Result<Self, Error> {
        let lo = ptr::addr_of!(__sdata) as usize;
        let hi = ptr::addr_of!(__edata) as usize;
        let op = __uv98_flash_op as *const () as usize & !1;
        let op_end = ptr::addr_of!(__uv98_flash_op_end) as usize;
        if unsafe { ptr::read_volatile(FLASH_SIZE_KIB) } != 256
            || ptr::addr_of!(__settings_start) as usize != START
            || ptr::addr_of!(__settings_end) as usize != END
            || !(SRAM_START <= lo && lo <= op && op < op_end && op_end <= hi && hi <= SRAM_END)
        {
            return Err(Error::Layout);
        }
        if flash.slib_sts0().read().slib_enf().bit_is_set()
            && flash.slib_sts1().read().slib_es().bits() >= 126
        {
            return Err(Error::Protection);
        }
        Ok(Self { flash })
    }

    /// Return the most recent committed record, or None on a blank journal.
    pub fn load(&mut self) -> Result<Option<Settings>, Error> {
        journal::load(self)
    }

    /// Save a user-confirmed change. Call in foreground with RF quiescent and
    /// enough watchdog margin for a bounded erase and at most eight word writes.
    /// Each operation masks interrupts until the flash controller is idle.
    /// A stuck busy controller is left to the armed independent watchdog.
    /// If final verification fails after commit, a subsequent load can see
    /// the new value. An error is never a confirmed save.
    pub fn save(&mut self, settings: Settings) -> Result<(), Error> {
        journal::save(self, settings)
    }

    fn operation(
        &mut self,
        address: usize,
        value: u32,
        mode: u32,
        deadline_cycles: u32,
    ) -> Result<(), Error> {
        // EPPS bit 31 covers main sectors 62 through 127 in the xC profile.
        if self.flash.epps().read().bits() & (1 << 31) == 0 {
            return Err(Error::Protection);
        }
        let clock = unsafe { &*CRM::ptr() }.ctrl().read();
        let dwt_enabled = unsafe { ptr::read_volatile(DWT_CTRL) } & 1 != 0;
        if !clock.hicken().bit_is_set() || !clock.hickstbl().bit_is_set() || !dwt_enabled {
            return Err(Error::Clock);
        }
        // SAFETY: only private Storage routes below supply an address. The
        // assembly checks it again before enabling programming or sector erase.
        match unsafe { __uv98_flash_op(address as u32, value, mode, deadline_cycles) } {
            0 => Ok(()),
            1 => Err(Error::Controller),
            _ => Err(Error::Operation),
        }
    }
}

fn page_address(page: Page) -> usize {
    match page {
        Page::First => START,
        Page::Second => START + PAGE_SIZE,
    }
}

fn slot_address(page: Page, slot: usize) -> Result<usize, Error> {
    const RECORD_SIZE: usize = WORDS * size_of::<u32>();
    if slot >= PAGE_SIZE / RECORD_SIZE {
        return Err(Error::Layout);
    }
    Ok(page_address(page) + slot * RECORD_SIZE)
}

impl Storage for FlashStore {
    fn read(&mut self, page: Page, slot: usize) -> Result<[u32; WORDS], Error> {
        let address = slot_address(page, slot)? as *const u32;
        let mut record = [0; WORDS];
        for (index, word) in record.iter_mut().enumerate() {
            *word = unsafe { ptr::read_volatile(address.add(index)) };
        }
        Ok(record)
    }

    fn erase(&mut self, page: Page) -> Result<(), Error> {
        self.operation(page_address(page), 0, 2, ERASE_DEADLINE_CYCLES)
    }

    fn program(&mut self, page: Page, slot: usize, word: usize, value: u32) -> Result<(), Error> {
        if word >= WORDS {
            return Err(Error::Layout);
        }
        let address = slot_address(page, slot)? + word * size_of::<u32>();
        if unsafe { ptr::read_volatile(address as *const u32) } != u32::MAX {
            return Err(Error::Verify);
        }
        self.operation(address, value, 1, PROGRAM_DEADLINE_CYCLES)
    }
}
