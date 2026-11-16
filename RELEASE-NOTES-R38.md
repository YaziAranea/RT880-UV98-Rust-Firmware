# R38 — display scheduling, CB grid and MDC1200

**Experimental pre-alpha. Install and use at your own risk. No warranty;
see the license.** This release is software-verified, not hardware-qualified.

- Menu redraws share a bounded row budget across separate damaged regions.
  Routine UART logging waits for idle time, and telemetry stays consistent
  during a draw pass. A representative menu move uses three foreground passes
  instead of seven in the host test; this is not a measured LCD frame rate.
- CB channel grid for A, starting at **15 AM / 27.135 MHz**, with an explicit
  5 kHz / zero-ending switch, numeric selection and an assignable long key.
  B is preserved; leaving CB restores A's previous context. AM is receive-only.
- The actual transmitting A/B card has a red outline, independent of focus.
- Optional **MDC1200 PTT-ID** at beginning, end or both, separately for A/B.
  Default off. Modem setup and restoration remain interruptible by the
  physical PTT/TOT guard. Radio-side MDC reception is not implemented.
- RU/EN keypad text entry for a **local label**, plus hexadecimal MDC ID entry.
  MDC sends the numeric ID, not the typed label. Settings and labels are held
  in RAM and reset on power-off.
- The HF plot publishes complete measured sweeps. A small progress indicator
  replaces repeated redraws of the entire waterfall. Processing/mode changes
  discard stale measurements. It remains a sequential RSSI sweep, not an FFT.

The kit includes unchanged **CPS 0.7.0**, with explicit RU/EN/DE selection.
Prepare a per-radio image using CPS and your verified NOR backup before flashing.

Validation: **607 Rust tests passed, 1 ignored; 80 Python tests; 59 ARM cases;
59 decoder cases; calibration-profile checks; 116 simulated PNG frames.**
Physical LCD smoothness, MDC interoperability/deviation, CB RF performance and
HF sweep timing still require radio tests. Frequency-offset FM is not offered
as a supposedly compatible AM transmitter.

## По-русски

Исправлены найденные причины рывков экрана, добавлены CB-сетка для A,
красная рамка активного TX, MDC1200 PTT-ID и ввод локального текста RU/EN.
КВ-водопад больше не стирает предыдущий график каждым отдельным замером.

**Меню → Каналы / VFO → Сетка CB:** 15 AM / 27,135 МГц; `#` — пятёрки/нули,
`*` — AM/FM, стрелки или номер — канал, MENU — выбрать, долгий EXIT —
восстановить прежний A. AM-передача не реализована.

**Меню → GPS / Сигналы → MDC1200:** выключено/начало/конец/оба; ID отдельно
для A/B. Удержание цифр 1–6 вводит A–F. Текст — локальная подпись до 12
символов: `#` — RU/EN, ВВЕРХ — следующая буква, `*` — удалить,
MENU — сохранить, EXIT — отменить. В эфир уходит ID; текст остаётся
в рации. После выключения новые настройки и текст сбрасываются.

Плавность на настоящем LCD и MDC с другой рацией ещё не проверены.
Подробности: [отчёт](R38-REPORT-RU.txt),
[исследование](research/r38/EVIDENCE-RU.md),
[подготовка своего образа](docs/GETTING-STARTED.ru.md).
