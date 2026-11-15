#![no_std]
#![forbid(unsafe_code)]
//! Disjoint ownership for the related RT880 pin profile. This crate performs
//! no register access. The profile is not physical identification of a UV98.
//!
//! Pin capabilities cannot be copied, constructed externally, sent to another
//! thread or shared between threads. Backend configuration of shared GPIO
//! registers must remain serialized on the same foreground execution context.
//! IRQ/global/raw-pointer access still requires an explicit unsafe contract.

use core::marker::PhantomData;

pub const NOR_A_MASK: u16 = 0;
pub const NOR_B_MASK: u16 = 0xf000;
pub const NOR_C_MASK: u16 = 0;
pub const LCD_A_MASK: u16 = 1 << 15;
pub const LCD_B_MASK: u16 = (1 << 3) | (1 << 5);
pub const LCD_C_MASK: u16 = (1 << 13) | (1 << 14);
pub const KEYSCAN_A_MASK: u16 = 0;
pub const KEYSCAN_B_MASK: u16 = 0x0447;
pub const KEYSCAN_C_MASK: u16 = 0x1c00;
pub const KEYSCAN_D_MASK: u16 = 0x0004;
pub const RF_PRIMARY_A_MASK: u16 = 1 << 8;
pub const RF_PRIMARY_C_MASK: u16 = 1 << 9;
pub const RF_PRIMARY_F_MASK: u16 = 1 << 6;
pub const RF_SECONDARY_C_MASK: u16 = (1 << 6) | (1 << 7) | (1 << 8);
pub const RF_CONTROL_F_MASK: u16 = (1 << 0) | (1 << 8);
pub const DIAGNOSTIC_CONTROL_A_MASK: u16 = 1 << 4;
pub const DIAGNOSTIC_CONTROL_B_MASK: u16 = 1 << 11;
pub const DIAGNOSTIC_CONTROL_C_MASK: u16 = (1 << 4) | (1 << 5) | (1 << 15);
pub const DIAGNOSTIC_CONTROL_F_MASK: u16 = 1 << 8;
pub const BOOT_LOG_A_MASK: u16 = 1 << 11;
pub const SIDE_ADC_C_MASK: u16 = 1 << 2;
pub const BATTERY_ADC_A_MASK: u16 = 1 << 7;
/// Operational LCD/keypad lights and green/red status LEDs. The latter two
/// consume PA13/PA14, so the operational backend cannot coexist with SWD.
pub const RADIO_CONTROL_A_MASK: u16 = (1 << 4) | (1 << 9) | (1 << 13) | (1 << 14);
/// Diagnostic C outputs plus PC3: LOW selects the ordinary MICI input of U501.
/// PC3 is transferred from ReservedAfterBootLog, never diagnostic early ownership.
pub const RADIO_CONTROL_C_MASK: u16 = DIAGNOSTIC_CONTROL_C_MASK | (1 << 3);

/// PB12..PB15. Only the owning backend may access these pin fields/latches.
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<uv98_board_pins::NorPins>();
/// ```
pub struct NorPins {
    _single_context: PhantomData<*mut ()>,
}
/// PA15 CS, PB3 SCK, PB5 MOSI, PC13 DC, PC14 RESET. Backlight is excluded.
/// ```compile_fail
/// fn require_sync<T: Sync>() {}
/// require_sync::<uv98_board_pins::LcdPins>();
/// ```
pub struct LcdPins {
    _single_context: PhantomData<*mut ()>,
}
/// PB10/PB2/PB1/PB0 columns, PC10/PC11/PC12/PD2 rows, PB6 PTT.
/// Auxiliary ADC and encoder pins are not part of this capability.
pub struct KeyscanPins {
    _single_context: PhantomData<*mut ()>,
}
/// Remaining A/B/C/D pins are intentionally inaccessible until another reviewed
/// backend defines their ownership. This includes RF control and backlight.
pub struct ReservedPins {
    _single_context: PhantomData<*mut ()>,
}

/// RF A: PA8 CS, PC9 DATA, PF6 SCK. Not constructible/copyable externally.
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<uv98_board_pins::RfPrimaryPins>();
/// ```
pub struct RfPrimaryPins {
    _single_context: PhantomData<*mut ()>,
}
/// RF B: PC7 CS, PC6 DATA, PC8 SCK.
pub struct RfSecondaryPins {
    _single_context: PhantomData<*mut ()>,
}
/// PF0 antenna relay request and PF8 AF MUTE line. No TX gates included.
/// ```compile_fail
/// fn require_sync<T: Sync>() {}
/// require_sync::<uv98_board_pins::RfControlPins>();
/// ```
pub struct RfControlPins {
    _single_context: PhantomData<*mut ()>,
}
/// Remaining A/B/C/D/F pins after RF allocation. Deliberately has no with_rf
/// method: RF capabilities cannot be allocated a second time from this owner.
pub struct ReservedAfterRf {
    _single_context: PhantomData<*mut ()>,
}
pub struct RfPins {
    pub primary: RfPrimaryPins,
    pub secondary: RfSecondaryPins,
    pub control: RfControlPins,
    pub reserved: ReservedAfterRf,
}

/// Diagnostic-only output requests: PA4 backlight; PB11 TX29/50M-SW;
/// PC4 RXTS-SW, PC5 FM-POWER-EN, PC15 HF-TXPOWER-EN; PF8 AF MUTE.
/// The owning backend must never use this capability to enable an RF path.
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<uv98_board_pins::DiagnosticControlPins>();
/// ```
pub struct DiagnosticControlPins {
    _single_context: PhantomData<*mut ()>,
}
/// All remaining pins, including PF0 and the cable's PA11 TX line.
pub struct ReservedAfterDiagnostic {
    _single_context: PhantomData<*mut ()>,
}
/// PA11 USART6 TX only. PA12 RX remains reserved; no command receiver exists.
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<uv98_board_pins::BootLogPins>();
/// ```
pub struct BootLogPins {
    _single_context: PhantomData<*mut ()>,
}
pub struct ReservedAfterBootLog {
    _single_context: PhantomData<*mut ()>,
}
/// The already allocated diagnostic outputs plus PF0 antenna relay, PA9 keypad
/// backlight, PA13 green RX LED, PA14 red TX LED and PC3 microphone mux select.
/// Lights/indicators are active HIGH. PC3 stays LOW for the ordinary MICI path;
/// HIGH would select the external BT-MICI/digital path, which is not enabled.
/// Consumes both capabilities, so diagnostic/operational drivers cannot coexist.
/// PA13/PA14 no longer belong to SWD once the operational backend is prepared.
pub struct RadioControlPins {
    _single_context: PhantomData<*mut ()>,
}
/// PC2 / ADC1_IN12. No keyboard matrix or PTT PB6 alias is granted.
pub struct SideAdcPins {
    _single_context: PhantomData<*mut ()>,
}
/// PA7 / ADC1_IN7, LOWBATTY-DET. Shares ADC1 with the side-key input.
pub struct BatteryAdcPins {
    _single_context: PhantomData<*mut ()>,
}
pub struct ReservedAfterRadio {
    _single_context: PhantomData<*mut ()>,
}
/// PC0/PC1, I2C1 AF7. PC5 tuner power remains with RadioControlPins.
pub struct ShortwavePins {
    _single_context: PhantomData<*mut ()>,
}
pub struct RemainingPins {
    _single_context: PhantomData<*mut ()>,
}
/// GPS USART2 RX PA3 and its stock enable PB9. Does not own UART TX.
pub struct GpsPins {
    _single_context: PhantomData<*mut ()>,
}
/// Stock modem C-BUS PA0/PA1/PA2/PF1, selector PB4 and TX audio gate PB7.
/// PC3 audio routing remains owned by RadioControlPins.
pub struct ModemPins {
    _single_context: PhantomData<*mut ()>,
}
pub struct ReservedAfterAuxiliary {
    _single_context: PhantomData<*mut ()>,
}
impl RemainingPins {
    pub fn with_auxiliary(self) -> (GpsPins, ModemPins, ReservedAfterAuxiliary) {
        (
            GpsPins {
                _single_context: PhantomData,
            },
            ModemPins {
                _single_context: PhantomData,
            },
            ReservedAfterAuxiliary {
                _single_context: PhantomData,
            },
        )
    }
}
impl ReservedAfterRadio {
    pub fn with_shortwave(self) -> (ShortwavePins, RemainingPins) {
        (
            ShortwavePins {
                _single_context: PhantomData,
            },
            RemainingPins {
                _single_context: PhantomData,
            },
        )
    }
}

pub struct RadioPins {
    pub control: RadioControlPins,
    pub side_adc: SideAdcPins,
    pub battery_adc: BatteryAdcPins,
    pub reserved: ReservedAfterRadio,
}
impl ReservedAfterBootLog {
    pub fn with_radio(self, _diagnostic: DiagnosticControlPins) -> RadioPins {
        RadioPins {
            control: RadioControlPins {
                _single_context: PhantomData,
            },
            side_adc: SideAdcPins {
                _single_context: PhantomData,
            },
            battery_adc: BatteryAdcPins {
                _single_context: PhantomData,
            },
            reserved: ReservedAfterRadio {
                _single_context: PhantomData,
            },
        }
    }
}
pub struct BootLogParts {
    pub tx: BootLogPins,
    pub reserved: ReservedAfterBootLog,
}
impl ReservedAfterDiagnostic {
    /// Pure ownership split. Does not configure the UART or any pin.
    pub fn with_boot_log(self) -> BootLogParts {
        BootLogParts {
            tx: BootLogPins {
                _single_context: PhantomData,
            },
            reserved: ReservedAfterBootLog {
                _single_context: PhantomData,
            },
        }
    }
}
pub struct DiagnosticPins {
    pub control: DiagnosticControlPins,
    pub reserved: ReservedAfterDiagnostic,
}
impl ReservedAfterRf {
    /// Consume PF0/PF8 ownership and the still-reserved pins together.
    /// PF0 remains unused/reserved: diagnostic startup does not move the relay.
    /// Pure ownership split, without MMIO or an assertion of physical RF-off.
    pub fn with_diagnostic(self, _rf_control: RfControlPins) -> DiagnosticPins {
        DiagnosticPins {
            control: DiagnosticControlPins {
                _single_context: PhantomData,
            },
            reserved: ReservedAfterDiagnostic {
                _single_context: PhantomData,
            },
        }
    }
}

#[cfg(feature = "hardware")]
impl ReservedPins {
    /// Consume the remaining A/B/C/D capability AND the whole GPIOF owner.
    /// Pure ownership split: no clock, pin level, GPIO mode or RF operation.
    /// The existing BoardPins::split(A,B,C,D) API remains unchanged.
    pub fn with_rf(self, _f: at32f4xx_pac::at32f423::GPIOF) -> RfPins {
        RfPins {
            primary: RfPrimaryPins {
                _single_context: PhantomData,
            },
            secondary: RfSecondaryPins {
                _single_context: PhantomData,
            },
            control: RfControlPins {
                _single_context: PhantomData,
            },
            reserved: ReservedAfterRf {
                _single_context: PhantomData,
            },
        }
    }
}

pub struct BoardPins {
    pub nor: NorPins,
    pub lcd: LcdPins,
    pub keyscan: KeyscanPins,
    pub reserved: ReservedPins,
}

#[cfg(feature = "hardware")]
impl BoardPins {
    /// Consume whole PAC GPIO owners and divide ownership without MMIO. This
    /// neither changes modes nor assumes the inherited output states are safe.
    pub fn split(
        _a: at32f4xx_pac::at32f423::GPIOA,
        _b: at32f4xx_pac::at32f423::GPIOB,
        _c: at32f4xx_pac::at32f423::GPIOC,
        _d: at32f4xx_pac::at32f423::GPIOD,
    ) -> Self {
        Self {
            nor: NorPins {
                _single_context: PhantomData,
            },
            lcd: LcdPins {
                _single_context: PhantomData,
            },
            keyscan: KeyscanPins {
                _single_context: PhantomData,
            },
            reserved: ReservedPins {
                _single_context: PhantomData,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radio_indicator_ownership_excludes_nor_lcd_keys_rf_and_uart() {
        assert_eq!(RADIO_CONTROL_A_MASK, 0x6210);
        assert_eq!(
            RADIO_CONTROL_A_MASK
                & (NOR_A_MASK | LCD_A_MASK | KEYSCAN_A_MASK | RF_PRIMARY_A_MASK | BOOT_LOG_A_MASK),
            0
        );
        assert_eq!(RADIO_CONTROL_A_MASK & DIAGNOSTIC_CONTROL_A_MASK, 1 << 4);
        assert_eq!(RADIO_CONTROL_C_MASK, 0x8038);
        assert_eq!(RADIO_CONTROL_C_MASK & DIAGNOSTIC_CONTROL_C_MASK, 0x8030);
        assert_eq!(DIAGNOSTIC_CONTROL_C_MASK & (1 << 3), 0);
        assert_eq!(
            RADIO_CONTROL_C_MASK
                & (NOR_C_MASK
                    | LCD_C_MASK
                    | KEYSCAN_C_MASK
                    | RF_PRIMARY_C_MASK
                    | RF_SECONDARY_C_MASK
                    | SIDE_ADC_C_MASK),
            0
        );
    }
    #[test]
    fn battery_input_does_not_alias_existing_gpioa_owners() {
        let gps_and_modem = 0x000f;
        assert_eq!(BATTERY_ADC_A_MASK, 0x0080);
        assert_eq!(
            BATTERY_ADC_A_MASK
                & (NOR_A_MASK
                    | LCD_A_MASK
                    | KEYSCAN_A_MASK
                    | RF_PRIMARY_A_MASK
                    | RADIO_CONTROL_A_MASK
                    | BOOT_LOG_A_MASK
                    | gps_and_modem),
            0
        );
    }
    #[test]
    fn nor_and_lcd_fields_are_disjoint_and_rf_pins_are_reserved() {
        assert_eq!(NOR_A_MASK & LCD_A_MASK, 0);
        assert_eq!(NOR_B_MASK & LCD_B_MASK, 0);
        assert_eq!(NOR_C_MASK & LCD_C_MASK, 0);
        assert_eq!((NOR_A_MASK | LCD_A_MASK) & KEYSCAN_A_MASK, 0);
        assert_eq!((NOR_B_MASK | LCD_B_MASK) & KEYSCAN_B_MASK, 0);
        assert_eq!((NOR_C_MASK | LCD_C_MASK) & KEYSCAN_C_MASK, 0);
        assert_eq!(KEYSCAN_D_MASK, 1 << 2);
        assert_eq!(KEYSCAN_B_MASK & (1 << 11), 0);
        assert_eq!(
            (NOR_C_MASK | LCD_C_MASK) & ((1 << 15) | (1 << 4) | (1 << 5)),
            0
        );
        assert_eq!((NOR_A_MASK | LCD_A_MASK) & (1 << 4), 0);
    }

    #[test]
    fn rf_ownership_is_disjoint_from_existing_functions_and_tx_gates() {
        assert_eq!(
            (NOR_A_MASK | LCD_A_MASK | KEYSCAN_A_MASK) & RF_PRIMARY_A_MASK,
            0
        );
        assert_eq!(
            (NOR_C_MASK | LCD_C_MASK | KEYSCAN_C_MASK) & (RF_PRIMARY_C_MASK | RF_SECONDARY_C_MASK),
            0
        );
        assert_eq!(RF_PRIMARY_C_MASK & RF_SECONDARY_C_MASK, 0);
        assert_eq!(RF_PRIMARY_F_MASK & RF_CONTROL_F_MASK, 0);
        assert_eq!(RF_PRIMARY_A_MASK, 0x0100);
        assert_eq!(RF_PRIMARY_C_MASK, 0x0200);
        assert_eq!(RF_SECONDARY_C_MASK, 0x01c0);
        assert_eq!(RF_PRIMARY_F_MASK | RF_CONTROL_F_MASK, 0x0141);
        assert_eq!(
            (RF_PRIMARY_C_MASK | RF_SECONDARY_C_MASK) & ((1 << 15) | (1 << 4) | (1 << 5)),
            0
        );
        // RF capabilities include no GPIOB field at all: PB11 is untouched.
    }

    #[test]
    fn diagnostic_controls_do_not_alias_buses_keys_or_relay() {
        assert_eq!(
            BOOT_LOG_A_MASK
                & (NOR_A_MASK
                    | LCD_A_MASK
                    | KEYSCAN_A_MASK
                    | RF_PRIMARY_A_MASK
                    | DIAGNOSTIC_CONTROL_A_MASK),
            0
        );
        assert_eq!(BOOT_LOG_A_MASK & (1 << 12), 0);
        assert_eq!(
            DIAGNOSTIC_CONTROL_A_MASK
                & (NOR_A_MASK | LCD_A_MASK | KEYSCAN_A_MASK | RF_PRIMARY_A_MASK),
            0
        );
        assert_eq!(
            DIAGNOSTIC_CONTROL_B_MASK & (NOR_B_MASK | LCD_B_MASK | KEYSCAN_B_MASK),
            0
        );
        assert_eq!(
            DIAGNOSTIC_CONTROL_C_MASK
                & (NOR_C_MASK
                    | LCD_C_MASK
                    | KEYSCAN_C_MASK
                    | RF_PRIMARY_C_MASK
                    | RF_SECONDARY_C_MASK),
            0
        );
        assert_eq!(DIAGNOSTIC_CONTROL_F_MASK & RF_PRIMARY_F_MASK, 0);
        assert_eq!(DIAGNOSTIC_CONTROL_F_MASK & 1, 0);
        assert_eq!(DIAGNOSTIC_CONTROL_F_MASK & RF_CONTROL_F_MASK, 1 << 8);
    }
}
