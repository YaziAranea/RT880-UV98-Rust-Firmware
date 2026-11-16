![UV98 Rust firmware — English, Deutsch, Русский](docs/images/project-banner.svg)

# Radtel RT-880 / iRadio UV-98 Plus — Rust Firmware

> **PRE-ALPHA — experimental firmware and CPS.** This project is at a very early stage, before alpha. Features may fail; installation can leave the radio unusable. Install and use entirely at your own risk. The software is provided as is, without warranty; the authors disclaim liability for damage or loss under the terms of the license. Read the license and backup/recovery instructions before deciding to install.

**Open-source Rust firmware and Windows CPS for Radtel RT-880 (RT880) / iRadio UV-98 Plus radio platform.**

Independent A/B operation, a colour spectrum display, shortwave tools and access to the source.

**English** · [Deutsch](README.de.md) · [Русский](README.ru.md)

[**Firmware BIN**](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r38/UV98-r38.bin) · [**Firmware + CPS 0.7.0**](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r38/UV98-r38-2026-11-16.zip) · [**Start here: backup and installation**](docs/GETTING-STARTED.en.md) · [Release notes](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/tag/r38)

> **Experimental firmware for the investigated AT32F423 board profile.** Use CPS to prepare a personal R38 image from your radio's verified NOR backup before flashing. CPS binds the image to your factory calibration. Verify board compatibility before proceeding. [Backup and preparation guide](docs/GETTING-STARTED.en.md#4-check-compatibility-before-flashing)

## Windows CPS 0.7.0

Use **Язык / Language / Sprache** in the toolbar to select **Русский**, **English** or **Deutsch**. All eight tabs and existing status messages switch immediately. Unsaved edits stay in place, and CPS remembers the selection in your Windows user profile. File dialog titles and filters follow CPS. Standard dialog buttons follow Windows.

The R38 kit bundles the released CPS 0.7.0 build. [CPS 0.7.0 source](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/tree/cps-v0.7.0)

![CPS 0.7.0 channel editor in English](docs/images/cps-0.7.0/en-channels.png)

*Headless render of the CPS interface from egui with simulated data.*

## On the screen

<table>
<tr>
<td align="center"><img src="docs/images/r38/en-home.png" width="240" alt="English dual A/B frequency screen with signal meters"><br><b>A/B channels and VFOs</b></td>
<td align="center"><img src="docs/images/r38/en-spectrum.png" width="240" alt="Colour frequency spectrum and waterfall"><br><b>Spectrum and waterfall</b></td>
</tr>
<tr>
<td align="center"><img src="docs/images/r38/en-hf.png" width="240" alt="Shortwave receiver with an RSSI waterfall"><br><b>Shortwave tools</b></td>
<td align="center"><img src="docs/images/r38/en-menu.png" width="240" alt="English radio menu"><br><b>English · Deutsch · Русский</b></td>
</tr>
<tr>
<td align="center"><img src="docs/images/r38/en-tx-b.png" width="240" alt="Red TX outline on VFO B while VFO A is selected"><br><b>PTT B with A selected</b></td>
<td align="center"><img src="docs/images/r38/en-battery-low.png" width="240" alt="Low battery indication on the main screen"><br><b>Low battery</b></td>
</tr>
<tr>
<td align="center"><img src="docs/images/r38/en-cb-grid.png" width="240" alt="40-channel CB grid at channel 15 AM"><br><b>CB grid</b></td>
<td align="center"><img src="docs/images/r38/en-mdc.png" width="240" alt="MDC1200 settings for slot B and its local name"><br><b>MDC1200</b></td>
</tr>
</table>

*UI simulations rendered by the same Rust renderer used on the radio. Signals and readings in these illustrations are simulated; the images are not RF measurements.*

## New in R38

- **CB grid.** The 40-channel CEPT grid for VFO A opens on channel 15 AM at 27.135 MHz on first use. Use `#` for the zero-ending frequencies. Any long digit can open the grid. Hold EXIT on the grid to restore the previous A context. CB AM is receive-only. [CB controls](docs/GETTING-STARTED.en.md#cb-grid)
- **TX outline.** A red outline follows the slot that actually transmits. With the default bindings, the upper PTT requests A and the lower PTT requests B. [Simulated TX on A with B selected](docs/images/r38/en-cb-tx-a.png)
- **MDC1200.** Set an individual PTT ID and choose off, start, end or both for each slot. The 12-character RU/EN name is a local label stored in RAM. [MDC1200 and keypad controls](docs/GETTING-STARTED.en.md#mdc1200-and-local-name)
- **Display and shortwave.** Each LCD update slice can handle several changed regions. The Si4732 waterfall redraws the rows affected by the RSSI sweep. Timing and reception on the radio still need verification. [R38 evidence and test limits](research/r38/EVIDENCE-RU.md)

## What is included

| Area | Features |
|---|---|
| Everyday operation | Independent A/B channel or VFO contexts, zones, repeater shift, CTCSS/DCS indications, segmented signal meters and assignable keys. |
| Finding signals | Frequency scanning, spectrum cursor and window control, peak listening and a colour waterfall. |
| Shortwave | Si4732 receiver, AM/USB/LSB/CW controls, band and step selection, sequential RSSI waterfall, AM filters and soft mute. |
| Signalling | MDC1200 PTT ID, keypad DTMF while holding PTT, and five-digit ZVEI1/CCIR identification at the end of transmission. |
| Navigation | GPS position handling and an explicitly requested APRS position beacon through the supported C-BUS modem. |
| Windows CPS | Channel/zone editing, calibration and NOR backups, GPS/APRS file settings, image validation and flashing through the stock loader. |
| Languages | English, German and Russian documentation and selectable radio UI. Channel names remain the user's own text. |

The stock “800 MHz” mode starts at **840 MHz**. The preset uses 850.000 MHz.

## Start with a backup

1. Keep the radio on its stock firmware and save its original `.880dmf` / `.98pdmf` file.
2. Use our CPS to save **calibration**, **4 MiB NOR** and **settings/channel sections**. Each operation reads twice and compares the results.
3. Keep the complete snapshot folders and a matching factory recovery file in two places. The public package does not include the manufacturer's firmware.
4. Check board compatibility, then prepare your personal R38 image in CPS from the verified NOR snapshot. No Rust compiler is needed for this step.

The [file checklist and step-by-step guide](docs/GETTING-STARTED.en.md) names every CPS button and expected file. **The 4 MiB NOR backup is not an MCU firmware or bootloader dump.**

## Current limits

R33 is the owner's accepted hardware baseline. R38 still needs on-radio checks of TX indication, display timing, CB/HF operation and MDC1200 decoding. Build and simulation results are in the [R38 release notes](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/tag/r38).

- Dual Watch alternates the receiver. Simultaneous audio from two BK4819 receivers is not implemented.
- The HF waterfall retunes one Si4732 and interrupts audio during the sweep; it is not an FFT.
- APRS is currently a manual beacon. An APRS receive UI, automatic beacons and the alternative GPIO modem remain unfinished.
- AM filters/soft mute are available; general-purpose voice NR/NB on the main BK4819 is not implemented.
- Radio settings, including the selected UI language, are currently volatile. The CPS file editor is not yet a USB settings-write protocol for the Rust firmware.

## Source and development

| Path | Purpose |
|---|---|
| [`firmware-diagnostic/`](firmware-diagnostic/) | Executable firmware and the main radio loop. |
| [`crates/`](crates/) | Drivers, calibration interpretation, RF runtime, UI and signalling. |
| [`apps/cps/`](apps/cps/) | Windows CPS source. |
| [`apps/handheld-image-sim/`](apps/handheld-image-sim/) | Screen simulator using the shared renderer. |
| [`research/`](research/) | Hardware evidence and protocol notes. |

[Build instructions](BUILDING.md) · [Contributing](CONTRIBUTING.md) · [R37 handoff](HANDOFF-R37.md) · [Requirements and observations](FEATURES-NEXT-RU.txt) · [Proposed features (Russian)](docs/NEXT-FEATURES.ru.md) · [Private input policy](PRIVATE-INPUTS.md)

Personal calibration blocks, NOR dumps and codeplugs are excluded from the repository. Some verification uses an original private fixture; see the build instructions before attempting to reproduce it. Original project code and documentation use the [MIT licence](LICENSE); third-party files retain their [own licences and attribution](THIRD-PARTY-NOTICES.md).

For a useful issue, include the release and BIN hash, radio model/board information, exact steps, expected/actual behaviour and a photo of the relevant screen. Do not attach your full NOR, private codeplug or personal location logs to a public issue.
