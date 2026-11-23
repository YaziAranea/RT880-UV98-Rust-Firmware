//! Stock PB4==0 C-BUS modem. Bell 202 carries synchronous NRZI bytes at 1200
//! bps. Service the one-byte register in less than 6.67 ms during RX and TX.
use crate::aprs::{Wire, WIRE_CAPACITY};

const RX_MODE: u8 = 0xe2;
const RX_DATA: u8 = 0xe5;
const STATUS: u8 = 0xe6;
// CML D/868/9 and D/868A/3 §1.5.10.4: Bell 202 1200 bps, stock level
// index 0, synchronous 8-bit USART.
const RX_BELL_202: u16 = 0x303e;
const RX_READY: u16 = 0x0040;
const RX_OVERFLOW: u16 = 0x0020;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedBackend,
    NotReady,
    Busy,
    Empty,
    Timeout,
    Underflow,
    Overflow,
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

fn shutdown(bus: &mut impl Bus) -> Result<(), Error> {
    bus.tx_audio(false);
    let tx = bus.write16(0xe1, 0);
    let rx = bus.write16(RX_MODE, 0);
    let power = bus.write16(0xe0, 0x1000);
    tx.and(rx).and(power)
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
    receive_gain: Option<u8>,
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
            receive_gain: None,
            at: 0,
            started: 0,
            wire: [0; WIRE_CAPACITY],
            len: 0,
            index: 0,
        }
    }
    pub fn begin(&mut self, now: u32) -> Result<(), Error> {
        if self.receive_gain.is_some() || self.sending() {
            return Err(Error::Busy);
        }
        self.begin_inner(now);
        Ok(())
    }
    /// Reserve the modem for reception until off() or a bus fault.
    pub fn begin_rx(&mut self, now: u32, gain: u8) -> Result<(), Error> {
        if !matches!(self.phase, Phase::Off | Phase::Fault) {
            return Err(Error::Busy);
        }
        self.receive_gain = Some(if gain <= 7 { gain } else { 0 });
        self.begin_inner(now);
        Ok(())
    }
    fn begin_inner(&mut self, now: u32) {
        self.bus.tx_audio(false);
        self.at = now;
        self.phase = Phase::PowerWait;
    }
    /// Start only after caller has obtained ordinary calibrated FM-TX ownership.
    /// No RF PA/route changes here. Startup is explicit, no beacon timer.
    pub fn start_tx(&mut self, wire: &Wire, now: u32, level: u8) -> Result<(), Error> {
        if self.phase != Phase::Ready || self.receive_gain.is_some() {
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
        self.phase = Phase::Off;
        self.receive_gain = None;
        shutdown(&mut self.bus)
    }
    fn fail_receive(&mut self) {
        let _ = self.off();
        self.phase = Phase::Fault;
    }
    pub fn poll(&mut self, now: u32) -> Result<Progress, Error> {
        if self.phase == Phase::Fault {
            return Err(Error::NotReady);
        }
        let result = self.poll_inner(now);
        if result.is_err() {
            if self.receive_gain.is_some() {
                self.fail_receive();
            } else {
                self.bus.tx_audio(false);
                let _ = self.bus.write16(0xe1, 0);
                self.phase = Phase::Fault;
            }
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
                // The control registers are write-only. Probe internal
                // synchronous TX-ready with PB7 HIGH, as for the manual
                // beacon. This driver owns neither RF nor the PA.
                self.bus.write16(0xe1, 0x321c)?;
                let gain = u16::from(self.receive_gain.unwrap_or(0)) << 9;
                self.bus.write16(RX_MODE, RX_BELL_202 | gain)?;
                self.phase = Phase::Probe;
                self.at = now;
                Ok(Progress::Starting)
            }
            Phase::Probe => {
                if elapsed < 10 {
                    return Ok(Progress::Starting);
                }
                let status = self.bus.read16(STATUS)?;
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
                if self.receive_gain.is_some() {
                    // Discard any byte accumulated during the internal probe.
                    self.bus.read8(RX_DATA)?;
                }
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
    /// One bounded status+read during a ready RX session. Bit 0 arrived first.
    /// On overflow the pending byte is discarded and the caller resets HDLC.
    pub fn receive(&mut self) -> Result<Option<u8>, Error> {
        if self.phase != Phase::Ready || self.receive_gain.is_none() {
            return Ok(None);
        }
        let result = self.read_rx_byte();
        if !matches!(result, Ok(_) | Err(Error::Overflow)) {
            self.fail_receive();
        }
        result
    }
    fn read_rx_byte(&mut self) -> Result<Option<u8>, Error> {
        let status = self.bus.read16(STATUS)?;
        if status == 0xffff {
            return Err(Error::Bus);
        }
        if status & (RX_READY | RX_OVERFLOW) == 0 {
            return Ok(None);
        }
        // Reading E5 clears READY and OVERFLOW (CML §1.5.9 and §1.5.10.7).
        let byte = self.bus.read8(RX_DATA)?;
        if status & RX_OVERFLOW != 0 {
            return Err(Error::Overflow);
        }
        Ok(Some(byte))
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
            let _ = super::shutdown(self);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Mock {
        status: u16,
        audio: bool,
        last: (u8, u16),
        written: u16,
        control: u16,
        tx_mode: u16,
        rx_mode: u16,
        rx_reads: u8,
        rx_byte: u8,
        fail_write: Option<u8>,
        fail_read: bool,
    }
    impl Bus for Mock {
        fn reset(&mut self) -> Result<(), Error> {
            Ok(())
        }
        fn write16(&mut self, r: u8, v: u16) -> Result<(), Error> {
            if self.fail_write == Some(r) {
                return Err(Error::Bus);
            }
            self.last = (r, v);
            match r {
                0xe0 => self.control = v,
                0xe1 => self.tx_mode = v,
                RX_MODE => self.rx_mode = v,
                _ => unreachable!(),
            }
            Ok(())
        }
        fn write8(&mut self, _: u8, _: u8) -> Result<(), Error> {
            self.written += 1;
            Ok(())
        }
        fn read16(&mut self, _: u8) -> Result<u16, Error> {
            Ok(self.status)
        }
        fn read8(&mut self, r: u8) -> Result<u8, Error> {
            assert_eq!(r, RX_DATA);
            if self.fail_read {
                return Err(Error::Bus);
            }
            self.rx_reads += 1;
            self.status &= !(RX_READY | RX_OVERFLOW);
            Ok(self.rx_byte)
        }
        fn tx_audio(&mut self, e: bool) {
            self.audio = e;
        }
    }
    #[test]
    fn underrun_and_deadline_remove_audio() {
        let mut modem = Modem::new(Mock {
            status: 0x1000,
            ..Mock::default()
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
            ..Mock::default()
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
        let mut modem = Modem::new(Mock::default());
        modem.begin(0).unwrap();
        for now in [100, 150, 175, 275, 285] {
            assert_eq!(modem.poll(now), Ok(Progress::Starting));
            assert!(!modem.bus.audio);
        }
        assert_eq!(modem.poll(325), Err(Error::Timeout));
        assert!(!modem.bus.audio);
        assert_eq!(modem.bus.last, (0xe1, 0));
    }

    fn rx_starting(status: u16, gain: u8) -> Modem<Mock> {
        let mut modem = Modem::new(Mock {
            status,
            ..Mock::default()
        });
        modem.begin_rx(0, gain).unwrap();
        for now in [100, 150, 175, 275] {
            assert_eq!(modem.poll(now), Ok(Progress::Starting));
            assert_eq!(modem.receive(), Ok(None));
            assert!(!modem.bus.audio);
        }
        modem
    }

    #[test]
    fn rx_owns_modem_and_delivers_raw_bytes_until_off() {
        let mut modem = rx_starting(0x1000 | RX_READY | RX_OVERFLOW, 6);
        assert_eq!(modem.poll(285), Ok(Progress::Ready));
        assert_eq!(modem.bus.rx_mode, 0x3c3e);
        assert_eq!(modem.bus.tx_mode, 0);
        assert_eq!(modem.bus.rx_reads, 1); // Startup byte was discarded.
        assert_eq!(modem.receive(), Ok(None));
        assert_eq!(modem.begin(285), Err(Error::Busy));
        assert_eq!(modem.begin_rx(285, 0), Err(Error::Busy));
        let wire = Wire {
            bytes: [0; WIRE_CAPACITY],
            len: 1,
        };
        assert_eq!(modem.start_tx(&wire, 285, 1), Err(Error::NotReady));
        assert_eq!(modem.bus.written, 0);
        assert!(!modem.bus.audio);

        modem.bus.rx_byte = 0b1000_0000;
        modem.bus.status = RX_READY;
        assert_eq!(modem.receive(), Ok(Some(0b1000_0000)));
        assert_eq!(modem.receive(), Ok(None));
        assert_eq!(modem.bus.rx_reads, 2);

        modem.off().unwrap();
        assert_eq!(modem.poll(286), Ok(Progress::Off));
        assert_eq!(modem.bus.rx_mode, 0);
        assert_eq!(modem.bus.control, 0x1000);
        assert_eq!(modem.receive(), Ok(None));
        modem.begin(287).unwrap();
        assert_eq!(modem.begin_rx(287, 0), Err(Error::Busy));
    }

    #[test]
    fn rx_overflow_discards_one_byte_and_next_byte_is_available() {
        let mut modem = rx_starting(0x1000, 0xff);
        assert_eq!(modem.poll(285), Ok(Progress::Ready));
        assert_eq!(modem.bus.rx_mode, 0x303e);
        modem.bus.status = RX_READY | RX_OVERFLOW;
        assert_eq!(modem.receive(), Err(Error::Overflow));
        assert_eq!(modem.bus.rx_reads, 2);
        assert_eq!(modem.receive(), Ok(None));
        modem.bus.status = RX_READY;
        modem.bus.rx_byte = 0x6a;
        assert_eq!(modem.receive(), Ok(Some(0x6a)));
    }

    #[test]
    fn rx_probe_timeout_or_floating_bus_shuts_down() {
        for (status, error) in [(0, Error::Timeout), (0xffff, Error::Bus)] {
            let mut modem = rx_starting(status, 0);
            assert_eq!(modem.poll(325), Err(error));
            assert_eq!(modem.bus.rx_mode, 0);
            assert_eq!(modem.bus.tx_mode, 0);
            assert_eq!(modem.bus.control, 0x1000);
            assert!(!modem.bus.audio);
            assert_eq!(modem.receive(), Ok(None));
            assert_eq!(modem.poll(326), Err(Error::NotReady));
            modem.begin_rx(327, 0).unwrap();
        }
    }

    #[test]
    fn rx_data_bus_failure_attempts_all_shutdown_writes() {
        let mut modem = rx_starting(0x1000, 0);
        assert_eq!(modem.poll(285), Ok(Progress::Ready));
        modem.bus.status = RX_READY | RX_OVERFLOW;
        modem.bus.fail_read = true;
        modem.bus.fail_write = Some(RX_MODE);
        assert_eq!(modem.receive(), Err(Error::Bus));
        assert_eq!(modem.bus.tx_mode, 0);
        assert_eq!(modem.bus.rx_mode, RX_BELL_202);
        assert_eq!(modem.bus.control, 0x1000);
        assert!(!modem.bus.audio);
        assert_eq!(modem.receive(), Ok(None));
    }
}
