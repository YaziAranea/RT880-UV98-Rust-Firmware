use uv98_codeplug::{
    zones::{Zone, RECORD_LEN},
    FILE_LEN, ZONES_OFFSET,
};
use uv98_cps_core::{zone_display_members, Document, ZoneDisplayError};

fn record(count: u8, members: &[(usize, u16)]) -> [u8; RECORD_LEN] {
    let mut raw = [0xff; RECORD_LEN];
    raw[..2].fill(count);
    for &(slot, code) in members {
        raw[20 + slot * 2..22 + slot * 2].copy_from_slice(&code.to_le_bytes());
    }
    raw
}

#[test]
fn empty_header_never_invents_two_hundred_channel_one_references() {
    let zero_filled = [0; RECORD_LEN];
    let erased_members = record(0, &[]);
    for raw in [&zero_filled, &erased_members] {
        assert_eq!(zone_display_members(Zone::from_bytes(raw)), Ok(Vec::new()));
    }
}

#[test]
fn valid_members_keep_sparse_storage_order_and_duplicates() {
    let raw = record(3, &[(0, 20), (3, 0), (199, 20)]);
    assert_eq!(
        zone_display_members(Zone::from_bytes(&raw)),
        Ok(vec![20, 0, 20])
    );
}

#[test]
fn ambiguous_nonempty_records_are_not_normalized_into_editable_members() {
    let mut mismatch = record(2, &[(0, 0), (1, 1)]);
    mismatch[1] = 3;
    let cases = [
        (
            mismatch,
            ZoneDisplayError::HeaderMismatch {
                first: 2,
                second: 3,
            },
        ),
        (
            record(201, &[]),
            ZoneDisplayError::HeaderCount { declared: 201 },
        ),
        (
            record(1, &[(0, 0), (1, 1)]),
            ZoneDisplayError::CountMismatch {
                declared: 1,
                found: 2,
            },
        ),
        (
            record(2, &[(0, 0)]),
            ZoneDisplayError::CountMismatch {
                declared: 2,
                found: 1,
            },
        ),
        (
            record(1, &[(0, 1024)]),
            ZoneDisplayError::MemberCode {
                slot: 0,
                code: 1024,
            },
        ),
    ];
    for (raw, error) in cases {
        assert_eq!(zone_display_members(Zone::from_bytes(&raw)), Err(error));
    }
}

#[test]
fn display_inspection_keeps_every_original_file_byte_and_undo_history_untouched() {
    for raw in [
        [0; RECORD_LEN],
        record(2, &[(0, 20), (4, 0)]),
        record(201, &[]),
    ] {
        let mut bytes = vec![0xa5; FILE_LEN];
        bytes[ZONES_OFFSET..ZONES_OFFSET + RECORD_LEN].copy_from_slice(&raw);
        let document = Document::from_bytes(&bytes).unwrap();
        let zones = uv98_codeplug::zones::Zones::parse(document.view().zones()).unwrap();
        let _ = zone_display_members(zones.zone(0).unwrap());
        assert_eq!(document.bytes(), bytes);
        assert_eq!(document.original_bytes(), bytes);
        assert_eq!(document.export_bytes(), bytes);
        assert_eq!(document.undo_depth(), 0);
        assert!(!document.is_dirty());
    }
}
