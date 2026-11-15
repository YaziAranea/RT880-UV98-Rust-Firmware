//! Per-user desktop preferences; never stored in radio data or firmware catalogs.
use crate::i18n::{Language, LocalizedText};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .filter(|value| !value.is_empty())
        .map(|root| PathBuf::from(root).join("UV98-CPS/language"))
}

pub fn load() -> Language {
    path().map_or_else(Language::default, |path| load_from(&path))
}

fn load_from(path: &Path) -> Language {
    let Ok(file) = fs::File::open(path) else {
        return Language::default();
    };
    let mut text = String::new();
    if file.take(32).read_to_string(&mut text).is_err() {
        return Language::default();
    }
    Language::from_code(&text).unwrap_or_default()
}

pub fn save(language: Language) -> Result<(), LocalizedText> {
    let path = path().ok_or(msg!(
        "Не найдена папка настроек пользователя. Язык изменён только до закрытия CPS.",
        "User settings folder unavailable. Language changed for this session only.",
        "Benutzereinstellungsordner nicht verfügbar. Sprache nur für diese Sitzung geändert."
    ))?;
    save_to(&path, language).map_err(|error| {
        msgf!(
            "Язык изменён, но не сохранён: {error}",
            "Language changed but could not be saved: {error}",
            "Sprache geändert, aber nicht gespeichert: {error}"
        )
    })
}

fn save_to(path: &Path, language: Language) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Preference path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        "language-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = file
        .write_all(format!("{}\n", language.code()).as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_roundtrip_and_invalid_preference_do_not_touch_radio_files() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/cps-language-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("language");
        let sentinel = dir.join("original.880dmf");
        fs::write(&sentinel, b"preserved radio bytes").unwrap();
        assert_eq!(load_from(&path), Language::Russian);
        for language in Language::ALL {
            save_to(&path, language).unwrap();
            assert_eq!(load_from(&path), language);
        }
        fs::write(&path, b"unknown-language").unwrap();
        assert_eq!(load_from(&path), Language::Russian);
        fs::write(&path, [0xff, 0xfe]).unwrap();
        assert_eq!(load_from(&path), Language::Russian);
        assert!(save_to(&sentinel.join("language"), Language::German).is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"preserved radio bytes");
        fs::remove_file(path).unwrap();
        fs::remove_file(sentinel).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
