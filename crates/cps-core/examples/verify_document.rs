//! Optional local corpus check. Reads explicit paths; never writes a file.
//! No file content, channel names, or frequencies are printed.
//! Example: cargo run -p uv98-cps-core --example verify_document -- saved.880dmf

use std::{env, fs, io};

use uv98_codeplug::channels::{patch_channel, ChannelPatch};
use uv98_codeplug::{Section, CHANNELS_LEN, CHANNELS_OFFSET};
use uv98_cps_core::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = env::args_os().skip(1).collect();
    if paths.is_empty() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "provide saved CPS file paths").into(),
        );
    }
    for (index, path) in paths.iter().enumerate() {
        let bytes = fs::read(path)?;
        let mut document = Document::from_bytes(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        assert_eq!(document.export_bytes(), bytes, "unchanged round trip");
        let time_before = document.view().time().to_vec();
        let channel_end = CHANNELS_OFFSET + CHANNELS_LEN;
        let initial_scan = document.bytes()[CHANNELS_OFFSET + 19] & 0x80 == 0;
        let patch = ChannelPatch {
            scan_add: Some(!initial_scan),
            ..ChannelPatch::default()
        };
        document
            .apply(|candidate| {
                patch_channel(&mut candidate[CHANNELS_OFFSET..channel_end], 0, &patch)
            })
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        assert_eq!(document.dirty_bytes(), 1, "one bit affects one byte");
        assert_eq!(document.dirty_sections(), [Section::Channels]);
        assert_eq!(document.view().time().as_slice(), time_before);
        assert_eq!(document.original_bytes(), bytes);
        assert!(document.undo());
        assert_eq!(document.export_bytes(), bytes, "undo round trip");
        assert!(!document.is_dirty());
        println!("file_index={index} bytes={} unchanged_roundtrip=true single_bit_patch=true time_preserved=true undo_roundtrip=true source_written=false", bytes.len());
    }
    Ok(())
}
