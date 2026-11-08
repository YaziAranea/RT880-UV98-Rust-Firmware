//! Independent behavioral tests: no hardware, GPIO backend or timing claims.
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use uv98_calibration::Calibration;
use uv98_receive::{
    executor::Io,
    runtime::{Runtime, RuntimeIo, State, Timing, Volume},
    Chip, Plan, Width,
};

const CAL: &[u8] = include_bytes!("../../calibration/tests/fixtures/uv98-plus-stock-1.16.bin");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Call {
    Guard,
    Read(Chip, u8),
    Write(Chip, u8, u16),
    Antenna(bool),
    Mute(bool),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MockError {
    Injected(usize),
    Ptt,
    Mute,
}
#[derive(Default)]
struct MockState {
    calls: Vec<Call>,
    fail_at: Option<usize>,
    ptt_on_guard_at: Option<usize>,
    ptt: bool,
    ptt_after_read: Option<u8>,
    fail_mute: bool,
    reg33: VecDeque<u16>,
    reg01: u16,
    reg67: u16,
    reg0c: u16,
}
type Shared = Rc<RefCell<MockState>>;
struct Mock(Shared);
impl Mock {
    fn call(&mut self, call: Call) -> Result<(), MockError> {
        let mut s = self.0.borrow_mut();
        let index = s.calls.len();
        s.calls.push(call);
        if s.fail_at == Some(index) {
            Err(MockError::Injected(index))
        } else {
            Ok(())
        }
    }
}
impl Io for Mock {
    type Error = MockError;
    fn check_receive_preconditions(&mut self) -> Result<(), MockError> {
        self.call(Call::Guard)?;
        {
            let mut s = self.0.borrow_mut();
            if s.ptt_on_guard_at == Some(s.calls.len() - 1) {
                s.ptt = true;
            }
        }
        if self.0.borrow().ptt {
            Err(MockError::Ptt)
        } else {
            Ok(())
        }
    }
    fn read(&mut self, chip: Chip, register: u8) -> Result<u16, MockError> {
        self.call(Call::Read(chip, register))?;
        assert_eq!(chip, Chip::A, "runtime must not invent a B RSSI path");
        let mut s = self.0.borrow_mut();
        let value = match register {
            0x33 => s.reg33.pop_front().unwrap_or(0),
            0x01 => s.reg01,
            0x67 => s.reg67,
            0x0c => s.reg0c,
            _ => panic!("unexpected register read {register:02x}"),
        };
        if s.ptt_after_read == Some(register) {
            s.ptt = true;
        }
        Ok(value)
    }
    fn write(&mut self, chip: Chip, register: u8, value: u16) -> Result<(), MockError> {
        self.call(Call::Write(chip, register, value))
    }
    fn set_antenna_pf0(&mut self, high: bool) -> Result<(), MockError> {
        self.call(Call::Antenna(high))
    }
}
impl RuntimeIo for Mock {
    fn set_audio_mute(&mut self, muted: bool) -> Result<(), MockError> {
        self.call(Call::Mute(muted))?;
        if self.0.borrow().fail_mute {
            Err(MockError::Mute)
        } else {
            Ok(())
        }
    }
}
fn plan(level: u8) -> Plan {
    Plan::primary(
        &Calibration::parse(CAL).unwrap(),
        14_550_000,
        Width::Wide,
        level,
    )
    .unwrap()
}
fn make(timing: Timing) -> (Runtime<Mock>, Shared) {
    let shared = Rc::new(RefCell::new(MockState::default()));
    (Runtime::new(Mock(shared.clone()), timing), shared)
}
fn normal() -> (Runtime<Mock>, Shared) {
    make(Timing::new(1_000, 100, 5).unwrap())
}
fn drive_until(r: &mut Runtime<Mock>, now: &mut u32, target: State) {
    for _ in 0..100 {
        if r.state() == target || matches!(r.state(), State::Faulted | State::Stopped) {
            return;
        }
        *now = now.wrapping_add(1);
        let _ = r.step(*now);
    }
    panic!("runtime did not reach {target:?}: {:?}", r.state());
}
fn listening(level: u8) -> (Runtime<Mock>, Shared, u32) {
    let (mut r, shared) = normal();
    let mut now = 0;
    let ticket = r.start(plan(level), now).unwrap();
    drive_until(&mut r, &mut now, State::AwaitSettling);
    assert_eq!(r.state(), State::AwaitSettling);
    r.step(now).unwrap(); // Post-write observation establishes minimum baseline.
    now += 5;
    r.acknowledge_settled(ticket, now).unwrap();
    assert_eq!(r.state(), State::Listening);
    (r, shared, now)
}

#[test]
fn constructor_and_start_are_inert_and_tx_check_precedes_any_configuration_write() {
    let (mut r, shared) = normal();
    assert!(shared.borrow().calls.is_empty());
    r.start(plan(5), 0).unwrap();
    assert!(shared.borrow().calls.is_empty());
    r.step(0).unwrap();
    let calls = shared.borrow().calls.clone();
    assert_eq!(calls, [Call::Guard, Call::Read(Chip::A, 0x33)]);
    for tx in [4, 8, 12, 0xffff] {
        let (mut r, shared) = normal();
        shared.borrow_mut().reg33.push_back(tx);
        r.start(plan(5), 0).unwrap();
        assert!(r.step(0).is_err());
        assert_eq!(r.state(), State::Faulted);
        assert!(!shared
            .borrow()
            .calls
            .iter()
            .any(|c| matches!(c, Call::Write(..) | Call::Antenna(_) | Call::Mute(false))));
    }
}

#[test]
fn no_sample_before_both_startup_and_current_settled_ack_and_minimum_delay() {
    let (mut r, shared) = normal();
    let ticket = r.start(plan(5), 0).unwrap();
    assert!(!r.acknowledge_settled(ticket, 0).unwrap());
    assert!(r.sample(0).is_err());
    assert!(shared.borrow().calls.is_empty());
    let mut now = 0;
    drive_until(&mut r, &mut now, State::AwaitSettling);
    assert_eq!(r.state(), State::AwaitSettling);
    r.acknowledge_settled(ticket, now).unwrap();
    assert_eq!(r.state(), State::AwaitSettling);
    assert!(r.sample(now).is_err());
    r.step(now + 4).unwrap();
    assert_eq!(r.state(), State::AwaitSettling);
    r.step(now + 5).unwrap();
    assert_eq!(r.state(), State::Listening);
    assert!(!shared.borrow().calls.contains(&Call::Read(Chip::A, 0x67)));
    r.sample(now + 5).unwrap();
    assert!(shared.borrow().calls.contains(&Call::Read(Chip::A, 0x67)));
}

#[test]
fn elapsed_time_alone_and_old_ack_cannot_prepare_a_new_tune() {
    let (mut r, shared, now) = listening(5);
    let old = r.ticket().unwrap();
    let current = r.start(plan(5), now).unwrap();
    assert_ne!(old, current);
    let mut now = now;
    drive_until(&mut r, &mut now, State::AwaitSettling);
    r.step(now).unwrap(); // Arm after last W30, not before that write.
    r.step(now + 5).unwrap();
    assert_eq!(r.state(), State::AwaitSettling);
    assert!(!r.acknowledge_settled(old, now + 5).unwrap());
    assert_eq!(r.state(), State::AwaitSettling);
    assert!(r.sample(now + 5).is_err());
    assert!(!shared.borrow().calls.contains(&Call::Read(Chip::A, 0x67)));
    r.acknowledge_settled(current, now + 5).unwrap();
    assert_eq!(r.state(), State::Listening);
}

#[test]
fn nominal_ten_millisecond_start_delay_has_an_exact_boundary_and_wraps() {
    let (mut r, shared) = normal();
    let mut now = u32::MAX - 15;
    r.start(plan(5), now).unwrap();
    drive_until(&mut r, &mut now, State::PowerDelay);
    assert_eq!(r.state(), State::PowerDelay);
    let begin = now;
    r.step(begin).unwrap(); // First post-W37 observation arms the full delay.
    r.step(begin.wrapping_add(9)).unwrap();
    assert_eq!(r.state(), State::PowerDelay);
    assert!(!shared
        .borrow()
        .calls
        .iter()
        .any(|c| matches!(c, Call::Write(_, 0x30, _))));
    now = begin.wrapping_add(10);
    r.step(now).unwrap();
    assert_ne!(r.state(), State::PowerDelay);
    drive_until(&mut r, &mut now, State::AwaitSettling);
    let writes: Vec<_> = shared
        .borrow()
        .calls
        .iter()
        .filter_map(|c| match *c {
            Call::Write(Chip::A, reg @ (0x37 | 0x30), value) => Some((reg, value)),
            _ => None,
        })
        .collect();
    assert_eq!(writes, [(0x37, 0x1f0f), (0x30, 0x0200), (0x30, 0xbff1)]);
}

#[test]
fn ptt_during_rssi_read_discards_result_and_mutes_without_next_read_or_resume() {
    let (mut r, shared, now) = listening(5);
    let prefix = shared.borrow().calls.len();
    shared.borrow_mut().ptt_after_read = Some(0x67);
    assert!(r.sample(now).is_err());
    assert_eq!(r.state(), State::Faulted);
    let tail = shared.borrow().calls[prefix..].to_vec();
    assert!(tail.contains(&Call::Read(Chip::A, 0x67)));
    assert!(!tail.contains(&Call::Read(Chip::A, 0x0c)));
    assert_eq!(tail.last(), Some(&Call::Mute(true)));
    let calls = shared.borrow().calls.clone();
    assert!(r.sample(now).is_err());
    assert!(r.start(plan(5), now).is_err());
    let _ = r.step(now);
    r.stop();
    assert_eq!(shared.borrow().calls, calls);
}

#[test]
fn rssi_mask_and_squelch_match_independent_stock_oracle_without_software_override() {
    let (mut r, shared, now) = listening(5);
    for line in include_str!("../../../research/rx-runtime/rssi.csv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split(',').collect();
        let reg: u16 = fields[0].parse().unwrap();
        let expected: u16 = fields[1].parse().unwrap();
        shared.borrow_mut().reg67 = reg;
        assert_eq!(
            r.sample(now).unwrap().raw_rssi,
            expected,
            "register {reg:04x}"
        );
    }
    for line in include_str!("../../../research/rx-runtime/squelch-status.csv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split(',').collect();
        let level: u8 = fields[0].parse().unwrap();
        let flag: u8 = fields[1].parse().unwrap();
        if flag != 0 || level > 10 {
            continue;
        }
        let reg: u16 = fields[2].parse().unwrap();
        let reads: usize = fields[3].parse().unwrap();
        let expected: u8 = fields[4].parse().unwrap();
        let (mut r, shared, now) = listening(level);
        shared.borrow_mut().reg0c = reg;
        let before = shared.borrow().calls.len();
        assert_eq!(r.sample(now).unwrap().squelch_open, expected != 0);
        let observed = shared.borrow().calls[before..]
            .iter()
            .filter(|c| **c == Call::Read(Chip::A, 0x0c))
            .count();
        assert_eq!(observed, reads, "level {level}, reg0c {reg:04x}");
    }
}

#[test]
fn audio_volume_validates_before_io_and_matches_stock_fragment_for_supported_fields() {
    assert!(Volume::new(64, 0).is_err());
    assert!(Volume::new(0, 16).is_err());
    let rows: Vec<_> = include_str!("../../../research/rx-runtime/fm-audio.csv")
        .lines()
        .skip(1)
        .collect();
    assert_eq!(rows.len() % 3, 0);
    let mut checked = 0;
    for chunk in rows.chunks_exact(3) {
        let f: Vec<_> = chunk[0].split(',').collect();
        let gain: u8 = f[1].parse().unwrap();
        let dac: u8 = f[2].parse().unwrap();
        if gain > 63 || dac > 15 {
            continue;
        }
        let reg01: u16 = f[3].parse().unwrap();
        let expected: u16 = chunk[2].split(',').nth(7).unwrap().parse().unwrap();
        let (mut r, shared, now) = listening(5);
        shared.borrow_mut().reg01 = reg01;
        let before = shared.borrow().calls.len();
        r.open_fm_audio(Volume::new(gain, dac).unwrap(), now)
            .unwrap();
        let tail = shared.borrow().calls[before..].to_vec();
        let rf: Vec<_> = tail
            .iter()
            .filter(|c| matches!(c, Call::Write(..) | Call::Read(..)))
            .copied()
            .collect();
        assert_eq!(
            rf,
            [
                Call::Write(Chip::A, 0x47, 0x6140),
                Call::Read(Chip::A, 0x01),
                Call::Write(Chip::A, 0x48, expected)
            ]
        );
        assert!(tail.contains(&Call::Mute(true)));
        assert!(tail.contains(&Call::Mute(false)));
        assert!(r.progress().audio_open);
        checked += 1;
    }
    assert!(
        checked >= 9,
        "oracle must cover multiple gain/dac/readback values"
    );
}

#[test]
fn stop_is_terminal_at_each_startup_state_and_does_not_repeat_cleanup() {
    for target in [
        State::Idle,
        State::Preflight,
        State::MuteOutput,
        State::MuteInternal,
        State::Configuring,
        State::PowerOn,
        State::PowerDelay,
        State::StartLow,
        State::StartHigh,
        State::AwaitSettling,
        State::Listening,
    ] {
        let (mut r, shared) = normal();
        let mut now = 0;
        if target != State::Idle {
            let ticket = r.start(plan(5), now).unwrap();
            if target == State::Listening {
                drive_until(&mut r, &mut now, State::AwaitSettling);
                r.step(now).unwrap();
                now += 5;
                r.acknowledge_settled(ticket, now).unwrap();
            } else {
                drive_until(&mut r, &mut now, target);
            }
        }
        assert_eq!(r.state(), target);
        let prefix = shared.borrow().calls.len();
        r.stop();
        assert_eq!(r.state(), State::Stopped);
        assert_eq!(&shared.borrow().calls[prefix..], [Call::Mute(true)]);
        let all = shared.borrow().calls.clone();
        assert!(r.start(plan(5), now).is_err());
        assert!(r.sample(now).is_err());
        let _ = r.step(now);
        r.stop();
        assert_eq!(shared.borrow().calls, all, "target {target:?}");
    }
}

#[test]
fn every_startup_io_failure_stops_at_that_prefix_then_only_attempts_mute_cleanup() {
    let (mut baseline, shared) = normal();
    let mut now = 0;
    baseline.start(plan(5), now).unwrap();
    drive_until(&mut baseline, &mut now, State::AwaitSettling);
    let calls = shared.borrow().calls.clone();
    for fail_at in 0..calls.len() {
        let (mut r, shared) = normal();
        shared.borrow_mut().fail_at = Some(fail_at);
        let mut now = 0;
        r.start(plan(5), now).unwrap();
        drive_until(&mut r, &mut now, State::AwaitSettling);
        assert_eq!(r.state(), State::Faulted, "failure {fail_at}");
        assert!(r.fault().is_some());
        let observed = shared.borrow().calls.clone();
        assert_eq!(&observed[..=fail_at], &calls[..=fail_at]);
        assert_eq!(
            &observed[fail_at + 1..],
            [Call::Mute(true)],
            "failure {fail_at}"
        );
        assert!(r.start(plan(5), now).is_err());
        assert!(r.sample(now).is_err());
        let _ = r.step(now);
        r.stop();
        assert_eq!(shared.borrow().calls, observed);
    }
}

#[test]
fn guard_failure_before_each_operation_allows_only_terminal_mute_cleanup() {
    let (mut baseline, shared) = normal();
    let mut now = 0;
    baseline.start(plan(5), now).unwrap();
    drive_until(&mut baseline, &mut now, State::AwaitSettling);
    let calls = shared.borrow().calls.clone();
    for (fail_at, call) in calls.iter().enumerate() {
        if *call != Call::Guard {
            continue;
        }
        let (mut r, shared) = normal();
        shared.borrow_mut().ptt_on_guard_at = Some(fail_at);
        let mut now = 0;
        r.start(plan(5), now).unwrap();
        drive_until(&mut r, &mut now, State::AwaitSettling);
        assert_eq!(r.state(), State::Faulted);
        assert_eq!(&shared.borrow().calls[fail_at + 1..], [Call::Mute(true)]);
    }
}

#[test]
fn operation_and_settle_deadlines_reject_equality_and_backward_clock() {
    let (mut r, shared) = make(Timing::new(50, 100, 5).unwrap());
    r.start(plan(5), 10).unwrap();
    assert!(r.step(60).is_err());
    assert_eq!(r.state(), State::Faulted);
    assert_eq!(shared.borrow().calls, [Call::Mute(true)]);
    let (mut r, _) = normal();
    r.start(plan(5), 10).unwrap();
    assert!(r.step(9).is_err());
    assert_eq!(r.state(), State::Faulted);
    let (mut r, _) = normal();
    let ticket = r.start(plan(5), 0).unwrap();
    let mut now = 0;
    drive_until(&mut r, &mut now, State::AwaitSettling);
    r.step(now).unwrap();
    assert!(r.acknowledge_settled(ticket, now + 100).is_err());
    assert_eq!(r.state(), State::Faulted);
}

#[test]
fn mute_cleanup_failure_is_retained_separately_and_never_retried() {
    let (mut r, shared, now) = listening(5);
    shared.borrow_mut().ptt = true;
    shared.borrow_mut().fail_mute = true;
    assert!(r.sample(now).is_err());
    let fault = r.fault().unwrap();
    assert_eq!(fault.cleanup_error, Some(MockError::Mute));
    assert!(format!("{:?}", fault.reason).contains("Ptt"));
    let all = shared.borrow().calls.clone();
    r.stop();
    let _ = r.step(now);
    assert_eq!(shared.borrow().calls, all);
}

#[test]
fn every_sampling_and_audio_fault_or_ptt_guard_stops_the_remaining_sequence() {
    // Each intent is independently reached through the complete RX handshake.
    // A failing operation may have partially applied; only mute cleanup follows.
    for intent in 0..3 {
        let (mut baseline, shared, now) = listening(5);
        let prefix = shared.borrow().calls.len();
        match intent {
            0 => {
                baseline.sample(now).unwrap();
            }
            1 => baseline
                .open_fm_audio(Volume::new(30, 4).unwrap(), now)
                .unwrap(),
            _ => baseline.mute_audio(now).unwrap(),
        }
        let tail = shared.borrow().calls[prefix..].to_vec();
        for ptt in [false, true] {
            for (index, call) in tail.iter().enumerate() {
                if ptt && *call != Call::Guard {
                    continue;
                }
                let (mut r, shared, now) = listening(5);
                let prefix = shared.borrow().calls.len();
                if ptt {
                    shared.borrow_mut().ptt_on_guard_at = Some(prefix + index);
                } else {
                    shared.borrow_mut().fail_at = Some(prefix + index);
                }
                let result = match intent {
                    0 => r.sample(now).map(|_| ()),
                    1 => r.open_fm_audio(Volume::new(30, 4).unwrap(), now),
                    _ => r.mute_audio(now),
                };
                assert!(result.is_err(), "intent {intent}, step {index}, ptt {ptt}");
                assert_eq!(r.state(), State::Faulted);
                let actual = shared.borrow().calls[prefix..].to_vec();
                assert_eq!(&actual[..=index], &tail[..=index]);
                assert_eq!(&actual[index + 1..], [Call::Mute(true)]);
                let all = shared.borrow().calls.clone();
                r.stop();
                assert!(r.open_fm_audio(Volume::new(0, 0).unwrap(), now).is_err());
                assert!(r.sample(now).is_err());
                assert_eq!(shared.borrow().calls, all);
            }
        }
    }
}

#[test]
fn ptt_during_audio_read_prevents_volume_write_and_external_unmute() {
    let (mut r, shared, now) = listening(5);
    let prefix = shared.borrow().calls.len();
    shared.borrow_mut().ptt_after_read = Some(0x01);
    assert!(r.open_fm_audio(Volume::new(63, 15).unwrap(), now).is_err());
    let tail = shared.borrow().calls[prefix..].to_vec();
    assert!(tail.contains(&Call::Read(Chip::A, 0x01)));
    assert!(!tail
        .iter()
        .any(|c| matches!(c, Call::Write(_, 0x48, _) | Call::Mute(false))));
    assert_eq!(tail.last(), Some(&Call::Mute(true)));
    assert_eq!(r.state(), State::Faulted);
}

#[test]
fn failed_stop_mute_retains_error_and_is_terminal_even_from_idle() {
    let (mut r, shared) = normal();
    shared.borrow_mut().fail_mute = true;
    r.stop();
    assert_eq!(r.state(), State::Faulted);
    assert_eq!(shared.borrow().calls, [Call::Mute(true)]);
    let fault = r.fault().unwrap();
    assert!(format!("{:?}", fault.reason).contains("Mute"));
    assert_eq!(fault.cleanup_error, None);
    assert!(r.start(plan(5), 0).is_err());
    r.stop();
    assert_eq!(shared.borrow().calls, [Call::Mute(true)]);
}

#[test]
fn timing_is_bounded_and_invalid_settle_cannot_construct_a_runtime_policy() {
    for operation in [0, 1, 10, 1 << 31, u32::MAX] {
        assert!(Timing::new(operation, 100, 0).is_err());
    }
    for settling in [0, 1 << 31, u32::MAX] {
        assert!(Timing::new(100, settling, 0).is_err());
    }
    for minimum in [100, 101, 1 << 31, u32::MAX] {
        assert!(Timing::new(100, 100, minimum).is_err());
    }
    assert!(Timing::new(11, 1, 0).is_ok());
}

#[test]
fn delayed_first_post_write_observation_does_not_spend_the_new_delay_in_advance() {
    let (mut r, shared) = normal();
    let ticket = r.start(plan(5), 0).unwrap();
    let mut now = 0;
    drive_until(&mut r, &mut now, State::PowerDelay);
    // The call's input timestamp preceded W37. Delayed foreground scheduling
    // cannot be used to infer how much time actually elapsed after that write.
    now += 400;
    r.step(now).unwrap();
    assert_eq!(r.state(), State::PowerDelay);
    r.step(now + 9).unwrap();
    assert_eq!(r.state(), State::PowerDelay);
    assert!(!shared
        .borrow()
        .calls
        .iter()
        .any(|c| matches!(c, Call::Write(_, 0x30, _))));
    now += 10;
    r.step(now).unwrap();
    assert_eq!(r.state(), State::StartLow);
    drive_until(&mut r, &mut now, State::AwaitSettling);
    now += 400;
    r.acknowledge_settled(ticket, now).unwrap();
    assert_eq!(r.state(), State::AwaitSettling);
    assert!(r.sample(now).is_err());
    r.step(now + 4).unwrap();
    assert_eq!(r.state(), State::AwaitSettling);
    r.step(now + 5).unwrap();
    assert_eq!(r.state(), State::Listening);
}

#[test]
fn dropping_active_runtime_attempts_mute_once_but_idle_and_terminal_drop_are_inert() {
    let (idle, shared) = normal();
    drop(idle);
    assert!(shared.borrow().calls.is_empty());
    for target in [
        State::Preflight,
        State::PowerDelay,
        State::AwaitSettling,
        State::Listening,
    ] {
        let (mut r, shared) = normal();
        let mut now = 0;
        let ticket = r.start(plan(5), now).unwrap();
        if target == State::Listening {
            drive_until(&mut r, &mut now, State::AwaitSettling);
            r.step(now).unwrap();
            r.acknowledge_settled(ticket, now + 5).unwrap();
        } else {
            drive_until(&mut r, &mut now, target);
        }
        assert_eq!(r.state(), target);
        let prefix = shared.borrow().calls.len();
        drop(r);
        assert_eq!(
            &shared.borrow().calls[prefix..],
            [Call::Mute(true)],
            "{target:?}"
        );
    }
    for faulted in [false, true] {
        let (mut r, shared, now) = listening(5);
        if faulted {
            shared.borrow_mut().ptt = true;
            shared.borrow_mut().fail_mute = true;
            assert!(r.sample(now).is_err());
            assert_eq!(r.state(), State::Faulted);
        } else {
            r.stop();
            assert_eq!(r.state(), State::Stopped);
        }
        let calls = shared.borrow().calls.clone();
        drop(r);
        assert_eq!(shared.borrow().calls, calls);
    }
    let (mut r, shared) = normal();
    r.start(plan(5), 0).unwrap();
    shared.borrow_mut().fail_mute = true;
    drop(r); // Best effort cannot report this failure, but must not retry.
    assert_eq!(shared.borrow().calls, [Call::Mute(true)]);
}
