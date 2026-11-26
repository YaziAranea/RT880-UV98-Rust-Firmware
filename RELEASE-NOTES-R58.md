# R58 PRE-ALPHA · RT-880 / UV-98 Plus · CPS 0.11.0

[Полный комплект / Complete kit / Komplettpaket](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58/R58-RT880-UV98.zip) · [Windows CPS 0.11.0](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58/UV98-CPS-0.11.0.zip)

BIN [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58/UV98-r58-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58/UV98-r58-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58/UV98-r58-de.bin)

## Русский

- Заголовки и выбранные параметры открываемых страниц выводятся раньше остального содержимого. При сканировании зоны активная строка сохраняет последнюю карточку до подтверждения перестройки, затем обновляет изменившиеся цифры и связанные поля.
- В VFO можно задать границы и шаг сканирования. Журнал хранит до 16 частот с пиком RSSI, количеством открытий и временем последнего открытия от включения. Из журнала частоту можно перенести в VFO A или B либо пометить для пропуска VFO-сканом.
- При прослушивании сигнала водопад продолжает добавлять измерения удерживаемой частоты. Верхний график сохраняет последний обзор полосы. Короткое **#** переключает **АВТОЗВУК** и **АВТООБЗОР**.

### Принять 126,400 МГц в спектре

1. В режиме VFO введите **126400** и подтвердите **MENU**. Удерживайте **0**, если на нуле сохранено штатное назначение. Через меню тот же экран открывается по пути **ПОИСК / СПЕКТР → СПЕКТР ЧАСТОТ**.
2. Рядом с частотой курсора отображается режим. **AUTO** выбирает **AM** от 118,000 МГц до 137,000 МГц с исключённой верхней границей. На остальных частотах он выбирает **FM**.
3. Для ручного выбора во время обзора нажмите **MENU → ↑**. В пункте **AM/FM** переключайте **AUTO**, **AM** и **FM** кнопкой **MENU**. Нажмите **EXIT**, чтобы закрыть меню, и **6**, чтобы слушать курсор. Короткое **0** возвращает к обзору.

Выбор действует и при ручном прослушивании, и при автоматическом захвате. Станция хранит режим спектра, границы сканирования и журнал до выключения. Настройки приёма обеих строк A/B сохраняются при выходе из временного прослушивания.

Распакуйте комплект и откройте шаблон BIN своего языка в CPS 0.11.0. В публичных шаблонах хеш калибровок нулевой. Перед записью подготовьте персональный образ. Выберите **Подготовить образ для моей рации…** и укажите манифест проверенного полного снимка своей станции. Полная запись очищает журнал настроек. После запуска откройте **СИСТЕМА / TX → О ПРОШИВКЕ** и проверьте **R58-SC3**. [Подготовка, установка и управление сканированием](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r58/docs/GETTING-STARTED.ru.md)

На одной станции с SC3 RU исчезли заметные зависания интерфейса. Исполняемая секция R58 побайтно совпадает с соответствующей сборкой SC3 для RU, EN и DE. В R58 обновлены подписи версии и поле персонализации. Приём AM на реальном сигнале предстоит проверить отдельно.

## English

- Pages draw their titles and selected controls before the rest of their content. During zone scanning, the active line shows the last confirmed channel while the receiver retunes. The changed digits and related fields then update.
- VFO scanning accepts custom limits and step size. The log holds up to 16 frequencies with peak RSSI, opening count and time of last opening since power-on. Recall a frequency to VFO A or B, or mark it for the VFO scan to skip.
- The waterfall keeps taking measurements at the held frequency while you listen. The upper graph keeps the previous sweep. Tap **#** to switch between **AUTO AUDIO** and **AUTO SWEEP**.

### Listen to 126.400 MHz from the spectrum

1. In VFO mode, enter **126400** and press **MENU**. Hold **0** with the default shortcut, or open **SCAN/SPECTRUM → SPECTRUM**.
2. The mode appears beside the cursor frequency. **AUTO** selects **AM** from 118.000 MHz up to but excluding 137.000 MHz. It selects **FM** elsewhere.
3. To set the mode manually during a sweep, press **MENU → ↑**. In **AM/FM**, press **MENU** to cycle through **AUTO**, **AM** and **FM**. Press **EXIT** to close the menu, then **6** to listen at the cursor. Tap **0** to return to the sweep.

The mode applies to manual listening and automatic signal capture. The radio keeps the spectrum mode, scan limits and log until power-off. Temporary listening preserves the settings of both A/B slots.

Extract the kit and select the BIN template for your language in CPS 0.11.0. The public templates have a zero calibration digest. Prepare a personal image before flashing. Choose **Prepare an image for my radio…** and select the manifest of your radio's verified full backup. A full firmware write clears the settings journal. After boot, check **R58-SC3** under **SYSTEM / TX → ABOUT FIRMWARE**. [Preparation, installation and scan controls](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r58/docs/GETTING-STARTED.en.md)

The noticeable UI freezes disappeared on one radio running SC3 RU. The R58 executable section matches the corresponding SC3 build byte for byte in RU, EN and DE. R58 updates the version markings and calibration personalization field. AM reception on a live signal still needs a device check.

## Deutsch

- Beim Öffnen einer Seite erscheinen zuerst der Titel und das ausgewählte Bedienelement. Beim Zonensuchlauf bleibt der zuletzt bestätigte Kanal sichtbar, bis der Empfänger neu abgestimmt ist. Danach werden die geänderten Ziffern und die zugehörigen Felder aktualisiert.
- Für den VFO-Suchlauf lassen sich Grenzen und Schrittweite einstellen. Das Protokoll speichert bis zu 16 Frequenzen mit RSSI-Spitzenwert, Anzahl der Öffnungen und Zeitpunkt der letzten Öffnung seit dem Einschalten. Frequenzen lassen sich in VFO A oder B übernehmen oder zum Überspringen beim VFO-Suchlauf markieren.
- Beim Abhören misst der Wasserfall auf der gehaltenen Frequenz weiter. Das obere Diagramm behält den letzten Durchlauf. Ein kurzer Druck auf **#** wechselt zwischen **AUTO AUDIO** und **AUTO SCAN**.

### 126,400 MHz im Spektrum abhören

1. Im VFO **126400** eingeben und mit **MENU** bestätigen. Bei Standardbelegung **0** gedrückt halten oder **SCAN/SPEKTRUM → SPEKTRUM** öffnen.
2. Die Modulation steht neben der Cursorfrequenz. **AUTO** wählt **AM** ab 118,000 MHz bis unter 137,000 MHz. Auf den übrigen Frequenzen wählt es **FM**.
3. Während eines Durchlaufs **MENU → ↑** drücken. Unter **AM/FM** mit **MENU** zwischen **AUTO**, **AM** und **FM** wechseln. Mit **EXIT** das Menü schließen und mit **6** die Cursorfrequenz abhören. Ein kurzer Druck auf **0** setzt den Durchlauf fort.

Die Einstellung gilt für das manuelle Abhören und den automatischen Signalstopp. Spektrummodus, Suchlaufgrenzen und Protokoll bleiben bis zum Ausschalten erhalten. Beim vorübergehenden Abhören bleiben die Einstellungen der Zeilen A und B erhalten.

Das Paket entpacken und die BIN-Vorlage für die gewünschte Sprache in CPS 0.11.0 auswählen. In den öffentlichen Vorlagen steht ein Nullwert für den Kalibrierungs-Hash. Vor dem Flashen ein persönliches Image vorbereiten. **Image für mein Funkgerät vorbereiten…** wählen und das Manifest der geprüften vollständigen Sicherung des eigenen Geräts öffnen. Ein vollständiges Firmware-Update löscht das Einstellungsjournal. Nach dem Start unter **SYSTEM / TX → ÜBER FIRMWARE** nach **R58-SC3** sehen. [Vorbereitung, Installation und Suchlauf](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r58/docs/GETTING-STARTED.de.md)

Auf einem Funkgerät mit SC3 RU verschwanden die spürbaren Aussetzer der Bedienoberfläche. Der ausführbare Abschnitt von R58 stimmt für RU, EN und DE bytegenau mit der jeweiligen SC3-Fassung überein. R58 aktualisiert die Versionsanzeige und das Feld zur Personalisierung der Kalibrierung. Der AM-Empfang mit einem realen Signal ist noch zu prüfen.
