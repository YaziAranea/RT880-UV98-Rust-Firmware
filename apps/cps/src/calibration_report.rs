//! Standalone UTF-8 report derived from bytes and known descriptors only.
//! No file, serial or RF operations. Unmapped bits are never given a meaning.
use std::fmt::Write;
use uv98_calibration::BLOCK_LEN;
use uv98_calibration_edit::{band_label, descriptor, fields, Consumption, Document, Field, Group};

fn label(field: Field) -> String {
    let d = descriptor(field).expect("enumerated descriptor");
    match field {
        Field::BatteryPoint { index } => format!("{} / точка {}", d.label, index + 1),
        Field::TablePoint { point, .. } => format!("{} / точка {:02}", d.label, point + 1),
        Field::WidthParameter { width, .. } => format!("{width:?} / {}", d.label),
        _ => d.label.into(),
    }
}

fn group_label(group: Group) -> String {
    match group {
        Group::General => "Общие параметры".into(),
        Group::Band { index } => format!("Блок {}: {} МГц", index + 1, band_label(index).unwrap()),
    }
}

fn single_line(value: Option<&str>) -> String {
    value
        .unwrap_or("не указан")
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

pub fn render(
    original: &[u8],
    working: &[u8],
    source: Option<&str>,
    source_sha256: Option<&str>,
) -> Result<String, String> {
    let original_doc = Document::new(original).map_err(|e| e.to_string())?;
    let working_doc = Document::new(working).map_err(|e| e.to_string())?;
    let mut known_masks = [0u8; BLOCK_LEN];
    let mut changed_fields = Vec::new();
    let mut notes = Vec::<&str>::new();
    for field in fields() {
        let d = descriptor(field).expect("enumerated descriptor");
        for byte in 0..usize::from(d.width) {
            known_masks[d.offset + byte] |= (d.mask >> (byte * 8)) as u8;
        }
        if original_doc.value(field).unwrap() != working_doc.value(field).unwrap() {
            changed_fields.push(field);
        }
        if !notes.contains(&d.detail) {
            notes.push(d.detail);
        }
    }
    let changed_bytes = original.iter().zip(working).filter(|(a, b)| a != b).count();
    let unknown_changed_bits: u32 = original
        .iter()
        .zip(working)
        .zip(known_masks)
        .map(|((&a, &b), known)| ((a ^ b) & !known).count_ones())
        .sum();
    let uncovered_bytes = known_masks.iter().filter(|&&mask| mask != 0xff).count();
    let mut out = String::new();
    writeln!(out, "КАЛИБРОВКИ UV98 — ОТЧЁТ О ЛОКАЛЬНОЙ КОПИИ").unwrap();
    writeln!(out, "Формат: UTF-8, обычный текст. Excel не требуется.").unwrap();
    writeln!(out, "Источник: {}", single_line(source)).unwrap();
    writeln!(
        out,
        "SHA256 файла-источника: {}",
        single_line(source_sha256)
    )
    .unwrap();
    writeln!(
        out,
        "SHA256 исходного блока (4096 байт): {}",
        crate::radio::sha256(original)
    )
    .unwrap();
    writeln!(
        out,
        "SHA256 рабочей копии (4096 байт): {}",
        crate::radio::sha256(working)
    )
    .unwrap();
    writeln!(out, "SHA файла-источника может относиться к NOR 4 МиБ; SHA блоков — только к первым 4096 байтам.").unwrap();
    writeln!(
        out,
        "Формат проверен по размеру и маркеру 0x9A. Контрольная сумма калибровок неизвестна."
    )
    .unwrap();
    writeln!(
        out,
        "Исходник не изменён. Отчёт не записывает данные в рацию и не заменяет бинарный резерв."
    )
    .unwrap();
    writeln!(out, "\nСВОДКА ИЗМЕНЕНИЙ").unwrap();
    writeln!(out, "Изменено известных полей: {}", changed_fields.len()).unwrap();
    writeln!(out, "Изменено байт: {changed_bytes} из {BLOCK_LEN}").unwrap();
    writeln!(out, "Изменено неописанных битов: {unknown_changed_bits}").unwrap();
    writeln!(
        out,
        "Байтов с неописанными битами: {uncovered_bytes}; им не приписаны названия или единицы."
    )
    .unwrap();
    if changed_fields.is_empty() {
        writeln!(out, "Известные поля не изменены.").unwrap();
    }
    for &field in &changed_fields {
        let d = descriptor(field).unwrap();
        writeln!(
            out,
            "{} | 0x{:04X} | {} | {} -> {} | маска 0x{:04X}",
            group_label(d.group),
            d.offset,
            label(field),
            original_doc.value(field).unwrap(),
            working_doc.value(field).unwrap(),
            d.mask
        )
        .unwrap();
    }
    writeln!(out, "\nЕДИНИЦЫ И ГРАНИЦЫ ИНТЕРПРЕТАЦИИ").unwrap();
    writeln!(
        out,
        "Числа в таблицах — коды хранения, а не измеренные ватты, dBm, S-единицы или герцы."
    )
    .unwrap();
    writeln!(
        out,
        "Поправка частоты: uint16 LE минус 32768. Подписи диапазонов взяты из tuning UI,"
    )
    .unwrap();
    writeln!(
        out,
        "а не из измерений чувствительности или разрешённых диапазонов RX/TX."
    )
    .unwrap();
    writeln!(
        out,
        "Для батареи штатный UI показывает байт/10; точность и физическая единица не подтверждены."
    )
    .unwrap();
    writeln!(out, "Адреса 0x0000..0x0FFF — смещения внутри блока; для извлечения из начала NOR они совпадают с NOR-адресами.").unwrap();
    writeln!(out, "Маска выделяет описанные биты слова little-endian; RAW показывает всё слово, включая остальные биты.").unwrap();
    writeln!(out, "Статус «разрешена правка» означает только возможность локального редактора, не безопасную RF-настройку.").unwrap();
    writeln!(out, "\nТОЧНАЯ ПОБАЙТОВАЯ РАЗНИЦА").unwrap();
    writeln!(
        out,
        "Адрес | исходный RAW -> рабочий RAW | маска изменённых неописанных битов"
    )
    .unwrap();
    if changed_bytes == 0 {
        writeln!(out, "Байтовых изменений нет.").unwrap();
    }
    for (offset, (&a, &b)) in original.iter().zip(working).enumerate() {
        if a != b {
            writeln!(
                out,
                "0x{offset:04X} | 0x{a:02X} -> 0x{b:02X} | 0x{:02X}",
                (a ^ b) & !known_masks[offset]
            )
            .unwrap();
        }
    }
    writeln!(out, "\nВСЕ ИЗВЕСТНЫЕ ПОЛЯ").unwrap();
    writeln!(out, "Адрес | поле | код исходный -> рабочий | RAW исходный -> рабочий | маска | статус | примечание").unwrap();
    let mut previous_group = None;
    for field in fields() {
        let d = descriptor(field).unwrap();
        if previous_group != Some(d.group) {
            writeln!(out, "\n{}", group_label(d.group)).unwrap();
            previous_group = Some(d.group);
        }
        let evidence = match d.consumption {
            Consumption::Structural => "структура",
            Consumption::TuningLayoutOnly => "только tuning layout",
            Consumption::RelatedStockTraced => "чтение найдено в related stock",
        };
        let note = notes.iter().position(|&n| n == d.detail).unwrap() + 1;
        writeln!(
            out,
            "0x{:04X} | {} | {} -> {} | 0x{:04X} -> 0x{:04X} | 0x{:04X} | {}, {} | [{}]",
            d.offset,
            label(field),
            original_doc.value(field).unwrap(),
            working_doc.value(field).unwrap(),
            original_doc.raw_word(field).unwrap(),
            working_doc.raw_word(field).unwrap(),
            d.mask,
            if d.editable {
                "разрешена правка"
            } else {
                "только чтение"
            },
            evidence,
            note
        )
        .unwrap();
    }
    writeln!(out, "\nПОЯСНЕНИЯ ИЗ ОПИСАНИЯ ФОРМАТА").unwrap();
    for (index, detail) in notes.iter().enumerate() {
        writeln!(out, "[{}] {detail}", index + 1).unwrap();
    }
    writeln!(
        out,
        "\nRF-калибровка и аппаратная совместимость этим отчётом не подтверждаются."
    )
    .unwrap();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uv98_calibration_edit::{ChannelWidth, GeneralField, WidthField};
    const ORIGINAL: &[u8] =
        include_bytes!("../../../crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin");

    #[test]
    fn report_uses_actual_hashes_and_signed_or_masked_values_without_changing_bytes() {
        let mut doc = Document::new(ORIGINAL).unwrap();
        doc.set(Field::FrequencyOffset { band: 1 }, -123).unwrap();
        doc.set(Field::General(GeneralField::AdcCorrectionCode), -17)
            .unwrap();
        let gain = Field::WidthParameter {
            band: 1,
            width: ChannelWidth::Wide,
            parameter: WidthField::VolumeGainCode,
        };
        doc.set(gain, 23).unwrap();
        let before = *doc.bytes();
        let text = render(
            doc.original(),
            doc.bytes(),
            Some("source.bin"),
            Some("source digest"),
        )
        .unwrap();
        assert!(text.contains(&crate::radio::sha256(ORIGINAL)));
        assert!(text.contains(&crate::radio::sha256(&before)));
        assert!(text.contains("Изменено известных полей: 3"));
        assert!(text.contains("Изменено неописанных битов: 0"));
        assert!(text.contains("-> -123"));
        assert!(text.contains("-> -17"));
        assert!(text.contains("маска 0x03F0"));
        assert!(text.contains("не измеренные ватты"));
        assert_eq!(doc.original().as_slice(), ORIGINAL);
        assert_eq!(doc.bytes(), &before);
    }

    #[test]
    fn unknown_bit_changes_are_exact_raw_differences_not_invented_fields() {
        let mut work = ORIGINAL.to_vec();
        work[4095] ^= 0x80;
        let text = render(ORIGINAL, &work, None, None).unwrap();
        assert!(text.contains("Изменено известных полей: 0"));
        assert!(text.contains("Изменено байт: 1 из 4096"));
        assert!(text.contains("Изменено неописанных битов: 1"));
        assert!(text.contains(&format!(
            "0x0FFF | 0x{:02X} -> 0x{:02X} | 0x80",
            ORIGINAL[4095], work[4095]
        )));
        assert!(text.contains("Источник: не указан"));
        assert!(render(&[0; 3], &work, None, None).is_err());
    }

    #[test]
    fn unchanged_report_and_source_control_characters_are_unambiguous() {
        let text = render(ORIGINAL, ORIGINAL, Some("a\nSHA256: fake\t.bin"), None).unwrap();
        assert!(text.contains("Источник: a\\nSHA256: fake\\t.bin\n"));
        assert!(text.contains("Байтовых изменений нет."));
        assert!(text.contains("Изменено известных полей: 0"));
        assert!(text.contains("Блок 15:"));
        assert!(text.contains("только чтение"));
    }
}
