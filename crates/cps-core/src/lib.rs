#![forbid(unsafe_code)]
//! In-memory document editing for the exact-length saved CPS file format.
//!
//! This crate does not access files, ports, radios or networks. Import checks
//! length only through `uv98-codeplug`; no acquisition provenance or semantic
//! validity is inferred. In particular, Time bytes may be stale or initialized
//! by the source CPS. Export returns the complete supplied file with committed
//! edits, never a reconstruction from partially read radio data.
//!
//! Edits run on an isolated candidate. The callback should use the reviewed
//! codeplug field APIs; returning an error discards all candidate changes.
//! No mutable reference to the document's committed or original bytes escapes.

use uv98_codeplug::{Codeplug, Error, Section, SECTIONS};

mod zone_display;
pub use zone_display::{zone_display_members, ZoneDisplayError};

/// A contiguous changed run within one section; offsets are saved-file offsets.
/// `before` and `after` contain equally many bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ByteRangeChange {
    pub file_offset: usize,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SectionChanges {
    pub section: Section,
    pub changed_bytes: usize,
    pub ranges: Vec<ByteRangeChange>,
}

/// Counts relative to the state immediately before one committed transaction.
/// These differ from document dirty counts, which compare with the original.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditSummary {
    pub changed_bytes: usize,
    pub changed_sections: usize,
}

#[derive(Debug)]
struct UndoRange {
    offset: usize,
    before: Vec<u8>,
}

/// Owns original bytes, working bytes, and inverse patches for undo.
/// No filename, model identity, acquisition timestamp, or read status is inferred.
#[derive(Debug)]
pub struct Document {
    original: Vec<u8>,
    working: Vec<u8>,
    undo: Vec<Vec<UndoRange>>,
}

impl Document {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Codeplug::parse(bytes)?;
        Ok(Self {
            original: bytes.to_vec(),
            working: bytes.to_vec(),
            undo: Vec::new(),
        })
    }

    /// Current committed bytes, including opaque sections and unknown fields.
    pub fn bytes(&self) -> &[u8] {
        &self.working
    }

    /// The exact input bytes; commits and undo never modify this baseline.
    pub fn original_bytes(&self) -> &[u8] {
        &self.original
    }

    /// A read-only view of the current bytes. Length is invariant after import.
    pub fn view(&self) -> Codeplug<'_> {
        Codeplug::parse(&self.working).expect("internal document length invariant")
    }

    /// Apply one local transaction atomically.
    ///
    /// A fixed slice prevents resizing. The callback is responsible for field
    /// validation and should return its codeplug API error. `Err` leaves bytes
    /// and undo history untouched. A successful no-op creates no undo entry.
    /// This method does not perform semantic validation beyond the callback.
    pub fn apply<E>(
        &mut self,
        edit: impl FnOnce(&mut [u8]) -> Result<(), E>,
    ) -> Result<EditSummary, E> {
        let mut candidate = self.working.clone();
        edit(&mut candidate)?;
        let changes = compare_sections(&self.working, &candidate);
        let summary = EditSummary {
            changed_bytes: changes.iter().map(|section| section.changed_bytes).sum(),
            changed_sections: changes.len(),
        };
        if summary.changed_bytes != 0 {
            let inverse = changes
                .into_iter()
                .flat_map(|section| section.ranges)
                .map(|range| UndoRange {
                    offset: range.file_offset,
                    before: range.before,
                })
                .collect();
            self.undo.push(inverse);
            self.working = candidate;
        }
        Ok(summary)
    }

    /// Undo the most recent successful nonempty transaction, if one exists.
    pub fn undo(&mut self) -> bool {
        let Some(inverse) = self.undo.pop() else {
            return false;
        };
        for range in inverse {
            let end = range.offset + range.before.len();
            self.working[range.offset..end].copy_from_slice(&range.before);
        }
        true
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Restore the original input and clear all undo history.
    /// Returns whether bytes changed; history is cleared even if already clean.
    /// Revert itself does not create an undo transaction.
    pub fn revert_all(&mut self) -> bool {
        let changed = self.is_dirty();
        self.working.clone_from(&self.original);
        self.undo.clear();
        changed
    }

    pub fn is_dirty(&self) -> bool {
        self.working != self.original
    }

    pub fn dirty_bytes(&self) -> usize {
        self.original
            .iter()
            .zip(&self.working)
            .filter(|(before, after)| before != after)
            .count()
    }

    /// Changed sections in the format's file order.
    pub fn dirty_sections(&self) -> Vec<Section> {
        SECTIONS
            .into_iter()
            .filter(|section| {
                let start = section.offset();
                let end = start + section.byte_len();
                self.original[start..end] != self.working[start..end]
            })
            .collect()
    }

    /// Review current changes against the original, grouped by section.
    /// Unchanged sections and bytes are omitted. Adjacent changed runs never
    /// cross a section boundary, even if both boundary bytes are changed.
    pub fn changes(&self) -> Vec<SectionChanges> {
        compare_sections(&self.original, &self.working)
    }

    /// Copy the current complete file bytes for the caller to save or inspect.
    /// No I/O occurs, no unknown bytes are rebuilt, and the baseline is unchanged.
    pub fn export_bytes(&self) -> Vec<u8> {
        self.working.clone()
    }
}

fn compare_sections(before: &[u8], after: &[u8]) -> Vec<SectionChanges> {
    let mut result = Vec::new();
    for section in SECTIONS {
        let end = section.offset() + section.byte_len();
        let mut at = section.offset();
        let mut ranges = Vec::new();
        let mut changed_bytes = 0;
        while at < end {
            if before[at] == after[at] {
                at += 1;
                continue;
            }
            let start = at;
            while at < end && before[at] != after[at] {
                at += 1;
            }
            changed_bytes += at - start;
            ranges.push(ByteRangeChange {
                file_offset: start,
                before: before[start..at].to_vec(),
                after: after[start..at].to_vec(),
            });
        }
        if changed_bytes != 0 {
            result.push(SectionChanges {
                section,
                changed_bytes,
                ranges,
            });
        }
    }
    result
}
