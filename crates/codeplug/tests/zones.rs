use uv98_codeplug::zones::{Error, Zone, ZoneMut, Zones, ZonesMut, EMPTY_MEMBER, RECORD_LEN};
use uv98_codeplug::ZONES_LEN;

#[test]
fn zone_section_size_and_index_bounds_are_strict() {
    for len in [0, ZONES_LEN - 1, ZONES_LEN + 1] {
        let mut bytes = vec![0xFF; len];
        assert!(
            matches!(Zones::parse(&bytes), Err(Error::WrongLength { actual }) if actual == len)
        );
        assert!(
            matches!(ZonesMut::parse(&mut bytes), Err(Error::WrongLength { actual }) if actual == len)
        );
    }
    let mut bytes = vec![0xA5; ZONES_LEN];
    let mut zones = ZonesMut::parse(&mut bytes).unwrap();
    assert!(zones.zone(256).is_none());
    assert!(zones.zone(usize::MAX).is_none());
    assert!(zones.view().zone(usize::MAX).is_none());
    zones.zone(255).unwrap().set_member_code(199, 1023).unwrap();
    let last = zones.view().zone(255).unwrap();
    assert_eq!(last.member_code(199), Some(1023));
    assert_eq!(last.member_code(200), None);
    assert_eq!(last.member_code(usize::MAX), None);
    let mut expected = vec![0xA5; ZONES_LEN];
    expected[255 * 512 + 418] = 0xFF;
    expected[255 * 512 + 419] = 3;
    assert_eq!(zones.view().raw().as_slice(), expected);
}

#[test]
fn membership_edits_preserve_header_holes_tail_and_unknown_codes() {
    let mut bytes = [0xA5; RECORD_LEN];
    bytes[0] = 17;
    bytes[1] = 93;
    bytes[22..24].copy_from_slice(&0x1234_u16.to_le_bytes());
    let mut expected = bytes;
    expected[20] = 0;
    expected[21] = 0;
    expected[418] = 0xFF;
    expected[419] = 3;
    let mut zone = ZoneMut::from_bytes(&mut bytes);
    zone.set_member_code(0, 0).unwrap();
    zone.set_member_code(199, 1023).unwrap();
    assert_eq!(zone.view().member_code(1), Some(0x1234));
    assert_eq!(zone.view().cps_count_bytes(), [17, 93]);
    assert_eq!(zone.view().raw(), &expected);
    expected[20..22].fill(0xFF);
    zone.set_member_code(0, EMPTY_MEMBER).unwrap();
    assert_eq!(zone.view().raw(), &expected);
}

#[test]
fn count_refresh_explicitly_duplicates_count_without_compacting() {
    let mut bytes = [0xFF; RECORD_LEN];
    bytes[20..22].copy_from_slice(&0_u16.to_le_bytes());
    bytes[24..26].copy_from_slice(&1023_u16.to_le_bytes());
    bytes[418..420].copy_from_slice(&0x1234_u16.to_le_bytes());
    let mut expected = bytes;
    expected[0] = 2;
    expected[1] = 2;
    let mut zone = ZoneMut::from_bytes(&mut bytes);
    assert_eq!(zone.refresh_cps_count(), 2);
    assert_eq!(zone.view().raw(), &expected);
    assert_eq!(zone.view().member_code(1), Some(0xFFFF));
    assert_eq!(zone.view().member_code(199), Some(0x1234));
}

#[test]
fn invalid_members_and_long_names_do_not_mutate_zone() {
    let mut bytes = [0x96; RECORD_LEN];
    let mut zone = ZoneMut::from_bytes(&mut bytes);
    assert_eq!(
        zone.set_member_code(200, 1),
        Err(Error::MemberOutOfRange { slot: 200 })
    );
    assert_eq!(
        zone.set_member_code(usize::MAX, 1),
        Err(Error::MemberOutOfRange { slot: usize::MAX })
    );
    assert_eq!(
        zone.set_member_code(1, 1024),
        Err(Error::InvalidMemberCode { value: 1024 })
    );
    assert_eq!(
        zone.set_member_code(1, 65534),
        Err(Error::InvalidMemberCode { value: 65534 })
    );
    assert_eq!(
        zone.set_name_encoded(&[0; 17]),
        Err(Error::NameTooLong { actual: 17 })
    );
    assert_eq!(zone.view().raw(), &[0x96; RECORD_LEN]);
}

#[test]
fn name_patches_preserve_gbk_bytes_and_other_record_bytes() {
    let mut bytes = [0x69; RECORD_LEN];
    let mut expected = bytes;
    let name = [0xD6, 0xD0, 0xCE, 0xC4]; // Already encoded; no UTF-8 assumption.
    expected[4..20].fill(0xFF);
    expected[4..8].copy_from_slice(&name);
    let mut zone = ZoneMut::from_bytes(&mut bytes);
    zone.set_name_encoded(&name).unwrap();
    assert_eq!(zone.view().raw(), &expected);
    let exact = [0x80, 0, 0xFF, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
    expected[4..20].copy_from_slice(&exact);
    zone.set_name_raw(&exact);
    assert_eq!(zone.view().name_raw(), &exact);
    assert_eq!(zone.view().raw(), &expected);
    assert_eq!(Zone::from_bytes(&bytes).raw(), &expected);
}
