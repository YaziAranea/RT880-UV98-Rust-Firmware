use uv98_codeplug::channels::{
    channel, padded_name, patch_channel, patch_record, Bandwidth, Channel, ChannelError,
    ChannelPatch, OperationMode, Power, Tone, CHANNEL_COUNT, CHANNEL_LEN, NAME_LEN,
};
use uv98_codeplug::CHANNELS_LEN;

fn patterned_record() -> [u8; CHANNEL_LEN] {
    core::array::from_fn(|index| (index as u8).wrapping_mul(37).wrapping_add(13))
}

#[test]
fn source_offsets_decode_without_normalizing_the_record() {
    let mut raw = patterned_record();
    raw[1] = 1;
    raw[2] = 1;
    raw[3] = 0;
    raw[4..6].copy_from_slice(&[0x75, 0x13]); // CTCSS 88.5: 0x1000 | 885.
    raw[6..10].copy_from_slice(&[0x78, 0x56, 0x34, 0x12]);
    raw[10..14].copy_from_slice(&[0xef, 0xcd, 0xab, 0x89]);
    raw[14..16].copy_from_slice(&[0x13, 0x30]); // D023I: 0x3000 | octal 023.
    raw[16] = 2;
    raw[19] = 0x53;
    raw[32..48].copy_from_slice(&[b'A', 0, b'B', 0xff, b'C', 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    let decoded = Channel::parse(&raw).unwrap();
    assert_eq!(decoded.raw(), &raw);
    assert!(decoded.is_active());
    assert_eq!(decoded.operation_mode(), Some(OperationMode::ReceiveOnly));
    assert_eq!(decoded.rx_frequency_code(), 0x1234_5678);
    assert_eq!(decoded.tx_frequency_code(), 0x89ab_cdef);
    assert_eq!(decoded.rx_tone(), Some(Tone::Ctcss { tenths_hz: 885 }));
    assert_eq!(
        decoded.tx_tone(),
        Some(Tone::Dcs {
            code: 0o023,
            inverted: true
        })
    );
    assert_eq!(decoded.bandwidth(), Some(Bandwidth::Wide));
    assert_eq!(decoded.power(), Some(Power::High));
    assert!(decoded.scan_add());
    assert_eq!(decoded.name_bytes(), b"A\0B");
    assert_eq!(decoded.name_raw()[4], b'C');
}

#[test]
fn unknown_fields_are_observable_and_noop_is_identical() {
    let mut raw = [0xff; CHANNEL_LEN];
    let decoded = Channel::parse(&raw).unwrap();
    assert!(!decoded.is_active());
    assert_eq!(decoded.active_marker(), 0xff);
    assert_eq!(decoded.operation_mode(), None);
    assert_eq!(decoded.operation_mode_raw(), 0xff);
    assert_eq!(decoded.bandwidth(), None);
    assert_eq!(decoded.bandwidth_raw(), 0xff);
    assert_eq!(decoded.power(), None);
    assert_eq!(decoded.power_raw(), 0xff);
    assert_eq!(decoded.rx_tone(), None);
    assert_eq!(decoded.rx_tone_raw(), 0xffff);
    assert_eq!(decoded.tx_tone(), None);
    assert_eq!(decoded.tx_tone_raw(), 0xffff);
    assert_eq!(decoded.tx_frequency_code(), u32::MAX);
    assert_eq!(decoded.name_bytes(), b"");
    patch_record(&mut raw, &ChannelPatch::default()).unwrap();
    assert_eq!(raw, [0xff; CHANNEL_LEN]);
}

#[test]
fn frequency_patch_preserves_every_other_byte_and_inactive_marker() {
    let mut raw = patterned_record();
    let mut expected = raw;
    expected[6..10].copy_from_slice(&[0xef, 0xbe, 0xad, 0xde]);
    patch_record(
        &mut raw,
        &ChannelPatch {
            rx_frequency_code: Some(0xdead_beef),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(raw, expected);
    assert!(!Channel::parse(&raw).unwrap().is_active());
}

#[test]
fn tx_frequency_patch_changes_exact_four_bytes() {
    let mut raw = patterned_record();
    let mut expected = raw;
    expected[10..14].copy_from_slice(&[0xef, 0xbe, 0xad, 0xde]);
    patch_record(
        &mut raw,
        &ChannelPatch {
            tx_frequency_code: Some(0xdead_beef),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(raw, expected);
}

#[test]
fn scan_patch_preserves_all_lower_bits_for_every_original_byte() {
    for original in 0..=u8::MAX {
        for add in [true, false] {
            let mut raw = patterned_record();
            raw[19] = original;
            let before = raw;
            patch_record(
                &mut raw,
                &ChannelPatch {
                    scan_add: Some(add),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(raw[19] & 0x7f, original & 0x7f);
            assert_eq!(Channel::parse(&raw).unwrap().scan_add(), add);
            assert_eq!(&raw[..19], &before[..19]);
            assert_eq!(&raw[20..], &before[20..]);
        }
    }
}

#[test]
fn explicit_known_enums_replace_only_their_fields() {
    let mut raw = patterned_record();
    let mut expected = raw;
    expected[1] = 1;
    expected[3] = 1;
    expected[16] = 0;
    patch_record(
        &mut raw,
        &ChannelPatch {
            operation_mode: Some(OperationMode::ReceiveOnly),
            bandwidth: Some(Bandwidth::Narrow),
            power: Some(Power::Low),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(raw, expected);
    assert_eq!(
        Channel::parse(&raw).unwrap().tx_frequency_code(),
        Channel::parse(&expected).unwrap().tx_frequency_code()
    );
}

#[test]
fn name_patch_is_exact_and_padding_does_not_truncate() {
    assert_eq!(padded_name(b"").unwrap(), [0xff; NAME_LEN]);
    assert_eq!(padded_name(&[0xa1; NAME_LEN]).unwrap(), [0xa1; NAME_LEN]);
    assert_eq!(
        padded_name(&[0xa1; NAME_LEN + 1]),
        Err(ChannelError::NameTooLong {
            actual: NAME_LEN + 1
        })
    );
    // Raw GBK bytes are accepted, without interpreting or dropping NUL.
    let name = padded_name(&[0xb2, 0xe2, 0xca, 0xd4, 0, b'A']).unwrap();
    assert_eq!(&name[..6], &[0xb2, 0xe2, 0xca, 0xd4, 0, b'A']);
    assert_eq!(&name[6..], &[0xff; 10]);
    let mut raw = patterned_record();
    let before = raw;
    patch_record(
        &mut raw,
        &ChannelPatch {
            name_raw: Some(name),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(&raw[..32], &before[..32]);
    assert_eq!(Channel::parse(&raw).unwrap().name_raw(), &name);
}

#[test]
fn tones_match_the_source_encoding_vectors() {
    for (tone, word) in [
        (Tone::None, 0x0000),
        (Tone::Ctcss { tenths_hz: 670 }, 0x129e),
        (Tone::Ctcss { tenths_hz: 885 }, 0x1375),
        (Tone::Ctcss { tenths_hz: 2541 }, 0x19ed),
        (
            Tone::Dcs {
                code: 0o023,
                inverted: false,
            },
            0x2013,
        ),
        (
            Tone::Dcs {
                code: 0o023,
                inverted: true,
            },
            0x3013,
        ),
        (
            Tone::Dcs {
                code: 0o777,
                inverted: false,
            },
            0x21ff,
        ),
        (
            Tone::Dcs {
                code: 0o777,
                inverted: true,
            },
            0x31ff,
        ),
    ] {
        assert_eq!(tone.encode(), Ok(word));
        assert_eq!(Tone::decode(word), Some(tone));
    }
}

#[test]
fn all_decodable_tone_words_roundtrip_without_normalization() {
    let mut recognized = 0;
    for word in 0..=u16::MAX {
        if let Some(tone) = Tone::decode(word) {
            recognized += 1;
            assert_eq!(tone.encode(), Ok(word));
        }
    }
    assert_eq!(recognized, 1 + 4096 + 512 + 512);
    for unknown in [0x0001, 0x0fff, 0x2200, 0x3200, 0x4000, 0x7013, 0xffff] {
        assert_eq!(Tone::decode(unknown), None);
    }
}

#[test]
fn tone_patch_replaces_only_the_explicit_words() {
    let mut raw = patterned_record();
    let mut expected = raw;
    expected[4..6].copy_from_slice(&[0x13, 0x20]);
    expected[14..16].copy_from_slice(&[0x75, 0x13]);
    patch_record(
        &mut raw,
        &ChannelPatch {
            rx_tone: Some(Tone::Dcs {
                code: 0o023,
                inverted: false,
            }),
            tx_tone: Some(Tone::Ctcss { tenths_hz: 885 }),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(raw, expected);
}

#[test]
fn invalid_tone_rolls_back_the_whole_patch_before_any_mutation() {
    for (invalid, expected_error) in [
        (
            Tone::Ctcss { tenths_hz: 4096 },
            ChannelError::CtcssOutOfRange { tenths_hz: 4096 },
        ),
        (
            Tone::Dcs {
                code: 512,
                inverted: false,
            },
            ChannelError::DcsOutOfRange { code: 512 },
        ),
    ] {
        let mut raw = patterned_record();
        let before = raw;
        let patch = ChannelPatch {
            rx_frequency_code: Some(0),
            name_raw: Some([0; NAME_LEN]),
            rx_tone: Some(Tone::None),
            tx_tone: Some(invalid),
            ..Default::default()
        };
        assert_eq!(patch_record(&mut raw, &patch), Err(expected_error));
        assert_eq!(raw, before);
    }
}

#[test]
fn channel_index_and_section_length_are_checked_before_arithmetic() {
    let mut section = vec![0x5a; CHANNELS_LEN];
    let patch = ChannelPatch {
        power: Some(Power::High),
        ..Default::default()
    };
    for index in [CHANNEL_COUNT, usize::MAX] {
        assert!(
            matches!(channel(&section, index), Err(ChannelError::IndexOutOfRange { index: got }) if got == index)
        );
        assert_eq!(
            patch_channel(&mut section, index, &patch),
            Err(ChannelError::IndexOutOfRange { index })
        );
    }
    assert_eq!(section, vec![0x5a; CHANNELS_LEN]);
    assert!(matches!(
        channel(&section[..CHANNELS_LEN - 1], 0),
        Err(ChannelError::WrongSectionLength { .. })
    ));
    assert!(matches!(
        patch_channel(&mut section[..CHANNELS_LEN - 1], 0, &patch),
        Err(ChannelError::WrongSectionLength { .. })
    ));
    assert!(matches!(
        Channel::parse(&[0; CHANNEL_LEN - 1]),
        Err(ChannelError::WrongRecordLength { .. })
    ));
    assert!(matches!(
        patch_record(&mut [0; CHANNEL_LEN + 1], &patch),
        Err(ChannelError::WrongRecordLength { .. })
    ));
}

#[test]
fn last_channel_patch_leaves_all_other_records_identical() {
    let mut section = vec![0xa5; CHANNELS_LEN];
    patch_channel(
        &mut section,
        CHANNEL_COUNT - 1,
        &ChannelPatch {
            power: Some(Power::Medium),
            ..Default::default()
        },
    )
    .unwrap();
    let offset = (CHANNEL_COUNT - 1) * CHANNEL_LEN + 16;
    assert!(section[..offset].iter().all(|&value| value == 0xa5));
    assert_eq!(section[offset], 1);
    assert!(section[offset + 1..].iter().all(|&value| value == 0xa5));
    assert_eq!(
        channel(&section, CHANNEL_COUNT - 1).unwrap().power(),
        Some(Power::Medium)
    );
}

#[test]
fn enum_decoders_accept_only_proven_indices() {
    for raw in 0..=u8::MAX {
        assert_eq!(Bandwidth::decode(raw).is_some(), raw <= 1);
        assert_eq!(Power::decode(raw).is_some(), raw <= 2);
        assert_eq!(OperationMode::decode(raw).is_some(), raw <= 2);
        if let Some(value) = Bandwidth::decode(raw) {
            assert_eq!(value.encode(), raw);
        }
        if let Some(value) = Power::decode(raw) {
            assert_eq!(value.encode(), raw);
        }
        if let Some(value) = OperationMode::decode(raw) {
            assert_eq!(value.encode(), raw);
        }
    }
}
