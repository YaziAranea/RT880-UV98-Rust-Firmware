# R37 PRE-ALPHA · multilingual CPS 0.7.0

> **PRE-ALPHA — experimental firmware and CPS, before alpha.** Install and use entirely at your own risk. The software is provided as is, without warranty; liability is excluded under the terms of the license. Errors can leave a radio unusable. Read the backup and recovery instructions before installing firmware.
>
> **ПРЕ-АЛЬФА — экспериментальная прошивка и CPS, ещё до альфы.** Установка и использование полностью на свой страх и риск. ПО предоставляется как есть, без гарантий; ответственность определяется условиями лицензии. Возможны ошибки и потеря работоспособности станции. Перед прошивкой изучите резервирование и возврат на сток.

[**Current full kit / Новый комплект CPS 0.7.0**](https://github.com/YaziAranea/uv98-rust/releases/download/r37/UV98-r37-CPS-0.7.0-2026-11-16.zip) · [Firmware BIN / Прошивка](https://github.com/YaziAranea/uv98-rust/releases/download/r37/UV98-r37.bin) · [SHA256](https://github.com/YaziAranea/uv98-rust/releases/download/r37/UV98-r37-CPS-0.7.0-2026-11-16.zip.sha256)

## CPS 0.7.0 update / Обновление CPS

- Explicit selector at the top: **Язык / Language / Sprache → Русский / English / Deutsch**. The choice is saved for the Windows user and restored on restart.
- Все 8 вкладок, подсказки, предупреждения, ошибки ввода и сообщения операций переведены. Язык меняется сразу, включая уже показанные сообщения.
- Switching languages preserves the open document, unsaved drafts, selection and undo history. Radio data and protocol values are not translated. Standard native dialog buttons and OS error details follow Windows language.
- Калибровочные TXT-отчёты доступны на выбранном языке. Запись калибровок в рацию не добавлена.
- **Firmware R37 is unchanged. Updating CPS does not require reflashing the radio.** Прошивка R37 прежняя; достаточно распаковать новый комплект и запустить его UV98-CPS.exe.

132 tests passed across 17 suites; 48 headless UI combinations (8 tabs × 3 languages × 2 window sizes), native release build and independent code review passed. Computer Use and radio access were not performed; the complete native GUI workflow is still not hardware acceptance.

[CPS 0.7.0 source](https://github.com/YaziAranea/uv98-rust/tree/cps-v0.7.0) · [Detailed report / Подробный отчёт](https://github.com/YaziAranea/uv98-rust/blob/main/CPS-0.7-REPORT-RU.txt) · [Инструкция RU](https://github.com/YaziAranea/uv98-rust/blob/main/docs/GETTING-STARTED.ru.md) · [English](https://github.com/YaziAranea/uv98-rust/blob/main/docs/GETTING-STARTED.en.md) · [Deutsch](https://github.com/YaziAranea/uv98-rust/blob/main/docs/GETTING-STARTED.de.md)

## Firmware R37 / Прошивка R37

Battery level icon and voltage readout; `PTT A/B` inside the transmitting card; lower PTT can transmit B while A stays selected. Same-page menu movement renders 15 rows instead of 67 (about 78% less pixel payload, not a measured on-radio FPS gain).

The original firmware validation remains: 572 Rust tests passed, 1 ignored; 80 Python tests; 59 linked ARM scenarios and 59 log decodes; two calibration-profile checks. **R37 has not been tested on the radio.** Battery accuracy, lower PTT and LCD responsiveness await on-radio acceptance.

R37 is a template: use CPS and your own verified NOR backup before installing on another compatible unit. This does not establish board compatibility. Radio settings remain in RAM; per-slot power starts at LOW. APRS RX/automatic beacons, simultaneous audio from both BK receivers and main-BK NR/NB remain unfinished. Manufacturer firmware and personal radio data are not bundled.

The original R37 tag and older CPS 0.6.1 kit remain unchanged for traceability; use the **CPS 0.7.0 kit linked above** for the new interface. The new source.zip belongs to the CPS tag, while firmware provenance remains linked to r37.

CPS 0.7.0 EXE SHA256: `c5cb90c29ea9c70c125a9e6777d7f09a20e877f2f109c56e64c8ba52f14a7619`

Unchanged R37 BIN SHA256: `f3a4033d0121443d0604d283458c50d7318b814f18b76753bf517bd8c1ff0f80`
