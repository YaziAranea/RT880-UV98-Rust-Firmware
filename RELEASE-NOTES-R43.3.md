# R43.3 PRE-ALPHA — больше свободной Flash

Экспериментальная прошивка для исследованного профиля AT32F423 в iRadio UV-98 Plus / RT-880. Перед установкой проверьте ревизию платы и подготовьте в CPS персональный BIN по проверенному снимку NOR своей станции. [Установка и восстановление](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.3/docs/GETTING-STARTED.ru.md) · [Лицензия](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.3/LICENSE).

- Освободили **10 264 байта Flash** в каждой локали. До журнала настроек теперь свободно **10 412 байт в RU**, **10 928 в EN** и **10 820 в DE**. BIN сохраняет штатную длину 251 904 байта.
- Убрали повторы в форматировании стартовых UART-кадров, строках шрифта и машинном коде Thumb. Сохранили набор функций, уровни оптимизации UI и LCD, а также границы загрузчика и журнала. PTT и возврат RX используют логику R43.2. [Изменения R43.2](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/tag/r43.2).
- Чистая пересборка дала побайтно совпадающие RU, EN и DE. Прошли 682 теста прошивки и 58 тестов CPS. На 69 сценариях старта и отказов UART-кадры и записи в регистры совпали с R43.2. Из 121 кадра интерфейса изменился экран с номером версии.

На станции с RU-образом подтвердили обычный запуск, работу обеих PTT и слышимый хвост репитера. Оператор отметил более плавное меню. Задержки на устройстве и частота редких пропусков короткой PTT остаются открытыми. [Сценарий проверки](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.3/R43.3-NOTES-RU.txt).

**Скачать** [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.3/UV98-r43.3-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.3/UV98-r43.3-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.3/UV98-r43.3-de.bin) · [CPS 0.8.0 для Windows](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.3/UV98-CPS-0.8.0.zip) · [Комплект прошивок](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.3/UV98-r43.3-firmware.zip) · [SHA-256](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.3/SHA256SUMS.txt).

CPS 0.8.0 совпадает с предыдущим выпуском побайтно. В CPS откройте **Прошивка → Выбрать BIN…**. Полная перепрошивка очищает внутренний журнал имени, Roger и усиления микрофона.

**English.** R43.3 frees 10,264 bytes of flash in each language build. The feature set, bootloader boundary and settings journal layout are retained. Initial RU hardware testing confirmed boot, both PTT buttons and the repeater tail. The menu felt smoother during this test. Hardware timing and intermittent short-press miss rates remain unmeasured.
