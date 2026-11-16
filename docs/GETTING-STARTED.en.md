# Back up first, then install

> **PRE-ALPHA — experimental firmware and CPS.** This project is at a very early stage, before alpha. Features may fail; installation can leave the radio unusable. Install and use entirely at your own risk. The software is provided as is, without warranty; the authors disclaim liability for damage or loss under the terms of the license. Read the license and backup/recovery instructions before deciding to install.

[Project home](../README.md) · [Deutsch](GETTING-STARTED.de.md) · [Русский](GETTING-STARTED.ru.md)

This guide covers first-time preparation of a UV-98 Plus / the investigated RT-880 platform. **Read the original data while stock firmware is installed. Choose firmware only after checking compatibility.** You can complete the backup without installing Rust firmware.

## 1. Prepare the radio and a folder

1. Charge the battery. Connect the programming cable to the computer and radio.
2. Turn the radio on **normally, without holding PTT**. The stock channel/frequency screen should appear. A black screen with a green LED is the bootloader and is not the mode for reading backups.
3. Use the stock CPS for your model to read the radio and save its original `.880dmf` or `.98pdmf` file. Keep an existing original such as `999.880dmf` unchanged.
4. Close the stock CPS, Radio Tuning Software and other programs using the cable. Before the first read in our CPS, power the radio off for 3 seconds, then start it normally again.
5. Create a folder such as `Documents/UV98-backup-2026-11-16`. Suggested subfolders: `01-original`, `02-calibration`, `03-nor`, `04-settings`, `05-recovery`. Put the original CPS file in `01-original`.
6. Extract the entire [R38 package with CPS 0.7.0](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r38/UV98-r38-2026-11-16.zip). Run `UV98-CPS.exe`, choose **English** in **Язык / Language / Sprache**, then open **Backups**. Select the actual cable COM port using **Refresh list → Available ports**. Old references to COM20/COM21 are not your required port number.

The toolbar language selector is always visible. All eight tabs and existing status messages switch immediately and unsaved edits stay in place. CPS remembers the selection in your Windows user profile. File dialog titles and filters follow CPS. Standard dialog buttons follow Windows. The R38 kit includes the released CPS 0.7.0 build.

Keep a short text note beside the backups with the radio model, stock version and board revision if known.

## 2. Perform three reads

Before each operation, select its subfolder with **Snapshot folder…**. CPS creates a new `UV98-read-…` directory inside it. Keep the complete directory.

| Order | CPS button | Expected output |
|---|---|---|
| 1 | **Read calibration (4 KiB)** | `calibration.bin`, exactly **4096 bytes**: this radio's individual factory data. |
| 2 | **Read settings storage (4 MiB)** | `storage-address-space.bin`, exactly **4194304 bytes**, plus a separate `calibration.bin`: the external NOR address space available to the protocol. |
| 3 | **Read channels, settings, and calibration** | `calibration.bin`, `cfg.bin`, `channels.bin`, `zones.bin`, `fm.bin`: separate usable sections. |

The second operation takes the longest. Wait for **both passes**. Do not operate the radio or change its settings during a read.

After **each** operation, look for:

> Both reads matched. The snapshot and log were saved. The port is closed.

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

Place the file named `RT-880_Upgrade_V1_16_250915.bin` in a `recovery` folder beside `UV98-CPS.exe` (create the folder if needed). **Save stock BIN 1.16…** can then make a separate copy; CPS checks its SHA. You can also select this original BIN directly on the Firmware tab. Do not rename another version to this filename.

## 4. Check compatibility before flashing

R38 targets the investigated AT32F423 board profile. Verify the board revision before flashing. Backup-based preparation binds the image to your calibration.

For a confirmed compatible board, prepare a personal BIN. **No Rust compiler is needed.**

1. Open **Firmware** in our CPS. Select the original `UV98-r38.bin` from the current package.
2. Click **Prepare an image for my radio…**.
3. Select **`manifest.json` from the full 4 MiB NOR snapshot** created by “Read settings storage (4 MiB)”. Select the manifest, not `storage-address-space.bin` itself. A calibration-only snapshot is not sufficient.
4. Choose a parent folder for new images, for example `Documents/UV98-personal`. CPS creates a new `UV98-personal-…` directory inside it.
5. Wait for successful preparation. The output is `firmware/UV98-personal.bin` and `firmware/catalog.json`; CPS automatically selects the new BIN for flashing.
6. Keep the entire new folder. If you reopen CPS later, select `UV98-personal.bin` with its adjacent catalog intact.

CPS checks the two-read status, snapshot sizes and SHA values, calibration equality with the start of NOR, and the original template against its release catalog and calibration-profile marker. Only the expected **SHA of your calibration** is placed in the new BIN, not the calibration data themselves. The original image and backup remain unchanged. At boot, the firmware compares this hash with the actual calibration in the radio.

The personal BIN has its own SHA, recorded in the catalog created by CPS. Keep the BIN and catalog together and use the backup from your own radio.

If preparation is rejected, verify that you selected your own complete, verified NOR snapshot and the original R38 BIN from the same package. Keep validation enabled and use the original manifest and catalog.

Proceed only after successful preparation, confirmation of board compatibility, and saving verified backups and the correct factory BIN in two places.

## 5. Install a compatible image

1. Extract the whole package. CPS validates BIN files against `firmware/catalog.json`; a lone image beside an unrelated old CPS is not sufficient.
2. Open **Firmware** in CPS and select the prepared `UV98-personal.bin`. Check its name and SHA256 on screen; CPS compares them with the new personal catalog.
3. Switch the radio off. Hold the **upper PTT**, turn it on, then release PTT after 2–3 seconds. On the investigated radio, the stock loader shows a black screen and green LED.
4. Select the cable's COM port. Tick the loader readiness confirmation, then click **Write firmware** once.
5. Wait for explicit completion. Do not switch off the radio, unplug the cable or close CPS during writing.
6. After completion, switch off for 3 seconds and start **without PTT**. Check the display, keys and reception. Keep the operation log.

**Choose the display language:** MENU → СИСТЕМА / TX → ЯЗЫК / LANGUAGE → ENGLISH. Use ↑/↓, MENU to confirm and EXIT to cancel. Available choices are РУССКИЙ, ENGLISH and DEUTSCH. After changing to English, the path reads MENU → SYSTEM / TX → LANGUAGE. The selection currently resets to Russian after power-off.

With the default bindings, the upper PTT requests A and the lower PTT requests B. During transmission, the transmitting card shows TX and a red outline, regardless of the selected slot. Each slot's current power setting is held in RAM and starts at LOW. The battery icon shows up to four approximate level bars and the numeric voltage in 0.1 V steps (`--.-V` when unavailable). [Lower PTT hardware evidence](../research/r37/ptt.md)

Loader acknowledgements confirm file transfer. Check boot and operation on the device. R33 is the owner's accepted baseline. R38's TX display, lower PTT, battery readings, CB/HF operation and external MDC1200 decoding still need on-radio checks. [R38 evidence and test limits](../research/r38/EVIDENCE-RU.md)

### CB grid

1. Open **MENU → CHANNELS / VFO → CB GRID**. The first opening after power-on selects channel **15 AM, 27.135 MHz** in the 40-channel CEPT grid.
2. Use ↑/↓ or enter channel **1–40**, then press MENU to tune VFO A. Use `#` to switch between the five- and zero-ending grids. Channel 15 on the zero-ending grid is **27.130 MHz**. Use `*` to choose AM or FM. CB AM is receive-only. FM uses the existing TX checks.
3. For a shortcut, open **MENU → KEYS → LONG-PRESS KEYS**, press the digit you want to assign, choose **CB GRID**, then confirm with MENU. Hold that digit on the home screen to open the grid.
4. Hold EXIT on the grid to leave CB and restore the previous VFO A context. VFO B is independent. The last confirmed CB selection stays available until power-off.

### MDC1200 and local name

1. On the home screen, select A or B. Hold `*` to switch slots. Open **MENU → GPS / SIGNALS → MDC1200** and check the slot in the header.
2. In **MDC TIMING**, choose **OFF**, **START OF TX**, **END OF TX** or **START AND END OF TX**. The initial mode is **OFF**. Confirm with MENU.
3. In **UNIT ID**, enter four hex digits from `0001` through `DEEE`. Tap 0–9 for digits and hold 1–6 for A–F. Use `*` to delete, MENU to accept, and EXIT to cancel. Confirm an empty field to clear the ID.
4. In **NAME (LOCAL)**, tap a digit repeatedly to cycle its characters. Press ↑ before entering a second character on the same key. Use `#` to switch RU/EN and `*` to delete. MENU saves up to 12 characters. EXIT cancels the edit.

The MDC1200 packet carries the PTT ID. The name is a label on this radio. Timing, ID, name and other firmware settings are stored in RAM until power-off. [Simulated name editor](images/r38/en-mdc-alias-en.png)

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
