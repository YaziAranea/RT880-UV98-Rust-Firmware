use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    source: PreparedImage,
    source_catalog: PathBuf,
    manifest: PathBuf,
    data: Value,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "uv98-profile-test-{}-{}-{}",
            std::process::id(),
            ms(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let source_path = root.join("source.bin");
        let source_catalog = root.join("catalog.json");
        let mut bytes = vec![0x33; IMAGE_BYTES];
        bytes[..4].copy_from_slice(&0x2000_c000u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&0x0800_3001u32.to_le_bytes());
        bytes[1024..1040].copy_from_slice(MAGIC);
        bytes[1040..1072].fill(0x44);
        fs::write(&source_path, &bytes).unwrap();
        fs::write(&source_catalog, serde_json::to_vec(&json!({"schema":1,"profile":PROFILE,"images":[{"label":"SYNTHETIC TEST R36","sha256":hash(&bytes),"bytes":IMAGE_BYTES,"tx_sha256":wire_hash(&bytes)}]})).unwrap()).unwrap();
        let source = prepare_with_catalog(&source_path, &source_catalog).unwrap();

        let backup = root.join("backup");
        fs::create_dir(&backup).unwrap();
        let mut storage: Vec<u8> = (0..STORAGE_BYTES)
            .map(|i| ((i / 97 + i * 17) & 255) as u8)
            .collect();
        storage[0] = uv98_calibration::MARKER;
        let calibration = &storage[..CAL_BYTES];
        fs::write(backup.join("storage-address-space.bin"), &storage).unwrap();
        fs::write(backup.join("calibration.bin"), calibration).unwrap();
        let mut frames = Vec::with_capacity(STORAGE_BLOCKS * FRAME_SIZE);
        let mut tx = CONNECT.to_vec();
        let mut packets = vec![CONNECT.to_vec()];
        let mut frame_metadata = Vec::new();
        for (block, payload) in storage.chunks_exact(BLOCK_SIZE).enumerate() {
            let header = [0x52, (block >> 8) as u8, block as u8];
            let mut request = header.to_vec();
            request.push(header.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
            tx.extend(&request);
            packets.push(request);
            let mut frame = header.to_vec();
            frame.extend(payload);
            frame.push(frame.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
            frame_metadata.push(json!({"block":block,"frame_bytes":FRAME_SIZE,"validated":true,"sha256":hash(&frame)}));
            frames.extend(frame);
        }
        tx.extend(END);
        packets.push(END.to_vec());
        let mut rx = vec![0x06];
        rx.extend(&frames);
        let mut passes = Vec::new();
        for pass in 1..=2 {
            fs::write(backup.join(format!("pass-{pass:02}.tx.bin")), &tx).unwrap();
            fs::write(backup.join(format!("pass-{pass:02}.rx.bin")), &rx).unwrap();
            fs::write(backup.join(format!("pass-{pass:02}.frames.bin")), &frames).unwrap();
            passes.push(json!({"pass":pass,"complete":true,"end_sent":true,"cleanup_error":null,
                "tx_sha256":hash(&tx),"rx_sha256":hash(&rx),"frames_sha256":hash(&frames),
                "tx_packets":packets.iter().map(|packet| packet.iter().map(|b| format!("{b:02X}")).collect::<String>()).collect::<Vec<_>>(),
                "frames":frame_metadata}));
        }
        let data = json!({"schema":1,"status":"verified_two_reads","two_reads_byte_identical":true,
            "plan":"stock_protocol_4_mib","write_or_erase_commands_sent":false,
            "sections":[
                {"file":"storage-address-space.bin","first_block":0,"bytes":STORAGE_BYTES,"sha256":hash(&storage)},
                {"file":"calibration.bin","first_block":0,"bytes":CAL_BYTES,"sha256":hash(calibration)}
            ],"passes":passes});
        let manifest = backup.join("manifest.json");
        fs::write(&manifest, serde_json::to_vec_pretty(&data).unwrap()).unwrap();
        Self {
            root,
            source,
            source_catalog,
            manifest,
            data,
        }
    }

    fn set_manifest(&self, data: &Value) {
        fs::write(&self.manifest, serde_json::to_vec_pretty(data).unwrap()).unwrap();
    }

    fn rewrite_source(&mut self, bytes: &[u8]) {
        fs::write(&self.source.path, bytes).unwrap();
        fs::write(&self.source_catalog, serde_json::to_vec(&json!({"schema":1,"profile":PROFILE,"images":[{"label":"SYNTHETIC TEST R36","sha256":hash(bytes),"bytes":IMAGE_BYTES,"tx_sha256":wire_hash(bytes)}]})).unwrap()).unwrap();
        self.source = prepare_with_catalog(&self.source.path, &self.source_catalog).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn replace_voice_then_personalize_and_reopen() {
    use crate::firmware::voice;
    use uv98_voice_clip::{Samples, SLOT_BYTES};
    let mut fixture = Fixture::new();
    let mut original = fixture.source.bytes.clone();
    if let Some(real) = std::env::var_os("UV98_VOICE_TEST_BIN") {
        original = fs::read(real).unwrap();
    } else {
        original[0x700..0x710].copy_from_slice(&voice::test_policy(2));
        let initial = uv98_voice_clip::encode(&[0; 64]).unwrap();
        original[8192..8192 + SLOT_BYTES].copy_from_slice(&initial);
    }
    fixture.rewrite_source(&original);
    let start = fixture.source.voice_layout.unwrap().clip;
    if fixture.source.voice_config().is_none() {
        let destination = fixture.root.join("legacy-config.bin");
        assert!(voice::save(
            &fixture.source,
            None,
            Some(Default::default()),
            &destination
        )
        .is_err());
        assert!(!destination.exists());
    }
    let wav = fixture.root.join("voice.wav");
    let mut pcm = Vec::from(&b"RIFF"[..]);
    pcm.extend(836u32.to_le_bytes());
    pcm.extend(b"WAVEfmt ");
    pcm.extend(16u32.to_le_bytes());
    for n in [1u16, 1] {
        pcm.extend(n.to_le_bytes());
    }
    for n in [8000u32, 16000] {
        pcm.extend(n.to_le_bytes());
    }
    for n in [2u16, 16] {
        pcm.extend(n.to_le_bytes());
    }
    pcm.extend(b"data");
    pcm.extend(800u32.to_le_bytes());
    for i in 0..400 {
        pcm.extend((if i % 16 < 8 { 9000i16 } else { -9000i16 }).to_le_bytes());
    }
    fs::write(&wav, &pcm).unwrap();
    let path = fixture.root.join("edited.bin");
    let clip = voice::load(&wav).unwrap().clip(0).unwrap();
    let edited = voice::save(&fixture.source, Some(&clip), None, &path).unwrap();
    assert_eq!(&original[..start], &edited.bytes[..start]);
    assert_eq!(
        &original[start + SLOT_BYTES..],
        &edited.bytes[start + SLOT_BYTES..]
    );
    assert_ne!(
        &original[start..start + SLOT_BYTES],
        &edited.bytes[start..start + SLOT_BYTES]
    );
    assert_eq!(fs::read(&fixture.source.path).unwrap(), original);
    let decoded = Samples::from_body(&edited.bytes[start + 16..start + SLOT_BYTES]).unwrap();
    assert_eq!(decoded.len(), 400);
    assert!(decoded.max().unwrap() > 148);
    assert!(voice::save(&fixture.source, Some(&clip), None, &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), edited.bytes);
    fs::write(&wav, b"broken").unwrap();
    assert!(voice::load(&wav).is_err());
    let personal = personalize(&edited, &fixture.manifest, &fixture.root.join("personal")).unwrap();
    let selected = prepare_selected(&personal.path, None).unwrap();
    assert!(selected.revalidate().is_ok());
    assert!(!selected.can_add_voice());
    let digest = digest_offset(&original).unwrap();
    assert_eq!(&selected.bytes[..digest], &edited.bytes[..digest]);
    assert_eq!(&selected.bytes[digest + 32..], &edited.bytes[digest + 32..]);

    // The CPS workflow uses the selected portion of a real stereo MPEG Layer III file.
    let mp3 = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/voice-clip/tests/fixtures/silence-tone.mp3");
    let recording = voice::load(&mp3).unwrap();
    let last = recording.last_start_ms();
    let clip = recording.clip(last).unwrap();
    let edited = voice::save(
        &fixture.source,
        Some(&clip),
        None,
        &fixture.root.join("mp3.bin"),
    )
    .unwrap();
    assert_eq!(&original[..start], &edited.bytes[..start]);
    assert_eq!(
        &original[start + SLOT_BYTES..],
        &edited.bytes[start + SLOT_BYTES..]
    );
    let personal = personalize(
        &edited,
        &fixture.manifest,
        &fixture.root.join("personal-mp3"),
    )
    .unwrap();
    let reopened = prepare_selected(&personal.path, None).unwrap();
    assert!(reopened.revalidate().is_ok());
    assert_eq!(&reopened.bytes[start..start + SLOT_BYTES], &clip);
}

#[test]
fn voice_settings_only_and_mp3_survive_save_reopen_and_personalization() {
    use crate::firmware::voice;
    use uv98_voice_clip::{VoiceConfig, CONFIG_BYTES, MAX_DELAY_MS, SLOT_BYTES};

    let mut fixture = Fixture::new();
    let original = if let Some(path) = std::env::var_os("UV98_VOICE_CONFIG_TEST_BIN") {
        fs::read(path).unwrap()
    } else {
        let mut bytes = fixture.source.bytes.clone();
        bytes[0x700..0x710].copy_from_slice(&voice::test_policy(3));
        bytes[0x1000..0x1000 + CONFIG_BYTES]
            .copy_from_slice(&VoiceConfig::default().encode().unwrap());
        bytes[0x2200..0x2200 + SLOT_BYTES]
            .copy_from_slice(&uv98_voice_clip::encode(&[1000; 142]).unwrap());
        bytes
    };
    fixture.rewrite_source(&original);
    let slots = fixture.source.voice_layout.unwrap();
    let (at, _) = slots
        .config
        .expect("expected R57 image with separate config record");
    let config = VoiceConfig {
        key: Some(0),
        delay_ms: MAX_DELAY_MS,
    };
    let path = fixture.root.join("assigned.bin");
    voice::save(&fixture.source, None, Some(config), &path).unwrap();
    let assigned = prepare_selected(&path, None).unwrap();
    assert_eq!(assigned.voice_config(), Some(config));
    let mut expected = original.clone();
    expected[at..at + CONFIG_BYTES].copy_from_slice(&config.encode().unwrap());
    assert_eq!(assigned.bytes, expected);

    let mp3 = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/voice-clip/tests/fixtures/silence-tone.mp3");
    let recording = voice::load(&mp3).unwrap();
    let clip = recording.clip(recording.last_start_ms()).unwrap();
    let unassigned = VoiceConfig {
        key: None,
        delay_ms: 1,
    };
    let edited = voice::save(
        &assigned,
        Some(&clip),
        Some(unassigned),
        &fixture.root.join("modified.bin"),
    )
    .unwrap();
    expected[at..at + CONFIG_BYTES].copy_from_slice(&unassigned.encode().unwrap());
    expected[slots.clip..slots.clip + SLOT_BYTES].copy_from_slice(&clip);
    assert_eq!(edited.bytes, expected);
    let personal = personalize(&edited, &fixture.manifest, &fixture.root.join("personal")).unwrap();
    let reopened = prepare_selected(&personal.path, None).unwrap();
    reopened.revalidate().unwrap();
    assert_eq!(reopened.voice_config(), Some(unassigned));
    assert_eq!(&reopened.bytes[slots.clip..slots.clip + SLOT_BYTES], &clip);
    let digest = digest_offset(&expected).unwrap();
    assert_eq!(&reopened.bytes[..digest], &expected[..digest]);
    assert_eq!(&reopened.bytes[digest + 32..], &expected[digest + 32..]);
    let forbidden = fixture.root.join("forbidden.bin");
    assert!(voice::save(&reopened, None, Some(config), &forbidden).is_err());
    assert!(voice::save(
        &assigned,
        None,
        Some(VoiceConfig {
            key: Some(10),
            delay_ms: 0
        }),
        &forbidden
    )
    .is_err());
    assert!(voice::save(
        &assigned,
        None,
        Some(VoiceConfig {
            key: Some(9),
            delay_ms: MAX_DELAY_MS + 1
        }),
        &forbidden
    )
    .is_err());
    fs::write(&assigned.path, b"replaced after selection").unwrap();
    assert!(voice::save(&assigned, None, Some(config), &forbidden).is_err());
    assert!(!forbidden.exists());
    assert_eq!(fs::read(&fixture.source.path).unwrap(), original);
}

#[test]
fn independent_source_changes_only_digest_and_reopens_without_original_files() {
    let mut fixture = Fixture::new();
    fs::remove_file(&fixture.source_catalog).unwrap();
    fixture.source = prepare_selected(&fixture.source.path, None).unwrap();
    assert_eq!(fixture.source.provenance, Provenance::Imported);
    assert!(fixture.source.source_warning(Language::English).is_some());
    let original = fixture.source.bytes.clone();
    let backup_manifest = fs::read(&fixture.manifest).unwrap();
    let output = fixture.root.join("personal");
    let image = personalize(&fixture.source, &fixture.manifest, &output).unwrap();
    let calibration = fs::read(fixture.manifest.parent().unwrap().join("calibration.bin")).unwrap();
    let offset = digest_offset(&original).unwrap();
    assert_eq!(image.bytes[..offset], original[..offset]);
    assert_eq!(image.bytes[offset + 32..], original[offset + 32..]);
    assert_eq!(
        image.bytes[offset..offset + 32],
        Sha256::digest(calibration)[..]
    );
    assert_eq!(fs::read(&fixture.source.path).unwrap(), original);
    assert_eq!(fs::read(&fixture.manifest).unwrap(), backup_manifest);
    assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
    assert_eq!(fs::read_dir(output.join("firmware")).unwrap().count(), 3);
    assert_eq!(
        fs::read(image.path.with_file_name(SOURCE_BIN)).unwrap(),
        original
    );
    fs::remove_file(&fixture.source.path).unwrap();
    fs::remove_file(&fixture.manifest).unwrap();
    let selected = prepare_selected(&image.path, None).unwrap();
    assert!(matches!(
        selected.provenance,
        Provenance::Personal {
            legacy_catalog: None,
            ..
        }
    ));
    assert!(selected.revalidate().is_ok());
    for (lang, expected) in [
        (Language::Russian, "Подлинность исходника"),
        (Language::English, "Source authenticity"),
        (Language::German, "Echtheit der Quelle"),
    ] {
        assert!(selected.source_warning(lang).unwrap().contains(expected));
    }
    assert_eq!(selected.sha256, image.sha256);
    assert_eq!(selected.tx_sha256, wire_hash(&selected.bytes));
    assert_eq!(default_image(&output).unwrap().path, image.path);
    let renamed = image.path.with_file_name("renamed.bin");
    fs::rename(&image.path, &renamed).unwrap();
    assert!(matches!(
        prepare_selected(&renamed, None).unwrap().provenance,
        Provenance::Personal { .. }
    ));
}

#[test]
fn failed_or_incomplete_manifest_and_corrupt_files_are_rejected_before_output() {
    let fixture = Fixture::new();
    let output = fixture.root.join("personal");
    for (key, bad_value) in [
        ("status", json!("failed")),
        ("two_reads_byte_identical", json!(false)),
        ("plan", json!("calibration")),
    ] {
        let mut broken = fixture.data.clone();
        broken[key] = bad_value;
        fixture.set_manifest(&broken);
        assert!(personalize(&fixture.source, &fixture.manifest, &output).is_err());
        assert!(!output.exists());
    }
    fixture.set_manifest(&fixture.data);
    let frame_path = fixture
        .manifest
        .parent()
        .unwrap()
        .join("pass-02.frames.bin");
    let mut frames = fs::read(&frame_path).unwrap();
    frames[100] ^= 1;
    fs::write(&frame_path, frames).unwrap();
    assert!(personalize(&fixture.source, &fixture.manifest, &output).is_err());
    assert!(!output.exists());
}

#[test]
fn calibration_must_agree_with_nor_and_not_be_empty_or_marker_only() {
    let fixture = Fixture::new();
    let directory = fixture.manifest.parent().unwrap();
    let original_storage = fs::read(directory.join("storage-address-space.bin")).unwrap();
    for fill in [0, 0xff] {
        for marker in [fill, uv98_calibration::MARKER] {
            let mut calibration = vec![fill; CAL_BYTES];
            calibration[0] = marker;
            let mut storage = original_storage.clone();
            storage[..CAL_BYTES].copy_from_slice(&calibration);
            fs::write(directory.join("calibration.bin"), &calibration).unwrap();
            fs::write(directory.join("storage-address-space.bin"), &storage).unwrap();
            let mut data = fixture.data.clone();
            data["sections"][0]["sha256"] = json!(hash(&storage));
            data["sections"][1]["sha256"] = json!(hash(&calibration));
            fixture.set_manifest(&data);
            let error = verify_snapshot(&fixture.manifest).err().unwrap();
            let error = error.as_str(Language::Russian);
            assert!(
                error.contains("калибровок") || error.contains("Калибровки"),
                "{error}"
            );
        }
    }
    fs::write(
        directory.join("storage-address-space.bin"),
        &original_storage,
    )
    .unwrap();
    let calibration = vec![1; CAL_BYTES];
    fs::write(directory.join("calibration.bin"), &calibration).unwrap();
    let mut data = fixture.data.clone();
    data["sections"][1]["sha256"] = json!(hash(&calibration));
    fixture.set_manifest(&data);
    assert!(verify_snapshot(&fixture.manifest)
        .err()
        .unwrap()
        .as_str(Language::Russian)
        .contains("не совпадает с началом"));
}

#[test]
fn missing_or_multiple_profile_and_existing_output_are_refused() {
    let mut fixture = Fixture::new();
    let original = fixture.source.bytes.clone();
    let output = fixture.root.join("personal");
    let mut bytes = original.clone();
    bytes[1024] = 0;
    fixture.rewrite_source(&bytes);
    assert!(personalize(&fixture.source, &fixture.manifest, &output).is_err());
    bytes = original.clone();
    bytes[1000..1016].copy_from_slice(MAGIC);
    fixture.rewrite_source(&bytes);
    assert!(personalize(&fixture.source, &fixture.manifest, &output).is_err());
    fixture.rewrite_source(&original);
    fs::create_dir(&output).unwrap();
    fs::write(output.join("keep.txt"), b"unrelated").unwrap();
    assert!(personalize(&fixture.source, &fixture.manifest, &output).is_err());
    assert_eq!(fs::read(output.join("keep.txt")).unwrap(), b"unrelated");
    assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
}

#[test]
fn self_consistent_personal_catalog_cannot_allow_unrelated_code_changes() {
    let fixture = Fixture::new();
    let image = personalize(
        &fixture.source,
        &fixture.manifest,
        &fixture.root.join("personal"),
    )
    .unwrap();
    let mut bytes = image.bytes;
    bytes[2000] ^= 1;
    fs::write(&image.path, &bytes).unwrap();
    let sidecar = image.path.with_file_name("catalog.json");
    let mut data: Value = serde_json::from_slice(&fs::read(&sidecar).unwrap()).unwrap();
    data["images"][0]["sha256"] = json!(hash(&bytes));
    data["images"][0]["tx_sha256"] = json!(wire_hash(&bytes));
    let offset = digest_offset(&bytes).unwrap();
    bytes[offset..offset + 32].copy_from_slice(
        &sha_bytes(&data["personalization"]["parent_calibration_sha256"]).unwrap(),
    );
    data["personalization"]["parent_image_sha256"] = json!(hash(&bytes));
    fs::write(&sidecar, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(prepare_selected(&image.path, None).is_err());
}

#[test]
fn personal_sidecar_and_archived_source_are_required_and_revalidated() {
    let fixture = Fixture::new();
    let image = personalize(
        &fixture.source,
        &fixture.manifest,
        &fixture.root.join("personal"),
    )
    .unwrap();
    let source_path = image.path.with_file_name(SOURCE_BIN);
    fs::remove_file(&source_path).unwrap();
    assert!(prepare_selected(&image.path, None).is_err());
    assert!(image.revalidate().is_err());
    fs::write(source_path, &fixture.source.bytes).unwrap();

    let sidecar_path = image.path.with_file_name("catalog.json");
    let sidecar = fs::read(&sidecar_path).unwrap();
    let mut forged: Value = serde_json::from_slice(&sidecar).unwrap();
    forged["personalization"]["snapshot_manifest_sha256"] = json!("invalid");
    fs::write(&sidecar_path, serde_json::to_vec(&forged).unwrap()).unwrap();
    assert!(prepare_selected(&image.path, None).is_err());
    assert!(image.revalidate().is_err());

    fs::remove_file(&sidecar_path).unwrap();
    assert!(prepare_selected(&image.path, None).is_err());
    fs::write(sidecar_path, sidecar).unwrap();
    assert!(prepare_selected(&image.path, None)
        .unwrap()
        .revalidate()
        .is_ok());
}

#[test]
fn legacy_personal_catalog_requires_the_release_catalog() {
    let fixture = Fixture::new();
    let image = personalize(
        &fixture.source,
        &fixture.manifest,
        &fixture.root.join("personal"),
    )
    .unwrap();
    let sidecar = image.path.with_file_name("catalog.json");
    let mut data: Value = serde_json::from_slice(&fs::read(&sidecar).unwrap()).unwrap();
    data["personalization"]["schema"] = json!(1);
    fs::write(&sidecar, serde_json::to_vec(&data).unwrap()).unwrap();
    fs::remove_file(image.path.with_file_name(SOURCE_BIN)).unwrap();
    assert!(prepare_selected(&image.path, None).is_err());
    let selected = prepare_selected(&image.path, Some(&fixture.source_catalog)).unwrap();
    assert!(matches!(
        selected.provenance,
        Provenance::Personal {
            legacy_catalog: Some(_),
            ..
        }
    ));
    assert!(selected.revalidate().is_ok());
}

#[test]
fn independent_source_changed_after_selection_cannot_be_personalized() {
    let fixture = Fixture::new();
    let source = prepare_selected(&fixture.source.path, None).unwrap();
    let mut changed = source.bytes.clone();
    changed[2000] ^= 1;
    fs::write(&source.path, changed).unwrap();
    let output = fixture.root.join("personal");
    assert!(personalize(&source, &fixture.manifest, &output).is_err());
    assert!(!output.exists());
}
