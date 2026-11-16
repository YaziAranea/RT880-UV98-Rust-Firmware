use crate::{Error, Settings};

pub const PAGE_SIZE: usize = 2048;
pub const WORDS: usize = 8;
const SLOTS: usize = PAGE_SIZE / (WORDS * size_of::<u32>());
const ERASED: u32 = u32::MAX;
const HEADER: u32 = 0x5556_0100; // "UV", schema 1, flags in the low byte.
const COMMIT: u32 = 0x51A7_6E39;
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
        words[5] = HEADER | u32::from(self.settings.roger_enabled);
        words[6] = crc32(&words[..6]);
        words[COMMIT_INDEX] = COMMIT;
        words
    }

    fn decode(words: Words) -> Option<Self> {
        if words[COMMIT_INDEX] != COMMIT
            || words[5] & !1 != HEADER
            || crc32(&words[..6]) != words[6]
        {
            return None;
        }
        let mut name = [0; 16];
        for (destination, word) in name.as_chunks_mut::<4>().0.iter_mut().zip(&words[1..5]) {
            *destination = word.to_le_bytes();
        }
        Some(Self {
            generation: words[0],
            settings: Settings {
                name,
                roger_enabled: words[5] & 1 != 0,
            },
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

fn latest(storage: &mut impl Storage) -> Result<Option<(Page, Entry)>, Error> {
    let mut latest: Option<(Page, Entry)> = None;
    for page in Page::ALL {
        for slot in 0..SLOTS {
            let Some(entry) = Entry::decode(storage.read(page, slot)?) else {
                continue;
            };
            if let Some((_, previous)) = latest {
                // At most 128 nearby generations survive, including at wrap.
                let delta = entry.generation.wrapping_sub(previous.generation);
                match delta {
                    0 | 0x8000_0000 => return Err(Error::Conflict),
                    1..=0x7fff_ffff => {}
                    _ => continue,
                }
            }
            latest = Some((page, entry));
        }
    }
    Ok(latest)
}

pub fn load(storage: &mut impl Storage) -> Result<Option<Settings>, Error> {
    Ok(latest(storage)?.map(|(_, entry)| entry.settings))
}

pub fn save(storage: &mut impl Storage, settings: Settings) -> Result<(), Error> {
    let (active, generation) = match latest(storage)? {
        Some((_, entry)) if settings == entry.settings => return Ok(()),
        Some((page, entry)) => (page, entry.generation.wrapping_add(1)),
        None => (Page::First, 0),
    };
    // Append on the page with the newest valid entry. Skip interrupted writes.
    for slot in 0..SLOTS {
        if storage.read(active, slot)? == [ERASED; WORDS] {
            return commit(
                storage,
                active,
                slot,
                Entry {
                    generation,
                    settings,
                },
            );
        }
    }
    // Leave the active page untouched throughout erase and commit.
    let next = active.other();
    storage.erase(next)?;
    for slot in 0..SLOTS {
        if storage.read(next, slot)? != [ERASED; WORDS] {
            return Err(Error::Verify);
        }
    }
    commit(
        storage,
        next,
        0,
        Entry {
            generation,
            settings,
        },
    )
}

fn commit(storage: &mut impl Storage, page: Page, slot: usize, entry: Entry) -> Result<(), Error> {
    let words = entry.encode();
    for (word, &value) in words[..COMMIT_INDEX].iter().enumerate() {
        if value != ERASED {
            storage.program(page, slot, word, value)?;
        }
    }
    let staged = storage.read(page, slot)?;
    if staged[..COMMIT_INDEX] != words[..COMMIT_INDEX] || staged[COMMIT_INDEX] != ERASED {
        return Err(Error::Verify);
    }
    storage.program(page, slot, COMMIT_INDEX, COMMIT)?;
    if storage.read(page, slot)? != words {
        return Err(Error::Verify);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
