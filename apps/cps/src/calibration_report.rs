//! Standalone UTF-8 report derived from bytes and known descriptors only.
//! No file, serial or RF operations. Unmapped bits are never given a meaning.
use crate::i18n::{Language, LocalizedText};
use std::fmt::Write;
use uv98_calibration::{Filter, Table, BAND_COUNT, BLOCK_LEN, MARKER};
use uv98_calibration_edit::{
    band_label, descriptor, fields, ChannelWidth, Consumption, Descriptor, Document, Field,
    GeneralField, Group, WidthField,
};

pub(crate) fn field_label(d: Descriptor, language: Language) -> String {
    let (en, de) = match d.field {
        Field::General(field) => match field {
            GeneralField::Marker => ("Block marker", "Blockkennung"),
            GeneralField::TestMode => ("Test mode (read-only)", "Testmodus (schreibgeschützt)"),
            GeneralField::Register3eRaw => ("REG 3E raw word", "REG 3E Rohwort"),
            GeneralField::ExpanderModeCode => ("Expander mode code", "Expander-Moduscode"),
            GeneralField::ExpanderAfCode => ("Expander AF code", "Expander-AF-Code"),
            GeneralField::ExpanderNoiseCode => ("Expander noise code", "Expander-Rauschcode"),
            GeneralField::CompressorModeCode => ("Compressor mode code", "Kompressor-Moduscode"),
            GeneralField::CompressorAfCode => ("Compressor AF code", "Kompressor-AF-Code"),
            GeneralField::CompressorNoiseCode => ("Compressor noise code", "Kompressor-Rauschcode"),
            GeneralField::VoxOnRaw => ("VOX On raw code", "VOX Ein (Rohcode)"),
            GeneralField::VoxOffRaw => ("VOX Off raw code", "VOX Aus (Rohcode)"),
            GeneralField::AdcCorrectionCode => ("ADC correction code", "ADC-Korrekturcode"),
        },
        Field::Filter(filter) => match filter {
            Filter::Tx300 => ("TX 300 filter raw index", "TX-300-Filter (Rohindex)"),
            Filter::Tx3k => ("TX 3k filter raw index", "TX-3k-Filter (Rohindex)"),
            Filter::Rx300 => ("RX 300 filter raw index", "RX-300-Filter (Rohindex)"),
            Filter::Rx3k => ("RX 3k filter raw index", "RX-3k-Filter (Rohindex)"),
        },
        Field::BatteryPoint { .. } => ("Battery raw code", "Batterie-Rohcode"),
        Field::FrequencyOffset { .. } => ("Frequency offset code", "Frequenzkorrekturcode"),
        Field::WidthParameter { parameter, .. } => match parameter {
            WidthField::MicGain => ("MIC gain raw code", "MIC-Verstärkung (Rohcode)"),
            WidthField::TxDeviationRaw => (
                "TX deviation full raw word",
                "TX-Hub (vollständiges Rohwort)",
            ),
            WidthField::CtcssDeviationCode => ("CTCSS deviation code", "CTCSS-Hubcode"),
            WidthField::DcsDeviationCode => ("DCS deviation code", "DCS-Hubcode"),
            WidthField::VolumeGainCode => ("Volume gain code", "Lautstärke: Verstärkungscode"),
            WidthField::VolumeDacCode => ("Volume DAC code", "Lautstärke: DAC-Code"),
        },
        Field::TablePoint { table, .. } => match table {
            Table::PowerHigh => ("High power code", "Code für hohe Sendeleistung"),
            Table::PowerLow => ("Low power code", "Code für niedrige Sendeleistung"),
            Table::NoiseWide => ("Noise code (wide)", "Rauschcode (breit)"),
            Table::NoiseNarrow => ("Noise code (narrow)", "Rauschcode (schmal)"),
            Table::SquelchWide => ("Squelch code (wide)", "Rauschsperrencode (breit)"),
            Table::SquelchNarrow => ("Squelch code (narrow)", "Rauschsperrencode (schmal)"),
        },
    };
    let label = match language {
        Language::Russian => d.label,
        Language::English => en,
        Language::German => de,
    };
    match d.field {
        Field::BatteryPoint { index } => trf!(
            language,
            "{label} / точка {}",
            "{label} / point {}",
            "{label} / Punkt {}",
            index + 1
        ),
        Field::TablePoint { point, .. } => trf!(
            language,
            "{label} / точка {:02}",
            "{label} / point {:02}",
            "{label} / Punkt {:02}",
            point + 1
        ),
        Field::WidthParameter { width, .. } => {
            let width = match width {
                ChannelWidth::Wide => tr!(language, "Wide", "Wide", "Breit"),
                ChannelWidth::Narrow => tr!(language, "Narrow", "Narrow", "Schmal"),
            };
            format!("{width} / {label}")
        }
        _ => label.into(),
    }
}

pub(crate) fn field_detail(d: Descriptor, language: Language) -> &'static str {
    if language == Language::Russian {
        return d.detail;
    }
    let (en, de) = match d.field {
        Field::TablePoint { .. } if !d.editable => (
            "The original tuning UI hides this point. Its value is preserved and its use is unknown.",
            "Die Original-Tuningoberfläche blendet diesen Punkt aus. Sein Wert bleibt erhalten und seine Verwendung ist unbekannt.",
        ),
        Field::FrequencyOffset { band: 14 } => (
            "Offset binary: uint16 LE minus 32768. The value is a code. The examined stock selector repeats record 14 in its final branch. It never selects record 15, whose use remains unconfirmed.",
            "Offset-Binärformat: uint16 LE minus 32768. Der Wert ist ein Code. Die untersuchte Original-Firmware wählt im letzten Zweig erneut Datensatz 14. Die Verwendung von Datensatz 15 ist daher unbestätigt.",
        ),
        Field::WidthParameter { band: 14, parameter: WidthField::TxDeviationRaw, .. } => (
            "Full raw 16-bit word. The examined stock selector repeats record 14 in its final branch. It never selects record 15, whose use remains unconfirmed.",
            "Vollständiges 16-Bit-Rohwort. Die untersuchte Original-Firmware wählt im letzten Zweig erneut Datensatz 14. Die Verwendung von Datensatz 15 ist daher unbestätigt.",
        ),
        Field::TablePoint { band: 14, .. } => (
            "Raw point code with no confirmed physical unit. The examined stock selector repeats record 14 in its final branch. It never selects record 15, whose use remains unconfirmed.",
            "Rohcode des Punkts ohne bestätigte physikalische Einheit. Die untersuchte Original-Firmware wählt im letzten Zweig erneut Datensatz 14. Die Verwendung von Datensatz 15 ist daher unbestätigt.",
        ),
        Field::General(field) => match field {
            GeneralField::Marker => ("Marker 0x9A checks the block format. A checksum check is unavailable.", "Die Kennung 0x9A prüft das Blockformat. Eine Prüfsummenprüfung ist nicht verfügbar."),
            GeneralField::TestMode => ("Stop Test / Start Test field. Editing calibration data cannot enable test mode.", "Feld für Stop Test / Start Test. Beim Bearbeiten der Kalibrierdaten lässt sich der Testmodus nicht einschalten."),
            GeneralField::Register3eRaw => ("Full tuning word. The examined stock initialization uses a constant. Reads from this field were not observed.", "Vollständiges Tuningwort. Die untersuchte Initialisierung der Original-Firmware verwendet eine Konstante. Lesezugriffe auf dieses Feld wurden nicht beobachtet."),
            GeneralField::ExpanderModeCode | GeneralField::CompressorModeCode => ("Bits 15–14 of the word. Other bits are preserved. Use of this tuning field in the examined reset routine is unconfirmed.", "Bits 15–14 des Worts. Die übrigen Bits bleiben erhalten. Die Verwendung dieses Tuningfelds in der untersuchten Reset-Routine ist unbestätigt."),
            GeneralField::ExpanderAfCode | GeneralField::CompressorAfCode => ("Bits 13–7 of the word. Mode and noise bits are preserved. Physical units are unknown.", "Bits 13–7 des Worts. Modus- und Rauschbits bleiben erhalten. Die physikalischen Einheiten sind unbekannt."),
            GeneralField::ExpanderNoiseCode | GeneralField::CompressorNoiseCode => ("Bits 6–0 of the word. Other bits are preserved. Physical units are unknown.", "Bits 6–0 des Worts. Die übrigen Bits bleiben erhalten. Die physikalischen Einheiten sind unbekannt."),
            GeneralField::VoxOnRaw | GeneralField::VoxOffRaw => ("Tuning field. The examined VOX routine uses other settings. Use of this field was not observed.", "Tuningfeld. Die untersuchte VOX-Routine verwendet andere Einstellungen. Eine Verwendung dieses Felds wurde nicht beobachtet."),
            GeneralField::AdcCorrectionCode => ("Stored as code + 128. Use of this tuning field between DMA and the display was not observed. The code has no confirmed voltage unit.", "Gespeichert als Code + 128. Die Verwendung dieses Tuningfelds zwischen DMA und Anzeige wurde nicht beobachtet. Für den Code ist keine Spannungseinheit bestätigt."),
        },
        Field::Filter(_) => ("The examined stock routine reads the full byte and uses its fallback for unmatched values. The tuning UI applies &7. The range 0–255 describes storage.", "Die untersuchte Originalroutine liest das ganze Byte und verwendet den Standardfall für unbekannte Werte. Die Tuningoberfläche verwendet &7. Der Bereich 0–255 beschreibt die Speicherung."),
        Field::BatteryPoint { .. } => ("The tuning UI divides the byte by 10. Physical accuracy and use of this table in the examined battery routine are unconfirmed.", "Die Tuningoberfläche teilt das Byte durch 10. Die physikalische Genauigkeit und die Verwendung in der untersuchten Batterieroutine sind unbestätigt."),
        Field::FrequencyOffset { .. } => ("Offset binary: uint16 LE minus 32768. The value is a stored code with no confirmed conversion to hertz.", "Offset-Binärformat: uint16 LE minus 32768. Der gespeicherte Code hat keine bestätigte Umrechnung in Hertz."),
        Field::WidthParameter { parameter, .. } => match parameter {
            WidthField::MicGain => ("Full tuning byte. The examined TX routine reads MIC gain from another configuration. Use of this field is unconfirmed.", "Vollständiges Tuningbyte. Die untersuchte TX-Routine liest die MIC-Verstärkung aus einer anderen Konfiguration. Die Verwendung dieses Felds ist unbestätigt."),
            WidthField::TxDeviationRaw => ("Stock firmware reads all 16 bits, including the upper four. The editor replaces the entire word without applying the UI mask &0xFFF.", "Die Original-Firmware liest alle 16 Bits einschließlich der oberen vier. Der Editor ersetzt das gesamte Wort ohne die UI-Maske &0xFFF."),
            WidthField::CtcssDeviationCode | WidthField::DcsDeviationCode => ("Edits affect the lower seven bits. The top bit is preserved. Use in the examined tone routines is unconfirmed.", "Änderungen betreffen die unteren sieben Bits. Das oberste Bit bleibt erhalten. Die Verwendung in den untersuchten Ton-Routinen ist unbestätigt."),
            WidthField::VolumeGainCode => ("Edits affect bits 9–4. DAC and the upper six bits are preserved. The examined RX routine reads volume from another configuration.", "Änderungen betreffen Bits 9–4. DAC und die oberen sechs Bits bleiben erhalten. Die untersuchte RX-Routine liest die Lautstärke aus einer anderen Konfiguration."),
            WidthField::VolumeDacCode => ("Edits affect bits 3–0. Gain and the upper six bits are preserved. Physical units are unknown.", "Änderungen betreffen Bits 3–0. Verstärkung und die oberen sechs Bits bleiben erhalten. Die physikalischen Einheiten sind unbekannt."),
        },
        Field::TablePoint { .. } => ("Stored byte for the selected table point. The related stock firmware was traced reading the selected point in this row. The value is a code with no confirmed physical unit.", "Gespeichertes Byte für den ausgewählten Tabellenpunkt. In der verwandten Original-Firmware wurde ein Lesezugriff auf den ausgewählten Punkt dieser Zeile gefunden. Der Code hat keine bestätigte physikalische Einheit."),
    };
    match language {
        Language::Russian => d.detail,
        Language::English => en,
        Language::German => de,
    }
}

pub(crate) fn general_label(language: Language) -> &'static str {
    tr!(
        language,
        "Общие параметры",
        "General settings",
        "Allgemeine Einstellungen"
    )
}

pub(crate) fn band_caption(index: usize, language: Language) -> String {
    trf!(
        language,
        "{}: {} МГц",
        "{}: {} MHz",
        "{}: {} MHz",
        index + 1,
        band_label(index).unwrap()
    )
}

pub(crate) fn group_label(group: Group, language: Language) -> String {
    match group {
        Group::General => general_label(language).into(),
        Group::Band { index } => trf!(
            language,
            "Блок {}",
            "Block {}",
            "Block {}",
            band_caption(index, language)
        ),
    }
}

pub(crate) fn block_error(error: uv98_calibration::Error) -> LocalizedText {
    use uv98_calibration::Error;
    match error {
        Error::WrongLength { actual } => msgf!(
            "Ожидалось {BLOCK_LEN} байт, получено {actual}",
            "Expected {BLOCK_LEN} bytes, got {actual}",
            "{BLOCK_LEN} Bytes erwartet, {actual} erhalten",
        ),
        Error::InvalidMarker { found } => msgf!(
            "Ожидался маркер 0x{MARKER:02X}, получен 0x{found:02X}",
            "Expected marker 0x{MARKER:02X}, got 0x{found:02X}",
            "Kennung 0x{MARKER:02X} erwartet, 0x{found:02X} erhalten",
        ),
        Error::BandOutOfRange { index } => msgf!(
            "Индекс диапазона {index} вне 0..{BAND_COUNT}",
            "Band index {index} is outside 0..{BAND_COUNT}",
            "Bandindex {index} liegt außerhalb von 0..{BAND_COUNT}",
        ),
    }
}

pub(crate) fn edit_error(error: uv98_calibration_edit::Error) -> LocalizedText {
    use uv98_calibration_edit::Error;
    match error {
        Error::Source(error) => block_error(error),
        Error::BandOutOfRange { band } => {
            block_error(uv98_calibration::Error::BandOutOfRange { index: band })
        }
        Error::BatteryPointOutOfRange { index } => msgf!(
            "Индекс точки батареи {index} вне 0..15",
            "Battery point index {index} is outside 0..15",
            "Batteriepunktindex {index} liegt außerhalb von 0..15",
        ),
        Error::TablePointOutOfRange { point } => msgf!(
            "Индекс точки таблицы {point} вне 0..16",
            "Table point index {point} is outside 0..16",
            "Tabellenpunktindex {point} liegt außerhalb von 0..16",
        ),
        Error::ReadOnly { field } => LocalizedText::from_fn(|language| {
            trf!(
                language,
                "Поле «{label}» доступно только для чтения",
                "Field “{label}” is read-only",
                "Feld „{label}“ ist schreibgeschützt",
                label = field_label(descriptor(field).expect("read-only descriptor"), language),
            )
        }),
        Error::ValueOutOfRange {
            field,
            value,
            min,
            max,
        } => LocalizedText::from_fn(|language| {
            trf!(
                language,
                "Поле «{label}»: {value} вне {min}..={max}",
                "Field “{label}”: {value} is outside {min}..={max}",
                "Feld „{label}“: {value} liegt außerhalb von {min}..={max}",
                label = field_label(
                    descriptor(field).expect("range-checked descriptor"),
                    language
                ),
            )
        }),
        Error::OverlappingChanges { first, second } => msgf!(
            "Правки {first} и {second} меняют одни и те же биты",
            "Changes {first} and {second} address the same bits",
            "Änderungen {first} und {second} betreffen dieselben Bits",
        ),
        Error::TooManyChanges { count } => msgf!(
            "Слишком много правок полей: {count}",
            "Too many field changes: {count}",
            "Zu viele Feldänderungen: {count}",
        ),
    }
}

fn single_line(value: Option<&str>, language: Language) -> String {
    value
        .unwrap_or(tr!(language, "не указан", "unspecified", "nicht angegeben"))
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
    language: Language,
) -> Result<String, LocalizedText> {
    let original_doc = Document::new(original).map_err(edit_error)?;
    let working_doc = Document::new(working).map_err(edit_error)?;
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
        let detail = field_detail(d, language);
        if !notes.contains(&detail) {
            notes.push(detail);
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
    // Report lines use the same language as the export dialog.
    macro_rules! line {
        ($ru:literal, $en:literal, $de:literal $(,)?) => {
            writeln!(out, "{}", tr!(language, $ru, $en, $de)).unwrap()
        };
        ($ru:literal, $en:literal, $de:literal, $($args:tt)+) => {
            writeln!(out, "{}", trf!(language, $ru, $en, $de, $($args)+)).unwrap()
        };
    }
    line!(
        "КАЛИБРОВКИ UV98 — ОТЧЁТ О ЛОКАЛЬНОЙ КОПИИ",
        "UV98 CALIBRATION — LOCAL COPY REPORT",
        "UV98-KALIBRIERUNG — BERICHT ZUR LOKALEN KOPIE"
    );
    line!(
        "Формат: UTF-8, обычный текст. Excel не требуется.",
        "Plain UTF-8 text. Excel is optional.",
        "UTF-8-Klartext. Excel ist optional."
    );
    line!(
        "Источник: {}",
        "Source: {}",
        "Quelle: {}",
        single_line(source, language)
    );
    line!(
        "SHA256 файла-источника: {}",
        "Source file SHA256: {}",
        "SHA256 der Quelldatei: {}",
        single_line(source_sha256, language)
    );
    line!(
        "SHA256 исходного блока (4096 байт): {}",
        "Original block SHA256 (4096 bytes): {}",
        "SHA256 des Originalblocks (4096 Bytes): {}",
        crate::radio::sha256(original)
    );
    line!(
        "SHA256 рабочей копии (4096 байт): {}",
        "Working copy SHA256 (4096 bytes): {}",
        "SHA256 der Arbeitskopie (4096 Bytes): {}",
        crate::radio::sha256(working)
    );
    line!(
        "SHA файла-источника может относиться к NOR 4 МиБ; SHA блоков — только к первым 4096 байтам.",
        "The source SHA256 may cover a 4 MiB NOR dump. The block digests cover its first 4096 bytes.",
        "Der SHA256-Wert der Quelle kann einen 4-MiB-NOR-Dump umfassen. Die Blockprüfwerte umfassen dessen erste 4096 Bytes.",
    );
    line!(
        "Формат проверен по размеру и маркеру 0x9A. Контрольная сумма калибровок неизвестна.",
        "The format check covers file size and marker 0x9A. The calibration checksum is unknown.",
        "Die Formatprüfung umfasst die Dateigröße und die Kennung 0x9A. Die Kalibrierprüfsumme ist unbekannt.",
    );
    line!(
        "Исходник не изменён. Отчёт не записывает данные в рацию и не заменяет бинарный резерв.",
        "The source stays intact. Keep a separate binary backup for recovery. Exporting this report performs no radio writes.",
        "Die Quelle bleibt erhalten. Bewahren Sie für die Wiederherstellung eine separate Binärsicherung auf. Der Export schreibt keine Daten auf das Funkgerät.",
    );
    line!(
        "\nСВОДКА ИЗМЕНЕНИЙ",
        "\nCHANGE SUMMARY",
        "\nÄNDERUNGSÜBERSICHT"
    );
    line!(
        "Изменено известных полей: {}",
        "Known fields changed: {}",
        "Geänderte bekannte Felder: {}",
        changed_fields.len()
    );
    line!(
        "Изменено байт: {} из {BLOCK_LEN}",
        "Bytes changed: {} of {BLOCK_LEN}",
        "Geänderte Bytes: {} von {BLOCK_LEN}",
        changed_bytes
    );
    line!(
        "Изменено неописанных битов: {}",
        "Undocumented bits changed: {}",
        "Geänderte undokumentierte Bits: {}",
        unknown_changed_bits
    );
    line!(
        "Байтов с неописанными битами: {}. Имена и единицы этих битов неизвестны.",
        "Bytes with undocumented bits: {}. Names and units for those bits are unknown.",
        "Bytes mit undokumentierten Bits: {}. Namen und Einheiten dieser Bits sind unbekannt.",
        uncovered_bytes,
    );
    if changed_fields.is_empty() {
        line!(
            "Известные поля не изменены.",
            "Known fields are unchanged.",
            "Bekannte Felder sind unverändert."
        );
    }
    for &field in &changed_fields {
        let d = descriptor(field).unwrap();
        line!(
            "{} | 0x{:04X} | {} | {} -> {} | маска 0x{:04X}",
            "{} | 0x{:04X} | {} | {} -> {} | mask 0x{:04X}",
            "{} | 0x{:04X} | {} | {} -> {} | Maske 0x{:04X}",
            group_label(d.group, language),
            d.offset,
            field_label(d, language),
            original_doc.value(field).unwrap(),
            working_doc.value(field).unwrap(),
            d.mask,
        );
    }
    line!(
        "\nЕДИНИЦЫ И ГРАНИЦЫ ИНТЕРПРЕТАЦИИ",
        "\nUNITS AND INTERPRETATION LIMITS",
        "\nEINHEITEN UND GRENZEN DER INTERPRETATION"
    );
    line!(
        "Числа в таблицах — коды хранения, а не измеренные ватты, dBm, S-единицы или герцы.",
        "Table entries are storage codes. No conversion to measured watts, dBm, S units or hertz has been established.",
        "Die Tabelleneinträge sind Speichercodes. Für die Umrechnung in gemessene Watt, dBm, S-Stufen oder Hertz gibt es keine Bestätigung.",
    );
    line!(
        "Поправка частоты хранится как uint16 LE минус 32768. Подписи диапазонов взяты из tuning UI и не описывают чувствительность или разрешённые диапазоны RX/TX.",
        "Frequency offset code: uint16 LE minus 32768. Band labels come from the tuning UI. They give no sensitivity measurements or supported RX/TX ranges.",
        "Frequenzkorrekturcode: uint16 LE minus 32768. Die Bandbezeichnungen stammen aus der Tuningoberfläche. Sie geben weder die Empfindlichkeit noch die unterstützten RX-/TX-Bereiche an.",
    );
    line!(
        "Для батареи штатный UI показывает байт/10. Точность и физическая единица не подтверждены.",
        "The stock UI displays the battery byte divided by 10. Its precision and physical unit are unconfirmed.",
        "Die Originaloberfläche zeigt das Batteriebyte geteilt durch 10 an. Genauigkeit und physikalische Einheit sind unbestätigt.",
    );
    line!(
        "Адреса 0x0000..0x0FFF — смещения внутри блока. Для блока из начала NOR они совпадают с NOR-адресами.",
        "Addresses 0x0000..0x0FFF are block offsets. When extracting from the beginning of NOR, they match the NOR addresses.",
        "Die Adressen 0x0000..0x0FFF sind Blockpositionen. Beim Extrahieren vom Anfang des NOR-Flashs entsprechen sie den NOR-Adressen.",
    );
    line!(
        "Маска выделяет описанные биты слова little-endian. RAW показывает всё слово, включая остальные биты.",
        "The mask identifies documented bits in the little-endian word. RAW shows the entire word.",
        "Die Maske kennzeichnet die dokumentierten Bits im Little-Endian-Wort. RAW zeigt das vollständige Wort.",
    );
    line!(
        "Статус «разрешена правка» относится только к локальному редактору. Безопасность RF-настройки не подтверждена.",
        "Editable fields can be changed in the local editor. Safe RF settings remain unverified.",
        "Bearbeitbare Felder können im lokalen Editor geändert werden. Sichere HF-Einstellungen sind unbestätigt.",
    );
    line!(
        "\nТОЧНАЯ ПОБАЙТОВАЯ РАЗНИЦА",
        "\nEXACT BYTE DIFFERENCES",
        "\nEXAKTE BYTEUNTERSCHIEDE"
    );
    line!(
        "Адрес | исходный RAW -> рабочий RAW | маска изменённых неописанных битов",
        "Address | original RAW -> working RAW | mask of changed undocumented bits",
        "Adresse | Original-RAW -> Arbeits-RAW | Maske geänderter undokumentierter Bits",
    );
    if changed_bytes == 0 {
        line!(
            "Байтовых изменений нет.",
            "No bytes changed.",
            "Keine Bytes geändert."
        );
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
    line!(
        "\nВСЕ ИЗВЕСТНЫЕ ПОЛЯ",
        "\nALL KNOWN FIELDS",
        "\nALLE BEKANNTEN FELDER"
    );
    line!(
        "Адрес | поле | код исходный -> рабочий | RAW исходный -> рабочий | маска | статус | примечание",
        "Address | field | original code -> working code | original RAW -> working RAW | mask | status | note",
        "Adresse | Feld | Originalcode -> Arbeitscode | Original-RAW -> Arbeits-RAW | Maske | Status | Hinweis",
    );
    let mut previous_group = None;
    for field in fields() {
        let d = descriptor(field).unwrap();
        if previous_group != Some(d.group) {
            writeln!(out, "\n{}", group_label(d.group, language)).unwrap();
            previous_group = Some(d.group);
        }
        let evidence = match d.consumption {
            Consumption::Structural => tr!(language, "структура", "structure", "Struktur"),
            Consumption::TuningLayoutOnly => tr!(
                language,
                "только tuning layout",
                "tuning layout only",
                "nur Tuninglayout"
            ),
            Consumption::RelatedStockTraced => tr!(
                language,
                "чтение найдено в related stock",
                "read traced in related stock firmware",
                "Lesezugriff in verwandter Original-Firmware gefunden"
            ),
        };
        let detail = field_detail(d, language);
        let note = notes.iter().position(|&n| n == detail).unwrap() + 1;
        writeln!(
            out,
            "0x{:04X} | {} | {} -> {} | 0x{:04X} -> 0x{:04X} | 0x{:04X} | {}, {} | [{}]",
            d.offset,
            field_label(d, language),
            original_doc.value(field).unwrap(),
            working_doc.value(field).unwrap(),
            original_doc.raw_word(field).unwrap(),
            working_doc.raw_word(field).unwrap(),
            d.mask,
            if d.editable {
                tr!(language, "разрешена правка", "editable", "bearbeitbar")
            } else {
                tr!(language, "только чтение", "read-only", "schreibgeschützt")
            },
            evidence,
            note
        )
        .unwrap();
    }
    line!(
        "\nПОЯСНЕНИЯ ИЗ ОПИСАНИЯ ФОРМАТА",
        "\nFORMAT NOTES",
        "\nHINWEISE ZUM FORMAT"
    );
    for (index, detail) in notes.iter().enumerate() {
        writeln!(out, "[{}] {detail}", index + 1).unwrap();
    }
    line!(
        "\nRF-калибровка и аппаратная совместимость этим отчётом не подтверждаются.",
        "\nRF calibration and hardware compatibility remain unverified.",
        "\nHF-Kalibrierung und Hardwarekompatibilität bleiben unbestätigt.",
    );
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
            Language::Russian,
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
        let text = render(ORIGINAL, &work, None, None, Language::Russian).unwrap();
        assert!(text.contains("Изменено известных полей: 0"));
        assert!(text.contains("Изменено байт: 1 из 4096"));
        assert!(text.contains("Изменено неописанных битов: 1"));
        assert!(text.contains(&format!(
            "0x0FFF | 0x{:02X} -> 0x{:02X} | 0x80",
            ORIGINAL[4095], work[4095]
        )));
        assert!(text.contains("Источник: не указан"));
        assert!(render(&[0; 3], &work, None, None, Language::Russian).is_err());
    }

    #[test]
    fn unchanged_report_and_source_control_characters_are_unambiguous() {
        let text = render(
            ORIGINAL,
            ORIGINAL,
            Some("a\nSHA256: fake\t.bin"),
            None,
            Language::Russian,
        )
        .unwrap();
        assert!(text.contains("Источник: a\\nSHA256: fake\\t.bin\n"));
        assert!(text.contains("Байтовых изменений нет."));
        assert!(text.contains("Изменено известных полей: 0"));
        assert!(text.contains("Блок 15:"));
        assert!(text.contains("только чтение"));
        for (language, hidden, untraced) in [
            (
                Language::English,
                "hides this point",
                "use remains unconfirmed",
            ),
            (
                Language::German,
                "blendet diesen Punkt aus",
                "Datensatz 15 ist daher unbestätigt",
            ),
        ] {
            let localized = render(ORIGINAL, ORIGINAL, Some("source.bin"), None, language).unwrap();
            assert!(localized.contains(hidden));
            assert!(localized.contains(untraced));
            assert!(!localized
                .chars()
                .any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)));
        }
    }
}
