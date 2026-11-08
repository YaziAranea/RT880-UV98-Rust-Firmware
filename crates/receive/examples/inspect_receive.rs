//! Read one saved calibration block and print a pure plan. No transport exists.
use std::{env, fs::File, io::Read, path::PathBuf, process::ExitCode};
use uv98_calibration::{Calibration, BLOCK_LEN};
use uv98_receive::{Chip, FrequencyPlan, Operation, Plan, Width};

const USAGE: &str = "Usage: inspect_receive <calibration-4096.bin> <frequency_10hz decimal|0xHEX> <wide|narrow> <level0..10> [--chip-b-frequency]";

fn unsigned(value: &str) -> Result<u32, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).map_err(|_| "Invalid hexadecimal u32 frequency code".into())
    } else {
        value
            .parse()
            .map_err(|_| "Invalid decimal u32 frequency code".into())
    }
}
fn print_operations(operations: &[Operation]) {
    for (index, operation) in operations.iter().enumerate() {
        match *operation {
            Operation::Write { chip, register, value } => println!("  {index}: {chip:?} register 0x{register:02X} <- 0x{value:04X}"),
            Operation::PrimaryRoute { route } => println!("  {index}: A33 fresh read-modify-write for RX route code {route}; executor must reject TX bits 3/2"),
            Operation::AntennaPf0 { high } => println!("  {index}: PF0 requested level {} (not verified relay position)", if high {"HIGH"} else {"LOW"}),
        }
    }
}
fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let file = PathBuf::from(args.next().ok_or(USAGE)?);
    let code = args.next().ok_or(USAGE)?.into_string().map_err(|_| USAGE)?;
    let frequency = unsigned(&code)?;
    let width = match args.next().ok_or(USAGE)?.to_str() {
        Some("wide") => Width::Wide,
        Some("narrow") => Width::Narrow,
        _ => return Err("Width must be exactly wide or narrow; raw 2..255 are unsupported".into()),
    };
    let level = args
        .next()
        .ok_or(USAGE)?
        .into_string()
        .map_err(|_| USAGE)?
        .parse::<u8>()
        .map_err(|_| "Level must be an integer in 0..=10")?;
    if level > 10 {
        return Err("Level must be in 0..=10".into());
    }
    let secondary = match args.next() {
        None => false,
        Some(flag) if flag == "--chip-b-frequency" => true,
        _ => return Err(USAGE.into()),
    };
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    // Saved regular files only; there is no stdin, port, write or export mode.
    let metadata =
        std::fs::metadata(&file).map_err(|e| format!("Cannot inspect saved file: {e}"))?;
    if !metadata.is_file() || metadata.len() != BLOCK_LEN as u64 {
        return Err("Expected one regular saved file of exactly 4096 bytes".into());
    }
    let mut input = File::open(&file).map_err(|e| format!("Cannot open saved file: {e}"))?;
    let mut bytes = [0u8; BLOCK_LEN];
    input
        .read_exact(&mut bytes)
        .map_err(|e| format!("Cannot read complete saved block: {e}"))?;
    if input.read(&mut [0u8; 1]).map_err(|e| e.to_string())? != 0 {
        return Err("File grew during reading; input rejected".into());
    }
    let calibration = Calibration::parse(&bytes).map_err(|e| e.to_string())?;
    let plan = Plan::primary(&calibration, frequency, width, level)
        .map_err(|e| format!("Plan rejected: {e:?}"))?;
    println!("OFFLINE PLAN ONLY - no transport, COM, GPIO, RF activation or storage writes.");
    println!("Length/marker validation is not a checksum or RF calibration test.");
    println!(
        "Related RT880 1.16 fragments; exact UV98 MCU identity and hardware readiness unverified."
    );
    println!("Scope: already-prepared primary receiver configuration, not full RF initialization.");
    println!(
        "Input code={} (10 Hz units), width={:?}, level={}",
        plan.frequency_10hz(),
        plan.width(),
        plan.level()
    );
    println!(
        "Selected record={} (zero-based), point={}, byte offset=0x{:04X}",
        plan.selection().band_index(),
        plan.selection().table_index(),
        plan.selection().byte_offset()
    );
    println!(
        "Stored offset=0x{:04X}, corrected code={}, noise raw={}, squelch raw={}",
        plan.raw_offset(),
        plan.frequency_words().code,
        plan.noise_calibration(),
        plan.squelch_calibration()
    );
    println!("Primary operations ({}):", plan.operations().len());
    print_operations(plan.operations());
    println!("Level 0 still configures threshold words; it is not RF-off or squelch-open policy.");
    println!("External interlock and fresh TX-gate checks precede any execution. This tool executes none.");
    if secondary {
        let frequency = FrequencyPlan::new(&calibration, Chip::B, frequency)
            .map_err(|e| format!("Secondary frequency plan rejected: {e:?}"))?;
        println!(
            "Secondary frequency-only operations ({}), no shared routing:",
            frequency.operations().len()
        );
        print_operations(frequency.operations());
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
