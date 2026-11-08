use uv98_stock_model::general::{
    battery_adc_code, battery_indicator_code, general_read_range, select_stored_source,
    squelch_words, vox_hysteresis, StoredSource,
};

fn rows(csv: &str, expected: usize) -> impl Iterator<Item = Vec<u32>> {
    let parsed: Vec<Vec<u32>> = csv
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.split(',')
                .map(|field| field.parse().unwrap())
                .collect()
        })
        .collect();
    assert_eq!(parsed.len(), expected, "Missing reference oracle vectors");
    parsed.into_iter()
}

#[test]
fn source_selection_matches_executed_stock() {
    for row in rows(include_str!("fixtures/general_selection.csv"), 768) {
        let source = select_stored_source(row[0] as u8, row[1] as u8);
        let stock_source = match source {
            StoredSource::Primary => 0,
            StoredSource::Backup => 1,
            StoredSource::NoValidStoredCopy => 2,
        };
        assert_eq!(stock_source, row[2], "{row:?}");
        match general_read_range(source) {
            Some(range) => {
                assert_eq!(range.length, 32);
                assert_eq!(range.address, row[4]);
            }
            None => assert_eq!(row[2], 2),
        }
        assert_eq!(row[3], if row[0] == 0x9A { 1 } else { 2 });
    }
}

#[test]
fn invalid_markers_never_provide_defaults_or_read_plan() {
    for primary in 0..=255 {
        for backup in 0..=255 {
            if primary != 0x9A && backup != 0x9A {
                let source = select_stored_source(primary, backup);
                assert_eq!(source, StoredSource::NoValidStoredCopy);
                assert_eq!(general_read_range(source), None);
            }
        }
    }
}

#[test]
fn adc_conversion_matches_executed_stock() {
    for row in rows(include_str!("fixtures/general_battery_adc.csv"), 4100) {
        assert_eq!(battery_adc_code(row[0] as u16) as u32, row[1], "{row:?}");
    }
}

#[test]
fn battery_indicator_matches_all_byte_inputs() {
    for row in rows(include_str!("fixtures/general_battery_indicator.csv"), 256) {
        assert_eq!(
            battery_indicator_code(row[0] as u8) as u32,
            row[1],
            "{row:?}"
        );
    }
}

#[test]
fn vox_hysteresis_matches_executed_stock_at_boundaries() {
    for row in rows(include_str!("fixtures/general_vox.csv"), 6108) {
        assert_eq!(
            vox_hysteresis(row[1] as u16, row[0] as u8, row[2] != 0) as u32,
            row[3],
            "{row:?}"
        );
    }
}

#[test]
fn squelch_words_match_executed_stock_for_all_codes_and_levels() {
    for row in rows(include_str!("fixtures/general_squelch.csv"), 2816) {
        let result = squelch_words(row[0] as u8, row[1] as u8, row[2] as u8).unwrap();
        assert_eq!(result.noise_4f as u32, row[3], "{row:?}");
        assert_eq!(result.squelch_78 as u32, row[4], "{row:?}");
    }
    for invalid_level in 11..=255 {
        assert_eq!(squelch_words(invalid_level, 0, 255), None);
    }
}
