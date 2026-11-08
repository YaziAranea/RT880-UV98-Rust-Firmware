use uv98_calibration::{
    Calibration, Filter, Table, BAND_COUNT, BLOCK_LEN, KNOWN_LAYOUT_END, MARKER,
};
use uv98_calibration_edit::{
    band_label, descriptor, field_at, field_count, fields, Change, ChannelWidth, Consumption,
    Document, Error, Field, GeneralField, Group, WidthField,
};

const GOLDEN: &[u8; BLOCK_LEN] =
    include_bytes!("../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin");

fn patterned() -> [u8; BLOCK_LEN] {
    let mut data = [0; BLOCK_LEN];
    for (index, byte) in data.iter_mut().enumerate() {
        *byte = (index.wrapping_mul(37) + 11) as u8;
    }
    data[0] = MARKER;
    data
}
fn width(band: usize, width: ChannelWidth, parameter: WidthField) -> Field {
    Field::WidthParameter {
        band,
        width,
        parameter,
    }
}

#[test]
fn descriptor_registry_is_complete_unique_and_has_only_expected_bit_coverage() {
    assert_eq!(field_count(), 1666);
    assert_eq!(fields().len(), field_count());
    let mut coverage = [0u8; BLOCK_LEN];
    let mut editable = 0;
    for (index, field) in fields().enumerate() {
        assert_eq!(field_at(index), Some(field));
        let d = descriptor(field).unwrap();
        assert_eq!(d.field, field);
        assert!(!d.label.is_empty());
        assert!(!d.detail.is_empty());
        assert!((1..=2).contains(&d.width));
        assert!(d.offset + usize::from(d.width) <= KNOWN_LAYOUT_END);
        assert_eq!(d.min, d.bias);
        assert_eq!(d.max - d.bias, i32::from(d.mask >> d.shift));
        assert!(d.mask != 0);
        if d.width == 1 {
            assert_eq!(d.mask & 0xff00, 0);
        }
        assert_eq!(
            d.group,
            if d.offset < 32 {
                Group::General
            } else {
                Group::Band {
                    index: (d.offset - 32) / 112,
                }
            }
        );
        for byte in 0..usize::from(d.width) {
            let mask = (d.mask >> (byte * 8)) as u8;
            assert_eq!(coverage[d.offset + byte] & mask, 0, "overlap at {field:?}");
            coverage[d.offset + byte] |= mask;
        }
        editable += usize::from(d.editable);
    }
    assert_eq!(editable, 1550); // 2 control bytes + 19 hidden points * 6 rows are read-only.
    assert!(coverage[..32].iter().all(|&mask| mask == 0xff));
    for band in 0..15 {
        for offset in 0..112 {
            let expected = match offset {
                5 | 6 | 12 | 13 => 0x7f,
                8 | 15 => 0x03,
                _ => 0xff,
            };
            assert_eq!(
                coverage[32 + band * 112 + offset],
                expected,
                "band {band} offset {offset}"
            );
        }
    }
    assert!(coverage[KNOWN_LAYOUT_END..].iter().all(|&mask| mask == 0));
    assert_eq!(field_at(field_count()), None);
    assert_eq!(field_at(usize::MAX), None);
    let mut iterator = fields();
    for _ in 0..field_count() {
        assert!(iterator.next().is_some());
    }
    assert_eq!(iterator.size_hint(), (0, Some(0)));
    assert_eq!(iterator.next(), None);
    assert_eq!(iterator.next(), None);
}

#[test]
fn descriptor_offsets_and_valid_points_match_existing_parser_without_defaults() {
    let parsed = Calibration::parse(GOLDEN).unwrap();
    let document = Document::new(GOLDEN).unwrap();
    assert_eq!(
        document
            .value(Field::General(GeneralField::ExpanderModeCode))
            .unwrap(),
        1
    );
    assert_eq!(
        document
            .value(Field::General(GeneralField::ExpanderAfCode))
            .unwrap(),
        86
    );
    assert_eq!(
        document
            .value(Field::General(GeneralField::ExpanderNoiseCode))
            .unwrap(),
        56
    );
    assert_eq!(
        document
            .value(Field::General(GeneralField::AdcCorrectionCode))
            .unwrap(),
        0
    );
    assert_eq!(
        document
            .raw_word(Field::General(GeneralField::Register3eRaw))
            .unwrap(),
        0xa037
    );
    for band in 0..BAND_COUNT {
        let view = parsed.band(band).unwrap();
        assert_eq!(
            document.value(Field::FrequencyOffset { band }).unwrap(),
            view.frequency_offset_code()
        );
        for (kind, group) in [
            (ChannelWidth::Wide, view.wide()),
            (ChannelWidth::Narrow, view.narrow()),
        ] {
            assert_eq!(
                document
                    .value(width(band, kind, WidthField::MicGain))
                    .unwrap(),
                i32::from(group.mic_gain_raw())
            );
            assert_eq!(
                document
                    .value(width(band, kind, WidthField::TxDeviationRaw))
                    .unwrap(),
                i32::from(group.tx_deviation_raw())
            );
            assert_eq!(
                document
                    .value(width(band, kind, WidthField::VolumeGainCode))
                    .unwrap(),
                i32::from(group.volume_gain_code())
            );
            assert_eq!(
                document
                    .value(width(band, kind, WidthField::VolumeDacCode))
                    .unwrap(),
                i32::from(group.volume_dac_code())
            );
        }
        for table in [
            Table::PowerHigh,
            Table::PowerLow,
            Table::NoiseWide,
            Table::SquelchWide,
            Table::NoiseNarrow,
            Table::SquelchNarrow,
        ] {
            for point in 0..16 {
                let field = Field::TablePoint { band, table, point };
                assert_eq!(
                    document.value(field).unwrap(),
                    i32::from(view.point_raw(table, point).unwrap())
                );
                assert_eq!(
                    descriptor(field).unwrap().editable,
                    point < view.valid_points()
                );
            }
        }
    }
    assert_eq!(document.original(), GOLDEN);
    assert_eq!(document.export_bytes(), GOLDEN);
    assert!(!document.is_dirty());
    assert_eq!(document.dirty_count(), 0);
    assert_eq!(core::mem::size_of::<Document>(), 2 * BLOCK_LEN);
}

#[test]
fn every_editable_field_changes_only_its_mask_and_preserves_original_and_tail() {
    let original = patterned();
    let mut document = Document::new(&original).unwrap();
    for field in fields() {
        let d = descriptor(field).unwrap();
        if !d.editable {
            continue;
        }
        let previous = document.value(field).unwrap();
        let desired = if previous == d.min { d.max } else { d.min };
        assert!(document.set(field, desired).unwrap());
        assert_eq!(document.value(field).unwrap(), desired);
        assert_eq!(document.original_value(field).unwrap(), previous);
        assert_eq!(document.original(), &original);
        for offset in 0..BLOCK_LEN {
            let permitted = if (d.offset..d.offset + usize::from(d.width)).contains(&offset) {
                (d.mask >> ((offset - d.offset) * 8)) as u8
            } else {
                0
            };
            assert_eq!(
                (document.bytes()[offset] ^ original[offset]) & !permitted,
                0,
                "field {field:?} modified unrelated bits at {offset}"
            );
        }
        assert_eq!(&document.bytes()[1712..], &original[1712..]);
        document.reset_all();
        assert_eq!(document.bytes(), &original);
    }
}

#[test]
fn all_storage_boundaries_reject_overflow_without_changing_any_byte() {
    let original = patterned();
    let mut document = Document::new(&original).unwrap();
    for field in fields() {
        let d = descriptor(field).unwrap();
        if !d.editable {
            continue;
        }
        for value in [d.min - 1, d.max + 1, i32::MIN, i32::MAX] {
            assert_eq!(
                document.set(field, value),
                Err(Error::ValueOutOfRange {
                    field,
                    value,
                    min: d.min,
                    max: d.max
                })
            );
            assert_eq!(document.bytes(), &original);
        }
        document.set(field, d.min).unwrap();
        assert_eq!(document.value(field).unwrap(), d.min);
        document.set(field, d.max).unwrap();
        assert_eq!(document.value(field).unwrap(), d.max);
        document.reset_all();
    }
}

#[test]
fn masked_edits_preserve_reserved_bits_without_vendor_constants() {
    let mut original = patterned();
    original[37] = 0xff; // Wide CTCSS.
    original[38] = 0x80; // Wide DCS.
    original[39..41].copy_from_slice(&0xedcbu16.to_le_bytes());
    let mut document = Document::new(&original).unwrap();
    let ctcss = width(0, ChannelWidth::Wide, WidthField::CtcssDeviationCode);
    let dcs = width(0, ChannelWidth::Wide, WidthField::DcsDeviationCode);
    let gain = width(0, ChannelWidth::Wide, WidthField::VolumeGainCode);
    let dac = width(0, ChannelWidth::Wide, WidthField::VolumeDacCode);
    assert_eq!(
        document
            .apply(&[
                Change {
                    field: ctcss,
                    value: 1
                },
                Change {
                    field: dcs,
                    value: 127
                },
                Change {
                    field: gain,
                    value: 1
                },
                Change {
                    field: dac,
                    value: 9
                }
            ])
            .unwrap(),
        4
    );
    assert_eq!(document.bytes()[37], 0x81);
    assert_eq!(document.bytes()[38], 0xff);
    assert_eq!(document.raw_word(gain).unwrap(), 0xec19);
    assert_eq!(document.raw_word(gain).unwrap() & 0xfc00, 0xec00); // Not forced to vendor 0xB000.
    document.reset_field(gain).unwrap();
    assert_eq!(document.raw_word(gain).unwrap(), 0xedc9); // DAC edit survives.
    document.reset_field(dac).unwrap();
    assert_eq!(document.raw_word(gain).unwrap(), 0xedcb);
    assert_eq!(document.original(), &original);
}

#[test]
fn full_deviation_and_filter_edits_do_not_reuse_display_masks() {
    let mut original = patterned();
    original[35..37].copy_from_slice(&0xfabcu16.to_le_bytes());
    original[8] = 0xfe;
    let mut document = Document::new(&original).unwrap();
    let deviation = width(0, ChannelWidth::Wide, WidthField::TxDeviationRaw);
    let filter = Field::Filter(Filter::Tx300);
    assert_eq!(document.value(deviation).unwrap(), 0xfabc);
    assert_eq!(document.value(filter).unwrap(), 0xfe);
    document.set(deviation, 0xe123).unwrap();
    document.set(filter, 0xf0).unwrap();
    assert_eq!(document.raw_word(deviation).unwrap(), 0xe123); // Not 0x3123 or 0x0123.
    assert_eq!(document.bytes()[8], 0xf0); // Not 0.
    let before = *document.bytes();
    assert!(document.set(deviation, 65536).is_err());
    assert!(document.set(filter, 256).is_err());
    assert_eq!(document.bytes(), &before);
    document.reset_field(deviation).unwrap();
    document.reset_field(filter).unwrap();
    assert_eq!(document.bytes(), &original);
}

#[test]
fn signed_offset_codes_encode_exactly_and_reset_to_original() {
    let original = patterned();
    let mut document = Document::new(&original).unwrap();
    let frequency = Field::FrequencyOffset { band: 14 };
    let adc = Field::General(GeneralField::AdcCorrectionCode);
    for (value, raw) in [(-32768, 0), (0, 32768), (32767, 65535)] {
        document.set(frequency, value).unwrap();
        assert_eq!(document.raw_word(frequency).unwrap(), raw);
    }
    for (value, raw) in [(-128, 0), (0, 128), (127, 255)] {
        document.set(adc, value).unwrap();
        assert_eq!(document.raw_word(adc).unwrap(), raw);
    }
    document.reset_all();
    assert_eq!(document.bytes(), &original);
}

#[test]
fn invalid_batch_is_atomic_even_after_valid_fields_and_unknown_bits() {
    let original = patterned();
    let mut document = Document::new(&original).unwrap();
    let valid = Change {
        field: Field::BatteryPoint { index: 0 },
        value: 1,
    };
    let invalid = Change {
        field: width(14, ChannelWidth::Narrow, WidthField::VolumeDacCode),
        value: 16,
    };
    assert!(document.apply(&[valid, invalid]).is_err());
    assert_eq!(document.bytes(), &original);
    let invalid_index = Change {
        field: Field::FrequencyOffset { band: usize::MAX },
        value: 0,
    };
    assert_eq!(
        document.apply(&[valid, invalid_index]),
        Err(Error::BandOutOfRange { band: usize::MAX })
    );
    assert_eq!(document.bytes(), &original);
    assert_eq!(
        document.apply(&[valid, valid]),
        Err(Error::OverlappingChanges {
            first: 0,
            second: 1
        })
    );
    assert_eq!(document.bytes(), &original);
    let too_many = vec![valid; field_count() + 1];
    assert_eq!(
        document.apply(&too_many),
        Err(Error::TooManyChanges {
            count: field_count() + 1
        })
    );
    assert_eq!(document.bytes(), &original);
    assert_eq!(document.apply(&[]).unwrap(), 0);
}

#[test]
fn disjoint_compander_subfields_can_share_a_word_in_one_batch() {
    let original = patterned();
    let mut document = Document::new(&original).unwrap();
    let mode = Field::General(GeneralField::ExpanderModeCode);
    let af = Field::General(GeneralField::ExpanderAfCode);
    let noise = Field::General(GeneralField::ExpanderNoiseCode);
    document
        .apply(&[
            Change {
                field: mode,
                value: 3,
            },
            Change {
                field: af,
                value: 127,
            },
            Change {
                field: noise,
                value: 1,
            },
        ])
        .unwrap();
    assert_eq!(document.raw_word(mode).unwrap(), 0xff81);
    document.reset_field(mode).unwrap();
    assert_eq!(
        document.value(mode).unwrap(),
        document.original_value(mode).unwrap()
    );
    assert_eq!(document.value(af).unwrap(), 127);
    assert_eq!(document.value(noise).unwrap(), 1);
    assert_eq!(document.bytes()[6..8], original[6..8]);
}

#[test]
fn marker_test_mode_and_hidden_table_points_remain_read_only() {
    let original = patterned();
    let mut document = Document::new(&original).unwrap();
    let readonly = [
        Field::General(GeneralField::Marker),
        Field::General(GeneralField::TestMode),
        Field::TablePoint {
            band: 0,
            table: Table::PowerHigh,
            point: 8,
        },
        Field::TablePoint {
            band: 14,
            table: Table::SquelchNarrow,
            point: 15,
        },
    ];
    for field in readonly {
        assert!(!descriptor(field).unwrap().editable);
        assert_eq!(document.set(field, 0), Err(Error::ReadOnly { field }));
        assert_eq!(document.reset_field(field), Err(Error::ReadOnly { field }));
        assert_eq!(
            document.apply(&[
                Change {
                    field: Field::BatteryPoint { index: 0 },
                    value: 1
                },
                Change { field, value: 0 }
            ]),
            Err(Error::ReadOnly { field })
        );
        assert_eq!(document.bytes(), &original);
    }
    assert_eq!(
        descriptor(Field::General(GeneralField::VoxOnRaw))
            .unwrap()
            .consumption,
        Consumption::TuningLayoutOnly
    );
    assert_eq!(
        descriptor(Field::Filter(Filter::Tx3k)).unwrap().consumption,
        Consumption::RelatedStockTraced
    );
    assert_eq!(
        descriptor(Field::FrequencyOffset { band: 14 })
            .unwrap()
            .consumption,
        Consumption::TuningLayoutOnly
    );
    assert_eq!(
        descriptor(width(14, ChannelWidth::Wide, WidthField::TxDeviationRaw))
            .unwrap()
            .consumption,
        Consumption::TuningLayoutOnly
    );
}

#[test]
fn diffs_are_sorted_byte_changes_and_clones_keep_independent_working_copies() {
    let mut original = [0; BLOCK_LEN];
    original[0] = MARKER;
    let mut document = Document::new(&original).unwrap();
    let history = document.clone();
    let vox = Field::General(GeneralField::VoxOnRaw);
    assert!(document.set(vox, 65535).unwrap());
    assert!(!document.set(vox, 65535).unwrap());
    document.set(Field::Filter(Filter::Tx300), 1).unwrap();
    let differences: Vec<_> = document.diff().collect();
    assert_eq!(
        differences
            .iter()
            .map(|change| change.offset)
            .collect::<Vec<_>>(),
        [8, 12, 13]
    );
    assert_eq!(
        differences
            .iter()
            .map(|change| change.original)
            .collect::<Vec<_>>(),
        [0, 0, 0]
    );
    assert_eq!(
        differences
            .iter()
            .map(|change| change.working)
            .collect::<Vec<_>>(),
        [1, 255, 255]
    );
    assert_eq!(document.dirty_count(), 3);
    assert!(document.is_dirty());
    assert_eq!(history.bytes(), &original);
    assert_eq!(document.original(), &original);
    assert_eq!(document.reset_all(), 3);
    assert_eq!(document.reset_all(), 0);
    assert_eq!(document.export_bytes(), &original);
    let mut diff = document.diff();
    assert_eq!(diff.next(), None);
    assert_eq!(diff.next(), None);
    assert_eq!(diff.size_hint(), (0, Some(0)));
}

#[test]
fn import_is_exact_length_and_marker_only_not_a_checksum_or_repair() {
    for length in [0, 1, 1712, 4095, 4097] {
        let mut raw = vec![0; length];
        if length != 0 {
            raw[0] = MARKER;
        }
        assert!(
            matches!(Document::new(&raw), Err(Error::Source(uv98_calibration::Error::WrongLength { actual })) if actual == length)
        );
    }
    for marker in [0, 1, 0x99, 0xff] {
        let mut raw = patterned();
        raw[0] = marker;
        assert!(
            matches!(Document::new(&raw), Err(Error::Source(uv98_calibration::Error::InvalidMarker { found })) if found == marker)
        );
    }
    let mut arbitrary = patterned();
    arbitrary[1] = 0xff;
    arbitrary[4095] ^= 0x80;
    let document = Document::new(&arbitrary).unwrap();
    assert_eq!(document.bytes(), &arbitrary); // No guessed checksum, defaults or normalization.
}

#[test]
fn hostile_field_indices_are_rejected_before_offset_arithmetic() {
    for band in [15, 16, usize::MAX] {
        for field in [
            Field::FrequencyOffset { band },
            width(band, ChannelWidth::Narrow, WidthField::VolumeDacCode),
            Field::TablePoint {
                band,
                table: Table::SquelchNarrow,
                point: 0,
            },
        ] {
            assert_eq!(descriptor(field), Err(Error::BandOutOfRange { band }));
        }
    }
    for index in [15, 16, usize::MAX] {
        assert_eq!(
            descriptor(Field::BatteryPoint { index }),
            Err(Error::BatteryPointOutOfRange { index })
        );
    }
    for point in [16, 17, usize::MAX] {
        assert_eq!(
            descriptor(Field::TablePoint {
                band: 14,
                table: Table::PowerLow,
                point
            }),
            Err(Error::TablePointOutOfRange { point })
        );
    }
}

#[test]
fn source_tab_labels_retain_order_and_ambiguity_without_becoming_range_rules() {
    assert_eq!(band_label(0), Some("136-174"));
    assert_eq!(band_label(1), Some("400-480"));
    assert_eq!(band_label(2), Some("18-64"));
    assert_eq!(band_label(8), Some("560-640(660)"));
    assert_eq!(band_label(14), Some("1240-1300"));
    assert_eq!(band_label(15), None);
    assert_eq!(band_label(usize::MAX), None);
}
