//! Runtime UI language with an immutable, compact catalog and bounded decoding.
//! The selected language belongs to View, never global state. User channel/zone
//! names have no message markers and are preserved verbatim by the renderer.

use core::fmt::Write;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Language {
    #[default]
    Russian = 0,
    English = 1,
    German = 2,
}

impl Language {
    pub const fn from_index(index: u8) -> Self {
        match index {
            1 => Self::English,
            2 => Self::German,
            _ => Self::Russian,
        }
    }
}

const CATALOG: &[u8] = include_bytes!("../locales/messages.bin");
const CAPACITY: usize = 256;
const GLYPHS: usize = CATALOG[2] as usize;
const PAIRS: usize = CATALOG[3] as usize;
const DICTIONARY: usize = 4 + GLYPHS * 2;
const OFFSETS: usize = DICTIONARY + PAIRS * 2;
const COUNT: usize = u16::from_le_bytes([CATALOG[0], CATALOG[1]]) as usize;
const POOL: usize = OFFSETS + (COUNT * 3 + 1) * 2;

/// A temporary UTF-8 message. All writes are bounded; no allocation or storage I/O.
pub struct Message {
    bytes: [u8; CAPACITY],
    len: usize,
}

impl Message {
    pub fn as_str(&self) -> &str {
        // Construction only copies complete UTF-8 strings/chars.
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("?")
    }

    fn entry(&mut self, id: usize, language: Language) {
        if id >= COUNT {
            let _ = self.write_char('?');
            return;
        }
        let offset = OFFSETS + (id * 3 + language as usize) * 2;
        let start = POOL + word(offset);
        let end = POOL + word(offset + 2);
        let mut stack = [0u8; 24];
        for &token in &CATALOG[start..end] {
            let mut used = 1;
            stack[0] = token;
            while used != 0 {
                used -= 1;
                let value = usize::from(stack[used]);
                if value < GLYPHS {
                    if let Some(character) = char::from_u32(word(4 + value * 2) as u32) {
                        let _ = self.write_char(character);
                    }
                } else {
                    let pair = DICTIONARY + (value - GLYPHS) * 2;
                    // pack_ui_locales.py verifies maximum nesting. The bound
                    // also makes invalid catalogs fail closed instead of panic.
                    if used + 2 > stack.len() || pair + 1 >= OFFSETS {
                        let _ = self.write_char('?');
                        return;
                    }
                    stack[used] = CATALOG[pair + 1];
                    stack[used + 1] = CATALOG[pair];
                    used += 2;
                }
            }
        }
    }
}

impl Write for Message {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        if self.len + value.len() > self.bytes.len() {
            return Err(core::fmt::Error);
        }
        self.bytes[self.len..self.len + value.len()].copy_from_slice(value.as_bytes());
        self.len += value.len();
        Ok(())
    }
}

fn word(offset: usize) -> usize {
    u16::from_le_bytes([CATALOG[offset], CATALOG[offset + 1]]) as usize
}

/// Expand fixed-width message IDs, including IDs embedded in formatted values.
/// All other UTF-8 is copied unchanged. Numbers and saved names are never keys.
pub fn resolve(value: &str, language: Language) -> Message {
    let mut message = Message {
        bytes: [0; CAPACITY],
        len: 0,
    };
    let mut remaining = value;
    while let Some((literal, tail)) = remaining.split_once('\x1f') {
        let _ = message.write_str(literal);
        if tail.len() >= 3 && tail.is_char_boundary(3) {
            if let Ok(id) = usize::from_str_radix(&tail[..3], 16) {
                message.entry(id, language);
                remaining = &tail[3..];
                continue;
            }
        }
        let _ = message.write_char('?');
        remaining = tail;
    }
    let _ = message.write_str(remaining);
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_catalog_matches_every_reviewed_translation() {
        let source = include_str!("../locales/messages.tsv");
        assert_eq!(source.lines().skip(1).count(), COUNT);
        for (id, line) in source.lines().skip(1).enumerate() {
            let mut translations = line.split('\t');
            for language in [Language::Russian, Language::English, Language::German] {
                let expected = translations.next().unwrap();
                let mut actual = Message {
                    bytes: [0; CAPACITY],
                    len: 0,
                };
                actual.entry(id, language);
                assert_eq!(actual.as_str(), expected, "message {id}, {language:?}");
                if language != Language::Russian && expected != "РУССКИЙ" {
                    assert!(
                        !expected
                            .chars()
                            .any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)),
                        "untranslated message {id}, {language:?}"
                    );
                }
            }
            assert!(translations.next().is_none());
        }
    }

    #[test]
    fn saved_names_and_unmarked_technical_values_are_not_translated() {
        for language in [Language::Russian, Language::English, Language::German] {
            for text in [
                "МОСКВА",
                "RELAIS 70CM",
                "432.97500",
                "CTCSS 77.0",
                "USB",
                "DEUTSCH",
            ] {
                assert_eq!(resolve(text, language).as_str(), text);
            }
        }
    }

    #[test]
    fn malformed_marker_is_bounded_and_has_visible_fallback() {
        assert_eq!(resolve("LEFT\x1f", Language::English).as_str(), "LEFT?");
        assert_eq!(resolve("\x1fFFF END", Language::English).as_str(), "? END");
        assert_eq!(resolve("\x1fЖЖ", Language::German).as_str(), "?ЖЖ");
    }
}
