//! Application-only iRadio writer, ported from the locally proven pinned writer.
//! No read probe exists in this loader: the first command starts erase.
//! Writes complete application images without padding, relocation, END or retries.
use crate::i18n::{Language, LocalizedText};
use serde_json::json;
pub mod profile;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const IMAGE_BYTES: usize = 251_904;
pub const BLOCKS: usize = IMAGE_BYTES / 1024;
const APPLICATION_BASE: u32 = 0x0800_2800;
const APPLICATION_END: u32 = 0x0804_0000;
const TIMEOUT: Duration = Duration::from_secs(5);
const PROFILE: &str = "iradio-uv98-app-v1";
const STOCK_SHA256: &str = "eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc";

#[derive(Clone, Debug, PartialEq, Eq)]
enum Provenance {
    Stock,
    Catalog(PathBuf),
    Imported,
    Personal {
        sidecar_sha256: String,
        legacy_catalog: Option<PathBuf>,
    },
}

impl Provenance {
    fn journal_value(&self) -> &'static str {
        match self {
            Self::Stock => "pinned_stock_sha256",
            Self::Catalog(_) => "bundle_catalog",
            Self::Imported => "structural_import",
            Self::Personal { .. } => "personal_profile",
        }
    }
}

#[derive(Clone)]
pub struct PreparedImage {
    pub path: PathBuf,
    label: String,
    pub sha256: String,
    pub bytes_len: usize,
    bytes: Vec<u8>,
    tx_sha256: String,
    provenance: Provenance,
}
impl PreparedImage {
    fn new(path: &Path, bytes: Vec<u8>, provenance: Provenance) -> Self {
        Self {
            path: path.to_path_buf(),
            label: path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
            sha256: hash(&bytes),
            bytes_len: bytes.len(),
            tx_sha256: wire_hash(&bytes),
            bytes,
            provenance,
        }
    }

    pub fn display_label(&self) -> &str {
        &self.label
    }

    pub fn provenance_label(&self, lang: Language) -> &'static str {
        match self.provenance {
            Provenance::Stock => tr!(
                lang,
                "Заводской BIN · SHA256 подтверждён",
                "Stock BIN · SHA256 verified",
                "Werks-BIN · SHA256 geprüft"
            ),
            Provenance::Catalog(_) => tr!(
                lang,
                "Проверен по каталогу комплекта",
                "Matched the bundle catalog",
                "Stimmt mit dem Paketkatalog überein"
            ),
            Provenance::Imported => tr!(
                lang,
                "Импорт BIN · проверена структура",
                "Imported BIN · structure checked",
                "Importierte BIN · Struktur geprüft"
            ),
            Provenance::Personal { .. } => tr!(
                lang,
                "Личный профиль калибровок",
                "Personal calibration profile",
                "Persönliches Kalibrierprofil"
            ),
        }
    }

    pub fn source_warning(&self, lang: Language) -> Option<&'static str> {
        match self.provenance {
            Provenance::Imported => Some(tr!(
                lang,
                "Подлинность BIN и совместимость с платой не подтверждены. Используйте образ из доверенного источника для своей ревизии.",
                "BIN authenticity and board compatibility are unverified. Use an image from a source you trust for your board revision.",
                "Echtheit der BIN und Platinenkompatibilität sind unbestätigt. Verwenden Sie ein Image aus vertrauenswürdiger Quelle für Ihre Platinenrevision.",
            )),
            Provenance::Personal { .. } => Some(tr!(
                lang,
                "CPS сверила личный BIN с исходником. Подлинность исходника и совместимость с платой не подтверждены.",
                "CPS checked the personal BIN against its source. Source authenticity and board compatibility remain unverified.",
                "CPS hat die persönliche BIN mit ihrer Quelle verglichen. Echtheit der Quelle und Platinenkompatibilität sind unbestätigt.",
            )),
            Provenance::Stock | Provenance::Catalog(_) => None,
        }
    }

    pub fn is_personalization_source(&self) -> bool {
        matches!(
            self.provenance,
            Provenance::Catalog(_) | Provenance::Imported
        )
    }

    // Read the selected file again. Validation uses these exact bytes and the
    // writer keeps the immutable bytes confirmed at selection.
    fn revalidate(&self) -> Result<(), LocalizedText> {
        let bytes = bounded_read(&self.path, IMAGE_BYTES)?;
        if bytes != self.bytes || hash(&bytes) != self.sha256 || wire_hash(&bytes) != self.tx_sha256
        {
            return Err(msg!(
                "BIN изменился после выбора; выберите его заново",
                "The BIN changed after selection. Select it again.",
                "Die BIN wurde nach der Auswahl verändert. Wählen Sie sie erneut aus."
            ));
        }
        match &self.provenance {
            Provenance::Catalog(path) => {
                let data = read_catalog(path)?;
                bundle_entry(&data, &self.sha256, &self.tx_sha256)?;
            }
            Provenance::Personal {
                sidecar_sha256,
                legacy_catalog,
            } => {
                let sidecar = profile::read_sidecar(&self.path)?;
                if hash(&sidecar) != *sidecar_sha256 {
                    return Err(msg!(
                        "Каталог личного BIN изменился после выбора; выберите BIN заново",
                        "The personal BIN catalog changed after selection. Select the BIN again.",
                        "Der Katalog der persönlichen BIN wurde nach der Auswahl verändert. Wählen Sie die BIN erneut aus."
                    ));
                }
                profile::verify_saved(
                    &self.path,
                    &bytes,
                    &self.sha256,
                    &self.tx_sha256,
                    &parse_catalog(&sidecar)?,
                    legacy_catalog.as_deref(),
                )?;
            }
            Provenance::Stock | Provenance::Imported => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub phase: LocalizedText,
    pub completed_blocks: usize,
    pub total_blocks: usize,
    pub destructive_started: bool,
}
pub struct FlashReport {
    pub confirmed_blocks: usize,
    pub total_blocks: usize,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn bounded_read(path: &Path, max: usize) -> Result<Vec<u8>, LocalizedText> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > max as u64 {
        return Err(msg!(
            "Файл слишком большой",
            "File is too large",
            "Datei ist zu groß"
        ));
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err(msg!(
            "Размер файла изменился при чтении",
            "File size changed while reading",
            "Dateigröße hat sich beim Lesen geändert"
        ));
    }
    Ok(bytes)
}
fn validate_structure(data: &[u8]) -> Result<(), LocalizedText> {
    if data.len() != IMAGE_BYTES {
        return Err(msgf!(
            "Нужен полный application BIN: {IMAGE_BYTES} байт. Дополнение/обрезка запрещены.",
            "A complete application BIN of {IMAGE_BYTES} bytes is required. Padding and truncation are prohibited.",
            "Erforderlich ist eine vollständige Anwendungs-BIN mit {IMAGE_BYTES} Byte. Auffüllen und Kürzen sind unzulässig.",
        ));
    }
    let sp = u32::from_le_bytes(data[..4].try_into().unwrap());
    let reset = u32::from_le_bytes(data[4..8].try_into().unwrap());
    if sp <= 0x2000_0000
        || sp > 0x2000_c000
        || sp % 8 != 0
        || reset & 1 == 0
        || !(APPLICATION_BASE + 8..APPLICATION_END).contains(&(reset & !1))
    {
        return Err(
            msg!(
                "BIN не соответствует приложению UV98: векторы SP/Reset вне допустимого окна",
                "BIN does not match a UV98 application: SP/Reset vectors are outside the allowed range",
                "BIN passt nicht zur UV98-Anwendung: SP-/Reset-Vektoren liegen außerhalb des zulässigen Bereichs",
            ),
        );
    }
    Ok(())
}
pub fn export_stock(destination: &Path) -> Result<(), LocalizedText> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let source = exe
        .parent()
        .ok_or_else(|| {
            msg!(
                "Нет каталога CPS",
                "CPS directory not found",
                "CPS-Verzeichnis fehlt"
            )
        })?
        .join("recovery/RT-880_Upgrade_V1_16_250915.bin");
    if !source.is_file() {
        return Err(msg!(
            "Заводской BIN не входит в публичный комплект. Поместите свой RT-880_Upgrade_V1_16_250915.bin в папку recovery рядом с CPS; файл будет проверен по SHA256. Это файл восстановления, не чтение MCU рации.",
            "The public bundle omits the stock BIN. Place your RT-880_Upgrade_V1_16_250915.bin in the recovery folder beside CPS. CPS checks the SHA256. This file is for recovery and contains no MCU readout from your radio.",
            "Das öffentliche Paket enthält keine Werks-BIN. Legen Sie Ihre RT-880_Upgrade_V1_16_250915.bin in den Ordner recovery neben CPS. CPS prüft die SHA256-Prüfsumme. Die Datei dient zur Wiederherstellung und enthält keinen MCU-Auszug Ihres Funkgeräts.",
        ));
    }
    let bytes = bounded_read(&source, IMAGE_BYTES)?;
    if hash(&bytes) != STOCK_SHA256 {
        return Err(msg!(
            "SHA256 заводского образа не совпадает с комплектом",
            "Stock image SHA256 does not match the bundle",
            "SHA256 des Werks-Images stimmt nicht mit dem Paket überein"
        ));
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| format!("{}: {error}", destination.display()))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(destination);
        return Err(format!("{}: {error}", destination.display()).into());
    }
    Ok(())
}

pub fn prepare(path: &Path) -> Result<PreparedImage, LocalizedText> {
    let catalog = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|base| base.join("firmware/catalog.json")));
    prepare_selected(path, catalog.as_deref())
}

fn read_image(path: &Path) -> Result<PreparedImage, LocalizedText> {
    let bytes = bounded_read(path, IMAGE_BYTES)?;
    validate_structure(&bytes)?;
    Ok(PreparedImage::new(path, bytes, Provenance::Imported))
}

fn prepare_selected(path: &Path, catalog: Option<&Path>) -> Result<PreparedImage, LocalizedText> {
    let mut image = read_image(path)?;
    if image.sha256 == STOCK_SHA256 {
        image.provenance = Provenance::Stock;
        return Ok(image);
    }
    if let Some((sidecar, data)) = profile::saved_sidecar(&image)? {
        return profile::prepare_saved(image, &sidecar, &data, catalog);
    }
    if let Some(catalog) = catalog {
        if let Ok(data) = read_catalog(catalog) {
            if data["personalization"].is_null() && catalog_entry(&data, &image.sha256).is_some() {
                return bind_bundle_entry(image, &data, catalog);
            }
        }
    }
    // A marker and Cortex-M vectors only establish the supported layout.
    // They cannot authenticate the author or prove hardware compatibility.
    profile::digest_offset(&image.bytes)?;
    Ok(image)
}

/// Only a catalog match can select a bundled image automatically.
pub fn default_image(base: &Path) -> Option<PreparedImage> {
    let catalog = base.join("firmware/catalog.json");
    let encoded = bounded_read(&catalog, 65536).ok()?;
    let data = parse_catalog(&encoded).ok()?;
    let relative = Path::new(data["default_image"].as_str()?);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    let image = read_image(&base.join(relative)).ok()?;
    if !data["personalization"].is_null() {
        return profile::prepare_saved(image, &encoded, &data, None).ok();
    }
    bind_bundle_entry(image, &data, &catalog).ok()
}

fn parse_catalog(encoded: &[u8]) -> Result<serde_json::Value, LocalizedText> {
    let data: serde_json::Value = serde_json::from_slice(encoded).map_err(|e| {
        msgf!(
            "Каталог образов: {e}",
            "Image catalog: {e}",
            "Image-Katalog: {e}"
        )
    })?;
    if data["schema"] != 1 || data["profile"] != PROFILE || !data["images"].is_array() {
        return Err(msg!(
            "Неподдерживаемый каталог образов CPS",
            "Unsupported CPS image catalog",
            "CPS-Image-Katalog wird nicht unterstützt"
        ));
    }
    Ok(data)
}

fn read_catalog(path: &Path) -> Result<serde_json::Value, LocalizedText> {
    parse_catalog(&bounded_read(path, 65536)?)
}

fn catalog_entry<'a>(
    catalog: &'a serde_json::Value,
    sha256: &str,
) -> Option<&'a serde_json::Value> {
    catalog["images"]
        .as_array()?
        .iter()
        .find(|entry| entry["sha256"].as_str() == Some(sha256))
}

fn verify_entry<'a>(
    catalog: &'a serde_json::Value,
    sha256: &str,
    tx_sha256: &str,
) -> Result<&'a serde_json::Value, LocalizedText> {
    let entry = catalog_entry(catalog, sha256).ok_or_else(|| {
        msg!(
            "SHA256 BIN отсутствует в каталоге",
            "The BIN SHA256 is missing from the catalog",
            "Die SHA256-Prüfsumme der BIN fehlt im Katalog"
        )
    })?;
    if entry["bytes"].as_u64() != Some(IMAGE_BYTES as u64) {
        return Err(msg!(
            "Размер в каталоге не совпадает",
            "Size does not match the catalog",
            "Größe stimmt nicht mit dem Katalog überein"
        ));
    }
    if entry["tx_sha256"].as_str() != Some(tx_sha256) {
        return Err(msg!(
            "Контрольная сумма протокольного потока не совпадает с каталогом",
            "The protocol stream checksum does not match the catalog",
            "Die Prüfsumme des Protokolldatenstroms stimmt nicht mit dem Katalog überein"
        ));
    }
    Ok(entry)
}

fn bundle_entry<'a>(
    catalog: &'a serde_json::Value,
    sha256: &str,
    tx_sha256: &str,
) -> Result<&'a serde_json::Value, LocalizedText> {
    if !catalog["personalization"].is_null() {
        return Err(msg!(
            "Личный каталог не является каталогом выпуска",
            "A personal catalog cannot serve as a release catalog",
            "Ein persönlicher Katalog ist kein Versionskatalog"
        ));
    }
    verify_entry(catalog, sha256, tx_sha256)
}

fn bind_bundle_entry(
    mut image: PreparedImage,
    data: &serde_json::Value,
    catalog: &Path,
) -> Result<PreparedImage, LocalizedText> {
    let entry = bundle_entry(data, &image.sha256, &image.tx_sha256)?;
    if let Some(label) = entry["label"].as_str() {
        image.label = label.to_string();
    }
    image.provenance = if image.sha256 == STOCK_SHA256 {
        Provenance::Stock
    } else {
        Provenance::Catalog(catalog.to_path_buf())
    };
    Ok(image)
}

#[cfg(test)]
fn prepare_with_catalog(path: &Path, catalog: &Path) -> Result<PreparedImage, LocalizedText> {
    bind_bundle_entry(read_image(path)?, &read_catalog(catalog)?, catalog)
}

fn frame(kind: u8, offset: usize, payload: &[u8]) -> Vec<u8> {
    let mut data = vec![kind, ((offset >> 8) & 255) as u8, (offset & 255) as u8];
    data.extend_from_slice(payload);
    data.push(data.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
    data
}
fn packets(data: &[u8]) -> impl Iterator<Item = Vec<u8>> + '_ {
    [
        frame(0x39, 0x3305, &[0x10]),
        frame(0x39, 0x3305, &[0x10]),
        frame(0x39, 0x3305, &[0x55]),
    ]
    .into_iter()
    .chain(
        data.chunks_exact(1024)
            .enumerate()
            .map(|(i, block)| frame(0x57, i * 1024, block)),
    )
}
fn wire_hash(data: &[u8]) -> String {
    let mut digest = Sha256::new();
    for frame in packets(data) {
        digest.update(frame);
    }
    format!("{:x}", digest.finalize())
}
pub(super) trait Transport {
    fn read_byte(&mut self, timeout: Duration) -> io::Result<Option<u8>>;
    fn write_frame(&mut self, data: &[u8]) -> io::Result<usize>;
    fn close(&mut self) -> io::Result<()>;
}
fn new_file(path: &Path) -> Result<File, LocalizedText> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()).into())
}
fn durable(file: &mut File, bytes: &[u8]) -> Result<(), LocalizedText> {
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| msgf!("Журнал: {e}", "Log: {e}", "Protokoll: {e}"))
}
fn write_json(path: &Path, data: &serde_json::Value) -> Result<(), LocalizedText> {
    durable(
        &mut new_file(path)?,
        &serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?,
    )
}

struct TransferError {
    notice: LocalizedText,
    diagnostic: Option<String>,
}

impl From<LocalizedText> for TransferError {
    fn from(notice: LocalizedText) -> Self {
        Self {
            notice,
            diagnostic: None,
        }
    }
}

impl TransferError {
    fn diagnostic(&self) -> &str {
        self.diagnostic
            .as_deref()
            .unwrap_or_else(|| self.notice.as_str(Language::Russian))
    }

    fn transport(context: Option<&LocalizedText>, error: io::Error) -> Self {
        let raw = error.to_string();
        let render = |lang, detail: &str| {
            context.map_or_else(
                || detail.to_owned(),
                |context| format!("{}: {detail}", context.as_str(lang)),
            )
        };
        // Persist the original transport detail even if the UI has a translation.
        let diagnostic = Some(render(Language::Russian, &raw));
        let notice = LocalizedText::from_fn(|lang| {
            #[cfg(windows)]
            let detail = crate::firmware_windows::error_detail(&error, lang)
                .map_or(raw.as_str(), |detail| detail);
            #[cfg(not(windows))]
            let detail = &raw;
            render(lang, detail)
        });
        Self { notice, diagnostic }
    }
}

fn run<T: Transport>(
    image: PreparedImage,
    port: &str,
    directory: &Path,
    factory: impl FnOnce(&str) -> Result<T, TransferError>,
    mut progress: impl FnMut(Progress),
) -> Result<FlashReport, LocalizedText> {
    image.revalidate()?;
    if let Some(parent) = directory.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::create_dir(directory).map_err(|e| {
        msgf!(
            "Новая папка журнала: {e}",
            "New log directory: {e}",
            "Neues Protokollverzeichnis: {e}"
        )
    })?;
    let started = json!({"schema":1,"status":"incomplete","started_unix_ms":ms(),"port":port,"image_sha256":image.sha256,"image_provenance":image.provenance.journal_value(),"image_bytes":IMAGE_BYTES,"tx_sha256":image.tx_sha256,"profile":PROFILE,"application_base":"0x08002800","application_end":"0x08040000","automatic_retries":0,"serial":{"baud":115200,"data_bits":8,"parity":"N","stop_bits":1,"dtr":false,"rts":false,"flow_control":false},"hardware_boot_verified":false,"mcu_readback_verified":false,"calibration_after_boot_verified":false});
    write_json(&directory.join("started.json"), &started)?;
    let mut tx = new_file(&directory.join("tx-attempted.bin"))?;
    let mut rx = new_file(&directory.join("rx.bin"))?;
    let mut events = new_file(&directory.join("journal.jsonl"))?;
    let mut opened: Option<T> = None;
    let mut acknowledged = 0usize;
    let mut ack_count = 0usize;
    let mut destructive = false;
    let mut final_ack = false;
    let mut stage = msg!("Открытие порта", "Opening port", "Port wird geöffnet");
    progress(Progress {
        phase: stage.clone(),
        completed_blocks: 0,
        total_blocks: BLOCKS,
        destructive_started: false,
    });
    let result = (|| -> Result<(), TransferError> {
        opened = Some(factory(port)?);
        let serial = opened.as_mut().unwrap();
        if let Some(b) = serial
            .read_byte(Duration::from_millis(200))
            .map_err(|e| TransferError::transport(None, e))?
        {
            durable(&mut rx, &[b])?;
            return Err(msg!(
                "Перед началом из порта пришли данные. Команды стирания не отправлены. Нужен загрузчик, не обычный экран.",
                "The port sent data before the transfer began. No erase commands were sent. Put the radio in bootloader mode.",
                "Der Port hat vor dem Start Daten gesendet. Es wurden keine Löschbefehle gesendet. Versetzen Sie das Funkgerät in den Bootloader-Modus.",
            ).into());
        }
        for (index, packet) in packets(&image.bytes).enumerate() {
            stage = if index < 3 {
                msgf!(
                    "Подготовка приложения {}/3",
                    "Preparing application {}/3",
                    "Anwendung wird vorbereitet {}/3",
                    index + 1
                )
            } else {
                msgf!(
                    "Запись блока {}/{}",
                    "Writing block {}/{}",
                    "Block wird geschrieben {}/{}",
                    index - 2,
                    BLOCKS
                )
            };
            durable(&mut events, format!("{}\n",json!({"event":"tx_attempt","stage":stage.as_str(Language::Russian),"bytes":packet.len(),"sha256":hash(&packet),"unix_ms":ms()})).as_bytes())?;
            durable(&mut tx, &packet)?;
            destructive = true;
            progress(Progress {
                phase: stage.clone(),
                completed_blocks: acknowledged,
                total_blocks: BLOCKS,
                destructive_started: true,
            });
            let write_result = serial.write_frame(&packet);
            durable(&mut events, format!("{}\n",json!({"event":"tx_return","stage":stage.as_str(Language::Russian),"reported_written":write_result.as_ref().ok().copied(),"error":write_result.as_ref().err().map(ToString::to_string),"unix_ms":ms()})).as_bytes())?;
            let written = write_result.map_err(|e| TransferError::transport(Some(&stage), e))?;
            if written != packet.len() {
                return Err(LocalizedText::from_fn(|lang| {
                    trf!(
                        lang,
                        "{phase}: неполная отправка {written}/{}. Автоповтор отключён.",
                        "{phase}: short send, {written}/{}. Automatic retry is disabled.",
                        "{phase}: unvollständiger Versand, {written}/{}. Automatische Wiederholung ist deaktiviert.",
                        packet.len(),
                        phase = stage.as_str(lang)
                    )
                }).into());
            }
            let byte = serial
                .read_byte(TIMEOUT)
                .map_err(|e| TransferError::transport(Some(&stage), e))?;
            if let Some(b) = byte {
                durable(&mut rx, &[b])?;
            }
            if byte != Some(6) {
                return Err(LocalizedText::from_fn(|lang| {
                    let reply = byte.map_or_else(
                        || tr!(lang, "таймаут", "timeout", "Zeitüberschreitung").into(),
                        |b| format!("{b:02X}"),
                    );
                    trf!(
                        lang,
                        "{phase}: ожидался ACK 06, получено {reply}. Запись остановлена.",
                        "{phase}: expected ACK 06, got {reply}. Writing stopped.",
                        "{phase}: ACK 06 erwartet, {reply} erhalten. Schreiben abgebrochen.",
                        phase = stage.as_str(lang)
                    )
                })
                .into());
            }
            ack_count += 1;
            if index >= 3 {
                acknowledged += 1;
            }
            durable(&mut events, format!("{}\n",json!({"event":"ack","stage":stage.as_str(Language::Russian),"acked_blocks":acknowledged,"ack_count":ack_count})).as_bytes())?;
        }
        stage = msg!(
            "Ожидание итогового подтверждения",
            "Waiting for final acknowledgement",
            "Warte auf abschließende Bestätigung"
        );
        progress(Progress {
            phase: stage.clone(),
            completed_blocks: acknowledged,
            total_blocks: BLOCKS,
            destructive_started: true,
        });
        let byte = serial
            .read_byte(TIMEOUT)
            .map_err(|e| TransferError::transport(None, e))?;
        if let Some(b) = byte {
            durable(&mut rx, &[b])?;
        }
        if byte != Some(6) {
            return Err(msg!(
                "Нет итогового ACK 06. Передача файла не подтверждена; не считать прошивку успешной.",
                "No final ACK 06. The file transfer is unconfirmed. Treat the flash as unsuccessful.",
                "Abschließendes ACK 06 fehlt. Die Dateiübertragung ist nicht bestätigt. Betrachten Sie den Schreibvorgang als fehlgeschlagen.",
            ).into());
        }
        ack_count += 1;
        final_ack = true;
        Ok(())
    })();
    let close = opened.as_mut().map(|serial| serial.close()).transpose();
    let closed = opened.is_some() && close.is_ok();
    // Keep cleanup evidence even when the primary transfer already failed.
    // `and_then` intentionally returns the original transfer error to the UI,
    // but must not erase a separate close failure from the durable report.
    let close_error = close.as_ref().err().map(ToString::to_string);
    let outcome = result.and_then(|()| {
        close.map(|_| ()).map_err(|error| {
            let context = msg!("Закрытие COM", "Closing COM port", "COM-Port schließen");
            TransferError::transport(Some(&context), error)
        })
    });
    let error = outcome.as_ref().err().map(TransferError::diagnostic);
    write_json(
        &directory.join("manifest.json"),
        &json!({"schema":1,"status":if outcome.is_ok(){"transfer_acknowledged"}else{"failed"},"finished_unix_ms":ms(),"port":port,"image_sha256":image.sha256,"image_provenance":image.provenance.journal_value(),"last_stage":stage.as_str(Language::Russian),"erase_sequence_attempted":destructive,"acked_blocks":acknowledged,"ack_count":ack_count,"final_ack_received":final_ack,"port_closed":closed,"error":error,"close_error":close_error,"automatic_retries":0,"hardware_boot_verified":false,"mcu_readback_verified":false,"calibration_after_boot_verified":false}),
    )?;
    outcome.map_err(|error| error.notice)?;
    Ok(FlashReport {
        confirmed_blocks: acknowledged,
        total_blocks: BLOCKS,
    })
}
pub fn flash_windows(
    image: PreparedImage,
    port: &str,
    log_dir: &Path,
    on_progress: impl FnMut(Progress),
) -> Result<FlashReport, LocalizedText> {
    #[cfg(windows)]
    {
        run(
            image,
            port,
            log_dir,
            |name| {
                crate::firmware_windows::WindowsPort::open(name).map_err(|error| {
                    let context = msgf!(
                        "Не удалось открыть {name}",
                        "Could not open {name}",
                        "{name} konnte nicht geöffnet werden"
                    );
                    TransferError::transport(Some(&context), error)
                })
            },
            on_progress,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (image, port, log_dir, on_progress);
        Err(msg!(
            "Запись поддерживается только в Windows",
            "Firmware writing is available only on Windows",
            "Firmware kann nur unter Windows geschrieben werden"
        ))
    }
}

#[cfg(test)]
mod tests;
