use uv98_calibration::{Calibration, Error, Filter, Table, BAND_COUNT, BLOCK_LEN, MARKER};

const GOLDEN: &[u8; BLOCK_LEN] = include_bytes!("fixtures/uv98-plus-stock-1.16.bin");

#[test]
fn real_backup_general_fields_match_independent_offline_decode() {
    let calibration = Calibration::parse(GOLDEN).unwrap();
    let general = calibration.general();
    assert_eq!(calibration.first_byte(), 0x9A);
    assert_eq!(general.test_mode_raw(), 0);
    assert_eq!(general.reg_3e_raw(), 0xA037);
    assert_eq!(general.reg_3e_ui_index(), 1);
    assert_eq!(general.expander().raw(), 0x6B38);
    assert_eq!(general.expander().mode_code(), 1);
    assert_eq!(general.expander().af_code(), 86);
    assert_eq!(general.expander().noise_code(), 56);
    assert_eq!(general.compressor().raw(), 0xAB40);
    assert_eq!(general.compressor().mode_code(), 2);
    assert_eq!(general.filter_code(Filter::Tx300), 6);
    assert_eq!(general.filter_code(Filter::Tx3k), 0);
    assert_eq!(general.filter_code(Filter::Rx300), 6);
    assert_eq!(general.filter_code(Filter::Rx3k), 4);
    assert_eq!(general.vox_on_raw(), 10);
    assert_eq!(general.vox_off_raw(), 5);
    assert_eq!(
        general.battery_points_raw(),
        &[58, 60, 63, 65, 68, 71, 74, 75, 76, 77, 78, 79, 80, 81, 82]
    );
    assert_eq!(general.adc_correction_raw(), 128);
    assert_eq!(general.adc_correction_code(), 0);
}

#[test]
fn real_backup_preserves_factory_variation_and_unused_points() {
    let calibration = Calibration::parse(GOLDEN).unwrap();
    let first = calibration.band(0).unwrap();
    let second = calibration.band(1).unwrap();
    assert_eq!(first.frequency_offset_code(), 27);
    assert_eq!(second.frequency_offset_code(), 65);
    assert_eq!(first.wide().tx_deviation_raw(), 0x34E2);
    assert_eq!(first.wide().tx_deviation_code(), 1250);
    assert_eq!(first.narrow().volume_raw(), 0xB34F);
    assert_eq!(first.narrow().volume_gain_code(), 52);
    assert_eq!(first.narrow().volume_dac_code(), 15);
    assert_eq!(first.valid_points(), 8);
    assert_eq!(
        first.table_raw(Table::PowerHigh),
        &[42, 42, 40, 40, 40, 41, 42, 43, 55, 55, 55, 55, 55, 55, 55, 55]
    );
    assert_eq!(first.point_raw(Table::PowerHigh, 15), Some(55));
    assert_eq!(
        second.table_raw(Table::PowerLow),
        &[28, 27, 26, 25, 24, 23, 23, 23, 23, 22, 22, 22, 22, 22, 22, 22]
    );
}

#[test]
fn all_views_borrow_original_bytes_including_last_slot_and_tail() {
    let calibration = Calibration::parse(GOLDEN).unwrap();
    assert!(core::ptr::eq(calibration.raw(), GOLDEN));
    assert_eq!(calibration.general().raw().as_ptr(), GOLDEN.as_ptr());
    let counts = [8, 16, 10, 16, 15, 16, 16, 16, 16, 16, 16, 16, 16, 16, 12];
    for (index, count) in counts.into_iter().enumerate() {
        let band = calibration.band(index).unwrap();
        let start = 32 + index * 112;
        assert_eq!(band.index(), index);
        assert_eq!(band.byte_offset(), start);
        assert_eq!(band.valid_points(), count);
        assert_eq!(band.raw().as_slice(), &GOLDEN[start..start + 112]);
        assert_eq!(band.raw().as_ptr(), GOLDEN[start..].as_ptr());
    }
    let last = calibration.band(14).unwrap();
    assert_eq!(last.frequency_offset_code(), 0);
    assert_eq!(last.table_raw(Table::SquelchNarrow), &[75; 16]);
    assert_eq!(
        calibration.uninterpreted_tail().as_slice(),
        &GOLDEN[1712..4096]
    );
    assert_eq!(
        calibration.uninterpreted_tail().as_ptr(),
        GOLDEN[1712..].as_ptr()
    );
}

#[test]
fn rejects_truncation_empty_input_and_appended_data_including_updat() {
    for length in [0, 1, 32, 1712, 4095] {
        assert_eq!(
            Calibration::parse(&GOLDEN[..length]).unwrap_err(),
            Error::WrongLength { actual: length }
        );
    }
    for length in [4097, 4119, 8192] {
        let mut data = vec![0u8; length];
        data[0] = MARKER;
        assert_eq!(
            Calibration::parse(&data).unwrap_err(),
            Error::WrongLength { actual: length }
        );
    }
}

#[test]
fn rejects_wrong_marker_including_erased_and_default_image_markers() {
    for marker in [0, 1, 0x99, 0x9B, 0xFF] {
        let mut data = *GOLDEN;
        data[0] = marker;
        assert_eq!(
            Calibration::parse(&data).unwrap_err(),
            Error::InvalidMarker { found: marker }
        );
    }
}

#[test]
fn invalid_indices_are_rejected_before_arithmetic() {
    let calibration = Calibration::parse(GOLDEN).unwrap();
    for index in [BAND_COUNT, 16, usize::MAX] {
        assert_eq!(
            calibration.band(index).unwrap_err(),
            Error::BandOutOfRange { index }
        );
    }
    let band = calibration.band(14).unwrap();
    for index in [16, 17, usize::MAX] {
        assert_eq!(band.point_raw(Table::SquelchNarrow, index), None);
    }
}

#[test]
fn preserves_unknown_bits_and_arbitrary_nonmarker_changes_without_repair() {
    let mut data = *GOLDEN;
    data[1] = 0xFF;
    data[8] = 0xFE;
    data[31] = 0;
    data[35..41].copy_from_slice(&[0xBC, 0xFA, 0xFF, 0x80, 0xCB, 0xED]);
    data[1712] = 0xA5;
    data[4095] = 0x5A;
    let original = data;
    let calibration = Calibration::parse(&data).unwrap();
    assert_eq!(calibration.general().test_mode_raw(), 255);
    assert_eq!(calibration.general().test_mode_ui_index(), 1);
    assert_eq!(calibration.general().filter_raw(Filter::Tx300), 0xFE);
    assert_eq!(calibration.general().filter_code(Filter::Tx300), 6);
    assert_eq!(calibration.general().adc_correction_code(), -128);
    let width = calibration.band(0).unwrap().wide();
    assert_eq!(width.tx_deviation_raw(), 0xFABC);
    assert_eq!(width.tx_deviation_code(), 0xABC);
    assert_eq!(width.ctcss_deviation_raw(), 0xFF);
    assert_eq!(width.ctcss_deviation_code(), 127);
    assert_eq!(width.dcs_deviation_raw(), 0x80);
    assert_eq!(width.dcs_deviation_code(), 0);
    assert_eq!(width.volume_raw(), 0xEDCB);
    assert_eq!(width.volume_gain_code(), 28);
    assert_eq!(width.volume_dac_code(), 11);
    assert_eq!(calibration.uninterpreted_tail()[0], 0xA5);
    assert_eq!(calibration.uninterpreted_tail()[2383], 0x5A);
    assert_eq!(calibration.raw(), &original);
}

#[test]
fn wide_and_narrow_use_distinct_adjacent_seven_byte_groups() {
    let mut data = *GOLDEN;
    let wide_raw = [0x12, 0xBC, 0xFA, 0xFF, 0x80, 0xCB, 0xED];
    let narrow_raw = [0x34, 0x23, 0x41, 0x81, 0xFE, 0x56, 0xA7];
    data[34..41].copy_from_slice(&wide_raw);
    data[41..48].copy_from_slice(&narrow_raw);
    let calibration = Calibration::parse(&data).unwrap();
    let band = calibration.band(0).unwrap();

    assert_eq!(band.wide().raw(), &wide_raw);
    assert_eq!(band.narrow().raw(), &narrow_raw);
    assert_eq!(band.wide().raw().as_ptr(), data[34..].as_ptr());
    assert_eq!(band.narrow().raw().as_ptr(), data[41..].as_ptr());
    assert_eq!(band.wide().mic_gain_raw(), 0x12);
    assert_eq!(band.narrow().mic_gain_raw(), 0x34);
    assert_eq!(band.wide().tx_deviation_code(), 0xABC);
    assert_eq!(band.narrow().tx_deviation_code(), 0x123);
    assert_eq!(band.wide().ctcss_deviation_code(), 127);
    assert_eq!(band.narrow().ctcss_deviation_code(), 1);
    assert_eq!(band.wide().dcs_deviation_code(), 0);
    assert_eq!(band.narrow().dcs_deviation_code(), 126);
    assert_eq!(band.wide().volume_gain_code(), 28);
    assert_eq!(band.narrow().volume_gain_code(), 53);
    assert_eq!(band.wide().volume_dac_code(), 11);
    assert_eq!(band.narrow().volume_dac_code(), 6);
    // The narrow group ends immediately before the first table, without overlap.
    assert_eq!(band.table_raw(Table::PowerHigh)[0], GOLDEN[48]);
}

#[test]
fn offset_binary_endpoints_do_not_wrap() {
    for (raw, expected) in [(0u16, -32768), (32767, -1), (32768, 0), (65535, 32767)] {
        let mut data = *GOLDEN;
        data[32..34].copy_from_slice(&raw.to_le_bytes());
        let calibration = Calibration::parse(&data).unwrap();
        assert_eq!(calibration.band(0).unwrap().frequency_offset_raw(), raw);
        assert_eq!(
            calibration.band(0).unwrap().frequency_offset_code(),
            expected
        );
    }
}

#[test]
fn all_six_tables_keep_row_order_and_last_point() {
    let tables = [
        Table::PowerHigh,
        Table::PowerLow,
        Table::NoiseWide,
        Table::SquelchWide,
        Table::NoiseNarrow,
        Table::SquelchNarrow,
    ];
    let mut data = *GOLDEN;
    for (row, _) in tables.iter().enumerate() {
        let start = 1600 + 16 + row * 16;
        data[start] = 100 + row as u8;
        data[start + 15] = 200 + row as u8;
    }
    let calibration = Calibration::parse(&data).unwrap();
    let last = calibration.band(14).unwrap();
    for (row, table) in tables.into_iter().enumerate() {
        assert_eq!(last.point_raw(table, 0), Some(100 + row as u8));
        assert_eq!(last.point_raw(table, 15), Some(200 + row as u8));
    }
}
