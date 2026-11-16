# R42 PRE-ALPHA — меню, Roger, быстрый КВ и микрофон

**Экспериментальная пре-альфа. Установка и использование на свой страх и риск, без гарантий — см. LICENSE.** Проверки на ПК не подтверждают работу конкретной платы или совместимость по эфиру.

- **Меню:** сохранено залитое выделение. Сначала рисуется актуальный пункт, затем предыдущий; глифы текста кэшируются. Изменённые строки объединяются в ограниченные пакеты с проверкой клавиш и обоих PTT между окнами LCD. Плавность на реальном экране ещё нужно проверить.
- **Имя после PTT:** исправлена подготовка посылки, которая могла истечь по таймеру до запуска модема при медленном основном цикле. Подготовка идёт несколькими ограниченными шагами со свежими проверками PTT/TOT. Имя сохраняет CTCSS/DCS канала. Формат — штатный буквенный Roger RT880 1.16, не азбука Морзе. Передача и распознавание между физическими рациями требуют проверки.
- **Возврат RX:** настройка приёмника начинается сразу после подтверждённого выключения передатчика, до UART и LCD. Штатные паузы настройки и условия открытия звука сохранены. Приём возвращается на выбранный VFO; временного переключения на другой VFO нет. Короткий ответ репитера ещё нужно проверить.
- **КВ-обзор:** по умолчанию 8 измеряемых частот. Короткое `#` в обзоре переключает 8 → 16 → 32, меняя детализацию при близком охвате. Повторный замер уже настроенной частоты не ждёт очередного интервала 750 мс. Готовность Si4732 и частота проверяются. Это последовательные измерения RSSI, не FFT; звук на время обзора прерывается.
- **Микрофон:** «СИСТЕМА / TX → МИКРОФОН», 0–31. MENU подтверждает, EXIT отменяет. 0 — минимальная чувствительность, не отключение. Настройка сохраняется вместе с именем и Roger во внутреннем журнале Flash. Полная перепрошивка очищает журнал. Начальное значение берётся из штатных настроек, при недопустимом значении — 16.
- **CPS 0.8 не менялась:** архив побайтно совпадает с R41. Уже установленную CPS скачивать заново не нужно. Откройте новый исходный BIN через «Прошивка → Выбрать BIN…» и подготовьте персональный образ по своему проверенному снимку.
- **Сохранены:** батарея и напряжение, крупные L/M/H, независимые PTT A/B, зелёная RX и красная TX-рамка нужного VFO, автоматический FM DTMF, CB-сетка и экспериментальные +2 кГц на TX в CB AM, отдельные RU/EN/DE BIN. Select V удалён ранее.
- **DSP:** доступность аудио на PA5 пока не подтверждена на устройстве. Отдельный RX-only диагностический комплект позволяет захватить отсчёты и WAV без пайки. Полноценное шумоподавление аудиопотока пока не реализовано.

Скачать:

- Обычная прошивка: [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-r42-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-r42-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-r42-de.bin).
- [Неизменившаяся CPS 0.8 для Windows](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-CPS-0.8.0.zip).
- [PA5: диагностика, сборщик WAV и инструкция](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-r42-pa5-diagnostic.zip).
- [Комплект трёх прошивок и проверки](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-r42-firmware.zip) · [исходники](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/UV98-r42-source.zip) · [SHA-256](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r42/SHA256SUMS.txt).

BIN предназначены для исследованного профиля AT32F423. [Установка](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r42/docs/GETTING-STARTED.ru.md) · [проверка R42 на рации](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r42/R42-NOTES-RU.txt) · [проверка PA5](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r42/research/r42/PA5-TEST-RU.txt).

Проверки включают host-тесты, выполнение связанного ARM-кода старта и записи журнала в модели периферии, проверку ELF/BIN и имитацию интерфейса. Точные результаты и хеши находятся в манифесте комплекта. COM-порт не открывался, устройство не прошивалось. Аппаратные проверки остаются открытыми.

**English:** experimental pre-alpha. R42 prioritizes and caches filled menu rows, fixes name-burst setup timing while preserving channel tones, restores the selected receiver before display/log work, adds measured 8/16/32-point HF sweeps and persistent microphone gain 0–31. CPS 0.8 is byte-for-byte unchanged. RF interoperability, display performance, repeater pickup and PA5 audio require device tests. Full audio DSP noise reduction is not implemented. Use at your own risk; see LICENSE.
