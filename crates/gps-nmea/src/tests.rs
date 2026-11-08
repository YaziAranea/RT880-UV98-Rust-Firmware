extern crate std;
use super::*;
use std::{format, string::String, vec::Vec};

const GGA: &[u8] = b"$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47\r\n";
const RMC: &[u8] = b"$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A\r\n";

fn sentence(body: &str) -> Vec<u8> {
    let checksum = body.bytes().fold(0, |sum, byte| sum ^ byte);
    format!("${body}*{checksum:02X}\r\n").into_bytes()
}
fn feed(parser: &mut Parser, bytes: &[u8], now: u32) -> Option<Event> {
    let mut event = None;
    for &byte in bytes {
        if let Some(next) = parser.push(byte, now) {
            event = Some(next);
        }
        assert!(parser.buffered_len() <= LINE_CAPACITY);
    }
    event
}
fn accepted(parser: &mut Parser, bytes: &[u8], now: u32) -> Report {
    match feed(parser, bytes, now) {
        Some(Event::Accepted(report)) => report,
        other => panic!("{other:?}"),
    }
}

#[test]
fn fixed_known_checksums_coordinates_and_metadata() {
    let mut parser = Parser::new();
    assert_eq!(parser.snapshot(0, 1000), None);
    let gga = accepted(&mut parser, GGA, 100);
    assert_eq!(
        gga.position,
        Some(Position {
            latitude_e7: 481_173_000,
            longitude_e7: 115_166_667
        })
    );
    assert_eq!(
        gga.utc,
        Some(Utc {
            hour: 12,
            minute: 35,
            second: 19,
            nanosecond: 0
        })
    );
    assert_eq!(
        (gga.satellites, gga.hdop_centi, gga.quality, gga.date),
        (Some(8), Some(90), Some(1), None)
    );
    let rmc = accepted(&mut parser, RMC, 200);
    assert_eq!(rmc.position, gga.position);
    assert_eq!(
        rmc.date,
        Some(Date {
            day: 23,
            month: 3,
            year_2digit: 94
        })
    );
    assert_eq!((rmc.satellites, rmc.hdop_centi), (None, None));
    assert!(parser.snapshot(1200, 1000).is_some());
    assert!(parser.snapshot(1201, 1000).is_none());
}

#[test]
fn supported_talkers_hemispheres_poles_rounding_and_no_metadata_epoch_merge() {
    let mut parser = Parser::new();
    accepted(&mut parser, RMC, 0);
    for (index, talker) in ["GP", "GN", "BD"].iter().enumerate() {
        let bytes = sentence(&format!(
            "{talker}GGA,000000.123456789,9000.000000000,S,18000.000000000,W,1,12,1.235,0,M,0,M,,"
        ));
        let report = accepted(&mut parser, &bytes, index as u32 * 2 + 1);
        assert_eq!(
            report.position,
            Some(Position {
                latitude_e7: -900_000_000,
                longitude_e7: -1_800_000_000
            })
        );
        assert_eq!(report.utc.unwrap().nanosecond, 123_456_789);
        assert_eq!(report.hdop_centi, Some(124));
        assert_eq!(report.date, None); // Yesterday's RMC date is not reused.
        let bytes = sentence(&format!(
            "{talker}RMC,235960,A,0000.000003,S,00000.000003,W,0,0,290224,,,A"
        ));
        let report = accepted(&mut parser, &bytes, index as u32 * 2 + 2);
        assert_eq!(
            report.position,
            Some(Position {
                latitude_e7: -1,
                longitude_e7: -1
            })
        );
        assert_eq!(report.utc.unwrap().second, 60);
    }
}

#[test]
fn explicit_loss_of_fix_removes_position_including_stale_coordinates_and_mode_n() {
    let mut parser = Parser::new();
    let losses = [
        "GPRMC,123520,V,4807.038,N,01131.000,E,0,0,230394,,,",
        "GNRMC,123520,V,,,,,,,230394,,,N",
        "BDGGA,123520,,,,,0,00,,,,,,,",
        "GPGGA,123520,4807.038,N,01131.000,E,0,00,0,0,M,0,M,,",
        "GPRMC,123520,A,4807.038,N,01131.000,E,0,0,230394,,,N",
    ];
    for loss in losses {
        accepted(&mut parser, GGA, 100);
        let report = accepted(&mut parser, &sentence(loss), 100);
        assert!(!report.valid);
        assert_eq!(report.position, None);
        assert_eq!(parser.snapshot(100, 10_000), None);
    }
}

#[test]
fn malformed_data_cannot_overwrite_good_report_or_refresh_its_age() {
    let mut parser = Parser::new();
    let good = accepted(&mut parser, GGA, 10);
    let cases = [
        "GPGGA,123519,4807.038,N,01131.000,E,9,08,0.9,0,M,0,M,,",
        "GPGGA,123519,9060.000,N,01131.000,E,1,08,0.9,0,M,0,M,,",
        "GPGGA,123519,9000.001,N,01131.000,E,1,08,0.9,0,M,0,M,,",
        "GPGGA,123519,4807.038,E,01131.000,E,1,08,0.9,0,M,0,M,,",
        "GPGGA,123519,4807.038,N,18100.000,E,1,08,0.9,0,M,0,M,,",
        "GPGGA,123519,4807.038,N,01160.000,E,1,08,0.9,0,M,0,M,,",
        "GPGGA,246000,4807.038,N,01131.000,E,1,08,0.9,0,M,0,M,,",
        "GPGGA,123519,4807.038,N,01131.000,E,1,999999999999,0.9,0,M,0,M,,",
        "GPGGA,123519,4807.038,N,01131.000,E,1,08,NaN,0,M,0,M,,",
        "GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,garbage,M,0,M,,",
        "GPRMC,123519,A,4807.038,N,01131.000,E,0,0,310224,,,A",
        "GPRMC,123519,A,4807.038,N,01131.000,E,0,0,290223,,,A",
        "GPRMC,123519,A,4807.038,N,01131.000,E,0,0,010124,,,X",
        "GPRMC,123519,A,4807.038,N,01131.000,E",
    ];
    for body in cases {
        assert!(
            matches!(
                feed(&mut parser, &sentence(body), 999),
                Some(Event::Rejected(_))
            ),
            "{body}"
        );
        assert_eq!(parser.last_report(), Some(good));
        assert_eq!(parser.snapshot(999, 100), None);
    }
}

#[test]
fn checksum_and_strict_crlf_precede_acceptance_and_resynchronize() {
    let mut parser = Parser::new();
    let good = accepted(&mut parser, GGA, 10);
    for corrupt in [
        b"$GPGGA,123519*00\r\n".as_slice(),
        b"$GPGGA,123519*XX\r\n",
        b"$GPGGA,123519*001\r\n",
        b"$GPGGA,123519**00\r\n",
    ] {
        assert_eq!(
            feed(&mut parser, corrupt, 20),
            Some(Event::Rejected(Error::Checksum))
        );
        assert_eq!(parser.last_report(), Some(good));
    }
    assert_eq!(feed(&mut parser, &RMC[..RMC.len() - 2], 20), None);
    assert_eq!(parser.last_report(), Some(good));
    assert_eq!(
        parser.push(b'\n', 20),
        Some(Event::Rejected(Error::Framing))
    );
    assert_eq!(parser.last_report(), Some(good));
    assert_eq!(feed(&mut parser, &RMC[..RMC.len() - 1], 20), None);
    assert!(matches!(parser.push(b'\n', 20), Some(Event::Accepted(_))));
    for length in 1..GGA.len() {
        feed(&mut parser, &GGA[..length], 20);
        assert!(matches!(
            feed(&mut parser, RMC, 20),
            Some(Event::Accepted(_))
        ));
    }
}

#[test]
fn overflow_unknown_sentences_and_every_byte_stream_are_bounded() {
    let mut parser = Parser::new();
    let good = accepted(&mut parser, GGA, 10);
    parser.push(b'$', 20);
    for _ in 0..LINE_CAPACITY {
        assert_eq!(parser.push(b'A', 20), None);
    }
    assert_eq!(parser.push(b'A', 20), Some(Event::Rejected(Error::TooLong)));
    assert_eq!(parser.last_report(), Some(good));
    assert!(matches!(
        feed(&mut parser, RMC, 20),
        Some(Event::Accepted(_))
    ));
    for body in ["GLRMC,123519,A", "GPVTG,0,T,0,M,0,N,0,K"] {
        let original = parser.last_report();
        assert_eq!(feed(&mut parser, &sentence(body), 30), Some(Event::Ignored));
        assert_eq!(parser.last_report(), original);
    }
    // All byte values at every framing/line position, plus an untrusted stream.
    for byte in 0u8..=255 {
        for length in 0..=LINE_CAPACITY + 1 {
            let mut candidate = Parser::new();
            candidate.push(b'$', 40);
            for _ in 0..length {
                candidate.push(byte, 40);
            }
            candidate.push(b'\r', 40);
            candidate.push(b'\n', 40);
            assert!(candidate.buffered_len() <= LINE_CAPACITY);
            assert!(matches!(
                feed(&mut candidate, GGA, 40),
                Some(Event::Accepted(_))
            ));
        }
    }
    let mut state = 0x12345678u32;
    for _ in 0..100_000 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        parser.push((state >> 24) as u8, 50);
        assert!(parser.buffered_len() <= LINE_CAPACITY);
    }
    assert!(matches!(
        feed(&mut parser, RMC, 50),
        Some(Event::Accepted(_))
    ));
    assert!(core::mem::size_of::<Parser>() < 256);
}

#[test]
fn wrapping_clock_expiry_and_backwards_updates_are_explicit() {
    let mut parser = Parser::new();
    let good = accepted(&mut parser, GGA, u32::MAX - 100);
    assert!(parser.snapshot(49, 150).is_some());
    assert!(parser.snapshot(50, 150).is_none());
    assert!(parser.snapshot(u32::MAX - 101, 1000).is_none());
    assert!(parser.snapshot(0, 1 << 31).is_none());
    assert_eq!(
        feed(&mut parser, RMC, u32::MAX - 101),
        Some(Event::Rejected(Error::Clock))
    );
    assert_eq!(parser.last_report(), Some(good));
    accepted(&mut parser, RMC, 50);
    assert!(parser.snapshot(50, 0).is_some());
}

#[test]
fn baud_table_is_exhaustive_and_no_numeric_metadata_defaults_are_invented() {
    let expected = [
        4800, 9600, 14400, 19200, 38400, 56000, 57600, 115200, 128000, 256000,
    ];
    for selector in 0u8..=255 {
        assert_eq!(
            stock_baud(selector),
            expected.get(selector as usize).copied().unwrap_or(9600)
        );
    }
    let mut parser = Parser::new();
    let report = accepted(&mut parser, &sentence("GPGGA,,0000,N,00000,E,1,,,,,,,,"), 0);
    assert_eq!(
        (
            report.utc,
            report.date,
            report.satellites,
            report.hdop_centi
        ),
        (None, None, None, None)
    );
    assert_eq!(
        report.position,
        Some(Position {
            latitude_e7: 0,
            longitude_e7: 0
        })
    ); // Only because explicitly received.
    let mut lower_checksum = String::from_utf8(RMC.to_vec()).unwrap();
    lower_checksum = lower_checksum.replace("*6A", "*6a");
    accepted(&mut parser, lower_checksum.as_bytes(), 1);
}
