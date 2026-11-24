//! Edit an embedded message in a new source BIN before ordinary personalization.
use super::*;
use uv98_voice_clip::{wav, Samples, VoiceConfig, CONFIG_BYTES, CONFIG_MAGIC, MAGIC, SLOT_BYTES};

const POLICY_PREFIX: [u8; 12] = [8, 1, 0, 0, 1, 0, 0, 0, 0x16, 0, 0, 0];
const POLICY_BYTES: usize = 16;
const POLICY_VK01: u32 = 0x564b3031;
const POLICY_VK02: u32 = 0x564b3032;
const POLICY_VK03: u32 = 0x564b3033;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub clip: usize,
    pub config: Option<(usize, VoiceConfig)>,
}

fn offsets<'a>(bytes: &'a [u8], marker: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    bytes
        .windows(marker.len())
        .enumerate()
        .filter_map(move |(at, data)| (data == marker).then_some(at))
}

fn unique(mut matches: impl Iterator<Item = usize>) -> Option<usize> {
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn overlap(a: usize, len_a: usize, b: usize, len_b: usize) -> bool {
    a < b + len_b && b < a + len_a
}

pub(super) fn layout(bytes: &[u8]) -> Option<Layout> {
    if bytes.len() != IMAGE_BYTES {
        return None;
    }
    let clip = unique(offsets(bytes, MAGIC))?;
    let digest = profile::digest_offset(bytes).ok()?;
    let reset = (u32::from_le_bytes(bytes[4..8].try_into().ok()?) & !1)
        .checked_sub(APPLICATION_BASE)? as usize;
    let writable = |at, count| {
        at >= 0x2c0
            && at + count <= IMAGE_BYTES - 4096
            && !overlap(
                at,
                count,
                digest - profile::MAGIC.len(),
                profile::MAGIC.len() + 32,
            )
            && !overlap(at, count, reset, 2)
    };
    if !writable(clip, SLOT_BYTES) {
        return None;
    }
    Samples::from_body(&bytes[clip + MAGIC.len()..clip + SLOT_BYTES])?;

    // Locate the complete aligned release declaration independently of its filename.
    // Compressed audio can contain arbitrary bytes, including a copy of that declaration.
    let policy = unique(offsets(bytes, &POLICY_PREFIX).filter(|&at| {
        at % 4 == 0
            && at + POLICY_BYTES <= bytes.len()
            && !overlap(at, POLICY_BYTES, clip, SLOT_BYTES)
    }))?;
    if !writable(policy, POLICY_BYTES) {
        return None;
    }
    let version = u32::from_le_bytes(bytes[policy + 12..policy + 16].try_into().ok()?);
    let config = match version {
        POLICY_VK01 | POLICY_VK02 => None,
        POLICY_VK03 => {
            // Compressed nibbles and unused padding may both spell CONFIG_MAGIC.
            // A marker crossing the slot boundary is still invalid.
            let at = unique(offsets(bytes, CONFIG_MAGIC).filter(|&at| {
                !(at >= clip && at + CONFIG_MAGIC.len() <= clip + SLOT_BYTES)
            }))?;
            if !writable(at, CONFIG_BYTES)
                || overlap(at, CONFIG_BYTES, clip, SLOT_BYTES)
                || overlap(at, CONFIG_BYTES, policy, POLICY_BYTES)
            {
                return None;
            }
            Some((
                at,
                VoiceConfig::from_body(&bytes[at + CONFIG_MAGIC.len()..at + CONFIG_BYTES])?,
            ))
        }
        _ => return None,
    };
    Some(Layout { clip, config })
}

fn invalid_layout() -> LocalizedText {
    msg!("В BIN повреждена или недоступна голосовая запись, настройка кнопки или политика выпуска.",
        "The BIN voice slot, key settings, or release policy are invalid or unavailable.",
        "Sprachspeicher, Tasteneinstellungen oder Freigabekennung der BIN fehlen oder sind ungültig.")
}

pub fn save(
    source: &PreparedImage,
    clip: Option<&[u8; SLOT_BYTES]>,
    config: Option<VoiceConfig>,
    destination: &Path,
) -> Result<PreparedImage, LocalizedText> {
    let source_layout = source
        .voice_layout
        .filter(|_| source.is_personalization_source())
        .ok_or_else(invalid_layout)?;
    if clip.is_none() && config.is_none() {
        return Err(invalid_layout());
    }
    source.revalidate()?;
    let mut bytes = source.bytes.clone();
    let mut expected = source_layout;
    if let Some(clip) = clip {
        bytes[source_layout.clip..source_layout.clip + SLOT_BYTES].copy_from_slice(clip);
    }
    if let Some(config) = config {
        let (at, _) = source_layout.config.ok_or_else(invalid_layout)?;
        let record = config.encode().ok_or_else(invalid_layout)?;
        bytes[at..at + CONFIG_BYTES].copy_from_slice(&record);
        expected.config = Some((at, config));
    }
    // Reject malformed/overlapping or displaced records before creating any file.
    if layout(&bytes) != Some(expected) {
        return Err(invalid_layout());
    }
    let mut file = new_file(destination)?;
    let result = (|| {
        durable(&mut file, &bytes)?;
        drop(file);
        let image = prepare_selected(destination, None)?;
        if image.bytes != bytes || !image.can_add_voice() || image.voice_layout != Some(expected) {
            return Err(msg!(
                "Проверка сохранённого голосового BIN не прошла.",
                "Saved voice BIN verification failed.",
                "Prüfung der gespeicherten Sprach-BIN fehlgeschlagen."
            ));
        }
        Ok(image)
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

pub fn load(path: &Path) -> Result<wav::Recording, LocalizedText> {
    let bytes = bounded_read(path, 32 * 1024 * 1024)?;
    let result = if bytes.starts_with(b"RIFF") {
        wav::decode(&bytes)
    } else {
        wav::decode_mp3(bytes)
    };
    result.map_err(audio_error)
}

pub fn audio_error(error: wav::Error) -> LocalizedText {
    match error {
        wav::Error::Format => msg!(
            "Не удалось прочитать звук. Выберите MP3 или несжатый PCM WAV (1–2 канала, 8/16/24/32 бит).",
            "Could not decode the audio. Choose MP3 or uncompressed PCM WAV (1–2 channels, 8/16/24/32 bits).",
            "Audiodatei konnte nicht gelesen werden. MP3 oder unkomprimierte PCM-WAV wählen (1–2 Kanäle, 8/16/24/32 Bit)."
        ),
        wav::Error::Duration => msg!("В выбранном фрагменте нет доступных отсчётов.",
            "No samples available in the selected excerpt.", "Der gewählte Ausschnitt enthält keine Samples."),
        wav::Error::TooLarge => msg!("Запись слишком длинная для импорта (до 16 млн отсчётов на канал).",
            "Recording exceeds the import limit (16 million samples per channel).",
            "Die Aufnahme überschreitet das Importlimit (16 Millionen Samples pro Kanal)."),
    }
}

#[cfg(windows)]
pub fn play(clip: &[u8; SLOT_BYTES]) -> bool {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT, SND_SYNC};
    let Some(pcm) = Samples::from_body(&clip[MAGIC.len()..]) else {
        return false;
    };
    let count = pcm.len() as u32;
    let rate = (uv98_voice_clip::RATE * 2) as u32;
    let mut wave = Vec::with_capacity(44 + count as usize);
    wave.extend_from_slice(b"RIFF");
    wave.extend((36 + count).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend(16u32.to_le_bytes());
    wave.extend(1u16.to_le_bytes()); // PCM
    wave.extend(1u16.to_le_bytes()); // mono
    wave.extend(rate.to_le_bytes());
    wave.extend(rate.to_le_bytes()); // one byte per sample
    wave.extend(1u16.to_le_bytes());
    wave.extend(8u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend(count.to_le_bytes());
    wave.extend(pcm);
    // SND_MEMORY treats this pointer as RIFF bytes. SND_SYNC keeps the backing buffer
    // alive until playback completes on the worker thread.
    unsafe {
        PlaySoundW(
            wave.as_ptr().cast(),
            std::ptr::null_mut(),
            SND_MEMORY | SND_NODEFAULT | SND_SYNC,
        ) != 0
    }
}

#[cfg(test)]
pub(crate) fn test_policy(version: u8) -> [u8; POLICY_BYTES] {
    let mut bytes = [0; POLICY_BYTES];
    bytes[..12].copy_from_slice(&POLICY_PREFIX);
    bytes[12..].copy_from_slice(&(0x564b3030 + u32::from(version)).to_le_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_valid_nonoverlapping_slot_may_be_patched() {
        let mut bin = vec![0xff; IMAGE_BYTES];
        bin[4..8].copy_from_slice(&(APPLICATION_BASE + 0x301).to_le_bytes());
        bin[0x500..0x510].copy_from_slice(&test_policy(2));
        bin[0x400..0x410].copy_from_slice(profile::MAGIC);
        let data = uv98_voice_clip::encode(&[0; 42]).unwrap();
        bin[0x1000..0x1000 + SLOT_BYTES].copy_from_slice(&data);
        assert_eq!(
            layout(&bin),
            Some(Layout {
                clip: 0x1000,
                config: None
            })
        );
        bin[0x1015] ^= 1;
        assert_eq!(layout(&bin), None);
        bin[0x1015] ^= 1;
        bin[0x3000..0x3010].copy_from_slice(MAGIC);
        assert_eq!(layout(&bin), None);
        bin[0x3000..0x3010].fill(0xff);
        bin[4..8].copy_from_slice(&(APPLICATION_BASE + 0x1041).to_le_bytes());
        assert_eq!(layout(&bin), None);
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn config_requires_a_unique_intact_vk03_record_in_writable_space() {
        let mut image = vec![0xff; IMAGE_BYTES];
        image[4..8].copy_from_slice(&(APPLICATION_BASE + 0x801).to_le_bytes());
        image[0x410..0x420].copy_from_slice(profile::MAGIC);
        image[0x600..0x610].copy_from_slice(&test_policy(3));
        let clip = uv98_voice_clip::encode(&[104; 40]).unwrap();
        image[0x1000..0x1000 + SLOT_BYTES].copy_from_slice(&clip);
        let config = VoiceConfig {
            key: Some(0),
            delay_ms: 1,
        };
        let record = config.encode().unwrap();
        let at = 0x2000;
        assert!(layout(&image).is_none());
        for bad in [
            0x100,
            0x400,
            0x420,
            0x800,
            0x1100,
            IMAGE_BYTES - 4096 - 2,
            IMAGE_BYTES - CONFIG_BYTES,
        ] {
            let mut wrong = image.clone();
            wrong[bad..bad + CONFIG_BYTES].copy_from_slice(&record);
            assert!(layout(&wrong).is_none(), "unsafe record at {bad:#x}");
        }
        image[at..at + CONFIG_BYTES].copy_from_slice(&record);
        assert_eq!(layout(&image).unwrap().config, Some((at, config)));
        // A 32-frame ADPCM recording whose 16 packed audio bytes spell the
        // config marker. 0x056a is CRC-16/CCITT-FALSE over count LE + nibbles.
        image[0x1010..0x1014].copy_from_slice(&[0x6a, 0x05, 32, 0]);
        image[0x1014..0x1024].copy_from_slice(CONFIG_MAGIC);
        image[0x1140..0x1150].copy_from_slice(CONFIG_MAGIC);
        assert_eq!(layout(&image).unwrap().config, Some((at, config)));
        for index in 0..CONFIG_BYTES {
            let mut wrong = image.clone();
            wrong[at + index] ^= 1;
            assert!(layout(&wrong).is_none(), "corrupted config byte {index}");
        }
        image[0x3000..0x3000 + CONFIG_BYTES].copy_from_slice(&record);
        assert!(layout(&image).is_none());
        image[0x3000..0x3000 + CONFIG_BYTES].fill(0xff);
        image[0x500..0x510].copy_from_slice(&test_policy(3));
        assert!(layout(&image).is_none());
        image[0x500..0x510].fill(0xff);
        // R56 audio may contain arbitrary data, including full markers.
        image[0x600..0x610].copy_from_slice(&test_policy(2));
        image[0x1100..0x1110].copy_from_slice(&test_policy(3));
        image[0x1140..0x1140 + CONFIG_BYTES].copy_from_slice(&record);
        assert_eq!(layout(&image).unwrap().config, None);
    }
}
