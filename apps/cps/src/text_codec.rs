use crate::i18n::LocalizedText;
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

pub fn encode_name(text: &str) -> Result<[u8; 16], LocalizedText> {
    if text.chars().any(char::is_control) {
        return Err(msg!(
            "Имя не должно содержать управляющие символы",
            "Name must not contain control characters",
            "Der Name darf keine Steuerzeichen enthalten"
        )
        .into());
    }
    let (encoded, _, errors) = GBK.encode(text);
    if errors {
        return Err(msg!(
            "В имени есть символы, которые не поддерживает кодировка CPS (GBK)",
            "Name contains characters unsupported by CPS encoding (GBK)",
            "Der Name enthält Zeichen, die von der CPS-Kodierung (GBK) nicht unterstützt werden"
        )
        .into());
    }
    if encoded.len() > 16 {
        return Err(msgf!(
            "Имя занимает {} байт; допустимо 16",
            "Name uses {} bytes; maximum 16",
            "Name belegt {} Bytes; höchstens 16",
            encoded.len()
        ));
    }
    let mut raw = [0xFF; 16];
    raw[..encoded.len()].copy_from_slice(&encoded);
    Ok(raw)
}

pub fn format_frequency(code: u32) -> String {
    format!("{}.{:05}", code / 100_000, code % 100_000)
}

pub fn parse_frequency(text: &str) -> Result<u32, LocalizedText> {
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
        return Err(msg!(
            "Частота: МГц, до пяти цифр после запятой; например 145.50000",
            "Frequency: MHz, up to five decimal places; for example 145.50000",
            "Frequenz: MHz, bis fünf Nachkommastellen; zum Beispiel 145.50000"
        )
        .into());
    }
    let whole: u64 = whole.parse().map_err(|_| {
        msg!(
            "Частота слишком велика",
            "Frequency is too large",
            "Frequenz ist zu groß"
        )
    })?;
    let fraction: u64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().map_err(|_| {
            msg!(
                "Неверная частота",
                "Invalid frequency",
                "Ungültige Frequenz"
            )
        })?
    };
    let result = whole
        .checked_mul(100_000)
        .and_then(|v| v.checked_add(fraction * 10u64.pow(5 - parts_fraction_length(&normalized))))
        .ok_or(msg!(
            "Частота слишком велика",
            "Frequency is too large",
            "Frequenz ist zu groß"
        ))?;
    u32::try_from(result).map_err(|_| {
        msg!(
            "Частота не помещается в формат CPS",
            "Frequency does not fit the CPS format",
            "Frequenz passt nicht in das CPS-Format"
        )
        .into()
    })
}

fn parts_fraction_length(text: &str) -> u32 {
    text.split_once('.')
        .map_or(0, |(_, fraction)| fraction.len() as u32)
}

pub fn format_tone(raw: u16) -> String {
    match Tone::decode(raw) {
        Some(Tone::None) => String::new(),
        Some(Tone::Ctcss { tenths_hz }) => format!("{}.{:01}", tenths_hz / 10, tenths_hz % 10),
        Some(Tone::Dcs { code, inverted }) => {
            format!("D{code:03o}{}", if inverted { 'I' } else { 'N' })
        }
        None => format!("0x{raw:04X}"),
    }
}

pub fn parse_tone(text: &str) -> Result<Tone, LocalizedText> {
    let text = text.trim().to_uppercase().replace(',', ".");
    if text.is_empty() || text == "НЕТ" || text == "NONE" || text == "AUS" || text == "KEIN" {
        return Ok(Tone::None);
    }
    let tone = if let Some(dcs) = text.strip_prefix('D') {
        if dcs.len() != 4 || !dcs.is_ascii() {
            return Err(msg!(
                "DCS: например D023N или D023I",
                "DCS: for example D023N or D023I",
                "DCS: zum Beispiel D023N oder D023I"
            )
            .into());
        }
        if !dcs[..3].bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
            return Err(msg!(
                "DCS: три восьмеричные цифры",
                "DCS: three octal digits",
                "DCS: drei Oktalziffern"
            )
            .into());
        }
        let inverted = match &dcs[3..] {
            "N" => false,
            "I" => true,
            _ => {
                return Err(msg!(
                    "DCS: последняя буква N или I",
                    "DCS: last letter must be N or I",
                    "DCS: letzter Buchstabe muss N oder I sein"
                )
                .into())
            }
        };
        let code = u16::from_str_radix(&dcs[..3], 8).map_err(|_| {
            msg!(
                "DCS: три восьмеричные цифры",
                "DCS: three octal digits",
                "DCS: drei Oktalziffern"
            )
        })?;
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
            return Err(msg!(
                "Тон: Нет, CTCSS 67.0 или DCS D023N/D023I",
                "Tone: blank, CTCSS 67.0 or DCS D023N/D023I",
                "Ton: leer, CTCSS 67.0 oder DCS D023N/D023I"
            )
            .into());
        }
        let whole: u16 = whole
            .parse()
            .map_err(|_| msg!("Неверный CTCSS", "Invalid CTCSS", "Ungültiger CTCSS"))?;
        let fraction: u16 = fraction
            .parse()
            .map_err(|_| msg!("Неверный CTCSS", "Invalid CTCSS", "Ungültiger CTCSS"))?;
        let tenths_hz = whole
            .checked_mul(10)
            .and_then(|v| v.checked_add(fraction))
            .ok_or(msg!(
                "CTCSS слишком велик",
                "CTCSS is too large",
                "CTCSS ist zu groß"
            ))?;
        Tone::Ctcss { tenths_hz }
    };
    tone.encode().map_err(|error| match error {
        uv98_codeplug::channels::ChannelError::CtcssOutOfRange { tenths_hz } => msgf!(
            "Код CTCSS {tenths_hz} превышает предел 4095 (409,5 Гц)",
            "CTCSS code {tenths_hz} exceeds 4095 (409.5 Hz)",
            "CTCSS-Code {tenths_hz} überschreitet 4095 (409,5 Hz)"
        ),
        other => LocalizedText::from(other.to_string()),
    })?;
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
