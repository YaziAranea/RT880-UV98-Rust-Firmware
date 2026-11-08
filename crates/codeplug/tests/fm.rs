use uv98_codeplug::fm::{
    Error, Field, FmChannel, FmChannelMut, FmChannels, FmChannelsMut, RECORD_LEN,
};
use uv98_codeplug::FM_LEN;

#[test]
fn fm_section_length_and_channel_bounds_are_strict() {
    for len in [0, FM_LEN - 1, FM_LEN + 1] {
        let mut bytes = vec![0; len];
        assert!(
            matches!(FmChannels::parse(&bytes), Err(Error::WrongLength { actual }) if actual == len)
        );
        assert!(
            matches!(FmChannelsMut::parse(&mut bytes), Err(Error::WrongLength { actual }) if actual == len)
        );
    }
    let mut bytes = [0xA5; FM_LEN];
    let mut expected = bytes;
    expected[127 * 32 + 7] = 0x34;
    expected[127 * 32 + 8] = 0x12;
    let mut channels = FmChannelsMut::parse(&mut bytes).unwrap();
    assert!(channels.channel(128).is_none());
    assert!(channels.channel(usize::MAX).is_none());
    assert!(channels.view().channel(usize::MAX).is_none());
    channels.channel(127).unwrap().set_frequency_code(0x1234);
    assert_eq!(
        channels.view().channel(127).unwrap().frequency_code(),
        0x1234
    );
    assert_eq!(channels.view().raw(), &expected);
}

#[test]
fn each_fm_selector_patches_only_one_confirmed_byte() {
    for (field, offset, value) in [
        (Field::Range, 0, 3),
        (Field::Demodulation, 1, 4),
        (Field::Step, 2, 6),
        (Field::Bandwidth, 3, 5),
        (Field::Attenuation, 4, 5),
    ] {
        let mut bytes = [0xA5; RECORD_LEN];
        let mut expected = bytes;
        expected[offset] = value;
        let mut channel = FmChannelMut::from_bytes(&mut bytes);
        channel.set_code(field, value).unwrap();
        assert_eq!(channel.view().code(field), value);
        assert_eq!(channel.view().raw(), &expected);
    }
}

#[test]
fn unknown_fm_values_are_read_without_defaults_and_invalid_edits_are_atomic() {
    let mut bytes = [0xF0; RECORD_LEN];
    let mut channel = FmChannelMut::from_bytes(&mut bytes);
    for (field, invalid) in [
        (Field::Range, 4),
        (Field::Demodulation, 5),
        (Field::Step, 7), // UI has this choice, SaveSingleFM does not emit it.
        (Field::Bandwidth, 6),
        (Field::Attenuation, 6), // Ditto: UI extends to 38; save loop ends at 5.
    ] {
        assert_eq!(channel.view().code(field), 0xF0);
        assert_eq!(
            channel.set_code(field, invalid),
            Err(Error::InvalidCode {
                field,
                value: invalid
            })
        );
    }
    assert_eq!(
        channel.set_bfo_value(i16::MIN),
        Err(Error::BfoOutsideCpsRange { value: i16::MIN })
    );
    assert_eq!(
        channel.set_name_encoded(&[0; 17]),
        Err(Error::NameTooLong { actual: 17 })
    );
    assert_eq!(channel.view().raw(), &[0xF0; RECORD_LEN]);
}

#[test]
fn bfo_uses_offset_binary_not_signed_twos_complement() {
    for (value, low, high) in [
        (-32767, 1, 0),
        (-1, 0xFF, 0x7F),
        (0, 0, 0x80),
        (32767, 0xFF, 0xFF),
    ] {
        let mut bytes = [0x96; RECORD_LEN];
        let mut expected = bytes;
        expected[5] = low;
        expected[6] = high;
        let mut channel = FmChannelMut::from_bytes(&mut bytes);
        channel.set_bfo_value(value).unwrap();
        assert_eq!(channel.view().bfo_value(), value);
        assert_eq!(channel.view().raw(), &expected);
    }
    let mut bytes = [0x96; RECORD_LEN];
    let mut expected = bytes;
    expected[5..7].fill(0);
    let mut channel = FmChannelMut::from_bytes(&mut bytes);
    channel.set_bfo_code(0);
    assert_eq!(channel.view().bfo_value(), -32768);
    assert_eq!(channel.view().bfo_code(), 0);
    assert_eq!(channel.view().raw(), &expected);
}

#[test]
fn present_marker_and_encoded_name_do_not_initialize_unknown_bytes() {
    let mut bytes = [0xA5; RECORD_LEN];
    assert!(!FmChannel::from_bytes(&bytes).is_present());
    let mut expected = bytes;
    expected[9] = 1;
    expected[16..32].fill(0xFF);
    expected[16..20].copy_from_slice(&[0xD6, 0xD0, 0xCE, 0xC4]);
    let mut channel = FmChannelMut::from_bytes(&mut bytes);
    channel.mark_present();
    channel.set_name_encoded(&[0xD6, 0xD0, 0xCE, 0xC4]).unwrap();
    assert!(channel.view().is_present());
    assert_eq!(channel.view().presence_code(), 1);
    assert_eq!(channel.view().raw(), &expected);
    let exact = [0, 0x80, 0xFF, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
    expected[16..32].copy_from_slice(&exact);
    channel.set_name_raw(&exact);
    assert_eq!(channel.view().name_raw(), &exact);
    assert_eq!(channel.view().raw(), &expected);
}
