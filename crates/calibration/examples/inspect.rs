//! Offline host inspection only: reads one file and prints to stdout.
//! No serial access, hardware access, or file writes.

use std::{env, fs, path::PathBuf, process::ExitCode};
use uv98_calibration::{Calibration, Filter, Table, BAND_COUNT, TABLE_LEN};

fn raw_range(values: &[u8; TABLE_LEN]) -> (u8, u8) {
    values
        .iter()
        .copied()
        .fold((values[0], values[0]), |(minimum, maximum), value| {
            (minimum.min(value), maximum.max(value))
        })
}

fn run() -> Result<(), String> {
    let mut args = env::args_os();
    let _program = args.next();
    let input = args
        .next()
        .map(PathBuf::from)
        .ok_or("Usage: inspect <raw-4096-byte-file>")?;
    if args.next().is_some() {
        return Err("Usage: inspect <raw-4096-byte-file> (exactly one path)".into());
    }
    let bytes =
        fs::read(&input).map_err(|error| format!("Cannot read {}: {error}", input.display()))?;
    let calibration = Calibration::parse(&bytes)
        .map_err(|error| format!("Invalid calibration block: {error}"))?;
    let general = calibration.general();

    println!("OFFLINE READ-ONLY: stored codes only, no physical units inferred.");
    println!("Input: {}", input.display());
    println!(
        "Block: {} bytes; marker=0x{:02X}; only size and marker validated.",
        calibration.raw().len(),
        calibration.first_byte()
    );
    println!("Layout: tuning tool 241219; revision compatibility and RF behavior unverified.");
    println!(
        "General: test_mode_raw={} reg3e_raw=0x{:04X} ADC_raw={} ADC_code={} VOX_on_raw={} VOX_off_raw={}",
        general.test_mode_raw(),
        general.reg_3e_raw(),
        general.adc_correction_raw(),
        general.adc_correction_code(),
        general.vox_on_raw(),
        general.vox_off_raw()
    );
    println!("Battery raw points: {:?}", general.battery_points_raw());
    for (name, filter) in [
        ("TX300", Filter::Tx300),
        ("TX3k", Filter::Tx3k),
        ("RX300", Filter::Rx300),
        ("RX3k", Filter::Rx3k),
    ] {
        println!(
            "Filter {name}: raw={} code3bit={}",
            general.filter_raw(filter),
            general.filter_code(filter)
        );
    }
    for (name, compander) in [
        ("Expander", general.expander()),
        ("Compressor", general.compressor()),
    ] {
        println!(
            "{name}: raw=0x{:04X} mode_code={} AF_code={} noise_code={}",
            compander.raw(),
            compander.mode_code(),
            compander.af_code(),
            compander.noise_code()
        );
    }

    println!("Bands: points=tool-visible count; power ranges cover all 16 stored bytes.");
    for index in 0..BAND_COUNT {
        let band = calibration.band(index).map_err(|error| error.to_string())?;
        let wide = band.wide();
        let narrow = band.narrow();
        let (high_min, high_max) = raw_range(band.table_raw(Table::PowerHigh));
        let (low_min, low_max) = raw_range(band.table_raw(Table::PowerLow));
        println!(
            "Slot {:02} offset=0x{:04X} freq_code={} points={} W(mic_raw={},dev_code={},dev_raw=0x{:04X}) N(mic_raw={},dev_code={},dev_raw=0x{:04X}) powerH_raw16={high_min}..{high_max} powerL_raw16={low_min}..{low_max}",
            index + 1,
            band.byte_offset(),
            band.frequency_offset_code(),
            band.valid_points(),
            wide.mic_gain_raw(),
            wide.tx_deviation_code(),
            wide.tx_deviation_raw(),
            narrow.mic_gain_raw(),
            narrow.tx_deviation_code(),
            narrow.tx_deviation_raw(),
        );
    }
    println!(
        "Uninterpreted tail: {} bytes retained; not an RF calibration implementation.",
        calibration.uninterpreted_tail().len()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
