//! Local personalization of a catalog-verified application. Never opens a port.
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

struct VerifiedSnapshot {
    manifest_sha256: String,
    nor_sha256: String,
    calibration: [u8; 32],
}

fn digest_offset(bytes: &[u8]) -> Result<usize, LocalizedText> {
    let mut locations = bytes
        .windows(MAGIC.len())
        .enumerate()
        .filter_map(|(offset, value)| (value == MAGIC).then_some(offset + MAGIC.len()));
    let offset = locations.next().ok_or_else(|| msg!(
        "Образ не поддерживает персонализацию. Выберите исходный R36 или новее с профилем калибровок.",
        "This image does not support personalization. Select an original R36 or later image with a calibration profile.",
        "Dieses Image unterstützt keine Personalisierung. Wählen Sie ein Original-Image ab R36 mit Kalibrierprofil.",
    ))?;
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
    let checked = prepare_with_catalog(&source.path, &source.catalog)?;
    if checked.bytes != source.bytes || checked.sha256 != source.sha256 {
        return Err(msg!(
            "Исходный образ изменился после выбора",
            "The source image changed after selection",
            "Das Quell-Image wurde nach der Auswahl verändert"
        ));
    }
    let original_catalog: Value = serde_json::from_slice(&bounded_read(&source.catalog, 65536)?)
        .map_err(|error| error.to_string())?;
    if !original_catalog["personalization"].is_null() {
        return Err(msg!(
            "Для новой персонализации выберите исходный образ комплекта",
            "Select an original bundle image to create a new personalized image",
            "Wählen Sie für eine neue Personalisierung ein Original-Image des Pakets"
        ));
    }
    let offset = digest_offset(&checked.bytes)?;
    let snapshot = verify_snapshot(manifest)?;
    let original_digest = &checked.bytes[offset..offset + 32];
    let mut bytes = checked.bytes.clone();
    bytes[offset..offset + 32].copy_from_slice(&snapshot.calibration);
    validate_structure(&bytes)?;
    let calibration_sha256 = snapshot
        .calibration
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let catalog_data = json!({
        "schema":1, "profile":PROFILE, "default_image":"firmware/UV98-personal.bin",
        "personalization":{
            "schema":1,"method":METHOD,"created_unix_ms":ms(),
            "parent_image_sha256":checked.sha256,
            "parent_calibration_sha256":original_digest.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "calibration_sha256":calibration_sha256,
            "snapshot_manifest_sha256":snapshot.manifest_sha256,"nor_sha256":snapshot.nor_sha256,
            "verified_two_reads":true,"mcu_flash_read":false,
            "board_scope":"same independently verified UV98/RT880 board revision only; snapshot does not prove hardware compatibility"
        },
        "images":[{"label":format!("{}{PERSONAL_LABEL_SUFFIX}",checked.label),"path":"firmware/UV98-personal.bin",
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
    let image_path = folder.join("UV98-personal.bin");
    let catalog_path = folder.join("catalog.json");
    let mut created_files = Vec::new();
    let mut created_folder = false;
    let result = (|| {
        fs::create_dir(&folder).map_err(|error| error.to_string())?;
        created_folder = true;
        for (path, contents) in [
            (&image_path, bytes.as_slice()),
            (&catalog_path, encoded_catalog.as_slice()),
        ] {
            let mut file = new_file(path)?;
            created_files.push(path.clone());
            durable(&mut file, contents)?;
        }
        // The same validation path used by a later CPS session must accept the
        // derivative and its exact, trusted parent before reporting success.
        prepare_saved(&image_path, &source.catalog)
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

/// Reopening a personal image does not let a self-written catalog replace the
/// release allowlist. Restoring the old digest must reproduce an allowed BIN.
pub(super) fn prepare_saved(
    path: &Path,
    release_catalog: &Path,
) -> Result<PreparedImage, LocalizedText> {
    let own_catalog = path
        .parent()
        .ok_or_else(|| {
            msg!(
                "Нет папки образа",
                "Image folder is missing",
                "Image-Ordner fehlt"
            )
        })?
        .join("catalog.json");
    let mut prepared = prepare_with_catalog(path, &own_catalog)?;
    let data: Value = serde_json::from_slice(&bounded_read(&own_catalog, 65536)?)
        .map_err(|error| error.to_string())?;
    let provenance = &data["personalization"];
    if provenance["schema"] != 1
        || provenance["method"] != METHOD
        || provenance["verified_two_reads"] != true
    {
        return Err(msg!(
            "Нет подтверждённого происхождения персонального образа",
            "The personalized image has no verified provenance",
            "Die Herkunft des personalisierten Images ist nicht bestätigt"
        ));
    }
    let offset = digest_offset(&prepared.bytes)?;
    if prepared.bytes[offset..offset + 32] != sha_bytes(&provenance["calibration_sha256"])? {
        return Err(msg!(
            "Профиль BIN не совпадает с каталогом",
            "The profile in the BIN does not match the catalog",
            "Das Profil in der BIN stimmt nicht mit dem Katalog überein"
        ));
    }
    let mut restored = prepared.bytes.clone();
    restored[offset..offset + 32]
        .copy_from_slice(&sha_bytes(&provenance["parent_calibration_sha256"])?);
    let parent_sha = hash(&restored);
    if provenance["parent_image_sha256"].as_str() != Some(&parent_sha) {
        return Err(msg!(
            "Изменения BIN выходят за границы профиля калибровок",
            "BIN changes extend beyond the calibration profile",
            "Änderungen an der BIN gehen über das Kalibrierprofil hinaus"
        ));
    }
    let trusted: Value = serde_json::from_slice(&bounded_read(release_catalog, 65536)?)
        .map_err(|error| error.to_string())?;
    if trusted["schema"] != 1 || trusted["profile"] != PROFILE {
        return Err(msg!(
            "Неподдерживаемый каталог комплекта",
            "Unsupported bundle catalog",
            "Paketkatalog wird nicht unterstützt"
        ));
    }
    let entry = trusted["images"]
        .as_array()
        .ok_or_else(|| {
            msg!(
                "Нет images в каталоге",
                "The catalog is missing images",
                "Im Katalog fehlt images"
            )
        })?
        .iter()
        .find(|entry| entry["sha256"].as_str() == Some(&parent_sha))
        .ok_or_else(|| {
            msg!(
                "Исходный BIN персонального образа отсутствует в каталоге этого комплекта",
                "The source BIN of the personalized image is missing from this bundle's catalog",
                "Die Quell-BIN des personalisierten Images fehlt im Katalog dieses Pakets"
            )
        })?;
    if entry["bytes"].as_u64() != Some(IMAGE_BYTES as u64)
        || entry["tx_sha256"].as_str() != Some(&wire_hash(&restored))
    {
        return Err(msg!(
            "Исходный BIN не соответствует каталогу комплекта",
            "The source BIN does not match the bundle catalog",
            "Die Quell-BIN stimmt nicht mit dem Paketkatalog überein"
        ));
    }
    prepared.personalized = true;
    Ok(prepared)
}

#[cfg(test)]
mod tests;
