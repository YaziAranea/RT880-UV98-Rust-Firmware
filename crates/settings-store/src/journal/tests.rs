use super::*;

#[derive(Clone, Copy)]
enum Cut {
    Before,
    During,
    After,
}

#[derive(Clone)]
struct Model {
    pages: [[[u32; WORDS]; SLOTS]; 2],
    operations: usize,
    cut: Option<(usize, Cut)>,
    ignored_write: Option<usize>,
    read_error: bool,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            pages: [[[ERASED; WORDS]; SLOTS]; 2],
            operations: 0,
            cut: None,
            ignored_write: None,
            read_error: false,
        }
    }
}

impl Model {
    fn reset_operations(&mut self) {
        self.operations = 0;
        self.cut = None;
        self.ignored_write = None;
    }

    fn next_cut(&mut self) -> Option<Cut> {
        self.operations += 1;
        self.cut
            .filter(|&(op, _)| op == self.operations)
            .map(|(_, cut)| cut)
    }
}

impl Storage for Model {
    fn read(&mut self, page: Page, slot: usize) -> Result<Words, Error> {
        if self.read_error {
            Err(Error::Controller)
        } else {
            Ok(self.pages[page as usize][slot])
        }
    }

    fn erase(&mut self, page: Page) -> Result<(), Error> {
        let cut = self.next_cut();
        let page = &mut self.pages[page as usize];
        match cut {
            Some(Cut::Before) => return Err(Error::Operation),
            Some(Cut::During) => {
                // Independent old records and complete erased words can survive
                // beside a partially erased record.
                page[1] = [ERASED; WORDS];
                for word in &mut page[2] {
                    *word |= 0x55aa_55aa;
                }
                return Err(Error::Operation);
            }
            _ if self.ignored_write != Some(self.operations) => *page = [[ERASED; WORDS]; SLOTS],
            _ => {}
        }
        if matches!(cut, Some(Cut::After)) {
            return Err(Error::Operation);
        }
        Ok(())
    }

    fn program(&mut self, page: Page, slot: usize, word: usize, value: u32) -> Result<(), Error> {
        let cut = self.next_cut();
        let destination = &mut self.pages[page as usize][slot][word];
        assert_eq!(*destination, ERASED);
        match cut {
            Some(Cut::Before) => return Err(Error::Operation),
            Some(Cut::During) => {
                *destination = value | 0x55aa_55aa;
                return Err(Error::Operation);
            }
            _ if self.ignored_write != Some(self.operations) => *destination = value,
            _ => {}
        }
        if matches!(cut, Some(Cut::After)) {
            return Err(Error::Operation);
        }
        Ok(())
    }
}

fn settings(seed: u8) -> Settings {
    Settings {
        name: [seed; 16],
        roger_enabled: seed & 1 != 0,
        microphone_gain: Some(seed & 31),
        positions: None,
    }
}

#[test]
fn microphone_levels_round_trip_and_schema_one_retains_factory_gain() {
    for gain in 0..32 {
        let mut saved = settings(42);
        saved.microphone_gain = Some(gain);
        let record = Entry {
            generation: 7,
            settings: saved,
        }
        .encode();
        assert_eq!(Entry::decode(record).unwrap().settings, saved);
    }
    let mut legacy = Entry {
        generation: 8,
        settings: settings(43),
    }
    .encode();
    legacy[5] = LEGACY_HEADER | 1;
    legacy[6] = crc32(&legacy[..6]);
    let loaded = Entry::decode(legacy).unwrap();
    assert_eq!(loaded.settings.microphone_gain, None);
    assert!(loaded.settings.roger_enabled);
    assert_eq!(loaded.settings.name, [43; 16]);
    assert_eq!(loaded.settings.positions, None);
    let mut malformed = legacy;
    malformed[5] = HEADER | 2; // Gain bits without the validity flag.
    malformed[6] = crc32(&malformed[..6]);
    assert!(Entry::decode(malformed).is_none());
}

#[test]
fn invalid_microphone_gain_is_rejected_before_storage_access() {
    let mut model = Model {
        read_error: true,
        ..Model::default()
    };
    for gain in [32, 63, 128, 255] {
        let mut invalid = settings(42);
        invalid.microphone_gain = Some(gain);
        assert_eq!(save(&mut model, invalid), Err(Error::InvalidSettings));
        assert_eq!(model.operations, 0);
    }
}

fn fill_both_pages() -> Model {
    let mut model = Model::default();
    for sequence in 0..(2 * SLOTS) {
        save(&mut model, settings(sequence as u8)).unwrap();
    }
    model.reset_operations();
    model
}

#[test]
fn record_round_trip_and_crc_reject_every_single_bit_corruption() {
    let entry = Entry {
        generation: 0x4567_99aa,
        settings: Settings {
            name: *b"ABCD\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff",
            roger_enabled: true,
            microphone_gain: Some(31),
            positions: None,
        },
    };
    let words = entry.encode();
    assert_eq!(crc32(&[0]), 0x2144_df1c); // IEEE CRC-32 of 4 zero bytes.
    assert_eq!(Entry::decode(words).unwrap().settings, entry.settings);
    for word in 0..WORDS {
        for bit in 0..u32::BITS {
            let mut corrupted = words;
            corrupted[word] ^= 1 << bit;
            assert!(Entry::decode(corrupted).is_none());
        }
    }
}

#[test]
fn positions_use_adjacent_extension_without_changing_primary_and_bad_extension_keeps_primary() {
    let positions = [0x1020_3040, 0xa0b0_c0d0];
    let mut saved = settings(42);
    saved.positions = Some(positions);
    let primary_without_extension = Entry {
        generation: 0,
        settings: settings(42),
    }
    .encode();
    let mut model = Model::default();
    save(&mut model, saved).unwrap();
    assert_eq!(model.pages[0][0], primary_without_extension);
    assert_eq!(
        Extension::decode(
            model.pages[0][1],
            primary_without_extension[0],
            primary_without_extension[6]
        ),
        Some(positions)
    );
    assert_eq!(load(&mut model), Ok(Some(saved)));
    let committed = model.pages;
    model.reset_operations();
    save(&mut model, saved).unwrap();
    assert_eq!(model.operations, 0);
    assert_eq!(model.pages, committed);

    for (slot, bit) in [(0, 0), (2, 7), (4, 19), (6, 31), (7, 1)] {
        let mut corrupted = model.clone();
        corrupted.pages[0][1][slot] ^= 1 << bit;
        let loaded = load(&mut corrupted).unwrap().unwrap();
        assert_eq!(loaded.name, saved.name);
        assert_eq!(loaded.roger_enabled, saved.roger_enabled);
        assert_eq!(loaded.microphone_gain, saved.microphone_gain);
        assert_eq!(loaded.positions, None);
    }
}

#[test]
fn positions_power_cut_at_every_append_program_keeps_old_or_fully_committed_value() {
    let mut initial = Model::default();
    let previous = settings(1);
    save(&mut initial, previous).unwrap();
    initial.reset_operations();
    let mut next = settings(2);
    next.positions = Some([0x1234_5678, 0x8765_4321]);

    let mut complete = initial.clone();
    save(&mut complete, next).unwrap();
    assert_eq!(complete.operations, 2 * (WORDS - 1) + 2);
    for cut_at in 1..=complete.operations {
        for cut in [Cut::Before, Cut::During, Cut::After] {
            let mut interrupted = initial.clone();
            interrupted.cut = Some((cut_at, cut));
            assert_eq!(save(&mut interrupted, next), Err(Error::Operation));
            interrupted.reset_operations();
            let persisted = if cut_at == complete.operations && matches!(cut, Cut::After) {
                Some(next)
            } else {
                Some(previous)
            };
            assert_eq!(load(&mut interrupted), Ok(persisted), "operation {cut_at}");
            save(&mut interrupted, next).unwrap();
            assert_eq!(load(&mut interrupted), Ok(Some(next)));
        }
    }
}

#[test]
fn staged_primary_blocks_legacy_writer_from_resurrecting_extension() {
    let mut interrupted = Model::default();
    let mut next = settings(7);
    next.positions = Some([0x1234_5678, 0x8765_4321]);
    // Primary staging (7 words) and extension commit (8 words) finish before
    // the primary commit word. Power fails at the first attempted final word.
    interrupted.cut = Some((2 * (WORDS - 1) + 1, Cut::After));
    assert_eq!(save(&mut interrupted, next), Err(Error::Operation));
    interrupted.reset_operations();
    assert_eq!(
        Extension::decode(
            interrupted.pages[0][1],
            interrupted.pages[0][0][0],
            interrupted.pages[0][0][6]
        ),
        next.positions
    );

    // An old writer ignores extension magic and finds slot 2, since the
    // uncommitted primary in slot 0 and extension in slot 1 are both occupied.
    let mut old_payload = next;
    old_payload.positions = None;
    let legacy_primary = Entry {
        generation: 0,
        settings: old_payload,
    }
    .encode();
    commit_words(&mut interrupted, Page::First, 2, &legacy_primary, COMMIT).unwrap();
    assert_eq!(load(&mut interrupted), Ok(Some(old_payload)));
}

#[test]
fn positions_rollover_erases_only_inactive_page_and_commits_pair() {
    let mut model = fill_both_pages();
    let (active, _, _, _) = latest(&mut model).unwrap().unwrap();
    let active_before = model.pages[active as usize];
    let mut next = settings(220);
    next.positions = Some([0x1122_3344, 0x5566_7788]);
    model.reset_operations();
    save(&mut model, next).unwrap();
    let destination = active.other();
    assert_eq!(model.pages[active as usize], active_before);
    assert_eq!(
        Entry::decode(model.pages[destination as usize][0])
            .unwrap()
            .settings,
        settings(220)
    );
    assert_eq!(
        Extension::decode(
            model.pages[destination as usize][1],
            model.pages[destination as usize][0][0],
            model.pages[destination as usize][0][6]
        ),
        next.positions
    );
    assert_eq!(load(&mut model), Ok(Some(next)));
}

#[test]
fn append_reboot_idempotence_and_generation_wrap() {
    let mut model = Model::default();
    assert_eq!(load(&mut model), Ok(None));
    model.pages[0][0] = Entry {
        generation: u32::MAX,
        settings: settings(1),
    }
    .encode();
    save(&mut model, settings(2)).unwrap();
    assert_eq!(model.pages[0][1][0], 0);
    assert_eq!(load(&mut model.clone()), Ok(Some(settings(2))));
    model.reset_operations();
    save(&mut model, settings(2)).unwrap();
    assert_eq!(model.operations, 0);
}

#[test]
fn power_cut_before_during_after_every_append_or_rollover_write() {
    let mut append = Model::default();
    save(&mut append, settings(1)).unwrap();
    append.reset_operations();
    for (mut initial, erase_expected) in [
        (Model::default(), false),
        (append, false),
        (fill_both_pages(), true),
    ] {
        let previous = load(&mut initial).unwrap();
        let active = latest(&mut initial).unwrap().map(|(page, _, _, _)| page);
        let next = settings(133);
        let mut complete = initial.clone();
        save(&mut complete, next).unwrap();
        let actions = complete.operations;
        assert_eq!(actions, WORDS + usize::from(erase_expected));
        for cut_at in 1..=actions {
            for cut in [Cut::Before, Cut::During, Cut::After] {
                let mut interrupted = initial.clone();
                interrupted.cut = Some((cut_at, cut));
                assert_eq!(save(&mut interrupted, next), Err(Error::Operation));
                interrupted.reset_operations();
                let persisted = if cut_at == actions && matches!(cut, Cut::After) {
                    Some(next)
                } else {
                    previous
                };
                assert_eq!(load(&mut interrupted), Ok(persisted), "operation {cut_at}");
                if erase_expected {
                    let active = active.unwrap();
                    assert_eq!(
                        interrupted.pages[active as usize],
                        initial.pages[active as usize]
                    );
                }
                save(&mut interrupted, next).unwrap();
                assert_eq!(load(&mut interrupted), Ok(Some(next)));
            }
        }
    }
}

#[test]
fn ignored_erase_and_program_fail_readback_and_recover_previous_record() {
    let initial = fill_both_pages();
    let mut model = initial.clone();
    let previous = load(&mut model).unwrap();
    for op in 1..=WORDS + 1 {
        let mut model = initial.clone();
        model.ignored_write = Some(op);
        assert_eq!(save(&mut model, settings(201)), Err(Error::Verify));
        assert_eq!(load(&mut model), Ok(previous));
        assert_eq!(model.pages[1], initial.pages[1]);
        if op != WORDS + 1 {
            assert_eq!(
                model.pages[0][0][COMMIT_INDEX],
                if op == 1 { COMMIT } else { ERASED }
            );
        }
    }
}

#[test]
fn unreadable_or_conflicting_records_prevent_write() {
    let mut model = Model {
        read_error: true,
        ..Model::default()
    };
    assert_eq!(save(&mut model, settings(1)), Err(Error::Controller));
    assert_eq!(model.operations, 0);
    model.read_error = false;
    model.pages[0][0] = Entry {
        generation: 7,
        settings: settings(2),
    }
    .encode();
    model.pages[1][0] = Entry {
        generation: 7,
        settings: settings(3),
    }
    .encode();
    assert_eq!(load(&mut model), Err(Error::Conflict));
    assert_eq!(save(&mut model, settings(4)), Err(Error::Conflict));
    assert_eq!(model.operations, 0);
}
