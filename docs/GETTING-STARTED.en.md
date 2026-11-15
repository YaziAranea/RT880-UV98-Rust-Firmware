# Back up first, then install

[Project home](../README.md) · [Deutsch](GETTING-STARTED.de.md) · [Русский](GETTING-STARTED.ru.md)

This guide covers first-time preparation of a UV-98 Plus / the investigated RT-880 platform. **Read the original data while stock firmware is installed. Choose firmware only after checking compatibility.** You can complete the backup without installing Rust firmware.

## 1. Prepare the radio and a folder

1. Charge the battery. Connect the programming cable to the computer and radio.
2. Turn the radio on **normally, without holding PTT**. The stock channel/frequency screen should appear. A black screen with a green LED is the bootloader and is not the mode for reading backups.
3. Use the stock CPS for your model to read the radio and save its original `.880dmf` or `.98pdmf` file. Keep an existing original such as `999.880dmf` unchanged.
4. Close the stock CPS, Radio Tuning Software and other programs using the cable. Before the first read in our CPS, power the radio off for 3 seconds, then start it normally again.
5. Create a folder such as `Documents/UV98-backup-2026-11-16`. Suggested subfolders: `01-original`, `02-calibration`, `03-nor`, `04-settings`, `05-recovery`. Put the original CPS file in `01-original`.
6. Extract the entire [package](https://github.com/YaziAranea/uv98-rust/releases/tag/r36). Run `UV98-CPS.exe` and open **«Резервные копии» (Backups)**. Select the actual cable COM port using **«Обновить список» (Refresh) → «Доступные порты» (Available ports)**. Old references to COM20/COM21 are not your required port number.

The CPS labels below match the current Russian Windows interface. Keep a short text note beside the backups with the radio model, stock version and board revision if known.

## 2. Perform three reads

Before each operation, select its subfolder with **«Папка для снимков…» (Snapshot folder)**. CPS creates a new `UV98-read-…` directory inside it. Keep the complete directory.

| Order | CPS button | Expected output |
|---|---|---|
| 1 | **Считать калибровки (4 КиБ)** — Read calibration | `calibration.bin`, exactly **4096 bytes**: this radio's individual factory data. |
| 2 | **Считать память настроек (4 МиБ)** — Read settings memory | `storage-address-space.bin`, exactly **4194304 bytes**, plus a separate `calibration.bin`: the external NOR address space available to the protocol. |
| 3 | **Считать каналы, настройки и калибровки** — Read channels, settings and calibration | `calibration.bin`, `cfg.bin`, `channels.bin`, `zones.bin`, `fm.bin`: separate usable sections. |

The second operation takes the longest. Wait for **both passes**. Do not operate the radio or change its settings during a read.

After **each** operation, look for:

> Два чтения совпали. Снимок и журнал сохранены. Порт закрыт.

This means: “Both reads matched. Snapshot and log saved. Port closed.”

Keep `manifest.json`, `started.json` and `pass-*.bin` along with the data files. A successful `manifest.json` contains:

```json
"status": "verified_two_reads",
"two_reads_byte_identical": true
```

A `.bin` file existing on disk does not by itself mean the backup succeeded. If reading fails, is cancelled or produces mismatched passes, do not proceed to flashing. Keep the failed snapshot, resolve the connection problem and start a new read in a new folder.

## 3. Check and store the backups

- Check the sizes of `calibration.bin` and `storage-address-space.bin` against the table.
- Compare the `calibration.bin` SHA256 values in the first and second read manifests. The factory block must match. Investigate a difference before proceeding.
- Store a **matching factory firmware file** in `05-recovery`. The public Rust package does not contain manufacturer firmware. For the investigated board, use the verified 1.16 source below; confirm compatibility before using it with another revision.
- Copy the entire backup folder to a second location: another drive, USB storage or your own protected storage. Do not post personal dumps in public GitHub issues.

Keep all of the following:

| Saved item | Purpose |
|---|---|
| Original `.880dmf` / `.98pdmf` | Channels and settings in the stock CPS format. |
| Verified `calibration.bin` | Individual factory calibration. |
| Verified `storage-address-space.bin` | External NOR memory within the readable address space. |
| Section snapshot folders and logs | Comparison, diagnosis and preparation of an edited settings copy. |
| Matching factory BIN | Restore the stock MCU application through the loader. |

**The 4 MiB NOR is not an MCU dump or bootloader copy.** The known stock protocol has no command to read those regions. Saving the factory BIN copies an existing recovery file; it does not extract the firmware from your radio. This backup set is therefore not a byte-for-byte clone of every chip in the radio.

Reading sections **does not automatically create a new `.880dmf`**. To make an updated file, open your original file in CPS, apply the read sections and save a separate copy. The `Time` section stays from that original file; calibration is stored separately. This additional step is not required merely to preserve the raw backups.

### Where to get the stock recovery file

The investigated profile uses stock 1.16: [file page in nicsure's archive](https://github.com/nicsure/RT880-FlasherX/blob/915545ae89f4a7547f6cea37408a5ba1d48e21f6/RT-880_Upgrade_V1_16_250915.bin) · [download BIN](https://raw.githubusercontent.com/nicsure/RT880-FlasherX/915545ae89f4a7547f6cea37408a5ba1d48e21f6/RT-880_Upgrade_V1_16_250915.bin). Expected size: **251904 bytes**. SHA256:

```text
eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
```

Place the file named `RT-880_Upgrade_V1_16_250915.bin` in a `recovery` folder beside `UV98-CPS.exe` (create the folder if needed). **«Сохранить заводской BIN 1.16…» (Save factory BIN 1.16)** can then make a separate copy; CPS checks its SHA. You can also select this original BIN directly on the Firmware tab. Do not rename another version to this filename.

## 4. Check compatibility before flashing

R36 targets the investigated AT32F423 board profile. A similar case or an RT-880 label does not establish compatibility with another revision. Backup-based preparation binds the image to your calibration; it does not automatically determine hardware compatibility.

For a confirmed compatible board, prepare a personal BIN. **No Rust compiler is needed.**

1. Open **«Прошивка» (Firmware)** in our CPS. Select the original `UV98-r36.bin` from the current package.
2. Click **«Подготовить образ для моей рации…» (Prepare image for my radio)**.
3. Select **`manifest.json` from the full 4 MiB NOR snapshot** created by “Read settings memory (4 MiB)”. Select the manifest, not `storage-address-space.bin` itself. A calibration-only snapshot is not sufficient.
4. Choose a parent folder for new images, for example `Documents/UV98-personal`. CPS creates a new `UV98-personal-…` directory inside it.
5. Wait for successful preparation. The output is `firmware/UV98-personal.bin` and `firmware/catalog.json`; CPS automatically selects the new BIN for flashing.
6. Keep the entire new folder. If you reopen CPS later, select `UV98-personal.bin` with its adjacent catalog intact.

CPS checks the two-read status, snapshot sizes and SHA values, calibration equality with the start of NOR, and the original R36 marker. Only the expected **SHA of your calibration** is placed in the new BIN, not the calibration data themselves. The original image and backup remain unchanged. At boot, the firmware compares this hash with the actual calibration in the radio.

The personal BIN's SHA differs from the original R36 template's SHA; that is expected. Writing uses the new catalog created by CPS. Do not move a lone BIN without its catalog or substitute a backup from another radio.

If preparation is rejected, do not edit the manifest or catalog to bypass validation. Check that you selected your own complete, verified NOR snapshot and the original R36 from the same package. R35 and older images do not support this preparation method.

Proceed only after successful preparation, confirmation of board compatibility, and saving verified backups and the correct factory BIN in two places.

## 5. Install a compatible image

1. Extract the whole package. CPS validates BIN files against `firmware/catalog.json`; a lone image beside an unrelated old CPS is not sufficient.
2. Open **«Прошивка» (Firmware)** in CPS and select the prepared `UV98-personal.bin`. Check its name and SHA256 on screen; CPS compares them with the new personal catalog.
3. Switch the radio off. Hold the **upper PTT**, turn it on, then release PTT after 2–3 seconds. On the investigated radio, the stock loader shows a black screen and green LED.
4. Select the cable's COM port. Tick the loader readiness confirmation, then click **«Записать прошивку» (Write firmware)** once.
5. Wait for explicit completion. Do not switch off the radio, unplug the cable or close CPS during writing.
6. After completion, switch off for 3 seconds and start **without PTT**. Check the display, keys and reception. Keep the operation log.

**Choose the display language:** MENU → СИСТЕМА / TX → ЯЗЫК / LANGUAGE → ENGLISH. Use ↑/↓, MENU to confirm and EXIT to cancel. Available choices are РУССКИЙ, ENGLISH and DEUTSCH. After changing to English, the path reads MENU → SYSTEM / TX → LANGUAGE. The selection currently resets to Russian after power-off.

Loader acknowledgements confirm file transfer. A successful boot and radio operation must be checked on the device. Current Rust settings live in RAM; the selected language is not yet retained after power-off.

## 6. Return to stock if needed

Close other programs. Enter the stock loader with the upper PTT again and select the **correct factory BIN** in our CPS. Write it using the same procedure and start normally after completion.

On the investigated device, writing the MCU application does not overwrite the external channels and calibration. **Reinstalling stock does not repair an already damaged NOR.** Our CPS does not yet write a raw 4 MiB dump or calibration block back to the radio. Damage to those areas needs a separate recovery plan using your saved files; never select them as an MCU firmware image.

| Problem | Next action |
|---|---|
| COM port busy or unavailable | Close all CPS programs. Reconnect the cable's USB end, refresh the list and select the actual port. |
| Read failed | Check normal stock boot. Retry into a new folder and retain the previous capture. |
| Writing interrupted | Keep the log. Re-enter the loader and write the correct complete BIN again. Once erasure has started, normal boot may fail until a write succeeds. |
| New image does not boot | Restore known matching stock through the loader. Do not erase NOR or alter calibration at random. |
| Loader does not appear | Stop and report the model, version, steps and log. Do not try unrelated BIN files. |
