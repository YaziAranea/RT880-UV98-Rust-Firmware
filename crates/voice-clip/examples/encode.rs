use std::{env, fs, io::Write, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 2 {
        return Err("usage: encode INPUT.wav NEW.clip".into());
    }
    if fs::metadata(&args[0])?.len() > 3 * 1024 * 1024 {
        return Err("input too large".into());
    }
    let slot =
        uv98_voice_clip::wav::import(&fs::read(&args[0])?).map_err(|e| format!("WAV: {e:?}"))?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    output.write_all(&slot)?;
    output.sync_all()?;
    Ok(())
}
