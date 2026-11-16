# Back up first, then install

> **PRE-ALPHA — experimental firmware and CPS.** This project is at a very early stage, before alpha. Features may fail; installation can leave the radio unusable. Install and use entirely at your own risk. The software is provided as is, without warranty; the authors disclaim liability for damage or loss under the terms of the license. Read the license and backup/recovery instructions before deciding to install.

[Project home](../README.md) · [Deutsch](GETTING-STARTED.de.md) · [Русский](GETTING-STARTED.ru.md)

This guide covers first-time preparation of a UV-98 Plus / the investigated RT-880 platform. **Read the original data while stock firmware is installed. Choose firmware only after checking compatibility.** You can complete the backup without installing Rust firmware.

R42 adds SYSTEM / TX → MIC GAIN: 0–31, MENU saves and EXIT cancels. 0 is minimum sensitivity, not mute. Name, Roger and gain share internal Flash storage; a full firmware update erases it. Short `#` in HF scope selects 8/16/32 measurements, with 8 by default. CPS0.8 is unchanged; no new download is required. [R42 device checks (Russian)](../R42-NOTES-RU.txt).

## 1. Prepare the radio and a folder

1. Charge the battery. Connect the programming cable to the computer and radio.
2. Turn the radio on **normally, without holding PTT**. The stock channel/frequency screen should appear. A black screen with a green LED is the bootloader and is not the mode for reading backups.
3. Use the stock CPS for your model to read the radio and save its original `.880dmf` or `.98pdmf` file. Keep an existing original such as `999.880dmf` unchanged.
4. Close the stock CPS, Radio Tuning Software and other programs using the cable. Before the first read in our CPS, power the radio off for 3 seconds, then start it normally again.
5. Create a folder such as `Documents/UV98-backup-2026-11-16`. Suggested subfolders: `01-original`, `02-calibration`, `03-nor`, `04-settings`, `05-recovery`. Put the original CPS file in `01-original`.
6. Extract [CPS 0.8.0 for Windows](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-CPS-0.8.0.zip). Run `UV98-CPS.exe`, choose **English** in **Язык / Language / Sprache**, then open **Backups**. Select the actual cable COM port using **Refresh list → Available ports**. Old port numbers in screenshots and logs may differ.

The toolbar language selector is always visible. All eight tabs and existing status messages switch immediately and unsaved edits stay in place. CPS remembers the selection in your Windows user profile. File dialog titles and filters follow CPS. Standard dialog buttons follow Windows.

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

R42 targets the investigated AT32F423 board profile. Verify the board revision before flashing. Backup-based preparation binds the image to your calibration.

For a confirmed compatible board, prepare a personal BIN. **No Rust compiler is needed.**

1. Download the original R42 BIN in the desired language. [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-r42-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-r42-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-r42-de.bin) In CPS open **Firmware → Select BIN…** and choose that file. A release catalog is optional for a compatible original application BIN with a calibration profile.
2. Click **Prepare an image for my radio…**.
3. Select **`manifest.json` from the full 4 MiB NOR snapshot** created by “Read settings storage (4 MiB)”. Select the manifest, not `storage-address-space.bin` itself. A calibration-only snapshot is not sufficient.
4. Choose a parent folder for new images, for example `Documents/UV98-personal`. CPS creates a new `UV98-personal-…` directory inside it.
5. Wait for successful preparation. In the output folder, CPS writes `firmware/UV98-personal.bin`, `firmware/UV98-source.bin` and `firmware/catalog.json`. CPS selects the new personal BIN.
6. Keep the entire folder. When reopening `UV98-personal.bin`, keep its source BIN and catalog beside it.

CPS checks the two-read status, file sizes and SHA256 values, and verifies that the calibration block matches the beginning of NOR. It checks the original BIN's size, vectors and calibration profile before personalization. The new BIN contains the expected **SHA256 of your calibration**. At boot the firmware compares it with the radio's actual calibration. CPS preserves the input BIN and backup.

The catalog records the personal BIN's SHA256 and schema 2 personalization provenance, including the source and snapshot hashes. CPS structural checks establish file layout and integrity. Verify the release source, publisher and board revision independently.

If preparation is rejected, check that you selected your own complete two-read NOR snapshot and the original R42 BIN. Keep the original manifest and snapshot files together.

Proceed only after successful preparation, confirmation of board compatibility, and saving verified backups and the correct factory BIN in two places.

## 5. Install a compatible image

1. Open **Firmware → Select BIN…** in CPS 0.8.0.
2. Select the prepared `UV98-personal.bin` and check its name and SHA256 on screen. CPS verifies it using the accompanying source BIN and personal catalog.
3. Switch the radio off. Hold the **upper PTT**, turn it on, then release PTT after 2–3 seconds. On the investigated radio, the stock loader shows a black screen and green LED.
4. Select the cable's COM port. Tick the loader readiness confirmation, then click **Write firmware** once.
5. Wait for explicit completion. Do not switch off the radio, unplug the cable or close CPS during writing.
6. After completion, switch off for 3 seconds and start **without PTT**. Check the display, keys and reception. Keep the operation log.

**Firmware language.** Flash the BIN for your chosen language. Each R42 BIN has one interface language. Set the CPS language separately in its toolbar.

With the default bindings, the upper PTT requests A and the lower PTT requests B. A green border marks the receiving slot. The red TX border takes priority. A large red **L**, yellow **M** or green **H** shows the configured power of the transmitting slot, including B while A is selected. Each slot's power starts at LOW and changes are held in RAM. The battery icon shows up to four approximate level bars and the voltage in 0.1 V steps (`--.-V` when unavailable). [Lower PTT hardware evidence](../research/r37/ptt.md)

Loader acknowledgements confirm file transfer. Check boot and reception on the radio. R42 prioritizes receiver setup after PTT release. Check whether the start of a short repeater response is audible, and verify the RX/TX outlines and DTMF window on the device.

### CB grid

1. Open **MENU → CHANNELS / VFO → CB GRID**. The first opening after power-on selects channel **15 AM, 27.135 MHz** in the 40-channel CEPT grid.
2. Use ↑/↓ or enter channel **1–40**, then press MENU to tune VFO A. Use `#` to switch between the five- and zero-ending grids. Channel 15 on the zero-ending grid is **27.130 MHz**. Use `*` to choose AM or FM.
3. For a shortcut, open **MENU → KEYS → LONG-PRESS KEYS**, press the digit you want to assign, choose **CB GRID**, then confirm with MENU. Hold that digit on the home screen to open the grid.
4. Hold EXIT on the grid to leave CB and restore the previous VFO A context. VFO B is independent. The last confirmed CB selection stays available until power-off.

**Experimental CB AM.** AM in the CB grid on VFO A receives on the nominal channel frequency. PTT transmits narrow FM 2 kHz higher. On channel 15, the CEPT grid uses **RX 27.135 → TX 27.137 MHz**, and the zero-ending grid uses **RX 27.130 → TX 27.132 MHz**. In CB FM, both use the nominal channel frequency. VFO B keeps its own settings. Ordinary AM modes still support reception only. Both CB transmit modes use the existing TX checks.

An AM receiver may detect the offset FM signal on the slope of its filter. Reception depends on the receiver's filter and FM deviation. Compatibility on the radio remains to be tested.

### Shortwave AM

Hold `1` to cycle the 6, 3 and 1 kHz filters. Hold `2` to toggle soft mute. In ordinary listening these changes keep the current Si4732 frequency.

### Incoming DTMF

In ordinary BK4819 FM reception with audio open, the home-screen popup appears automatically. It retains the most recent 16 symbols for three seconds after the last symbol. It closes when reception mode changes or TX starts. AM, spectrum, shortwave and crossband use no DTMF decode window. Select V, ZVEI and CCIR are removed.

### Numeric MDC1200

1. On the home screen, select A or B. Hold `*` to switch slots. Open **MENU → GPS / SIGNALS → MDC1200** and check the slot in the header.
2. In **MDC TIMING**, choose **OFF**, **START OF TX**, **END OF TX** or **START AND END OF TX**. The initial mode is **OFF**. Confirm with MENU.
3. In **UNIT ID**, enter four hex digits from `0001` through `DEEE`. Tap 0–9 for digits and hold 1–6 for A–F. Use `*` to delete, MENU to accept, and EXIT to cancel. Confirm an empty field to clear the ID.

Numeric MDC timing and ID are stored in RAM until power-off.

### Radio name after PTT

1. Open **MENU → SYSTEM / TX → RADIO NAME**. Enter up to 16 uppercase Latin letters, digits or spaces. Tap a digit repeatedly to cycle its characters. For example, `2` cycles A, B, C and 2. Tap `0` for a space or hold a digit to enter the digit itself.
2. Press ↑ to finish the current character, or wait about 650 ms before using the same key for another letter. Use `*` to delete the last character. Press **MENU/OK** to save or EXIT to cancel.
3. Set **SYSTEM / TX → NAME AFTER PTT** to **ON** or **OFF** and confirm with **MENU/OK**. If saving is unconfirmed, check the shown value and retry.

The name and the PTT toggle share an internal Flash journal. After power-off, check both values. A full firmware write erases the journal. At startup with an empty journal, the firmware loads usable name and Roger settings from the stock NOR. [Simulated R42 name editor](images/r42/en-radio-name.png)

When enabled, the radio sends the name after PTT in the stock RT880 1.16 Roger format. The numeric MDC PTT ID has separate settings. If both modes are active, the name uses the end of TX and the numeric MDC ID can still be sent at the start. On-radio compatibility and storage checks are pending. [R42 notes (Russian)](../R42-NOTES-RU.txt)

### Separate PA5 audio test

The RX-only PA5 image records one short receive-audio block for inspection on a PC. [PA5 test procedure (Russian)](../research/r42/PA5-TEST-RU.txt)

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
