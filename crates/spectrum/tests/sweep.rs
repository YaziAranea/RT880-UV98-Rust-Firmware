use uv98_spectrum::{
    Action, CancelReason, Engine, Event, Measurement, Mode, PeakHold, Range, RangeError, State,
    Ticket, Timing, TimingError, MAX_BINS,
};

fn engine(mode: Mode, peak: PeakHold, settle: u32) -> Engine {
    Engine::new(
        Range::new(14_400_000, 14_400_020, 10).unwrap(),
        Timing::new(50, 40, settle, 30).unwrap(),
        mode,
        peak,
    )
}
fn prepare(engine: &mut Engine, now: u32) -> Ticket {
    match engine.handle(now, Event::Start) {
        Some(Action::PrepareReceive { ticket }) => ticket,
        other => panic!("expected prepare, got {other:?}"),
    }
}
fn tune(action: Option<Action>, expected_index: usize) -> Ticket {
    match action {
        Some(Action::TuneReceive {
            ticket,
            index,
            frequency_10hz,
        }) => {
            assert_eq!(index, expected_index);
            assert_eq!(frequency_10hz, 14_400_000 + expected_index as u32 * 10);
            ticket
        }
        other => panic!("expected tune, got {other:?}"),
    }
}
fn read(action: Option<Action>, expected_index: usize) -> Ticket {
    match action {
        Some(Action::ReadSignal { ticket, index }) => {
            assert_eq!(index, expected_index);
            ticket
        }
        other => panic!("expected read, got {other:?}"),
    }
}
fn assert_cancel(action: Option<Action>, expected: CancelReason) {
    assert!(
        matches!(action, Some(Action::Cancel {reason,..}) if reason == expected),
        "{action:?}"
    );
}

#[test]
fn range_grid_capacity_and_u32_boundaries_are_checked() {
    assert_eq!(Range::new(0, 10, 1), Err(RangeError::ZeroFrequency));
    assert_eq!(Range::new(1, 10, 0), Err(RangeError::ZeroStep));
    assert_eq!(Range::new(10, 1, 1), Err(RangeError::Reversed));
    assert_eq!(Range::new(1, 10, 2), Err(RangeError::EndOffGrid));
    assert_eq!(
        Range::new(1, MAX_BINS as u32 + 1, 1),
        Err(RangeError::TooManyBins)
    );
    assert_eq!(Range::new(1, u32::MAX, 1), Err(RangeError::TooManyBins));
    let full = Range::new(u32::MAX - 239, u32::MAX, 1).unwrap();
    assert_eq!(full.len(), 240);
    assert_eq!(full.frequency_10hz(239), Some(u32::MAX));
    assert_eq!(full.frequency_10hz(240), None);
    assert_eq!(full.frequency_10hz(usize::MAX), None);
    let single = Range::new(u32::MAX, u32::MAX, u32::MAX).unwrap();
    assert_eq!(single.len(), 1);
    assert_eq!(single.frequency_10hz(0), Some(u32::MAX));
    let wide = Range::new(1, u32::MAX, u32::MAX - 1).unwrap();
    assert_eq!(wide.frequency_10hz(1), Some(u32::MAX));
}

#[test]
fn timing_has_explicit_finite_deadlines_and_zero_settle_still_needs_ack() {
    for timing in [
        Timing::new(0, 1, 0, 1),
        Timing::new(1, 0, 0, 1),
        Timing::new(1, 1, 0, 0),
    ] {
        assert_eq!(timing, Err(TimingError::ZeroTimeout));
    }
    for timing in [
        Timing::new(1 << 31, 1, 0, 1),
        Timing::new(1, 1 << 31, 0, 1),
        Timing::new(1, 1, 1 << 31, 1),
        Timing::new(1, 1, 0, 1 << 31),
    ] {
        assert_eq!(timing, Err(TimingError::TooLong));
    }
    let mut e = engine(Mode::Once, PeakHold::Off, 0);
    let p = prepare(&mut e, 0);
    assert_eq!(e.handle(1, Event::Poll), None);
    let t = tune(e.handle(2, Event::Ready(p)), 0);
    assert_eq!(e.handle(3, Event::Poll), None);
    assert_eq!(e.state(), State::AwaitSettled);
    read(e.handle(4, Event::Settled(t)), 0);
}

#[test]
fn no_missing_ack_is_replaced_by_polling_or_sample_data() {
    let mut e = engine(Mode::Once, PeakHold::Off, 5);
    let p = prepare(&mut e, 0);
    for now in [0, 10, 20, 49] {
        assert_eq!(e.handle(now, Event::Poll), None);
    }
    assert_cancel(e.handle(50, Event::Ready(p)), CancelReason::ReadyTimeout);
    assert_eq!(e.handle(51, Event::Ready(p)), None);
    let p = prepare(&mut e, 100);
    let t = tune(e.handle(100, Event::Ready(p)), 0);
    assert_eq!(
        e.handle(
            101,
            Event::Sample {
                ticket: t,
                raw: 999
            }
        ),
        None
    );
    assert_eq!(e.handle(139, Event::Poll), None);
    assert_cancel(
        e.handle(140, Event::Settled(t)),
        CancelReason::SettledTimeout,
    );
    assert!(e.bins().iter().all(|b| b.latest.is_none()));
}

#[test]
fn early_settled_waits_until_minimum_and_repeated_poll_never_retunes() {
    let mut e = engine(Mode::Once, PeakHold::Off, 10);
    let p = prepare(&mut e, 100);
    let t = tune(e.handle(101, Event::Ready(p)), 0);
    assert_eq!(e.handle(103, Event::Settled(t)), None);
    assert_eq!(e.state(), State::Settling);
    for now in [103, 104, 110] {
        assert_eq!(e.handle(now, Event::Poll), None);
    }
    assert_eq!(
        e.handle(
            110,
            Event::Sample {
                ticket: t,
                raw: 111
            }
        ),
        None
    );
    let r = read(e.handle(111, Event::Poll), 0);
    assert_ne!(r, t);
    assert_eq!(e.handle(111, Event::Poll), None);
    assert_eq!(e.handle(112, Event::Settled(t)), None);
    assert_eq!(e.handle(112, Event::Poll), None);
    assert_cancel(
        e.handle(
            141,
            Event::Sample {
                ticket: r,
                raw: 222,
            },
        ),
        CancelReason::SampleTimeout,
    );
    assert_eq!(e.bins()[0].latest, None);
}

#[test]
fn completed_once_sweep_preserves_unscaled_values_and_waits_for_start() {
    let mut e = engine(Mode::Once, PeakHold::On, 0);
    let p = prepare(&mut e, 0);
    let mut t = tune(e.handle(0, Event::Ready(p)), 0);
    for (index, raw) in [0, 0xffff, 513].into_iter().enumerate() {
        let r = read(e.handle(1 + index as u32 * 2, Event::Settled(t)), index);
        let result = e.handle(2 + index as u32 * 2, Event::Sample { ticket: r, raw });
        assert_eq!(e.bins()[index].latest, Some(Measurement { raw, sweep: 1 }));
        assert_eq!(e.bins()[index].peak, Some(raw));
        if index < 2 {
            t = tune(result, index + 1);
        } else {
            assert_eq!(result, Some(Action::SweepComplete { sweep: 1 }));
        }
        assert_eq!(
            e.handle(2 + index as u32 * 2, Event::Sample { ticket: r, raw: 5 }),
            None
        );
    }
    assert_eq!(e.state(), State::SweepComplete);
    assert!(!e.is_running());
    assert_eq!(e.handle(10_000, Event::Poll), None);
    assert!(matches!(
        e.handle(10_001, Event::Start),
        Some(Action::PrepareReceive { .. })
    ));
}

#[test]
fn continuous_sweep_is_explicitly_polled_and_old_bins_keep_their_generation() {
    let mut e = engine(Mode::Continuous, PeakHold::On, 0);
    let p = prepare(&mut e, 0);
    let mut t = tune(e.handle(0, Event::Ready(p)), 0);
    for index in 0..3 {
        let r = read(e.handle(0, Event::Settled(t)), index);
        let next = e.handle(
            0,
            Event::Sample {
                ticket: r,
                raw: 100 + index as u16,
            },
        );
        if index < 2 {
            t = tune(next, index + 1);
        } else {
            assert_eq!(next, Some(Action::SweepComplete { sweep: 1 }));
        }
    }
    assert_eq!(e.sweep(), 1);
    assert!(e.is_running());
    let t = tune(e.handle(1, Event::Poll), 0);
    assert_eq!(e.sweep(), 2);
    assert_eq!(e.bins()[0].latest.unwrap().sweep, 1);
    let r = read(e.handle(1, Event::Settled(t)), 0);
    tune(e.handle(1, Event::Sample { ticket: r, raw: 20 }), 1);
    assert_eq!(e.bins()[0].latest, Some(Measurement { raw: 20, sweep: 2 }));
    assert_eq!(e.bins()[0].peak, Some(100));
    assert_eq!(e.bins()[1].latest.unwrap().sweep, 1);
    assert_eq!(e.handle(1, Event::ResetPeaks), None);
    assert!(e.bins().iter().all(|b| b.peak.is_none()));
    assert_eq!(e.bins()[0].latest.unwrap().raw, 20);
}

#[test]
fn stop_and_held_ptt_cancel_before_clock_validation_and_never_restart() {
    for event in [Event::Stop, Event::Ptt(true)] {
        let mut e = engine(Mode::Continuous, PeakHold::Off, 5);
        let p = prepare(&mut e, 100);
        let t = tune(e.handle(101, Event::Ready(p)), 0);
        let reason = if event == Event::Stop {
            CancelReason::Stop
        } else {
            CancelReason::Ptt
        };
        assert_eq!(
            e.handle(0, event),
            Some(Action::Cancel {
                ticket: Some(t),
                reason
            })
        );
        assert_eq!(e.handle(1, Event::Settled(t)), None);
        assert_eq!(e.handle(2, Event::Poll), None);
        assert_eq!(e.handle(3, event), None);
        if event == Event::Ptt(true) {
            assert!(e.ptt_held());
            assert_cancel(e.handle(4, Event::Start), CancelReason::Ptt);
            assert_eq!(e.handle(5, Event::Ptt(false)), None);
            assert_eq!(e.handle(6, Event::Poll), None);
        }
        assert!(matches!(
            e.handle(7, Event::Start),
            Some(Action::PrepareReceive { .. })
        ));
    }
}

#[test]
fn stop_and_ptt_cover_every_active_phase() {
    for phase in 0..5 {
        let mut e = engine(Mode::Continuous, PeakHold::Off, 5);
        let p = prepare(&mut e, 0);
        if phase >= 1 {
            let t = tune(e.handle(0, Event::Ready(p)), 0);
            if phase >= 2 {
                assert_eq!(e.handle(0, Event::Settled(t)), None);
                if phase >= 3 {
                    let r = read(e.handle(5, Event::Poll), 0);
                    if phase == 4 {
                        tune(e.handle(5, Event::Sample { ticket: r, raw: 1 }), 1);
                    }
                }
            }
        }
        assert_cancel(e.handle(u32::MAX, Event::Ptt(true)), CancelReason::Ptt);
        assert!(!e.is_running());
    }
}

#[test]
fn current_fault_cancels_and_late_events_from_cancelled_run_are_ignored() {
    let mut e = engine(Mode::Once, PeakHold::Off, 0);
    let old = prepare(&mut e, 0);
    assert_cancel(
        e.handle(
            1,
            Event::Failed {
                ticket: old,
                code: 7,
            },
        ),
        CancelReason::Hardware { code: 7 },
    );
    let p = prepare(&mut e, 2);
    assert_ne!(p, old);
    assert_eq!(e.handle(3, Event::Ready(old)), None);
    assert_eq!(
        e.handle(
            3,
            Event::Failed {
                ticket: old,
                code: 8
            }
        ),
        None
    );
    let t = tune(e.handle(4, Event::Ready(p)), 0);
    let r = read(e.handle(5, Event::Settled(t)), 0);
    assert_cancel(
        e.handle(
            6,
            Event::Failed {
                ticket: r,
                code: 0xdead,
            },
        ),
        CancelReason::Hardware { code: 0xdead },
    );
    assert_eq!(e.handle(7, Event::Sample { ticket: r, raw: 90 }), None);
    assert_eq!(e.bins()[0].latest, None);
}

#[test]
fn clock_wrap_is_supported_and_backward_or_ambiguous_time_cancels() {
    let mut e = engine(Mode::Once, PeakHold::Off, 5);
    let p = prepare(&mut e, u32::MAX - 3);
    let t = tune(e.handle(u32::MAX - 2, Event::Ready(p)), 0);
    assert_eq!(e.handle(u32::MAX - 1, Event::Settled(t)), None);
    assert_eq!(e.handle(0, Event::Poll), None);
    read(e.handle(2, Event::Poll), 0);
    assert_cancel(e.handle(1, Event::Poll), CancelReason::Clock);
    let p = prepare(&mut e, 10);
    assert_cancel(
        e.handle(10 + (1 << 31), Event::Ready(p)),
        CancelReason::Clock,
    );
}

#[test]
fn no_peak_history_is_invented_when_disabled_or_on_a_missing_sample() {
    let mut e = Engine::new(
        Range::new(1, 1, 1).unwrap(),
        Timing::new(10, 10, 0, 10).unwrap(),
        Mode::Continuous,
        PeakHold::Off,
    );
    assert!(e.bins()[0].latest.is_none());
    let p = prepare(&mut e, 0);
    let Some(Action::TuneReceive { ticket: t, .. }) = e.handle(0, Event::Ready(p)) else {
        panic!("tune")
    };
    let r = read(e.handle(0, Event::Settled(t)), 0);
    assert_eq!(
        e.handle(
            0,
            Event::Sample {
                ticket: r,
                raw: 0xffff
            }
        ),
        Some(Action::SweepComplete { sweep: 1 })
    );
    assert_eq!(e.bins()[0].peak, None);
    assert_eq!(e.bins()[0].latest.unwrap().raw, 0xffff);
    assert_cancel(e.handle(1, Event::Stop), CancelReason::Stop);
    assert_eq!(e.bins()[0].latest.unwrap().sweep, 1);
}
