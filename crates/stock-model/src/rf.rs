//! Pure register-code models from the related RT880 1.16 reference image.
//!
//! Address suffixes identify the exact machine-code path. These functions do
//! not access RF hardware and do not infer measured frequency, power or gain.
//! MIC/volume helpers are not proof that the corresponding *band calibration*
//! fields are consumed: reviewed callers instead use separate configuration.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterCode {
    pub address: u8,
    pub value: u16,
}

/// Ordinary path at 0x08005552, with override flag RAM[0x200011FA] == 0.
/// `mode` is channel byte +0x10. Its encoding is not shared with 0x080040DC.
pub const fn power_code_at_05552(mode: u8, high: u8, low: u8) -> u8 {
    match mode {
        2 => high,
        1 => ((high as u16 + low as u16) / 2) as u8,
        _ => low,
    }
}

/// Ordinary other-record path at 0x080040DC, override flag == 0.
/// Unlike 0x08005552, raw mode zero selects the average here.
pub const fn power_code_at_040dc(mode: u8, high: u8, low: u8) -> u8 {
    match mode {
        2 => high,
        0 => ((high as u16 + low as u16) / 2) as u8,
        _ => low,
    }
}

/// Word sent to register 0x36 by 0x08004F0C (mirror: 0x08003E08).
/// The code is truncated to six bits, not clamped. Enable tests the full input.
pub const fn power_register_04f0c(code: u32) -> u16 {
    (((code & 0x3F) << 10) | 0x7F | if code == 0 { 0 } else { 0x80 }) as u16
}

/// Raw channel +3 zero selects band+3, nonzero selects band+10.
/// Channel +0x13 low nibble enables subtraction of 200 only when raw > 200.
/// This is the full stored 16-bit word, including its upper four bits.
pub const fn deviation_at_054cc(
    channel_byte3: u8,
    channel_byte13: u8,
    wide: u16,
    narrow: u16,
) -> u16 {
    let raw = deviation_at_040a8(channel_byte3, wide, narrow);
    if channel_byte13 & 0x0F != 0 && raw > 200 {
        raw - 200
    } else {
        raw
    }
}

/// Word sent unchanged to register 0x40 by 0x080040A8..0x080040CA.
/// This path does not apply the conditional subtraction at 0x080054CC.
pub const fn deviation_at_040a8(channel_byte3: u8, wide: u16, narrow: u16) -> u16 {
    if channel_byte3 == 0 {
        wide
    } else {
        narrow
    }
}

/// Register 0x7D helper 0x08004E90, mirror 0x08003DE8.
/// Truncation is modulo 65536. Caller 0x080054C2 supplies configuration +0xAD,
/// not the selected band's MIC fields.
pub const fn mic_register_04e90(input: u32) -> u16 {
    0xE940u32.wrapping_add(input) as u16
}

/// Register 0x48 helper 0x08005768. `reg1` is the full value returned by
/// reading RF register 1. Only values 2, 3, 6, 7 select the correction branch.
/// Do not replace this with a low-bit mask of `reg1`.
pub const fn volume_register_05768(input: u16, reg1: u16) -> u16 {
    if matches!(reg1, 2 | 3 | 6 | 7) && input & 0xF > 3 {
        input - 3
    } else {
        input
    }
}

/// Fixed compander-reset sequence at 0x08004518, mirror 0x08003C20.
/// These are constants in the reviewed function, not reads of calibration +4/+6.
pub const fn compander_reset_04518(reg31: u16) -> [RegisterCode; 3] {
    [
        RegisterCode {
            address: 0x31,
            value: reg31 & !8,
        },
        RegisterCode {
            address: 0x28,
            value: 0x6B38,
        },
        RegisterCode {
            address: 0x29,
            value: 0xAB40,
        },
    ]
}

/// Names follow tuning-tool fields; mapping is confirmed by the initialization
/// caller at 0x08004E32, not inferred from hardware bandwidth terminology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterField {
    Tx300,
    Tx3k,
    Rx300,
    Rx3k,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilterWords {
    pub first: RegisterCode,
    pub second: Option<RegisterCode>,
}

/// Filter coefficient switch at 0x080038D0, mirror 0x08003A2C.
/// Out-of-switch indices deliberately retain the machine code's initial words.
/// No mask is applied to `index`; this accepts the original raw byte.
pub const fn filter_registers(field: FilterField, index: u8) -> FilterWords {
    let tx = matches!(field, FilterField::Tx300 | FilterField::Tx3k);
    let is_3k = matches!(field, FilterField::Tx3k | FilterField::Rx3k);
    if is_3k {
        let value = match index {
            0 => 0xE61C,
            1 => 0xDF22,
            2 => 0xD42D,
            3 => 0xCC35,
            4 => 0xFA02,
            _ => 0xF50B,
        };
        FilterWords {
            first: RegisterCode {
                address: if tx { 0x74 } else { 0x75 },
                value,
            },
            second: None,
        }
    } else {
        let (first, second) = match index {
            0 => (0x8BD7, 0x3507),
            1 => (0x8AED, 0x35E1),
            2 => (0x8A96, 0x3606),
            3 => (0x8908, 0x3779),
            4 => (0x8DDE, 0x335A),
            5 => (0x8EB3, 0x32BA),
            6 => (0x8F88, 0x3201),
            _ => (0x8BE3, 0x352D),
        };
        FilterWords {
            first: RegisterCode {
                address: if tx { 0x44 } else { 0x54 },
                value: first,
            },
            second: Some(RegisterCode {
                address: if tx { 0x45 } else { 0x55 },
                value: second,
            }),
        }
    }
}
