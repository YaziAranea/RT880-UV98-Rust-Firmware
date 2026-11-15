//! Operator-facing sweep window: an explicit centre, span and grid step,
//! independent of the ordinary VFO step. Every derived number is checked; a
//! window that does not fit the compact engine is refused, never truncated.

use crate::{Range, RangeError};

/// One more than this many intervals would exceed the compact engine's bins.
const MAX_INTERVALS: u32 = (crate::compact::BINS - 1) as u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowError {
    ZeroCenter,
    ZeroStep,
    /// Span is shorter than two grid steps, so no centre point can exist.
    TooNarrow,
    /// Span needs more than 81 points.
    TooWide,
    /// Centre, start or end does not fit the 10 Hz integer range.
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Window {
    range: Range,
    center_10hz: u32,
}

impl Window {
    /// Symmetric window around `center_10hz`. The requested span is rounded
    /// down to an even number of whole steps so the centre stays an exact bin;
    /// one requested span can therefore display slightly less spectrum.
    pub const fn centered(
        center_10hz: u32,
        span_10hz: u32,
        step_10hz: u32,
    ) -> Result<Self, WindowError> {
        if center_10hz == 0 {
            return Err(WindowError::ZeroCenter);
        }
        if step_10hz == 0 {
            return Err(WindowError::ZeroStep);
        }
        let intervals = span_10hz / step_10hz;
        if intervals < 2 {
            return Err(WindowError::TooNarrow);
        }
        let even = intervals & !1;
        if even > MAX_INTERVALS {
            return Err(WindowError::TooWide);
        }
        let half = even / 2;
        let Some(offset) = half.checked_mul(step_10hz) else {
            return Err(WindowError::Overflow);
        };
        let Some(start) = center_10hz.checked_sub(offset) else {
            return Err(WindowError::Overflow);
        };
        let Some(end) = center_10hz.checked_add(offset) else {
            return Err(WindowError::Overflow);
        };
        let range = match Range::new(start, end, step_10hz) {
            Ok(range) => range,
            Err(RangeError::TooManyBins) => return Err(WindowError::TooWide),
            Err(_) => return Err(WindowError::Overflow),
        };
        Ok(Self { range, center_10hz })
    }

    pub const fn range(self) -> Range {
        self.range
    }
    pub const fn center_10hz(self) -> u32 {
        self.center_10hz
    }
    /// Displayed span after rounding to whole steps, not the requested value.
    pub const fn span_10hz(self) -> u32 {
        self.range.end_10hz() - self.range.start_10hz()
    }
    pub const fn start_10hz(self) -> u32 {
        self.range.start_10hz()
    }
    pub const fn end_10hz(self) -> u32 {
        self.range.end_10hz()
    }
    pub const fn step_10hz(self) -> u32 {
        self.range.step_10hz()
    }
    pub const fn len(self) -> usize {
        self.range.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetric_windows_keep_the_centre_as_an_exact_bin() {
        let w = Window::centered(43_350_000, 1_000_000, 12_500).unwrap();
        assert_eq!(w.len(), 81);
        assert_eq!((w.start_10hz(), w.end_10hz()), (42_850_000, 43_850_000));
        assert_eq!(w.span_10hz(), 1_000_000);
        assert_eq!(w.range().frequency_10hz(40), Some(w.center_10hz()));
        let narrow = Window::centered(43_350_000, 500_000, 12_500).unwrap();
        assert_eq!(narrow.len(), 41);
        assert_eq!(narrow.range().frequency_10hz(20), Some(43_350_000));
    }

    #[test]
    fn one_off_span_rounds_down_instead_of_shifting_the_centre() {
        // 41.5 intervals is not an odd grid: the last half step is dropped.
        let w = Window::centered(30_000_000, 12_500 * 41 + 6_000, 12_500).unwrap();
        assert_eq!(w.len(), 41);
        assert_eq!(w.range().frequency_10hz(20), Some(30_000_000));
        assert_eq!(w.span_10hz(), 12_500 * 40);
        // Two intervals are the smallest window with a centre and two edges.
        assert_eq!(
            Window::centered(30_000_000, 25_000, 12_500).unwrap().len(),
            3
        );
    }

    #[test]
    fn invalid_or_unrepresentable_windows_are_named_not_truncated() {
        assert_eq!(
            Window::centered(0, 1_000_000, 12_500),
            Err(WindowError::ZeroCenter)
        );
        assert_eq!(
            Window::centered(30_000_000, 1_000_000, 0),
            Err(WindowError::ZeroStep)
        );
        assert_eq!(
            Window::centered(30_000_000, 20_000, 12_500),
            Err(WindowError::TooNarrow)
        );
        assert_eq!(
            Window::centered(30_000_000, 12_500 * 82, 12_500),
            Err(WindowError::TooWide)
        );
        assert_eq!(
            Window::centered(u32::MAX, 25_000, 12_500),
            Err(WindowError::Overflow)
        );
        assert_eq!(
            Window::centered(12_490, 25_000, 12_500),
            Err(WindowError::Overflow)
        );
    }
}
