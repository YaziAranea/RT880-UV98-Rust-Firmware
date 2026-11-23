//! Immutable, compact UI catalogs with bounded decoding. Firmware includes one
//! catalog. Host previews can select a catalog per view. User channel and zone
//! names have no message markers and are preserved verbatim by the renderer.

use core::fmt::Write;

pub use uv98_handheld_ui_state::Language;

#[cfg(any(
    all(feature = "locale-ru", feature = "locale-en"),
    all(feature = "locale-ru", feature = "locale-de"),
    all(feature = "locale-en", feature = "locale-de"),
    all(
        feature = "locale-all",
        any(feature = "locale-ru", feature = "locale-en", feature = "locale-de")
    )
))]
compile_error!("Choose only one handheld UI locale feature");

#[cfg(all(feature = "locale-all", target_os = "none"))]
compile_error!("locale-all is for host previews only");

const CAPACITY: usize = 256;

struct Catalog {
    bytes: &'static [u8],
    count: usize,
    glyphs: usize,
    dictionary: usize,
    offsets: usize,
    pool: usize,
}

impl Catalog {
    const fn new(bytes: &'static [u8]) -> Self {
        let count = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        let glyphs = bytes[2] as usize;
        let pairs = bytes[3] as usize;
        let dictionary = 4 + glyphs * 2;
        let offsets = dictionary + pairs * 2;
        Self {
            bytes,
            count,
            glyphs,
            dictionary,
            offsets,
            pool: offsets + (count + 1) * 2,
        }
    }

    fn word(&self, offset: usize) -> usize {
        u16::from_le_bytes([self.bytes[offset], self.bytes[offset + 1]]) as usize
    }
}

#[cfg(any(test, feature = "locale-all"))]
fn catalog(language: Language) -> &'static Catalog {
    const RUSSIAN: Catalog = Catalog::new(include_bytes!("../locales/messages-ru.bin"));
    const ENGLISH: Catalog = Catalog::new(include_bytes!("../locales/messages-en.bin"));
    const GERMAN: Catalog = Catalog::new(include_bytes!("../locales/messages-de.bin"));
    match language {
        Language::Russian => &RUSSIAN,
        Language::English => &ENGLISH,
        Language::German => &GERMAN,
    }
}

#[cfg(not(any(test, feature = "locale-all")))]
fn catalog(_: Language) -> &'static Catalog {
    #[cfg(feature = "locale-en")]
    const BYTES: &[u8] = include_bytes!("../locales/messages-en.bin");
    #[cfg(all(not(feature = "locale-en"), feature = "locale-de"))]
    const BYTES: &[u8] = include_bytes!("../locales/messages-de.bin");
    #[cfg(not(any(feature = "locale-en", feature = "locale-de")))]
    const BYTES: &[u8] = include_bytes!("../locales/messages-ru.bin");
    const CATALOG: Catalog = Catalog::new(BYTES);
    &CATALOG
}

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

    fn entry(&mut self, id: usize, catalog: &Catalog) {
        if id >= catalog.count {
            let _ = self.write_char('?');
            return;
        }
        let offset = catalog.offsets + id * 2;
        let start = catalog.pool + catalog.word(offset);
        let end = catalog.pool + catalog.word(offset + 2);
        let mut stack = [0u8; 24];
        for &token in &catalog.bytes[start..end] {
            let mut used = 1;
            stack[0] = token;
            while used != 0 {
                used -= 1;
                let value = usize::from(stack[used]);
                if value < catalog.glyphs {
                    if let Some(character) = char::from_u32(catalog.word(4 + value * 2) as u32) {
                        let _ = self.write_char(character);
                    }
                } else {
                    let pair = catalog.dictionary + (value - catalog.glyphs) * 2;
                    // pack_ui_locales.py verifies maximum nesting. The bound
                    // also makes invalid catalogs fail closed instead of panic.
                    if used + 2 > stack.len() || pair + 1 >= catalog.offsets {
                        let _ = self.write_char('?');
                        return;
                    }
                    stack[used] = catalog.bytes[pair + 1];
                    stack[used + 1] = catalog.bytes[pair];
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

/// Expand fixed-width message IDs, including IDs embedded in formatted values.
/// All other UTF-8 is copied unchanged. Numbers and saved names are never keys.
pub fn resolve(value: &str, language: Language) -> Message {
    let catalog = catalog(language);
    let mut message = Message {
        bytes: [0; CAPACITY],
        len: 0,
    };
    let mut remaining = value;
    while let Some((literal, tail)) = remaining.split_once('\x1f') {
        let _ = message.write_str(literal);
        if tail.len() >= 3 && tail.is_char_boundary(3) {
            if let Ok(id) = usize::from_str_radix(&tail[..3], 16) {
                message.entry(id, catalog);
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
        for language in [Language::Russian, Language::English, Language::German] {
            assert_eq!(source.lines().skip(1).count(), catalog(language).count);
        }
        for (id, line) in source.lines().skip(1).enumerate() {
            let mut translations = line.split('\t');
            for language in [Language::Russian, Language::English, Language::German] {
                let expected = translations.next().unwrap_or_default();
                let mut actual = Message {
                    bytes: [0; CAPACITY],
                    len: 0,
                };
                actual.entry(id, catalog(language));
                assert_eq!(actual.as_str(), expected, "message {id}, {language:?}");
                if language != Language::Russian {
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
