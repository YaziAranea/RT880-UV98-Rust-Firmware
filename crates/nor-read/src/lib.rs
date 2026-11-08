#![no_std]
#![forbid(unsafe_code)]
//! Ordinary SPI NOR READ (03h), bounded to the related radio's 4 MiB profile.
//!
//! No allocation, PAC/GPIO implementation, timings, device discovery, write-enable,
//! erase, program, retry, or Stage 0 runtime hookup is provided. A caller supplies
//! an exclusively owned, configured bus and must establish the actual chip's
//! compatible protocol and capacity separately. This profile does not identify
//! the user's chip or back up MCU flash, loader, OTP or security registers.
//!
//! The reviewed RT880 helper at 0x08025D30 sends 03h and a big-endian 24-bit
//! address. The 4 MiB host bound comes from the related W25Q32 schematic and
//! the independently captured main-array range, not a capacity check in stock.

use uv98_calibration::{Calibration, BLOCK_LEN};

pub const CAPACITY_BYTES: u32 = 4 * 1024 * 1024;
pub const READ_OPCODE: u8 = 0x03;
pub const CALIBRATION_ADDRESS: u32 = 0;

/// Blocking transaction primitives. The implementation owns physical SPI/CS.
///
/// Operations must return errors rather than silently accepting short transfers;
/// `read` must fill the complete slice on success. Backend timeouts, bus mode,
/// clock speed and CS timing are its responsibility. Receive dummy values are
/// unspecified here. No operation may issue extra NOR command transactions.
///
/// `select` asserts this chip's CS; `deselect` deasserts it. Even if `select`
/// fails, attempting `deselect` must be permitted for best-effort cleanup.
pub trait Bus {
    type Error;
    fn select(&mut self) -> Result<(), Self::Error>;
    fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
    fn read(&mut self, destination: &mut [u8]) -> Result<(), Self::Error>;
    fn deselect(&mut self) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Select,
    Command,
    Data,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadError<E> {
    OutOfBounds {
        address: u32,
        length: usize,
    },
    Bus {
        phase: Phase,
        source: E,
        /// The original failure remains available even when cleanup also fails.
        deselect_error: Option<E>,
    },
    Deselect(E),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CalibrationReadError<E> {
    Read(ReadError<E>),
    Layout(uv98_calibration::Error),
}

/// Check a half-open range without overflowing on hostile address/length inputs.
pub const fn range_is_valid(address: u32, length: usize) -> bool {
    address <= CAPACITY_BYTES && length <= (CAPACITY_BYTES - address) as usize
}

/// Read one contiguous main-array range. The four-byte command is 03h,A23..A0.
/// No dummy address-cycle byte is added for this ordinary READ command.
///
/// Bounds are checked before any bus call, without address/length addition.
/// An empty read at any address through CAPACITY_BYTES is a no-op; addresses
/// beyond that limit are invalid even with an empty destination.
///
/// Every attempted select has exactly one attempted deselect on normal return,
/// including select/write/read failures. If deselect fails, CS state is unknown:
/// the caller must recover the backend before another transaction. No cleanup
/// guarantee is made for backend panic, abort or power loss.
///
/// On a data error the caller's buffer may be partly changed; on a deselect error
/// it may be completely filled. Any Err invalidates the entire requested result.
/// This allocation-free function does not promise atomic updates of the buffer.
pub fn read<B: Bus>(
    bus: &mut B,
    address: u32,
    destination: &mut [u8],
) -> Result<(), ReadError<B::Error>> {
    if !range_is_valid(address, destination.len()) {
        return Err(ReadError::OutOfBounds {
            address,
            length: destination.len(),
        });
    }
    if destination.is_empty() {
        return Ok(());
    }

    if let Err(source) = bus.select() {
        return Err(ReadError::Bus {
            phase: Phase::Select,
            source,
            deselect_error: bus.deselect().err(),
        });
    }
    let command = [
        READ_OPCODE,
        (address >> 16) as u8,
        (address >> 8) as u8,
        address as u8,
    ];
    if let Err(source) = bus.write(&command) {
        return Err(ReadError::Bus {
            phase: Phase::Command,
            source,
            deselect_error: bus.deselect().err(),
        });
    }
    if let Err(source) = bus.read(destination) {
        return Err(ReadError::Bus {
            phase: Phase::Data,
            source,
            deselect_error: bus.deselect().err(),
        });
    }
    bus.deselect().map_err(ReadError::Deselect)
}

/// Read the observed 4096-byte calibration region into caller-owned storage.
/// A borrowed view is returned only after complete reading, successful deselect
/// and parser validation. Errors never return a usable calibration view.
///
/// The parser checks length/marker only; it is not a checksum or RF validation.
/// Preserve the independently acquired backup. The supplied buffer may change
/// on error as documented by `read`; no defaults or previous values are inserted.
pub fn read_calibration<'a, B: Bus>(
    bus: &mut B,
    buffer: &'a mut [u8; BLOCK_LEN],
) -> Result<Calibration<'a>, CalibrationReadError<B::Error>> {
    read(bus, CALIBRATION_ADDRESS, buffer).map_err(CalibrationReadError::Read)?;
    Calibration::parse(buffer).map_err(CalibrationReadError::Layout)
}
