//! Application-only iRadio writer, ported from the locally proven pinned writer.
//! No read probe exists in this loader: the first command starts erase.
//! Images must match the release catalog; no padding, relocation, END or retries.
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const IMAGE_BYTES: usize = 251_904;
pub const BLOCKS: usize = IMAGE_BYTES / 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const PROFILE: &str = "iradio-uv98-app-v1";

#[derive(Clone)]
pub struct PreparedImage {
    pub path: PathBuf,
    pub label: String,
    pub sha256: String,
    pub bytes_len: usize,
    bytes: Vec<u8>,
    tx_sha256: String,
    catalog: PathBuf,
}
#[derive(Clone, Debug)]
pub struct Progress {
    pub phase: String,
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
fn bounded_read(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > max as u64 {
        return Err("Файл слишком большой".into());
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("Размер файла изменился при чтении".into());
    }
    Ok(bytes)
}
fn validate_structure(data: &[u8]) -> Result<(), String> {
    if data.len() != IMAGE_BYTES {
        return Err(format!(
            "Нужен полный application BIN: {IMAGE_BYTES} байт. Дополнение/обрезка запрещены."
        ));
    }
    let sp = u32::from_le_bytes(data[..4].try_into().unwrap());
    let reset = u32::from_le_bytes(data[4..8].try_into().unwrap());
    if sp <= 0x2000_0000
        || sp > 0x2000_c000
        || sp % 8 != 0
        || reset & 1 == 0
        || !(0x0800_2808..0x0804_0000).contains(&(reset & !1))
    {
        return Err(
            "BIN не соответствует приложению UV98: векторы SP/Reset вне допустимого окна".into(),
        );
    }
    Ok(())
}
pub fn export_stock(destination: &Path) -> Result<(), String> {
    const STOCK_SHA256: &str = "eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc";
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let source = exe
        .parent()
        .ok_or("Нет каталога CPS")?
        .join("recovery/RT-880_Upgrade_V1_16_250915.bin");
    let bytes = bounded_read(&source, IMAGE_BYTES)?;
    if hash(&bytes) != STOCK_SHA256 {
        return Err("SHA256 заводского образа не совпадает с комплектом".into());
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| format!("{}: {error}", destination.display()))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(destination);
        return Err(format!("{}: {error}", destination.display()));
    }
    Ok(())
}

pub fn prepare(path: &Path) -> Result<PreparedImage, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    prepare_with_catalog(
        path,
        &exe.parent()
            .ok_or("Нет каталога CPS")?
            .join("firmware/catalog.json"),
    )
}

/// The release manifest chooses the image, so an older hardcoded filename can
/// never silently become the default of a newer portable CPS bundle.
pub fn default_image(base: &Path) -> Option<PathBuf> {
    let catalog = base.join("firmware/catalog.json");
    let data: serde_json::Value =
        serde_json::from_slice(&bounded_read(&catalog, 65536).ok()?).ok()?;
    if data["schema"] != 1 || data["profile"] != PROFILE {
        return None;
    }
    let relative = Path::new(data["default_image"].as_str()?);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    let path = base.join(relative);
    prepare_with_catalog(&path, &catalog).ok()?;
    Some(path)
}
pub fn prepare_with_catalog(path: &Path, catalog: &Path) -> Result<PreparedImage, String> {
    let bytes = bounded_read(path, IMAGE_BYTES)?;
    validate_structure(&bytes)?;
    let sha256 = hash(&bytes);
    let catalog_data: serde_json::Value = serde_json::from_slice(&bounded_read(catalog, 65536)?)
        .map_err(|e| format!("Каталог образов: {e}"))?;
    if catalog_data["schema"] != 1 || catalog_data["profile"] != PROFILE {
        return Err("Неподдерживаемый каталог образов CPS".into());
    }
    let entries = catalog_data["images"]
        .as_array()
        .ok_or("В каталоге нет images")?;
    let found = entries.iter().find(|entry| entry["sha256"].as_str() == Some(&sha256)).ok_or("Этот BIN отсутствует в проверенном каталоге комплекта. Выберите образ из папки firmware или recovery.")?;
    if found["bytes"].as_u64() != Some(IMAGE_BYTES as u64) {
        return Err("Размер в каталоге не совпадает".into());
    }
    let tx_sha256 = wire_hash(&bytes);
    if found["tx_sha256"].as_str() != Some(&tx_sha256) {
        return Err("Контрольная сумма протокольного потока не совпадает с каталогом".into());
    }
    Ok(PreparedImage {
        path: path.to_path_buf(),
        label: found["label"].as_str().unwrap_or("UV98").to_string(),
        sha256,
        bytes_len: bytes.len(),
        bytes,
        tx_sha256,
        catalog: catalog.to_path_buf(),
    })
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
fn new_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn durable(file: &mut File, bytes: &[u8]) -> Result<(), String> {
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("Журнал: {e}"))
}
fn write_json(path: &Path, data: &serde_json::Value) -> Result<(), String> {
    durable(
        &mut new_file(path)?,
        &serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?,
    )
}

fn run<T: Transport>(
    image: PreparedImage,
    port: &str,
    directory: &Path,
    factory: impl FnOnce(&str) -> Result<T, String>,
    mut progress: impl FnMut(Progress),
) -> Result<FlashReport, String> {
    // Revalidate the selected file and catalog immediately before creating the
    // journal/opening a port. Flash this immutable byte vector, not a file handle.
    let checked = prepare_with_catalog(&image.path, &image.catalog)?;
    if checked.sha256 != image.sha256
        || checked.bytes != image.bytes
        || checked.tx_sha256 != image.tx_sha256
    {
        return Err("BIN изменился после выбора; выберите его заново".into());
    }
    if let Some(parent) = directory.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::create_dir(directory).map_err(|e| format!("Новая папка журнала: {e}"))?;
    let started = json!({"schema":1,"status":"incomplete","started_unix_ms":ms(),"port":port,"image_sha256":image.sha256,"image_bytes":IMAGE_BYTES,"tx_sha256":image.tx_sha256,"profile":PROFILE,"application_base":"0x08002800","application_end":"0x08040000","automatic_retries":0,"serial":{"baud":115200,"data_bits":8,"parity":"N","stop_bits":1,"dtr":false,"rts":false,"flow_control":false},"hardware_boot_verified":false,"mcu_readback_verified":false,"calibration_after_boot_verified":false});
    write_json(&directory.join("started.json"), &started)?;
    let mut tx = new_file(&directory.join("tx-attempted.bin"))?;
    let mut rx = new_file(&directory.join("rx.bin"))?;
    let mut events = new_file(&directory.join("journal.jsonl"))?;
    let mut opened: Option<T> = None;
    let mut acknowledged = 0usize;
    let mut ack_count = 0usize;
    let mut destructive = false;
    let mut final_ack = false;
    let mut stage = "Открытие порта".to_string();
    progress(Progress {
        phase: stage.clone(),
        completed_blocks: 0,
        total_blocks: BLOCKS,
        destructive_started: false,
    });
    let result = (|| -> Result<(), String> {
        opened = Some(factory(port)?);
        let serial = opened.as_mut().unwrap();
        if let Some(b) = serial
            .read_byte(Duration::from_millis(200))
            .map_err(|e| e.to_string())?
        {
            durable(&mut rx, &[b])?;
            return Err("Перед началом из порта пришли данные. Команды стирания не отправлены. Нужен загрузчик, не обычный экран.".into());
        }
        for (index, packet) in packets(&image.bytes).enumerate() {
            stage = if index < 3 {
                format!("Подготовка приложения {}/3", index + 1)
            } else {
                format!("Запись блока {}/{}", index - 2, BLOCKS)
            };
            durable(&mut events, format!("{}\n",json!({"event":"tx_attempt","stage":stage,"bytes":packet.len(),"sha256":hash(&packet),"unix_ms":ms()})).as_bytes())?;
            durable(&mut tx, &packet)?;
            destructive = true;
            progress(Progress {
                phase: stage.clone(),
                completed_blocks: acknowledged,
                total_blocks: BLOCKS,
                destructive_started: true,
            });
            let write_result = serial.write_frame(&packet);
            durable(&mut events, format!("{}\n",json!({"event":"tx_return","stage":stage,"reported_written":write_result.as_ref().ok().copied(),"error":write_result.as_ref().err().map(ToString::to_string),"unix_ms":ms()})).as_bytes())?;
            let written = write_result.map_err(|e| format!("{stage}: {e}"))?;
            if written != packet.len() {
                return Err(format!(
                    "{stage}: неполная отправка {written}/{}. Автоповтор отключён.",
                    packet.len()
                ));
            }
            let byte = serial
                .read_byte(TIMEOUT)
                .map_err(|e| format!("{stage}: {e}"))?;
            if let Some(b) = byte {
                durable(&mut rx, &[b])?;
            }
            if byte != Some(6) {
                return Err(format!(
                    "{stage}: ожидался ACK 06, получено {}. Запись остановлена.",
                    byte.map_or("таймаут".to_string(), |b| format!("{b:02X}"))
                ));
            }
            ack_count += 1;
            if index >= 3 {
                acknowledged += 1;
            }
            durable(&mut events, format!("{}\n",json!({"event":"ack","stage":stage,"acked_blocks":acknowledged,"ack_count":ack_count})).as_bytes())?;
        }
        stage = "Ожидание итогового подтверждения".into();
        progress(Progress {
            phase: stage.clone(),
            completed_blocks: acknowledged,
            total_blocks: BLOCKS,
            destructive_started: true,
        });
        let byte = serial.read_byte(TIMEOUT).map_err(|e| e.to_string())?;
        if let Some(b) = byte {
            durable(&mut rx, &[b])?;
        }
        if byte != Some(6) {
            return Err("Нет итогового ACK 06. Передача файла не подтверждена; не считать прошивку успешной.".into());
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
    let outcome = result.and_then(|()| close.map(|_| ()).map_err(|e| format!("Закрытие COM: {e}")));
    let error = outcome.as_ref().err().cloned();
    write_json(
        &directory.join("manifest.json"),
        &json!({"schema":1,"status":if outcome.is_ok(){"transfer_acknowledged"}else{"failed"},"finished_unix_ms":ms(),"port":port,"image_sha256":image.sha256,"last_stage":stage,"erase_sequence_attempted":destructive,"acked_blocks":acknowledged,"ack_count":ack_count,"final_ack_received":final_ack,"port_closed":closed,"error":error,"close_error":close_error,"automatic_retries":0,"hardware_boot_verified":false,"mcu_readback_verified":false,"calibration_after_boot_verified":false}),
    )?;
    outcome?;
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
) -> Result<FlashReport, String> {
    #[cfg(windows)]
    {
        run(
            image,
            port,
            log_dir,
            |name| {
                crate::firmware_windows::WindowsPort::open(name)
                    .map_err(|e| format!("Не удалось открыть {name}: {e}"))
            },
            on_progress,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (image, port, log_dir, on_progress);
        Err("Запись поддерживается только в Windows".into())
    }
}

#[cfg(test)]
mod tests;
