//! Opt-in AT32F423 GPIO adapter. No ADC, encoder, IRQ or radio operation.
use crate::{
    engine::{Engine, Registers},
    Error, GpioRegisters, GuardScan, Scan, ScanTiming, COLUMN_MASK,
};
use at32f4xx_pac::at32f423::{CRM, GPIOB, GPIOC, GPIOD};
use uv98_board_pins::KeyscanPins;

struct Mmio {
    _pins: KeyscanPins,
}
impl Registers for Mmio {
    fn release(&mut self) {
        unsafe { &*GPIOB::ptr() }
            .scr()
            .write(|w| unsafe { w.bits(u32::from(COLUMN_MASK)) });
    }
    fn select(&mut self, column_pin: u16) {
        unsafe { &*GPIOB::ptr() }
            .clr()
            .write(|w| unsafe { w.bits(u32::from(column_pin)) });
    }
    fn select_all(&mut self) {
        unsafe { &*GPIOB::ptr() }
            .clr()
            .write(|w| unsafe { w.bits(u32::from(COLUMN_MASK)) });
    }
    fn rows_high(&mut self) -> u8 {
        let c = unsafe { &*GPIOC::ptr() }.idt().read().bits();
        let d = unsafe { &*GPIOD::ptr() }.idt().read().bits();
        (((c >> 10) & 7) | (((d >> 2) & 1) << 3)) as u8
    }
    fn ptt_high(&mut self) -> bool {
        unsafe { &*GPIOB::ptr() }.idt().read().bits() & (1 << 6) != 0
    }
}

/// Holds disjoint keyscan pin ownership. `scan` performs two finite scans;
/// stability across scans is evidence, not proof of continuous physical state.
/// Caller must debounce accepted single keys and explicitly cancel/rearm input
/// handling on Ambiguous/errors. Do not turn either case into `Released`.
pub struct KeyScanner<T: ScanTiming> {
    engine: Engine<Mmio, T>,
}
impl<T: ScanTiming> KeyScanner<T> {
    /// Configure only keypad/PTT pin fields. All columns remain released.
    ///
    /// # Safety
    /// Caller must establish AT32F423 and the related passive matrix wiring,
    /// voltage levels, valid pull-ups and open-drain compatibility. No external
    /// active driver may contend with the selected low column. Columns must be
    /// released/quiescent before entry. PTT PB6 is an input only, independently
    /// wired; RF/PA inactivity is the caller's separate responsibility. Serialize
    /// shared GPIO configuration RMW and CRM access against ALL other pin owners;
    /// no IRQ/DMA/raw alias may access these owned pins. Stable GPIO gates/power
    /// must persist through Drop. No whole-port reset is permitted. Timing must
    /// allow matrix/pull-up RC and input synchronizers to settle. It must not
    /// panic or re-enter. Actual board/loader identity is not inferred here.
    pub unsafe fn from_quiescent(pins: KeyscanPins, crm: &mut CRM, timing: T) -> Self {
        crm.ahben1()
            .modify(|_, w| w.gpiob().set_bit().gpioc().set_bit().gpiod().set_bit());
        let _ = crm.ahben1().read().bits();
        let b = unsafe { &*GPIOB::ptr() };
        let c = unsafe { &*GPIOC::ptr() };
        let d = unsafe { &*GPIOD::ptr() };
        let pb = GpioRegisters {
            cfgr: b.cfgr().read().bits(),
            omode: b.omode().read().bits(),
            odrvr: b.odrvr().read().bits(),
            pull: b.pull().read().bits(),
        }
        .with_columns_ptt();
        let pc = GpioRegisters {
            cfgr: c.cfgr().read().bits(),
            omode: c.omode().read().bits(),
            odrvr: c.odrvr().read().bits(),
            pull: c.pull().read().bits(),
        }
        .with_rows_c();
        let pd = GpioRegisters {
            cfgr: d.cfgr().read().bits(),
            omode: d.omode().read().bits(),
            odrvr: d.odrvr().read().bits(),
            pull: d.pull().read().bits(),
        }
        .with_row_d();
        // Temporarily make all owned B pins inputs, then preload released
        // latches and open-drain type before enabling the column outputs.
        b.cfgr()
            .modify(|r, w| unsafe { w.bits(r.bits() & !0x0030_303F) });
        b.scr().write(|w| unsafe { w.bits(u32::from(COLUMN_MASK)) });
        b.omode().write(|w| unsafe { w.bits(pb.omode) });
        b.odrvr().write(|w| unsafe { w.bits(pb.odrvr) });
        b.pull().write(|w| unsafe { w.bits(pb.pull) });
        c.cfgr().write(|w| unsafe { w.bits(pc.cfgr) });
        c.omode().write(|w| unsafe { w.bits(pc.omode) });
        c.odrvr().write(|w| unsafe { w.bits(pc.odrvr) });
        c.pull().write(|w| unsafe { w.bits(pc.pull) });
        d.cfgr().write(|w| unsafe { w.bits(pd.cfgr) });
        d.omode().write(|w| unsafe { w.bits(pd.omode) });
        d.odrvr().write(|w| unsafe { w.bits(pd.odrvr) });
        d.pull().write(|w| unsafe { w.bits(pd.pull) });
        b.cfgr().write(|w| unsafe { w.bits(pb.cfgr) });
        Self {
            engine: Engine::new(Mmio { _pins: pins }, timing),
        }
    }
    pub fn scan(&mut self) -> Result<Scan, Error> {
        self.engine.scan()
    }
    /// Prove a released keypad/PTT with two settled aggregate observations;
    /// otherwise perform the existing two-pass scan. Only a clear matrix AND
    /// PB6 HIGH may take the shortcut. Every key or PTT press uses full scans.
    /// An aggregate error propagates unchanged with columns released; it is
    /// never substituted with release or retried as a successful full scan.
    pub fn scan_idle_or_full(&mut self) -> Result<Scan, Error> {
        self.engine.scan_idle_or_full()
    }
    /// Rejection-only observation for an already-armed TX, with two existing
    /// settle phases. Never use this reduced observation to arm or identify a
    /// key. Columns are released before every return, including errors.
    pub fn guard_scan(&mut self) -> Result<GuardScan, Error> {
        self.engine.guard_scan()
    }
}
