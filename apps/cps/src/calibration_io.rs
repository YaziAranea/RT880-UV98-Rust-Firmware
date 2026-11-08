//! Local calibration files only. No serial, RF commands, or source overwrites.
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use uv98_calibration::{Calibration, BLOCK_LEN};

pub struct Loaded {
    pub bytes: [u8; BLOCK_LEN],
    pub source_sha256: String,
    pub extracted_from_nor: bool,
}
pub fn load(path: &Path) -> Result<Loaded, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if len != BLOCK_LEN as u64 && len != 4 * 1024 * 1024 {
        return Err("Нужен блок калибровок 4096 байт или сохранённый NOR-дамп 4 МиБ. Файл CPS калибровок не содержит.".into());
    }
    let mut bytes = Vec::with_capacity(len as usize);
    file.take(len + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != len {
        return Err("Размер файла изменился во время чтения".into());
    }
    let block = Calibration::parse(&bytes[..BLOCK_LEN]).map_err(|e| e.to_string())?;
    Ok(Loaded {
        bytes: *block.raw(),
        source_sha256: crate::radio::sha256(&bytes),
        extracted_from_nor: len != BLOCK_LEN as u64,
    })
}
fn write_new(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("Не удалось создать новый файл {}: {e}", path.display()))?;
    file.write_all(data)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("Запись не завершена; файл может быть неполным: {e}"))?;
    if fs::read(path).map_err(|e| e.to_string())? != data {
        return Err("Проверка записанного файла не совпала".into());
    }
    Ok(())
}
pub fn save_original(path: &Path, bytes: &[u8]) -> Result<(), String> {
    Calibration::parse(bytes).map_err(|e| e.to_string())?;
    write_new(path, bytes)
}
pub fn save_text_report(
    path: &Path,
    original: &[u8],
    working: &[u8],
    source: &str,
    source_sha256: &str,
) -> Result<(), String> {
    let report =
        crate::calibration_report::render(original, working, Some(source), Some(source_sha256))?;
    write_new(path, report.as_bytes())
}
static NEXT: AtomicUsize = AtomicUsize::new(0);
pub fn export_bundle(
    parent: &Path,
    original: &[u8],
    working: &[u8],
    report: &Value,
) -> Result<PathBuf, String> {
    Calibration::parse(original).map_err(|e| e.to_string())?;
    Calibration::parse(working).map_err(|e| e.to_string())?;
    let readable = crate::calibration_report::render(
        original,
        working,
        report.get("source").and_then(Value::as_str),
        report.get("source_sha256").and_then(Value::as_str),
    )?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let directory = parent.join(format!(
        "UV98-calibration-{stamp}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let result = (|| {
        write_new(&directory.join("original.bin"), original)?;
        write_new(&directory.join("working.bin"), working)?;
        let fields = serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?;
        write_new(&directory.join("fields-and-changes.json"), &fields)?;
        write_new(
            &directory.join("calibration-report-ru.txt"),
            readable.as_bytes(),
        )?;
        write_new(&directory.join("README-RU.txt"), "Калибровки UV98 CPS 0.3\noriginal.bin — исходные 4096 байт без изменений.\nworking.bin — отдельная рабочая копия.\ncalibration-report-ru.txt — читаемый UTF-8 отчёт: SHA256, сводка правок, адреса, RAW и известные поля. Excel не нужен.\nfields-and-changes.json — машиночитаемые поля, их значения и побайтовая разница.\nmanifest.json появляется последним, после проверки всех файлов.\nSHA256 контролирует сохранность копии, но не доказывает правильность RF-калибровки.\nЗначения — коды, не измеренные ватты/герцы. Запись в рацию не выполнялась.\n".as_bytes())?;
        let manifest = json!({"schema":1,"app_version":"0.3.0","status":"complete_local_export",
            "original_sha256":crate::radio::sha256(original),"working_sha256":crate::radio::sha256(working),
            "fields_sha256":crate::radio::sha256(&fields),"readable_report_sha256":crate::radio::sha256(readable.as_bytes()),"original_bytes":BLOCK_LEN,"working_bytes":BLOCK_LEN,
            "radio_written":false,"rf_validated":false,"calibration_checksum_known":false});
        write_new(
            &directory.join("manifest.json"),
            &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
        )
    })();
    result.map_err(|e| format!("Пакет не завершён: {}. {e}", directory.display()))?;
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ORIGINAL: &[u8] =
        include_bytes!("../../../crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin");
    fn dir() -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/calibration-io-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn original_existing_file_is_never_overwritten_and_exports_reopen() {
        let folder = dir();
        let source = folder.join("source.bin");
        save_original(&source, ORIGINAL).unwrap();
        let mut changed = ORIGINAL.to_vec();
        changed[31] ^= 1;
        assert!(save_original(&source, &changed).is_err());
        assert_eq!(&load(&source).unwrap().bytes, ORIGINAL);
        let bundle = export_bundle(&folder, ORIGINAL, &changed, &json!({"test":true})).unwrap();
        assert_eq!(&load(&bundle.join("original.bin")).unwrap().bytes, ORIGINAL);
        assert_eq!(
            &load(&bundle.join("working.bin")).unwrap().bytes,
            changed.as_slice()
        );
        assert!(bundle.join("manifest.json").is_file());
        let readable = fs::read(bundle.join("calibration-report-ru.txt")).unwrap();
        let text = std::str::from_utf8(&readable).unwrap();
        assert!(text.contains(&crate::radio::sha256(ORIGINAL)));
        assert!(text.contains(&crate::radio::sha256(&changed)));
        let manifest: Value =
            serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest["readable_report_sha256"],
            crate::radio::sha256(&readable)
        );
        assert!(export_bundle(&folder, &[0; 3], ORIGINAL, &json!({})).is_err());
    }
    #[test]
    fn nor_extraction_uses_first_block_and_invalid_marker_is_rejected() {
        let folder = dir();
        let path = folder.join("nor.bin");
        let mut nor = vec![0xff; 4 * 1024 * 1024];
        nor[..BLOCK_LEN].copy_from_slice(ORIGINAL);
        fs::write(&path, &nor).unwrap();
        let loaded = load(&path).unwrap();
        assert!(loaded.extracted_from_nor);
        assert_eq!(loaded.bytes.as_slice(), ORIGINAL);
        assert_eq!(loaded.source_sha256, crate::radio::sha256(&nor));
        nor[0] = 0;
        fs::write(&path, &nor).unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn standalone_report_is_utf8_create_new_and_never_overwrites_source_or_existing_export() {
        let folder = dir();
        let source = folder.join("source.bin");
        save_original(&source, ORIGINAL).unwrap();
        let target = folder.join("calibration-report.txt");
        let mut working = ORIGINAL.to_vec();
        working[31] ^= 1;
        let source_hash = crate::radio::sha256(ORIGINAL);
        save_text_report(&target, ORIGINAL, &working, "source.bin", &source_hash).unwrap();
        let first = fs::read(&target).unwrap();
        let report = std::str::from_utf8(&first).unwrap();
        assert!(report.contains("КАЛИБРОВКИ UV98"));
        assert!(report.contains("0x001F"));
        assert!(save_text_report(&target, ORIGINAL, ORIGINAL, "source.bin", &source_hash).is_err());
        assert_eq!(fs::read(&target).unwrap(), first);
        assert!(save_text_report(&source, ORIGINAL, &working, "source.bin", &source_hash).is_err());
        assert_eq!(fs::read(&source).unwrap(), ORIGINAL);
        let invalid = folder.join("invalid.txt");
        assert!(save_text_report(&invalid, &[0; 3], &working, "source.bin", &source_hash).is_err());
        assert!(!invalid.exists());
    }
}
