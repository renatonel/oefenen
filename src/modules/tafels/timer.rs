//! The three timer modes, and the colour stage a countdown is in.
//!
//! The timer itself holds no clock: every query takes the number of
//! milliseconds that count against the limit. That keeps the whole thing a pure
//! function of elapsed time, which is what makes pausing (during feedback) and
//! expiry testable without sleeping.

use serde::{Deserialize, Serialize};

/// How long the learner has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerMode {
    /// A countdown that restarts for every question.
    PerQuestion,
    /// One countdown for the whole session.
    Total,
    /// A stopwatch that counts up and can never run out.
    None,
}

impl TimerMode {
    /// The three modes, in the order the settings screen lists them.
    pub fn all() -> [Self; 3] {
        [Self::PerQuestion, Self::Total, Self::None]
    }

    /// Whether this mode ever expires.
    pub fn has_limit(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Whether this mode's limit is the length of the whole session.
    pub fn is_total(self) -> bool {
        matches!(self, Self::Total)
    }

    /// Index into [`TimerMode::all`], used by the settings screen.
    pub fn index(self) -> usize {
        match self {
            Self::PerQuestion => 0,
            Self::Total => 1,
            Self::None => 2,
        }
    }

    pub fn from_index(index: usize) -> Self {
        Self::all().get(index).copied().unwrap_or(Self::PerQuestion)
    }
}

/// The colour stage of a countdown.
///
/// Colour on its own is not accessible, so the stage also drives the width of
/// the gauge under the timer and the boldness of the digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerStage {
    /// More than half of the time is left.
    Ok,
    /// At most half, but more than a quarter.
    Warn,
    /// A quarter or less: urgent.
    Critical,
}

impl TimerStage {
    /// The stage a given fraction of remaining time belongs to.
    ///
    /// The boundaries are inclusive on the urgent side, exactly as the screen
    /// promises: above 50% is green, 50% and 25% are already the next colour.
    pub fn of(remaining_fraction: f64) -> Self {
        if remaining_fraction > 0.5 {
            Self::Ok
        } else if remaining_fraction > 0.25 {
            Self::Warn
        } else {
            Self::Critical
        }
    }
}

/// A countdown or a stopwatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timer {
    mode: TimerMode,
    limit_ms: u64,
}

impl Timer {
    pub fn new(mode: TimerMode, limit_seconds: u64) -> Self {
        Self {
            mode,
            limit_ms: limit_seconds.saturating_mul(1000),
        }
    }

    /// A timer that only counts up.
    pub fn stopwatch() -> Self {
        Self::new(TimerMode::None, 0)
    }

    pub fn mode(self) -> TimerMode {
        self.mode
    }

    /// The limit in milliseconds; meaningless without a limit.
    pub fn limit_ms(self) -> u64 {
        self.limit_ms
    }

    /// Whether the mode has a countdown at all.
    pub fn has_limit(self) -> bool {
        self.mode.has_limit()
    }

    /// Milliseconds left, or `None` when there is no limit.
    pub fn remaining_ms(self, elapsed_ms: u64) -> Option<u64> {
        self.mode
            .has_limit()
            .then(|| self.limit_ms.saturating_sub(elapsed_ms))
    }

    /// Whether the countdown has run out.
    pub fn is_expired(self, elapsed_ms: u64) -> bool {
        self.mode.has_limit() && elapsed_ms >= self.limit_ms
    }

    /// The part of the limit still left, between 0 and 1.
    ///
    /// `None` without a limit: there is no fraction to shrink towards zero.
    pub fn remaining_fraction(self, elapsed_ms: u64) -> Option<f64> {
        if !self.mode.has_limit() || self.limit_ms == 0 {
            return None;
        }
        let remaining = self.limit_ms.saturating_sub(elapsed_ms) as f64;
        Some((remaining / self.limit_ms as f64).clamp(0.0, 1.0))
    }

    /// The colour stage for the time counted so far.
    pub fn stage(self, elapsed_ms: u64) -> Option<TimerStage> {
        self.remaining_fraction(elapsed_ms).map(TimerStage::of)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stage_boundaries_are_exactly_where_the_screen_says() {
        // More than half left: green.
        assert_eq!(TimerStage::of(1.0), TimerStage::Ok);
        assert_eq!(TimerStage::of(0.51), TimerStage::Ok);
        // Exactly half is no longer green.
        assert_eq!(TimerStage::of(0.5), TimerStage::Warn);
        assert_eq!(TimerStage::of(0.499_999), TimerStage::Warn);
        // Just above a quarter stays yellow.
        assert_eq!(TimerStage::of(0.26), TimerStage::Warn);
        assert_eq!(TimerStage::of(0.250_001), TimerStage::Warn);
        // A quarter and less is red.
        assert_eq!(TimerStage::of(0.25), TimerStage::Critical);
        assert_eq!(TimerStage::of(0.0), TimerStage::Critical);
    }

    #[test]
    fn the_stages_change_as_the_clock_runs_out() {
        let timer = Timer::new(TimerMode::Total, 10);
        assert_eq!(timer.stage(0), Some(TimerStage::Ok));
        assert_eq!(timer.stage(4_999), Some(TimerStage::Ok));
        assert_eq!(timer.stage(5_000), Some(TimerStage::Warn));
        // A quarter left is the last yellow moment.
        assert_eq!(timer.stage(7_499), Some(TimerStage::Warn));
        assert_eq!(timer.stage(7_500), Some(TimerStage::Critical));
        assert_eq!(timer.stage(10_000), Some(TimerStage::Critical));
    }

    #[test]
    fn a_countdown_counts_down_and_expires() {
        let timer = Timer::new(TimerMode::PerQuestion, 5);
        assert_eq!(timer.remaining_ms(0), Some(5_000));
        assert_eq!(timer.remaining_ms(1_250), Some(3_750));
        assert_eq!(timer.remaining_ms(4_999), Some(1));
        // Never negative, and expired exactly at the limit.
        assert_eq!(timer.remaining_ms(9_999), Some(0));
        assert!(!timer.is_expired(4_999));
        assert!(timer.is_expired(5_000));
        assert!(timer.is_expired(5_001));
    }

    #[test]
    fn the_fraction_is_always_between_zero_and_one() {
        let timer = Timer::new(TimerMode::Total, 4);
        assert_eq!(timer.remaining_fraction(0), Some(1.0));
        assert_eq!(timer.remaining_fraction(1_000), Some(0.75));
        assert_eq!(timer.remaining_fraction(3_000), Some(0.25));
        assert_eq!(timer.remaining_fraction(99_000), Some(0.0));
    }

    #[test]
    fn a_stopwatch_never_expires_and_has_no_fraction() {
        let timer = Timer::stopwatch();
        assert!(!timer.has_limit());
        assert_eq!(timer.remaining_ms(0), None);
        assert_eq!(timer.remaining_ms(3_600_000), None);
        assert_eq!(timer.remaining_fraction(1_000), None);
        assert_eq!(timer.stage(1_000), None);
        assert!(!timer.is_expired(u64::MAX));
    }

    #[test]
    fn modes_round_trip_through_their_position() {
        for (index, mode) in TimerMode::all().into_iter().enumerate() {
            assert_eq!(mode.index(), index);
            assert_eq!(TimerMode::from_index(index), mode);
        }
        // Anything out of range falls back to the first mode instead of panicking.
        assert_eq!(TimerMode::from_index(9), TimerMode::PerQuestion);
        assert!(TimerMode::PerQuestion.has_limit());
        assert!(TimerMode::Total.has_limit());
        assert!(!TimerMode::None.has_limit());
        assert!(TimerMode::Total.is_total());
        assert!(!TimerMode::PerQuestion.is_total());
    }

    #[test]
    fn modes_are_stored_and_read_back() {
        for mode in TimerMode::all() {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: TimerMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode);
        }
        assert_eq!(serde_json::to_string(&TimerMode::None).unwrap(), "\"none\"");
    }
}
