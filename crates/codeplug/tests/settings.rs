use uv98_codeplug::settings::{
    Error, Field, ScanDirection, ScanMode, ScanReturn, Settings, SettingsMut, TuningStep,
};
use uv98_codeplug::CFG_LEN;

#[test]
fn settings_reject_wrong_section_sizes() {
    for len in [0, CFG_LEN - 1, CFG_LEN + 1] {
        let mut bytes = vec![0; len];
        assert!(
            matches!(Settings::parse(&bytes), Err(Error::WrongLength { actual }) if actual == len)
        );
        assert!(
            matches!(SettingsMut::parse(&mut bytes), Err(Error::WrongLength { actual }) if actual == len)
        );
    }
}

#[test]
fn each_confirmed_setting_changes_only_its_byte() {
    // Literal offsets/ranges come from CPS DataToPanel / SaveData and controls.
    for (field, offset, value) in [
        (Field::VoicePrompt, 92, 1),
        (Field::KeyBeep, 93, 1),
        (Field::KeyLock, 94, 1),
        (Field::AutoLockDelay, 95, 42),
        (Field::KeypadLight, 96, 1),
        (Field::BacklightLevel, 97, 4),
        (Field::BacklightDelay, 98, 42),
        (Field::SaverMode, 99, 3),
        (Field::SaverStart, 100, 42),
        (Field::MenuExitDelay, 101, 42),
        (Field::TuningStep, 120, 13),
        (Field::ScanDirection, 156, 1),
        (Field::ScanMode, 157, 2),
        (Field::ScanReturn, 158, 1),
        (Field::ScanDwell, 159, 30),
        (Field::Squelch, 164, 10),
        (Field::Vox, 167, 1),
        (Field::VoxThreshold, 168, 255),
        (Field::VoxDelay, 169, 10),
        (Field::RxSpeaker, 174, 63),
        (Field::RxDac, 175, 15),
        (Field::RxSpeakerAm, 176, 63),
        (Field::RxDacAm, 177, 15),
        (Field::FmStandby, 261, 1),
    ] {
        let mut bytes = [0xA5; CFG_LEN];
        let mut expected = bytes;
        expected[offset] = value;
        let mut settings = SettingsMut::parse(&mut bytes).unwrap();
        settings.set_code(field, value).unwrap();
        assert_eq!(settings.view().code(field), value);
        assert_eq!(settings.view().raw(), &expected);
    }
}

#[test]
fn every_field_is_unique_and_all_byte_codes_preserve_other_bytes() {
    assert_eq!(Field::ALL.len(), 24);
    for (index, field) in Field::ALL.into_iter().enumerate() {
        assert!(!field.label_ru().is_empty());
        assert!(!field.group_ru().is_empty());
        assert!(Field::ALL[..index]
            .iter()
            .all(|other| other.offset() != field.offset()));
        for value in 0..=u8::MAX {
            let mut bytes = core::array::from_fn::<_, CFG_LEN, _>(|i| {
                (i as u8).wrapping_mul(53).wrapping_add(19)
            });
            // Unknown values must survive the read path, even when a normal
            // setter would reject them. Parsing does not normalize defaults.
            bytes[field.offset()] = value;
            let before = bytes;
            assert_eq!(Settings::parse(&bytes).unwrap().code(field), value);
            let mut edit = SettingsMut::parse(&mut bytes).unwrap();
            let result = edit.set_code(field, value);
            if value <= field.max_code() {
                assert_eq!(result, Ok(()));
            } else {
                assert_eq!(result, Err(Error::InvalidCode { field, value }));
                assert_eq!(field.choice_label(value), None);
            }
            assert_eq!(bytes, before);
        }
        let mut bytes = [0xF0; CFG_LEN];
        if field.max_code() < u8::MAX {
            let invalid = field.max_code() + 1;
            assert!(SettingsMut::parse(&mut bytes)
                .unwrap()
                .set_code(field, invalid)
                .is_err());
            assert_eq!(bytes, [0xF0; CFG_LEN]);
        }
    }
}

#[test]
fn typed_scan_and_step_edits_touch_only_their_literal_stock_offsets() {
    let mut bytes = [0xF0; CFG_LEN];
    let view = Settings::parse(&bytes).unwrap();
    assert_eq!(view.scan_direction(), None);
    assert_eq!(view.scan_mode(), None);
    assert_eq!(view.scan_return(), None);
    assert_eq!(view.tuning_step(), None);
    let mut expected = bytes;
    expected[156] = 1;
    expected[157] = 2;
    expected[158] = 1;
    expected[120] = 6;
    let mut edit = SettingsMut::parse(&mut bytes).unwrap();
    edit.set_scan_direction(ScanDirection::Down);
    edit.set_scan_mode(ScanMode::Se);
    edit.set_scan_return(ScanReturn::CurrentChannel);
    edit.set_tuning_step(TuningStep::Hz12500);
    assert_eq!(edit.view().scan_direction(), Some(ScanDirection::Down));
    assert_eq!(edit.view().scan_mode(), Some(ScanMode::Se));
    assert_eq!(edit.view().scan_return(), Some(ScanReturn::CurrentChannel));
    assert_eq!(edit.view().tuning_step(), Some(TuningStep::Hz12500));
    assert_eq!(bytes, expected);
}

#[test]
fn typed_codes_and_frequency_steps_match_all_cps_choices_without_fallback() {
    let exact_10hz = [
        25, 125, 250, 500, 625, 1000, 1250, 2000, 2500, 5000, 10000, 50000, 100000, 500000,
    ];
    for raw in 0..=u8::MAX {
        assert_eq!(
            ScanDirection::decode(raw).map(|v| v.code()),
            (raw <= 1).then_some(raw)
        );
        assert_eq!(
            ScanMode::decode(raw).map(|v| v.code()),
            (raw <= 2).then_some(raw)
        );
        assert_eq!(
            ScanReturn::decode(raw).map(|v| v.code()),
            (raw <= 1).then_some(raw)
        );
        let step = TuningStep::decode(raw);
        assert_eq!(step.map(|v| v.code()), (raw <= 13).then_some(raw));
        if let Some(step) = step {
            assert_eq!(step.units_10hz(), exact_10hz[raw as usize]);
            assert!(Field::TuningStep.choice_label(raw).is_some());
        }
    }
}

#[test]
fn timer_captions_do_not_invent_physical_units_or_map_unknown_to_disabled() {
    let exact = [
        0, 5, 10, 15, 30, 45, 60, 75, 90, 105, 120, 135, 150, 165, 180, 195, 210, 225, 240, 255,
        270, 285, 300, 315, 330, 345, 360, 375, 390, 405, 420, 435, 450, 465, 480, 495, 510, 525,
        540, 555, 570, 585, 600,
    ];
    for field in [
        Field::AutoLockDelay,
        Field::BacklightDelay,
        Field::MenuExitDelay,
        Field::SaverStart,
    ] {
        for (code, value) in exact.into_iter().enumerate() {
            assert_eq!(field.timer_value(code as u8), Some(value));
            if code != 0 || field == Field::SaverStart {
                assert_eq!(
                    field.choice_label(code as u8),
                    Some(value.to_string().as_str())
                );
            } else {
                assert_eq!(field.choice_label(0), Some("Выключено"));
            }
        }
        assert_eq!(field.timer_value(43), None);
        assert_eq!(field.choice_label(255), None);
    }
    assert_eq!(Field::ScanDwell.timer_value(1), None);
    assert_eq!(Field::VoxDelay.choice_label(1), Some("0,5 с"));
    assert_eq!(Field::VoxDelay.choice_label(10), Some("5 с"));
    assert_eq!(Field::VoxDelay.choice_label(11), None);
    assert_eq!(Field::RxDac.choice_label(0), None);
    assert_eq!(Field::VoicePrompt.choice_label(255), None);
}

#[test]
fn unknown_settings_are_preserved_and_bad_edits_are_atomic() {
    let mut bytes = [0xF0; CFG_LEN];
    for (field, invalid) in [
        (Field::SaverMode, 4),
        (Field::SaverStart, 43),
        (Field::Squelch, 11),
        (Field::Vox, 2),
        (Field::VoxDelay, 11),
    ] {
        let mut settings = SettingsMut::parse(&mut bytes).unwrap();
        assert_eq!(settings.view().code(field), 0xF0);
        assert_eq!(
            settings.set_code(field, invalid),
            Err(Error::InvalidCode {
                field,
                value: invalid
            })
        );
        assert_eq!(settings.view().raw(), &[0xF0; CFG_LEN]);
    }
}
