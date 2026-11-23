# R54 PRE-ALPHA — RT-880 / UV-98 Plus · scan, PTT, APRS · CPS 0.8.1

[Русский](#русский) · [English](#english) · [Deutsch](#deutsch)

[**RT-880 / UV-98 Plus + Windows CPS**](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r54/R54-RT880-UV98.zip) · [**Windows CPS 0.8.1**](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r54/UV98-CPS-0.8.1.zip)
BIN [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r54/UV98-r54-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r54/UV98-r54-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r54/UV98-r54-de.bin)
[PA5 RX-only](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r54/R54-PA5-RX-only.zip) · [Source](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r54/R54-source-RT880-UV98.zip) · [SHA-256](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r54/SHA256SUMS.txt) · [Manifest](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r54/release-manifest.json)

## Русский

- Боковую кнопку можно назначить на запуск и остановку скана. Скан зоны и «Все» учитывает исключения CPS, в том числе для приоритета. Открытый разговор удерживается. Короткая `*` временно пропускает шумящий канал до следующего запуска скана.
- После передачи верхняя PTT повторно проверяет короткое нажатие во время восстановления RX. В кроссбенде станция повторно проверяет кнопки и несущую перед включением передатчика.
- На экране APRS видны счётчики принятых байтов, годных пакетов, ошибок CRC, задержек опроса и переполнений. Ручной маяк при SQL 0 сразу подсказывает выбрать SQL 1–10. На КВ в AM с Si4732 долгое `2` переключает soft mute, долгое `3` — фильтр 100 Гц. Для измерения звука есть отдельная сборка PA5 только для приёма.
- На компьютере прошли 1014 Rust-тестов и отдельный тест отрисовки, 106 Python-тестов и симуляция 144 кадров. Все три основных BIN воспроизведены побайтно из архива исходников. R54 ещё предстоит проверить на станции. Свободно до журнала RU/EN/DE — 9656/10224/10104 байта. [Аудит и программа испытаний](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r54/AUDIT-R54.ru.md)

Сборка рассчитана на исследованный профиль платы AT32F423. Перед записью проверьте ревизию и подготовьте в CPS личный BIN по полной проверенной копии NOR своей станции. Полная запись очищает внутренний журнал настроек. [Установка и восстановление](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/main/docs/GETTING-STARTED.ru.md)

## English

- An assigned side button starts and stops scanning. Zone and All scans respect CPS exclusions, including when selecting the priority channel. An open conversation holds the scan. Tap `*` to skip a noisy channel until the next scan.
- Upper PTT rechecks a short press while RX recovers after transmission. Crossband relay rechecks the carrier and operator input immediately before enabling TX.
- The APRS receive screen counts bytes, accepted packets, CRC errors, long polling intervals and modem overruns. The manual beacon prompts for squelch 1–10 when set to zero. In Si4732 shortwave AM, hold `2` for soft mute and `3` for the independent 100 Hz high-pass filter. The separate PA5 image is for receive-only audio measurements.
- Software verification covered 1014 Rust tests plus a renderer profiling test, 106 Python tests and 144 simulator frames. Rebuilding the source archive reproduced all three standard BINs byte for byte. Hardware testing of R54 is pending.

For the tested AT32F423 board profile, confirm the board revision and use CPS to prepare a personal BIN from your radio's verified full NOR backup. A full flash clears the internal settings journal. [Installation and recovery](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/main/docs/GETTING-STARTED.en.md)

## Deutsch

- Eine zugewiesene Seitentaste startet und stoppt den Suchlauf. Der Zonen- und Gesamtsuchlauf berücksichtigt die in CPS ausgeschlossenen Kanäle auch bei der Prioritätswahl. Bei einem laufenden Gespräch bleibt der Suchlauf auf dem Kanal. Ein kurzer Druck auf `*` überspringt einen störenden Kanal bis zum nächsten Suchlauf.
- Die obere PTT prüft einen kurzen Tastendruck beim Übergang zum Empfang erneut. Vor dem Sendestart prüft das Crossband-Relais Träger und Tasteneingaben ein zweites Mal.
- Der APRS-Empfang zeigt die Anzahl der Bytes, gültigen Pakete, CRC-Fehler, langen Abfrageintervalle und Modemüberläufe. Bei SQL 0 fordert die manuelle Bake zur Wahl von SQL 1–10 auf. Im Kurzwellen-AM-Betrieb mit Si4732 schaltet langes Drücken von `2` Soft Mute und von `3` den separaten 100-Hz-Hochpass. Das gesonderte PA5-Abbild dient der Audiomessung im Empfangsbetrieb.
- 1014 Rust-Tests, ein separater Test der Bildausgabe, 106 Python-Tests und 144 simulierte Bildschirmbilder wurden geprüft. Alle drei regulären BIN-Dateien ließen sich aus dem Quellarchiv bytegleich erzeugen. Die Geräteprüfung von R54 steht aus.

Die Firmware ist für das untersuchte AT32F423-Platinenprofil vorgesehen. Vor dem Flashen die Platinenrevision prüfen und in CPS ein persönliches BIN aus der vollständigen, geprüften NOR-Sicherung des eigenen Geräts erstellen. Beim vollständigen Flashen wird das interne Einstellungsjournal gelöscht. [Installation und Wiederherstellung](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/main/docs/GETTING-STARTED.de.md)
