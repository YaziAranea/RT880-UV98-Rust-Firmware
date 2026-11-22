# R43.2 PRE-ALPHA — короткие и повторные нажатия PTT

Экспериментальная прошивка для исследованного профиля AT32F423 в iRadio UV-98 Plus / RT-880. Перед установкой проверьте ревизию платы и подготовьте в CPS персональный BIN по проверенному снимку NOR своей станции. [Установка и восстановление](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.2/docs/GETTING-STARTED.ru.md) · [Лицензия](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.2/LICENSE).

- **Короткая PTT A/B.** Три чистых наблюдения за 12 мс или дольше подтверждают нажатие. Отпускание сбрасывает неподтверждённый тап. Для PTT B используется текущий отсчёт ADC, поэтому фильтр боковых клавиш больше не задерживает её отпускание.
- **Свежая проверка перед TX.** До начала настройки передатчика проверяется, что та же кнопка удерживается. Отпускание до завершения настройки отменяет запуск. Аппаратные паузы, контроль удержания и TOT сохранены.
- **Приоритет опроса.** В обычном приёме главный экран уступает опросу при новом вводе между окнами LCD. Возврат RX и приоритет ответа репитера работают по логике R43.1. [Изменения R43.1](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/tag/r43.1).

RU-образ записан и запущен на рации. По первой пробе короткие нажатия стали отзывчивее. Частота пропусков по каждой кнопке и минимальная рабочая длительность ещё требуют замера. На ПК прошли 681 тест прошивки и 58 тестов CPS, включая подготовку и повторное открытие этого BIN. Для RU, EN и DE проверены связанный ARM-код, журнал настроек и структура BIN. [Сценарий проверки](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.2/R43.2-NOTES-RU.txt).

**Скачать** [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.2/UV98-r43.2-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.2/UV98-r43.2-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.2/UV98-r43.2-de.bin) · [CPS 0.8.0 для Windows](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.2/UV98-CPS-0.8.0.zip) · [Комплект прошивок](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.2/UV98-r43.2-firmware.zip) · [SHA-256](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.2/SHA256SUMS.txt).

CPS 0.8.0 совпадает с предыдущим выпуском побайтно. В CPS откройте **Прошивка → Выбрать BIN…**. Полная перепрошивка очищает внутренний журнал имени, Roger и усиления микрофона.

**English.** Experimental pre-alpha. R43.2 confirms short A/B presses after three clean observations spanning at least 12 ms. PTT B uses the current ADC reading, and the firmware checks that the same button is still held before TX setup. Post-PTT RX retains the R43.1 behavior. Initial testing of the RU build found short presses more responsive. Per-button miss rates and minimum effective press duration still need measurement.
