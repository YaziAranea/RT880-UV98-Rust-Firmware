//! Local CPS file I/O only. No serial, networking or device access.
use crate::i18n::LocalizedText;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use uv98_codeplug::FILE_LEN;
use uv98_cps_core::Document;

pub fn read_document(path: &Path) -> Result<Document, LocalizedText> {
    let file = File::open(path).map_err(|error| LocalizedText::from(error.to_string()))?;
    if file
        .metadata()
        .map_err(|error| LocalizedText::from(error.to_string()))?
        .len()
        != FILE_LEN as u64
    {
        return Err(msgf!(
            "Ожидается CPS-файл размером {FILE_LEN} байта",
            "Expected a CPS file of {FILE_LEN} bytes",
            "Eine CPS-Datei mit {FILE_LEN} Bytes wird erwartet"
        ));
    }
    let mut bytes = Vec::with_capacity(FILE_LEN);
    file.take(FILE_LEN as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| LocalizedText::from(error.to_string()))?;
    Document::from_bytes(&bytes).map_err(|error| LocalizedText::from(error.to_string()))
}

pub fn save_new_copy(path: &Path, bytes: &[u8]) -> Result<(), LocalizedText> {
    if bytes.len() != FILE_LEN {
        return Err(msg!(
            "Неверная длина экспортируемого CPS-файла",
            "Invalid exported CPS file length",
            "Ungültige Länge der exportierten CPS-Datei"
        )
        .into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            msgf!(
                "Не удалось создать новую копию: {error}. Выберите новое имя файла.",
                "Could not create a new copy: {error}. Choose a new file name.",
                "Neue Kopie konnte nicht erstellt werden: {error}. Einen neuen Dateinamen wählen."
            )
        })?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| msgf!("Запись не завершена: {error}. Новый файл может быть неполным.", "Writing did not finish: {error}. The new file may be incomplete.", "Schreiben nicht abgeschlossen: {error}. Die neue Datei ist möglicherweise unvollständig."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/cps-file-tests")
                .join(format!(
                    "{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            for name in ["original.880dmf", "export.880dmf", "bad.880dmf"] {
                let _ = fs::remove_file(self.0.join(name));
            }
            let _ = fs::remove_dir(&self.0);
        }
    }
    #[test]
    fn existing_source_is_never_truncated_or_overwritten() {
        let dir = Directory::new();
        let path = dir.0.join("original.880dmf");
        let original = vec![0xA5; FILE_LEN];
        fs::write(&path, &original).unwrap();
        assert!(save_new_copy(&path, &vec![0; FILE_LEN]).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    #[test]
    fn complete_export_reopens_byte_identically() {
        let dir = Directory::new();
        let path = dir.0.join("export.880dmf");
        let bytes: Vec<u8> = (0..FILE_LEN)
            .map(|index| (index.wrapping_mul(73) & 255) as u8)
            .collect();
        save_new_copy(&path, &bytes).unwrap();
        assert_eq!(read_document(&path).unwrap().export_bytes(), bytes);
    }
    #[test]
    fn wrong_sizes_are_rejected_before_export_creation() {
        let dir = Directory::new();
        let path = dir.0.join("bad.880dmf");
        assert!(save_new_copy(&path, &[1, 2, 3]).is_err());
        assert!(!path.exists());
        fs::write(&path, [1, 2, 3]).unwrap();
        assert!(read_document(&path).is_err());
    }
}
