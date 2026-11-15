use crate::{baud_divider, nmea, Counters};
use core::{
    cell::UnsafeCell,
    ptr::{read_volatile, write_volatile},
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};
use uv98_board_pins::GpsPins;

const UART: usize = 0x4000_4400;
const CRM: usize = 0x4002_3800;
const GPIOA: usize = 0x4002_0000;
const GPIOB: usize = 0x4002_0400;
const CAPACITY: usize = 512;
struct Ring(UnsafeCell<[u8; CAPACITY]>);
// Single IRQ producer, single foreground consumer; slot ownership is transferred
// with release/acquire indices. The buffer is never handed out as a reference.
unsafe impl Sync for Ring {}
static RING: Ring = Ring(UnsafeCell::new([0; CAPACITY]));
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicU32 = AtomicU32::new(0);
static ERRORS: AtomicU32 = AtomicU32::new(0);
static OVERFLOW: AtomicU32 = AtomicU32::new(0);

unsafe fn read(address: usize) -> u32 {
    unsafe { read_volatile(address as *const u32) }
}
unsafe fn write(address: usize, value: u32) {
    unsafe { write_volatile(address as *mut u32, value) }
}
unsafe fn modify(address: usize, clear: u32, set: u32) {
    unsafe { write(address, (read(address) & !clear) | set) }
}

/// Forward the PAC `USART2` vector (IRQ38) here. It never parses NMEA, touches
/// GPIO, or executes more than eight UART reads. Reading STS then DT clears errors.
/// # Safety
/// Only USART2 IRQ may call this. A unique Gps instance owns USART2 and ring.
pub unsafe fn on_interrupt() {
    for _ in 0..8 {
        let status = unsafe { read(UART) };
        if status & 0x2f == 0 {
            break;
        }
        let byte = unsafe { read(UART + 4) } as u8;
        if status & 0xf != 0 {
            ERRORS.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        BYTES.fetch_add(1, Ordering::Relaxed);
        let head = HEAD.load(Ordering::Relaxed);
        let next = (head + 1) & (CAPACITY - 1);
        if next == TAIL.load(Ordering::Acquire) {
            OVERFLOW.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        unsafe {
            (*RING.0.get())[head] = byte;
        }
        HEAD.store(next, Ordering::Release);
    }
}

pub struct Gps {
    _pins: GpsPins,
    parser: nmea::Parser,
    enabled: bool,
    losses: u32,
    accepted: u32,
    rejected: u32,
}
impl Gps {
    /// Configures RX-only GPIO/UART but leaves power and NVIC IRQ off.
    /// # Safety
    /// Exclusive USART2 and token pins; no DMA or other USART2 code. Valid 48MHz
    /// HCLK/24MHz APB1 profile; GPIO/CRM RMW serialized in foreground. Parent must
    /// install USART2 vector before enabling interrupts. Constructor does not
    /// globally enable interrupts or change any unrelated NVIC line.
    pub unsafe fn new(pins: GpsPins) -> Self {
        unsafe {
            write(0xe000_e184, 1 << 6); // NVIC ICER1 IRQ38
            modify(CRM + 0x30, 0, 3);
            modify(CRM + 0x40, 0, 1 << 17);
            let _ = read(CRM + 0x40);
            write(UART + 0x0c, 0);
            // USART2 has an independent source mux: 00=PCLK1. Do not rely on
            // a prior firmware leaving its reset default intact.
            modify(CRM + 0x68, 3 << 2, 0);
            write(UART + 0x10, 0);
            write(UART + 0x14, 0);
            modify(GPIOA + 0x0c, 3 << 6, 1 << 6);
            modify(GPIOA + 0x20, 15 << 12, 7 << 12);
            modify(GPIOA, 3 << 6, 2 << 6);
            write(GPIOB + 0x28, 1 << 9);
            modify(GPIOB + 4, 1 << 9, 0);
            modify(GPIOB + 0x0c, 3 << 18, 0);
            modify(GPIOB, 3 << 18, 1 << 18);
            write_volatile((0xe000_e400usize + 38) as *mut u8, 0xc0);
        }
        HEAD.store(0, Ordering::Relaxed);
        TAIL.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
        ERRORS.store(0, Ordering::Relaxed);
        OVERFLOW.store(0, Ordering::Relaxed);
        Self {
            _pins: pins,
            parser: nmea::Parser::new(),
            enabled: false,
            losses: 0,
            accepted: 0,
            rejected: 0,
        }
    }
    pub fn configure(&mut self, enabled: bool, baud_selector: u8) {
        // Disable the sole producer before flushing its queue or changing baud.
        unsafe {
            write(0xe000_e184, 1 << 6);
            write(UART + 0x0c, 0);
            let _ = read(UART);
            let _ = read(UART + 4);
            write(0xe000_e284, 1 << 6); // NVIC ICPR1
        }
        TAIL.store(HEAD.load(Ordering::Acquire), Ordering::Release);
        self.parser = nmea::Parser::new();
        self.losses = ERRORS
            .load(Ordering::Relaxed)
            .wrapping_add(OVERFLOW.load(Ordering::Relaxed));
        unsafe {
            write(GPIOB + if enabled { 0x18 } else { 0x28 }, 1 << 9);
            if enabled {
                write(
                    UART + 8,
                    u32::from(baud_divider(24_000_000, nmea::stock_baud(baud_selector)).unwrap()),
                );
                write(UART + 0x0c, (1 << 13) | (1 << 5) | (1 << 2)); // UEN, RXNEIE, REN; TEN=0
                write(0xe000_e104, 1 << 6); // NVIC ISER1
            }
        }
        self.enabled = enabled;
    }
    /// Parse at most budget<=128 bytes per foreground visit. A loss resets the
    /// parser AND fix so truncated data cannot leave a deceptively valid position.
    pub fn poll(&mut self, now_ms: u32, budget: usize) -> Counters {
        let losses = ERRORS
            .load(Ordering::Relaxed)
            .wrapping_add(OVERFLOW.load(Ordering::Relaxed));
        if losses != self.losses {
            self.parser = nmea::Parser::new();
            self.losses = losses;
            TAIL.store(HEAD.load(Ordering::Acquire), Ordering::Release);
        }
        for _ in 0..budget.min(128) {
            let tail = TAIL.load(Ordering::Relaxed);
            if tail == HEAD.load(Ordering::Acquire) {
                break;
            }
            let byte = unsafe { (*RING.0.get())[tail] };
            TAIL.store((tail + 1) & (CAPACITY - 1), Ordering::Release);
            match self.parser.push(byte, now_ms) {
                Some(nmea::Event::Accepted(_)) => self.accepted = self.accepted.wrapping_add(1),
                Some(nmea::Event::Rejected(_)) => self.rejected = self.rejected.wrapping_add(1),
                _ => {}
            }
        }
        self.counters()
    }
    pub fn fix(&self, now_ms: u32) -> Option<nmea::Fix> {
        if self.enabled {
            self.parser.snapshot(now_ms, 5_000)
        } else {
            None
        }
    }
    pub fn last_report(&self) -> Option<nmea::Report> {
        self.parser.last_report()
    }
    pub fn counters(&self) -> Counters {
        Counters {
            bytes: BYTES.load(Ordering::Relaxed),
            uart_errors: ERRORS.load(Ordering::Relaxed),
            overflow: OVERFLOW.load(Ordering::Relaxed),
            accepted: self.accepted,
            rejected: self.rejected,
        }
    }
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}
impl Drop for Gps {
    fn drop(&mut self) {
        self.configure(false, 1);
    }
}
