use uv98_codeplug::{
    Codeplug, Error, Section, CFG_LEN, CFG_OFFSET, CHANNELS_LEN, CHANNELS_OFFSET, FILE_LEN, FM_LEN,
    FM_OFFSET, SECTIONS, TIME_LEN, TIME_OFFSET, ZONES_LEN, ZONES_OFFSET,
};

#[test]
fn section_boundaries_match_the_headerless_file_layout() {
    assert_eq!(FILE_LEN, 221_184);
    assert_eq!(
        [
            CFG_OFFSET,
            CHANNELS_OFFSET,
            FM_OFFSET,
            TIME_OFFSET,
            ZONES_OFFSET
        ],
        [0x00000, 0x01000, 0x0D000, 0x0E000, 0x16000]
    );
    assert_eq!(
        [CFG_LEN, CHANNELS_LEN, FM_LEN, TIME_LEN, ZONES_LEN],
        [0x01000, 0x0C000, 0x01000, 0x08000, 0x20000]
    );

    let mut bytes = vec![0; FILE_LEN];
    for (index, section) in SECTIONS.into_iter().enumerate() {
        let start = section.offset();
        let end = start + section.byte_len();
        bytes[start..end].fill(index as u8 + 1);
        bytes[start] = 0xA0 + index as u8;
        bytes[end - 1] = 0xB0 + index as u8;
    }
    let codeplug = Codeplug::parse(&bytes).unwrap();
    for (index, section) in SECTIONS.into_iter().enumerate() {
        let view = codeplug.section(section);
        assert_eq!(view.len(), section.byte_len());
        assert_eq!(view[0], 0xA0 + index as u8);
        assert_eq!(view[view.len() - 1], 0xB0 + index as u8);
        assert!(view[1..view.len() - 1]
            .iter()
            .all(|byte| *byte == index as u8 + 1));
        if index + 1 < SECTIONS.len() {
            assert_eq!(
                section.offset() + section.byte_len(),
                SECTIONS[index + 1].offset()
            );
        } else {
            assert_eq!(section.offset() + section.byte_len(), FILE_LEN);
        }
    }
}

#[test]
fn every_view_borrows_original_bytes_without_changes() {
    let bytes: Vec<u8> = (0..FILE_LEN)
        .map(|index| ((index * 73 + index / 257 + 19) & 255) as u8)
        .collect();
    let before = bytes.clone();
    let codeplug = Codeplug::parse(&bytes).unwrap();
    assert_eq!(codeplug.raw().as_slice(), bytes.as_slice());
    assert_eq!(codeplug.raw().as_ptr(), bytes.as_ptr());

    for section in SECTIONS {
        let start = section.offset();
        let expected = &bytes[start..start + section.byte_len()];
        assert_eq!(codeplug.section(section), expected);
        assert_eq!(codeplug.section(section).as_ptr(), expected.as_ptr());
    }
    for (section, fixed) in [
        (Section::Cfg, codeplug.cfg().as_slice()),
        (Section::Channels, codeplug.channels().as_slice()),
        (Section::Fm, codeplug.fm().as_slice()),
        (Section::Time, codeplug.time().as_slice()),
        (Section::Zones, codeplug.zones().as_slice()),
    ] {
        assert_eq!(fixed, codeplug.section(section));
        assert_eq!(fixed.as_ptr(), codeplug.section(section).as_ptr());
    }
    assert_eq!(bytes, before);
}

#[test]
fn rejects_empty_truncated_and_oversized_files() {
    let bytes = vec![0; FILE_LEN + 4096];
    for length in [
        0,
        1,
        CFG_LEN - 1,
        CHANNELS_OFFSET,
        FM_OFFSET - 1,
        TIME_OFFSET,
        ZONES_OFFSET - 1,
        FILE_LEN - 1,
        FILE_LEN + 1,
        FILE_LEN + 4096,
    ] {
        assert_eq!(
            Codeplug::parse(&bytes[..length]).unwrap_err(),
            Error::WrongLength { actual: length }
        );
    }
}

#[test]
fn exact_length_does_not_claim_semantic_validity_or_time_provenance() {
    for value in [0, 0x9A, 0xA5, 0xFF] {
        let bytes = vec![value; FILE_LEN];
        let codeplug = Codeplug::parse(&bytes).unwrap();
        assert_eq!(codeplug.raw().as_slice(), bytes.as_slice());
        assert!(codeplug.time().iter().all(|byte| *byte == value));
        assert_eq!(codeplug.time().as_ptr(), bytes[TIME_OFFSET..].as_ptr());
    }
}
