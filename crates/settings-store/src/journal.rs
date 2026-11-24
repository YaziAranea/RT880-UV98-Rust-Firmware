use crate::{Error, Settings};

pub const PAGE_SIZE: usize = 2048;
pub const WORDS: usize = 8;
const SLOTS: usize = PAGE_SIZE / (WORDS * size_of::<u32>());
const ERASED: u32 = u32::MAX;
const LEGACY_HEADER: u32 = 0x5556_0100;
const HEADER: u32 = 0x5556_0200; // "UV", schema 2, flags in the low byte.
const COMMIT: u32 = 0x51A7_6E39;
const EXT_HEADER: u32 = 0x5556_0300;
const EXT_COMMIT: u32 = 0x51A7_6E3A;
const EXT_POSITIONS: u32 = 1;
const EXT_VOICE: u32 = 2;
const HAS_POSITIONS: u32 = 1 << 8;
const VOICE_DIGIT_SHIFT: u32 = 9;
const VOICE_DISABLED: u8 = 10;
const COMMIT_INDEX: usize = WORDS - 1;
type Words = [u32; WORDS];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Page {
    First,
    Second,
}

impl Page {
    const ALL: [Self; 2] = [Self::First, Self::Second];

    fn other(self) -> Self {
        match self {
            Self::First => Self::Second,
            Self::Second => Self::First,
        }
    }
}

pub trait Storage {
    fn read(&mut self, page: Page, slot: usize) -> Result<Words, Error>;
    fn erase(&mut self, page: Page) -> Result<(), Error>;
    fn program(&mut self, page: Page, slot: usize, word: usize, value: u32) -> Result<(), Error>;
}

#[derive(Clone, Copy)]
struct Entry {
    generation: u32,
    settings: Settings,
}

impl Entry {
    fn encode(self) -> Words {
        let mut words = [ERASED; WORDS];
        words[0] = self.generation;
        for (destination, &name) in words[1..5]
            .iter_mut()
            .zip(self.settings.name.as_chunks::<4>().0)
        {
            *destination = u32::from_le_bytes(name);
        }
        words[5] = HEADER
            | u32::from(self.settings.roger_enabled)
            | self
                .settings
                .microphone_gain
                .map_or(0, |gain| 0x40 | (u32::from(gain) << 1));
        words[6] = crc32(&words[..6]);
        words[COMMIT_INDEX] = COMMIT;
        words
    }

    fn decode(words: Words) -> Option<Self> {
        if words[COMMIT_INDEX] != COMMIT || crc32(&words[..6]) != words[6] {
            return None;
        }
        let microphone_gain = if words[5] & !1 == LEGACY_HEADER {
            None
        } else if words[5] & !0x7f == HEADER {
            if words[5] & 0x40 != 0 {
                Some(((words[5] >> 1) & 31) as u8)
            } else if words[5] & 0x3e == 0 {
                None
            } else {
                return None;
            }
        } else {
            return None;
        };
        let mut name = [0; 16];
        for (destination, word) in name.as_chunks_mut::<4>().0.iter_mut().zip(&words[1..5]) {
            *destination = word.to_le_bytes();
        }
        Some(Self {
            generation: words[0],
            settings: Settings {
                name,
                roger_enabled: words[5] & 1 != 0,
                microphone_gain,
                positions: None,
                voice_shortcut: None,
            },
        })
    }
}

#[derive(Clone, Copy)]
struct Extension {
    generation: u32,
    primary_crc: u32,
    positions: Option<[u32; 2]>,
    voice_shortcut: Option<Option<u8>>,
}

impl Extension {
    fn encode(self) -> Words {
        let mut words = [ERASED; WORDS];
        words[0] = EXT_HEADER;
        words[1] = self.generation;
        if let Some(positions) = self.positions {
            words[2..4].copy_from_slice(&positions);
        }
        words[4] = self.primary_crc;
        words[5] = self.voice_shortcut.map_or(EXT_POSITIONS, |digit| {
            EXT_VOICE
                | if self.positions.is_some() {
                    HAS_POSITIONS
                } else {
                    0
                }
                | (u32::from(digit.unwrap_or(VOICE_DISABLED)) << VOICE_DIGIT_SHIFT)
        });
        words[6] = crc32(&words[..6]);
        words[COMMIT_INDEX] = EXT_COMMIT;
        words
    }

    fn decode(words: Words, generation: u32, primary_crc: u32) -> Option<Self> {
        if words[COMMIT_INDEX] != EXT_COMMIT
            || words[0] != EXT_HEADER
            || words[1] != generation
            || words[4] != primary_crc
            || crc32(&words[..6]) != words[6]
        {
            return None;
        }
        let digit = words[5] >> VOICE_DIGIT_SHIFT;
        let (positions, voice_shortcut) = if words[5] == EXT_POSITIONS {
            (Some([words[2], words[3]]), None)
        } else if words[5] & 0xff == EXT_VOICE && digit <= u32::from(VOICE_DISABLED) {
            let positions = (words[5] & HAS_POSITIONS != 0).then_some([words[2], words[3]]);
            let digit = digit as u8;
            (positions, Some((digit != VOICE_DISABLED).then_some(digit)))
        } else {
            return None;
        };
        Some(Self {
            generation,
            primary_crc,
            positions,
            voice_shortcut,
        })
    }
}

fn crc32(words: &[u32]) -> u32 {
    // Reflecting IEEE CRC-32 over each word in little-endian order.
    let mut crc = u32::MAX;
    for &word in words {
        crc ^= word;
        for _ in 0..u32::BITS {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn latest(storage: &mut impl Storage) -> Result<Option<(Page, usize, u32, Entry)>, Error> {
    let mut latest: Option<(Page, usize, u32, Entry)> = None;
    for page in Page::ALL {
        for slot in 0..SLOTS {
            let words = storage.read(page, slot)?;
            let Some(entry) = Entry::decode(words) else {
                continue;
            };
            if let Some((_, _, _, previous)) = latest {
                // At most 128 nearby generations survive, including at wrap.
                let delta = entry.generation.wrapping_sub(previous.generation);
                match delta {
                    0 | 0x8000_0000 => return Err(Error::Conflict),
                    1..=0x7fff_ffff => {}
                    _ => continue,
                }
            }
            latest = Some((page, slot, words[6], entry));
        }
    }
    Ok(latest)
}

fn latest_settings(storage: &mut impl Storage) -> Result<Option<(Page, Entry)>, Error> {
    let Some((page, slot, crc, mut entry)) = latest(storage)? else {
        return Ok(None);
    };
    if slot + 1 < SLOTS {
        let extension = storage.read(page, slot + 1)?;
        if let Some(extension) = Extension::decode(extension, entry.generation, crc) {
            entry.settings.positions = extension.positions;
            entry.settings.voice_shortcut = extension.voice_shortcut;
        }
    }
    Ok(Some((page, entry)))
}

pub fn load(storage: &mut impl Storage) -> Result<Option<Settings>, Error> {
    Ok(latest_settings(storage)?.map(|(_, entry)| entry.settings))
}

pub fn save(storage: &mut impl Storage, settings: Settings) -> Result<(), Error> {
    if settings.microphone_gain.is_some_and(|gain| gain > 31)
        || settings
            .voice_shortcut
            .flatten()
            .is_some_and(|digit| digit > 9)
    {
        return Err(Error::InvalidSettings);
    }
    let (active, generation) = match latest_settings(storage)? {
        Some((_, entry)) if settings == entry.settings => return Ok(()),
        Some((page, entry)) => (page, entry.generation.wrapping_add(1)),
        None => (Page::First, 0),
    };
    let entry = Entry {
        generation,
        settings,
    };
    // Append on the newest page. Extension records occupy the adjacent slot;
    // primary range scanning still decodes only the unchanged 32-byte format.
    let with_extension = settings.positions.is_some() || settings.voice_shortcut.is_some();
    if let Some(slot) = find_slot(storage, active, with_extension)? {
        return commit_entry(storage, active, slot, entry);
    }
    // Leave the active page untouched throughout erase and commit.
    let next = active.other();
    storage.erase(next)?;
    for slot in 0..SLOTS {
        if storage.read(next, slot)? != [ERASED; WORDS] {
            return Err(Error::Verify);
        }
    }
    commit_entry(storage, next, 0, entry)
}

fn find_slot(
    storage: &mut impl Storage,
    page: Page,
    with_extension: bool,
) -> Result<Option<usize>, Error> {
    for slot in 0..SLOTS {
        if storage.read(page, slot)? != [ERASED; WORDS] {
            continue;
        }
        if !with_extension {
            return Ok(Some(slot));
        }
        if slot + 1 < SLOTS && storage.read(page, slot + 1)? == [ERASED; WORDS] {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

fn commit_entry(
    storage: &mut impl Storage,
    page: Page,
    slot: usize,
    entry: Entry,
) -> Result<(), Error> {
    let primary = entry.encode();
    if entry.settings.positions.is_some() || entry.settings.voice_shortcut.is_some() {
        // Reserve and verify the primary before placing a committed extension.
        // This prevents a legacy writer from treating the primary as blank.
        stage_words(storage, page, slot, &primary)?;
        let extension = Extension {
            generation: entry.generation,
            primary_crc: primary[6],
            positions: entry.settings.positions,
            voice_shortcut: entry.settings.voice_shortcut,
        }
        .encode();
        commit_words(storage, page, slot + 1, &extension, EXT_COMMIT)?;
        commit_marker(storage, page, slot, &primary, COMMIT)?;
    } else {
        commit_words(storage, page, slot, &primary, COMMIT)?;
    }
    Ok(())
}

fn commit_words(
    storage: &mut impl Storage,
    page: Page,
    slot: usize,
    words: &Words,
    commit: u32,
) -> Result<(), Error> {
    stage_words(storage, page, slot, words)?;
    commit_marker(storage, page, slot, words, commit)
}

fn stage_words(
    storage: &mut impl Storage,
    page: Page,
    slot: usize,
    words: &Words,
) -> Result<(), Error> {
    for (word, &value) in words[..COMMIT_INDEX].iter().enumerate() {
        if value != ERASED {
            storage.program(page, slot, word, value)?;
        }
    }
    let staged = storage.read(page, slot)?;
    if staged[..COMMIT_INDEX] != words[..COMMIT_INDEX] || staged[COMMIT_INDEX] != ERASED {
        return Err(Error::Verify);
    }
    Ok(())
}

fn commit_marker(
    storage: &mut impl Storage,
    page: Page,
    slot: usize,
    words: &Words,
    commit: u32,
) -> Result<(), Error> {
    storage.program(page, slot, COMMIT_INDEX, commit)?;
    if storage.read(page, slot)? != *words {
        return Err(Error::Verify);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
