//! Local calibration files only. No serial, RF commands, or source overwrites.
use crate::calibration_report::block_error;
use crate::i18n::{Language, LocalizedText};
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
pub fn load(path: &Path) -> Result<Loaded, LocalizedText> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if len != BLOCK_LEN as u64 && len != 4 * 1024 * 1024 {
        return Err(msg!(
            "Нужен блок калибровок 4096 байт или сохранённый NOR-дамп 4 МиБ. Файл CPS калибровок не содержит.",
            "Select a 4096-byte calibration block or a saved 4 MiB NOR dump. CPS files contain no calibration data.",
            "Wählen Sie einen 4096-Byte-Kalibrierblock oder einen gespeicherten 4-MiB-NOR-Dump. CPS-Dateien enthalten keine Kalibrierdaten.",
        ));
    }
    let mut bytes = Vec::with_capacity(len as usize);
    file.take(len + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != len {
        return Err(msg!(
            "Размер файла изменился во время чтения",
            "File size changed during reading",
            "Die Dateigröße hat sich beim Lesen geändert"
        ));
    }
    let block = Calibration::parse(&bytes[..BLOCK_LEN]).map_err(block_error)?;
    Ok(Loaded {
        bytes: *block.raw(),
        source_sha256: crate::radio::sha256(&bytes),
        extracted_from_nor: len != BLOCK_LEN as u64,
    })
}
pub fn source_label(path: &Path, extracted_from_nor: bool) -> LocalizedText {
    if extracted_from_nor {
        let path = path.display();
        msgf!(
            "{path} (извлечён первый блок NOR)",
            "{path} (first NOR block extracted)",
            "{path} (erster NOR-Block extrahiert)",
        )
    } else {
        path.display().to_string().into()
    }
}
fn write_new(path: &Path, data: &[u8]) -> Result<(), LocalizedText> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            msgf!(
                "Не удалось создать новый файл {}: {e}",
                "Could not create new file {}: {e}",
                "Neue Datei {} konnte nicht erstellt werden: {e}",
                path.display(),
            )
        })?;
    file.write_all(data)
        .and_then(|()| file.sync_all())
        .map_err(|e| msgf!(
            "Запись не завершена. Файл может быть неполным: {e}",
            "Writing did not finish. The file may be incomplete: {e}",
            "Der Schreibvorgang wurde nicht abgeschlossen. Die Datei ist möglicherweise unvollständig: {e}",
        ))?;
    if fs::read(path).map_err(|e| e.to_string())? != data {
        return Err(msg!(
            "Записанные данные не прошли проверку",
            "Written data did not match during verification",
            "Die geschriebenen Daten stimmen bei der Überprüfung nicht überein",
        ));
    }
    Ok(())
}
pub fn save_original(path: &Path, bytes: &[u8]) -> Result<(), LocalizedText> {
    Calibration::parse(bytes).map_err(block_error)?;
    write_new(path, bytes)
}
pub fn save_text_report(
    path: &Path,
    original: &[u8],
    working: &[u8],
    source: &str,
    source_sha256: &str,
    language: Language,
) -> Result<(), LocalizedText> {
    let report = crate::calibration_report::render(
        original,
        working,
        Some(source),
        Some(source_sha256),
        language,
    )?;
    write_new(path, report.as_bytes())
}
static NEXT: AtomicUsize = AtomicUsize::new(0);
pub fn export_bundle(
    parent: &Path,
    original: &[u8],
    working: &[u8],
    report: &Value,
    source: Option<&str>,
    language: Language,
) -> Result<PathBuf, LocalizedText> {
    Calibration::parse(original).map_err(block_error)?;
    Calibration::parse(working).map_err(block_error)?;
    let readable = crate::calibration_report::render(
        original,
        working,
        source,
        report.get("source_sha256").and_then(Value::as_str),
        language,
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
    let report_name = format!("calibration-report-{}.txt", language.code());
    let readme_name = format!("README-{}.txt", language.code().to_ascii_uppercase());
    let result = (|| {
        write_new(&directory.join("original.bin"), original)?;
        write_new(&directory.join("working.bin"), working)?;
        let fields = serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?;
        write_new(&directory.join("fields-and-changes.json"), &fields)?;
        write_new(&directory.join(&report_name), readable.as_bytes())?;
        let readme = trf!(language,
            "Калибровки UV98 CPS\n\
             original.bin — исходные 4096 байт без изменений.\n\
             working.bin — отдельная рабочая копия.\n\
             {report_name} — UTF-8 отчёт с SHA256, сводкой правок, адресами, RAW и известными полями.\n\
             fields-and-changes.json — машиночитаемые поля, их значения и побайтовая разница.\n\
             manifest.json появляется последним, после проверки всех файлов.\n\
             SHA256 позволяет проверить целостность копии. Точность RF-калибровки не подтверждена.\n\
             Значения хранятся как коды. Пересчёт в ватты или герцы не подтверждён. Запись в рацию не выполнялась.\n",
            "UV98 CPS calibration\n\
             original.bin contains the original 4096 bytes.\n\
             working.bin contains the separate working copy.\n\
             {report_name} is a UTF-8 report of SHA256, changes, addresses, RAW values and known fields.\n\
             fields-and-changes.json contains machine-readable fields, values and byte differences.\n\
             manifest.json is created after all other files are verified.\n\
             SHA256 verifies copy integrity. RF calibration accuracy remains unverified.\n\
             Values are stored as codes with no confirmed conversion to watts or hertz. The export performs no radio writes.\n",
            "UV98 CPS Kalibrierung\n\
             original.bin enthält die ursprünglichen 4096 Bytes.\n\
             working.bin enthält die separate Arbeitskopie.\n\
             {report_name} ist der UTF-8-Bericht mit SHA256, Änderungen, Adressen, RAW-Werten und bekannten Feldern.\n\
             fields-and-changes.json enthält maschinenlesbare Felder, Werte und Byteunterschiede.\n\
             manifest.json wird nach der Prüfung aller anderen Dateien erstellt.\n\
             SHA256 dient zur Prüfung der Kopie. Die Genauigkeit der HF-Kalibrierung bleibt unbestätigt.\n\
             Werte sind als Codes gespeichert. Die Umrechnung in Watt oder Hertz ist unbestätigt. Der Export schreibt keine Daten auf das Funkgerät.\n",
        );
        write_new(&directory.join(readme_name), readme.as_bytes())?;
        let manifest = json!({"schema":1,"app_version":env!("CARGO_PKG_VERSION"),"status":"complete_local_export",
            "original_sha256":crate::radio::sha256(original),"working_sha256":crate::radio::sha256(working),
            "fields_sha256":crate::radio::sha256(&fields),"readable_report_file":report_name,"readable_report_sha256":crate::radio::sha256(readable.as_bytes()),"original_bytes":BLOCK_LEN,"working_bytes":BLOCK_LEN,
            "radio_written":false,"rf_validated":false,"calibration_checksum_known":false});
        write_new(
            &directory.join("manifest.json"),
            &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
        )
    })();
    result.map_err(|error| {
        LocalizedText::from_fn(|language| {
            trf!(
                language,
                "Пакет не завершён: {}. {detail}",
                "Bundle is incomplete: {}. {detail}",
                "Paket ist unvollständig: {}. {detail}",
                directory.display(),
                detail = error.as_str(language),
            )
        })
    })?;
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
        let bundle = export_bundle(
            &folder,
            ORIGINAL,
            &changed,
            &json!({"test":true}),
            None,
            Language::Russian,
        )
        .unwrap();
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
        assert!(export_bundle(
            &folder,
            &[0; 3],
            ORIGINAL,
            &json!({}),
            None,
            Language::Russian
        )
        .is_err());
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
        let language = Language::Russian;
        save_text_report(
            &target,
            ORIGINAL,
            &working,
            "source.bin",
            &source_hash,
            language,
        )
        .unwrap();
        let first = fs::read(&target).unwrap();
        let report = std::str::from_utf8(&first).unwrap();
        assert!(report.contains("КАЛИБРОВКИ UV98"));
        assert!(report.contains("0x001F"));
        assert!(save_text_report(
            &target,
            ORIGINAL,
            ORIGINAL,
            "source.bin",
            &source_hash,
            language
        )
        .is_err());
        assert_eq!(fs::read(&target).unwrap(), first);
        assert!(save_text_report(
            &source,
            ORIGINAL,
            &working,
            "source.bin",
            &source_hash,
            language
        )
        .is_err());
        assert_eq!(fs::read(&source).unwrap(), ORIGINAL);
        let invalid = folder.join("invalid.txt");
        assert!(save_text_report(
            &invalid,
            &[0; 3],
            &working,
            "source.bin",
            &source_hash,
            language
        )
        .is_err());
        assert!(!invalid.exists());
    }
}
