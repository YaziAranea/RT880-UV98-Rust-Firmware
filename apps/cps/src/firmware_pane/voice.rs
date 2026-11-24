use super::*;
use crate::firmware::voice::{audio_error, load, save};
use uv98_voice_clip::{wav::Recording, VoiceConfig, MAX_DELAY_MS, MAX_FRAMES, RATE, SLOT_BYTES};

struct Selection {
    name: String,
    recording: Recording,
    start_ms: u32,
}

#[derive(Default)]
pub(super) struct VoiceImport {
    config: Option<VoiceConfig>,
    loading: Option<Receiver<Result<Selection, LocalizedText>>>,
    selection: Option<Selection>,
    #[cfg(windows)]
    playback: Option<Receiver<bool>>,
}

impl VoiceImport {
    pub(super) fn for_image(image: &PreparedImage) -> Self {
        Self {
            config: image.voice_config(),
            ..Self::default()
        }
    }

    pub(super) fn config_changed(&self, image: &PreparedImage) -> bool {
        self.config != image.voice_config()
    }

    pub(super) fn pending(&self) -> bool {
        self.loading.is_some() || self.selection.is_some()
    }
}

impl FirmwarePane {
    pub(super) fn poll_voice(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.voice.loading {
            match receiver.try_recv() {
                Ok(Ok(selection)) => {
                    self.voice.selection = Some(selection);
                    self.voice.loading = None;
                    self.error = false;
                    self.message = msg!(
                        "Запись открыта. Прослушайте или сохраните выбранный фрагмент.",
                        "Recording opened. Preview or save the selected excerpt.",
                        "Aufnahme geöffnet. Den gewählten Ausschnitt anhören oder speichern."
                    );
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(50)),
                result => {
                    self.voice.loading = None;
                    self.error = true;
                    self.message = match result {
                        Ok(Err(e)) => e,
                        _ => msg!(
                            "Импорт звука прерван.",
                            "Audio import interrupted.",
                            "Audioimport abgebrochen."
                        ),
                    };
                }
            }
        }
        #[cfg(windows)]
        if let Some(receiver) = &self.voice.playback {
            match receiver.try_recv() {
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(80)),
                result => {
                    self.voice.playback = None;
                    if !matches!(result, Ok(true)) {
                        self.error = true;
                        self.message = msg!(
                            "Не удалось воспроизвести через аудиовыход Windows.",
                            "Windows audio playback failed.",
                            "Wiedergabe über den Windows-Audioausgang fehlgeschlagen."
                        );
                    }
                }
            }
        }
    }

    fn discard_voice_edits(&mut self) {
        self.voice = self
            .image
            .as_ref()
            .map_or_else(VoiceImport::default, VoiceImport::for_image);
        self.armed = false;
        self.error = false;
        self.message = msg!(
            "Голосовые настройки и запись восстановлены из выбранного BIN.",
            "Voice settings and recording restored from the selected BIN.",
            "Spracheinstellungen und Aufnahme aus der gewählten BIN wiederhergestellt."
        );
    }

    fn choose_recording(&mut self) {
        let lang = self.language;
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!(
                lang,
                "Выберите голосовую запись",
                "Choose a voice recording",
                "Sprachaufnahme auswählen"
            ))
            .add_filter("MP3 / PCM WAV", &["mp3", "wav"])
            .pick_file()
        else {
            return;
        };
        self.armed = false;
        let (send, receive) = mpsc::channel();
        self.voice.loading = Some(receive);
        thread::spawn(move || {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let _ = send.send(load(&path).map(|recording| Selection {
                name,
                recording,
                start_ms: 0,
            }));
        });
    }

    pub(super) fn voice_controls(&mut self, ui: &mut egui::Ui) {
        let lang = self.language;
        let Some(image) = &self.image else {
            return;
        };
        let editable = image.can_add_voice();
        let saved_config = image.voice_config();
        if let Some(config) = &mut self.voice.config {
            let unassigned = tr!(lang, "Не назначена", "Unassigned", "Nicht zugewiesen");
            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let label = tr!(
                        lang,
                        "Долгое нажатие для передачи фразы",
                        "Hold to transmit the voice clip",
                        "Sprachaufnahme durch langes Drücken senden"
                    );
                    egui::ComboBox::from_label(label)
                        .selected_text(
                            config
                                .key
                                .map_or_else(|| unassigned.to_owned(), |digit| digit.to_string()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut config.key, None, unassigned);
                            for digit in 0..=9 {
                                ui.selectable_value(
                                    &mut config.key,
                                    Some(digit),
                                    digit.to_string(),
                                );
                            }
                        });
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(trf!(
                        lang,
                        "Задержка от включения TX до фразы (0–{MAX_DELAY_MS} мс)",
                        "Delay from TX start to speech (0–{MAX_DELAY_MS} ms)",
                        "Verzögerung vom TX-Start bis zur Sprache (0–{MAX_DELAY_MS} ms)"
                    ));
                    ui.add(
                        egui::DragValue::new(&mut config.delay_ms)
                            .range(0..=MAX_DELAY_MS)
                            .speed(1.0),
                    );
                });
            });
        }
        if !editable {
            return;
        }
        let changed = self.voice.config != saved_config;
        if changed {
            self.armed = false;
        }
        if self.has_voice_edits()
            && ui
                .button(tr!(
                    lang,
                    "Отменить изменения",
                    "Discard changes",
                    "Änderungen verwerfen"
                ))
                .clicked()
        {
            self.discard_voice_edits();
            return;
        }
        if self.voice.loading.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr!(
                    lang,
                    "Чтение звука…",
                    "Decoding audio…",
                    "Audio wird gelesen…"
                ));
            });
            return;
        }
        if ui
            .button(tr!(
                lang,
                "Выбрать MP3 или WAV…",
                "Choose MP3 or WAV…",
                "MP3 oder WAV auswählen…"
            ))
            .clicked()
        {
            self.choose_recording();
            ui.ctx().request_repaint();
        }
        let Some(selection) = self.voice.selection.as_mut() else {
            if changed && self.voice.loading.is_none() {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(tr!(
                            lang,
                            "Сохранить настройки в BIN…",
                            "Save settings in BIN…",
                            "Einstellungen in BIN speichern…"
                        ))
                        .clicked()
                    {
                        self.save_voice(None);
                    }
                });
            }
            return;
        };
        let total = f64::from(selection.recording.duration_ms()) / 1000.0;
        ui.label(trf!(
            lang,
            "{} · {:.3} с",
            "{} · {:.3} s",
            "{} · {:.3} s",
            selection.name,
            total
        ));
        let last = selection.recording.last_start_ms();
        if last != 0 {
            let limit = MAX_FRAMES as f64 / RATE as f64;
            ui.label(trf!(
                lang,
                "Выберите начало фрагмента длиной {limit:.1} с.",
                "Choose the start of the {limit:.1} s excerpt.",
                "Start des {limit:.1} s langen Ausschnitts wählen."
            ));
            ui.add(
                egui::Slider::new(&mut selection.start_ms, 0..=last).text(tr!(
                    lang,
                    "Начало, мс",
                    "Start, ms",
                    "Start, ms"
                )),
            );
        }
        let mut save_clip = false;
        #[cfg(windows)]
        let mut listen = false;
        ui.horizontal_wrapped(|ui| {
            #[cfg(windows)]
            {
                listen = ui
                    .add_enabled(
                        self.voice.playback.is_none(),
                        egui::Button::new(tr!(
                            lang,
                            "Прослушать фрагмент",
                            "Play excerpt",
                            "Ausschnitt anhören"
                        )),
                    )
                    .clicked();
            }
            save_clip = ui
                .button(tr!(
                    lang,
                    "Сохранить в BIN…",
                    "Save in BIN…",
                    "In BIN speichern…"
                ))
                .clicked();
        });
        #[cfg(not(windows))]
        let listen = false;
        if !(save_clip || listen) {
            return;
        }
        let clip = match selection.recording.clip(selection.start_ms) {
            Ok(clip) => clip,
            Err(e) => {
                self.error = true;
                self.message = audio_error(e);
                return;
            }
        };
        if save_clip {
            self.save_voice(Some(&clip));
        }
        #[cfg(windows)]
        if listen {
            let (send, receive) = mpsc::channel();
            self.voice.playback = Some(receive);
            thread::spawn(move || {
                let _ = send.send(firmware::voice::play(&clip));
            });
            ui.ctx().request_repaint();
        }
    }

    fn save_voice(&mut self, clip: Option<&[u8; SLOT_BYTES]>) {
        let Some(source) = &self.image else {
            return;
        };
        let lang = self.language;
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!(
                lang,
                "Сохранить новый исходный BIN",
                "Save new source BIN",
                "Neue Quell-BIN speichern"
            ))
            .set_file_name("UV98-voice-source.bin")
            .add_filter("BIN", &["bin"])
            .save_file()
        else {
            return;
        };
        match save(source, clip, self.voice.config, &path) {
            Ok(image) => {
                self.set_prepared(image);
                self.finished = false;
                self.progress = None;
                self.session_dir = None;
                self.log.clear();
                self.message = msg!("Новый исходный BIN сохранён. Теперь подготовьте образ для своей рации.",
                    "New source BIN saved. Now prepare the image for your radio.",
                    "Neue Quell-BIN gespeichert. Jetzt das Image für das eigene Funkgerät vorbereiten.");
            }
            Err(error) => {
                self.error = true;
                self.message = error;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopens_config_and_renders_controls_only_for_supported_images() {
        let dir = std::env::temp_dir().join(format!(
            "uv98-voice-gui-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("radio.bin");
        let mut bin = vec![0xff; firmware::IMAGE_BYTES];
        bin[..4].copy_from_slice(&0x2000_c000u32.to_le_bytes());
        bin[4..8].copy_from_slice(&0x0800_3001u32.to_le_bytes());
        bin[0x400..0x410].copy_from_slice(firmware::profile::MAGIC);
        bin[0x600..0x610].copy_from_slice(&firmware::voice::test_policy(3));
        let clip = uv98_voice_clip::encode(&[20; 16]).unwrap();
        bin[0x2000..0x2000 + SLOT_BYTES].copy_from_slice(&clip);
        let stored = VoiceConfig {
            key: Some(9),
            delay_ms: 1739,
        };
        bin[0x1000..0x1000 + uv98_voice_clip::CONFIG_BYTES]
            .copy_from_slice(&stored.encode().unwrap());
        if let Some(real) = std::env::var_os("UV98_VOICE_CONFIG_TEST_BIN") {
            let source_path = dir.join("original.bin");
            std::fs::copy(real, &source_path).unwrap();
            let source = firmware::prepare(&source_path).unwrap();
            firmware::voice::save(&source, None, Some(stored), &path).unwrap();
        } else {
            std::fs::write(&path, &bin).unwrap();
        }
        let mut pane = FirmwarePane::default();
        pane.load_image(path.clone());
        assert_eq!(pane.voice.config, Some(stored));
        for language in Language::ALL {
            pane.set_language(language);
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(Default::default(), |ui| pane.voice_controls(ui));
            // This test inspects text shapes without uploading a font atlas.
            output.textures_delta.clear();
            let mut text = Vec::new();
            for shape in output.shapes {
                crate::ui_language_tests::collect_text(&shape.shape, &mut text);
            }
            assert!(text.iter().any(|t| t.contains("1739")));
            assert!(text.iter().any(|t| t == "9"));
            assert!(text.iter().any(|t| t.contains(tr!(
                language,
                "Задержка",
                "Delay",
                "Verzögerung"
            ))));
        }
        pane.voice.config = Some(VoiceConfig {
            key: None,
            delay_ms: 5,
        });
        assert!(pane.voice.config_changed(pane.image.as_ref().unwrap()));
        pane.port = "offline".to_owned();
        pane.armed = true;
        assert!(pane
            .start(&egui::Context::default(), false)
            .unwrap_err()
            .as_str(Language::English)
            .contains("voice settings"));
        assert!(!pane.busy());
        // A failed replacement audio file must leave the previous clip and settings available.
        let mp3 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/voice-clip/tests/fixtures/silence-tone.mp3");
        pane.voice.selection = Some(Selection {
            recording: load(&mp3).unwrap(),
            name: "previous".to_owned(),
            start_ms: 100,
        });
        let (send, receive) = mpsc::channel();
        pane.voice.loading = Some(receive);
        assert!(send.send(Err(LocalizedText::default())).is_ok());
        pane.poll_voice(&egui::Context::default());
        assert!(pane.error && pane.voice.loading.is_none());
        assert_eq!(pane.voice.selection.as_ref().unwrap().start_ms, 100);
        assert_eq!(pane.voice.config.unwrap().delay_ms, 5);
        pane.load_image(path.clone());
        assert_eq!(pane.voice.config.unwrap().delay_ms, 5);
        let missing = dir.join("missing.bin");
        pane.load_image(missing.clone());
        assert!(pane.error);
        assert_eq!(pane.selected_path.as_ref(), Some(&path));
        assert_eq!(pane.voice.config.unwrap().delay_ms, 5);
        let other = dir.join("other.bin");
        std::fs::copy(&path, &other).unwrap();
        pane.load_image(other.clone());
        assert_eq!(pane.selected_path.as_ref(), Some(&path));
        assert_eq!(pane.voice.config.unwrap().delay_ms, 5);
        pane.discard_voice_edits();
        assert_eq!(pane.voice.config, Some(stored));
        pane.load_image(missing);
        assert!(pane.error);
        assert_eq!(pane.selected_path.as_ref(), Some(&path));
        assert_eq!(pane.voice.config, Some(stored));
        pane.load_image(other.clone());
        assert_eq!(pane.selected_path, Some(other));
        assert_eq!(pane.voice.config, Some(stored));
        bin[0x600..0x610].copy_from_slice(&firmware::voice::test_policy(2));
        if let Some(real) = std::env::var_os("UV98_VOICE_LEGACY_TEST_BIN") {
            bin = std::fs::read(real).unwrap();
        }
        std::fs::write(&path, &bin).unwrap();
        pane.load_image(path);
        assert!(pane.image.unwrap().can_add_voice());
        assert!(pane.voice.config.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unfinished_import_stays_with_its_source_until_explicit_discard() {
        let mut pane = FirmwarePane::default();
        let (send, receive) = mpsc::channel();
        pane.voice.loading = Some(receive);
        pane.armed = true;
        assert!(pane
            .start(&egui::Context::default(), false)
            .unwrap_err()
            .as_str(Language::English)
            .contains("voice settings"));
        assert!(!pane.busy());
        let missing = std::env::temp_dir().join(format!(
            "uv98-missing-voice-image-{}-{}.bin",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        pane.load_image(missing);
        assert!(pane.error && pane.voice.pending());
        pane.discard_voice_edits();
        assert!(!pane.voice.pending());
        assert!(send.send(Err(LocalizedText::default())).is_err());
    }
}
