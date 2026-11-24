//! Headless tests of the production egui interface. No native windows or COM.
use super::*;
use egui::epaint::{ImageData, Primitive, Shape};
use serde_json::json;

fn sample_app() -> App {
    let mut bytes = vec![0xff; FILE_LEN];
    bytes[..CFG_LEN].fill(0);
    bytes[CHANNELS_OFFSET + 2] = 1;
    channels::patch_channel(
        &mut bytes[CHANNELS_OFFSET..CHANNELS_OFFSET + CHANNELS_LEN],
        0,
        &ChannelPatch {
            name_raw: Some(encode_name("CPS SAMPLE").unwrap()),
            rx_frequency_code: Some(14_550_000),
            tx_frequency_code: Some(14_550_000),
            operation_mode: Some(OperationMode::ReceiveTransmit),
            bandwidth: Some(Bandwidth::Wide),
            power: Some(Power::Low),
            rx_tone: Some(channels::Tone::None),
            tx_tone: Some(channels::Tone::None),
            ..Default::default()
        },
    )
    .unwrap();
    bytes[ZONES_OFFSET..ZONES_OFFSET + 2].fill(0);
    bytes[ZONES_OFFSET + 4..ZONES_OFFSET + 20]
        .copy_from_slice(&encode_name("SAMPLE ZONE").unwrap());
    let document = Document::from_bytes(&bytes).unwrap();
    let mut app = App {
        last_export: bytes,
        document: Some(document),
        channel_index: Some(0),
        ..Default::default()
    };
    app.refresh_forms();
    app.firmware.set_preview(true);
    // Synthetic UI values, not a substitute for the private golden fixture.
    let mut simulated_calibration = [0; 4096];
    simulated_calibration[0] = uv98_calibration::MARKER;
    app.calibrations
        .load_bytes(
            &simulated_calibration,
            "SIMULATED calibration".into(),
            "SIMULATED".into(),
        )
        .unwrap();
    app
}

pub(crate) fn collect_text(shape: &Shape, result: &mut Vec<String>) {
    match shape {
        Shape::Text(text) => result.push(text.galley.text().to_owned()),
        Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, result);
            }
        }
        _ => {}
    }
}

/// Optional public preview input: actual tessellated egui meshes + font atlas.
/// Only synthetic state is passed here, never an opened user file or radio dump.
fn export_preview(ctx: &egui::Context, mut output: egui::FullOutput, name: &str, size: [f32; 2]) {
    let Some(folder) = std::env::var_os("UV98_CPS_PREVIEW_DIR") else {
        output.textures_delta.clear();
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    let mut textures = Vec::new();
    for (id, deltas) in &output.textures_delta.set {
        for delta in deltas {
            let ImageData::Color(image) = &delta.image;
            let raw: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            let file = format!("{name}-texture-{}.rgba", textures.len());
            std::fs::write(folder.join(&file), raw).unwrap();
            textures.push(
                json!({"id":format!("{id:?}"),"size":image.size,"pos":delta.pos,"file":file}),
            );
        }
    }
    output.textures_delta.clear();
    let meshes:Vec<_>=ctx.tessellate(output.shapes,output.pixels_per_point).into_iter().map(|primitive| {
        let Primitive::Mesh(mesh)=primitive.primitive else { panic!("Unexpected GPU callback in preview"); };
        let vertices:Vec<_>=mesh.vertices.iter().map(|v|json!([v.pos.x,v.pos.y,v.uv.x,v.uv.y,v.color.to_array()])).collect();
        json!({"clip":[primitive.clip_rect.min.x,primitive.clip_rect.min.y,primitive.clip_rect.max.x,primitive.clip_rect.max.y],"texture":format!("{:?}",mesh.texture_id),"vertices":vertices,"indices":mesh.indices})
    }).collect();
    std::fs::write(folder.join(format!("{name}.json")),serde_json::to_vec(&json!({"simulated":true,"size":size,"pixels_per_point":output.pixels_per_point,"textures":textures,"meshes":meshes})).unwrap()).unwrap();
}

#[test]
fn languages_render_every_page_without_touching_radio_or_document() {
    let tabs = [
        (Tab::Channels, "channels"),
        (Tab::Zones, "zones"),
        (Tab::Settings, "settings"),
        (Tab::Navigation, "navigation"),
        (Tab::Changes, "changes"),
        (Tab::Radio, "backup"),
        (Tab::Calibration, "calibration"),
        (Tab::Firmware, "firmware"),
    ];
    for size in [[940.0, 640.0], [1200.0, 800.0]] {
        for language in Language::ALL {
            for (tab, name) in tabs {
                let mut app = sample_app();
                app.tab = tab;
                app.set_language(language);
                let before = app.document.as_ref().unwrap().export_bytes();
                let ctx = egui::Context::default();
                configure_style(&ctx);
                let mut frame = eframe::Frame::_new_kittest();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(size[0], size[1]),
                    )),
                    ..Default::default()
                };
                // Warm up font atlases and layout before inspecting the settled
                // frame, keeping all texture deltas for the software renderer.
                let mut warmup = ctx.run_ui(input.clone(), |ui| {
                    eframe::App::ui(&mut app, ui, &mut frame)
                });
                let mut output = ctx.run_ui(input, |ui| eframe::App::ui(&mut app, ui, &mut frame));
                warmup
                    .textures_delta
                    .append(std::mem::take(&mut output.textures_delta));
                output.textures_delta = std::mem::take(&mut warmup.textures_delta);
                let mut texts = Vec::new();
                for shape in &output.shapes {
                    collect_text(&shape.shape, &mut texts);
                }
                export_preview(&ctx, output, &format!("{}-{name}", language.code()), size);
                let heading = match tab {
                    Tab::Channels => tr!(language, "Канал 1", "Channel 1", "Kanal 1"),
                    Tab::Zones => tr!(language, "Зона 1", "Zone 1", "Zone 1"),
                    Tab::Settings => tr!(
                        language,
                        "Общие настройки",
                        "General settings",
                        "Allgemeine Einstellungen"
                    ),
                    Tab::Navigation => "GPS / APRS",
                    Tab::Changes => tr!(
                        language,
                        "Изменения относительно исходного файла",
                        "Changes from the original file",
                        "Änderungen gegenüber der Originaldatei"
                    ),
                    Tab::Radio => tr!(
                        language,
                        "Резервные копии UV98",
                        "UV98 backups",
                        "UV98-Sicherungen"
                    ),
                    Tab::Calibration => tr!(
                        language,
                        "Калибровки станции",
                        "Radio calibration",
                        "Funkgeräte-Kalibrierung"
                    ),
                    Tab::Firmware => tr!(
                        language,
                        "Прошивка UV98 Plus",
                        "UV98 Plus firmware",
                        "UV98-Plus-Firmware"
                    ),
                };
                assert!(
                    texts.iter().any(|text| text == heading),
                    "{} {name}: page heading missing: {heading}",
                    language.code()
                );
                assert!(
                    texts.iter().any(|text| text == "Язык / Language / Sprache"),
                    "{name}: language selector missing"
                );
                assert!(
                    texts.iter().any(|text| text == language.autonym()),
                    "{name}: selected language missing"
                );
                if language != Language::Russian {
                    for text in &texts {
                        if text == "Язык / Language / Sprache" {
                            continue;
                        }
                        assert!(
                            !text.chars().any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)),
                            "{} {name}: untranslated text {text:?}",
                            language.code()
                        );
                    }
                }
                assert_eq!(app.document.as_ref().unwrap().bytes(), before);
                assert!(!app.radio.busy() && !app.firmware.busy());
            }
        }
    }
}

#[test]
fn switching_language_preserves_drafts_selection_undo_and_notices() {
    let mut app = sample_app();
    app.document
        .as_mut()
        .unwrap()
        .apply(|bytes| -> Result<(), ()> {
            bytes[99] = 2;
            Ok(())
        })
        .unwrap();
    app.channel_draft.as_mut().unwrap().current.name = "Unapplied name".into();
    app.search = "145".into();
    app.notice = msg!("Форма изменена", "Form changed", "Formular geändert");
    let bytes = app.document.as_ref().unwrap().export_bytes();
    let original = app.document.as_ref().unwrap().original_bytes().to_vec();
    let exported = app.last_export.clone();
    let draft = app.channel_draft.as_ref().unwrap().current.clone();
    let undo = app.document.as_ref().unwrap().undo_depth();
    for language in Language::ALL {
        app.set_language(language);
        assert_eq!(app.document.as_ref().unwrap().bytes(), bytes);
        assert_eq!(app.document.as_ref().unwrap().original_bytes(), original);
        assert_eq!(app.last_export, exported);
        assert!(app.channel_draft.as_ref().unwrap().current == draft);
        assert_eq!(app.document.as_ref().unwrap().undo_depth(), undo);
        assert_eq!(app.channel_index, Some(0));
        assert!(app.tab == Tab::Channels);
        assert_eq!(app.search, "145");
        assert!(app.pending_form() && app.unsaved());
    }
    assert_eq!(app.notice.as_str(Language::German), "Formular geändert");
    let error = parse_tone("409.6").unwrap_err();
    assert!(error.as_str(Language::German).contains("überschreitet"));
    assert!(error.as_str(Language::Russian).contains("превышает"));
    assert!(text_codec::format_tone(channels::Tone::None.encode().unwrap()).is_empty());
}
