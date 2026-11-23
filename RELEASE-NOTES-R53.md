# R53 PRE-ALPHA — compact firmware modules / компактные модули

[Русский](#русский) · [English](#english) · [Deutsch](#deutsch)

**Firmware / Прошивка / Firmware** [RU BIN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r53/UV98-r53-ru.bin) · [EN BIN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r53/UV98-r53-en.bin) · [DE BIN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r53/UV98-r53-de.bin) · [Firmware ZIP](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r53/UV98-r53-firmware.zip)

**Tools and source / Инструменты и исходники / Werkzeuge und Quellcode** [Windows CPS 0.8.0](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r53/UV98-CPS-0.8.0.zip) · [Source ZIP](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r53/UV98-r53-source.zip) · [SHA-256](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r53/SHA256SUMS.txt) · [Build manifest](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r53/release-manifest.json)

## Русский

Экспериментальная прошивка для исследованного профиля платы AT32F423 в iRadio UV-98 Plus / Radtel RT-880. Проверьте ревизию платы и подготовьте в CPS персональный BIN по проверенному снимку NOR своей станции. Полная запись очищает внутренний журнал настроек. CPS 0.8.0 совпадает с R43.3 побайтно. [Установка и восстановление](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r53/docs/GETTING-STARTED.ru.md).

- Со времени публичной R43.3 добавлен приоритетный канал при сканировании зоны. Временный пропуск по `*` сохранён. Также добавлены поиск RX-тона CTCSS/DCS и восстановление последнего сохранённого выбора A/B, канала зоны или частоты VFO.
- В передаче добавлены тон 1750 Гц, три временных слота DTMF, таймер 10/20/30 секунд и показ просадки батареи при наличии корректных замеров.
- На платах с поддерживаемым C-BUS/CMX868 доступны экран приёма APRS с историей четырёх пакетов в RAM, ручной маяк и включаемый оператором GPS-маяк по скорости и курсу. Автоматический режим выключен при включении питания.
- В R53 отдельно выделены координация скана и отрисовка меню. Уплотнены шрифты и таблица усиления AM. По сравнению с R52 освобождено 3540 байт Flash для RU и EN и 3536 для DE. До журнала свободно 11 152, 11 720 и 11 592 байта соответственно. Длина каждого BIN — 251 904 байта.

Проверка на компьютере включает побайтную пересборку трёх BIN, 766 тестов прошивки, 58 тестов CPS, 55 тестов инструментов сборки, 69 сценариев старта и отказов ARM и пять сценариев журнала на локаль. Сверены 1668 кадров навигации с исключением номера версии. После передачи RU-образа загрузчик подтвердил все 246 блоков и завершение записи. На одном RT-880 оператор подтвердил запуск с R53 на экране, срабатывание обеих PTT и слышимый хвост репитера. Проверки остальных функций и локалей на рации остаются открытыми. [Программа проверки](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r53/R53-NOTES-RU.txt).

## English

Experimental firmware for the investigated AT32F423 board profile in the iRadio UV-98 Plus / Radtel RT-880. Verify the board revision and use CPS to prepare a personal BIN from your radio's verified NOR snapshot. A full firmware write clears the internal settings journal. CPS 0.8.0 is byte-identical to the R43.3 download. [Installation and recovery](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r53/docs/GETTING-STARTED.en.md).

- Since the public R43.3, zone scanning has gained a priority channel and retained temporary skip with `*`. R53 also includes CTCSS/DCS receive-tone search and restoration of the saved A/B selection, zone channel or VFO frequency.
- Transmit features include a 1750 Hz tone, three temporary DTMF memory slots, a 10/20/30-second time-out timer and battery voltage sag display when valid readings are available.
- Boards with the supported C-BUS/CMX868 modem gain an APRS receive screen with four packets of history in RAM and an opt-in GPS beacon based on speed and course, alongside the manual beacon. Automatic beaconing is off at power-up.
- R53 separates scan coordination and menu rendering and packs fonts and the AM gain table more tightly. Relative to R52, it frees 3,540 bytes of Flash in RU and EN and 3,536 in DE. Space before the journal is 11,152, 11,720 and 11,592 bytes respectively. Each BIN is 251,904 bytes.

Offline verification includes byte-identical rebuilds of all three BINs, 766 firmware tests, 58 CPS tests, 55 build-tool tests, 69 ARM startup and fault cases and five settings journal cases per locale. Comparison covered 1,668 navigation frames with the version number masked. The bootloader acknowledged all 246 blocks and the end of the RU transfer. On one RT-880, the operator confirmed that R53 booted, both PTT buttons worked and the repeater tail was audible. On-radio checks of other features and locales remain open.

## Deutsch

Experimentelle Firmware für das untersuchte AT32F423-Platinenprofil im iRadio UV-98 Plus / Radtel RT-880. Platinenrevision prüfen und in CPS aus der verifizierten NOR-Sicherung des eigenen Funkgeräts eine persönliche BIN vorbereiten. Ein vollständiges Firmware-Update löscht das interne Einstellungsjournal. CPS 0.8.0 ist bytegleich mit dem Download von R43.3. [Installation und Wiederherstellung](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r53/docs/GETTING-STARTED.de.md).

- Seit der öffentlichen R43.3 unterstützt der Zonenscan einen Prioritätskanal. Das vorübergehende Überspringen per `*` bleibt erhalten. R53 bietet außerdem CTCSS/DCS-Empfangstonsuche und die Wiederherstellung der gespeicherten A/B-Auswahl, des Zonenkanals oder der VFO-Frequenz.
- Beim Senden stehen ein 1750-Hz-Ton, drei temporäre DTMF-Speicher, ein Sendezeitlimit von 10/20/30 Sekunden und bei gültigen Messwerten eine Anzeige des Spannungseinbruchs zur Verfügung.
- Platinen mit unterstütztem C-BUS/CMX868-Modem bieten eine APRS-Empfangsansicht mit vier Paketen im RAM sowie neben der manuellen Bake eine zuschaltbare GPS-Bake nach Geschwindigkeit und Kurs. Beim Einschalten ist die Automatik aus.
- R53 trennt Scansteuerung und Menüdarstellung in eigene Module und verkleinert Schrift- und AM-Verstärkungstabellen. Gegenüber R52 werden in RU und EN je 3.540 Byte Flash frei, in DE 3.536 Byte. Vor dem Journal bleiben 11.152, 11.720 bzw. 11.592 Byte frei. Jede BIN ist 251.904 Byte groß.

Die Offline-Prüfung umfasst bytegleiche Neubauten aller drei BINs, 766 Firmware-Tests, 58 CPS-Tests, 55 Tests der Build-Werkzeuge, 69 ARM-Start- und Fehlerfälle und fünf Journalfälle pro Sprache. 1.668 Navigationsbilder wurden bei ausgeblendeter Versionsnummer verglichen. Der Bootloader bestätigte alle 246 Blöcke und den Abschluss der RU-Übertragung. An einem RT-880 bestätigte der Bediener den Start mit R53 auf dem Display, die Funktion beider PTT-Tasten und den hörbaren Relaisnachlauf. Gerätetests der weiteren Funktionen und Sprachen stehen aus.
