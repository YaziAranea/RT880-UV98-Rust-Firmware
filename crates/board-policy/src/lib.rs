#![no_std]
#![forbid(unsafe_code)]
//! Pure requests derived from bounded related-RT880 stock paths.
//!
//! No GPIO/RF I/O, clock, reset, delay, TX, polling or executor is provided.
//! Applying these requests to a software model proves no physical RF state.
//! Exact UV98 wiring, register readback, GPIO direction/polarity, supply state
//! and loader handoff remain separate requirements. The full stock RF init is
//! deliberately not modelled as an off sequence: it enables internal blocks.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Chip {
    /// Schematic U201: stock write 0x0801E660, CS PA8, SDA PC9, SCK PF6.
    A,
    /// Schematic U400: stock write 0x0801E6D4, CS PC7, SDA PC6, SCK PC8.
    B,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McuLine {
    /// PC15, related schematic HF-TXPOWER-EN. Stock low at 0x08005320.
    Pc15HfTxPower,
    /// PB11/pin30, related TX29/50M-SW selector. AF MUTE is PF8/pin31.
    Pb11Tx29_50mSwitch,
    /// PC4, related schematic RXTS-SW; its physical function is not inferred.
    Pc4RxTsSwitch,
    /// PC5, related schematic FM-POWER-EN. Not a BK4819 TX gate.
    Pc5FmPower,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Change {
    Write(u16),
    /// Read-modify-write preserving every bit in `keep_mask`.
    /// A future executor needs a fresh read of the addressed chip/register;
    /// a stale software cache is not an acceptable substitute.
    Keep(u16),
}

impl Change {
    pub const fn apply(self, previous: u16) -> u16 {
        match self {
            Self::Write(value) => value,
            Self::Keep(mask) => previous & mask,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    ClearMcu(McuLine),
    Register {
        chip: Chip,
        address: u8,
        change: Change,
    },
}

const fn write(chip: Chip, address: u8, value: u16) -> Operation {
    Operation::Register {
        chip,
        address,
        change: Change::Write(value),
    }
}

const fn keep(chip: Chip, address: u8, mask: u16) -> Operation {
    Operation::Register {
        chip,
        address,
        change: Change::Keep(mask),
    }
}

/// Observed boot levels from 0x08011B74/78/7C. These three pins are not a
/// complete radio-off mechanism. GPIO direction and electrical setup omitted.
pub const MCU_BOOT_LEVELS: [Operation; 3] = [
    Operation::ClearMcu(McuLine::Pc15HfTxPower),
    Operation::ClearMcu(McuLine::Pc4RxTsSwitch),
    Operation::ClearMcu(McuLine::Pc5FmPower),
];

/// Exact I/O subsequence of 0x08005318, in order. Clears A GPIO output bits3/2
/// (related VHF/UHF TX gates) while preserving the rest of register33.
pub const TX_GATES_OFF_A: [Operation; 3] = [
    Operation::ClearMcu(McuLine::Pc15HfTxPower),
    Operation::ClearMcu(McuLine::Pb11Tx29_50mSwitch),
    keep(Chip::A, 0x33, 0xfff3),
];

/// Exact register I/O of 0x080052B0 (including callee0x08004FC8).
/// IMPORTANT: maskFFCC preserves TX bits3/2. This alone is not TX-gates-off.
/// The routine's software flag write to RAM0x20001124 is deliberately omitted.
pub const SLEEP_A: [Operation; 3] = [
    keep(Chip::A, 0x33, 0xffcc),
    write(Chip::A, 0x30, 0),
    write(Chip::A, 0x37, 0x1d00),
];

/// Exact register I/O of secondary sleep helper0x08003EC0.
pub const SLEEP_B: [Operation; 2] = [write(Chip::B, 0x30, 0), write(Chip::B, 0x37, 0x1d00)];

/// Exact register I/O of 0x08003F08. GPIO2/A-B switch is among cleared bits;
/// the physical switched path is not inferred from an unverified PCB revision.
pub const CLEAR_B_GPIO: [Operation; 1] = [keep(Chip::B, 0x33, 0xffe0)];

/// Code emitted by primary0x08004F0C / secondary0x08003E08 with argument0.
/// This is a stored control word, not a claim of zero measured output power.
pub const fn zero_pa_code(chip: Chip) -> Operation {
    write(chip, 0x36, 0x007f)
}

/// New diagnostic composition of individually observed stock operations.
/// This complete array is NOT claimed to occur verbatim in stock firmware.
/// External TX-gate requests precede PA-code/RX-filter/internal-block requests.
/// No RF reset or receive/transmit enabling operation is included.
///
/// Requests require already configured, exclusively owned MCU/RF buses and
/// fresh read-modify-write values. No `known_off` token/result is returned:
/// neither this plan nor a successful bus call proves physical RF inactivity.
pub const DIAGNOSTIC_QUIESCENCE_REQUESTS: [Operation; 12] = [
    TX_GATES_OFF_A[0],
    TX_GATES_OFF_A[1],
    TX_GATES_OFF_A[2],
    zero_pa_code(Chip::A),
    SLEEP_A[0],
    SLEEP_A[1],
    SLEEP_A[2],
    CLEAR_B_GPIO[0],
    zero_pa_code(Chip::B),
    SLEEP_B[0],
    SLEEP_B[1],
    Operation::ClearMcu(McuLine::Pc5FmPower),
];
