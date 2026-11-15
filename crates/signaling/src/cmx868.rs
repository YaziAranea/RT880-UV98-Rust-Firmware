//! Stock PB4==0 C-BUS modem. Bell2021200bps synchronous NRZI bytes are supplied
//! by aprs::Wire. Poll service must run faster than one byte/6.67ms during TX;
//! any buffer underflow cancels the frame. No GPIO bit-banged 1200Hz main loop.
use crate::aprs::{Wire, WIRE_CAPACITY};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedBackend,
    NotReady,
    Busy,
    Empty,
    Timeout,
    Underflow,
    Bus,
}
pub trait Bus {
    fn reset(&mut self) -> Result<(), Error>;
    fn write16(&mut self, register: u8, value: u16) -> Result<(), Error>;
    fn write8(&mut self, register: u8, value: u8) -> Result<(), Error>;
    fn read16(&mut self, register: u8) -> Result<u16, Error>;
    fn read8(&mut self, register: u8) -> Result<u8, Error>;
    /// Stock08021800 routes C-BUS TX through PB7 LOW and restores PB7 HIGH.
    fn tx_audio(&mut self, enabled: bool);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Off,
    Starting,
    Ready,
    Sending,
    Complete,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Off,
    PowerWait,
    Reset,
    Powered,
    Settling,
    Probe,
    Ready,
    Priming,
    Sending,
    Drain,
    Fault,
}
pub struct Modem<B: Bus> {
    bus: B,
    phase: Phase,
    at: u32,
    started: u32,
    wire: [u8; WIRE_CAPACITY],
    len: usize,
    index: usize,
}
impl<B: Bus> Modem<B> {
    pub fn new(bus: B) -> Self {
        Self {
            bus,
            phase: Phase::Off,
            at: 0,
            started: 0,
            wire: [0; WIRE_CAPACITY],
            len: 0,
            index: 0,
        }
    }
    pub fn begin(&mut self, now: u32) -> Result<(), Error> {
        if matches!(self.phase, Phase::Priming | Phase::Sending | Phase::Drain) {
            return Err(Error::Busy);
        }
        self.bus.tx_audio(false);
        self.at = now;
        self.phase = Phase::PowerWait;
        Ok(())
    }
    /// Start only after caller has obtained ordinary calibrated FM-TX ownership.
    /// No RF PA/route changes here. Startup is explicit, no beacon timer.
    pub fn start_tx(&mut self, wire: &Wire, now: u32, level: u8) -> Result<(), Error> {
        if self.phase != Phase::Ready {
            return Err(Error::NotReady);
        }
        if wire.len == 0 || wire.len > WIRE_CAPACITY {
            return Err(Error::Empty);
        }
        self.wire[..wire.len].copy_from_slice(wire.as_bytes());
        self.len = wire.len;
        self.index = 0;
        self.started = now;
        self.at = now;
        // Mode3 Bell202, no scrambler/guard; bits4:3 synchronous, bit2 buffer.
        self.bus
            .write16(0xe1, 0x301c | (u16::from(level.min(7)) << 9))?;
        self.phase = Phase::Priming;
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<(), Error> {
        self.bus.tx_audio(false);
        if matches!(self.phase, Phase::Priming | Phase::Sending | Phase::Drain) {
            self.phase = Phase::Ready;
        }
        if let Err(error) = self.bus.write16(0xe1, 0) {
            self.phase = Phase::Fault;
            return Err(error);
        }
        Ok(())
    }
    pub fn off(&mut self) -> Result<(), Error> {
        self.bus.tx_audio(false);
        self.phase = Phase::Off;
        let first = self.bus.write16(0xe1, 0);
        let second = self.bus.write16(0xe0, 0x1000);
        first.and(second)
    }
    pub fn poll(&mut self, now: u32) -> Result<Progress, Error> {
        if self.phase == Phase::Fault {
            return Err(Error::NotReady);
        }
        let result = self.poll_inner(now);
        if result.is_err() {
            self.bus.tx_audio(false);
            let _ = self.bus.write16(0xe1, 0);
            self.phase = Phase::Fault;
        }
        result
    }
    fn poll_inner(&mut self, now: u32) -> Result<Progress, Error> {
        let elapsed = now.wrapping_sub(self.at);
        match self.phase {
            Phase::Off => Ok(Progress::Off),
            Phase::Fault => Err(Error::NotReady),
            Phase::PowerWait => {
                if elapsed >= 100 {
                    self.bus.reset()?;
                    self.phase = Phase::Reset;
                    self.at = now;
                }
                Ok(Progress::Starting)
            }
            Phase::Reset => {
                if elapsed >= 50 {
                    self.bus.write16(0xe0, 0x1180)?;
                    self.phase = Phase::Powered;
                    self.at = now;
                }
                Ok(Progress::Starting)
            }
            Phase::Powered => {
                if elapsed >= 25 {
                    self.bus.write16(0xe0, 0x1140)?;
                    self.phase = Phase::Settling;
                    self.at = now;
                }
                Ok(Progress::Starting)
            }
            Phase::Settling => {
                if elapsed < 100 {
                    return Ok(Progress::Starting);
                }
                // Probe only the internal modem. PB7 remains HIGH, and this
                // driver does not own RF PA. Zero idle status is legitimate;
                // require an observable TX-ready flag in synchronous mode.
                self.bus.write16(0xe1, 0x321c)?;
                self.bus.write16(0xe2, 0x303e)?; // Bell202, synchronous 8bit data
                self.phase = Phase::Probe;
                self.at = now;
                Ok(Progress::Starting)
            }
            Phase::Probe => {
                if elapsed < 10 {
                    return Ok(Progress::Starting);
                }
                let status = self.bus.read16(0xe6)?;
                if status == 0xffff {
                    return Err(Error::Bus);
                }
                if status & 0x1000 == 0 {
                    return if elapsed < 50 {
                        Ok(Progress::Starting)
                    } else {
                        Err(Error::Timeout)
                    };
                }
                self.bus.write16(0xe1, 0)?;
                self.phase = Phase::Ready;
                Ok(Progress::Ready)
            }
            Phase::Ready => Ok(Progress::Ready),
            Phase::Priming => {
                let status = self.bus.read16(0xe6)?;
                if status & 0x1000 != 0 {
                    // Initial underflow precedes the first buffer fill. Never
                    // write TxData before its ready flag has been observed.
                    self.bus.write8(0xe3, self.wire[0])?;
                    self.index = 1;
                    self.bus.tx_audio(true);
                    self.phase = Phase::Sending;
                    self.at = now;
                } else if elapsed > 30 {
                    return Err(Error::Timeout);
                }
                Ok(Progress::Sending)
            }
            Phase::Sending => {
                if now.wrapping_sub(self.started) > 4_000 {
                    return Err(Error::Timeout);
                }
                let status = self.bus.read16(0xe6)?;
                if status & 0x0800 != 0 {
                    return Err(Error::Underflow);
                }
                if status & 0x1000 != 0 {
                    if self.index < self.len {
                        self.bus.write8(0xe3, self.wire[self.index])?;
                        self.index += 1;
                        self.at = now;
                    } else {
                        self.phase = Phase::Drain;
                        self.at = now;
                    }
                } else if elapsed > 30 {
                    return Err(Error::Timeout);
                }
                Ok(Progress::Sending)
            }
            Phase::Drain => {
                // Byte-buffer ready means the last byte just entered the shift
                // register. Wait its 6.67ms, plus margin, before removing audio.
                if elapsed >= 8 {
                    self.cancel()?;
                    Ok(Progress::Complete)
                } else {
                    Ok(Progress::Sending)
                }
            }
        }
    }
    /// One bounded status+read. Overflow discards the byte; caller resets HDLC.
    pub fn receive(&mut self) -> Result<Option<u8>, Error> {
        if self.phase != Phase::Ready {
            return Ok(None);
        }
        let status = self.bus.read16(0xe6)?;
        if status & 0x0020 != 0 {
            let _ = self.bus.read8(0xe5)?;
            return Err(Error::Bus);
        }
        if status & 0x0040 != 0 {
            Ok(Some(self.bus.read8(0xe5)?))
        } else {
            Ok(None)
        }
    }
    pub const fn sending(&self) -> bool {
        matches!(self.phase, Phase::Priming | Phase::Sending | Phase::Drain)
    }
    pub const fn ready(&self) -> bool {
        matches!(self.phase, Phase::Ready)
    }
}

#[cfg(feature = "hardware")]
pub mod hardware {
    use super::{Bus, Error};
    use core::ptr::{read_volatile, write_volatile};
    use uv98_board_pins::ModemPins;
    const A: usize = 0x4002_0000;
    const B: usize = 0x4002_0400;
    const F: usize = 0x4002_1400;
    unsafe fn rd(a: usize) -> u32 {
        unsafe { read_volatile(a as *const u32) }
    }
    unsafe fn wr(a: usize, v: u32) {
        unsafe { write_volatile(a as *mut u32, v) }
    }
    unsafe fn rmw(a: usize, clear: u32, set: u32) {
        unsafe { wr(a, (rd(a) & !clear) | set) }
    }
    fn delay() {
        // >1us at48MHz including taken branches, below C-BUS maximum clock.
        for _ in 0..48 {
            core::hint::spin_loop();
        }
    }
    pub struct CBus {
        _pins: ModemPins,
    }
    impl CBus {
        /// # Safety
        /// Verified AT32F42348MHz profile, exclusive pins+CRM RMW. Parent owns RF
        /// state. PB7 is modem audio gate, never PA. PB4 selects board backend.
        /// Unsupported PB4-high leaves modem/audio outputs untouched. Only PB4
        /// input mode is selected; its board strap/pull configuration is retained.
        pub unsafe fn new(pins: ModemPins) -> Result<Self, Error> {
            unsafe {
                rmw(0x4002_3830, 0, (1 << 0) | (1 << 1) | (1 << 5));
                let _ = rd(0x4002_3830);
                // Do not change strap pull: read the stock-selected backend.
                rmw(B, 3 << 8, 0);
                for _ in 0..3 {
                    if rd(B + 0x10) & (1 << 4) != 0 {
                        return Err(Error::UnsupportedBackend);
                    }
                    delay();
                }
                wr(F + 0x18, 2);
                wr(A + 0x28, 3);
                wr(B + 0x18, 1 << 7);
                rmw(A + 4, 3, 0);
                rmw(A + 0x0c, 0x3f, 1 << 4);
                rmw(A, 0x3f, 5); // PA0/1 output, PA2 input
                rmw(F + 4, 2, 0);
                rmw(F + 0x0c, 3 << 2, 0);
                rmw(F, 3 << 2, 1 << 2);
                rmw(B + 4, 1 << 7, 0);
                rmw(B + 0x0c, 3 << 14, 0);
                rmw(B, 3 << 14, 1 << 14);
            }
            Ok(Self { _pins: pins })
        }
        fn select(&mut self, on: bool) {
            unsafe {
                wr(F + if on { 0x28 } else { 0x18 }, 2);
            }
            delay();
        }
        fn put(&mut self, byte: u8) {
            for bit in (0..8).rev() {
                unsafe {
                    wr(A + 0x28, 2);
                    wr(A + if byte & (1 << bit) != 0 { 0x18 } else { 0x28 }, 1);
                }
                delay();
                unsafe {
                    wr(A + 0x18, 2);
                }
                delay();
            }
        }
        fn get(&mut self) -> u8 {
            let mut value = 0;
            for _ in 0..8 {
                unsafe {
                    wr(A + 0x28, 2);
                }
                delay();
                value = (value << 1) | u8::from(unsafe { rd(A + 0x10) } & 4 != 0);
                unsafe {
                    wr(A + 0x18, 2);
                }
                delay();
            }
            value
        }
    }
    impl Bus for CBus {
        fn reset(&mut self) -> Result<(), Error> {
            self.select(true);
            self.put(1);
            self.select(false);
            Ok(())
        }
        fn write16(&mut self, r: u8, v: u16) -> Result<(), Error> {
            self.select(true);
            self.put(r);
            self.put((v >> 8) as u8);
            self.put(v as u8);
            self.select(false);
            Ok(())
        }
        fn write8(&mut self, r: u8, v: u8) -> Result<(), Error> {
            self.select(true);
            self.put(r);
            self.put(v);
            self.select(false);
            Ok(())
        }
        fn read16(&mut self, r: u8) -> Result<u16, Error> {
            self.select(true);
            self.put(r);
            let high = self.get();
            let low = self.get();
            self.select(false);
            Ok(u16::from_be_bytes([high, low]))
        }
        fn read8(&mut self, r: u8) -> Result<u8, Error> {
            self.select(true);
            self.put(r);
            let value = self.get();
            self.select(false);
            Ok(value)
        }
        fn tx_audio(&mut self, enabled: bool) {
            unsafe {
                wr(B + if enabled { 0x28 } else { 0x18 }, 1 << 7);
            }
        }
    }
    impl Drop for CBus {
        fn drop(&mut self) {
            self.tx_audio(false);
            let _ = self.write16(0xe1, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Mock {
        status: u16,
        audio: bool,
        last: (u8, u16),
        written: u16,
    }
    impl Bus for Mock {
        fn reset(&mut self) -> Result<(), Error> {
            Ok(())
        }
        fn write16(&mut self, r: u8, v: u16) -> Result<(), Error> {
            self.last = (r, v);
            Ok(())
        }
        fn write8(&mut self, _: u8, _: u8) -> Result<(), Error> {
            self.written += 1;
            Ok(())
        }
        fn read16(&mut self, _: u8) -> Result<u16, Error> {
            Ok(self.status)
        }
        fn read8(&mut self, _: u8) -> Result<u8, Error> {
            Ok(0)
        }
        fn tx_audio(&mut self, e: bool) {
            self.audio = e;
        }
    }
    #[test]
    fn underrun_and_deadline_remove_audio() {
        let mut modem = Modem::new(Mock {
            status: 0x1000,
            audio: false,
            last: (0, 0),
            written: 0,
        });
        modem.begin(0).unwrap();
        modem.poll(100).unwrap();
        modem.poll(150).unwrap();
        modem.poll(175).unwrap();
        modem.poll(275).unwrap();
        assert_eq!(modem.poll(285), Ok(Progress::Ready));
        let wire = Wire {
            bytes: [0; WIRE_CAPACITY],
            len: 4,
        };
        modem.start_tx(&wire, 300, 1).unwrap();
        modem.poll(300).unwrap();
        assert!(modem.bus.audio);
        modem.bus.status = 0x1800;
        assert_eq!(modem.poll(301), Err(Error::Underflow));
        assert!(!modem.bus.audio);
        assert_eq!(modem.bus.last, (0xe1, 0));
    }
    #[test]
    fn finish_waits_last_shift_byte() {
        let mut modem = Modem::new(Mock {
            status: 0x1000,
            audio: false,
            last: (0, 0),
            written: 0,
        });
        modem.begin(0).unwrap();
        modem.poll(100).unwrap();
        modem.poll(150).unwrap();
        modem.poll(175).unwrap();
        modem.poll(275).unwrap();
        modem.poll(285).unwrap();
        modem
            .start_tx(
                &Wire {
                    bytes: [0; WIRE_CAPACITY],
                    len: 2,
                },
                300,
                1,
            )
            .unwrap();
        modem.poll(300).unwrap();
        modem.poll(307).unwrap();
        modem.poll(314).unwrap();
        assert_eq!(modem.poll(321), Ok(Progress::Sending));
        assert!(modem.bus.audio);
        assert_eq!(modem.poll(322), Ok(Progress::Complete));
        assert!(!modem.bus.audio);
        assert_eq!(modem.bus.written, 2);
    }
    #[test]
    fn idle_zero_is_not_identity_and_unresponsive_probe_times_out() {
        let mut modem = Modem::new(Mock {
            status: 0,
            audio: false,
            last: (0, 0),
            written: 0,
        });
        modem.begin(0).unwrap();
        for now in [100, 150, 175, 275, 285] {
            assert_eq!(modem.poll(now), Ok(Progress::Starting));
            assert!(!modem.bus.audio);
        }
        assert_eq!(modem.poll(325), Err(Error::Timeout));
        assert!(!modem.bus.audio);
        assert_eq!(modem.bus.last, (0xe1, 0));
    }
}
