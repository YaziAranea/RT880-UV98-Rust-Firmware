# R39 — experimental CB pseudo-AM TX +2 kHz

**Experimental pre-alpha. Install and use at your own risk, without warranty;
see the license. Hardware interoperability has not been verified.**

CB AM on VFO A now receives AM at the channel frequency and transmits narrow
FM exactly **2 kHz higher**. Channel 15 CEPT: **27.135 MHz RX → 27.137 MHz TX**;
zero-ending grid: **27.130 → 27.132 MHz**. This intentionally uses the remote
AM receiver's filter slope; intelligibility depends on its filter and deviation.
CB FM stays on-channel. B and ordinary AM/airband restrictions are preserved.

The transmitting card shows the actual shifted frequency, FM and its red border,
including A transmission while B is selected. PTT/TOT and calibrated TX checks
remain active; release restores the selected receiver through the normal path.

Large UI state copies were removed. Extra UART key/RX/profiling snapshots are
now opt-in through `verbose-runtime`; LCD measurements, RF events and faults
remain available. CPS 0.7.0 is reused unchanged, with explicit RU/EN/DE selection.

Validation: **611 Rust tests passed, 1 ignored; 80 Python tests; 59 ARM cases;
59 decoder cases; calibration checks; 116 simulated PNGs.** No radio was accessed
or flashed. Prepare your own image in CPS from your verified NOR backup.

## По-русски

**MENU → КАНАЛЫ / VFO → СЕТКА CB**, выбрать AM клавишей `*`, подтвердить MENU.
На приёме — частота канала AM, при нажатии PTT A — узкая FM на **+2 кГц**.
Это экспериментальная псевдо-AM; разборчивость с обычной AM-рацией пока не проверена.

Ввод позывного уже есть: **GPS / СИГНАЛЫ → MDC1200 → ИМЯ (ЛОКАЛЬНО)**.
Повторные нажатия перебирают буквы; `#` — RU/EN, `↑` — следующая буква
на той же клавише, `*` — удалить, MENU — сохранить. До 12 символов в RAM,
после выключения сбрасывается. В эфир передаётся цифровой MDC-ID.

[Отчёт](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r39/R39-REPORT-RU.txt) ·
[Принцип и проверки](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r39/research/r39/EVIDENCE-RU.md) ·
[Подготовка своего образа](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r39/docs/GETTING-STARTED.ru.md)
