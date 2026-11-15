use super::*;
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug)]
enum Damage {
    None,
    Checksum(u16),
    Address(u16),
    Opcode(u16),
    Short(u16),
    Extra(u16),
    Changed(u16),
    ReadError(u16),
    ShortSend(u16),
    NoAck,
    Ack(u8),
    ZeroThenAck,
    LateAck,
    EndError,
    CloseError,
    Overflow,
}

#[derive(Default, Debug)]
struct Log {
    packets: Vec<Vec<u8>>,
    closes: usize,
}

struct Fake {
    pending: VecDeque<u8>,
    fragment: usize,
    damage: Damage,
    block: Option<u16>,
    connects: usize,
    log: Arc<Mutex<Log>>,
}

fn payload(block: u16) -> Vec<u8> {
    (0..BLOCK_SIZE)
        .map(|offset| (u32::from(block) * 31 + offset as u32 * 17 + 11) as u8)
        .collect()
}

fn frame(block: u16) -> Vec<u8> {
    let mut bytes = vec![0x52, (block >> 8) as u8, block as u8];
    bytes.extend(payload(block));
    bytes.push(bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)));
    bytes
}

fn update_checksum(bytes: &mut [u8]) {
    let last = bytes.len() - 1;
    bytes[last] = bytes[..last]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
}

impl Fake {
    fn new(damage: Damage, fragment: usize, log: Arc<Mutex<Log>>) -> Self {
        Self {
            pending: VecDeque::new(),
            fragment,
            damage,
            block: None,
            connects: 0,
            log,
        }
    }
}

impl Transport for Fake {
    fn read(&mut self, buffer: &mut [u8], _timeout: Duration) -> io::Result<usize> {
        if matches!(self.damage, Damage::LateAck)
            && self.block.is_none()
            && !self.pending.is_empty()
        {
            std::thread::sleep(Duration::from_millis(25));
        }
        if matches!(self.damage, Damage::ReadError(block) if self.block == Some(block)) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "simulated disconnect",
            ));
        }
        if matches!(self.damage, Damage::Overflow) && self.block.is_some() {
            return Ok(buffer.len() + 1);
        }
        let count = buffer.len().min(self.fragment).min(self.pending.len());
        for byte in buffer.iter_mut().take(count) {
            *byte = self.pending.pop_front().unwrap();
        }
        Ok(count)
    }
    fn send(&mut self, packet: &ReadPacket, _timeout: Duration) -> io::Result<usize> {
        let packet = packet.as_bytes();
        assert!(is_allowed_packet(packet), "unapproved packet {packet:02x?}");
        self.log.lock().unwrap().packets.push(packet.to_vec());
        if packet == CONNECT {
            self.connects += 1;
            match self.damage {
                Damage::NoAck => {}
                Damage::Ack(byte) => self.pending.push_back(byte),
                Damage::ZeroThenAck if self.connects == 1 => self.pending.push_back(0),
                _ => self.pending.push_back(0x06),
            }
        } else if packet == END {
            if matches!(self.damage, Damage::EndError) {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "END transport failure",
                ));
            }
        } else {
            let block = u16::from_be_bytes([packet[1], packet[2]]);
            self.block = Some(block);
            if matches!(self.damage, Damage::ShortSend(target) if target == block) {
                return Ok(2);
            }
            let mut bytes = frame(block);
            match self.damage {
                Damage::Checksum(target) if target == block => bytes[FRAME_SIZE - 1] ^= 1,
                Damage::Address(target) if target == block => {
                    bytes[2] ^= 1;
                    update_checksum(&mut bytes);
                }
                Damage::Opcode(target) if target == block => {
                    bytes[0] = 0x53;
                    update_checksum(&mut bytes);
                }
                Damage::Changed(target) if target == block => {
                    bytes[3 + 17] ^= 1;
                    update_checksum(&mut bytes);
                }
                Damage::Short(target) if target == block => bytes.truncate(73),
                Damage::Extra(target) if target == block => bytes.push(0xa5),
                _ => {}
            }
            self.pending.extend(bytes);
        }
        Ok(packet.len())
    }
    fn close(&mut self) -> io::Result<()> {
        self.log.lock().unwrap().closes += 1;
        if matches!(self.damage, Damage::CloseError) {
            Err(io::Error::other("close failure"))
        } else {
            Ok(())
        }
    }
}

fn fast_timing() -> Timing {
    Timing {
        connect: Duration::from_millis(3),
        frame: Duration::from_millis(8),
        send: Duration::from_millis(3),
        poll: Duration::from_millis(1),
        gap: Duration::ZERO,
        between_passes: Duration::ZERO,
    }
}

fn run(
    modes: [Damage; 2],
    plan: ReadPlan,
    timing: Timing,
) -> (Result<ReadSnapshot, ReadFailure>, Vec<Arc<Mutex<Log>>>) {
    let logs = vec![
        Arc::new(Mutex::new(Log::default())),
        Arc::new(Mutex::new(Log::default())),
    ];
    let mut index = 0;
    let result = read_twice_timed(
        || {
            let fake = Fake::new(modes[index], 37, logs[index].clone());
            index += 1;
            Ok(fake)
        },
        plan,
        &CancelToken::new(),
        |_| {},
        timing,
    );
    (result, logs)
}

#[test]
fn two_fragmented_full_plans_preserve_every_region_and_only_fixed_packets() {
    let (result, logs) = run(
        [Damage::None; 2],
        ReadPlan::CpsAndCalibration,
        fast_timing(),
    );
    let snapshot = result.unwrap();
    assert_eq!(snapshot.sections().len(), 5);
    assert_eq!(
        snapshot.time_provenance(),
        TimeProvenance::NotReadByStockCps
    );
    assert_eq!(snapshot.plan().block_count(), 188);
    assert_eq!(
        snapshot
            .sections()
            .iter()
            .map(|s| s.bytes.len())
            .sum::<usize>(),
        192_512
    );
    for section in snapshot.sections() {
        assert_eq!(
            section.bytes.len(),
            section.region.block_count() * BLOCK_SIZE
        );
        for (index, chunk) in section.bytes.chunks_exact(BLOCK_SIZE).enumerate() {
            assert_eq!(chunk, payload(section.first_block + index as u16));
        }
    }
    let expected: Vec<Vec<u8>> = std::iter::once(CONNECT.to_vec())
        .chain(
            ReadPlan::CpsAndCalibration
                .blocks()
                .into_iter()
                .map(|block| ReadPacket::block(block).bytes),
        )
        .chain(std::iter::once(END.to_vec()))
        .collect();
    for (capture, log) in snapshot.passes().iter().zip(logs) {
        assert!(capture.complete && capture.end_sent);
        assert!(capture.cleanup_error.is_none());
        assert_eq!(capture.tx_packets, expected);
        assert_eq!(capture.rx.len(), 1 + 188 * FRAME_SIZE);
        assert_eq!(capture.frames.len(), 188);
        assert!(capture
            .frames
            .iter()
            .all(|f| f.validated && f.bytes.len() == FRAME_SIZE));
        assert_eq!(log.lock().unwrap().closes, 1);
    }
    assert_eq!(
        snapshot.clone().section(Region::Calibration),
        snapshot.section(Region::Calibration)
    );
}

#[test]
fn separate_plans_never_add_unrequested_calibration_or_time() {
    let (result, _) = run([Damage::None; 2], ReadPlan::Cps, fast_timing());
    let snapshot = result.unwrap();
    assert_eq!(snapshot.plan().block_count(), 184);
    assert!(snapshot.section(Region::Calibration).is_none());
    assert_eq!(
        snapshot
            .sections()
            .iter()
            .map(|s| s.bytes.len())
            .sum::<usize>(),
        188_416
    );
    let (result, _) = run([Damage::None; 2], ReadPlan::Calibration, fast_timing());
    let calibration = result.unwrap();
    assert_eq!(calibration.sections().len(), 1);
    assert_eq!(
        calibration.section(Region::Calibration).unwrap().len(),
        4096
    );
    assert!(calibration.section(Region::Cfg).is_none());
}

#[test]
fn exhaustive_block_allowlist_and_checksum_reject_other_commands() {
    let expected = 0..4096;
    for block in 0..=u16::MAX {
        let packet = ReadPacket::block(block);
        assert_eq!(
            is_allowed_packet(packet.as_bytes()),
            expected.contains(&block)
        );
        let mut corrupt = packet.bytes;
        corrupt[3] ^= 1;
        assert!(!is_allowed_packet(&corrupt));
    }
    for forbidden in [
        &[0x90, 0, 0, 0x90][..],
        &[0x91, 0, 0, 0x91],
        &[0x92, 0, 0, 0x92],
        &[0x99, 0, 0, 0x99],
        &[0x2a, 0],
        &[0x39, 0x33, 5, 0x55, 0xc6],
        &[0x34, 0x52, 5, 0x0c, 0x97],
        &[],
    ] {
        assert!(!is_allowed_packet(forbidden));
    }
    assert!(is_allowed_packet(&CONNECT) && is_allowed_packet(&END));
}

#[test]
fn wrong_checksum_address_and_signature_abort_without_read_retry() {
    for damage in [Damage::Checksum(1), Damage::Address(1), Damage::Opcode(1)] {
        let (result, logs) = run([damage, Damage::None], ReadPlan::Calibration, fast_timing());
        let failure = result.unwrap_err();
        assert!(matches!(
            failure.error,
            ReadError::Checksum { .. } | ReadError::FrameHeader { .. }
        ));
        let capture = &failure.passes[0];
        assert_eq!(capture.frames.len(), 2);
        assert!(capture.frames[0].validated);
        assert!(!capture.frames[1].validated);
        assert_eq!(capture.frames[1].bytes.len(), FRAME_SIZE);
        assert_eq!(
            capture.tx_packets,
            [
                CONNECT.to_vec(),
                ReadPacket::block(0).bytes,
                ReadPacket::block(1).bytes,
                END.to_vec()
            ]
        );
        assert!(capture.end_sent && !capture.complete);
        assert_eq!(logs[0].lock().unwrap().closes, 1);
        assert_eq!(logs[1].lock().unwrap().closes, 0);
    }
}

#[test]
fn short_frame_timeout_preserves_exact_partial_received_bytes() {
    let (result, _) = run(
        [Damage::Short(1), Damage::None],
        ReadPlan::Calibration,
        fast_timing(),
    );
    let failure = result.unwrap_err();
    assert_eq!(
        failure.error,
        ReadError::Timeout {
            operation: "frame",
            received: 73,
            expected: FRAME_SIZE
        }
    );
    let capture = &failure.passes[0];
    assert_eq!(capture.rx.len(), 1 + FRAME_SIZE + 73);
    assert_eq!(capture.frames[1].bytes, frame(1)[..73]);
    assert!(!capture.frames[1].validated);
    assert!(capture.end_sent);
}

#[test]
fn final_frame_surplus_byte_is_rejected_and_retained_in_rx() {
    let mut timing = fast_timing();
    timing.gap = Duration::from_millis(1);
    let (result, _) = run(
        [Damage::Extra(3), Damage::None],
        ReadPlan::Calibration,
        timing,
    );
    let failure = result.unwrap_err();
    assert_eq!(
        failure.error,
        ReadError::UnexpectedTrailingByte {
            block: 3,
            byte: 0xa5
        }
    );
    assert_eq!(failure.passes[0].rx.last(), Some(&0xa5));
    assert_eq!(failure.passes[0].frames.len(), 4);
    assert!(!failure.passes[0].complete);
}

#[test]
fn valid_but_different_passes_abort_at_first_changed_block() {
    let (result, _) = run(
        [Damage::None, Damage::Changed(1)],
        ReadPlan::Calibration,
        fast_timing(),
    );
    let failure = result.unwrap_err();
    assert_eq!(
        failure.error,
        ReadError::PassMismatch {
            block: 1,
            byte_offset: 17
        }
    );
    assert_eq!(failure.passes.len(), 2);
    assert!(failure.passes[0].complete);
    assert!(!failure.passes[1].complete);
    assert_eq!(failure.passes[1].frames.len(), 2);
    assert!(failure.passes[1].frames.iter().all(|frame| frame.validated));
    assert!(failure.passes.iter().all(|capture| capture.end_sent));
}

#[test]
fn connect_timeout_has_exactly_three_attempts_and_no_end_without_ack() {
    let (result, logs) = run([Damage::NoAck; 2], ReadPlan::Calibration, fast_timing());
    let failure = result.unwrap_err();
    assert_eq!(
        failure.error,
        ReadError::Timeout {
            operation: "connection",
            received: 0,
            expected: 1
        }
    );
    assert_eq!(failure.passes[0].tx_packets, vec![CONNECT.to_vec(); 3]);
    assert!(!failure.passes[0].end_sent);
    assert!(failure.passes[0].frames.is_empty());
    assert_eq!(logs[0].lock().unwrap().closes, 1);
}

#[test]
fn late_ack_is_fatal_but_observed_session_gets_end_and_close() {
    let (result, logs) = run([Damage::LateAck; 2], ReadPlan::Calibration, fast_timing());
    let failure = result.unwrap_err();
    assert_eq!(
        failure.error,
        ReadError::Timeout {
            operation: "connection",
            received: 1,
            expected: 1
        }
    );
    assert_eq!(failure.passes.len(), 1);
    let capture = &failure.passes[0];
    assert_eq!(capture.rx, [0x06]);
    assert_eq!(capture.tx_packets, [CONNECT.to_vec(), END.to_vec()]);
    assert!(capture.frames.is_empty());
    assert!(capture.end_sent);
    assert!(!capture.complete);
    assert_eq!(logs[0].lock().unwrap().closes, 1);
    assert!(logs[1].lock().unwrap().packets.is_empty());
    assert_eq!(logs[1].lock().unwrap().closes, 0);
}

#[test]
fn unknown_ack_is_fatal_but_stock_zero_then_ack_is_bounded_retry() {
    let (result, _) = run([Damage::Ack(0xff); 2], ReadPlan::Calibration, fast_timing());
    let failure = result.unwrap_err();
    assert_eq!(failure.error, ReadError::UnexpectedAck(0xff));
    assert_eq!(failure.passes[0].tx_packets, [CONNECT.to_vec()]);
    let (result, _) = run(
        [Damage::ZeroThenAck; 2],
        ReadPlan::Calibration,
        fast_timing(),
    );
    let snapshot = result.unwrap();
    assert_eq!(&snapshot.passes()[0].rx[..2], &[0, 6]);
    assert_eq!(
        &snapshot.passes()[0].tx_packets[..2],
        &[CONNECT.to_vec(), CONNECT.to_vec()]
    );
}

#[test]
fn short_send_and_receive_io_error_preserve_transcript_and_close() {
    for damage in [Damage::ShortSend(1), Damage::ReadError(1), Damage::Overflow] {
        let (result, logs) = run([damage, Damage::None], ReadPlan::Calibration, fast_timing());
        let failure = result.unwrap_err();
        assert!(matches!(
            failure.error,
            ReadError::ShortSend { .. } | ReadError::Io { .. } | ReadError::TransportOverflow
        ));
        assert_eq!(failure.passes[0].tx_packets.last(), Some(&END.to_vec()));
        assert_eq!(logs[0].lock().unwrap().closes, 1);
    }
}

#[test]
fn cleanup_failure_cannot_publish_a_successful_snapshot() {
    for damage in [Damage::EndError, Damage::CloseError] {
        let (result, logs) = run([damage, Damage::None], ReadPlan::Calibration, fast_timing());
        let failure = result.unwrap_err();
        assert!(matches!(failure.error, ReadError::Io { .. }));
        assert_eq!(failure.passes[0].frames.len(), 4);
        assert!(!failure.passes[0].complete);
        assert!(failure.passes[0].cleanup_error.is_some());
        assert_eq!(logs[0].lock().unwrap().closes, 1);
    }
}

#[test]
fn cancellation_before_open_never_calls_factory() {
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = read_twice_timed::<Fake, _, _>(
        || panic!("must not open"),
        ReadPlan::Calibration,
        &cancel,
        |_| {},
        fast_timing(),
    );
    assert_eq!(result.unwrap_err().error, ReadError::Cancelled);
}

#[test]
fn cancellation_after_a_valid_frame_sends_end_and_retains_partial_pass() {
    let cancel = CancelToken::new();
    let trigger = cancel.clone();
    let log = Arc::new(Mutex::new(Log::default()));
    let result = read_twice_timed(
        || Ok(Fake::new(Damage::None, 11, log.clone())),
        ReadPlan::Calibration,
        &cancel,
        move |event| {
            if event.phase == Phase::Reading && event.completed_blocks == 1 {
                trigger.cancel();
            }
        },
        fast_timing(),
    );
    let failure = result.unwrap_err();
    assert_eq!(failure.error, ReadError::Cancelled);
    assert_eq!(failure.passes[0].frames.len(), 1);
    assert!(failure.passes[0].end_sent);
    assert_eq!(log.lock().unwrap().closes, 1);
}

#[test]
fn second_open_failure_keeps_complete_first_pass() {
    let mut opens = 0;
    let result = read_twice_timed(
        || {
            opens += 1;
            if opens == 2 {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "port busy"))
            } else {
                Ok(Fake::new(
                    Damage::None,
                    1028,
                    Arc::new(Mutex::new(Log::default())),
                ))
            }
        },
        ReadPlan::Calibration,
        &CancelToken::new(),
        |_| {},
        fast_timing(),
    );
    let failure = result.unwrap_err();
    assert!(matches!(
        failure.error,
        ReadError::Io {
            operation: "open port",
            ..
        }
    ));
    assert_eq!(failure.passes.len(), 2);
    assert!(failure.passes[0].complete);
    assert!(failure.passes[1].tx_packets.is_empty());
}

#[test]
fn pure_parser_rejects_short_long_frames_and_disallowed_blocks() {
    for size in [0, 1, 3, 73, FRAME_SIZE - 1, FRAME_SIZE + 1] {
        assert_eq!(
            validate_frame(0, &vec![0; size]),
            Err(ReadError::FrameLength {
                block: 0,
                actual: size
            })
        );
    }
    for block in [4096, 4097, u16::MAX] {
        assert_eq!(
            validate_frame(block, &frame(block)),
            Err(ReadError::InvalidBlock(block))
        );
    }
    assert_eq!(validate_frame(4095, &frame(4095)).unwrap(), payload(4095));
}

#[test]
fn port_names_cannot_inject_paths_or_select_non_com_devices() {
    for (name, expected) in [
        ("COM1", Some(1)),
        ("com20", Some(20)),
        ("COM65535", Some(65535)),
        ("COM0", None),
        ("COM01", None),
        ("COM65536", None),
        ("COM20\\x", None),
        ("\\\\.\\COM20", None),
        ("COM20\0", None),
        ("LPT1", None),
        ("COM", None),
        ("éCOM20", None),
    ] {
        assert_eq!(port_number(name), expected, "{name:?}");
    }
}

#[test]
fn cancellation_mid_frame_keeps_received_fragment_and_closes() {
    struct Cancelling {
        inner: Fake,
        token: CancelToken,
    }
    impl Transport for Cancelling {
        fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> io::Result<usize> {
            let count = self.inner.read(buffer, timeout)?;
            if self.inner.block == Some(1) && count > 0 {
                self.token.cancel();
            }
            Ok(count)
        }
        fn send(&mut self, packet: &ReadPacket, timeout: Duration) -> io::Result<usize> {
            self.inner.send(packet, timeout)
        }
        fn close(&mut self) -> io::Result<()> {
            self.inner.close()
        }
    }
    let cancel = CancelToken::new();
    let log = Arc::new(Mutex::new(Log::default()));
    let result = read_twice_timed(
        || {
            Ok(Cancelling {
                inner: Fake::new(Damage::None, 37, log.clone()),
                token: cancel.clone(),
            })
        },
        ReadPlan::Calibration,
        &cancel,
        |_| {},
        fast_timing(),
    );
    let failure = result.unwrap_err();
    assert_eq!(failure.error, ReadError::Cancelled);
    assert_eq!(failure.passes[0].frames[1].bytes, frame(1)[..37]);
    assert!(!failure.passes[0].frames[1].validated);
    assert!(failure.passes[0].end_sent);
    assert_eq!(log.lock().unwrap().closes, 1);
}

#[test]
fn cancellation_between_passes_prevents_second_open() {
    let cancel = CancelToken::new();
    let trigger = cancel.clone();
    let mut opens = 0;
    let result = read_twice_timed(
        || {
            opens += 1;
            Ok(Fake::new(
                Damage::None,
                1028,
                Arc::new(Mutex::new(Log::default())),
            ))
        },
        ReadPlan::Calibration,
        &cancel,
        move |event| {
            if event.phase == Phase::Closing && event.pass == 1 {
                trigger.cancel();
            }
        },
        fast_timing(),
    );
    let failure = result.unwrap_err();
    assert_eq!(failure.error, ReadError::Cancelled);
    assert_eq!(opens, 1);
    assert!(failure.passes[0].complete && failure.passes[0].end_sent);
}

#[test]
fn stock_storage_plan_reads_and_compares_all_4096_blocks() {
    let (result, _) = run([Damage::None; 2], ReadPlan::FullStorage, fast_timing());
    let snapshot = result.unwrap();
    let storage = snapshot.section(Region::Storage).unwrap();
    assert_eq!(storage.len(), 4 * 1024 * 1024);
    assert_eq!(&storage[4095 * BLOCK_SIZE..], payload(4095));
    assert_eq!(
        snapshot.section(Region::Calibration).unwrap(),
        &storage[..4096]
    );
    assert!(snapshot
        .passes()
        .iter()
        .all(|p| p.complete && p.frames.len() == 4096));
}
