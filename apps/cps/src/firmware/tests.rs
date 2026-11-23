use super::*;
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn release_default_requires_catalog_match_and_stays_inside_bundle() {
    let fixture = Fixture::new();
    let folder = fixture.dir.join("firmware");
    fs::create_dir(&folder).unwrap();
    let path = folder.join("UV98-r35.bin");
    fs::copy(&fixture.image.path, &path).unwrap();
    let catalog = folder.join("catalog.json");
    let mut data: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.dir.join("catalog.json")).unwrap()).unwrap();
    data["default_image"] = json!("firmware/UV98-r35.bin");
    fs::write(&catalog, serde_json::to_vec(&data).unwrap()).unwrap();
    assert_eq!(default_image(&fixture.dir).unwrap().path, path);
    data["default_image"] = json!("../app.bin");
    fs::write(&catalog, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(default_image(&fixture.dir).is_none());
    data["default_image"] = json!("firmware/UV98-r35.bin");
    fs::write(&catalog, serde_json::to_vec(&data).unwrap()).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    bytes[1000] ^= 1;
    fs::write(&path, bytes).unwrap();
    assert!(default_image(&fixture.dir).is_none());
}
struct Fixture {
    dir: PathBuf,
    image: PreparedImage,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "uv98-flash-test-{}-{}-{}",
            std::process::id(),
            ms(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let mut bytes: Vec<u8> = (0..IMAGE_BYTES)
            .map(|i| (i.wrapping_mul(17) & 255) as u8)
            .collect();
        bytes[..4].copy_from_slice(&0x2000_c000u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&0x0800_2901u32.to_le_bytes());
        let path = dir.join("app.bin");
        fs::write(&path, &bytes).unwrap();
        let catalog = dir.join("catalog.json");
        fs::write(&catalog,serde_json::to_vec(&json!({"schema":1,"profile":PROFILE,"images":[{"label":"TEST ONLY","bytes":IMAGE_BYTES,"sha256":hash(&bytes),"tx_sha256":wire_hash(&bytes)}]})).unwrap()).unwrap();
        let image = prepare_with_catalog(&path, &catalog).unwrap();
        Self { dir, image }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
#[derive(Default)]
struct FakeState {
    sent: Vec<Vec<u8>>,
    queue: VecDeque<u8>,
    bad_ack: Option<usize>,
    short: Option<usize>,
    missing_final: bool,
    closed: bool,
    close_error: bool,
    read_calls: usize,
    read_error: Option<usize>,
    write_error: Option<usize>,
}
struct Fake(Rc<RefCell<FakeState>>);
impl Transport for Fake {
    fn read_byte(&mut self, _: Duration) -> io::Result<Option<u8>> {
        let mut s = self.0.borrow_mut();
        s.read_calls += 1;
        if s.read_error == Some(s.read_calls) {
            return Err(io::Error::other("injected read failure"));
        }
        Ok(s.queue.pop_front())
    }
    fn write_frame(&mut self, data: &[u8]) -> io::Result<usize> {
        let mut s = self.0.borrow_mut();
        s.sent.push(data.to_vec());
        let n = s.sent.len();
        if s.write_error == Some(n) {
            return Err(io::Error::other("injected write failure"));
        }
        let ack = if s.bad_ack == Some(n) { 0xff } else { 6 };
        s.queue.push_back(ack);
        if n == 249 && !s.missing_final {
            s.queue.push_back(6);
        }
        Ok(data.len() - usize::from(s.short == Some(n)))
    }
    fn close(&mut self) -> io::Result<()> {
        let mut s = self.0.borrow_mut();
        s.closed = true;
        if s.close_error {
            Err(io::Error::other("close failed"))
        } else {
            Ok(())
        }
    }
}
fn invoke(f: &Fixture, s: Rc<RefCell<FakeState>>) -> Result<FlashReport, LocalizedText> {
    run(
        f.image.clone(),
        "FAKE",
        &f.dir.join("log"),
        move |_| Ok(Fake(s)),
        |_| {},
    )
}

#[test]
fn exact_wire_stream_ack_and_image_reconstruction() {
    let f = Fixture::new();
    let state = Rc::new(RefCell::new(FakeState::default()));
    let report = invoke(&f, state.clone()).unwrap();
    let s = state.borrow();
    assert!(s.closed);
    assert_eq!(report.confirmed_blocks, 246);
    assert_eq!(s.sent.len(), 249);
    assert_eq!(
        s.sent[..3],
        [
            vec![0x39, 0x33, 5, 0x10, 0x81],
            vec![0x39, 0x33, 5, 0x10, 0x81],
            vec![0x39, 0x33, 5, 0x55, 0xc6]
        ]
    );
    for (i, p) in s.sent[3..].iter().enumerate() {
        assert_eq!(p.len(), 1028);
        assert_eq!(p[..3], [0x57, ((i * 1024) >> 8) as u8, 0]);
        assert_eq!(
            p[1027],
            p[..1027].iter().fold(0u8, |a, b| a.wrapping_add(*b))
        );
    }
    let restored: Vec<u8> = s.sent[3..]
        .iter()
        .flat_map(|p| p[3..1027].iter().copied())
        .collect();
    assert_eq!(restored, f.image.bytes);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(f.dir.join("log/manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["ack_count"], 250);
    assert_eq!(manifest["hardware_boot_verified"], false);
    assert_eq!(fs::read(f.dir.join("log/rx.bin")).unwrap(), vec![6; 250]);
}
#[test]
fn faults_never_retry_and_always_close() {
    for (state, expected) in [
        (
            FakeState {
                bad_ack: Some(1),
                ..Default::default()
            },
            1,
        ),
        (
            FakeState {
                bad_ack: Some(70),
                ..Default::default()
            },
            70,
        ),
        (
            FakeState {
                short: Some(8),
                ..Default::default()
            },
            8,
        ),
        (
            FakeState {
                missing_final: true,
                ..Default::default()
            },
            249,
        ),
        (
            FakeState {
                queue: VecDeque::from([6]),
                ..Default::default()
            },
            0,
        ),
        (
            FakeState {
                close_error: true,
                ..Default::default()
            },
            249,
        ),
    ] {
        let f = Fixture::new();
        let s = Rc::new(RefCell::new(state));
        assert!(invoke(&f, s.clone()).is_err());
        assert!(s.borrow().closed);
        assert_eq!(s.borrow().sent.len(), expected);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(f.dir.join("log/manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["status"], "failed");
    }
}

#[test]
fn transport_exceptions_stop_close_and_preserve_attempted_stream() {
    for (state, expected_writes, error_text) in [
        (
            FakeState {
                read_error: Some(1),
                ..Default::default()
            },
            0,
            "read failure",
        ),
        (
            FakeState {
                read_error: Some(2),
                ..Default::default()
            },
            1,
            "read failure",
        ),
        (
            FakeState {
                read_error: Some(71),
                ..Default::default()
            },
            70,
            "read failure",
        ),
        (
            FakeState {
                write_error: Some(1),
                ..Default::default()
            },
            1,
            "write failure",
        ),
        (
            FakeState {
                write_error: Some(70),
                ..Default::default()
            },
            70,
            "write failure",
        ),
    ] {
        let fixture = Fixture::new();
        let state = Rc::new(RefCell::new(state));
        let error = invoke(&fixture, state.clone()).err().expect("must fail");
        assert!(error.as_str(Language::Russian).contains(error_text));
        let state = state.borrow();
        assert!(state.closed);
        assert_eq!(
            state.sent.len(),
            expected_writes,
            "no retry after an I/O exception"
        );
        let attempted: Vec<u8> = state.sent.iter().flatten().copied().collect();
        assert_eq!(
            fs::read(fixture.dir.join("log/tx-attempted.bin")).unwrap(),
            attempted
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.dir.join("log/manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["status"], "failed");
        assert_eq!(manifest["port_closed"], true);
        assert_eq!(manifest["erase_sequence_attempted"], expected_writes != 0);
        let journal = fs::read_to_string(fixture.dir.join("log/journal.jsonl")).unwrap();
        let returns: Vec<serde_json::Value> = journal
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|event| event["event"] == "tx_return")
            .collect();
        assert_eq!(returns.len(), expected_writes);
        if state.write_error.is_some() {
            let last = returns.last().unwrap();
            assert!(last["reported_written"].is_null());
            assert!(last["error"].as_str().unwrap().contains("write failure"));
        }
    }
}

#[test]
fn primary_transfer_and_close_failure_are_both_recorded() {
    let fixture = Fixture::new();
    let state = Rc::new(RefCell::new(FakeState {
        bad_ack: Some(1),
        close_error: true,
        ..Default::default()
    }));
    let error = invoke(&fixture, state).err().expect("must fail");
    assert!(error.as_str(Language::Russian).contains("ACK 06"));
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.dir.join("log/manifest.json")).unwrap()).unwrap();
    assert!(manifest["error"].as_str().unwrap().contains("ACK 06"));
    assert_eq!(manifest["close_error"], "close failed");
    assert_eq!(manifest["port_closed"], false);
}

#[test]
fn final_journal_failure_after_transfer_never_returns_success_or_leaks_port() {
    let fixture = Fixture::new();
    let directory = fixture.dir.join("log");
    let state = Rc::new(RefCell::new(FakeState::default()));
    let transport = state.clone();
    let mut injected = false;
    let result = run(
        fixture.image.clone(),
        "FAKE",
        &directory,
        move |_| Ok(Fake(transport)),
        |progress| {
            if progress.destructive_started && !injected {
                // A filesystem failure at final report creation after erase
                // must not be reported as successful completion. No production
                // serial handle or system-wide disk state is touched.
                fs::create_dir(directory.join("manifest.json")).unwrap();
                injected = true;
            }
        },
    );
    assert!(injected);
    assert!(result.is_err());
    assert!(state.borrow().closed);
    assert_eq!(state.borrow().sent.len(), 249);
    assert_eq!(fs::read(directory.join("rx.bin")).unwrap(), vec![6; 250]);
    assert!(directory.join("started.json").is_file());
    assert!(directory.join("tx-attempted.bin").is_file());
    let journal = fs::read_to_string(directory.join("journal.jsonl")).unwrap();
    assert_eq!(
        journal
            .lines()
            .filter(|line| line.contains("tx_return"))
            .count(),
        249
    );
}
#[test]
fn changed_image_and_catalog_fail_before_port_open() {
    for change_catalog in [false, true] {
        let f = Fixture::new();
        if change_catalog {
            fs::write(&f.dir.join("catalog.json"), b"{}").unwrap();
        } else {
            let mut b = f.image.bytes.clone();
            b[1024] ^= 1;
            fs::write(&f.image.path, b).unwrap();
        }
        assert!(run::<Fake>(
            f.image.clone(),
            "FAKE",
            &f.dir.join("log"),
            |_| panic!("opened port"),
            |_| {}
        )
        .is_err());
        assert!(!f.dir.join("log").exists());
    }
}
#[test]
fn existing_log_and_open_failure_are_not_success() {
    let f = Fixture::new();
    fs::create_dir(f.dir.join("log")).unwrap();
    assert!(run::<Fake>(
        f.image.clone(),
        "FAKE",
        &f.dir.join("log"),
        |_| panic!("opened port"),
        |_| {}
    )
    .is_err());
    assert!(run::<Fake>(
        f.image.clone(),
        "FAKE",
        &f.dir.join("open-fail"),
        |_| Err(LocalizedText::from("Windows 31").into()),
        |_| {}
    )
    .is_err());
    let m: serde_json::Value =
        serde_json::from_slice(&fs::read(f.dir.join("open-fail/manifest.json")).unwrap()).unwrap();
    assert_eq!(m["erase_sequence_attempted"], false);
    assert_eq!(m["port_closed"], false);
}
#[test]
fn malformed_size_vectors_and_unlisted_image_refused() {
    let f = Fixture::new();
    for n in [0, 1024, IMAGE_BYTES - 1, IMAGE_BYTES + 1] {
        assert!(validate_structure(&vec![0; n]).is_err());
    }
    for (offset, value) in [
        (0, 0x2000_0000u32),
        (0, 0x2000_c004),
        (4, 0x0800_2800),
        (4, 0x0800_0001),
        (4, 0x0804_0001),
    ] {
        let mut b = f.image.bytes.clone();
        b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(validate_structure(&b).is_err());
    }
    let mut b = f.image.bytes.clone();
    b[512] ^= 1;
    fs::write(&f.image.path, &b).unwrap();
    assert!(prepare_with_catalog(&f.image.path, &f.dir.join("catalog.json")).is_err());
}

#[test]
fn unverified_factory_candidate_names_the_supported_rollback_without_accepting_it() {
    let fixture = Fixture::new();
    let candidate = fixture.dir.join(STOCK_FILENAME);
    validate_structure(&fixture.image.bytes).unwrap();
    fs::write(&candidate, &fixture.image.bytes).unwrap();
    let error = match prepare_selected(&candidate, None) {
        Ok(_) => panic!("an unverified image cannot become stock by name"),
        Err(error) => error,
    };
    assert!(error
        .as_str(Language::English)
        .contains("verified stock 1.16 BIN"));
}

#[test]
fn standalone_import_checks_vectors_and_one_complete_calibration_profile() {
    let fixture = Fixture::new();
    let path = fixture.dir.join("future.bin");
    let mut bytes = fixture.image.bytes.clone();
    bytes[4..8].copy_from_slice(&0x0800_3001u32.to_le_bytes());
    bytes[1024..1040].copy_from_slice(profile::MAGIC);
    fs::write(&path, &bytes).unwrap();
    let catalog = fixture.dir.join("catalog.json");

    for optional_catalog in [None, Some(catalog.as_path())] {
        let image = prepare_selected(&path, optional_catalog).unwrap();
        assert_eq!(image.provenance, Provenance::Imported);
        assert_eq!(image.bytes, bytes);
    }
    let mut stale: serde_json::Value =
        serde_json::from_slice(&fs::read(&catalog).unwrap()).unwrap();
    stale["images"][0]["sha256"] = json!(hash(&bytes));
    // A matching catalog entry with the wrong wire checksum is inconsistent.
    fs::write(&catalog, serde_json::to_vec(&stale).unwrap()).unwrap();
    assert!(prepare_selected(&path, Some(&catalog)).is_err());
    fs::write(&catalog, b"{malformed").unwrap();
    assert_eq!(
        prepare_selected(&path, Some(&catalog)).unwrap().provenance,
        Provenance::Imported
    );

    let mut bad_reset = bytes.clone();
    bad_reset[4] &= !1;
    let mut duplicate = bytes.clone();
    duplicate[1100..1116].copy_from_slice(profile::MAGIC);
    let mut truncated_profile = bytes.clone();
    truncated_profile[1024] ^= 1;
    truncated_profile[IMAGE_BYTES - 16..].copy_from_slice(profile::MAGIC);
    for wrong in [
        bytes[..IMAGE_BYTES - 1].to_vec(),
        bad_reset,
        duplicate,
        truncated_profile,
        fixture.image.bytes.clone(),
    ] {
        fs::write(&path, wrong).unwrap();
        assert!(prepare_selected(&path, None).is_err());
    }
}

#[test]
fn standalone_import_rejects_vector_prefix_and_reset_entry_overlaps() {
    let fixture = Fixture::new();
    let path = fixture.dir.join("future.bin");
    for (marker, reset, region) in [
        (256, 0x0800_3001u32, "marker in vector prefix"),
        (1024, 0x0800_2abf, "reset in vector prefix"),
        (1024, 0x0800_2c01, "reset in marker"),
        (1024, 0x0800_2c11, "reset in digest"),
        (
            1025,
            0x0800_2c01,
            "marker overlaps second byte of reset halfword",
        ),
    ] {
        let mut bytes = fixture.image.bytes.clone();
        bytes[4..8].copy_from_slice(&reset.to_le_bytes());
        bytes[marker..marker + profile::MAGIC.len()].copy_from_slice(profile::MAGIC);
        fs::write(&path, bytes).unwrap();
        assert!(prepare_selected(&path, None).is_err(), "{region}");
    }
}

#[test]
fn standalone_transfer_refuses_changed_selection_and_uses_immutable_bytes() {
    let fixture = Fixture::new();
    let path = fixture.dir.join("future.bin");
    let mut bytes = fixture.image.bytes.clone();
    bytes[4..8].copy_from_slice(&0x0800_3001u32.to_le_bytes());
    bytes[1024..1040].copy_from_slice(profile::MAGIC);
    fs::write(&path, &bytes).unwrap();
    let image = prepare_selected(&path, None).unwrap();
    let directory = fixture.dir.join("log");
    let mut changed = bytes.clone();
    changed[1024] ^= 1;
    fs::write(&path, &changed).unwrap();
    assert!(run::<Fake>(
        image.clone(),
        "FAKE",
        &directory,
        |_| panic!("opened port"),
        |_| {},
    )
    .is_err());
    assert!(!directory.exists());

    fs::write(&path, &bytes).unwrap();
    let state = Rc::new(RefCell::new(FakeState::default()));
    run(
        image,
        "FAKE",
        &directory,
        |_| {
            // A replacement after preflight must never enter the wire stream.
            fs::write(&path, &changed).unwrap();
            Ok(Fake(state.clone()))
        },
        |_| {},
    )
    .unwrap();
    assert_eq!(
        fs::read(directory.join("tx-attempted.bin")).unwrap(),
        packets(&bytes).flatten().collect::<Vec<_>>()
    );
    let started: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("started.json")).unwrap()).unwrap();
    assert_eq!(started["image_provenance"], "structural_import");
}

/// Optional independent acceptance against the bundled real files; never opens COM.
#[test]
fn release_catalog_real_images_match_wire_oracle_when_supplied() {
    let Some(directory) = std::env::var_os("UV98_FLASH_BUNDLE") else {
        return;
    };
    let root = PathBuf::from(directory);
    let catalog = root.join("firmware/catalog.json");
    let listing: serde_json::Value = serde_json::from_slice(&fs::read(&catalog).unwrap()).unwrap();
    let images = listing["images"].as_array().unwrap();
    assert!(!images.is_empty());
    for entry in images {
        let relative = entry["path"].as_str().unwrap();
        let path = root.join(relative);
        let image = prepare_with_catalog(&path, &catalog).unwrap();
        if image.provenance == Provenance::Stock
            || image
                .bytes
                .windows(profile::MAGIC.len())
                .any(|window| window == profile::MAGIC)
        {
            let independent = prepare_selected(&path, None).unwrap();
            assert_eq!(independent.sha256, image.sha256);
            assert_eq!(independent.tx_sha256, image.tx_sha256);
            assert_eq!(
                independent.provenance,
                if image.sha256 == STOCK_SHA256 {
                    Provenance::Stock
                } else {
                    Provenance::Imported
                },
            );
        }
        let fixture = Fixture::new();
        let state = Rc::new(RefCell::new(FakeState::default()));
        let s = state.clone();
        let report = run(
            image,
            "FAKE",
            &fixture.dir.join("log"),
            move |_| Ok(Fake(s)),
            |_| {},
        )
        .unwrap();
        assert_eq!(report.confirmed_blocks, 246);
    }
}
