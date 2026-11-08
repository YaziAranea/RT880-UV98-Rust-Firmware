use uv98_codeplug::{Error, Section, CHANNELS_OFFSET, FILE_LEN, TIME_OFFSET};
use uv98_cps_core::{Document, EditSummary};

fn synthetic_file() -> Vec<u8> {
    // Deliberately synthetic bytes; not a reconstructed or acquired radio image.
    (0..FILE_LEN)
        .map(|index| ((index * 73 + index / 257 + 19) & 255) as u8)
        .collect()
}

#[test]
fn import_export_is_byte_exact_without_semantic_or_time_claims() {
    let bytes = synthetic_file();
    let document = Document::from_bytes(&bytes).unwrap();
    assert_eq!(document.bytes(), bytes);
    assert_eq!(document.original_bytes(), bytes);
    assert_eq!(document.export_bytes(), bytes);
    assert_eq!(
        document.view().time(),
        &bytes[TIME_OFFSET..TIME_OFFSET + 32_768]
    );
    assert!(!document.is_dirty());
    assert_eq!(document.dirty_bytes(), 0);
    assert!(document.dirty_sections().is_empty());
    assert!(document.changes().is_empty());
    assert_eq!(document.undo_depth(), 0);
}

#[test]
fn import_rejects_wrong_length_before_creating_a_document() {
    for length in [0, 4096, FILE_LEN - 1, FILE_LEN + 1] {
        assert_eq!(
            Document::from_bytes(&vec![0; length]).unwrap_err(),
            Error::WrongLength { actual: length }
        );
    }
}

#[test]
fn changes_are_grouped_and_do_not_cross_section_boundaries() {
    let bytes = synthetic_file();
    let mut document = Document::from_bytes(&bytes).unwrap();
    let summary = document
        .apply(|candidate| {
            for offset in [
                CHANNELS_OFFSET - 1,
                CHANNELS_OFFSET,
                CHANNELS_OFFSET + 1,
                CHANNELS_OFFSET + 3,
                FILE_LEN - 1,
            ] {
                candidate[offset] ^= 0xFF;
            }
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(
        summary,
        EditSummary {
            changed_bytes: 5,
            changed_sections: 3
        }
    );
    assert_eq!(document.dirty_bytes(), 5);
    assert_eq!(
        document.dirty_sections(),
        [Section::Cfg, Section::Channels, Section::Zones]
    );
    let changes = document.changes();
    assert_eq!(changes.len(), 3);
    assert_eq!(changes[0].section, Section::Cfg);
    assert_eq!(changes[0].ranges.len(), 1);
    assert_eq!(changes[0].ranges[0].file_offset, CHANNELS_OFFSET - 1);
    assert_eq!(changes[1].changed_bytes, 3);
    assert_eq!(changes[1].ranges.len(), 2);
    assert_eq!(changes[1].ranges[0].file_offset, CHANNELS_OFFSET);
    assert_eq!(
        changes[1].ranges[0].before,
        bytes[CHANNELS_OFFSET..CHANNELS_OFFSET + 2]
    );
    assert_eq!(
        changes[1].ranges[0].after,
        document.bytes()[CHANNELS_OFFSET..CHANNELS_OFFSET + 2]
    );
    assert_eq!(changes[1].ranges[1].file_offset, CHANNELS_OFFSET + 3);
    assert_eq!(changes[2].ranges[0].file_offset, FILE_LEN - 1);
    assert_eq!(document.original_bytes(), bytes);
}

#[test]
fn callback_error_rolls_back_every_candidate_edit_and_keeps_history() {
    let bytes = synthetic_file();
    let mut document = Document::from_bytes(&bytes).unwrap();
    document
        .apply(|candidate| {
            candidate[10] ^= 1;
            Ok::<_, ()>(())
        })
        .unwrap();
    let before_failed_transaction = document.export_bytes();
    let result = document.apply(|candidate| {
        candidate[0] ^= 0xFF;
        candidate[FILE_LEN - 1] ^= 0xFF;
        Err::<(), _>("field validation failed")
    });
    assert_eq!(result, Err("field validation failed"));
    assert_eq!(document.bytes(), before_failed_transaction);
    assert_eq!(document.undo_depth(), 1);
    assert!(document.undo());
    assert_eq!(document.bytes(), bytes);
    assert!(!document.undo());
}

#[test]
fn undo_reverts_transactions_in_order_and_noop_does_not_consume_history() {
    let bytes = synthetic_file();
    let mut document = Document::from_bytes(&bytes).unwrap();
    document
        .apply(|candidate| {
            candidate[8] ^= 1;
            Ok::<_, ()>(())
        })
        .unwrap();
    let first = document.export_bytes();
    document
        .apply(|candidate| {
            candidate[8] ^= 2;
            candidate[9] ^= 4;
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(document.undo_depth(), 2);
    let no_op = document.apply(|_| Ok::<_, ()>(())).unwrap();
    assert_eq!(
        no_op,
        EditSummary {
            changed_bytes: 0,
            changed_sections: 0
        }
    );
    assert_eq!(document.undo_depth(), 2);
    assert!(document.undo());
    assert_eq!(document.bytes(), first);
    assert_eq!(document.dirty_bytes(), 1);
    assert!(document.undo());
    assert_eq!(document.bytes(), bytes);
    assert_eq!(document.dirty_bytes(), 0);
    assert!(!document.undo());
}

#[test]
fn dirty_counts_compare_original_not_last_transaction_and_revert_clears_history() {
    let bytes = synthetic_file();
    let mut document = Document::from_bytes(&bytes).unwrap();
    document
        .apply(|candidate| {
            candidate[20] ^= 1;
            Ok::<_, ()>(())
        })
        .unwrap();
    let summary = document
        .apply(|candidate| {
            candidate[20] = bytes[20];
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(summary.changed_bytes, 1);
    assert_eq!(document.dirty_bytes(), 0);
    assert!(!document.is_dirty());
    assert_eq!(document.undo_depth(), 2);
    assert!(!document.revert_all());
    assert_eq!(document.undo_depth(), 0);
    assert!(!document.undo());

    document
        .apply(|candidate| {
            candidate[30] ^= 1;
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(document.revert_all());
    assert_eq!(document.export_bytes(), bytes);
    assert_eq!(document.original_bytes(), bytes);
    assert!(!document.undo());
}

#[test]
fn exported_copy_is_independent_of_document_and_opaque_time_is_preserved() {
    let bytes = synthetic_file();
    let mut document = Document::from_bytes(&bytes).unwrap();
    document
        .apply(|candidate| {
            candidate[CHANNELS_OFFSET + 5] ^= 8;
            Ok::<_, ()>(())
        })
        .unwrap();
    let mut exported = document.export_bytes();
    assert_eq!(
        &exported[TIME_OFFSET..TIME_OFFSET + 32_768],
        &bytes[TIME_OFFSET..TIME_OFFSET + 32_768]
    );
    exported[0] ^= 0xFF;
    assert_ne!(exported[0], document.bytes()[0]);
    assert_eq!(document.original_bytes(), bytes);
}

#[test]
fn reviewed_channel_patch_commits_atomically_and_preserves_other_bytes() {
    use uv98_codeplug::channels::{patch_channel, ChannelPatch, Tone};
    use uv98_codeplug::CHANNELS_LEN;

    let bytes = synthetic_file();
    let mut document = Document::from_bytes(&bytes).unwrap();
    let end = CHANNELS_OFFSET + CHANNELS_LEN;
    let patch = ChannelPatch {
        rx_frequency_code: Some(14_550_000),
        ..ChannelPatch::default()
    };
    document
        .apply(|candidate| patch_channel(&mut candidate[CHANNELS_OFFSET..end], 0, &patch))
        .unwrap();
    let after = document.export_bytes();
    assert_eq!(&after[..CHANNELS_OFFSET + 6], &bytes[..CHANNELS_OFFSET + 6]);
    assert_eq!(
        &after[CHANNELS_OFFSET + 10..],
        &bytes[CHANNELS_OFFSET + 10..]
    );
    assert_eq!(
        &after[CHANNELS_OFFSET + 6..CHANNELS_OFFSET + 10],
        &14_550_000_u32.to_le_bytes()
    );

    let invalid = ChannelPatch {
        tx_frequency_code: Some(43_350_000),
        rx_tone: Some(Tone::Ctcss { tenths_hz: 0x1000 }),
        ..ChannelPatch::default()
    };
    assert!(document
        .apply(|candidate| patch_channel(&mut candidate[CHANNELS_OFFSET..end], 0, &invalid))
        .is_err());
    assert_eq!(document.bytes(), after);
    assert_eq!(document.undo_depth(), 1);
    assert!(document.undo());
    assert_eq!(document.bytes(), bytes);
}
