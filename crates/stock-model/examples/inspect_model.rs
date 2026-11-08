//! Offline demonstration using a saved block. No device access or writes.
use std::{env, fs, path::PathBuf, process::ExitCode};
use uv98_calibration::{Calibration, Filter, Table};
use uv98_stock_model::{frequency, rf, REFERENCE_IMAGE_SHA256};

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("Usage: inspect_model <raw-4096-byte-file>")?,
    );
    if args.next().is_some() {
        return Err("Exactly one input file is required".into());
    }
    let bytes = fs::read(&path).map_err(|error| format!("Cannot read input: {error}"))?;
    let calibration = Calibration::parse(&bytes).map_err(|error| error.to_string())?;
    println!("OFFLINE MODEL ONLY. No device I/O or RF activation.");
    println!("Reference RT880 image SHA256: {REFERENCE_IMAGE_SHA256}");
    println!("Exact UV98 application identity and physical RF behavior remain unverified.");
    println!("Frequency and power values are stored codes, not measured units.");
    println!("Example context: channel+3=0, channel+0x13=0, power mode=2, override=0.");
    for code in [14_500_000, 43_350_000, 70_000_000, 124_000_001] {
        let Some(selection) = frequency::select(code) else {
            println!("code={code}: NO SELECTION; no prior record or defaults substituted.");
            continue;
        };
        let band = calibration
            .band(selection.band_index())
            .map_err(|error| error.to_string())?;
        let point = selection.table_index();
        let high = band
            .point_raw(Table::PowerHigh, point)
            .ok_or("Invalid high-power point")?;
        let low = band
            .point_raw(Table::PowerLow, point)
            .ok_or("Invalid low-power point")?;
        let words = frequency::correct_code(code, band.frequency_offset_raw());
        let power = rf::power_code_at_05552(2, high, low);
        let deviation = rf::deviation_at_054cc(
            0,
            0,
            band.wide().tx_deviation_raw(),
            band.narrow().tx_deviation_raw(),
        );
        println!(
            "code={code} slot={} point={point} offset={} corrected={} reg38={:04X} reg39={:04X} high_raw={high} low_raw={low} reg36={:04X} reg40={deviation:04X}",
            selection.band_index(), selection.byte_offset(), words.code,
            words.reg38, words.reg39, rf::power_register_04f0c(u32::from(power)),
        );
    }
    println!("Initialization filter coefficients from raw calibration bytes (no UI mask):");
    for (field, model_field) in [
        (Filter::Tx300, rf::FilterField::Tx300),
        (Filter::Tx3k, rf::FilterField::Tx3k),
        (Filter::Rx300, rf::FilterField::Rx300),
        (Filter::Rx3k, rf::FilterField::Rx3k),
    ] {
        let raw = calibration.general().filter_raw(field);
        let words = rf::filter_registers(model_field, raw);
        println!("{field:?} raw={raw}: {words:?}");
    }
    println!("This is a partial computation sample, not a hardware initialization sequence.");
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
