use crate::{Error, Wait};
use core::num::NonZeroU32;
use uv98_nor_read::{Bus, CAPACITY_BYTES, READ_OPCODE};

pub(crate) const READ_DUMMY: u8 = 0xa5;
const RDBF: u32 = 1;
const TDBE: u32 = 2;
const BUSY: u32 = 0x80;
const FAULTS: u32 = 0x78;

/// Implementations must perform complete volatile operations without panicking.
pub(crate) trait Registers {
    fn status(&mut self) -> u32;
    fn transmit(&mut self, byte: u8);
    fn receive(&mut self) -> u8;
    fn chip_select(&mut self, active: bool);
    fn disable(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Idle,
    Selected,
    Reading { remaining: u32 },
}

pub(crate) struct Engine<R: Registers> {
    pub(crate) registers: R,
    budget: NonZeroU32,
    state: State,
    poisoned: bool,
}

impl<R: Registers> Engine<R> {
    pub(crate) fn new(registers: R, budget: NonZeroU32) -> Self {
        Self {
            registers,
            budget,
            state: State::Idle,
            poisoned: false,
        }
    }

    fn wait(&mut self, kind: Wait) -> Result<(), Error> {
        for _ in 0..self.budget.get() {
            let status = self.registers.status();
            if status & FAULTS != 0 {
                self.poisoned = true;
                return Err(Error::Status(status));
            }
            let ready = match kind {
                Wait::TransmitEmpty => status & TDBE != 0,
                Wait::ReceiveFull => status & RDBF != 0,
                Wait::NotBusy => status & BUSY == 0,
            };
            if ready {
                return Ok(());
            }
        }
        self.poisoned = true;
        Err(Error::Timeout(kind))
    }

    fn exchange(&mut self, byte: u8) -> Result<u8, Error> {
        self.wait(Wait::TransmitEmpty)?;
        self.registers.transmit(byte);
        self.wait(Wait::ReceiveFull)?;
        Ok(self.registers.receive())
    }

    fn healthy(&self) -> Result<(), Error> {
        if self.poisoned {
            Err(Error::Poisoned)
        } else {
            Ok(())
        }
    }
}

impl<R: Registers> Bus for Engine<R> {
    type Error = Error;

    fn select(&mut self) -> Result<(), Error> {
        self.healthy()?;
        if self.state != State::Idle {
            return Err(Error::InvalidState);
        }
        self.wait(Wait::NotBusy)?;
        self.registers.chip_select(true);
        self.state = State::Selected;
        Ok(())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.healthy()?;
        if self.state != State::Selected {
            return Err(Error::InvalidState);
        }
        // Validate the complete transaction header before transmitting anything.
        if bytes.len() != 4 || bytes[0] != READ_OPCODE {
            return Err(Error::InvalidReadHeader);
        }
        let address = ((bytes[1] as u32) << 16) | ((bytes[2] as u32) << 8) | bytes[3] as u32;
        if address >= CAPACITY_BYTES {
            return Err(Error::OutOfBounds);
        }
        for &byte in bytes {
            let _ = self.exchange(byte)?;
            // Stock's single-byte command/address helper waits for BUSY to
            // clear after reading DT. Retain that boundary for all four bytes.
            self.wait(Wait::NotBusy)?;
        }
        self.state = State::Reading {
            remaining: CAPACITY_BYTES - address,
        };
        Ok(())
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<(), Error> {
        self.healthy()?;
        let State::Reading { remaining } = self.state else {
            return Err(Error::InvalidState);
        };
        if destination.len() > remaining as usize {
            return Err(Error::OutOfBounds);
        }
        for byte in destination.iter_mut() {
            *byte = self.exchange(READ_DUMMY)?;
        }
        self.state = State::Reading {
            remaining: remaining - destination.len() as u32,
        };
        Ok(())
    }

    fn deselect(&mut self) -> Result<(), Error> {
        // Even after timeout, try to finish the current byte within a fresh
        // finite wait. Always release CS, including failure and repeated calls.
        let result = self.wait(Wait::NotBusy);
        self.registers.chip_select(false);
        self.state = State::Idle;
        if self.poisoned {
            self.registers.disable();
        }
        result
    }
}

impl<R: Registers> Drop for Engine<R> {
    fn drop(&mut self) {
        // Drop is abortive cleanup, not a successful transfer boundary. A READ
        // may be truncated, but cannot become a program/erase transaction.
        self.registers.chip_select(false);
        self.registers.disable();
    }
}
