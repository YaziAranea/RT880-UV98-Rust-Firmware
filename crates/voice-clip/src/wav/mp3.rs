use super::{Error, Recording, MAX_SOURCE_FRAMES};
use std::{boxed::Box, io::Cursor, io::ErrorKind, vec::Vec};
use symphonia::core::{
    audio::SampleBuffer, codecs::CODEC_TYPE_MP3, errors::Error as MediaError,
    formats::FormatOptions, io::MediaSourceStream, probe::Hint,
};

pub fn decode_mp3(bytes: Vec<u8>) -> Result<Recording, Error> {
    let stream = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let options = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let mut reader = symphonia::default::get_probe()
        .format(&hint, stream, &options, &Default::default())
        .map_err(|_| Error::Format)?
        .format;
    let track = reader
        .default_track()
        .filter(|track| track.codec_params.codec == CODEC_TYPE_MP3)
        .ok_or(Error::Format)?;
    let id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &Default::default())
        .map_err(|_| Error::Format)?;
    let mut mono = Vec::new();
    let mut rate = None;
    loop {
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            Err(MediaError::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(_) => return Err(Error::Format),
        };
        if packet.track_id() != id {
            continue;
        }
        let decoded = decoder.decode(&packet).map_err(|_| Error::Format)?;
        let spec = *decoded.spec();
        let channels = spec.channels.count();
        if !(1..=2).contains(&channels)
            || !(8000..=48000).contains(&spec.rate)
            || rate.replace(spec.rate).is_some_and(|old| old != spec.rate)
        {
            return Err(Error::Format);
        }
        if mono.len() + decoded.frames() > MAX_SOURCE_FRAMES {
            return Err(Error::TooLarge);
        }
        let mut pcm = SampleBuffer::<i16>::new(decoded.capacity() as u64, spec);
        pcm.copy_interleaved_ref(decoded);
        mono.extend(pcm.samples().chunks_exact(channels).map(|frame| {
            (frame.iter().map(|&v| i32::from(v)).sum::<i32>() / channels as i32) as i16
        }));
    }
    if mono.is_empty() {
        return Err(Error::Format);
    }
    Ok(Recording {
        rate: rate.ok_or(Error::Format)? as usize,
        mono,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Samples, MAGIC, MAX_FRAMES};

    #[test]
    fn selects_late_tone_from_long_stereo_mp3() {
        // Synthetic input, no speech or third-party recording. LAME 32 kb/s stereo
        // at 16 kHz, 1.6 s silence followed by 1.6 s of 500 Hz sine at amplitude 12000.
        let mp3 = include_bytes!("../../tests/fixtures/silence-tone.mp3");
        let recording = decode_mp3(mp3.to_vec()).unwrap();
        assert!((3200..3400).contains(&recording.duration_ms()));
        let last = recording.last_start_ms();
        assert!(last >= 1700);
        assert!(recording.clip(last + 1).is_err());
        let quiet = recording.clip(0).unwrap();
        let loud = recording.clip(last).unwrap();
        let samples = |slot: &[u8]| {
            Samples::from_body(&slot[MAGIC.len()..])
                .unwrap()
                .collect::<Vec<_>>()
        };
        assert_eq!(samples(&quiet), std::vec![128; MAX_FRAMES * 2]);
        let tone = samples(&loud);
        assert_eq!(tone.len(), MAX_FRAMES * 2);
        assert!(tone.iter().filter(|&&s| s.abs_diff(128) > 16).count() > MAX_FRAMES);
        assert!(decode_mp3(b"not an MP3".to_vec()).is_err());
    }
}
