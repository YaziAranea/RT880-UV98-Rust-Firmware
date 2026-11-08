use uv98_stock_model::rf::*;

#[test]
fn machine_code_oracle_matches_pure_rf_models() {
    let fixture = include_str!("fixtures/rf-oracle.csv");
    let fields = [
        FilterField::Tx300,
        FilterField::Tx3k,
        FilterField::Rx300,
        FilterField::Rx3k,
    ];
    let mut count = 0;
    for line in fixture
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let (kind, values) = line.split_once(',').unwrap();
        let v: Vec<u32> = values
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        match kind {
            "power_helper" => assert_eq!(power_register_04f0c(v[0]), v[1] as u16, "{line}"),
            "mic_helper" => assert_eq!(mic_register_04e90(v[0]), v[1] as u16, "{line}"),
            "volume_helper" => assert_eq!(
                volume_register_05768(v[0] as u16, v[1] as u16),
                v[2] as u16,
                "{line}"
            ),
            "compander" => {
                assert_eq!(
                    compander_reset_04518(v[0] as u16),
                    [
                        RegisterCode {
                            address: 0x31,
                            value: v[1] as u16
                        },
                        RegisterCode {
                            address: 0x28,
                            value: v[2] as u16
                        },
                        RegisterCode {
                            address: 0x29,
                            value: v[3] as u16
                        },
                    ],
                    "{line}"
                );
            }
            "filter" => {
                let actual = filter_registers(fields[v[0] as usize], v[1] as u8);
                assert_eq!(
                    actual.first,
                    RegisterCode {
                        address: v[2] as u8,
                        value: v[3] as u16
                    },
                    "{line}"
                );
                let second = if v[4] != 0 {
                    Some(RegisterCode {
                        address: v[5] as u8,
                        value: v[6] as u16,
                    })
                } else {
                    None
                };
                assert_eq!(actual.second, second, "{line}");
            }
            "power_select" => {
                let actual = if v[0] == 0 {
                    power_code_at_05552(v[1] as u8, v[2] as u8, v[3] as u8)
                } else {
                    power_code_at_040dc(v[1] as u8, v[2] as u8, v[3] as u8)
                };
                assert_eq!(actual, v[4] as u8, "{line}");
            }
            "deviation_select" => {
                let actual = if v[0] == 0 {
                    deviation_at_054cc(v[1] as u8, v[2] as u8, v[3] as u16, v[4] as u16)
                } else {
                    deviation_at_040a8(v[1] as u8, v[3] as u16, v[4] as u16)
                };
                assert_eq!(actual, v[5] as u16, "{line}");
            }
            _ => panic!("unknown oracle vector: {line}"),
        }
        count += 1;
    }
    assert_eq!(count, 903, "missing machine-code oracle vectors");
}

#[test]
fn register_codes_preserve_high_bits_wrap_and_branch_specific_encodings() {
    assert_eq!(power_register_04f0c(0), 0x007F);
    assert_eq!(power_register_04f0c(64), 0x00FF); // truncation, not saturation or off
    assert_eq!(mic_register_04e90(0xFFFF), 0xE93F); // 16-bit wrap
    assert_eq!(deviation_at_040a8(0, 0xFABC, 0), 0xFABC); // not masked to twelve bits
    assert_eq!(deviation_at_054cc(0, 1, 0x34E2, 0), 0x341A);
    assert_eq!(deviation_at_054cc(0, 0x10, 0x34E2, 0), 0x34E2);
    assert_eq!(power_code_at_05552(0, 43, 28), 28);
    assert_eq!(power_code_at_040dc(0, 43, 28), 35);
    assert_eq!(volume_register_05768(0xB34F, 2), 0xB34C);
    assert_eq!(volume_register_05768(0xB34F, 10), 0xB34F); // full reg1 comparison
    assert_eq!(
        filter_registers(FilterField::Tx300, 0xF0).first.value,
        0x8BE3
    );
    assert_eq!(
        filter_registers(FilterField::Tx3k, 0xF0).first.value,
        0xF50B
    );
}
