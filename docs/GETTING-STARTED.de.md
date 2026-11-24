# Zuerst sichern, dann installieren

> **PRE-ALPHA — experimentelle Firmware und CPS.** Das Projekt befindet sich in einem sehr frühen Stadium, noch vor einer Alpha-Version. Funktionen können ausfallen; die Installation kann das Funkgerät unbrauchbar machen. Installation und Nutzung erfolgen vollständig auf eigenes Risiko. Die Software wird im vorliegenden Zustand ohne Gewährleistung bereitgestellt; die Autoren übernehmen nach Maßgabe der Lizenz keine Haftung für Schäden oder Verluste. Vor einer Installation die Lizenz sowie die Anleitungen zur Sicherung und Wiederherstellung lesen.

[Projektstartseite](../README.de.md) · [English](GETTING-STARTED.en.md) · [Русский](GETTING-STARTED.ru.md)

Diese Anleitung beschreibt die erste Vorbereitung eines UV-98 Plus / der untersuchten RT-880-Plattform. **Die Originaldaten zunächst mit installierter Originalfirmware auslesen. Erst nach der Kompatibilitätsprüfung ein Firmware-Image wählen.** Die Sicherung ist auch ohne Installation der Rust-Firmware möglich.

R57 bietet getrennte BINs für RU, EN und DE. Ein vollständiges Firmware-Update löscht das Einstellungsjournal samt gespeicherter A/B-Position. [Versionsseite](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/tag/r57) · [Sprachtaste in R57 einrichten](../RELEASE-NOTES-R57.md#deutsch)

## 1. Funkgerät und Ordner vorbereiten

1. Akku laden. Das Programmierkabel mit Computer und Funkgerät verbinden.
2. Das Gerät **normal, ohne gedrückte PTT** einschalten. Die originale Kanal-/Frequenzanzeige muss erscheinen. Ein schwarzer Bildschirm mit grüner LED bedeutet Bootloader; dieser Modus eignet sich nicht zum Auslesen der Sicherung.
3. Mit der originalen CPS für das eigene Modell das Gerät auslesen und die ursprüngliche `.880dmf`- oder `.98pdmf`-Datei speichern. Eine vorhandene Originaldatei wie `999.880dmf` unverändert aufbewahren.
4. Original-CPS, Radio Tuning Software und andere Programme schließen, die das Kabel verwenden. Vor dem ersten Auslesen mit unserer CPS das Gerät für 3 Sekunden ausschalten und wieder normal starten.
5. Einen Ordner anlegen, etwa `Dokumente/UV98-backup-2026-11-16`. Sinnvolle Unterordner: `01-original`, `02-calibration`, `03-nor`, `04-settings`, `05-recovery`. Die ursprüngliche CPS-Datei in `01-original` ablegen.
6. [CPS 0.11.0 für Windows](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r57/UV98-CPS-0.11.0.zip) oder das [R57-Komplettpaket](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r57/R57-RT880-UV98.zip) entpacken. `libunwind.dll` neben `UV98-CPS.exe` belassen. CPS starten, unter **Язык / Language / Sprache** die Option **Deutsch** wählen und **Sicherungen** öffnen. Den tatsächlichen COM-Port über **Liste aktualisieren → Verfügbare Ports** wählen. Portnummern in älteren Screenshots und Protokollen können abweichen.

Die Sprachwahl ist stets in der Symbolleiste sichtbar. Alle acht Reiter und bereits angezeigte Statusmeldungen wechseln sofort die Sprache. Ungespeicherte Änderungen bleiben erhalten. CPS speichert die Sprachwahl im Windows-Benutzerprofil. Titel und Filter der Dateidialoge folgen der CPS-Sprache. Die Standardschaltflächen folgen der Windows-Sprache.

Modell, originale Firmwareversion und bekannte Platinenrevision in einer kurzen Textdatei bei den Sicherungen notieren.

## 2. Drei Lesevorgänge ausführen

Vor jedem Vorgang mit **Ordner für Abbilder…** den passenden Unterordner wählen. CPS legt darin ein neues Verzeichnis `UV98-read-…` an. Das gesamte Verzeichnis aufbewahren.

| Reihenfolge | CPS-Schaltfläche | Erwartete Dateien |
|---|---|---|
| 1 | **Kalibrierung lesen (4 KiB)** | `calibration.bin`, genau **4096 Byte**: die individuellen Werksdaten dieses Geräts. |
| 2 | **Einstellungsspeicher lesen (4 MiB)** | `storage-address-space.bin`, genau **4194304 Byte**, zusätzlich `calibration.bin`: der über das Protokoll lesbare externe NOR-Adressraum. |
| 3 | **Kanäle, Einstellungen und Kalibrierung lesen** | `calibration.bin`, `cfg.bin`, `channels.bin`, `zones.bin`, `fm.bin`: getrennte Datenbereiche. |

Der zweite Vorgang dauert am längsten. **Beide Durchläufe** abwarten. Während des Lesens das Funkgerät nicht bedienen und keine Einstellungen ändern.

Nach **jedem** Vorgang muss folgende Meldung erscheinen:

> Beide Lesedurchläufe stimmen überein. Abbild und Protokoll wurden gespeichert. Der Port ist geschlossen.

`manifest.json`, `started.json` und `pass-*.bin` zusammen mit den Datendateien aufbewahren. Eine erfolgreiche `manifest.json` enthält:

```json
"status": "verified_two_reads",
"two_reads_byte_identical": true
```

Eine vorhandene `.bin`-Datei allein beweist keine erfolgreiche Sicherung. Bei Fehler, Abbruch oder unterschiedlichen Durchläufen nicht zum Flashen übergehen. Den fehlgeschlagenen Mitschnitt behalten, die Verbindungsursache beheben und in einem neuen Ordner erneut lesen.

## 3. Sicherungen prüfen und aufbewahren

- Die Größen von `calibration.bin` und `storage-address-space.bin` mit der Tabelle vergleichen.
- Die SHA256-Werte von `calibration.bin` in den Manifesten des ersten und zweiten Vorgangs vergleichen. Der Werksblock muss übereinstimmen. Unterschiede zuerst untersuchen.
- Eine **passende originale Firmwaredatei** in `05-recovery` ablegen. Das öffentliche Rust-Paket enthält keine Herstellerfirmware. Für die untersuchte Platine die unten geprüfte Quelle für 1.16 verwenden; vor Verwendung mit einer anderen Revision die Kompatibilität klären.
- Den gesamten Sicherungsordner an einen zweiten Ort kopieren: anderes Laufwerk, USB-Speicher oder eigenes geschütztes Archiv. Persönliche Dumps nicht in öffentliche GitHub-Issues hochladen.

Folgendes aufbewahren:

| Gesicherter Bestandteil | Zweck |
|---|---|
| Ursprüngliche `.880dmf` / `.98pdmf` | Kanäle und Einstellungen im Format der Original-CPS. |
| Geprüfte `calibration.bin` | Individuelle Werkskalibrierung. |
| Geprüfte `storage-address-space.bin` | Externer NOR-Speicher innerhalb des lesbaren Adressraums. |
| Ordner mit Einzelsektionen und Protokollen | Vergleich, Diagnose und Vorbereitung einer bearbeiteten Einstellungskopie. |
| Passende Werks-BIN | Wiederherstellung der originalen MCU-Anwendung über den Bootloader. |

**Die 4 MiB NOR sind weder ein MCU-Dump noch eine Bootloader-Kopie.** Das bekannte Originalprotokoll bietet keinen Befehl zum Lesen dieser Bereiche. Das Speichern der Werks-BIN kopiert eine vorhandene Wiederherstellungsdatei; es liest nicht die Firmware aus Ihrem Gerät. Dieser Sicherungssatz ist daher kein vollständiger Byte-für-Byte-Klon aller Speicherchips.

Das Lesen der Sektionen **erstellt nicht automatisch eine neue `.880dmf`**. Dafür die eigene Originaldatei in CPS öffnen, die gelesenen Sektionen übernehmen und eine separate Kopie speichern. Die Sektion `Time` bleibt aus der Originaldatei erhalten; Kalibrierungen werden separat gespeichert. Zum reinen Sichern der Rohdaten ist dieser zusätzliche Schritt nicht nötig.

### Woher kommt die Wiederherstellungsdatei?

Das untersuchte Profil verwendet Originalversion 1.16: [Dateiseite im Archiv von nicsure](https://github.com/nicsure/RT880-FlasherX/blob/915545ae89f4a7547f6cea37408a5ba1d48e21f6/RT-880_Upgrade_V1_16_250915.bin) · [BIN herunterladen](https://raw.githubusercontent.com/nicsure/RT880-FlasherX/915545ae89f4a7547f6cea37408a5ba1d48e21f6/RT-880_Upgrade_V1_16_250915.bin). Erwartete Größe: **251904 Byte**. SHA256:

```text
eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc
```

Die Datei mit dem Namen `RT-880_Upgrade_V1_16_250915.bin` im Ordner `recovery` neben `UV98-CPS.exe` ablegen; den Ordner bei Bedarf anlegen. **Werks-BIN 1.16 speichern…** kann dann eine separate Kopie erzeugen; CPS prüft den SHA. Die Original-BIN kann auch direkt auf der Firmware-Seite ausgewählt werden. Keine andere Version nur auf diesen Dateinamen umbenennen.

## 4. Kompatibilität vor dem Flashen prüfen

R57 ist für das untersuchte AT32F423-Platinenprofil vorgesehen. Vor dem Flashen die Platinenrevision prüfen. Die Vorbereitung anhand der Sicherung bindet das Image an die eigenen Kalibrierungen.

Für eine bestätigte kompatible Platine eine persönliche BIN vorbereiten. **Ein Rust-Compiler wird dafür nicht benötigt.**

1. Die ursprüngliche R57-BIN für die gewünschte Sprache aus dem Ordner `firmware` des Komplettpakets wählen oder separat herunterladen. [RU](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r57/UV98-r57-ru.bin) · [EN](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r57/UV98-r57-en.bin) · [DE](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r57/UV98-r57-de.bin) Die Datei in CPS unter **Firmware → BIN auswählen…** öffnen. Für eine kompatible ursprüngliche Anwendungs-BIN mit Kalibrierprofil ist der Versionskatalog optional.
2. **Image für mein Funkgerät vorbereiten…** anklicken.
3. **`manifest.json` der vollständigen 4-MiB-NOR-Sicherung** auswählen, die mit „Einstellungsspeicher lesen (4 MiB)“ erstellt wurde. Das Manifest auswählen, nicht `storage-address-space.bin` selbst. Eine reine Kalibrierungssicherung reicht nicht aus.
4. Einen übergeordneten Ordner für neue Images wählen, etwa `Dokumente/UV98-personal`. CPS erstellt darin ein neues Verzeichnis `UV98-personal-…`.
5. Die erfolgreiche Vorbereitung abwarten. CPS schreibt `firmware/UV98-personal.bin`, `firmware/UV98-source.bin` und `firmware/catalog.json` in den neuen Ordner und wählt die persönliche BIN aus.
6. Den gesamten Ordner aufbewahren. Beim erneuten Öffnen von `UV98-personal.bin` müssen Quell-BIN und Katalog daneben liegen.

CPS prüft den Status der zwei Lesevorgänge, Größen und SHA256-Werte der Sicherungsdateien und gleicht den Kalibrierungsblock mit dem NOR-Anfang ab. Vor der Personalisierung prüft CPS Größe, Vektoren und Kalibrierprofil der Quell-BIN. Die neue BIN enthält den erwarteten **SHA256-Wert der eigenen Kalibrierung**. Beim Start vergleicht die Firmware diesen Hash mit der Kalibrierung im Gerät. CPS bewahrt Quell-BIN und Sicherung unverändert auf.

Der Katalog enthält den SHA256-Wert der persönlichen BIN sowie Personalisierungsdaten im Schema 2 mit den Hashes von Quelle und Sicherung. CPS prüft Aufbau und Integrität der Datei. Quelle der Version, Herausgeber und Platinenrevision separat bestätigen.

Wird die Vorbereitung abgelehnt, die Auswahl prüfen. Erforderlich sind die vollständige NOR-Sicherung des eigenen Geräts mit zwei übereinstimmenden Lesevorgängen und die ursprüngliche R57-BIN. Das Originalmanifest bei den Dateien der Sicherung aufbewahren.

Erst nach erfolgreicher Vorbereitung, bestätigter Platinenkompatibilität und Sicherung der geprüften Daten sowie der richtigen Werks-BIN an zwei Orten fortfahren.

## 5. Ein kompatibles Image installieren

1. In CPS 0.11.0 **Firmware → BIN auswählen…** öffnen.
2. Die vorbereitete `UV98-personal.bin` wählen. Namen und SHA256 auf dem Bildschirm prüfen. CPS prüft das Image anhand der mitgespeicherten Quell-BIN und des Katalogs.
3. Das Gerät ausschalten. Die **obere PTT** gedrückt halten, einschalten und PTT nach 2–3 Sekunden loslassen. Beim untersuchten Gerät zeigt der originale Bootloader einen schwarzen Bildschirm und eine grüne LED.
4. Den COM-Port des Kabels wählen. Die Bestätigung zur Bootloader-Bereitschaft markieren und **Firmware schreiben** einmal anklicken.
5. Auf die ausdrückliche Abschlussmeldung warten. Während des Schreibens weder ausschalten noch das Kabel abziehen oder CPS schließen.
6. Nach Abschluss für 3 Sekunden ausschalten und **ohne PTT** starten. Anzeige, Tasten und Empfang prüfen. Das Vorgangsprotokoll aufbewahren.

**Anzeigesprache.** Vor dem Flashen die BIN für die gewünschte Sprache wählen. Jede R57-BIN enthält eine Anzeigesprache. Die CPS-Sprache wird getrennt in der Symbolleiste gewählt.

In der Standardbelegung fordert die obere PTT eine Sendung über A an, die untere über B. Der grüne Rahmen markiert den Empfang, der rote TX-Rahmen hat Vorrang. Ein großes rotes **L**, gelbes **M** oder grünes **H** zeigt die eingestellte Leistung des sendenden VFOs, auch wenn B sendet und A ausgewählt ist. Nach dem Einschalten steht die Leistung auf LOW. Änderungen bleiben im RAM. Das Akkusymbol zeigt den ungefähren Stand mit bis zu vier Balken und die Spannung in Schritten von 0,1 V (`--.-V`, wenn kein Messwert verfügbar ist). [Nachweise zur unteren PTT](../research/r37/ptt.md)

Bootloader-Bestätigungen belegen die Dateiübertragung. Start, Empfang, beide PTT-Tasten bei kurzem Druck, Relaisantwort und Zonenscan am Funkgerät prüfen.

### CB-Kanalraster

1. **MENU → KANÄLE / VFO → CB-RASTER** öffnen. Beim ersten Öffnen nach dem Einschalten ist **Kanal 15 AM auf 27,135 MHz** im CEPT-Raster mit 40 Kanälen ausgewählt.
2. Mit ↑/↓ einen Kanal wählen oder die Nummer **1–40** eingeben. Mit MENU VFO A abstimmen. Mit `#` zwischen 5er- und 0er-Raster wechseln. Kanal 15 im 0er-Raster liegt auf **27,130 MHz**. Mit `*` zwischen AM und FM wechseln.
3. Für einen Direktaufruf **MENU → TASTEN → LANGDRUCK** öffnen, die gewünschte Ziffer drücken, **CB-RASTER** wählen und mit MENU bestätigen. Auf der Hauptanzeige die zugewiesene Ziffer gedrückt halten.
4. Im Raster EXIT gedrückt halten, um CB zu verlassen und die vorherige Einstellung von VFO A wiederherzustellen. VFO B arbeitet unabhängig. Die letzte bestätigte CB-Auswahl bleibt bis zum Ausschalten verfügbar.

**Experimentelles CB-AM.** Im CB-Raster von VFO A empfängt AM auf der nominellen Kanalfrequenz. Beim Drücken der PTT sendet das Gerät in Schmalband-FM 2 kHz höher. Auf Kanal 15 gilt im CEPT-Raster **RX 27,135 → TX 27,137 MHz** und im 0er-Raster **RX 27,130 → TX 27,132 MHz**. Bei CB-FM liegen Empfang und Sendung auf der nominellen Kanalfrequenz. VFO B arbeitet mit seinen eigenen Einstellungen. Die übrigen AM-Modi sind auf Empfang beschränkt. Für beide CB-Sendemodi gelten die vorhandenen TX-Prüfungen.

Ein AM-Empfänger kann das versetzte FM-Signal an der Flanke seines Filters demodulieren. Der Empfang hängt von der Filtercharakteristik und dem FM-Hub ab. Die Kompatibilität muss noch an Funkgeräten geprüft werden.

### Kurzwellen-AM

Mit langem Druck auf `1` zwischen den Filtern mit 6, 3 und 1 kHz wechseln. Mit langem Druck auf `2` Soft Mute umschalten. Ab R54 schaltet langes Drücken von `3` in AM den 100-Hz-Hochpass unabhängig davon. Beim normalen Hören bleibt dabei die aktuelle Si4732-Frequenz erhalten.

### DTMF-Empfang

Beim normalen BK4819-FM-Empfang mit geöffnetem Audioweg öffnet sich das Fenster auf der Hauptanzeige automatisch. Es zeigt die letzten 16 Zeichen bis drei Sekunden nach dem letzten Zeichen an. Beim Wechsel des Empfangsmodus oder beim Sendebeginn schließt sich das Fenster. In AM, Spektrum, Kurzwelle und Crossband bleibt der Decoder aus. Select V, ZVEI und CCIR wurden entfernt.

### Numerisches MDC1200

1. Auf der Hauptanzeige A oder B wählen. Zum Umschalten zwischen den VFOs `*` gedrückt halten. **MENU → GPS / SIGNALE → MDC1200** öffnen und den VFO-Buchstaben in der Kopfzeile prüfen.
2. Unter **MDC ZEITPUNKT** **AUS**, **TX-ANFANG**, **TX-ENDE** oder **TX-ANFANG UND -ENDE** wählen. Die Voreinstellung ist **AUS**. Mit MENU bestätigen.
3. Unter **UNIT ID** vier Hexadezimalzeichen von `0001` bis `DEEE` eingeben. Für Ziffern 0–9 kurz drücken und für A–F die Tasten 1–6 gedrückt halten. Mit `*` löschen, mit MENU übernehmen und mit EXIT abbrechen. Zum Löschen der ID ein leeres Feld bestätigen.

Zeitpunkt und ID des numerischen MDC liegen bis zum Ausschalten im RAM.

### Funkgerätename nach PTT

1. **MENU → SYSTEM / TX → FUNKGERÄTENAME** öffnen. Bis zu 16 lateinische Großbuchstaben, Ziffern oder Leerzeichen eingeben. Eine Ziffer wiederholt kurz drücken, um die Zeichen zu wechseln. Mit `2` beispielsweise A, B, C und 2 durchgehen. Kurz `0` für ein Leerzeichen drücken. Gedrückt halten, um die Ziffer selbst einzugeben.
2. Mit ↑ das aktuelle Zeichen übernehmen oder rund 650 ms warten, bevor dieselbe Taste ein weiteres Zeichen eingibt. `*` löscht das letzte Zeichen. Mit **MENU/OK** speichern oder mit EXIT abbrechen.
3. Unter **SYSTEM / TX → NAME NACH PTT** **EIN** oder **AUS** wählen und mit **MENU/OK** bestätigen. Bleibt die Speicherbestätigung aus, den angezeigten Wert prüfen und erneut speichern.

Name und Schalter teilen sich ein internes Flash-Journal. Beide Werte nach dem Aus- und Einschalten prüfen. Ein vollständiges Firmware-Update löscht das Journal. Bei leerem Journal lädt die Firmware gültige Einstellungen für Name und Roger aus dem originalen NOR. [Simulierte Namenseingabe in R42](images/r42/de-radio-name.png)

Bei aktiver Option sendet das Funkgerät den Namen nach PTT im Roger-Format der Originalfirmware RT880 1.16. Die numerische MDC-PTT-ID hat eigene Einstellungen. Sind beide Funktionen aktiv, belegt der Name das Sendeende. Am Sendeanfang kann weiterhin die numerische MDC-ID gesendet werden. Gerätetests der Funkkompatibilität und der Speicherung stehen aus. [R42-Hinweise (Russisch)](../R42-NOTES-RU.txt)

### Separater PA5-Audiotest

Die RX-only-Diagnosefirmware zeichnet während des Empfangs einen kurzen Block von PA5-ADC-Abtastwerten zur Auswertung am PC auf. [PA5-Testanleitung (Russisch)](../research/r54/PA5-TEST-RU.txt)

## 6. Bei Bedarf zur Originalfirmware zurückkehren

Für die Wiederherstellung mit diesem CPS ist die kompatible **Werks-BIN 1.16 mit der oben angegebenen SHA256** erforderlich. Bei einer anderen Werksversion oder Hardwarerevision vorab das Verfahren des Herstellers und die passende Datei prüfen. Ist das Funkgerät mit der geprüften Version 1.16 kompatibel, andere Programme schließen, den Bootloader mit der oberen PTT starten, diese BIN wählen und wie zuvor schreiben. Nach Abschluss das Gerät normal einschalten.

Beim untersuchten Gerät überschreibt das Schreiben der MCU-Anwendung nicht die externen Kanäle und Kalibrierungen. **Eine Neuinstallation der Originalfirmware repariert keinen bereits beschädigten NOR-Speicher.** Unsere CPS schreibt derzeit weder einen rohen 4-MiB-Dump noch einen Kalibrierungsblock zurück ins Funkgerät. Dafür ist ein gesonderter Wiederherstellungsplan mit den eigenen gesicherten Dateien erforderlich; diese niemals als MCU-Firmwaredatei auswählen.

| Problem | Nächster Schritt |
|---|---|
| COM-Port belegt oder nicht verfügbar | Alle CPS-Programme schließen. USB-Stecker des Kabels neu verbinden, Liste aktualisieren und den tatsächlichen Port wählen. |
| Lesen fehlgeschlagen | Normalen Start der Originalfirmware prüfen. In einen neuen Ordner erneut lesen; vorherigen Mitschnitt behalten. |
| Schreiben unterbrochen | Protokoll behalten. Bootloader erneut starten und die passende vollständige BIN schreiben. Nach Beginn des Löschens kann der normale Start bis zum erfolgreichen Schreiben ausfallen. |
| Neues Image startet nicht | Bekannte passende Originalfirmware über den Loader wiederherstellen. NOR nicht löschen und Kalibrierungen nicht wahllos verändern. |
| Bootloader erscheint nicht | Anhalten und Modell, Version, Schritte und Protokoll melden. Keine beliebigen BIN-Dateien ausprobieren. |
