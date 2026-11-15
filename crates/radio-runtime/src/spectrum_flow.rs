//! Pure completion policy for the spectrum session. The firmware main loop
//! owns RF execution, but which follow-up is allowed after a restore is decided
//! here so host tests can cover the ordering without hardware or a binary test.

use uv98_spectrum::{compact::StopReason, Window};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedWindow {
    pub generation: u32,
    pub start_10hz: u32,
    pub step_10hz: u32,
    pub len: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionError {
    NoPublishedGrid,
    NoScopeWindow,
    StaleWindow,
    BinOutOfRange,
    FrequencyMismatch,
}

/// Validate a UI selection against the grid the user actually sees and the
/// scope that owns RF. `valid(bin)` on the new engine is not enough: a restart
/// can move bin0 while the old snapshot is still displayed.
pub fn bind_bin(
    published: Option<PublishedWindow>,
    scope_window: Option<Window>,
    scope_generation: u32,
    bin: u8,
) -> Result<u32, SelectionError> {
    let published = published.ok_or(SelectionError::NoPublishedGrid)?;
    let scope = scope_window.ok_or(SelectionError::NoScopeWindow)?;
    if published.len == 0 || bin >= published.len {
        return Err(SelectionError::BinOutOfRange);
    }
    if published.generation != scope_generation
        || published.start_10hz != scope.start_10hz()
        || published.step_10hz != scope.step_10hz()
        || usize::from(published.len) != scope.len()
    {
        return Err(SelectionError::StaleWindow);
    }
    let expected = published
        .step_10hz
        .checked_mul(u32::from(bin))
        .and_then(|offset| published.start_10hz.checked_add(offset))
        .ok_or(SelectionError::FrequencyMismatch)?;
    let actual = scope
        .step_10hz()
        .checked_mul(u32::from(bin))
        .and_then(|offset| scope.start_10hz().checked_add(offset))
        .ok_or(SelectionError::FrequencyMismatch)?;
    if expected != actual {
        return Err(SelectionError::FrequencyMismatch);
    }
    Ok(expected)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Intent {
    Exit,
    Listen(u8),
    AcceptVfo(u8),
    Restart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    BeginListen,
    AcceptVfo,
    Restart,
    Finish,
}

/// A listen/accept intent only completes on the selected-bin restore with the
/// same bin. Binding the bin prevents an older `Accepted{X}` restore from
/// completing a newer operator intent for `Y`. A restart only completes on the
/// cancel restore this session requested; anything else finishes.
pub fn completion(intent: Option<Intent>, reason: StopReason) -> Action {
    match (intent, reason) {
        (Some(Intent::Listen(bin)), StopReason::Accepted { index }) if bin == index => {
            Action::BeginListen
        }
        (Some(Intent::AcceptVfo(bin)), StopReason::Accepted { index }) if bin == index => {
            Action::AcceptVfo
        }
        (Some(Intent::Restart), StopReason::Exit) => Action::Restart,
        _ => Action::Finish,
    }
}

/// End of a temporary listen: `Some(true)` resumes the same window,
/// `Some(false)` (or no request) exits the spectrum session.
pub const fn after_listen(resume: Option<bool>) -> bool {
    matches!(resume, Some(true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_matching_restore_reason_completes_each_intent() {
        assert_eq!(
            completion(Some(Intent::Listen(4)), StopReason::Accepted { index: 4 }),
            Action::BeginListen
        );
        assert_eq!(
            completion(
                Some(Intent::AcceptVfo(4)),
                StopReason::Accepted { index: 4 }
            ),
            Action::AcceptVfo
        );
        assert_eq!(
            completion(Some(Intent::Restart), StopReason::Exit),
            Action::Restart
        );
        assert_eq!(
            completion(Some(Intent::Exit), StopReason::Exit),
            Action::Finish
        );
    }

    #[test]
    fn superseded_actions_finish_and_never_restart_after_ptt_or_fault() {
        for reason in [
            StopReason::Ptt,
            StopReason::PrepareTimeout,
            StopReason::SampleTimeout,
            StopReason::Hardware { code: 3 },
            StopReason::Clock,
        ] {
            assert_eq!(
                completion(Some(Intent::Restart), reason),
                Action::Finish,
                "restart must not survive {reason:?}"
            );
            assert_eq!(
                completion(Some(Intent::Listen(4)), reason),
                Action::Finish,
                "listen must not start after {reason:?}"
            );
        }
        assert_eq!(completion(None, StopReason::Exit), Action::Finish);
    }

    #[test]
    fn published_grid_must_match_scope_generation_and_geometry() {
        let old = Window::centered(433_000_000, 1_000_000, 12_500).unwrap();
        let narrow = Window::centered(433_000_000, 500_000, 12_500).unwrap();
        let published = PublishedWindow {
            generation: 1,
            start_10hz: old.start_10hz(),
            step_10hz: old.step_10hz(),
            len: old.len() as u8,
        };
        assert_eq!(bind_bin(Some(published), Some(old), 1, 0), Ok(432_500_000));
        // Restart to the narrow grid: the old published bin0 means 432.500,
        // while the new scope bin0 is 432.750. Refuse before retune.
        assert_eq!(
            bind_bin(Some(published), Some(narrow), 2, 0),
            Err(SelectionError::StaleWindow)
        );
        let same_grid_new_gen = PublishedWindow {
            generation: 2,
            start_10hz: narrow.start_10hz(),
            step_10hz: narrow.step_10hz(),
            len: narrow.len() as u8,
        };
        assert_eq!(
            bind_bin(Some(same_grid_new_gen), Some(narrow), 2, 0),
            Ok(432_750_000)
        );
        assert_eq!(
            bind_bin(Some(same_grid_new_gen), Some(narrow), 2, 41),
            Err(SelectionError::BinOutOfRange)
        );
        assert_eq!(
            bind_bin(None, Some(narrow), 2, 0),
            Err(SelectionError::NoPublishedGrid)
        );
    }

    #[test]
    fn old_bin_cannot_be_completed_by_new_generation() {
        assert_eq!(
            bind_bin(
                Some(PublishedWindow {
                    generation: 1,
                    start_10hz: 432_500_000,
                    step_10hz: 12_500,
                    len: 81,
                }),
                Some(Window::centered(433_000_000, 500_000, 12_500).unwrap()),
                2,
                0,
            ),
            Err(SelectionError::StaleWindow)
        );
    }

    #[test]
    fn delayed_old_bin_cannot_complete_a_newer_intent() {
        // Operator asked for Y while the engine was still restoring X. The old
        // completion must not accept/listen Y.
        assert_eq!(
            completion(Some(Intent::Listen(9)), StopReason::Accepted { index: 2 }),
            Action::Finish
        );
        assert_eq!(
            completion(
                Some(Intent::AcceptVfo(9)),
                StopReason::Accepted { index: 2 }
            ),
            Action::Finish
        );
        assert_eq!(
            completion(Some(Intent::Listen(2)), StopReason::Accepted { index: 2 }),
            Action::BeginListen
        );
    }

    #[test]
    fn listen_end_defaults_to_resume_on_explicit_request_only() {
        assert!(after_listen(Some(true)));
        assert!(!after_listen(Some(false)));
        assert!(!after_listen(None));
    }
}
