# R57-VK · RT-880 / UV-98 Plus · Windows CPS 0.11.0

[**Комплект / Complete kit / Komplettpaket**](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r57/R57-RT880-UV98.zip) · [**Windows CPS**](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r57/UV98-CPS-0.11.0.zip)

## Русский

Репитеру может понадобиться время на открытие. В R57 станция включает передачу на активной строке A/B, ждёт заданное время и проигрывает короткую запись. Например, при 500 мс фраза начнётся не раньше чем через полсекунды после включения TX. Если включена начальная сигнализация, фраза дождётся и её завершения. Отпускание цифры во время ожидания отменяет фразу. Обе обычные PTT работают с микрофоном.

1. Распакуйте комплект и откройте **UV98-CPS.exe → Прошивка**. Выберите исходный BIN R57 нужного языка.
2. Выберите цифру для долгого нажатия (0–9) и введите задержку от 0 до 3000 мс с точностью до 1 мс. Исходные значения — «Не назначена» и 0 мс. Можно оставить встроенную фразу «Проверка связи» либо выбрать MP3/WAV, выделить и прослушать фрагмент до 1,5 с.
3. Сохраните настройки и запись в новый BIN. Выберите **«Подготовить образ для моей рации…»**, укажите manifest.json своего проверенного полного снимка и прошейте подготовленный образ. Назначение уже действует после загрузки. Позже его можно изменить или отключить в меню станции **«Долгие клавиши»**.
4. На главном экране FM с выключенным сканированием и двойным прослушиванием удерживайте выбранную цифру. Она передаёт фразу один раз. Для эфирной проверки послушайте, вернётся ли начало и конец фразы в ответе эхо-репитера.

## English

Give the repeater time to open before speech. Holding the assigned digit keys the active FM slot. The recording starts once the configured delay from TX has elapsed and any beginning-of-transmission signalling has finished. Releasing the digit during the wait cancels the recording. Both regular PTT buttons retain live microphone operation.

1. Extract the kit. In **UV98-CPS.exe → Firmware**, choose the original R57 BIN for your display language.
2. Select a long-press digit (0–9) and enter a delay from 0 to 3000 ms in 1 ms steps. The image defaults to **Unassigned** and **0 ms**. Use the built-in test phrase or import an MP3/WAV file, select an excerpt up to 1.5 s and preview it.
3. Save to a new BIN. Choose **Prepare an image for my radio…**, select the manifest.json from your own verified full backup, then flash the prepared image. The chosen digit works on first boot. Its assignment can also be changed or cleared on the radio under **Long-press keys**.
4. From the FM home screen, with scan and dual watch off, hold the digit to send the clip once. An echo repeater lets you check that the complete phrase returns.

## Deutsch

Damit das Relais vor der Durchsage öffnen kann, lässt sich der Sprachbeginn verzögern. Die zugewiesene Ziffer schaltet den aktiven FM-Sendeplatz ein. Die Aufnahme beginnt nach Ablauf der Verzögerung ab Sendebeginn und nach Abschluss einer eingestellten Anfangssignalisierung. Loslassen während der Wartezeit bricht die Durchsage ab. Beide regulären PTT-Tasten schalten weiterhin das Mikrofon ein.

1. Das Paket entpacken und unter **UV98-CPS.exe → Firmware** die ursprüngliche R57-BIN für die gewünschte Anzeigesprache auswählen.
2. Eine Ziffer für den Langdruck (0–9) auswählen und die Verzögerung von 0 bis 3000 ms in Schritten von 1 ms eingeben. Voreinstellung: **Nicht zugewiesen**, **0 ms**. Den eingebauten Testspruch verwenden oder eine MP3-/WAV-Datei öffnen, einen Ausschnitt bis 1,5 s auswählen und vorhören.
3. Als neue BIN speichern. Das Image mit der manifest.json der eigenen geprüften vollständigen Sicherung für das Funkgerät vorbereiten und anschließend flashen. Die Ziffer ist beim ersten Start belegt. Die Zuweisung kann am Gerät im Menü **Langdruck** geändert oder gelöscht werden.
4. Auf der FM-Hauptanzeige bei ausgeschaltetem Suchlauf und Dual Watch die Ziffer gedrückt halten. Die Aufnahme wird einmal gesendet. Mit einem Echorelais lässt sich prüfen, ob die gesamte Durchsage zurückkommt.

**Проверка / Verification / Prüfung.** Software and linked Cortex-M4 checks cover configuration, clip playback and cancellation. On-air timing and intelligibility on physical R57 hardware still require measurement.
