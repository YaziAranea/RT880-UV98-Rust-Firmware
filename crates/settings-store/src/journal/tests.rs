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
        let active = latest(&mut initial).unwrap().map(|(page, _)| page);
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
