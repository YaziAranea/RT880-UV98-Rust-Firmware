# Zuerst sichern, dann installieren

> **PRE-ALPHA — experimentelle Firmware und CPS.** Das Projekt befindet sich in einem sehr frühen Stadium, noch vor einer Alpha-Version. Funktionen können ausfallen; die Installation kann das Funkgerät unbrauchbar machen. Installation und Nutzung erfolgen vollständig auf eigenes Risiko. Die Software wird im vorliegenden Zustand ohne Gewährleistung bereitgestellt; die Autoren übernehmen nach Maßgabe der Lizenz keine Haftung für Schäden oder Verluste. Vor einer Installation die Lizenz sowie die Anleitungen zur Sicherung und Wiederherstellung lesen.

[Projektstartseite](../README.de.md) · [English](GETTING-STARTED.en.md) · [Русский](GETTING-STARTED.ru.md)

Diese Anleitung beschreibt die erste Vorbereitung eines UV-98 Plus / der untersuchten RT-880-Plattform. **Die Originaldaten zunächst mit installierter Originalfirmware auslesen. Erst nach der Kompatibilitätsprüfung ein Firmware-Image wählen.** Die Sicherung ist auch ohne Installation der Rust-Firmware möglich.

## 1. Funkgerät und Ordner vorbereiten

1. Akku laden. Das Programmierkabel mit Computer und Funkgerät verbinden.
2. Das Gerät **normal, ohne gedrückte PTT** einschalten. Die originale Kanal-/Frequenzanzeige muss erscheinen. Ein schwarzer Bildschirm mit grüner LED bedeutet Bootloader; dieser Modus eignet sich nicht zum Auslesen der Sicherung.
3. Mit der originalen CPS für das eigene Modell das Gerät auslesen und die ursprüngliche `.880dmf`- oder `.98pdmf`-Datei speichern. Eine vorhandene Originaldatei wie `999.880dmf` unverändert aufbewahren.
4. Original-CPS, Radio Tuning Software und andere Programme schließen, die das Kabel verwenden. Vor dem ersten Auslesen mit unserer CPS das Gerät für 3 Sekunden ausschalten und wieder normal starten.
5. Einen Ordner anlegen, etwa `Dokumente/UV98-backup-2026-11-16`. Sinnvolle Unterordner: `01-original`, `02-calibration`, `03-nor`, `04-settings`, `05-recovery`. Die ursprüngliche CPS-Datei in `01-original` ablegen.
6. Das [R39-Paket mit CPS 0.7.0](https://github.com/YaziAranea/RT880-UV98-Rust-Firmware/releases/download/r39/UV98-r39-2026-11-16.zip) vollständig entpacken. `UV98-CPS.exe` starten, unter **Язык / Language / Sprache** die Option **Deutsch** wählen und **Sicherungen** öffnen. Den tatsächlichen COM-Port über **Liste aktualisieren → Verfügbare Ports** wählen. Frühere Angaben wie COM20/COM21 sind keine Vorgabe für Ihren Rechner.

Die Sprachwahl ist stets in der Symbolleiste sichtbar. Alle acht Reiter und bereits angezeigte Statusmeldungen wechseln sofort die Sprache. Ungespeicherte Änderungen bleiben erhalten. CPS speichert die Sprachwahl im Windows-Benutzerprofil. Titel und Filter der Dateidialoge folgen der CPS-Sprache. Die Standardschaltflächen folgen der Windows-Sprache. Das R39-Paket enthält die veröffentlichte CPS 0.7.0.

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

R39 ist für das untersuchte AT32F423-Platinenprofil vorgesehen. Vor dem Flashen die Platinenrevision prüfen. Die Vorbereitung anhand der Sicherung bindet das Image an die eigenen Kalibrierungen.

Für eine bestätigte kompatible Platine eine persönliche BIN vorbereiten. **Ein Rust-Compiler wird dafür nicht benötigt.**

1. **Firmware** in unserer CPS öffnen. Die ursprüngliche `UV98-r39.bin` aus dem aktuellen Paket auswählen.
2. **Image für mein Funkgerät vorbereiten…** anklicken.
3. **`manifest.json` der vollständigen 4-MiB-NOR-Sicherung** auswählen, die mit „Einstellungsspeicher lesen (4 MiB)“ erstellt wurde. Das Manifest auswählen, nicht `storage-address-space.bin` selbst. Eine reine Kalibrierungssicherung reicht nicht aus.
4. Einen übergeordneten Ordner für neue Images wählen, etwa `Dokumente/UV98-personal`. CPS erstellt darin ein neues Verzeichnis `UV98-personal-…`.
5. Die erfolgreiche Vorbereitung abwarten. Es entstehen `firmware/UV98-personal.bin` und `firmware/catalog.json`; CPS wählt die neue BIN automatisch zum Flashen aus.
6. Den gesamten neuen Ordner aufbewahren. Beim späteren Öffnen der CPS die `UV98-personal.bin` auswählen und ihren benachbarten Katalog beibehalten.

CPS prüft den Status der zwei Lesevorgänge, Größen und SHA-Werte der Sicherungsdateien, die Übereinstimmung des Kalibrierungsblocks mit dem NOR-Anfang sowie die Originalvorlage anhand ihres Versionskatalogs und der Kalibrierungsprofil-Markierung. In die neue BIN wird nur der erwartete **SHA der eigenen Kalibrierungen** eingetragen, nicht die Kalibrierungsdaten selbst. Originalimage und Sicherung bleiben unverändert. Beim Start vergleicht die Firmware diesen Hash mit den tatsächlichen Kalibrierungen im Gerät.

Die persönliche BIN hat einen eigenen SHA im neuen CPS-Katalog. BIN und Katalog zusammen aufbewahren und die Sicherung des eigenen Geräts verwenden.

Wird die Vorbereitung abgelehnt, Auswahl prüfen. Erforderlich sind die vollständige und erfolgreich verglichene NOR-Sicherung des eigenen Geräts sowie die ursprüngliche R39-BIN aus demselben Paket. Für die Prüfung das Originalmanifest und den Originalkatalog verwenden.

Erst nach erfolgreicher Vorbereitung, bestätigter Platinenkompatibilität und Sicherung der geprüften Daten sowie der richtigen Werks-BIN an zwei Orten fortfahren.

## 5. Ein kompatibles Image installieren

1. Das gesamte Paket entpacken. CPS prüft BIN-Dateien anhand von `firmware/catalog.json`; eine einzelne Datei neben einer beliebigen alten CPS reicht nicht.
2. **Firmware** in CPS öffnen und die vorbereitete `UV98-personal.bin` wählen. Namen und SHA256 prüfen; CPS vergleicht sie mit dem neuen persönlichen Katalog.
3. Das Gerät ausschalten. Die **obere PTT** gedrückt halten, einschalten und PTT nach 2–3 Sekunden loslassen. Beim untersuchten Gerät zeigt der originale Bootloader einen schwarzen Bildschirm und eine grüne LED.
4. Den COM-Port des Kabels wählen. Die Bestätigung zur Bootloader-Bereitschaft markieren und **Firmware schreiben** einmal anklicken.
5. Auf die ausdrückliche Abschlussmeldung warten. Während des Schreibens weder ausschalten noch das Kabel abziehen oder CPS schließen.
6. Nach Abschluss für 3 Sekunden ausschalten und **ohne PTT** starten. Anzeige, Tasten und Empfang prüfen. Das Vorgangsprotokoll aufbewahren.

**Anzeigesprache wählen:** MENU → СИСТЕМА / TX → ЯЗЫК / LANGUAGE → DEUTSCH. Mit ↑/↓ wählen, mit MENU bestätigen, mit EXIT abbrechen. Zur Auswahl stehen РУССКИЙ, ENGLISH und DEUTSCH. Auf Deutsch lautet der Pfad MENU → SYSTEM / TX → SPRACHE. Nach dem Ausschalten wird derzeit wieder Russisch verwendet.

In der Standardbelegung fordert die obere PTT eine Sendung über A an, die untere über B. Das tatsächlich sendende Feld zeigt TX und einen roten Rahmen, unabhängig vom ausgewählten VFO. Die Leistung liegt je VFO im RAM und steht nach dem Einschalten auf LOW. Das Akkusymbol zeigt den ungefähren Stand mit bis zu vier Balken und die Spannung in Schritten von 0,1 V (`--.-V`, wenn kein Messwert verfügbar ist). [Nachweise zur unteren PTT](../research/r37/ptt.md)

Bootloader-Bestätigungen belegen die Dateiübertragung. Start und Funkbetrieb am Gerät prüfen. R33 ist die vom Besitzer akzeptierte Ausgangsversion. Für R39 stehen Gerätetests der TX-Anzeige, der unteren PTT, der Akkuanzeige, des CB-/KW-Betriebs, des Empfangs des versetzten FM-Signals und der MDC1200-Dekodierung aus. [R39-Nachweise und Prüfgrenzen](../research/r39/EVIDENCE-RU.md)

### CB-Kanalraster

1. **MENU → KANÄLE / VFO → CB-RASTER** öffnen. Beim ersten Öffnen nach dem Einschalten ist **Kanal 15 AM auf 27,135 MHz** im CEPT-Raster mit 40 Kanälen ausgewählt.
2. Mit ↑/↓ einen Kanal wählen oder die Nummer **1–40** eingeben. Mit MENU VFO A abstimmen. Mit `#` zwischen 5er- und 0er-Raster wechseln. Kanal 15 im 0er-Raster liegt auf **27,130 MHz**. Mit `*` zwischen AM und FM wechseln.
3. Für einen Direktaufruf **MENU → TASTEN → LANGDRUCK** öffnen, die gewünschte Ziffer drücken, **CB-RASTER** wählen und mit MENU bestätigen. Auf der Hauptanzeige die zugewiesene Ziffer gedrückt halten.
4. Im Raster EXIT gedrückt halten, um CB zu verlassen und die vorherige Einstellung von VFO A wiederherzustellen. VFO B arbeitet unabhängig. Die letzte bestätigte CB-Auswahl bleibt bis zum Ausschalten verfügbar.

**Experimentelles CB-AM.** Im CB-Raster von VFO A empfängt AM auf der nominellen Kanalfrequenz. Beim Drücken der PTT sendet das Gerät in Schmalband-FM 2 kHz höher. Auf Kanal 15 gilt im CEPT-Raster **RX 27,135 → TX 27,137 MHz** und im 0er-Raster **RX 27,130 → TX 27,132 MHz**. Bei CB-FM liegen Empfang und Sendung auf der nominellen Kanalfrequenz. VFO B arbeitet mit seinen eigenen Einstellungen. Die übrigen AM-Modi sind auf Empfang beschränkt. Für beide CB-Sendemodi gelten die vorhandenen TX-Prüfungen.

Ein AM-Empfänger kann das versetzte FM-Signal an der Flanke seines Filters demodulieren. Der Empfang hängt von der Filtercharakteristik und dem FM-Hub ab. Die Kompatibilität muss noch an Funkgeräten geprüft werden.

### MDC1200 und lokaler Name

1. Auf der Hauptanzeige A oder B wählen. Zum Umschalten zwischen den VFOs `*` gedrückt halten. **MENU → GPS / SIGNALE → MDC1200** öffnen und den VFO-Buchstaben in der Kopfzeile prüfen.
2. Unter **MDC ZEITPUNKT** **AUS**, **TX-ANFANG**, **TX-ENDE** oder **TX-ANFANG UND -ENDE** wählen. Die Voreinstellung ist **AUS**. Mit MENU bestätigen.
3. Unter **UNIT ID** vier Hexadezimalzeichen von `0001` bis `DEEE` eingeben. Für Ziffern 0–9 kurz drücken und für A–F die Tasten 1–6 gedrückt halten. Mit `*` löschen, mit MENU übernehmen und mit EXIT abbrechen. Zum Löschen der ID ein leeres Feld bestätigen.
4. Unter **NAME (LOKAL)** eine Ziffer wiederholt drücken, um ihre Zeichen durchzugehen. Vor einem weiteren Zeichen auf derselben Taste ↑ drücken. Mit `#` RU/EN umschalten und mit `*` löschen. MENU speichert bis zu zwölf Zeichen. EXIT bricht die Bearbeitung ab.

Das MDC1200-Paket überträgt die PTT-ID. Der Name dient als Beschriftung auf diesem Gerät. Zeitpunkt, ID, Name und weitere Firmware-Einstellungen liegen bis zum Ausschalten im RAM. [Simulierte Namenseingabe](images/r39/de-mdc-alias-en.png)

## 6. Bei Bedarf zur Originalfirmware zurückkehren

Andere Programme schließen. Den originalen Bootloader erneut mit der oberen PTT starten und in unserer CPS die **passende Werks-BIN** wählen. Mit demselben Ablauf schreiben und nach Abschluss normal einschalten.

Beim untersuchten Gerät überschreibt das Schreiben der MCU-Anwendung nicht die externen Kanäle und Kalibrierungen. **Eine Neuinstallation der Originalfirmware repariert keinen bereits beschädigten NOR-Speicher.** Unsere CPS schreibt derzeit weder einen rohen 4-MiB-Dump noch einen Kalibrierungsblock zurück ins Funkgerät. Dafür ist ein gesonderter Wiederherstellungsplan mit den eigenen gesicherten Dateien erforderlich; diese niemals als MCU-Firmwaredatei auswählen.

| Problem | Nächster Schritt |
|---|---|
| COM-Port belegt oder nicht verfügbar | Alle CPS-Programme schließen. USB-Stecker des Kabels neu verbinden, Liste aktualisieren und den tatsächlichen Port wählen. |
| Lesen fehlgeschlagen | Normalen Start der Originalfirmware prüfen. In einen neuen Ordner erneut lesen; vorherigen Mitschnitt behalten. |
| Schreiben unterbrochen | Protokoll behalten. Bootloader erneut starten und die passende vollständige BIN schreiben. Nach Beginn des Löschens kann der normale Start bis zum erfolgreichen Schreiben ausfallen. |
| Neues Image startet nicht | Bekannte passende Originalfirmware über den Loader wiederherstellen. NOR nicht löschen und Kalibrierungen nicht wahllos verändern. |
| Bootloader erscheint nicht | Anhalten und Modell, Version, Schritte und Protokoll melden. Keine beliebigen BIN-Dateien ausprobieren. |
