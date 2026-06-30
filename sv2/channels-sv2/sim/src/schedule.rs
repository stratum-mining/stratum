//! Hashrate schedule describing the miner's true hashrate over simulated time.
//!
//! A trial's miner is modeled as a step function over time. The simplest case
//! is a constant hashrate ([`HashrateSchedule::stable`]); other scenarios
//! exercise the algorithm's response to genuine load changes via step
//! transitions, brief throttling episodes, etc.

/// Specifies the miner's true hashrate as a piecewise-constant step function
/// over simulated time.
///
/// Segments are stored sorted by start time. A query at time `t` returns the
/// hashrate of the most recent segment with `start_secs <= t`. The schedule
/// must contain at least one segment starting at time 0.
#[derive(Debug, Clone)]
pub struct HashrateSchedule {
    /// Sorted ascending by `start_secs`. First entry always has `start_secs == 0`.
    segments: Vec<(u64, f32)>,
}

impl HashrateSchedule {
    /// Constructs a schedule from `segments`, which need not be sorted. The
    /// schedule is normalized: segments are sorted ascending by start time
    /// and a `(0, _)` entry is required.
    ///
    /// # Panics
    /// Panics if `segments` is empty or does not contain a segment starting
    /// at time 0.
    pub fn new(mut segments: Vec<(u64, f32)>) -> Self {
        assert!(
            !segments.is_empty(),
            "HashrateSchedule must contain at least one segment"
        );
        segments.sort_by_key(|&(t, _)| t);
        assert_eq!(
            segments[0].0, 0,
            "HashrateSchedule must contain a segment starting at time 0"
        );
        Self { segments }
    }

    /// A schedule with a single constant hashrate for the entire trial.
    pub fn stable(hashrate: f32) -> Self {
        Self::new(vec![(0, hashrate)])
    }

    /// A schedule that holds `before` until `change_at_secs`, then `after`
    /// thereafter. Convenient for "miner doubled hashrate" or "miner halved
    /// hashrate" scenarios.
    pub fn step(before: f32, after: f32, change_at_secs: u64) -> Self {
        Self::new(vec![(0, before), (change_at_secs, after)])
    }

    /// A schedule that drops to `during` between `start_secs` and `end_secs`,
    /// then recovers to `baseline`. Models a transient throttle or partial
    /// failure that resolves itself.
    pub fn throttle(baseline: f32, during: f32, start_secs: u64, end_secs: u64) -> Self {
        Self::new(vec![
            (0, baseline),
            (start_secs, during),
            (end_secs, baseline),
        ])
    }

    /// Returns the hashrate at simulated time `t`.
    pub fn at(&self, t: u64) -> f32 {
        let mut current = self.segments[0].1;
        for &(start, h) in &self.segments {
            if t >= start {
                current = h;
            } else {
                break;
            }
        }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_returns_same_hashrate_at_any_time() {
        let s = HashrateSchedule::stable(1.0e15);
        assert_eq!(s.at(0), 1.0e15);
        assert_eq!(s.at(60), 1.0e15);
        assert_eq!(s.at(3600), 1.0e15);
    }

    #[test]
    fn step_changes_at_the_right_time() {
        let s = HashrateSchedule::step(1.0e15, 5.0e14, 900);
        assert_eq!(s.at(0), 1.0e15);
        assert_eq!(s.at(899), 1.0e15);
        assert_eq!(s.at(900), 5.0e14);
        assert_eq!(s.at(1800), 5.0e14);
    }

    #[test]
    fn throttle_dips_and_recovers() {
        let s = HashrateSchedule::throttle(1.0e15, 7.0e14, 900, 1200);
        assert_eq!(s.at(0), 1.0e15);
        assert_eq!(s.at(900), 7.0e14);
        assert_eq!(s.at(1199), 7.0e14);
        assert_eq!(s.at(1200), 1.0e15);
        assert_eq!(s.at(1800), 1.0e15);
    }

    #[test]
    fn segments_passed_unsorted_are_normalized() {
        let s = HashrateSchedule::new(vec![(900, 5.0e14), (0, 1.0e15), (1800, 1.0e15)]);
        assert_eq!(s.at(0), 1.0e15);
        assert_eq!(s.at(900), 5.0e14);
        assert_eq!(s.at(1800), 1.0e15);
    }

    #[test]
    #[should_panic(expected = "starting at time 0")]
    fn missing_zero_segment_panics() {
        let _ = HashrateSchedule::new(vec![(60, 1.0e15)]);
    }

    #[test]
    #[should_panic(expected = "at least one segment")]
    fn empty_schedule_panics() {
        let _ = HashrateSchedule::new(vec![]);
    }
}
