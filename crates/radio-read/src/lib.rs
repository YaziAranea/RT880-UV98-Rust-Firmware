#![deny(unsafe_op_in_unsafe_fn)]
//! Bounded stock-protocol reading of 4 MiB; MCU Flash and bootloader are outside its scope.
//!
//! The only transmitted packets are CONNECT, 0x52 READ for blocks 0..4095,
//! and END after an acknowledged session. END may commit preexisting dirty
//! state inside stock firmware: an allowlisted transcript is not proof of zero
//! physical writes. No write/erase/test-mode/bootloader packet is implemented.
//! Time is not read by the stock CPS plan. No saved CPS file is synthesized.
//! Every raw region byte and attempted TX/received RX is retained in memory;
//! callers must save the returned snapshot or failure captures themselves.

use std::fmt;
use std::io;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

pub const BLOCK_SIZE: usize = 1024;
pub const FRAME_SIZE: usize = 1028;
pub const CONNECT: [u8; 5] = [0x34, 0x52, 0x05, 0x10, 0x9b];
pub const END: [u8; 5] = [0x34, 0x52, 0x05, 0xee, 0x79];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    Calibration,
    Cfg,
    Channels,
    Fm,
    Zones,
    Storage,
}

impl Region {
    pub const fn first_block(self) -> u16 {
        match self {
            Self::Calibration | Self::Storage => 0,
            Self::Cfg => 8,
            Self::Channels => 16,
            Self::Fm => 856,
            Self::Zones => 112,
        }
    }
    pub const fn block_count(self) -> usize {
        match self {
            Self::Calibration | Self::Cfg | Self::Fm => 4,
            Self::Channels => 48,
            Self::Zones => 128,
            Self::Storage => 4096,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadPlan {
    Calibration,
    Cps,
    CpsAndCalibration,
    FullStorage,
}

impl ReadPlan {
    pub fn regions(self) -> &'static [Region] {
        use Region::*;
        match self {
            Self::Calibration => &[Calibration],
            Self::Cps => &[Cfg, Channels, Zones, Fm],
            Self::CpsAndCalibration => &[Calibration, Cfg, Channels, Zones, Fm],
            Self::FullStorage => &[Storage],
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Calibration => "calibration",
            Self::Cps => "cps",
            Self::CpsAndCalibration => "cps_and_calibration",
            Self::FullStorage => "stock_protocol_4_mib",
        }
    }
    pub fn block_count(self) -> usize {
        self.regions().iter().map(|r| r.block_count()).sum()
    }
    fn blocks(self) -> Vec<u16> {
        self.regions()
            .iter()
            .flat_map(|r| r.first_block()..r.first_block() + r.block_count() as u16)
            .collect()
    }
}

#[derive(Clone, Default, Debug)]
pub struct CancelToken(Arc<AtomicBool>);
impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    fn check(&self) -> Result<(), ReadError> {
        if self.is_cancelled() {
            Err(ReadError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Opening,
    Connecting,
    Reading,
    Closing,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Progress {
    pub pass: u8,
    pub phase: Phase,
    pub completed_blocks: usize,
    pub total_blocks: usize,
    pub block: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameCapture {
    pub block: u16,
    /// Complete or partial response, including header/checksum when received.
    pub bytes: Vec<u8>,
    /// Header, exact frame length and checksum passed; not two-pass equality.
    pub validated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassCapture {
    pub pass: u8,
    /// Attempted packets; a short transport send remains recorded here.
    pub tx_packets: Vec<Vec<u8>>,
    pub rx: Vec<u8>,
    pub frames: Vec<FrameCapture>,
    pub complete: bool,
    /// True only if the transport reported sending all five END bytes.
    pub end_sent: bool,
    pub cleanup_error: Option<String>,
}

impl PassCapture {
    fn new(pass: u8) -> Self {
        Self {
            pass,
            tx_packets: Vec::new(),
            rx: Vec::new(),
            frames: Vec::new(),
            complete: false,
            end_sent: false,
            cleanup_error: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SectionData {
    pub region: Region,
    pub first_block: u16,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TimeProvenance {
    NotReadByStockCps,
}

/// Constructed only after both complete passes match, including successful cleanup.
#[derive(Clone, Debug)]
pub struct ReadSnapshot {
    plan: ReadPlan,
    sections: Vec<SectionData>,
    passes: Vec<PassCapture>,
}
impl ReadSnapshot {
    pub fn plan(&self) -> ReadPlan {
        self.plan
    }
    pub fn sections(&self) -> &[SectionData] {
        &self.sections
    }
    pub fn section(&self, region: Region) -> Option<&[u8]> {
        self.sections
            .iter()
            .find(|section| section.region == region)
            .map(|section| section.bytes.as_slice())
            .or_else(|| {
                if region == Region::Storage {
                    return None;
                }
                let storage = self.sections.iter().find(|s| s.region == Region::Storage)?;
                let start = usize::from(region.first_block()) * BLOCK_SIZE;
                storage
                    .bytes
                    .get(start..start + region.block_count() * BLOCK_SIZE)
            })
    }
    pub fn passes(&self) -> &[PassCapture] {
        &self.passes
    }
    pub fn time_provenance(&self) -> TimeProvenance {
        TimeProvenance::NotReadByStockCps
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadError {
    Cancelled,
    Io {
        operation: &'static str,
        message: String,
    },
    Timeout {
        operation: &'static str,
        received: usize,
        expected: usize,
    },
    UnexpectedAck(u8),
    InvalidBlock(u16),
    InvalidPortName,
    FrameLength {
        block: u16,
        actual: usize,
    },
    FrameHeader {
        block: u16,
        actual: [u8; 3],
    },
    Checksum {
        block: u16,
        expected: u8,
        actual: u8,
    },
    UnexpectedTrailingByte {
        block: u16,
        byte: u8,
    },
    ShortSend {
        expected: usize,
        actual: usize,
    },
    TransportOverflow,
    PacketNotAllowed,
    PassMismatch {
        block: u16,
        byte_offset: usize,
    },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Reading cancelled"),
            Self::Io { operation, message } => write!(f, "{operation}: {message}"),
            Self::Timeout {
                operation,
                received,
                expected,
            } => write!(f, "{operation} timeout: {received}/{expected} bytes"),
            Self::UnexpectedAck(byte) => write!(f, "Unexpected connection response: {byte:02X}"),
            Self::InvalidBlock(block) => {
                write!(f, "Block {block} is outside the fixed read regions")
            }
            Self::InvalidPortName => write!(f, "Expected a port name COM1..COM65535"),
            Self::FrameLength { block, actual } => write!(
                f,
                "Block {block}: expected {FRAME_SIZE} response bytes, got {actual}"
            ),
            Self::FrameHeader { block, actual } => {
                write!(f, "Block {block}: unexpected header/address {actual:02X?}")
            }
            Self::Checksum {
                block,
                expected,
                actual,
            } => write!(
                f,
                "Block {block}: checksum {actual:02X}, expected {expected:02X}"
            ),
            Self::UnexpectedTrailingByte { block, byte } => {
                write!(f, "Block {block}: extra response byte {byte:02X}")
            }
            Self::ShortSend { expected, actual } => {
                write!(f, "Short serial send: {actual}/{expected} bytes")
            }
            Self::TransportOverflow => write!(f, "Transport reported more bytes than requested"),
            Self::PacketNotAllowed => write!(f, "Packet is outside the fixed read-only allowlist"),
            Self::PassMismatch { block, byte_offset } => write!(
                f,
                "Independent reads differ at block {block}, payload byte {byte_offset}"
            ),
        }
    }
}
impl std::error::Error for ReadError {}

#[derive(Clone, Debug)]
pub struct ReadFailure {
    pub error: ReadError,
    pub passes: Vec<PassCapture>,
}
impl fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for ReadFailure {}

/// An opaque packet constructible only by this crate's fixed protocol engine.
#[derive(Clone, Debug)]
pub struct ReadPacket {
    bytes: Vec<u8>,
}
impl ReadPacket {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    fn fixed(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
        }
    }
    fn block(block: u16) -> Self {
        let hi = (block >> 8) as u8;
        let lo = block as u8;
        Self::fixed(&[0x52, hi, lo, 0x52u8.wrapping_add(hi).wrapping_add(lo)])
    }
}

/// Byte transport contract. Reads/sends must return within the supplied timeout.
/// Returning zero read bytes means no bytes arrived during this polling interval.
/// The engine handles fragmentation and its own total response deadline.
pub trait Transport {
    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> io::Result<usize>;
    fn send(&mut self, packet: &ReadPacket, timeout: Duration) -> io::Result<usize>;
    fn close(&mut self) -> io::Result<()>;
}

fn allowed_block(block: u16) -> bool {
    block < 4096
}

pub fn is_allowed_packet(packet: &[u8]) -> bool {
    packet == CONNECT
        || packet == END
        || (packet.len() == 4
            && packet[0] == 0x52
            && allowed_block(u16::from_be_bytes([packet[1], packet[2]]))
            && packet[..3]
                .iter()
                .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
                == packet[3])
}

pub fn validate_frame(block: u16, frame: &[u8]) -> Result<&[u8], ReadError> {
    if !allowed_block(block) {
        return Err(ReadError::InvalidBlock(block));
    }
    if frame.len() != FRAME_SIZE {
        return Err(ReadError::FrameLength {
            block,
            actual: frame.len(),
        });
    }
    let header = [0x52, (block >> 8) as u8, block as u8];
    if frame[..3] != header {
        return Err(ReadError::FrameHeader {
            block,
            actual: [frame[0], frame[1], frame[2]],
        });
    }
    let checksum = frame[..FRAME_SIZE - 1]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    if checksum != frame[FRAME_SIZE - 1] {
        return Err(ReadError::Checksum {
            block,
            expected: checksum,
            actual: frame[FRAME_SIZE - 1],
        });
    }
    Ok(&frame[3..FRAME_SIZE - 1])
}

#[derive(Clone, Copy)]
struct Timing {
    connect: Duration,
    frame: Duration,
    send: Duration,
    poll: Duration,
    gap: Duration,
    between_passes: Duration,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            connect: Duration::from_millis(200),
            frame: Duration::from_secs(3),
            send: Duration::from_secs(1),
            poll: Duration::from_millis(50),
            gap: Duration::from_millis(20),
            between_passes: Duration::from_millis(200),
        }
    }
}

fn io_error(operation: &'static str, error: io::Error) -> ReadError {
    ReadError::Io {
        operation,
        message: error.to_string(),
    }
}

fn pause(duration: Duration, cancel: &CancelToken) -> Result<(), ReadError> {
    let deadline = Instant::now() + duration;
    loop {
        cancel.check()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        std::thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

struct Session<T> {
    port: T,
    capture: PassCapture,
    acknowledged: bool,
    timing: Timing,
}
impl<T: Transport> Session<T> {
    fn send(&mut self, packet: ReadPacket) -> Result<(), ReadError> {
        if !is_allowed_packet(packet.as_bytes()) {
            return Err(ReadError::PacketNotAllowed);
        }
        self.capture.tx_packets.push(packet.as_bytes().to_vec());
        let count = self
            .port
            .send(&packet, self.timing.send)
            .map_err(|error| io_error("serial send", error))?;
        if count != packet.as_bytes().len() {
            return Err(ReadError::ShortSend {
                expected: packet.as_bytes().len(),
                actual: count,
            });
        }
        Ok(())
    }

    fn read_exact(
        &mut self,
        size: usize,
        timeout: Duration,
        operation: &'static str,
        cancel: &CancelToken,
    ) -> Result<Vec<u8>, ReadError> {
        let deadline = Instant::now() + timeout;
        let mut bytes = Vec::with_capacity(size);
        while bytes.len() < size {
            cancel.check()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ReadError::Timeout {
                    operation,
                    received: bytes.len(),
                    expected: size,
                });
            }
            let mut buffer = vec![0u8; size - bytes.len()];
            let count = self
                .port
                .read(&mut buffer, remaining.min(self.timing.poll))
                .map_err(|error| io_error("serial receive", error))?;
            if count > buffer.len() {
                return Err(ReadError::TransportOverflow);
            }
            self.capture.rx.extend_from_slice(&buffer[..count]);
            bytes.extend_from_slice(&buffer[..count]);
            if Instant::now() >= deadline {
                return Err(ReadError::Timeout {
                    operation,
                    received: bytes.len(),
                    expected: size,
                });
            }
            if count == 0 {
                std::thread::sleep(Duration::from_millis(1).min(remaining));
            }
        }
        Ok(bytes)
    }

    fn connect(&mut self, cancel: &CancelToken) -> Result<(), ReadError> {
        for attempt in 0..3 {
            cancel.check()?;
            self.send(ReadPacket::fixed(&CONNECT))?;
            let response_start = self.capture.rx.len();
            let response = self.read_exact(1, self.timing.connect, "connection", cancel);
            // A received ACK may arrive after the absolute deadline. It still
            // proves the radio entered a session, so cleanup must send END.
            // Keep the timeout fatal; do not retry CONNECT or proceed to READ.
            if self.capture.rx.get(response_start) == Some(&0x06) {
                self.acknowledged = true;
            }
            match response {
                Ok(reply) if reply[0] == 0x06 => {
                    self.acknowledged = true;
                    return Ok(());
                }
                Ok(reply) if reply[0] == 0 && attempt < 2 => pause(self.timing.connect, cancel)?,
                Ok(reply) => return Err(ReadError::UnexpectedAck(reply[0])),
                Err(error) if self.acknowledged => return Err(error),
                Err(ReadError::Timeout { .. }) if attempt < 2 => {}
                Err(error) => return Err(error),
            }
        }
        unreachable!("all three connection attempts return or fail")
    }

    /// Preserve a quiet inter-frame interval and reject surplus response bytes.
    fn quiet_gap(&mut self, block: u16, cancel: &CancelToken) -> Result<(), ReadError> {
        let deadline = Instant::now() + self.timing.gap;
        loop {
            cancel.check()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            let mut byte = [0u8; 1];
            let count = self
                .port
                .read(&mut byte, remaining.min(self.timing.poll))
                .map_err(|error| io_error("response boundary", error))?;
            if count > 1 {
                return Err(ReadError::TransportOverflow);
            }
            if count == 1 {
                self.capture.rx.push(byte[0]);
                return Err(ReadError::UnexpectedTrailingByte {
                    block,
                    byte: byte[0],
                });
            }
            std::thread::sleep(Duration::from_millis(1).min(remaining));
        }
    }

    fn read_block(&mut self, block: u16, cancel: &CancelToken) -> Result<Vec<u8>, ReadError> {
        cancel.check()?;
        self.send(ReadPacket::block(block))?;
        let start = self.capture.rx.len();
        let received = self.read_exact(FRAME_SIZE, self.timing.frame, "frame", cancel);
        let bytes = self.capture.rx[start..].to_vec();
        let validated = received.is_ok() && validate_frame(block, &bytes).is_ok();
        self.capture.frames.push(FrameCapture {
            block,
            bytes,
            validated,
        });
        let frame = received?;
        let payload = validate_frame(block, &frame)?.to_vec();
        self.quiet_gap(block, cancel)?;
        Ok(payload)
    }

    fn finish(&mut self) -> Result<(), ReadError> {
        // Cleanup ignores cancellation so an acknowledged programming session
        // gets END and close. END is not claimed to have zero stock side effects.
        let mut first_error = None;
        let mut cleanup_errors = Vec::new();
        if self.acknowledged {
            match self.send(ReadPacket::fixed(&END)) {
                Ok(()) => self.capture.end_sent = true,
                Err(error) => {
                    cleanup_errors.push(error.to_string());
                    first_error = Some(error);
                }
            }
            std::thread::sleep(self.timing.gap);
        }
        if let Err(error) = self.port.close() {
            let error = io_error("serial close", error);
            cleanup_errors.push(error.to_string());
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        if let Some(error) = first_error {
            self.capture.cleanup_error = Some(cleanup_errors.join("; "));
            Err(error)
        } else {
            Ok(())
        }
    }
}

/// Always open, read and close twice. A mismatch aborts during the second pass.
/// All captures survive any returned protocol/open/cleanup/cancellation error.
pub fn read_twice<T, F, P>(
    open_port: F,
    plan: ReadPlan,
    cancel: &CancelToken,
    on_progress: P,
) -> Result<ReadSnapshot, ReadFailure>
where
    T: Transport,
    F: FnMut() -> io::Result<T>,
    P: FnMut(Progress),
{
    read_twice_timed(open_port, plan, cancel, on_progress, Timing::default())
}

fn read_twice_timed<T, F, P>(
    mut open_port: F,
    plan: ReadPlan,
    cancel: &CancelToken,
    mut on_progress: P,
    timing: Timing,
) -> Result<ReadSnapshot, ReadFailure>
where
    T: Transport,
    F: FnMut() -> io::Result<T>,
    P: FnMut(Progress),
{
    let blocks = plan.blocks();
    let mut passes = Vec::new();
    let mut reference: Vec<Vec<u8>> = Vec::new();
    for pass in 1..=2 {
        if let Err(error) = cancel.check() {
            return Err(ReadFailure { error, passes });
        }
        on_progress(Progress {
            pass,
            phase: Phase::Opening,
            completed_blocks: 0,
            total_blocks: blocks.len(),
            block: None,
        });
        if let Err(error) = cancel.check() {
            return Err(ReadFailure { error, passes });
        }
        let port = match open_port() {
            Ok(port) => port,
            Err(error) => {
                passes.push(PassCapture::new(pass));
                return Err(ReadFailure {
                    error: io_error("open port", error),
                    passes,
                });
            }
        };
        let mut session = Session {
            port,
            capture: PassCapture::new(pass),
            acknowledged: false,
            timing,
        };
        let mut completed = 0;
        let result = (|| {
            on_progress(Progress {
                pass,
                phase: Phase::Connecting,
                completed_blocks: 0,
                total_blocks: blocks.len(),
                block: None,
            });
            session.connect(cancel)?;
            for (index, &block) in blocks.iter().enumerate() {
                let payload = session.read_block(block, cancel)?;
                if pass == 1 {
                    reference.push(payload);
                } else if payload != reference[index] {
                    let byte_offset = payload
                        .iter()
                        .zip(&reference[index])
                        .position(|(left, right)| left != right)
                        .expect("unequal fixed-size blocks");
                    return Err(ReadError::PassMismatch { block, byte_offset });
                }
                completed += 1;
                on_progress(Progress {
                    pass,
                    phase: Phase::Reading,
                    completed_blocks: completed,
                    total_blocks: blocks.len(),
                    block: Some(block),
                });
            }
            Ok(())
        })();
        on_progress(Progress {
            pass,
            phase: Phase::Closing,
            completed_blocks: completed,
            total_blocks: blocks.len(),
            block: None,
        });
        let cleanup = session.finish();
        session.capture.complete = result.is_ok() && cleanup.is_ok();
        passes.push(session.capture);
        if let Err(error) = result.and(cleanup) {
            return Err(ReadFailure { error, passes });
        }
        if pass == 1 {
            if let Err(error) = pause(timing.between_passes, cancel) {
                return Err(ReadFailure { error, passes });
            }
        }
    }
    if let Err(error) = cancel.check() {
        return Err(ReadFailure { error, passes });
    }
    let mut at = 0;
    let sections = plan
        .regions()
        .iter()
        .map(|&region| {
            let end = at + region.block_count();
            let bytes = reference[at..end].concat();
            at = end;
            SectionData {
                region,
                first_block: region.first_block(),
                bytes,
            }
        })
        .collect();
    on_progress(Progress {
        pass: 2,
        phase: Phase::Complete,
        completed_blocks: blocks.len(),
        total_blocks: blocks.len(),
        block: None,
    });
    Ok(ReadSnapshot {
        plan,
        sections,
        passes,
    })
}

/// Enumerate DOS COM names without opening any port or probing a radio.
pub fn list_ports() -> io::Result<Vec<String>> {
    #[cfg(windows)]
    {
        windows::list_ports()
    }
    #[cfg(not(windows))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows COM enumeration is unavailable on this platform",
        ))
    }
}

pub fn read_windows<P: FnMut(Progress)>(
    port: &str,
    plan: ReadPlan,
    cancel: &CancelToken,
    on_progress: P,
) -> Result<ReadSnapshot, ReadFailure> {
    #[cfg(windows)]
    {
        read_twice(
            || windows::WindowsPort::open(port),
            plan,
            cancel,
            on_progress,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (port, plan, cancel, on_progress);
        Err(ReadFailure {
            error: ReadError::Io {
                operation: "open port",
                message: "Windows serial transport is unavailable on this platform".into(),
            },
            passes: Vec::new(),
        })
    }
}

fn port_number(name: &str) -> Option<u16> {
    let suffix = name.get(3..)?;
    if !name.get(..3)?.eq_ignore_ascii_case("COM")
        || suffix.is_empty()
        || suffix.starts_with('0')
        || !suffix.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    suffix.parse::<u16>().ok().filter(|number| *number != 0)
}
