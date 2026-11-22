# R43.1 PRE-ALPHA — первый PTT · пропуск в скане · разнос VFO

Экспериментальная прошивка для исследованного профиля AT32F423 в iRadio UV-98 Plus / RT-880. Перед установкой проверьте ревизию платы, сохраните файлы восстановления и подготовьте в CPS персональный BIN по проверенному снимку NOR своей станции. [Установка и восстановление](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.1/docs/GETTING-STARTED.ru.md) · [Лицензия](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.1/LICENSE).

- **Первый PTT после покоя.** При одиночном нестабильном фронте верхней PTT в обычном RX прошивка один раз повторяет полный опрос. Дребезг, другие клавиши и условия разрешения TX проверяются штатно. На подтверждённом TX верхний красный край нужной рамки A/B рисуется в первом доступном окне LCD.
- **Ответ после PTT.** Из R43 сохранён приоритет приёмника на выбранном VFO в течение до 80 мс после готовности RX. Штатные паузы и условия открытия звука сохранены.
- **Шумящий канал.** На паузе скана короткое `*` исключает текущий канал или частоту VFO до остановки скана. При новом запуске список пропусков пуст.
- **Разнос.** В редакторе видна расчётная TX-частота для черновика разноса. Для AM и недопустимой частоты показывается прочерк.

RU-образ записан на станцию. При первом прогоне обе PTT сработали, хвост репитера слышен. Редкий отказ первого PTT после простоя требует серии повторов. Пропуск в скане и редактор разноса проверены на ПК. Сборки RU, EN и DE прошли проверки связанного ARM-кода, журнала настроек и структуры BIN. [Сценарии проверки R43](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.1/R43-NOTES-RU.txt) · [Проверка первого PTT](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r43.1/R43.1-NOTES-RU.txt).

**Скачать** [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.1/UV98-r43.1-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.1/UV98-r43.1-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.1/UV98-r43.1-de.bin) · [CPS 0.8.0 для Windows](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.1/UV98-CPS-0.8.0.zip) · [Комплект прошивок](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.1/UV98-r43.1-firmware.zip) · [SHA-256](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r43.1/SHA256SUMS.txt).

CPS 0.8.0 совпадает с R42 побайтно. В установленной CPS откройте **Прошивка → Выбрать BIN…**. Полная перепрошивка очищает внутренний журнал имени, Roger и усиления микрофона.

**English.** Experimental pre-alpha. R43.1 retries a clean unstable upper PTT edge once in normal receive and prioritizes the confirmed A/B TX border. It keeps R43’s post-PTT receiver priority and adds temporary scan skip on `*` and TX-frequency preview in the offset editor. Initial hardware check on the RU build observed both PTTs and a repeater tail. The intermittent first-press issue still needs repeat testing.
