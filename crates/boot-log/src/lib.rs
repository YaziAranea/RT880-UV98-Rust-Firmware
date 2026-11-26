#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
//! Bounded, TX-only diagnostic USART6 logging. Never feeds a watchdog.
//!
//! USART6 uses APB2, not the independent HICK clock mux of USART1/2/3.
//! Every successful frame drains TDC. Stop logging during clock changes and
//! explicitly reconfigure afterwards. Unknown clock sources fail closed.

pub mod error_text;

pub const BAUD: u32 = 115_200;
pub const FRAME_LEN: usize = 47;
pub const CHECKSUM_LEN: usize = 42;
pub const PREFIX: &[u8; 7] = b"UV98D1 ";
pub const DEFAULT_MAX_FRAMES: u16 = 48;
pub const HARD_MAX_FRAMES: u16 = 256;
/// Explicit0.1.8 radio profile. Legacy constructors keep the48-frame budget.
pub const RADIO_MAX_FRAMES: u16 = 192;
/// Enough for radio fault metadata, a fatal header and all eight Debug chunks,
/// plus remaining terminal markers. Normal events cannot spend this reserve.
pub const RADIO_FAULT_RESERVE: u16 = 16;
pub const DEFAULT_POLL_BUDGET: u32 = 8_192;
pub const FRAME_POLL_BUDGET: u32 = 65_536;

/// A manual PA5 export has its own finite allowance; ordinary log/fault quotas
/// remain untouched. Available only in the separate RX-only diagnostic build.
#[cfg(feature = "pa5-audio")]
pub struct AudioExportBudget(u16);
#[cfg(feature = "pa5-audio")]
pub const PA5_CONTEXT: u32 = 0xA610_0000;
#[cfg(feature = "pa5-audio")]
const PA5_EXPORT_FRAMES: u16 = 347;
#[cfg(feature = "pa5-audio")]
impl Default for AudioExportBudget {
    fn default() -> Self {
        Self(PA5_EXPORT_FRAMES)
    }
}

/// Shared by the Rust backend and the stackless startup assembler. These are
/// AT32F423 register definitions, independently checked against the vendor SDK.
pub mod register {
    pub const SERIES_REVISION: u32 = 0xe004_2020;
    // Family gate only. Exact product/revision/flash guards remain the
    // responsibility of startup before RF or application initialization.
    pub const FAMILY_MASK: u32 = 0xff00;
    pub const FAMILY_VALUE: u32 = 0x1200;
    pub const CRM: u32 = 0x4002_3800;
    pub const CRM_CTRL: u32 = CRM;
    pub const CRM_PLLCFG: u32 = CRM + 0x04;
    pub const CRM_CFG: u32 = CRM + 0x08;
    pub const CRM_APB2RST: u32 = CRM + 0x24;
    pub const CRM_AHBEN1: u32 = CRM + 0x30;
    pub const CRM_APB2EN: u32 = CRM + 0x44;
    pub const CRM_MISC1: u32 = CRM + 0xa0;
    pub const CRM_MISC2: u32 = CRM + 0xa4;
    pub const GPIOA: u32 = 0x4002_0000;
    pub const GPIO_CFGR: u32 = GPIOA;
    pub const GPIO_OMODE: u32 = GPIOA + 0x04;
    pub const GPIO_ODRVR: u32 = GPIOA + 0x08;
    pub const GPIO_PULL: u32 = GPIOA + 0x0c;
    pub const GPIO_LOCK: u32 = GPIOA + 0x1c;
    pub const GPIO_MUXH: u32 = GPIOA + 0x24;
    pub const TX_PIN: u32 = 1 << 11;
    pub const TX_MODE_MASK: u32 = 3 << 22;
    pub const TX_MODE_AF: u32 = 2 << 22;
    // STRONGER drive1: exact PA11 profile exercised by live UART probe 0.1.2.
    pub const TX_DRIVE: u32 = 1 << 22;
    pub const TX_AF_MASK: u32 = 0xf << 12;
    pub const TX_AF: u32 = 8 << 12;
    pub const GPIOA_GATE: u32 = 1;
    pub const USART6_GATE: u32 = 1 << 5;
    pub const USART6: u32 = 0x4001_1400;
    pub const USART_STS: u32 = USART6;
    pub const USART_DT: u32 = USART6 + 0x04;
    pub const USART_BAUDR: u32 = USART6 + 0x08;
    pub const USART_CTRL1: u32 = USART6 + 0x0c;
    pub const USART_CTRL2: u32 = USART6 + 0x10;
    pub const USART_CTRL3: u32 = USART6 + 0x14;
    pub const USART_TX_ONLY: u32 = (1 << 13) | (1 << 3);
    pub const TDBE: u32 = 1 << 7;
    pub const TDC: u32 = 1 << 6;
}

/// Wire format: `UV98D1 CCCCCCCC AAAAAAAA BBBBBBBB DDDDDDDD*SS\r\n`.
/// SS is the wrapping sum of the 42 ASCII bytes before `*`, uppercase hex.
/// This checksum detects partial/corrupt lines; it is not authentication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub code: u32,
    pub details: [u32; 3],
}

impl Frame {
    pub const fn new(code: u32, details: [u32; 3]) -> Self {
        Self { code, details }
    }

    pub fn encode(self) -> [u8; FRAME_LEN] {
        let mut out = [b' '; FRAME_LEN];
        out[..PREFIX.len()].copy_from_slice(PREFIX);
        for (index, word) in [self.code, self.details[0], self.details[1], self.details[2]]
            .into_iter()
            .enumerate()
        {
            for digit in 0..8 {
                out[7 + index * 9 + digit] = hex(((word >> (28 - digit * 4)) & 15) as u8);
            }
        }
        let checksum = out[..CHECKSUM_LEN]
            .iter()
            .fold(0u8, |sum, &byte| sum.wrapping_add(byte));
        out[42] = b'*';
        out[43] = hex(checksum >> 4);
        out[44] = hex(checksum & 15);
        out[45] = b'\r';
        out[46] = b'\n';
        out
    }
}

const fn hex(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'A' + value - 10
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockSnapshot {
    pub ctrl: u32,
    pub pllcfg: u32,
    pub cfg: u32,
    pub misc1: u32,
    pub misc2: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockError {
    ChangingSource,
    OscillatorNotReady,
    UnsupportedSource,
    HickDivider,
    PllConfiguration,
    FrequencyRange,
    BaudError,
}

impl ClockSnapshot {
    /// Nominal APB2 frequency for HICK or HICK-sourced PLL. No clock register
    /// is changed. External crystal frequency is deliberately not guessed.
    pub fn apb2_hz(self) -> Result<u32, ClockError> {
        self.uart_clock().map(|(hz, _)| hz)
    }

    fn uart_clock(self) -> Result<(u32, u16), ClockError> {
        let requested = self.cfg & 3;
        let active = (self.cfg >> 2) & 3;
        if requested != active {
            return Err(ClockError::ChangingSource);
        }
        if self.ctrl & 3 != 3 {
            return Err(ClockError::OscillatorNotReady);
        }
        let system = match active {
            0 => {
                let divider = (self.misc2 >> 16) & 7;
                if divider > 4 {
                    return Err(ClockError::HickDivider);
                }
                let hz = if self.misc1 & ((1 << 12) | (1 << 14)) == ((1 << 12) | (1 << 14)) {
                    48_000_000
                } else {
                    8_000_000
                };
                hz >> divider
            }
            2 => {
                if self.ctrl & ((1 << 24) | (1 << 25)) != ((1 << 24) | (1 << 25)) {
                    return Err(ClockError::OscillatorNotReady);
                }
                let ms = self.pllcfg & 15;
                let ns = (self.pllcfg >> 6) & 0x1ff;
                let fr = (self.pllcfg >> 16) & 7;
                // Vendor limits: NS31..500, MS1..15, FR1/2/4/8/16/32.
                // The 500..=1000 MHz VCO range is 62.5..=125 times the 8 MHz input.
                if self.pllcfg & (1 << 22) != 0
                    || ms == 0
                    || !(31..=500).contains(&ns)
                    || fr > 5
                    || self.misc1 & (1 << 12) == 0
                    || ms > 4
                    || ns * 2 < ms * 125
                    || ns > ms * 125
                {
                    return Err(ClockError::PllConfiguration);
                }
                // NS <= 500 keeps the numerator in u32.
                (8_000_000 * ns) / (ms * (2 << fr))
            }
            _ => return Err(ClockError::UnsupportedSource),
        };
        if system == 0 || system > 150_000_000 {
            return Err(ClockError::FrequencyRange);
        }
        let ahb_code = ((self.cfg >> 4) & 15) as usize;
        let apb2_code = ((self.cfg >> 13) & 7) as usize;
        let ahb_shift = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 6, 7, 8, 9][ahb_code];
        let apb2_shift = [0, 0, 0, 0, 1, 2, 3, 4][apb2_code];
        let hz = system >> (ahb_shift + apb2_shift);
        Ok((hz, baud_divisor(hz)?))
    }
}

/// Rounded APB2 / baud, matching the vendor USART initialization. Reject
/// unsupported oversampling ratios and a nominal baud error above 2%.
pub fn baud_divisor(pclk2_hz: u32) -> Result<u16, ClockError> {
    if !(BAUD * 16..=150_000_000).contains(&pclk2_hz) {
        return Err(ClockError::FrequencyRange);
    }
    let div = (pclk2_hz + BAUD / 2) / BAUD;
    if !(16..=u32::from(u16::MAX)).contains(&div) {
        return Err(ClockError::FrequencyRange);
    }
    let error = pclk2_hz.abs_diff(div * BAUD);
    // Rounding bounds the error to BAUD / 2. The baud target is
    // within BAUD / 2 of the 150 MHz input limit.
    if error * 100 > div * BAUD * 2 {
        return Err(ClockError::BaudError);
    }
    Ok(div as u16)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Context,
    Family(u32),
    PinLocked(u32),
    Clock(ClockError),
    ClockChanged,
    ClockMismatch { observed: u32, expected: u32 },
    InvalidLimits,
    NotConfigured,
    Muted,
    FrameBudget,
    Timeout { flag: u32, sent: u8 },
    ConfigurationReadback,
}

impl From<ClockError> for Error {
    fn from(value: ClockError) -> Self {
        Self::Clock(value)
    }
}

/// Exclusive register access for the boot phase. Implementors must serialize
/// shared GPIO/CRM read-modify-write operations and prevent competing writers.
pub trait RegisterIo {
    fn read32(&mut self, address: u32) -> u32;
    fn write32(&mut self, address: u32, value: u32);
    fn barrier(&mut self);
}

pub struct Logger<I> {
    io: I,
    clock: Option<ClockSnapshot>,
    frames_left: u16,
    reserved_frames_left: u16,
    radio_quotas: Option<RadioQuotas>,
    polls_per_wait: u32,
    muted: bool,
}

struct RadioQuotas {
    state: u8,
    adc: u8,
    ui: u8,
    timing: u8,
    throughput: u8,
    page_timing: u8,
    metadata: u8,
    signal: u8,
    rf_words: u8,
}
impl RadioQuotas {
    const fn new() -> Self {
        Self {
            state: 20,
            adc: 32,
            ui: 32,
            timing: 8,
            throughput: 8,
            page_timing: 8,
            metadata: 2,
            signal: 24,
            rf_words: 6,
        }
    }
    fn take(&mut self, code: u32) -> bool {
        let remaining = match code {
            0x170 => &mut self.state,
            0x171 => &mut self.adc,
            0x132 => &mut self.ui,
            0x172 => &mut self.timing,
            0x176 => &mut self.throughput,
            0x177 => &mut self.page_timing,
            0x173 => &mut self.metadata,
            0x174 => &mut self.signal,
            0x175 => &mut self.rf_words,
            _ => return true,
        };
        if *remaining == 0 {
            false
        } else {
            *remaining -= 1;
            true
        }
    }
}

const fn is_fault_frame(code: u32) -> bool {
    matches!(code, 0xE000..=0xEFFF | 0xE000_0000..=0xEFFF_FFFF |
        0x1F00_0000..=0x1F00_0007 | 0x173)
}

impl<I: RegisterIo> Logger<I> {
    pub fn new(io: I) -> Self {
        Self {
            io,
            clock: None,
            frames_left: DEFAULT_MAX_FRAMES,
            reserved_frames_left: 0,
            radio_quotas: None,
            polls_per_wait: DEFAULT_POLL_BUDGET,
            muted: false,
        }
    }

    /// Profile for interactive radio diagnostics. The total is finite and
    /// noisy ADC/UI/RF events have independent caps. UART failure still mutes
    /// every class: the fatal reserve never retries a broken UART or clock.
    pub fn radio_profile(io: I) -> Self {
        Self {
            io,
            clock: None,
            frames_left: RADIO_MAX_FRAMES,
            reserved_frames_left: RADIO_FAULT_RESERVE,
            radio_quotas: Some(RadioQuotas::new()),
            polls_per_wait: DEFAULT_POLL_BUDGET,
            muted: false,
        }
    }

    pub fn with_limits(io: I, max_frames: u16, polls_per_wait: u32) -> Result<Self, Error> {
        if max_frames == 0
            || max_frames > HARD_MAX_FRAMES
            || polls_per_wait == 0
            || polls_per_wait > DEFAULT_POLL_BUDGET
        {
            return Err(Error::InvalidLimits);
        }
        Ok(Self {
            io,
            clock: None,
            frames_left: max_frames,
            reserved_frames_left: 0,
            radio_quotas: None,
            polls_per_wait,
            muted: false,
        })
    }

    pub fn frames_left(&self) -> u16 {
        self.frames_left
    }

    /// Remaining total includes this protected portion in the radio profile.
    pub fn fault_reserve_left(&self) -> u16 {
        self.reserved_frames_left
    }

    fn family(&mut self) -> Result<(), Error> {
        let value = self.io.read32(register::SERIES_REVISION);
        if value & register::FAMILY_MASK != register::FAMILY_VALUE {
            return Err(Error::Family(value));
        }
        Ok(())
    }

    fn snapshot(&mut self) -> ClockSnapshot {
        ClockSnapshot {
            ctrl: self.io.read32(register::CRM_CTRL),
            pllcfg: self.io.read32(register::CRM_PLLCFG),
            cfg: self.io.read32(register::CRM_CFG),
            misc1: self.io.read32(register::CRM_MISC1),
            misc2: self.io.read32(register::CRM_MISC2),
        }
    }

    fn stable_clock(&mut self) -> Result<ClockSnapshot, Error> {
        let first = self.snapshot();
        self.io.barrier();
        if first != self.snapshot() {
            return Err(Error::ClockChanged);
        }
        Ok(first)
    }

    fn modify(&mut self, address: u32, clear: u32, set: u32) {
        let before = self.io.read32(address);
        self.io.write32(address, (before & !clear) | set);
    }

    fn wait(&mut self, flag: u32, sent: u8, frame_budget: &mut u32) -> Result<(), Error> {
        for _ in 0..self.polls_per_wait {
            if *frame_budget == 0 {
                break;
            }
            *frame_budget -= 1;
            if self.io.read32(register::USART_STS) & flag != 0 {
                return Ok(());
            }
        }
        self.muted = true;
        Err(Error::Timeout { flag, sent })
    }

    pub fn configure_for_current_clock(&mut self) -> Result<u32, Error> {
        self.configure_for_clock(None)
    }

    /// Reconfigure after a known clock transition. Failure never resets the
    /// frame budget, and a timed-out logger cannot be revived by reconfigure.
    pub fn configure_at_pclk2(&mut self, pclk2_hz: u32) -> Result<(), Error> {
        self.configure_for_clock(Some(pclk2_hz)).map(|_| ())
    }

    #[inline(never)]
    fn configure_for_clock(&mut self, expected_hz: Option<u32>) -> Result<u32, Error> {
        if self.muted {
            return Err(Error::Muted);
        }
        self.family()?;
        let snapshot = self.stable_clock()?;
        let (hz, divisor) = snapshot.uart_clock()?;
        if let Some(expected) = expected_hz {
            if hz != expected {
                return Err(Error::ClockMismatch {
                    observed: hz,
                    expected,
                });
            }
        }
        self.configure(snapshot, divisor)?;
        Ok(hz)
    }

    fn configure(&mut self, snapshot: ClockSnapshot, divisor: u16) -> Result<(), Error> {
        use register::*;
        // Only GPIOA's gate is enabled to inspect its lock. A locked TX pin
        // causes no GPIO configuration or UART writes.
        self.modify(CRM_AHBEN1, 0, GPIOA_GATE);
        self.io.barrier();
        let lock = self.io.read32(GPIO_LOCK);
        if lock & ((1 << 16) | TX_PIN) == ((1 << 16) | TX_PIN) {
            return Err(Error::PinLocked(lock));
        }
        if self.clock.is_some() {
            let mut budget = FRAME_POLL_BUDGET;
            self.wait(TDC, 0, &mut budget)?;
        }
        self.modify(CRM_APB2EN, 0, USART6_GATE);
        self.io.barrier();
        self.modify(CRM_APB2RST, 0, USART6_GATE);
        self.modify(CRM_APB2RST, USART6_GATE, 0);
        self.io.barrier();
        // TX only, 8 data bits, no parity, one stop bit. No receiver, IRQ,
        // DMA, inversion, flow control, single-wire, smartcard, or LIN mode.
        self.io.write32(USART_CTRL1, 0);
        self.io.write32(USART_CTRL2, 0);
        self.io.write32(USART_CTRL3, 0);
        self.io.write32(USART_BAUDR, u32::from(divisor));
        self.modify(GPIO_OMODE, TX_PIN, 0);
        self.modify(GPIO_PULL, TX_MODE_MASK, 0);
        self.modify(GPIO_ODRVR, TX_MODE_MASK, TX_DRIVE);
        self.modify(GPIO_MUXH, TX_AF_MASK, TX_AF);
        // Enable the idle-high transmitter before exposing PA11's AF mode.
        self.io.write32(USART_CTRL1, USART_TX_ONLY);
        self.modify(GPIO_CFGR, TX_MODE_MASK, TX_MODE_AF);
        self.io.barrier();
        if self.io.read32(USART_CTRL1) != USART_TX_ONLY
            || self.io.read32(USART_CTRL2) != 0
            || self.io.read32(USART_CTRL3) != 0
            || self.io.read32(USART_BAUDR) != u32::from(divisor)
            || self.io.read32(GPIO_CFGR) & TX_MODE_MASK != TX_MODE_AF
            || self.io.read32(GPIO_MUXH) & TX_AF_MASK != TX_AF
        {
            self.muted = true;
            return Err(Error::ConfigurationReadback);
        }
        match self.stable_clock() {
            Ok(current) if snapshot == current => {}
            _ => {
                self.muted = true;
                return Err(Error::ClockChanged);
            }
        }
        self.clock = Some(snapshot);
        Ok(())
    }

    pub fn emit(&mut self, frame: Frame) -> Result<(), Error> {
        if self.muted {
            return Err(Error::Muted);
        }
        let _ = self.clock.ok_or(Error::NotConfigured)?;
        let fault = is_fault_frame(frame.code);
        if self.frames_left == 0 || !fault && self.frames_left <= self.reserved_frames_left {
            return Err(Error::FrameBudget);
        }
        if self
            .radio_quotas
            .as_mut()
            .is_some_and(|quotas| !quotas.take(frame.code))
        {
            return Err(Error::FrameBudget);
        }
        if fault && self.frames_left <= self.reserved_frames_left {
            self.reserved_frames_left -= 1;
        }
        self.frames_left -= 1;
        self.emit_wire(frame)
    }

    #[cfg(feature = "pa5-audio")]
    pub fn emit_audio(
        &mut self,
        frame: Frame,
        budget: &mut AudioExportBudget,
    ) -> Result<(), Error> {
        if budget.0 == 0
            || !(matches!(
                frame.code,
                0xA510_0000..=0xA510_0002 | 0xA51F_0000 | PA5_CONTEXT
            ) || (0xA520_0000..0xA520_0156).contains(&frame.code))
        {
            return Err(Error::FrameBudget);
        }
        budget.0 -= 1;
        self.emit_wire(frame)
    }

    fn emit_wire(&mut self, frame: Frame) -> Result<(), Error> {
        if self.muted {
            return Err(Error::Muted);
        }
        let configured = self.clock.ok_or(Error::NotConfigured)?;
        match self.stable_clock() {
            Ok(current) if configured == current => {}
            _ => {
                self.muted = true;
                return Err(Error::ClockChanged);
            }
        }
        let bytes = frame.encode();
        let mut budget = FRAME_POLL_BUDGET;
        for (sent, byte) in bytes.into_iter().enumerate() {
            self.wait(register::TDBE, sent as u8, &mut budget)?;
            self.io.write32(register::USART_DT, u32::from(byte));
        }
        self.wait(register::TDC, FRAME_LEN as u8, &mut budget)
    }

    /// Stop this logger without touching peripheral configuration. Safe to
    /// call after a failure. There are deliberately no hardware Drop effects.
    pub fn disable(&mut self) {
        self.muted = true;
    }
}

#[cfg(all(feature = "hardware", target_arch = "arm"))]
pub mod hardware {
    use super::*;
    use at32f4xx_pac::at32f423::USART6;
    use core::marker::PhantomData;
    use uv98_board_pins::BootLogPins;

    /// Non-owning boot-phase accessor. Must be dropped before the owned logger
    /// is constructed; no global mutable reference or peripheral steal exists.
    pub struct RawMmio {
        _single_context: PhantomData<*mut ()>,
    }
    impl RawMmio {
        /// # Safety
        /// AT32F423 privileged thread mode, IRQs masked, valid stack/RAM. The
        /// caller exclusively reserves USART6 and PA11 until this value and
        /// its Logger are dropped. No interrupt/DMA/core may modify the shared
        /// GPIOA or CRM registers during read-modify-write. No concurrent raw
        /// logging, and no clock change while configure/emit executes.
        pub unsafe fn new() -> Self {
            Self {
                _single_context: PhantomData,
            }
        }
    }
    impl RegisterIo for RawMmio {
        fn read32(&mut self, address: u32) -> u32 {
            unsafe { core::ptr::read_volatile(address as *const u32) }
        }
        fn write32(&mut self, address: u32, value: u32) {
            unsafe { core::ptr::write_volatile(address as *mut u32, value) }
        }
        fn barrier(&mut self) {
            unsafe { core::arch::asm!("dsb", "isb", options(nostack, preserves_flags)) }
        }
    }

    pub struct TxLogger {
        logger: Logger<RawMmio>,
        _usart: USART6,
        _tx: BootLogPins,
    }
    impl TxLogger {
        pub fn new(usart: USART6, tx: BootLogPins, pclk2_hz: u32) -> Result<Self, Error> {
            Self::new_profile(usart, tx, pclk2_hz, false)
        }
        /// Explicit0.1.8 capacity/quotas with fatal reserve; wire protocol unchanged.
        pub fn new_radio(usart: USART6, tx: BootLogPins, pclk2_hz: u32) -> Result<Self, Error> {
            Self::new_profile(usart, tx, pclk2_hz, true)
        }
        fn new_profile(
            usart: USART6,
            tx: BootLogPins,
            pclk2_hz: u32,
            radio: bool,
        ) -> Result<Self, Error> {
            let (ipsr, control, primask): (u32, u32, u32);
            unsafe {
                core::arch::asm!(
                    "mrs {ipsr}, IPSR", "mrs {control}, CONTROL", "mrs {primask}, PRIMASK",
                    ipsr = out(reg) ipsr, control = out(reg) control, primask = out(reg) primask,
                    options(nomem, nostack, preserves_flags),
                );
            }
            if ipsr != 0 || control & 3 != 0 || primask & 1 == 0 {
                return Err(Error::Context);
            }
            // The capability split gives exclusive USART6/PA11 ownership.
            // Main keeps interrupts masked and bus masters stopped throughout.
            let io = unsafe { RawMmio::new() };
            let mut logger = if radio {
                Logger::radio_profile(io)
            } else {
                Logger::new(io)
            };
            logger.configure_at_pclk2(pclk2_hz)?;
            Ok(Self {
                logger,
                _usart: usart,
                _tx: tx,
            })
        }
        pub fn emit(&mut self, frame: Frame) -> Result<(), Error> {
            self.logger.emit(frame)
        }
        #[cfg(feature = "pa5-audio")]
        pub fn emit_audio(
            &mut self,
            frame: Frame,
            budget: &mut AudioExportBudget,
        ) -> Result<(), Error> {
            self.logger.emit_audio(frame, budget)
        }
        pub fn configure_at_pclk2(&mut self, hz: u32) -> Result<(), Error> {
            self.logger.configure_at_pclk2(hz)
        }
        pub fn configure_for_current_clock(&mut self) -> Result<u32, Error> {
            self.logger.configure_for_current_clock()
        }
        pub fn frames_left(&self) -> u16 {
            self.logger.frames_left()
        }
        pub fn fault_reserve_left(&self) -> u16 {
            self.logger.fault_reserve_left()
        }
        pub fn disable(&mut self) {
            self.logger.disable();
        }
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use register::*;
    use std::collections::BTreeMap;
    use std::vec::Vec;

    #[derive(Default)]
    struct FakeIo {
        regs: BTreeMap<u32, u32>,
        writes: Vec<(u32, u32)>,
        reads: Vec<u32>,
        status_reads: usize,
        stall_after: Option<usize>,
        ready_every: Option<usize>,
        drift_at_cfg_read: Option<usize>,
        cfg_reads: usize,
    }
    impl FakeIo {
        fn hick() -> Self {
            let mut io = Self::default();
            io.regs.insert(SERIES_REVISION, 0x1000_1200);
            io.regs.insert(CRM_CTRL, 3);
            io.regs.insert(USART_STS, TDBE | TDC);
            io
        }
    }
    impl RegisterIo for FakeIo {
        fn read32(&mut self, address: u32) -> u32 {
            self.reads.push(address);
            if address == USART_STS {
                self.status_reads += 1;
                if self.stall_after.is_some_and(|n| self.status_reads > n) {
                    return 0;
                }
                if self.ready_every.is_some_and(|n| self.status_reads % n != 0) {
                    return 0;
                }
            }
            if address == CRM_CFG {
                self.cfg_reads += 1;
                if self.drift_at_cfg_read.is_some_and(|n| self.cfg_reads >= n) {
                    return 1;
                }
            }
            self.regs.get(&address).copied().unwrap_or(0)
        }
        fn write32(&mut self, address: u32, value: u32) {
            self.writes.push((address, value));
            self.regs.insert(address, value);
        }
        fn barrier(&mut self) {}
    }

    fn configured() -> Logger<FakeIo> {
        let mut logger = Logger::new(FakeIo::hick());
        assert_eq!(logger.configure_for_current_clock(), Ok(8_000_000));
        logger
    }

    #[test]
    #[cfg(feature = "pa5-audio")]
    fn manual_audio_export_has_an_independent_finite_budget() {
        let mut logger = configured();
        let original = logger.frames_left();
        let mut budget = AudioExportBudget::default();
        assert_eq!(
            logger.emit_audio(Frame::new(0xE001, [0; 3]), &mut budget),
            Err(Error::FrameBudget)
        );
        logger
            .emit_audio(Frame::new(PA5_CONTEXT, [0; 3]), &mut budget)
            .unwrap();
        for _ in 1..PA5_EXPORT_FRAMES {
            logger
                .emit_audio(Frame::new(0xA510_0000, [0; 3]), &mut budget)
                .unwrap();
        }
        assert_eq!(logger.frames_left(), original);
        assert_eq!(
            logger.emit_audio(Frame::new(0xA51F_0000, [0; 3]), &mut budget),
            Err(Error::FrameBudget)
        );
        logger.disable();
        assert_eq!(
            logger.emit_audio(
                Frame::new(0xA510_0000, [0; 3]),
                &mut AudioExportBudget::default()
            ),
            Err(Error::Muted)
        );
    }

    #[test]
    fn frame_is_exact_uppercase_fixed_width_and_sum8() {
        let bytes = Frame::new(0x0123_abcd, [0, 0xffff_ffff, 0x8000_0000]).encode();
        assert_eq!(&bytes[..42], b"UV98D1 0123ABCD 00000000 FFFFFFFF 80000000");
        let checksum = bytes[..42].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        assert_eq!(
            &bytes[42..],
            &[b'*', hex(checksum >> 4), hex(checksum & 15), b'\r', b'\n']
        );
        assert_eq!(bytes.len(), FRAME_LEN);
    }

    #[test]
    fn clocks_cover_hick_dividers_and_stock_pll_without_external_guess() {
        let mut s = ClockSnapshot {
            ctrl: 3,
            pllcfg: 0,
            cfg: 0,
            misc1: 0,
            misc2: 0,
        };
        assert_eq!(s.apb2_hz(), Ok(8_000_000));
        assert_eq!(baud_divisor(8_000_000), Ok(69));
        s.misc1 = (1 << 12) | (1 << 14);
        assert_eq!(s.apb2_hz(), Ok(48_000_000));
        s.misc2 = 1 << 16;
        assert_eq!(s.apb2_hz(), Ok(24_000_000));
        s.misc2 = 7 << 16;
        assert_eq!(s.apb2_hz(), Err(ClockError::HickDivider));
        s.ctrl = 0x0300_0003;
        s.pllcfg = 0x31801;
        s.cfg = 0x100a;
        s.misc1 = 0x1000;
        s.misc2 = 0;
        assert_eq!(s.apb2_hz(), Ok(48_000_000));
        assert_eq!(baud_divisor(48_000_000), Ok(417));
        s.cfg |= 4 << 13;
        assert_eq!(s.apb2_hz(), Ok(24_000_000));
        s.pllcfg |= 1 << 22;
        assert_eq!(s.apb2_hz(), Err(ClockError::PllConfiguration));
        s.cfg = 5;
        assert_eq!(s.apb2_hz(), Err(ClockError::UnsupportedSource));
        s.cfg = 1;
        assert_eq!(s.apb2_hz(), Err(ClockError::ChangingSource));
    }

    #[test]
    fn guards_reject_unknown_family_clock_and_locked_tx_before_configuration() {
        let mut io = FakeIo::hick();
        io.regs.insert(SERIES_REVISION, 0x1300);
        let mut logger = Logger::new(io);
        assert_eq!(
            logger.configure_for_current_clock(),
            Err(Error::Family(0x1300))
        );
        assert!(logger.io.writes.is_empty());
        let mut io = FakeIo::hick();
        io.regs.insert(CRM_CFG, 5);
        let mut logger = Logger::new(io);
        assert_eq!(
            logger.configure_for_current_clock(),
            Err(Error::Clock(ClockError::UnsupportedSource))
        );
        assert!(logger.io.writes.is_empty());
        let mut io = FakeIo::hick();
        io.regs.insert(GPIO_LOCK, 0x1_0800);
        let mut logger = Logger::new(io);
        assert_eq!(
            logger.configure_for_current_clock(),
            Err(Error::PinLocked(0x1_0800))
        );
        assert!(logger
            .io
            .writes
            .iter()
            .all(|&(address, _)| address == CRM_AHBEN1));
    }

    #[test]
    fn init_preserves_other_pins_clocks_and_resets_and_never_enables_receive() {
        let mut io = FakeIo::hick();
        for address in [
            GPIO_CFGR,
            GPIO_OMODE,
            GPIO_PULL,
            GPIO_ODRVR,
            GPIO_MUXH,
            CRM_AHBEN1,
            CRM_APB2EN,
            CRM_APB2RST,
        ] {
            io.regs.insert(address, 0xaaaa_aaaa);
        }
        let mut logger = Logger::new(io);
        logger.configure_at_pclk2(8_000_000).unwrap();
        for (address, mask) in [
            (GPIO_CFGR, TX_MODE_MASK),
            (GPIO_OMODE, TX_PIN),
            (GPIO_PULL, TX_MODE_MASK),
            (GPIO_ODRVR, TX_MODE_MASK),
            (GPIO_MUXH, TX_AF_MASK),
            (CRM_AHBEN1, GPIOA_GATE),
            (CRM_APB2EN, USART6_GATE),
            (CRM_APB2RST, USART6_GATE),
        ] {
            assert_eq!(logger.io.regs[&address] & !mask, 0xaaaa_aaaa & !mask);
        }
        assert_eq!(logger.io.regs[&USART_CTRL1], USART_TX_ONLY);
        assert_eq!(logger.io.regs[&USART_CTRL2], 0);
        assert_eq!(logger.io.regs[&USART_CTRL3], 0);
        assert_eq!(logger.io.regs[&USART_BAUDR], 69);
        assert!(!logger.io.reads.contains(&USART_DT));
        assert!(!logger.io.writes.iter().any(|&(a, _)| a == USART_DT));
    }

    #[test]
    fn success_emits_one_frame_then_drains_transmission() {
        let mut logger = configured();
        let frame = Frame::new(5, [1, 2, 3]);
        logger.emit(frame).unwrap();
        let sent: Vec<_> = logger
            .io
            .writes
            .iter()
            .filter(|(a, _)| *a == USART_DT)
            .map(|(_, v)| *v as u8)
            .collect();
        assert_eq!(sent, frame.encode());
        assert_eq!(logger.io.status_reads, FRAME_LEN + 1);
        assert_eq!(logger.frames_left(), DEFAULT_MAX_FRAMES - 1);
    }

    #[test]
    fn hick_to_pll_reconfiguration_retains_proven_pa11_drive_and_other_pins() {
        let mut io = FakeIo::hick();
        io.regs.insert(GPIO_ODRVR, 0xaaaa_aaaa);
        let mut logger = Logger::new(io);
        assert_eq!(logger.configure_for_current_clock(), Ok(8_000_000));
        assert_eq!(logger.io.regs[&GPIO_ODRVR] & 0x00c0_0000, 0x0040_0000);
        assert_eq!(logger.io.regs[&USART_BAUDR], 69);

        // Main changes clocks with the drained logger paused, then configures
        // it again. This transition must not undo the observed pin profile.
        logger.io.regs.insert(CRM_CTRL, 0x0300_0003);
        logger.io.regs.insert(CRM_PLLCFG, 0x31801);
        logger.io.regs.insert(CRM_CFG, 0x100a);
        logger.io.regs.insert(CRM_MISC1, 0x1000);
        logger.configure_at_pclk2(48_000_000).unwrap();
        assert_eq!(logger.io.regs[&USART_BAUDR], 417);
        assert_eq!(logger.io.regs[&GPIO_ODRVR] & 0x00c0_0000, 0x0040_0000);
        assert_eq!(
            logger.io.regs[&GPIO_ODRVR] & !0x00c0_0000,
            0xaaaa_aaaa & !0x00c0_0000
        );
    }

    #[test]
    fn stalled_uart_is_bounded_poisoned_and_never_retried() {
        let mut logger = Logger::with_limits(FakeIo::hick(), 2, 3).unwrap();
        logger.configure_for_current_clock().unwrap();
        logger.io.stall_after = Some(5);
        assert_eq!(
            logger.emit(Frame::new(1, [0; 3])),
            Err(Error::Timeout {
                flag: TDBE,
                sent: 5
            })
        );
        assert_eq!(logger.io.status_reads, 8);
        let reads = logger.io.reads.len();
        let writes = logger.io.writes.len();
        assert_eq!(logger.emit(Frame::new(2, [0; 3])), Err(Error::Muted));
        assert_eq!(logger.configure_for_current_clock(), Err(Error::Muted));
        assert_eq!(logger.io.reads.len(), reads);
        assert_eq!(logger.io.writes.len(), writes);
    }

    #[test]
    fn final_drain_timeout_is_distinct_and_no_extra_bytes_follow() {
        let mut logger = Logger::with_limits(FakeIo::hick(), 2, 2).unwrap();
        logger.configure_for_current_clock().unwrap();
        logger.io.stall_after = Some(FRAME_LEN);
        assert_eq!(
            logger.emit(Frame::new(1, [0; 3])),
            Err(Error::Timeout {
                flag: TDC,
                sent: 47
            })
        );
        assert_eq!(
            logger
                .io
                .writes
                .iter()
                .filter(|(a, _)| *a == USART_DT)
                .count(),
            FRAME_LEN
        );
    }

    #[test]
    fn whole_frame_budget_bounds_a_uart_that_keeps_becoming_ready_late() {
        let mut logger = configured();
        logger.io.ready_every = Some(8_000);
        assert_eq!(
            logger.emit(Frame::new(1, [0; 3])),
            Err(Error::Timeout {
                flag: TDBE,
                sent: 8
            })
        );
        assert_eq!(logger.io.status_reads, FRAME_POLL_BUDGET as usize);
        assert_eq!(
            logger
                .io
                .writes
                .iter()
                .filter(|(a, _)| *a == USART_DT)
                .count(),
            8
        );
    }

    #[test]
    fn peripheral_writes_are_confined_to_uart_and_pa11_configuration() {
        let mut logger = configured();
        logger.emit(Frame::new(1, [0; 3])).unwrap();
        let allowed = [
            CRM_AHBEN1,
            CRM_APB2EN,
            CRM_APB2RST,
            GPIO_CFGR,
            GPIO_OMODE,
            GPIO_PULL,
            GPIO_ODRVR,
            GPIO_MUXH,
            USART_CTRL1,
            USART_CTRL2,
            USART_CTRL3,
            USART_BAUDR,
            USART_DT,
        ];
        assert!(logger
            .io
            .writes
            .iter()
            .all(|(address, _)| allowed.contains(address)));
        // In particular: no RX reads, watchdog refreshes, NOR/flash writes,
        // RF GPIO writes, USB enable, or oscillator/clock-source writes.
        assert!(!logger.io.reads.contains(&USART_DT));
    }

    #[test]
    fn uart_family_gate_does_not_hide_an_unsupported_revision_from_startup() {
        let mut io = FakeIo::hick();
        io.regs.insert(SERIES_REVISION, 0x1000_1203);
        let mut logger = Logger::new(io);
        assert_eq!(logger.configure_for_current_clock(), Ok(8_000_000));
        // This grants only the cable log. The startup crate retains the
        // narrower FF07 revision check before any RF or application use.
    }

    #[test]
    fn limit_cannot_be_extended_by_clock_reconfiguration() {
        let mut logger = Logger::with_limits(FakeIo::hick(), 1, 1).unwrap();
        logger.configure_for_current_clock().unwrap();
        logger.emit(Frame::new(1, [0; 3])).unwrap();
        logger.configure_for_current_clock().unwrap();
        let writes = logger.io.writes.len();
        assert_eq!(logger.emit(Frame::new(2, [0; 3])), Err(Error::FrameBudget));
        assert_eq!(logger.io.writes.len(), writes);
        assert!(matches!(
            Logger::with_limits(FakeIo::hick(), HARD_MAX_FRAMES + 1, 1),
            Err(Error::InvalidLimits)
        ));
    }

    #[test]
    fn radio_profile_normal_flood_cannot_consume_fatal_header_and_text_reserve() {
        assert!(RADIO_MAX_FRAMES <= HARD_MAX_FRAMES);
        let mut logger = Logger::radio_profile(FakeIo::hick());
        logger.configure_for_current_clock().unwrap();
        for _ in 0..RADIO_MAX_FRAMES - RADIO_FAULT_RESERVE {
            logger.emit(Frame::new(0x131, [0; 3])).unwrap();
        }
        assert_eq!(logger.frames_left(), RADIO_FAULT_RESERVE);
        let reads = logger.io.reads.len();
        let writes = logger.io.writes.len();
        assert_eq!(
            logger.emit(Frame::new(0x100, [0; 3])),
            Err(Error::FrameBudget)
        );
        assert_eq!(
            (logger.io.reads.len(), logger.io.writes.len()),
            (reads, writes)
        );
        logger.emit(Frame::new(0x173, [4, 0xe055, 3])).unwrap();
        let detail = error_text::ErrorText::format(&[u32::MAX; 32]);
        logger.emit(Frame::new(0xe055, detail.metadata())).unwrap();
        for frame in detail.frames() {
            logger.emit(frame).unwrap();
        }
        assert_eq!(logger.fault_reserve_left(), RADIO_FAULT_RESERVE - 10);
        while logger.frames_left() > 0 {
            logger.emit(Frame::new(0xe001, [0; 3])).unwrap();
        }
        assert_eq!(
            logger.emit(Frame::new(0xe0ff, [0; 3])),
            Err(Error::FrameBudget)
        );
        assert_eq!(
            logger
                .io
                .writes
                .iter()
                .filter(|(a, _)| *a == USART_DT)
                .count(),
            usize::from(RADIO_MAX_FRAMES) * FRAME_LEN
        );
    }

    #[test]
    fn radio_event_quotas_are_independent_and_rejections_do_no_io() {
        for (code, limit) in [
            (0x170, 20),
            (0x171, 32),
            (0x132, 32),
            (0x172, 8),
            (0x176, 8),
            (0x177, 8),
            (0x173, 2),
            (0x174, 24),
            (0x175, 6),
        ] {
            let mut logger = Logger::radio_profile(FakeIo::hick());
            logger.configure_for_current_clock().unwrap();
            for _ in 0..limit {
                logger.emit(Frame::new(code, [0; 3])).unwrap();
            }
            let reads = logger.io.reads.len();
            let writes = logger.io.writes.len();
            let left = logger.frames_left();
            for _ in 0..100 {
                assert_eq!(
                    logger.emit(Frame::new(code, [0; 3])),
                    Err(Error::FrameBudget)
                );
            }
            assert_eq!(
                (
                    logger.io.reads.len(),
                    logger.io.writes.len(),
                    logger.frames_left()
                ),
                (reads, writes, left)
            );
            logger.emit(Frame::new(0xe055, [0; 3])).unwrap();
            logger.emit(Frame::new(0x120, [0; 3])).unwrap();
        }
    }

    #[test]
    fn radio_quota_and_reserve_are_not_replenished_by_reconfiguration() {
        let mut logger = Logger::radio_profile(FakeIo::hick());
        logger.configure_for_current_clock().unwrap();
        for _ in 0..20 {
            logger.emit(Frame::new(0x170, [0; 3])).unwrap();
        }
        logger.configure_for_current_clock().unwrap();
        assert_eq!(logger.frames_left(), RADIO_MAX_FRAMES - 20);
        assert_eq!(
            logger.emit(Frame::new(0x170, [0; 3])),
            Err(Error::FrameBudget)
        );
        for _ in 0..RADIO_MAX_FRAMES - RADIO_FAULT_RESERVE - 20 {
            logger.emit(Frame::new(0x131, [0; 3])).unwrap();
        }
        logger.emit(Frame::new(0xe010, [0; 3])).unwrap();
        logger.configure_for_current_clock().unwrap();
        assert_eq!(logger.fault_reserve_left(), RADIO_FAULT_RESERVE - 1);
        assert_eq!(
            logger.emit(Frame::new(0x131, [0; 3])),
            Err(Error::FrameBudget)
        );
    }

    #[test]
    fn fatal_reserve_never_retries_a_uart_that_failed() {
        let mut logger = Logger::radio_profile(FakeIo::hick());
        logger.configure_for_current_clock().unwrap();
        logger.io.stall_after = Some(0);
        assert!(matches!(
            logger.emit(Frame::new(0x170, [0; 3])),
            Err(Error::Timeout { .. })
        ));
        let reads = logger.io.reads.len();
        let writes = logger.io.writes.len();
        assert_eq!(
            logger.emit(Frame::new(0xe055, [96, 1, 0])),
            Err(Error::Muted)
        );
        assert_eq!(
            logger.emit(Frame::new(0x1f00_0000, [0; 3])),
            Err(Error::Muted)
        );
        assert_eq!(
            (logger.io.reads.len(), logger.io.writes.len()),
            (reads, writes)
        );
    }

    #[test]
    fn legacy_profiles_keep_48_frames_without_radio_quotas_and_hard_limit_is_256() {
        let mut logger = configured();
        assert_eq!(logger.fault_reserve_left(), 0);
        for _ in 0..48 {
            logger.emit(Frame::new(0x170, [0; 3])).unwrap();
        }
        assert_eq!(
            logger.emit(Frame::new(0xe0ff, [0; 3])),
            Err(Error::FrameBudget)
        );
        assert!(Logger::with_limits(FakeIo::hick(), 256, 1).is_ok());
        assert!(matches!(
            Logger::with_limits(FakeIo::hick(), 257, 1),
            Err(Error::InvalidLimits)
        ));
    }

    #[test]
    fn changed_clock_prevents_transmit_and_snapshot_instability_is_rejected() {
        let mut logger = configured();
        logger.io.regs.insert(CRM_MISC1, (1 << 12) | (1 << 14));
        assert_eq!(logger.emit(Frame::new(1, [0; 3])), Err(Error::ClockChanged));
        assert!(!logger.io.writes.iter().any(|&(a, _)| a == USART_DT));
        let mut io = FakeIo::hick();
        io.drift_at_cfg_read = Some(2);
        let mut logger = Logger::new(io);
        assert_eq!(
            logger.configure_for_current_clock(),
            Err(Error::ClockChanged)
        );
        assert!(logger.io.writes.is_empty());
        let mut logger = Logger::new(FakeIo::hick());
        assert_eq!(
            logger.configure_at_pclk2(48_000_000),
            Err(Error::ClockMismatch {
                observed: 8_000_000,
                expected: 48_000_000
            })
        );
        assert!(logger.io.writes.is_empty());
    }
}
