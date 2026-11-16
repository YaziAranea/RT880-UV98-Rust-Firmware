//! Local personalization of a supported application image. Never opens a port.
//! A verified snapshot establishes data consistency, not board compatibility or
//! measured RF calibration quality. No radio dump is copied into the output.
use super::*;
use serde_json::Value;
use uv98_radio_read::{BLOCK_SIZE, CONNECT, END, FRAME_SIZE};

pub const MAGIC: &[u8; 16] = b"UV98CALPROFILE1\0";
const STORAGE_BYTES: usize = 4 * 1024 * 1024;
const STORAGE_BLOCKS: usize = STORAGE_BYTES / BLOCK_SIZE;
const CAL_BYTES: usize = uv98_calibration::BLOCK_LEN;
const METHOD: &str = "uv98-calibration-sha256-v1";
const PERSONAL_BIN: &str = "UV98-personal.bin";
const SOURCE_BIN: &str = "UV98-source.bin";

struct VerifiedSnapshot {
    manifest_sha256: String,
    nor_sha256: String,
    calibration: [u8; 32],
}

pub(super) fn digest_offset(bytes: &[u8]) -> Result<usize, LocalizedText> {
    let mut locations = bytes
        .windows(MAGIC.len())
        .enumerate()
        .filter_map(|(offset, value)| (value == MAGIC).then_some(offset + MAGIC.len()));
    let offset = locations.next().ok_or_else(|| {
        msg!(
            "В BIN нет поддерживаемого профиля калибровок UV98CALPROFILE1.",
            "The BIN has no supported UV98CALPROFILE1 calibration profile.",
            "Die BIN enthält kein unterstütztes Kalibrierprofil UV98CALPROFILE1.",
        )
    })?;
    if locations.next().is_some() || offset + 32 > bytes.len() {
        return Err(msg!(
            "Профиль калибровок в BIN повреждён или неоднозначен",
            "The calibration profile in the BIN is corrupt or ambiguous",
            "Das Kalibrierprofil in der BIN ist beschädigt oder mehrdeutig"
        ));
    }
    Ok(offset)
}

fn decode_hex(value: &str) -> Result<Vec<u8>, LocalizedText> {
    if !value.len().is_multiple_of(2) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(msg!(
            "Некорректное шестнадцатеричное поле снимка/каталога",
            "Invalid hexadecimal field in snapshot or catalog",
            "Ungültiges Hexadezimalfeld im Abbild oder Katalog"
        ));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
                .map_err(|error| error.to_string().into())
        })
        .collect()
}

fn sha_bytes(value: &Value) -> Result<[u8; 32], LocalizedText> {
    decode_hex(
        value
            .as_str()
            .ok_or_else(|| msg!("Отсутствует SHA256", "Missing SHA256", "SHA256 fehlt"))?,
    )?
    .try_into()
    .map_err(|_| {
        msg!(
            "SHA256 должен содержать 32 байта",
            "SHA256 must contain 32 bytes",
            "SHA256 muss 32 Byte enthalten"
        )
    })
}

fn checked_file(
    directory: &Path,
    filename: &str,
    expected_bytes: usize,
    expected_sha: &Value,
) -> Result<Vec<u8>, LocalizedText> {
    let bytes = bounded_read(&directory.join(filename), expected_bytes)?;
    if bytes.len() != expected_bytes || Sha256::digest(&bytes)[..] != sha_bytes(expected_sha)? {
        return Err(msgf!(
            "Снимок повреждён: размер или SHA256 {filename} не совпадает",
            "Snapshot is corrupt: size or SHA256 of {filename} does not match",
            "Abbild ist beschädigt: Größe oder SHA256 von {filename} stimmt nicht überein"
        ));
    }
    Ok(bytes)
}

fn verify_pass(
    directory: &Path,
    report: &Value,
    number: usize,
    storage: &[u8],
) -> Result<(), LocalizedText> {
    if report["pass"].as_u64() != Some(number as u64)
        || report["complete"] != true
        || report["end_sent"] != true
        || !report["cleanup_error"].is_null()
    {
        return Err(msgf!(
            "Проход {number} не завершён корректно",
            "Pass {number} did not finish correctly",
            "Lesedurchlauf {number} wurde nicht korrekt abgeschlossen"
        ));
    }
    let packets = report["tx_packets"].as_array().ok_or_else(|| {
        msg!(
            "Нет журнала запросов чтения",
            "Read request log is missing",
            "Protokoll der Leseanforderungen fehlt"
        )
    })?;
    // A successful stock session has one to three CONNECT attempts, every
    // bounded READ once in order, and one END. No writes/erase are accepted.
    if !(STORAGE_BLOCKS + 2..=STORAGE_BLOCKS + 4).contains(&packets.len()) {
        return Err(msg!(
            "Неверное число команд в полном снимке",
            "Invalid command count in the full snapshot",
            "Ungültige Befehlsanzahl im vollständigen Abbild"
        ));
    }
    let connects = packets.len() - STORAGE_BLOCKS - 1;
    let mut tx = Vec::with_capacity(STORAGE_BLOCKS * 4 + 20);
    for (index, packet) in packets.iter().enumerate() {
        let packet = decode_hex(packet.as_str().ok_or_else(|| {
            msg!(
                "Некорректная команда снимка",
                "Invalid snapshot command",
                "Ungültiger Befehl im Abbild"
            )
        })?)?;
        let expected = if index < connects {
            CONNECT.to_vec()
        } else if index == packets.len() - 1 {
            END.to_vec()
        } else {
            let block = (index - connects) as u16;
            let mut request = vec![0x52, (block >> 8) as u8, block as u8];
            request.push(request.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
            request
        };
        if packet != expected {
            return Err(msg!(
                "Снимок содержит неподдерживаемую последовательность команд",
                "The snapshot contains an unsupported command sequence",
                "Das Abbild enthält eine nicht unterstützte Befehlsfolge"
            ));
        }
        tx.extend(packet);
    }
    let prefix = format!("pass-{number:02}");
    if checked_file(
        directory,
        &format!("{prefix}.tx.bin"),
        tx.len(),
        &report["tx_sha256"],
    )? != tx
    {
        return Err(msg!(
            "Журнал TX не совпадает с manifest.json",
            "TX log does not match manifest.json",
            "TX-Protokoll stimmt nicht mit manifest.json überein"
        ));
    }
    let frames = checked_file(
        directory,
        &format!("{prefix}.frames.bin"),
        STORAGE_BLOCKS * FRAME_SIZE,
        &report["frames_sha256"],
    )?;
    let metadata = report["frames"].as_array().ok_or_else(|| {
        msg!(
            "Нет метаданных кадров",
            "Frame metadata is missing",
            "Frame-Metadaten fehlen"
        )
    })?;
    if metadata.len() != STORAGE_BLOCKS {
        return Err(msg!(
            "Неполный список кадров NOR",
            "The list of NOR frames is incomplete",
            "Die Liste der NOR-Frames ist unvollständig"
        ));
    }
    for (block, frame) in frames.chunks_exact(FRAME_SIZE).enumerate() {
        let item = &metadata[block];
        if item["block"].as_u64() != Some(block as u64)
            || item["frame_bytes"].as_u64() != Some(FRAME_SIZE as u64)
            || item["validated"] != true
            || Sha256::digest(frame)[..] != sha_bytes(&item["sha256"])?
        {
            return Err(msgf!(
                "Повреждены метаданные кадра {block}, проход {number}",
                "Corrupt metadata for frame {block}, pass {number}",
                "Beschädigte Metadaten für Frame {block}, Durchlauf {number}"
            ));
        }
        let payload = uv98_radio_read::validate_frame(block as u16, frame)
            .map_err(|error| crate::radio::localize_read_error(&error))?;
        if payload != &storage[block * BLOCK_SIZE..(block + 1) * BLOCK_SIZE] {
            return Err(msgf!(
                "Проход {number}, блок {block}: данные отличаются от сохранённой NOR",
                "Pass {number}, block {block}: data differs from the saved NOR image",
                "Durchlauf {number}, Block {block}: Daten weichen vom gespeicherten NOR-Abbild ab"
            ));
        }
    }
    // CONNECT may retry after a timeout (no byte) or a zero reply. Successful
    // RX ends with exactly the stored frames and begins with 0..2 zeroes + ACK.
    let rx = bounded_read(
        &directory.join(format!("{prefix}.rx.bin")),
        frames.len() + 3,
    )?;
    if rx.len() <= frames.len()
        || rx.len() > frames.len() + connects
        || Sha256::digest(&rx)[..] != sha_bytes(&report["rx_sha256"])?
    {
        return Err(msg!(
            "Повреждён журнал RX полного снимка",
            "The RX log for the full snapshot is corrupt",
            "Das RX-Protokoll des vollständigen Abbilds ist beschädigt"
        ));
    }
    let ack_len = rx.len() - frames.len();
    if rx[ack_len - 1] != 0x06
        || rx[..ack_len - 1].iter().any(|b| *b != 0)
        || rx[ack_len..] != frames
    {
        return Err(msg!(
            "Журнал RX не подтверждает сохранённые кадры",
            "The RX log does not confirm the saved frames",
            "Das RX-Protokoll bestätigt die gespeicherten Frames nicht"
        ));
    }
    Ok(())
}

fn verify_snapshot(manifest: &Path) -> Result<VerifiedSnapshot, LocalizedText> {
    let manifest_bytes = bounded_read(manifest, 4 * 1024 * 1024)?;
    let data: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| msgf!("Снимок: {error}", "Snapshot: {error}", "Abbild: {error}"))?;
    if data["schema"] != 1
        || data["status"] != "verified_two_reads"
        || data["two_reads_byte_identical"] != true
        || data["plan"] != "stock_protocol_4_mib"
        || data["write_or_erase_commands_sent"] != false
    {
        return Err(msg!(
            "Нужен полный снимок CPS 4 МиБ: два завершённых, совпавших чтения. Снимка только калибровок недостаточно.",
            "A full 4 MiB CPS snapshot from two complete, matching reads is required. A calibration-only snapshot is insufficient.",
            "Erforderlich ist ein vollständiges CPS-Abbild mit 4 MiB aus zwei abgeschlossenen, übereinstimmenden Lesedurchläufen. Ein reines Kalibrierabbild reicht nicht aus.",
        ));
    }
    let directory = manifest.parent().ok_or_else(|| {
        msg!(
            "Нет папки снимка",
            "Snapshot folder is missing",
            "Ordner des Abbilds fehlt"
        )
    })?;
    let sections = data["sections"].as_array().ok_or_else(|| {
        msg!(
            "Нет секций снимка",
            "Snapshot sections are missing",
            "Abschnitte des Abbilds fehlen"
        )
    })?;
    if sections.len() != 2 {
        return Err(msg!(
            "Неверный состав полного снимка CPS",
            "Invalid set of sections in the full CPS snapshot",
            "Ungültige Abschnittszusammenstellung im vollständigen CPS-Abbild"
        ));
    }
    let section = |name: &str, length: usize| -> Result<Vec<u8>, LocalizedText> {
        let mut matches = sections.iter().filter(|s| s["file"].as_str() == Some(name));
        let entry = matches.next().ok_or_else(|| {
            msgf!(
                "Нет {name} в снимке",
                "{name} is missing from the snapshot",
                "{name} fehlt im Abbild"
            )
        })?;
        if matches.next().is_some()
            || entry["first_block"] != 0
            || entry["bytes"].as_u64() != Some(length as u64)
        {
            return Err(msgf!(
                "Неверная геометрия {name}",
                "Invalid block layout for {name}",
                "Ungültige Blockstruktur für {name}"
            ));
        }
        checked_file(directory, name, length, &entry["sha256"])
    };
    let storage = section("storage-address-space.bin", STORAGE_BYTES)?;
    let calibration = section("calibration.bin", CAL_BYTES)?;
    if storage[..CAL_BYTES] != calibration {
        return Err(msg!(
            "calibration.bin не совпадает с началом полной NOR",
            "calibration.bin does not match the start of the full NOR image",
            "calibration.bin stimmt nicht mit dem Anfang des vollständigen NOR-Abbilds überein"
        ));
    }
    uv98_calibration::Calibration::parse(&calibration).map_err(|error| {
        let detail = crate::calibration_report::block_error(error);
        LocalizedText::from_fn(|lang| {
            trf!(
                lang,
                "Неверный блок калибровок: {detail}",
                "Invalid calibration block: {detail}",
                "Ungültiger Kalibrierblock: {detail}",
                detail = detail.as_str(lang),
            )
        })
    })?;
    let known = &calibration[1..uv98_calibration::KNOWN_LAYOUT_END];
    if known.iter().all(|b| *b == 0) || known.iter().all(|b| *b == 0xff) {
        return Err(msg!(
            "Калибровки пусты: блок из нулей/FF с одним маркером не принимается",
            "Calibration data is blank: a block of zeros or FF bytes with only a marker is rejected",
            "Kalibrierdaten sind leer: Ein Block aus Nullen oder FF-Bytes mit nur einer Kennung wird abgewiesen",
        ));
    }
    let passes = data["passes"].as_array().ok_or_else(|| {
        msg!(
            "Нет двух журналов чтения",
            "Two read logs are required",
            "Zwei Leseprotokolle sind erforderlich"
        )
    })?;
    if passes.len() != 2 {
        return Err(msg!(
            "Нужны ровно два полных прохода чтения",
            "Exactly two full read passes are required",
            "Genau zwei vollständige Lesedurchläufe sind erforderlich"
        ));
    }
    verify_pass(directory, &passes[0], 1, &storage)?;
    verify_pass(directory, &passes[1], 2, &storage)?;
    Ok(VerifiedSnapshot {
        manifest_sha256: hash(&manifest_bytes),
        nor_sha256: hash(&storage),
        calibration: Sha256::digest(&calibration).into(),
    })
}

/// Output must not exist. A failed save removes only paths this call created;
/// no recursive removal is used, so unrelated files can never be swept away.
pub fn personalize(
    source: &PreparedImage,
    manifest: &Path,
    output: &Path,
) -> Result<PreparedImage, LocalizedText> {
    if !source.is_personalization_source() {
        return Err(msg!(
            "Для персонализации выберите исходный BIN с профилем калибровок",
            "Select an original BIN with a calibration profile to personalize",
            "Wählen Sie zur Personalisierung eine ursprüngliche BIN mit Kalibrierprofil"
        ));
    }
    source.revalidate()?;
    let offset = digest_offset(&source.bytes)?;
    let snapshot = verify_snapshot(manifest)?;
    let original_digest = &source.bytes[offset..offset + 32];
    let mut bytes = source.bytes.clone();
    bytes[offset..offset + 32].copy_from_slice(&snapshot.calibration);
    digest_offset(&bytes)?;
    let calibration_sha256 = snapshot
        .calibration
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let relative_image = format!("firmware/{PERSONAL_BIN}");
    let catalog_data = json!({
        "schema":1, "profile":PROFILE, "default_image":relative_image,
        "personalization":{
            "schema":2,"method":METHOD,"created_unix_ms":ms(),
            "parent_image_sha256":source.sha256,
            "parent_calibration_sha256":original_digest.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "calibration_sha256":calibration_sha256,
            "snapshot_manifest_sha256":snapshot.manifest_sha256,"nor_sha256":snapshot.nor_sha256,
            "verified_two_reads":true,"mcu_flash_read":false,
            "board_scope":"same independently verified UV98/RT880 board revision only; snapshot does not prove hardware compatibility"
        },
        "images":[{"label":source.label,"path":relative_image,
            "sha256":hash(&bytes),"bytes":IMAGE_BYTES,"tx_sha256":wire_hash(&bytes)}]
    });
    let encoded_catalog =
        serde_json::to_vec_pretty(&catalog_data).map_err(|error| error.to_string())?;
    fs::create_dir(output).map_err(|error| {
        msgf!(
            "Нужна новая, ещё не существующая папка: {}: {error}",
            "A new folder is required: {}: {error}",
            "Ein neuer Ordner ist erforderlich: {}: {error}",
            output.display()
        )
    })?;
    let folder = output.join("firmware");
    let image_path = folder.join(PERSONAL_BIN);
    let source_path = folder.join(SOURCE_BIN);
    let catalog_path = folder.join("catalog.json");
    let mut created_files = Vec::new();
    let mut created_folder = false;
    let result = (|| {
        fs::create_dir(&folder).map_err(|error| error.to_string())?;
        created_folder = true;
        for (path, contents) in [
            (&source_path, source.bytes.as_slice()),
            (&image_path, bytes.as_slice()),
            (&catalog_path, encoded_catalog.as_slice()),
        ] {
            let mut file = new_file(path)?;
            created_files.push(path.clone());
            durable(&mut file, contents)?;
        }
        // A later CPS session needs only the files in this folder.
        prepare_selected(&image_path, None)
    })();
    if result.is_err() {
        for path in created_files.iter().rev() {
            let _ = fs::remove_file(path);
        }
        if created_folder {
            let _ = fs::remove_dir(&folder);
        }
        let _ = fs::remove_dir(output);
    }
    result
}

pub(super) fn read_sidecar(path: &Path) -> Result<Vec<u8>, LocalizedText> {
    bounded_read(&path.with_file_name("catalog.json"), 65536)
}

pub(super) fn saved_sidecar(
    image: &PreparedImage,
) -> Result<Option<(Vec<u8>, Value)>, LocalizedText> {
    let sidecar = read_sidecar(&image.path).and_then(|bytes| {
        let data = parse_catalog(&bytes)?;
        Ok((bytes, data))
    });
    if image
        .path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(std::ffi::OsStr::new(PERSONAL_BIN)))
    {
        // A missing or broken sidecar must fail closed for a saved BIN.
        return sidecar.map(Some);
    }
    // A renamed personal BIN can still be recognized by its exact hash.
    Ok(sidecar.ok().filter(|(_, data)| {
        !data["personalization"].is_null() && catalog_entry(data, &image.sha256).is_some()
    }))
}

pub(super) fn prepare_saved(
    mut image: PreparedImage,
    sidecar: &[u8],
    data: &Value,
    release_catalog: Option<&Path>,
) -> Result<PreparedImage, LocalizedText> {
    verify_saved(
        &image.path,
        &image.bytes,
        &image.sha256,
        &image.tx_sha256,
        data,
        release_catalog,
    )?;
    image.provenance = Provenance::Personal {
        sidecar_sha256: hash(sidecar),
        legacy_catalog: release_catalog
            .filter(|_| data["personalization"]["schema"] == 1)
            .map(Path::to_path_buf),
    };
    Ok(image)
}

/// New files carry the exact original BIN and recorded snapshot digests. This
/// verifies the byte-for-byte derivation, not a signature on the local files.
/// Earlier personal catalogs require their original release catalog.
pub(super) fn verify_saved(
    path: &Path,
    bytes: &[u8],
    sha256: &str,
    tx_sha256: &str,
    data: &Value,
    release_catalog: Option<&Path>,
) -> Result<(), LocalizedText> {
    verify_entry(data, sha256, tx_sha256)?;
    let provenance = &data["personalization"];
    if provenance["method"] != METHOD
        || provenance["verified_two_reads"] != true
        || provenance["mcu_flash_read"] != false
    {
        return Err(msg!(
            "В каталоге личного BIN отсутствуют сведения о проверке снимка",
            "The personal BIN catalog is missing the snapshot verification record",
            "Im Katalog der persönlichen BIN fehlen Angaben zur Prüfung des Abbilds"
        ));
    }
    sha_bytes(&provenance["snapshot_manifest_sha256"])?;
    sha_bytes(&provenance["nor_sha256"])?;
    let offset = digest_offset(bytes)?;
    if bytes[offset..offset + 32] != sha_bytes(&provenance["calibration_sha256"])? {
        return Err(msg!(
            "Профиль BIN не совпадает с каталогом",
            "The profile in the BIN does not match the catalog",
            "Das Profil in der BIN stimmt nicht mit dem Katalog überein"
        ));
    }
    let mut restored = bytes.to_vec();
    restored[offset..offset + 32]
        .copy_from_slice(&sha_bytes(&provenance["parent_calibration_sha256"])?);
    let parent_sha = hash(&restored);
    if provenance["parent_image_sha256"].as_str() != Some(&parent_sha) {
        return Err(source_mismatch());
    }
    match provenance["schema"].as_u64() {
        Some(2) => {
            let source = bounded_read(&path.with_file_name(SOURCE_BIN), IMAGE_BYTES)?;
            if source != restored || digest_offset(&source)? != offset {
                return Err(source_mismatch());
            }
        }
        Some(1) => {
            let release_catalog = release_catalog.ok_or_else(|| msg!(
                "Для личного BIN от старой CPS нужен каталог выпуска с исходным BIN",
                "A personal BIN from an older CPS needs the release catalog that lists its source BIN",
                "Eine persönliche BIN aus einer älteren CPS benötigt den Versionskatalog mit der Quell-BIN"
            ))?;
            bundle_entry(
                &read_catalog(release_catalog)?,
                &parent_sha,
                &wire_hash(&restored),
            )?;
        }
        _ => {
            return Err(msg!(
                "Версия каталога личного BIN не поддерживается",
                "Unsupported personal BIN catalog version",
                "Katalogversion der persönlichen BIN wird nicht unterstützt"
            ))
        }
    }
    Ok(())
}

fn source_mismatch() -> LocalizedText {
    msg!(
        "Личный BIN отличается от исходного за пределами профиля калибровок",
        "The personal BIN differs from its source outside the calibration profile",
        "Die persönliche BIN weicht außerhalb des Kalibrierprofils von der Quell-BIN ab"
    )
}

#[cfg(test)]
mod tests;
