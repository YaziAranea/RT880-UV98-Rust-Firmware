use uv98_calibration::{Calibration, BLOCK_LEN};
use uv98_stock_model::frequency::{correct_code, select};

#[test]
fn selection_matches_reference_machine_oracle() {
    // Values were captured from execution of the original Thumb selector in
    // an offline emulator, with the SPI callee replaced by a RAM fixture.
    let csv = include_str!("../testfixtures/frequency-selection.csv");
    let mut count = 0;
    for line in csv.lines().skip(1) {
        let columns: Vec<_> = line.split(',').collect();
        let frequency: u32 = columns[0].parse().unwrap();
        let valid = columns[2] == "1";
        match select(frequency) {
            Some(selection) => {
                assert!(valid, "{line}");
                assert_eq!(
                    selection.band_index(),
                    columns[3].parse::<usize>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    selection.table_index(),
                    columns[4].parse::<usize>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    selection.byte_offset(),
                    columns[5].parse::<usize>().unwrap(),
                    "{line}"
                );
            }
            None => assert!(!valid, "{line}"),
        }
        count += 1;
    }
    assert_eq!(
        count, 694,
        "Expected boundary and deterministic-random oracle coverage"
    );
}

#[test]
fn correction_matches_both_reference_register_paths() {
    let csv = include_str!("../testfixtures/frequency-correction.csv");
    let mut count = 0;
    for line in csv.lines().skip(1) {
        let columns: Vec<_> = line.split(',').collect();
        let result = correct_code(columns[1].parse().unwrap(), columns[2].parse().unwrap());
        assert_eq!(result.code, columns[3].parse::<u32>().unwrap(), "{line}");
        assert_eq!(result.reg38, columns[4].parse::<u16>().unwrap(), "{line}");
        assert_eq!(result.reg39, columns[5].parse::<u16>().unwrap(), "{line}");
        count += 1;
    }
    assert_eq!(count, 96);
}

#[test]
fn selected_offset_integrates_with_saved_calibration_without_mutation() {
    let mut bytes = [0xA5; BLOCK_LEN];
    bytes[0] = 0x9A;
    let selection = select(43_350_000).unwrap();
    let offset = selection.byte_offset();
    bytes[offset..offset + 2].copy_from_slice(&32_760u16.to_le_bytes());
    let original = bytes;
    let calibration = Calibration::parse(&bytes).unwrap();
    let band = calibration.band(selection.band_index()).unwrap();
    assert_eq!(band.byte_offset(), offset);
    assert_eq!(
        correct_code(43_350_000, band.frequency_offset_raw()).code,
        43_349_992
    );
    assert_eq!(bytes, original);
}
