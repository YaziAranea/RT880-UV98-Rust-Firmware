# R41 PRE-ALPHA — буквенный Roger, PA5 и CPS 0.8

**Экспериментальная пре-альфа. Установка и использование на свой страх и риск, без гарантий — см. LICENSE.** Испытания на ПК не подтверждают работу конкретной платы или совместимость по эфиру.

- **Позывной:** отдельный пункт «Имя рации», до 16 латинских букв, цифр и пробелов; набор повторными нажатиями, как на кнопочном телефоне. MENU сохраняет имя. «Имя после PTT» включает передачу в конце разговора.
- **Штатный буквенный Roger:** реализован формат имени заводской RT880 1.16, обозначенный там как MDC1200. Передача и приём имени проверены по исходным машинным функциям стока. Совместимость между физическими рациями ещё нужно проверить. Числовой MDC PTT-ID остаётся отдельной функцией.
- **Сохранение:** имя и переключатель Roger записываются в журнал внутренней Flash. Обычное выключение сохраняет их; полная перепрошивка очищает журнал. Остальные настройки пока остаются в RAM. Калибровки и внешняя NOR не перезаписываются.
- **Мощность:** крупные **L — красная, M — жёлтая, H — зелёная**, с привязкой к фактическому передающему A/B.
- **КВ-водопад:** перерисовываются только изменившиеся строки. В контрольных сценариях 101 или 64 строки вместо 187. Плавность физического экрана и короткий ответ репитера остаются проверками на устройстве.
- **CPS 0.8:** совместимый BIN можно выбрать отдельно, без каталога нового выпуска и повторного скачивания неизменившейся CPS. Сохраняются проверки структуры и персонализация по собственным калибровкам. Структурная проверка не подтверждает подлинность издателя или совместимость платы.
- **PA5:** отдельная RX-only диагностика без пайки: ручной захват 256 мс, статистика ADC, выгрузка исходных отсчётов и WAV через кабель. Это проверка доступности аудиопотока; полноценное шумоподавление DSP ещё не реализовано.

Скачать:

- Обычная прошивка: [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r41/UV98-r41-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r41/UV98-r41-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r41/UV98-r41-de.bin).
- [CPS 0.8 для Windows](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r41/UV98-CPS-0.8.0.zip) — отдельный архив, сохраняйте для следующих совместимых BIN.
- [PA5: диагностический BIN, сборщик WAV и инструкция](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r41/UV98-r41-pa5-diagnostic.zip).
- [Комплект трёх прошивок и проверки](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r41/UV98-r41-firmware.zip) · [исходники](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r41/UV98-r41-source.zip) · [SHA-256](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r41/SHA256SUMS.txt).

BIN предназначены для исследованного профиля AT32F423. Перед записью проверьте ревизию платы, сохраните резервы и подготовьте персональный образ в CPS по своему проверенному снимку. [Установка](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r41/docs/GETTING-STARTED.ru.md) · [клавиши и проверка Roger](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r41/R41-NOTES-RU.txt) · [проверка PA5](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r41/research/r41/PA5-TEST-RU.txt).

Проверки включают host-тесты, выполнение связанного ARM-кода старта и записи журнала в модели периферии, проверку ELF/BIN и рендерер интерфейса. Агент не открывал COM-порт и не прошивал устройство. Точные хеши и результаты находятся в манифесте и отчётах комплекта.

**English:** experimental pre-alpha. R41 adds the stock RT880 1.16 name/Roger format, persistent 16-character Latin station name, colored power badges, partial HF waterfall redraw and a separate passive PA5 audio diagnostic. CPS 0.8 imports compatible application BIN files independently of a release catalog. Name settings survive power-off but are erased by a firmware update. RF interoperability, real audio capture and physical display performance still require device tests. Use at your own risk; see LICENSE.
