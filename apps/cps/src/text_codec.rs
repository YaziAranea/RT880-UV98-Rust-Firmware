use encoding_rs::GBK;
use uv98_codeplug::channels::Tone;

pub fn decode_name(raw: &[u8]) -> (String, bool) {
    let end = raw
        .iter()
        .position(|byte| *byte == 0xFF)
        .unwrap_or(raw.len());
    let (text, errors) = GBK.decode_without_bom_handling(&raw[..end]);
    (text.into_owned(), errors)
}

pub fn encode_name(text: &str) -> Result<[u8; 16], String> {
    if text.chars().any(char::is_control) {
        return Err("Имя не должно содержать управляющие символы".into());
    }
    let (encoded, _, errors) = GBK.encode(text);
    if errors {
        return Err("В имени есть символы, которые не поддерживает кодировка CPS (GBK)".into());
    }
    if encoded.len() > 16 {
        return Err(format!("Имя занимает {} байт; допустимо 16", encoded.len()));
    }
    let mut raw = [0xFF; 16];
    raw[..encoded.len()].copy_from_slice(&encoded);
    Ok(raw)
}

pub fn format_frequency(code: u32) -> String {
    format!("{}.{:05}", code / 100_000, code % 100_000)
}

pub fn parse_frequency(text: &str) -> Result<u32, String> {
    let normalized = text.trim().replace(',', ".");
    let mut parts = normalized.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some()
        || whole.is_empty()
        || fraction.len() > 5
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || !fraction.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("Частота: МГц, до пяти цифр после запятой; например 145.50000".into());
    }
    let whole: u64 = whole.parse().map_err(|_| "Частота слишком велика")?;
    let fraction: u64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().map_err(|_| "Неверная частота")?
    };
    let result = whole
        .checked_mul(100_000)
        .and_then(|v| v.checked_add(fraction * 10u64.pow(5 - parts_fraction_length(&normalized))))
        .ok_or("Частота слишком велика")?;
    u32::try_from(result).map_err(|_| "Частота не помещается в формат CPS".into())
}

fn parts_fraction_length(text: &str) -> u32 {
    text.split_once('.')
        .map_or(0, |(_, fraction)| fraction.len() as u32)
}

pub fn format_tone(raw: u16) -> String {
    match Tone::decode(raw) {
        Some(Tone::None) => "Нет".into(),
        Some(Tone::Ctcss { tenths_hz }) => format!("{}.{:01}", tenths_hz / 10, tenths_hz % 10),
        Some(Tone::Dcs { code, inverted }) => {
            format!("D{code:03o}{}", if inverted { 'I' } else { 'N' })
        }
        None => format!("0x{raw:04X}"),
    }
}

pub fn parse_tone(text: &str) -> Result<Tone, String> {
    let text = text.trim().to_uppercase().replace(',', ".");
    if text.is_empty() || text == "НЕТ" || text == "NONE" {
        return Ok(Tone::None);
    }
    let tone = if let Some(dcs) = text.strip_prefix('D') {
        if dcs.len() != 4 || !dcs.is_ascii() {
            return Err("DCS: например D023N или D023I".into());
        }
        if !dcs[..3].bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
            return Err("DCS: три восьмеричные цифры".into());
        }
        let inverted = match &dcs[3..] {
            "N" => false,
            "I" => true,
            _ => return Err("DCS: последняя буква N или I".into()),
        };
        let code = u16::from_str_radix(&dcs[..3], 8).map_err(|_| "DCS: три восьмеричные цифры")?;
        Tone::Dcs { code, inverted }
    } else {
        let mut parts = text.split('.');
        let whole = parts.next().unwrap_or("");
        let fraction = parts.next().unwrap_or("0");
        if parts.next().is_some()
            || whole.is_empty()
            || fraction.len() != 1
            || !whole.bytes().all(|c| c.is_ascii_digit())
            || !fraction.bytes().all(|c| c.is_ascii_digit())
        {
            return Err("Тон: Нет, CTCSS 67.0 или DCS D023N/D023I".into());
        }
        let whole: u16 = whole.parse().map_err(|_| "Неверный CTCSS")?;
        let fraction: u16 = fraction.parse().map_err(|_| "Неверный CTCSS")?;
        let tenths_hz = whole
            .checked_mul(10)
            .and_then(|v| v.checked_add(fraction))
            .ok_or("CTCSS слишком велик")?;
        Tone::Ctcss { tenths_hz }
    };
    tone.encode().map_err(|error| error.to_string())?;
    Ok(tone)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frequency_is_decimal_and_lossless_without_float_rounding() {
        for code in [0, 1, 14_550_000, 43_350_001, u32::MAX] {
            assert_eq!(parse_frequency(&format_frequency(code)).unwrap(), code);
        }
        assert_eq!(parse_frequency("145,50001").unwrap(), 14_550_001);
        for invalid in ["-1", "NaN", "1.000001", "1..2", "42949.67296", "1e3", ""] {
            assert!(parse_frequency(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn names_never_truncate_multibyte_or_replace_unsupported_characters() {
        let raw = encode_name("Тест-1").unwrap();
        assert_eq!(decode_name(&raw), ("Тест-1".into(), false));
        assert!(encode_name("😀").is_err());
        assert!(encode_name("12345678901234567").is_err());
        assert!(encode_name("x\0y").is_err());
        assert_eq!(encode_name("").unwrap(), [0xFF; 16]);
    }
    #[test]
    fn gbk_name_does_not_sniff_utf8_bom_or_drop_source_bytes() {
        let raw = [0xEF, 0xBB, 0xBF, 0xAA, 0xFF];
        let (text, errors) = decode_name(&raw);
        assert!(!errors);
        assert_eq!(&encode_name(&text).unwrap()[..4], &raw[..4]);
    }
    #[test]
    fn tone_text_roundtrip_keeps_ctcss_and_dcs_polarity() {
        for tone in [
            Tone::None,
            Tone::Ctcss { tenths_hz: 670 },
            Tone::Dcs {
                code: 0o023,
                inverted: false,
            },
            Tone::Dcs {
                code: 0o754,
                inverted: true,
            },
        ] {
            let raw = tone.encode().unwrap();
            assert_eq!(parse_tone(&format_tone(raw)).unwrap(), tone);
        }
        assert!(parse_tone("D999N").is_err());
        assert!(parse_tone("D+23N").is_err());
        assert!(parse_tone("0xFFFF").is_err());
    }
}
