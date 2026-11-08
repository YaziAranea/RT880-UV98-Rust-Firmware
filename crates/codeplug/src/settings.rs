//! Confirmed whole-byte CFG fields from RT880 CPS 1.10.
//!
//! Codes are CPS indices or stored integer values, not physical measurements.
//! Reading preserves even unknown codes. Editing changes exactly one byte;
//! no defaults, magic bytes, VFO records, or unrelated settings are regenerated.

use crate::CFG_LEN;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    VoicePrompt,
    KeyBeep,
    KeyLock,
    AutoLockDelay,
    KeypadLight,
    BacklightLevel,
    BacklightDelay,
    SaverMode,
    SaverStart,
    MenuExitDelay,
    TuningStep,
    ScanDirection,
    ScanMode,
    ScanReturn,
    ScanDwell,
    Squelch,
    Vox,
    VoxThreshold,
    VoxDelay,
    RxSpeaker,
    RxDac,
    RxSpeakerAm,
    RxDacAm,
    FmStandby,
}

impl Field {
    /// Display order is grouped; it does not imply a serialization order.
    pub const ALL: [Self; 24] = [
        Self::VoicePrompt,
        Self::KeyBeep,
        Self::RxSpeaker,
        Self::RxDac,
        Self::RxSpeakerAm,
        Self::RxDacAm,
        Self::KeyLock,
        Self::AutoLockDelay,
        Self::KeypadLight,
        Self::BacklightLevel,
        Self::BacklightDelay,
        Self::MenuExitDelay,
        Self::SaverMode,
        Self::SaverStart,
        Self::Squelch,
        Self::TuningStep,
        Self::ScanDirection,
        Self::ScanMode,
        Self::ScanReturn,
        Self::ScanDwell,
        Self::Vox,
        Self::VoxThreshold,
        Self::VoxDelay,
        Self::FmStandby,
    ];

    pub const fn offset(self) -> usize {
        match self {
            Self::VoicePrompt => 92,
            Self::KeyBeep => 93,
            Self::KeyLock => 94,
            Self::AutoLockDelay => 95,
            Self::KeypadLight => 96,
            Self::BacklightLevel => 97,
            Self::BacklightDelay => 98,
            Self::SaverMode => 99,
            Self::SaverStart => 100,
            Self::MenuExitDelay => 101,
            Self::TuningStep => 120,
            Self::ScanDirection => 156,
            Self::ScanMode => 157,
            Self::ScanReturn => 158,
            Self::ScanDwell => 159,
            Self::Squelch => 164,
            Self::Vox => 167,
            Self::VoxThreshold => 168,
            Self::VoxDelay => 169,
            Self::RxSpeaker => 174,
            Self::RxDac => 175,
            Self::RxSpeakerAm => 176,
            Self::RxDacAm => 177,
            Self::FmStandby => 261,
        }
    }

    /// Maximum code selectable in the examined CPS control.
    pub const fn max_code(self) -> u8 {
        match self {
            Self::VoicePrompt
            | Self::KeyBeep
            | Self::KeyLock
            | Self::KeypadLight
            | Self::FmStandby
            | Self::ScanDirection
            | Self::ScanReturn => 1,
            Self::BacklightLevel => 4,
            Self::AutoLockDelay | Self::BacklightDelay | Self::MenuExitDelay => 42,
            Self::SaverMode => 3,
            Self::SaverStart => 42,
            Self::TuningStep => 13,
            Self::ScanMode => 2,
            Self::ScanDwell => 30,
            Self::Squelch => 10,
            Self::Vox => 1,
            Self::VoxThreshold => 255,
            Self::VoxDelay => 10,
            Self::RxSpeaker | Self::RxSpeakerAm => 63,
            Self::RxDac | Self::RxDacAm => 15,
        }
    }

    pub const fn label_ru(self) -> &'static str {
        match self {
            Self::VoicePrompt => "Голосовые подсказки",
            Self::KeyBeep => "Звук клавиш",
            Self::KeyLock => "Блокировка клавиатуры",
            Self::AutoLockDelay => "Таймер блокировки клавиатуры",
            Self::KeypadLight => "Подсветка клавиатуры",
            Self::BacklightLevel => "Яркость экрана",
            Self::BacklightDelay => "Таймер подсветки экрана",
            Self::SaverMode => "Энергосбережение",
            Self::SaverStart => "Задержка энергосбережения",
            Self::MenuExitDelay => "Таймер выхода из меню",
            Self::TuningStep => "Шаг настройки частоты",
            Self::ScanDirection => "Направление сканирования",
            Self::ScanMode => "Режим сканирования",
            Self::ScanReturn => "Канал возврата после сканирования",
            Self::ScanDwell => "Таймер остановки сканирования",
            Self::Squelch => "Шумоподавитель",
            Self::Vox => "VOX",
            Self::VoxThreshold => "Порог VOX",
            Self::VoxDelay => "Задержка VOX",
            Self::RxSpeaker => "Усиление динамика FM (код)",
            Self::RxDac => "Усиление ЦАП FM (код)",
            Self::RxSpeakerAm => "Усиление динамика AM (код)",
            Self::RxDacAm => "Усиление ЦАП AM (код)",
            Self::FmStandby => "Дежурный приём FM-радио",
        }
    }

    pub const fn group_ru(self) -> &'static str {
        match self {
            Self::VoicePrompt
            | Self::KeyBeep
            | Self::RxSpeaker
            | Self::RxDac
            | Self::RxSpeakerAm
            | Self::RxDacAm => "Звук",
            Self::KeyLock
            | Self::AutoLockDelay
            | Self::KeypadLight
            | Self::BacklightLevel
            | Self::BacklightDelay
            | Self::MenuExitDelay => "Экран и клавиатура",
            Self::SaverMode | Self::SaverStart => "Энергосбережение",
            Self::Squelch
            | Self::TuningStep
            | Self::ScanDirection
            | Self::ScanMode
            | Self::ScanReturn
            | Self::ScanDwell => "Приём и сканирование",
            Self::Vox | Self::VoxThreshold | Self::VoxDelay => "VOX",
            Self::FmStandby => "FM-радио",
        }
    }

    /// Proven CPS option captions, translated to Russian. None means either a
    /// raw numeric control or an unknown code; callers must keep the raw code.
    /// General timer captions have no confirmed unit, so no seconds are added.
    pub const fn choice_label(self, code: u8) -> Option<&'static str> {
        if code > self.max_code() {
            return None;
        }
        match self {
            Self::VoicePrompt | Self::KeyBeep | Self::KeypadLight | Self::Vox | Self::FmStandby => {
                Some(if code == 0 {
                    "Выключено"
                } else {
                    "Включено"
                })
            }
            Self::KeyLock => Some(if code == 0 {
                "Разблокирована"
            } else {
                "Заблокирована"
            }),
            Self::SaverMode => Some(["Выключено", "1:1", "1:2", "1:3"][code as usize]),
            Self::AutoLockDelay | Self::BacklightDelay | Self::MenuExitDelay => {
                Some(if code == 0 {
                    "Выключено"
                } else {
                    TIMER_CAPTIONS[code as usize]
                })
            }
            Self::SaverStart => Some(TIMER_CAPTIONS[code as usize]),
            Self::ScanDirection => Some(if code == 0 { "Вверх" } else { "Вниз" }),
            Self::ScanMode => {
                Some(["По несущей (CO)", "По времени (TO)", "Поиск (SE)"][code as usize])
            }
            Self::ScanReturn => Some(if code == 0 {
                "Исходный канал"
            } else {
                "Текущий канал"
            }),
            Self::TuningStep => Some(STEP_CAPTIONS[code as usize]),
            Self::VoxDelay => Some(
                [
                    "0 с", "0,5 с", "1 с", "1,5 с", "2 с", "2,5 с", "3 с", "3,5 с", "4 с", "4,5 с",
                    "5 с",
                ][code as usize],
            ),
            _ => None,
        }
    }

    /// Numerical caption of a confirmed 43-choice timer. It is not a duration
    /// in seconds. Code0 is Disabled for lock/backlight/menu, but "0" for saver.
    pub const fn timer_value(self, code: u8) -> Option<u16> {
        if !matches!(
            self,
            Self::AutoLockDelay | Self::BacklightDelay | Self::MenuExitDelay | Self::SaverStart
        ) || code > 42
        {
            return None;
        }
        Some(match code {
            0 => 0,
            1 => 5,
            2 => 10,
            _ => (code as u16 - 2) * 15,
        })
    }
}

const TIMER_CAPTIONS: [&str; 43] = [
    "0", "5", "10", "15", "30", "45", "60", "75", "90", "105", "120", "135", "150", "165", "180",
    "195", "210", "225", "240", "255", "270", "285", "300", "315", "330", "345", "360", "375",
    "390", "405", "420", "435", "450", "465", "480", "495", "510", "525", "540", "555", "570",
    "585", "600",
];
const STEP_CAPTIONS: [&str; 14] = [
    "0,25 кГц",
    "1,25 кГц",
    "2,5 кГц",
    "5 кГц",
    "6,25 кГц",
    "10 кГц",
    "12,5 кГц",
    "20 кГц",
    "25 кГц",
    "50 кГц",
    "100 кГц",
    "500 кГц",
    "1 МГц",
    "5 МГц",
];

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanDirection {
    Up = 0,
    Down = 1,
}
impl ScanDirection {
    pub const fn decode(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Up),
            1 => Some(Self::Down),
            _ => None,
        }
    }
    pub const fn code(self) -> u8 {
        self as u8
    }
}

/// CPS mode labels only; this does not implement or infer a receiver algorithm.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanMode {
    Co = 0,
    To = 1,
    Se = 2,
}
impl ScanMode {
    pub const fn decode(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Co),
            1 => Some(Self::To),
            2 => Some(Self::Se),
            _ => None,
        }
    }
    pub const fn code(self) -> u8 {
        self as u8
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanReturn {
    OriginalChannel = 0,
    CurrentChannel = 1,
}
impl ScanReturn {
    pub const fn decode(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::OriginalChannel),
            1 => Some(Self::CurrentChannel),
            _ => None,
        }
    }
    pub const fn code(self) -> u8 {
        self as u8
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuningStep {
    Hz250 = 0,
    Hz1250,
    Hz2500,
    Hz5000,
    Hz6250,
    Hz10000,
    Hz12500,
    Hz20000,
    Hz25000,
    Hz50000,
    Hz100000,
    Hz500000,
    Hz1000000,
    Hz5000000,
}
impl TuningStep {
    pub const ALL: [Self; 14] = [
        Self::Hz250,
        Self::Hz1250,
        Self::Hz2500,
        Self::Hz5000,
        Self::Hz6250,
        Self::Hz10000,
        Self::Hz12500,
        Self::Hz20000,
        Self::Hz25000,
        Self::Hz50000,
        Self::Hz100000,
        Self::Hz500000,
        Self::Hz1000000,
        Self::Hz5000000,
    ];
    pub const fn decode(raw: u8) -> Option<Self> {
        if raw < 14 {
            Some(Self::ALL[raw as usize])
        } else {
            None
        }
    }
    pub const fn code(self) -> u8 {
        self as u8
    }
    /// CPS frequency-step caption converted exactly to the codeplug 10 Hz unit.
    /// This does not qualify a requested hardware tuning range or step.
    pub const fn units_10hz(self) -> u32 {
        [
            25, 125, 250, 500, 625, 1000, 1250, 2000, 2500, 5000, 10000, 50000, 100000, 500000,
        ][self as usize]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    WrongLength { actual: usize },
    InvalidCode { field: Field, value: u8 },
}

#[derive(Clone, Copy, Debug)]
pub struct Settings<'a> {
    raw: &'a [u8; CFG_LEN],
}

impl<'a> Settings<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let raw = bytes.try_into().map_err(|_| Error::WrongLength {
            actual: bytes.len(),
        })?;
        Ok(Self { raw })
    }

    pub fn raw(&self) -> &'a [u8; CFG_LEN] {
        self.raw
    }

    /// Returns the original byte, including codes outside the CPS UI range.
    pub fn code(&self, field: Field) -> u8 {
        self.raw[field.offset()]
    }

    pub fn scan_direction(&self) -> Option<ScanDirection> {
        ScanDirection::decode(self.code(Field::ScanDirection))
    }
    pub fn scan_mode(&self) -> Option<ScanMode> {
        ScanMode::decode(self.code(Field::ScanMode))
    }
    pub fn scan_return(&self) -> Option<ScanReturn> {
        ScanReturn::decode(self.code(Field::ScanReturn))
    }
    pub fn tuning_step(&self) -> Option<TuningStep> {
        TuningStep::decode(self.code(Field::TuningStep))
    }
}

#[derive(Debug)]
pub struct SettingsMut<'a> {
    raw: &'a mut [u8; CFG_LEN],
}

impl<'a> SettingsMut<'a> {
    pub fn parse(bytes: &'a mut [u8]) -> Result<Self, Error> {
        let actual = bytes.len();
        let raw = bytes
            .try_into()
            .map_err(|_| Error::WrongLength { actual })?;
        Ok(Self { raw })
    }

    pub fn view(&self) -> Settings<'_> {
        Settings { raw: self.raw }
    }

    /// Validates before changing the one field byte; errors leave bytes intact.
    pub fn set_code(&mut self, field: Field, value: u8) -> Result<(), Error> {
        if value > field.max_code() {
            return Err(Error::InvalidCode { field, value });
        }
        self.raw[field.offset()] = value;
        Ok(())
    }

    pub fn set_scan_direction(&mut self, value: ScanDirection) {
        self.raw[Field::ScanDirection.offset()] = value.code();
    }
    pub fn set_scan_mode(&mut self, value: ScanMode) {
        self.raw[Field::ScanMode.offset()] = value.code();
    }
    pub fn set_scan_return(&mut self, value: ScanReturn) {
        self.raw[Field::ScanReturn.offset()] = value.code();
    }
    pub fn set_tuning_step(&mut self, value: TuningStep) {
        self.raw[Field::TuningStep.offset()] = value.code();
    }
}
