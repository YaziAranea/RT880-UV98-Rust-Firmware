//! Bounded keypad editor for volatile text and four-digit hexadecimal fields.

use core::fmt::{self, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextValue<const N: usize> {
    chars: [char; N],
    len: usize,
}

impl<const N: usize> TextValue<N> {
    pub const fn empty() -> Self {
        Self {
            chars: [' '; N],
            len: 0,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    fn append(&mut self, character: char) -> bool {
        if self.len == N {
            return false;
        }
        self.chars[self.len] = character;
        self.len += 1;
        true
    }

    fn delete(&mut self) {
        if self.len != 0 {
            self.len -= 1;
            self.chars[self.len] = ' ';
        }
    }
}

impl<const N: usize> fmt::Display for TextValue<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &character in &self.chars[..self.len] {
            f.write_char(character)?;
        }
        Ok(())
    }
}

impl TextValue<4> {
    pub fn from_hex(id: Option<u16>) -> Self {
        let mut text = Self::empty();
        if let Some(id) = id {
            for shift in [12, 8, 4, 0] {
                let nibble = ((id >> shift) & 0x0f) as usize;
                text.append(char::from(b"0123456789ABCDEF"[nibble]));
            }
        }
        text
    }

    pub fn to_hex(self) -> Option<u16> {
        if self.len != 4 {
            return None;
        }
        self.chars.into_iter().try_fold(0, |value, ch| {
            Some((value << 4) | u16::try_from(ch.to_digit(16)?).ok()?)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    English,
    Russian,
    Hex,
}

impl InputMode {
    const fn cycle_language(self) -> Self {
        match self {
            Self::English => Self::Russian,
            Self::Russian => Self::English,
            Self::Hex => Self::Hex,
        }
    }

    fn choices(self, digit: u8) -> &'static str {
        let alphabet = match self {
            Self::English => &[
                " 0", ".,-_/1", "ABC2", "DEF3", "GHI4", "JKL5", "MNO6", "PQRS7", "TUV8", "WXYZ9",
            ],
            Self::Russian => &[
                " 0",
                ".,-_/1",
                "АБВГ2",
                "ДЕЁЖЗ3",
                "ИЙКЛ4",
                "МНОП5",
                "РСТУ6",
                "ФХЦЧ7",
                "ШЩЪЫ8",
                "ЬЭЮЯ9",
            ],
            Self::Hex => return "",
        };
        alphabet.get(usize::from(digit)).copied().unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Editor<const N: usize> {
    mode: InputMode,
    value: TextValue<N>,
    pending: Option<(u8, u8)>,
}

impl<const N: usize> Editor<N> {
    pub const fn new(mode: InputMode, value: TextValue<N>) -> Self {
        Self {
            mode,
            value,
            pending: None,
        }
    }

    pub const fn mode(&self) -> InputMode {
        self.mode
    }

    pub const fn value(&self) -> TextValue<N> {
        self.value
    }

    pub fn commit_character(&mut self) {
        self.pending = None;
    }

    pub fn cycle_language(&mut self) {
        self.commit_character();
        self.mode = self.mode.cycle_language();
    }

    pub fn delete(&mut self) {
        self.commit_character();
        self.value.delete();
    }

    pub fn digit(&mut self, digit: u8, held: bool) -> bool {
        if digit > 9 {
            return false;
        }
        if self.mode == InputMode::Hex {
            let character = if held && (1..=6).contains(&digit) {
                char::from(b'A' + digit - 1)
            } else {
                char::from(b'0' + digit)
            };
            return self.value.append(character);
        }
        if held {
            self.commit_character();
            return self.value.append(char::from(b'0' + digit));
        }
        let choices = self.mode.choices(digit);
        if let Some((key, index)) = self.pending.filter(|(key, _)| *key == digit) {
            let next = (index + 1) % choices.chars().count() as u8;
            self.value.chars[self.value.len - 1] = choices.chars().nth(usize::from(next)).unwrap();
            self.pending = Some((key, next));
            return true;
        }
        if self.value.append(choices.chars().next().unwrap()) {
            self.pending = Some((digit, 0));
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::string::ToString;

    #[test]
    fn repeated_keys_cycle_then_commit_and_language_switch_keeps_text() {
        let mut editor = Editor::<3>::new(InputMode::English, TextValue::empty());
        assert!(editor.digit(2, false));
        assert!(editor.digit(2, false));
        assert_eq!(editor.value.to_string(), "B");
        editor.commit_character();
        assert!(editor.digit(2, false));
        editor.cycle_language();
        assert!(editor.digit(2, false));
        assert!(editor.digit(2, false));
        assert_eq!(editor.value.to_string(), "BAБ");
        assert!(!editor.digit(3, false));
        editor.delete();
        assert!(editor.digit(3, true));
        assert_eq!(editor.value.to_string(), "BA3");
    }

    #[test]
    fn hex_accepts_direct_digits_and_held_a_through_f() {
        let mut editor = Editor::new(InputMode::Hex, TextValue::<4>::empty());
        for (digit, held) in [(0, false), (1, true), (6, true), (3, false)] {
            assert!(editor.digit(digit, held));
        }
        assert_eq!(editor.value.to_string(), "0AF3");
        assert_eq!(editor.value.to_hex(), Some(0x0af3));
        assert_eq!(TextValue::from_hex(Some(0x0af3)), editor.value);
        assert!(!editor.digit(4, false));
        editor.delete();
        assert_eq!(editor.value.to_hex(), None);
    }
}
