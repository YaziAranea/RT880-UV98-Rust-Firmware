//! Presentation adapters over the confirmed codeplug metadata; codes stay intact.
use crate::i18n::Language;
use uv98_codeplug::settings::Field;

pub fn label(field: Field, language: Language) -> &'static str {
    if language == Language::Russian {
        return field.label_ru();
    }
    let (en, de) = match field {
        Field::VoicePrompt => ("Voice prompts", "Sprachansagen"),
        Field::KeyBeep => ("Key beep", "Tastenton"),
        Field::KeyLock => ("Keypad lock", "Tastensperre"),
        Field::AutoLockDelay => ("Keypad lock timer", "Tastensperr-Timer"),
        Field::KeypadLight => ("Keypad light", "Tastenbeleuchtung"),
        Field::BacklightLevel => ("Display brightness", "Displayhelligkeit"),
        Field::BacklightDelay => ("Backlight timer", "Beleuchtungstimer"),
        Field::SaverMode => ("Power saving", "Energiesparen"),
        Field::SaverStart => ("Power saving delay", "Energiesparverzögerung"),
        Field::MenuExitDelay => ("Menu exit timer", "Menüende-Timer"),
        Field::TuningStep => ("Frequency step", "Frequenzschritt"),
        Field::ScanDirection => ("Scan direction", "Suchlaufrichtung"),
        Field::ScanMode => ("Scan mode", "Suchlaufmodus"),
        Field::ScanReturn => (
            "Return channel after scanning",
            "Rückkehrkanal nach Suchlauf",
        ),
        Field::ScanDwell => ("Scan stop timer", "Suchlauf-Stopp-Timer"),
        Field::Squelch => ("Squelch", "Rauschsperre"),
        Field::Vox => ("VOX", "VOX"),
        Field::VoxThreshold => ("VOX threshold", "VOX-Schwelle"),
        Field::VoxDelay => ("VOX delay", "VOX-Verzögerung"),
        Field::RxSpeaker => (
            "FM speaker gain (code)",
            "FM-Lautsprecherverstärkung (Code)",
        ),
        Field::RxDac => ("FM DAC gain (code)", "FM-DAC-Verstärkung (Code)"),
        Field::RxSpeakerAm => (
            "AM speaker gain (code)",
            "AM-Lautsprecherverstärkung (Code)",
        ),
        Field::RxDacAm => ("AM DAC gain (code)", "AM-DAC-Verstärkung (Code)"),
        Field::FmStandby => ("FM broadcast standby", "UKW-Radio-Bereitschaft"),
    };
    if language == Language::English {
        en
    } else {
        de
    }
}

pub fn group(field: Field, language: Language) -> &'static str {
    if language == Language::Russian {
        return field.group_ru();
    }
    let (en, de) = match field {
        Field::VoicePrompt
        | Field::KeyBeep
        | Field::RxSpeaker
        | Field::RxDac
        | Field::RxSpeakerAm
        | Field::RxDacAm => ("Sound", "Ton"),
        Field::KeyLock
        | Field::AutoLockDelay
        | Field::KeypadLight
        | Field::BacklightLevel
        | Field::BacklightDelay
        | Field::MenuExitDelay => ("Display and keypad", "Display und Tastatur"),
        Field::SaverMode | Field::SaverStart => ("Power saving", "Energiesparen"),
        Field::Squelch
        | Field::TuningStep
        | Field::ScanDirection
        | Field::ScanMode
        | Field::ScanReturn
        | Field::ScanDwell => ("Receive and scan", "Empfang und Suchlauf"),
        Field::Vox | Field::VoxThreshold | Field::VoxDelay => ("VOX", "VOX"),
        Field::FmStandby => ("FM broadcast", "UKW-Radio"),
    };
    if language == Language::English {
        en
    } else {
        de
    }
}

pub fn choice(field: Field, code: u8, language: Language) -> String {
    if code > field.max_code() {
        return trf!(
            language,
            "Неизвестно (0x{code:02X})",
            "Unknown (0x{code:02X})",
            "Unbekannt (0x{code:02X})"
        );
    }
    let Some(caption) = field.choice_label(code) else {
        return code.to_string();
    };
    if language == Language::Russian {
        return caption.to_owned();
    }
    let translated = match caption {
        "Выключено" => tr!(language, "Выключено", "Off", "Aus"),
        "Включено" => tr!(language, "Включено", "On", "Ein"),
        "Разблокирована" => tr!(language, "Разблокирована", "Unlocked", "Entsperrt"),
        "Заблокирована" => tr!(language, "Заблокирована", "Locked", "Gesperrt"),
        "Вверх" => tr!(language, "Вверх", "Up", "Aufwärts"),
        "Вниз" => tr!(language, "Вниз", "Down", "Abwärts"),
        "По несущей (CO)" => {
            tr!(language, "По несущей (CO)", "Carrier (CO)", "Träger (CO)")
        }
        "По времени (TO)" => tr!(language, "По времени (TO)", "Time (TO)", "Zeit (TO)"),
        "Поиск (SE)" => tr!(language, "Поиск (SE)", "Search (SE)", "Suche (SE)"),
        "Исходный канал" => tr!(
            language,
            "Исходный канал",
            "Original channel",
            "Ausgangskanal"
        ),
        "Текущий канал" => tr!(
            language,
            "Текущий канал",
            "Current channel",
            "Aktueller Kanal"
        ),
        other => other,
    };
    let text = translated
        .replace(" кГц", " kHz")
        .replace(" МГц", " MHz")
        .replace(" с", " s");
    if language == Language::English {
        text.replace(',', ".")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_confirmed_options_are_translated_without_changing_codes_or_units() {
        for language in [Language::English, Language::German] {
            for field in Field::ALL {
                assert!(!label(field, language).is_empty());
                for code in 0..=field.max_code() {
                    assert!(
                        !choice(field, code, language)
                            .chars()
                            .any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)),
                        "{field:?}/{code}"
                    );
                }
            }
            assert!(choice(Field::KeyBeep, 0xfe, language).contains("0xFE"));
            assert_eq!(choice(Field::AutoLockDelay, 1, language), "5");
            assert_eq!(choice(Field::SaverStart, 0, language), "0");
        }
        assert_eq!(choice(Field::TuningStep, 0, Language::English), "0.25 kHz");
        assert_eq!(choice(Field::VoxDelay, 1, Language::German), "0,5 s");
    }
}
