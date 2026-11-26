# R58-SC5 PRE-ALPHA · RT-880 / UV-98 Plus · CPS 0.11.0

[Полный комплект / Complete kit / Komplettpaket](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58-sc5/R58-SC5-RT880-UV98.zip) · [Windows CPS 0.11.0](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58-sc5/UV98-CPS-0.11.0.zip)

BIN [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58-sc5/UV98-r58-sc5-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58-sc5/UV98-r58-sc5-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r58-sc5/UV98-r58-sc5-de.bin)

## Русский

В спектре короткое **#** переключает **АВТОЗВУК → АВТООБЗОР → RX+ОБЗОР**. По умолчанию включён **АВТОЗВУК**. Пока рация слушает несущую в этом режиме, её столбец обновляется, соседние столбцы сохраняют историю.

- В **RX+ОБЗОР** после секунды автоматического прослушивания рация заново измеряет всю полосу. Соседние столбцы получают свежие измерения. Если прежний сигнал ещё выше порога, рация возвращается к нему.
- Во время обхода звук прерывается. Длительность паузы в эфире ещё предстоит измерить. Проверить обход можно по счётчику **ПРОХОД** во время долгого сигнала.
- Стрелка остаётся на выбранной вручную частоте. Крупные цифры **RX** показывают частоту, которую слышно. Кнопка **6** включает ручное прослушивание частоты курсора, **0** возвращает к обзору.

**Установка.** Проверьте совместимость ревизии платы AT32F423. В CPS 0.11.0 откройте BIN нужного языка, нажмите **«Подготовить образ для моей рации…»** и выберите `manifest.json` полного снимка NOR своей станции с двумя совпавшими чтениями. Запишите подготовленный персональный образ. Полная запись сбрасывает внутренние настройки, включая сохранённую позицию A/B. После запуска в меню **СИСТЕМА / TX → О ПРОШИВКЕ** появится **R58-SC5**. [Пошаговая инструкция](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r58-sc5/docs/GETTING-STARTED.ru.md)

## English

In the spectrum, tap **#** to cycle through **AUTO AUDIO → AUTO SWEEP → RX+SWEEP**. The default is **AUTO AUDIO**. While listening in this mode, the radio updates the active signal’s column and keeps the history in the neighbouring columns.

- In **RX+SWEEP**, after one second of automatic listening the radio measures the entire band again. Neighbouring columns show new measurements. If the previous signal is still above threshold, the radio returns to it.
- Audio pauses during each sweep. The duration of the gap still needs an on-air measurement. During a sustained signal, watch the sweep counter advance.
- The cursor stays where you placed it. The large **RX** readout shows the frequency you hear. Press **6** to listen manually at the cursor or **0** to resume sweeping.

**Installation.** Check your AT32F423 board revision for compatibility. In CPS 0.11.0, open the BIN for your language, choose **Prepare an image for my radio…** and select `manifest.json` from your radio’s complete NOR backup with two matching reads. Flash the prepared personal image. A full write clears internal settings, including the saved A/B position. After startup, **SYSTEM / TX → ABOUT FIRMWARE** shows **R58-SC5**. [Installation guide](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r58-sc5/docs/GETTING-STARTED.en.md)

## Deutsch

In der Spektrumanzeige schaltet ein kurzer Druck auf **#** zwischen **AUTO AUDIO → AUTO SCAN → RX+SCAN** um. Voreingestellt ist **AUTO AUDIO**. Beim Abhören in diesem Modus wird die Spalte des empfangenen Signals aktualisiert. Die Nachbarspalten behalten ihren Verlauf.

- In **RX+SCAN** misst das Gerät nach einer Sekunde automatischen Abhörens den gesamten Bereich erneut. Die Nachbarspalten zeigen neue Messwerte. Liegt das bisherige Signal noch über der Schwelle, kehrt das Gerät dorthin zurück.
- Bei jedem Durchlauf setzt der Ton aus. Die Dauer dieser Pause muss im Funkbetrieb noch gemessen werden. Bei einem anhaltenden Signal lässt sich der Fortschritt am Durchlaufzähler ablesen.
- Der Cursor bleibt auf der manuell gewählten Frequenz. Die große **RX**-Anzeige zeigt die Frequenz, die gerade zu hören ist. Mit **6** die Cursorfrequenz manuell abhören, mit **0** zum Durchlauf zurückkehren.

**Installation.** Die AT32F423-Platinenrevision auf Kompatibilität prüfen. In CPS 0.11.0 die BIN für die gewünschte Sprache öffnen, **Image für mein Funkgerät vorbereiten…** wählen und die Datei `manifest.json` aus der vollständigen NOR-Sicherung des eigenen Geräts mit zwei übereinstimmenden Lesevorgängen öffnen. Das vorbereitete persönliche Image flashen. Ein vollständiges Update löscht die internen Einstellungen einschließlich der gespeicherten A/B-Position. Nach dem Start steht unter **SYSTEM / TX → ÜBER FIRMWARE** die Version **R58-SC5**. [Installationsanleitung](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/blob/r58-sc5/docs/GETTING-STARTED.de.md)
