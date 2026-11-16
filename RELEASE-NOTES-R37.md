# R37 — battery telemetry, independent PTT A/B and faster menu redraw

> **PRE-ALPHA — experimental firmware and CPS.** This project is at a very early stage, before alpha. Features may fail; installation can leave the radio unusable. Install and use entirely at your own risk. The software is provided as is, without warranty; the authors disclaim liability for damage or loss under the terms of the license. Read the license and backup/recovery instructions before deciding to install.

> **ПРЕ-АЛЬФА — экспериментальная прошивка и CPS.** Проект находится на очень ранней стадии, ещё до альфа-версии. Возможны ошибки, потеря работоспособности и повреждение станции. Решение об установке и всё использование — полностью на ваш страх и риск. ПО предоставляется как есть, без гарантий; авторы не несут ответственности за ущерб или убытки в соответствии с условиями лицензии. До установки прочитайте лицензию и инструкции резервирования и возврата на сток.

[Firmware BIN](https://github.com/YaziAranea/uv98-rust/releases/download/r37/UV98-r37.bin) · [Full kit with CPS 0.6.1](https://github.com/YaziAranea/uv98-rust/releases/download/r37/UV98-r37-2026-11-16.zip) · [Backup and installation](https://github.com/YaziAranea/uv98-rust/blob/r37/docs/GETTING-STARTED.en.md) · [Инструкция RU](https://github.com/YaziAranea/uv98-rust/blob/r37/docs/GETTING-STARTED.ru.md)

## Changes / Что изменилось

- Battery icon with up to four voltage bands and a numeric 0.1 V readout; red low-battery indication and an unknown-value state. Добавлены деления батареи и напряжение; точность ещё нужно сверить мультиметром.
- The top TX strip is replaced by `PTT A` / `PTT B` in the transmitting card. Состояние передачи, рамка и шкала относятся к фактически передающему A/B.
- With default bindings, the lower PTT transmits on B even while A is selected. The selected RX/editing slot stays unchanged. Нижняя PTT использует собственные частоту, сдвиг, тоны и текущие RAM-настройки B; сохранены отпускание, TOT и блокировки.
- Same-page menu movement renders 15 rows instead of 67: 7200 rather than 32160 pixel bytes, about 78% less. Это измерение работы рендерера; реальную задержку LCD ещё не измеряли.
- CPS 0.6.1, updated EN/DE/RU guides and 18 explicitly simulated gallery images. [Предложения следующих функций](https://github.com/YaziAranea/uv98-rust/blob/r37/docs/NEXT-FEATURES.ru.md).

## Validation and limits / Проверки и ограничения

572 Rust tests passed, 1 ignored; 127 CPS tests; 80 Python tests; 59 linked ARM scenarios and 59 log decodes; 2 linked calibration-profile checks; 101 simulated frames. Independent code review passed. Original private NOR/calibration files are unchanged.

**No radio was accessed or flashed for R37.** Battery accuracy, the lower PTT mapping and actual LCD responsiveness require hardware acceptance. Passing host tests does not establish RF operation or recovery.

**R37 is a template.** Before installing on another unit, use CPS to prepare an image from that unit's verified NOR backup. This does not prove compatibility with another board revision. Manufacturer firmware and personal radio data are not bundled.

Settings remain in RAM. Per-slot power starts at LOW after reboot; stored codeplug power is not yet applied. The complete CPS personalization GUI path still awaits acceptance. APRS RX/automatic beacons, simultaneous audio from two BK receivers and main-BK NR/NB are unfinished.

The ZIP includes firmware/catalog, CPS/runtime DLL, source.zip, documentation, licenses, verification reports and file hashes. [Подробный отчёт](https://github.com/YaziAranea/uv98-rust/blob/r37/R37-REPORT-RU.txt).
