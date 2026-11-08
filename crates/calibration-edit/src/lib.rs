//! Offline tuning document with immutable original and masked explicit edits.
//!
//! This crate owns two 4096-byte arrays (8 KiB) for a host editor. `no_std` does
//! not imply that this memory budget is appropriate for radio firmware. There
//! is no allocator, file/serial I/O, default substitution, normalization, radio
//! write, or RF validation. Ranges describe storage encodings, not safe settings.
//! Parsing checks length and marker only; it does not verify a checksum.

#![no_std]
#![forbid(unsafe_code)]

use core::fmt;
use uv98_calibration::{Calibration, Filter, Table, BAND_COUNT, BAND_LEN, BLOCK_LEN, GENERAL_LEN};

const GENERAL_FIELDS: usize = 31;
const WIDTH_FIELDS: usize = 6;
const BAND_FIELDS: usize = 1 + 2 * WIDTH_FIELDS + 6 * 16;
const FIELD_COUNT: usize = GENERAL_FIELDS + BAND_COUNT * BAND_FIELDS;
const VALID_POINTS: [usize; BAND_COUNT] =
    [8, 16, 10, 16, 15, 16, 16, 16, 16, 16, 16, 16, 16, 16, 12];
const BAND_LABELS: [&str; BAND_COUNT] = [
    "136-174",
    "400-480",
    "18-64",
    "64-136",
    "174-240",
    "240-320",
    "320-400",
    "480-560",
    "560-640(660)",
    "840-920",
    "920-1000",
    "1000-1080",
    "1080-1160",
    "1160-1240",
    "1240-1300",
];

/// Exact tuning-tool tab labels in its stored record order, including the
/// original "560-640(660)" ambiguity. These are display labels, NOT the stock
/// firmware's selection boundaries, supported RX/TX ranges, or validation.
pub fn band_label(index: usize) -> Option<&'static str> {
    BAND_LABELS.get(index).copied()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneralField {
    Marker,
    TestMode,
    Register3eRaw,
    ExpanderModeCode,
    ExpanderAfCode,
    ExpanderNoiseCode,
    CompressorModeCode,
    CompressorAfCode,
    CompressorNoiseCode,
    VoxOnRaw,
    VoxOffRaw,
    AdcCorrectionCode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelWidth {
    Wide,
    Narrow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WidthField {
    MicGain,
    /// Full word: the related stock consumer uses all 16 bits.
    TxDeviationRaw,
    CtcssDeviationCode,
    DcsDeviationCode,
    VolumeGainCode,
    VolumeDacCode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    General(GeneralField),
    /// Full byte: the related stock caller does not apply the tuning UI's &7.
    Filter(Filter),
    BatteryPoint {
        index: usize,
    },
    FrequencyOffset {
        band: usize,
    },
    WidthParameter {
        band: usize,
        width: ChannelWidth,
        parameter: WidthField,
    },
    TablePoint {
        band: usize,
        table: Table,
        point: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Group {
    General,
    Band { index: usize },
}

/// Bounded evidence, not a claim of compatibility with every UV98 revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Consumption {
    Structural,
    /// Layout is confirmed; consumption was not observed in the reviewed paths.
    TuningLayoutOnly,
    /// A related RT880 1.16 path reads this field/table. Exact user MCU differs
    /// in provenance: it has not been read back and identified as that image.
    RelatedStockTraced,
}

/// A descriptor cannot be supplied to a writer; writers resolve `Field` again.
/// `value = ((raw_word & mask) >> shift) + bias`, little-endian storage.
/// Only `mask` bits change. min/max are representation limits, not RF limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Descriptor {
    pub field: Field,
    pub group: Group,
    pub offset: usize,
    pub width: u8,
    pub mask: u16,
    pub shift: u8,
    pub bias: i32,
    pub min: i32,
    pub max: i32,
    pub label: &'static str,
    pub detail: &'static str,
    pub editable: bool,
    pub consumption: Consumption,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Change {
    pub field: Field,
    pub value: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteDiff {
    pub offset: usize,
    pub original: u8,
    pub working: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Source(uv98_calibration::Error),
    BandOutOfRange {
        band: usize,
    },
    BatteryPointOutOfRange {
        index: usize,
    },
    TablePointOutOfRange {
        point: usize,
    },
    ReadOnly {
        field: Field,
    },
    ValueOutOfRange {
        field: Field,
        value: i32,
        min: i32,
        max: i32,
    },
    OverlappingChanges {
        first: usize,
        second: usize,
    },
    TooManyChanges {
        count: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Source(error) => write!(formatter, "{error}"),
            Self::BandOutOfRange { band } => write!(formatter, "band {band} is outside 0..15"),
            Self::BatteryPointOutOfRange { index } => {
                write!(formatter, "battery point {index} is outside 0..15")
            }
            Self::TablePointOutOfRange { point } => {
                write!(formatter, "table point {point} is outside 0..16")
            }
            Self::ReadOnly { field } => write!(formatter, "field {field:?} is read-only"),
            Self::ValueOutOfRange {
                field,
                value,
                min,
                max,
            } => write!(
                formatter,
                "field {field:?}: {value} is outside {min}..={max}"
            ),
            Self::OverlappingChanges { first, second } => write!(
                formatter,
                "changes {first} and {second} address the same bits"
            ),
            Self::TooManyChanges { count } => write!(formatter, "too many field changes: {count}"),
        }
    }
}

fn base(
    field: Field,
    offset: usize,
    width: u8,
    mask: u16,
    shift: u8,
    bias: i32,
    label: &'static str,
    detail: &'static str,
) -> Descriptor {
    Descriptor {
        field,
        group: Group::General,
        offset,
        width,
        mask,
        shift,
        bias,
        min: bias,
        max: i32::from(mask >> shift) + bias,
        label,
        detail,
        editable: true,
        consumption: Consumption::TuningLayoutOnly,
    }
}

/// Resolve a confirmed layout field, rejecting indices before any arithmetic.
pub fn descriptor(field: Field) -> Result<Descriptor, Error> {
    let mut result = match field {
        Field::General(general) => match general {
            GeneralField::Marker => {
                let mut value = base(field, 0, 1, 0xff, 0, 0, "Маркер блока", "Маркер 0x9A; проверка не является контрольной суммой.");
                value.editable = false;
                value.consumption = Consumption::Structural;
                value
            }
            GeneralField::TestMode => {
                let mut value = base(field, 1, 1, 0xff, 0, 0, "Тестовый режим (только чтение)", "Поле Stop Test / Start Test, а не вариант дисплея. Редактор калибровок не включает тестовый режим.");
                value.editable = false;
                value.consumption = Consumption::RelatedStockTraced;
                value
            }
            GeneralField::Register3eRaw => base(field, 2, 2, 0xffff, 0, 0,
                "REG 3E — сырое слово", "Полное слово из tuning layout. В рассмотренной инициализации stock использует константу; чтение этого поля там не подтверждено."),
            GeneralField::ExpanderModeCode => base(field, 4, 2, 0xc000, 14, 0,
                "Экспандер — режим (код)", "Биты 15..14 слова; остальные биты сохраняются. Использование tuning-поля в рассмотренном reset-пути не подтверждено."),
            GeneralField::ExpanderAfCode => base(field, 4, 2, 0x3f80, 7, 0,
                "Экспандер — AF (код)", "Биты 13..7 слова; режим и Noise сохраняются. Физические единицы не установлены."),
            GeneralField::ExpanderNoiseCode => base(field, 4, 2, 0x007f, 0, 0,
                "Экспандер — Noise (код)", "Биты 6..0 слова; остальные биты сохраняются. Физические единицы не установлены."),
            GeneralField::CompressorModeCode => base(field, 6, 2, 0xc000, 14, 0,
                "Компрессор — режим (код)", "Биты 15..14 слова; остальные биты сохраняются. Использование tuning-поля в рассмотренном reset-пути не подтверждено."),
            GeneralField::CompressorAfCode => base(field, 6, 2, 0x3f80, 7, 0,
                "Компрессор — AF (код)", "Биты 13..7 слова; режим и Noise сохраняются. Физические единицы не установлены."),
            GeneralField::CompressorNoiseCode => base(field, 6, 2, 0x007f, 0, 0,
                "Компрессор — Noise (код)", "Биты 6..0 слова; остальные биты сохраняются. Физические единицы не установлены."),
            GeneralField::VoxOnRaw => base(field, 12, 2, 0xffff, 0, 0,
                "VOX On — сырой код", "Поле tuning-программы. В рассмотренном VOX-пути используются другие настройки; потребление этого поля там не наблюдалось."),
            GeneralField::VoxOffRaw => base(field, 14, 2, 0xffff, 0, 0,
                "VOX Off — сырой код", "Поле tuning-программы. В рассмотренном VOX-пути используются другие настройки; потребление этого поля там не наблюдалось."),
            GeneralField::AdcCorrectionCode => base(field, 31, 1, 0xff, 0, -128,
                "Поправка ADC (код)", "Хранится как код+128. В рассмотренном пути DMA→индикатор это tuning-поле не наблюдалось; не единицы напряжения."),
        },
        Field::Filter(filter) => {
            let (offset, label) = match filter {
                Filter::Tx300 => (8, "Фильтр TX 300 — сырой индекс"),
                Filter::Tx3k => (9, "Фильтр TX 3k — сырой индекс"),
                Filter::Rx300 => (10, "Фильтр RX 300 — сырой индекс"),
                Filter::Rx3k => (11, "Фильтр RX 3k — сырой индекс"),
            };
            let mut value = base(field, offset, 1, 0xff, 0, 0, label,
                "Stock-путь использует весь байт, без UI-маски &7. Значения вне switch выбирают его fallback; диапазон 0..255 означает только хранение.");
            value.consumption = Consumption::RelatedStockTraced;
            value
        }
        Field::BatteryPoint { index } => {
            if index >= 15 { return Err(Error::BatteryPointOutOfRange { index }); }
            base(field, 16 + index, 1, 0xff, 0, 0, "Батарея — точка (сырой код)",
                "Tuning UI показывает байт/10. Физическая точность и использование этой таблицы в рассмотренном battery-пути не подтверждены.")
        }
        Field::FrequencyOffset { band } => {
            check_band(band)?;
            let mut value = base(field, GENERAL_LEN + band * BAND_LEN, 2, 0xffff, 0, -32768,
                "Поправка частоты (код)", "Offset-binary: uint16_le−32768. Это сохранённый код, не установленное значение в герцах.");
            value.group = Group::Band { index: band };
            value.consumption = Consumption::RelatedStockTraced;
            value
        }
        Field::WidthParameter { band, width, parameter } => {
            check_band(band)?;
            let offset = GENERAL_LEN + band * BAND_LEN + match width { ChannelWidth::Wide => 2, ChannelWidth::Narrow => 9 };
            let mut value = match parameter {
                WidthField::MicGain => base(field, offset, 1, 0xff, 0, 0,
                    "MIC gain — сырой код", "Полный байт tuning layout. Рассмотренный TX-путь берёт MIC из другой конфигурации; применение этого поля там не подтверждено."),
                WidthField::TxDeviationRaw => {
                    let mut value = base(field, offset + 1, 2, 0xffff, 0, 0,
                        "TX deviation — полное сырое слово", "Stock читает все 16 бит, включая верхние четыре. Редактирование намеренно заменяет всё слово; UI-маска &0xFFF не применяется.");
                    value.consumption = Consumption::RelatedStockTraced;
                    value
                }
                WidthField::CtcssDeviationCode => base(field, offset + 3, 1, 0x7f, 0, 0,
                    "CTCSS deviation (код)", "Меняются только младшие 7 бит; старший бит сохранён. Потребление tuning-поля в рассмотренных tone-путях не подтверждено."),
                WidthField::DcsDeviationCode => base(field, offset + 4, 1, 0x7f, 0, 0,
                    "DCS deviation (код)", "Меняются только младшие 7 бит; старший бит сохранён. Потребление tuning-поля в рассмотренных tone-путях не подтверждено."),
                WidthField::VolumeGainCode => base(field, offset + 5, 2, 0x03f0, 4, 0,
                    "Громкость — Gain (код)", "Меняются биты 9..4; DAC и верхние 6 бит сохранены. Рассмотренный RX-путь получает громкость из другой конфигурации."),
                WidthField::VolumeDacCode => base(field, offset + 5, 2, 0x000f, 0, 0,
                    "Громкость — DAC (код)", "Меняются только биты 3..0; Gain и верхние 6 бит сохранены. Физические единицы не установлены."),
            };
            value.group = Group::Band { index: band };
            value
        }
        Field::TablePoint { band, table, point } => {
            check_band(band)?;
            if point >= 16 { return Err(Error::TablePointOutOfRange { point }); }
            let (row, label) = match table {
                Table::PowerHigh => (0, "Мощность High — точка (код)"),
                Table::PowerLow => (1, "Мощность Low — точка (код)"),
                Table::NoiseWide => (2, "Noise Wide — точка (код)"),
                Table::SquelchWide => (3, "Squelch Wide — точка (код)"),
                Table::NoiseNarrow => (4, "Noise Narrow — точка (код)"),
                Table::SquelchNarrow => (5, "Squelch Narrow — точка (код)"),
            };
            let mut value = base(field, GENERAL_LEN + band * BAND_LEN + 16 + row * 16 + point,
                1, 0xff, 0, 0, label,
                "Сохранённый байт выбранной точки таблицы; не ватты, не dBm, не S-единицы. Для строки подтверждено чтение выбранной точки related stock.");
            value.group = Group::Band { index: band };
            value.consumption = Consumption::RelatedStockTraced;
            if point >= VALID_POINTS[band] {
                value.editable = false;
                value.consumption = Consumption::TuningLayoutOnly;
                value.detail = "Эту точку исходный tuning UI не показывает. Сохранена без изменений; её применение не предполагается.";
            }
            value
        }
    };
    // Keep the representable range bound to the effective contiguous bit field.
    result.min = result.bias;
    result.max = i32::from(result.mask >> result.shift) + result.bias;
    // The related selector's final branch repeats slot 13; it never selects
    // slot 14. Keep its known format editable without claiming traced use.
    if result.group == (Group::Band { index: 14 })
        && result.consumption == Consumption::RelatedStockTraced
    {
        result.consumption = Consumption::TuningLayoutOnly;
        result.detail = match field {
            Field::FrequencyOffset { .. } => "Offset-binary uint16_le−32768, не герцы. Related stock selector не выбирает запись 15: последняя ветка повторяет запись 14; применение этого блока не подтверждено.",
            Field::WidthParameter { .. } => "Полное сырое 16-битное слово. Related stock selector не выбирает запись 15: последняя ветка повторяет запись 14; применение этого блока не подтверждено.",
            _ => "Сырой код точки, не физические единицы. Related stock selector не выбирает запись 15: последняя ветка повторяет запись 14; применение этого блока не подтверждено.",
        };
    }
    Ok(result)
}

fn check_band(band: usize) -> Result<(), Error> {
    if band >= BAND_COUNT {
        Err(Error::BandOutOfRange { band })
    } else {
        Ok(())
    }
}

pub const fn field_count() -> usize {
    FIELD_COUNT
}

/// Stable ordering: general, then each band's offset, wide/narrow, six tables.
pub fn field_at(index: usize) -> Option<Field> {
    let general = match index {
        0 => Some(Field::General(GeneralField::Marker)),
        1 => Some(Field::General(GeneralField::TestMode)),
        2 => Some(Field::General(GeneralField::Register3eRaw)),
        3 => Some(Field::General(GeneralField::ExpanderModeCode)),
        4 => Some(Field::General(GeneralField::ExpanderAfCode)),
        5 => Some(Field::General(GeneralField::ExpanderNoiseCode)),
        6 => Some(Field::General(GeneralField::CompressorModeCode)),
        7 => Some(Field::General(GeneralField::CompressorAfCode)),
        8 => Some(Field::General(GeneralField::CompressorNoiseCode)),
        9 => Some(Field::Filter(Filter::Tx300)),
        10 => Some(Field::Filter(Filter::Tx3k)),
        11 => Some(Field::Filter(Filter::Rx300)),
        12 => Some(Field::Filter(Filter::Rx3k)),
        13 => Some(Field::General(GeneralField::VoxOnRaw)),
        14 => Some(Field::General(GeneralField::VoxOffRaw)),
        15..=29 => Some(Field::BatteryPoint { index: index - 15 }),
        30 => Some(Field::General(GeneralField::AdcCorrectionCode)),
        _ => None,
    };
    if index < GENERAL_FIELDS {
        return general;
    }
    if index >= FIELD_COUNT {
        return None;
    }
    let position = index - GENERAL_FIELDS;
    let band = position / BAND_FIELDS;
    let position = position % BAND_FIELDS;
    if position == 0 {
        return Some(Field::FrequencyOffset { band });
    }
    if position <= 2 * WIDTH_FIELDS {
        let offset = position - 1;
        let width = if offset < WIDTH_FIELDS {
            ChannelWidth::Wide
        } else {
            ChannelWidth::Narrow
        };
        let parameter = match offset % WIDTH_FIELDS {
            0 => WidthField::MicGain,
            1 => WidthField::TxDeviationRaw,
            2 => WidthField::CtcssDeviationCode,
            3 => WidthField::DcsDeviationCode,
            4 => WidthField::VolumeGainCode,
            _ => WidthField::VolumeDacCode,
        };
        return Some(Field::WidthParameter {
            band,
            width,
            parameter,
        });
    }
    let offset = position - 1 - 2 * WIDTH_FIELDS;
    let table = match offset / 16 {
        0 => Table::PowerHigh,
        1 => Table::PowerLow,
        2 => Table::NoiseWide,
        3 => Table::SquelchWide,
        4 => Table::NoiseNarrow,
        _ => Table::SquelchNarrow,
    };
    Some(Field::TablePoint {
        band,
        table,
        point: offset % 16,
    })
}

pub struct Fields {
    next: usize,
}
pub const fn fields() -> Fields {
    Fields { next: 0 }
}
impl Iterator for Fields {
    type Item = Field;
    fn next(&mut self) -> Option<Field> {
        let field = field_at(self.next)?;
        self.next += 1;
        Some(field)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = FIELD_COUNT - self.next;
        (remaining, Some(remaining))
    }
}
impl ExactSizeIterator for Fields {}
impl core::iter::FusedIterator for Fields {}

/// Clone is an explicit full 8 KiB copy, useful for a bounded host undo history.
#[derive(Clone)]
pub struct Document {
    original: [u8; BLOCK_LEN],
    working: [u8; BLOCK_LEN],
}

impl Document {
    pub fn new(bytes: &[u8]) -> Result<Self, Error> {
        let parsed = Calibration::parse(bytes).map_err(Error::Source)?;
        Ok(Self {
            original: *parsed.raw(),
            working: *parsed.raw(),
        })
    }
    pub const fn original(&self) -> &[u8; BLOCK_LEN] {
        &self.original
    }
    pub const fn bytes(&self) -> &[u8; BLOCK_LEN] {
        &self.working
    }
    /// Returns the exact working bytes. It does not write a file or a radio.
    pub const fn export_bytes(&self) -> &[u8; BLOCK_LEN] {
        &self.working
    }
    pub fn value(&self, field: Field) -> Result<i32, Error> {
        Ok(read_value(&self.working, descriptor(field)?))
    }
    pub fn original_value(&self, field: Field) -> Result<i32, Error> {
        Ok(read_value(&self.original, descriptor(field)?))
    }
    /// Entire storage byte/word, including bits outside the editable mask.
    pub fn raw_word(&self, field: Field) -> Result<u16, Error> {
        Ok(read_word(&self.working, descriptor(field)?))
    }
    pub fn original_raw_word(&self, field: Field) -> Result<u16, Error> {
        Ok(read_word(&self.original, descriptor(field)?))
    }
    pub fn set(&mut self, field: Field, value: i32) -> Result<bool, Error> {
        Ok(self.apply(&[Change { field, value }])? != 0)
    }
    /// All validation precedes all changes, without a third 4096-byte buffer.
    /// Repeated/overlapping bit changes are rejected, including identical ones.
    /// Disjoint fields in the same word are allowed. Returns changed FIELD count;
    /// use dirty_count() or diff() for changed bytes relative to the original.
    pub fn apply(&mut self, changes: &[Change]) -> Result<usize, Error> {
        if changes.len() > FIELD_COUNT {
            return Err(Error::TooManyChanges {
                count: changes.len(),
            });
        }
        for (index, change) in changes.iter().enumerate() {
            let current = descriptor(change.field)?;
            validate_change(current, change.value)?;
            for (prior_index, prior) in changes[..index].iter().enumerate() {
                if overlaps(descriptor(prior.field)?, current) {
                    return Err(Error::OverlappingChanges {
                        first: prior_index,
                        second: index,
                    });
                }
            }
        }
        let mut changed = 0;
        for change in changes {
            let field = descriptor(change.field).expect("all fields validated before mutation");
            if read_value(&self.working, field) != change.value {
                write_value(&mut self.working, field, change.value);
                changed += 1;
            }
        }
        Ok(changed)
    }
    /// Restore only this field's mask from the original, retaining edits to
    /// adjacent fields. This is not a factory-default operation.
    pub fn reset_field(&mut self, field: Field) -> Result<bool, Error> {
        let value = self.original_value(field)?;
        self.set(field, value)
    }
    /// Restore the exact imported original. Returns bytes reverted.
    pub fn reset_all(&mut self) -> usize {
        let changed = self.dirty_count();
        self.working.copy_from_slice(&self.original);
        changed
    }
    pub fn dirty_count(&self) -> usize {
        self.diff().count()
    }
    pub fn is_dirty(&self) -> bool {
        self.original != self.working
    }
    pub fn diff(&self) -> Diff<'_> {
        Diff {
            document: self,
            next: 0,
        }
    }
}

pub struct Diff<'a> {
    document: &'a Document,
    next: usize,
}
impl Iterator for Diff<'_> {
    type Item = ByteDiff;
    fn next(&mut self) -> Option<ByteDiff> {
        while self.next < BLOCK_LEN {
            let offset = self.next;
            self.next += 1;
            let original = self.document.original[offset];
            let working = self.document.working[offset];
            if original != working {
                return Some(ByteDiff {
                    offset,
                    original,
                    working,
                });
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(BLOCK_LEN - self.next))
    }
}
impl core::iter::FusedIterator for Diff<'_> {}

fn read_word(bytes: &[u8; BLOCK_LEN], field: Descriptor) -> u16 {
    if field.width == 1 {
        u16::from(bytes[field.offset])
    } else {
        u16::from_le_bytes([bytes[field.offset], bytes[field.offset + 1]])
    }
}
fn read_value(bytes: &[u8; BLOCK_LEN], field: Descriptor) -> i32 {
    i32::from((read_word(bytes, field) & field.mask) >> field.shift) + field.bias
}
fn write_value(bytes: &mut [u8; BLOCK_LEN], field: Descriptor, value: i32) {
    let encoded = ((value - field.bias) as u16) << field.shift;
    let word = (read_word(bytes, field) & !field.mask) | (encoded & field.mask);
    let pair = word.to_le_bytes();
    bytes[field.offset] = pair[0];
    if field.width == 2 {
        bytes[field.offset + 1] = pair[1];
    }
}
fn validate_change(field: Descriptor, value: i32) -> Result<(), Error> {
    if !field.editable {
        return Err(Error::ReadOnly { field: field.field });
    }
    if value < field.min || value > field.max {
        return Err(Error::ValueOutOfRange {
            field: field.field,
            value,
            min: field.min,
            max: field.max,
        });
    }
    Ok(())
}
fn overlaps(left: Descriptor, right: Descriptor) -> bool {
    for offset in left.offset..left.offset + usize::from(left.width) {
        if offset >= right.offset && offset < right.offset + usize::from(right.width) {
            let left_mask = (left.mask >> ((offset - left.offset) * 8)) as u8;
            let right_mask = (right.mask >> ((offset - right.offset) * 8)) as u8;
            if left_mask & right_mask != 0 {
                return true;
            }
        }
    }
    false
}
