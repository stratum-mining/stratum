//! Metric computation over collections of [`Trial`] results.
//!
//! Each metric is computed as a distribution across many independent trials,
//! exposing percentiles (`p10` through `p99`), mean, and trial count. Where a
//! metric can fail (a trial that does not converge, does not react, etc.) the
//! function additionally reports the *rate* of successful trials — typically
//! both numbers matter and an algorithm regression usually shows up first in
//! the rate or in a tail percentile.
//!
//! See `VARDIFF_SIMULATION_FRAMEWORK.md` § "The five metrics" for the
//! definitional details of each. Implementations here follow those definitions
//! precisely.

use crate::trial::{FireEvent, Trial};

/// A sorted collection of numeric trial-derived values, plus accessors for
/// summary statistics.
///
/// Internally stores all values sorted ascending so percentile queries are
/// `O(1)`. Construction is `O(n log n)`. Designed for n in the low thousands —
/// not optimized for streaming.
#[derive(Clone)]
pub struct Distribution {
    /// Sorted ascending. NaN values are filtered out at construction time.
    sorted: Vec<f64>,
}

impl Distribution {
    /// Constructs a distribution from a vector of values. NaN values are
    /// silently dropped (they would otherwise corrupt percentile ordering).
    pub fn new(values: Vec<f64>) -> Self {
        let mut sorted: Vec<f64> = values.into_iter().filter(|x| !x.is_nan()).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Self { sorted }
    }

    /// Returns the value at the given percentile rank `p ∈ [0, 100]` using
    /// nearest-rank interpolation. `None` if the distribution is empty.
    ///
    /// Nearest-rank rather than linear interpolation because the use cases are
    /// (a) reporting, where readability of integer-positional values is fine,
    /// and (b) regression assertions, where interpolation noise across runs
    /// would muddy the comparison.
    pub fn percentile(&self, p: f64) -> Option<f64> {
        if self.sorted.is_empty() {
            return None;
        }
        let n = self.sorted.len();
        let idx = ((p / 100.0) * (n as f64 - 1.0)).round() as usize;
        Some(self.sorted[idx.min(n - 1)])
    }

    /// Returns the arithmetic mean. `None` if the distribution is empty.
    pub fn mean(&self) -> Option<f64> {
        if self.sorted.is_empty() {
            return None;
        }
        Some(self.sorted.iter().sum::<f64>() / self.sorted.len() as f64)
    }

    /// Returns the number of values in the distribution.
    pub fn count(&self) -> usize {
        self.sorted.len()
    }

    pub fn p10(&self) -> Option<f64> {
        self.percentile(10.0)
    }
    pub fn p25(&self) -> Option<f64> {
        self.percentile(25.0)
    }
    pub fn p50(&self) -> Option<f64> {
        self.percentile(50.0)
    }
    pub fn p75(&self) -> Option<f64> {
        self.percentile(75.0)
    }
    pub fn p90(&self) -> Option<f64> {
        self.percentile(90.0)
    }
    pub fn p95(&self) -> Option<f64> {
        self.percentile(95.0)
    }
    pub fn p99(&self) -> Option<f64> {
        self.percentile(99.0)
    }
}

impl std::fmt::Debug for Distribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Distribution(n={}", self.count())?;
        if let Some(p) = self.p50() {
            write!(f, " p50={p:.3}")?;
        }
        if let Some(p) = self.p90() {
            write!(f, " p90={p:.3}")?;
        }
        if let Some(p) = self.p99() {
            write!(f, " p99={p:.3}")?;
        }
        if let Some(m) = self.mean() {
            write!(f, " mean={m:.3}")?;
        }
        write!(f, ")")
    }
}

// ============================================================================
// Convergence time
// ============================================================================

/// Per-trial convergence time. Returns `Some(t_secs)` if the trial converged
/// (i.e., some fire `f` exists such that no subsequent fire occurs within
/// `[f.at_secs, f.at_secs + quiet_window_secs]` and that window fits inside
/// the trial). A trial with zero fires is treated as converged at time 0.
/// Returns `None` for trials whose fires never quiet down before the trial
/// duration ends.
///
/// The reported time is the timestamp of the *last fire* before the quiet
/// window — not when the quiet-detector concludes. A single late re-fire
/// pushes convergence time forward to that fire's timestamp; the trial is
/// still considered converged provided the late fire is followed by enough
/// silence within the trial.
pub fn convergence_time_for_trial(trial: &Trial, quiet_window_secs: u64) -> Option<u64> {
    if trial.fires.is_empty() {
        return Some(0);
    }
    for (i, fire) in trial.fires.iter().enumerate() {
        let quiet_end = fire.at_secs.saturating_add(quiet_window_secs);
        if quiet_end > trial.config.duration_secs {
            // No fire from here forward can satisfy the quiet-window-fits-in-trial
            // constraint, so neither can any later fire — stop scanning.
            break;
        }
        let has_subsequent_in_window = trial.fires[i + 1..]
            .iter()
            .any(|f2| f2.at_secs <= quiet_end);
        if !has_subsequent_in_window {
            return Some(fire.at_secs);
        }
    }
    None
}

/// Convergence-time distribution across a set of trials.
///
/// Returns `(convergence_rate, time_distribution_secs)`:
///
/// - `convergence_rate ∈ [0, 1]` is the fraction of trials that converged
///   (i.e., had at least one fire-followed-by-quiet-window or zero fires).
/// - `time_distribution_secs` contains the per-trial convergence times in
///   seconds for the converged trials only. DNF trials are excluded.
pub fn convergence_time_distribution(
    trials: &[Trial],
    quiet_window_secs: u64,
) -> (f64, Distribution) {
    if trials.is_empty() {
        return (0.0, Distribution::new(vec![]));
    }
    let mut times: Vec<f64> = Vec::with_capacity(trials.len());
    let mut converged = 0usize;
    for trial in trials {
        if let Some(t) = convergence_time_for_trial(trial, quiet_window_secs) {
            converged += 1;
            times.push(t as f64);
        }
    }
    let rate = converged as f64 / trials.len() as f64;
    (rate, Distribution::new(times))
}

// ============================================================================
// Settled accuracy
// ============================================================================

/// Relative error between the algorithm's final hashrate estimate and the true
/// miner hashrate at trial end.
///
/// `accuracy = |final_hashrate / true_hashrate - 1|`. A perfectly accurate
/// final estimate yields 0; a 50% over- or under-estimate yields 0.5.
///
/// Returns `None` if the trial's true hashrate is zero or negative, which
/// would make the relative error undefined.
pub fn settled_accuracy_for_trial(trial: &Trial) -> Option<f64> {
    let true_h = trial.true_hashrate_at_end as f64;
    if true_h <= 0.0 {
        return None;
    }
    let final_h = trial.final_hashrate as f64;
    Some((final_h / true_h - 1.0).abs())
}

/// Distribution of settled accuracy errors across a set of trials. Trials with
/// non-positive true hashrate are silently dropped.
pub fn settled_accuracy_distribution(trials: &[Trial]) -> Distribution {
    let values: Vec<f64> = trials.iter().filter_map(settled_accuracy_for_trial).collect();
    Distribution::new(values)
}

// ============================================================================
// Steady-state jitter
// ============================================================================

/// Per-trial steady-state jitter: count of fires per minute during the
/// settled period of the trial.
///
/// The settled period is `[convergence_time + settle_buffer_secs, trial_end]`.
/// `settle_buffer_secs` keeps any post-convergence transient fires from
/// polluting the count (these can happen right after the convergence
/// fire if the algorithm settles in a small oscillation).
///
/// Returns `None` if:
/// - The trial did not converge (DNF).
/// - The settled window is shorter than `min_settled_window_secs` — too
///   little data to be meaningful.
pub fn jitter_for_trial(
    trial: &Trial,
    quiet_window_secs: u64,
    settle_buffer_secs: u64,
    min_settled_window_secs: u64,
) -> Option<f64> {
    let convergence_time = convergence_time_for_trial(trial, quiet_window_secs)?;
    let start = convergence_time.saturating_add(settle_buffer_secs);
    let end = trial.config.duration_secs;
    if end < start.saturating_add(min_settled_window_secs) {
        return None;
    }
    let settled_fires = trial
        .fires
        .iter()
        .filter(|f| f.at_secs >= start && f.at_secs <= end)
        .count();
    let window_minutes = (end - start) as f64 / 60.0;
    if window_minutes <= 0.0 {
        return None;
    }
    Some(settled_fires as f64 / window_minutes)
}

/// Distribution of steady-state jitter (fires per minute) across trials.
/// Trials that did not converge or had too-short settled windows are dropped.
pub fn jitter_distribution(
    trials: &[Trial],
    quiet_window_secs: u64,
    settle_buffer_secs: u64,
    min_settled_window_secs: u64,
) -> Distribution {
    let values: Vec<f64> = trials
        .iter()
        .filter_map(|t| jitter_for_trial(t, quiet_window_secs, settle_buffer_secs, min_settled_window_secs))
        .collect();
    Distribution::new(values)
}

// ============================================================================
// Reaction time and sensitivity
// ============================================================================

/// Per-trial reaction time: seconds from a scheduled event at
/// `event_at_secs` to the first fire occurring after that event, provided
/// the fire happens within `react_window_secs` of the event.
///
/// Returns `None` if no fire occurs in `[event_at_secs, event_at_secs +
/// react_window_secs]`. The trial schedule itself is not inspected — the
/// caller is responsible for configuring trials with the relevant step
/// change at `event_at_secs`.
pub fn reaction_time_for_trial(
    trial: &Trial,
    event_at_secs: u64,
    react_window_secs: u64,
) -> Option<u64> {
    let window_end = event_at_secs.saturating_add(react_window_secs);
    let first_post_event_fire: Option<&FireEvent> =
        trial.fires.iter().find(|f| f.at_secs > event_at_secs && f.at_secs <= window_end);
    first_post_event_fire.map(|f| f.at_secs - event_at_secs)
}

/// Distribution of reaction times across trials. Returns
/// `(reaction_rate, time_distribution_secs)`:
///
/// - `reaction_rate ∈ [0, 1]` is the fraction of trials that produced any
///   fire in the reaction window. This is exactly the **reaction
///   sensitivity** of the algorithm at whatever step magnitude the trials
///   were configured with — see [`reaction_sensitivity`] for an explicit
///   alias.
/// - `time_distribution_secs` contains per-trial reaction times for trials
///   that reacted.
pub fn reaction_time_distribution(
    trials: &[Trial],
    event_at_secs: u64,
    react_window_secs: u64,
) -> (f64, Distribution) {
    if trials.is_empty() {
        return (0.0, Distribution::new(vec![]));
    }
    let mut times: Vec<f64> = Vec::with_capacity(trials.len());
    let mut reacted = 0usize;
    for trial in trials {
        if let Some(t) = reaction_time_for_trial(trial, event_at_secs, react_window_secs) {
            reacted += 1;
            times.push(t as f64);
        }
    }
    let rate = reacted as f64 / trials.len() as f64;
    (rate, Distribution::new(times))
}

/// Reaction sensitivity: probability of *any* fire within `react_window_secs`
/// of a scheduled event at `event_at_secs`. Returns a value in `[0, 1]`.
///
/// Identical to the `reaction_rate` component of [`reaction_time_distribution`];
/// kept as a separate function because it's the metric the baseline tables
/// report and the assertion policy operates on.
pub fn reaction_sensitivity(
    trials: &[Trial],
    event_at_secs: u64,
    react_window_secs: u64,
) -> f64 {
    let (rate, _) = reaction_time_distribution(trials, event_at_secs, react_window_secs);
    rate
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trial::{FireEvent, TrialConfig};

    fn trial_with_fires(duration_secs: u64, fire_times: &[u64]) -> Trial {
        let config = TrialConfig {
            duration_secs,
            ..TrialConfig::default()
        };
        let fires = fire_times
            .iter()
            .map(|&t| FireEvent {
                at_secs: t,
                old_hashrate: 1.0,
                new_hashrate: 1.0,
            })
            .collect();
        Trial {
            config,
            seed: 0,
            fires,
            final_hashrate: 1.0e15,
            true_hashrate_at_end: 1.0e15,
        }
    }

    fn trial_with_final_and_true(final_h: f32, true_h: f32) -> Trial {
        let config = TrialConfig::default();
        Trial {
            config,
            seed: 0,
            fires: vec![],
            final_hashrate: final_h,
            true_hashrate_at_end: true_h,
        }
    }

    // ---- Distribution ----

    #[test]
    fn distribution_empty_returns_none_for_stats() {
        let d = Distribution::new(vec![]);
        assert!(d.percentile(50.0).is_none());
        assert!(d.mean().is_none());
        assert_eq!(d.count(), 0);
    }

    #[test]
    fn distribution_percentiles_use_nearest_rank() {
        // Sorted values: 1 .. 100. Length 100. Indices 0..99.
        let values: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let d = Distribution::new(values);
        assert_eq!(d.count(), 100);
        // p50 → index round((50/100) * 99) = round(49.5) = 50 → value 51.
        // p50 in nearest-rank can land at 50 or 51 depending on rounding; verify
        // the rounding direction.
        let p50 = d.p50().unwrap();
        assert!(p50 == 50.0 || p50 == 51.0, "p50 was {p50}");
        // p99 → round(0.99 * 99) = round(98.01) = 98 → value 99.
        assert_eq!(d.p99().unwrap(), 99.0);
        // p10 → round(0.10 * 99) = round(9.9) = 10 → value 11.
        assert_eq!(d.p10().unwrap(), 11.0);
    }

    #[test]
    fn distribution_mean_is_arithmetic_mean() {
        let d = Distribution::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(d.mean().unwrap(), 3.0);
    }

    #[test]
    fn distribution_filters_nan() {
        let d = Distribution::new(vec![1.0, f64::NAN, 2.0]);
        assert_eq!(d.count(), 2);
        assert_eq!(d.mean().unwrap(), 1.5);
    }

    // ---- Convergence ----

    #[test]
    fn convergence_no_fires_means_converged_at_zero() {
        let t = trial_with_fires(1800, &[]);
        assert_eq!(convergence_time_for_trial(&t, 300), Some(0));
    }

    #[test]
    fn convergence_picks_first_fire_followed_by_quiet_window() {
        // Fires at 60, 300, 360, 420. quiet_window = 600.
        // 60 followed by 300 (≤ 60+600=660): not converged.
        // 300 followed by 360 (≤ 900): not converged.
        // 360 followed by 420 (≤ 960): not converged.
        // 420 followed by nothing (trial duration 1800, window 1020 fits): CONVERGED at 420.
        let t = trial_with_fires(1800, &[60, 300, 360, 420]);
        assert_eq!(convergence_time_for_trial(&t, 600), Some(420));
    }

    #[test]
    fn convergence_returns_none_when_fires_never_quiet_down() {
        // Fires every minute throughout the trial; never a 600s gap.
        let fires: Vec<u64> = (0..30).map(|i| (i + 1) * 60).collect();
        let t = trial_with_fires(1800, &fires);
        assert_eq!(convergence_time_for_trial(&t, 600), None);
    }

    #[test]
    fn convergence_late_fire_too_close_to_trial_end_is_dnf() {
        // Fire at 1700, trial ends at 1800. quiet_window=600. 1700+600=2300 > 1800.
        // No room to verify quietude → DNF.
        let t = trial_with_fires(1800, &[1700]);
        assert_eq!(convergence_time_for_trial(&t, 600), None);
    }

    #[test]
    fn convergence_rate_aggregates_correctly() {
        let trials = vec![
            trial_with_fires(1800, &[]),         // converged at 0
            trial_with_fires(1800, &[60, 1100]), // converged at 1100 (quiet through end)
            {
                // never quiets
                let fires: Vec<u64> = (0..30).map(|i| (i + 1) * 60).collect();
                trial_with_fires(1800, &fires)
            },
        ];
        let (rate, dist) = convergence_time_distribution(&trials, 600);
        assert!((rate - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(dist.count(), 2);
    }

    // ---- Settled accuracy ----

    #[test]
    fn settled_accuracy_perfect_estimate_is_zero() {
        let t = trial_with_final_and_true(1.0e15, 1.0e15);
        assert_eq!(settled_accuracy_for_trial(&t).unwrap(), 0.0);
    }

    #[test]
    fn settled_accuracy_50_percent_over_is_half() {
        let t = trial_with_final_and_true(1.5e15, 1.0e15);
        assert!((settled_accuracy_for_trial(&t).unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn settled_accuracy_50_percent_under_is_half() {
        let t = trial_with_final_and_true(0.5e15, 1.0e15);
        assert!((settled_accuracy_for_trial(&t).unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn settled_accuracy_zero_truth_returns_none() {
        let t = trial_with_final_and_true(1.0e15, 0.0);
        assert!(settled_accuracy_for_trial(&t).is_none());
    }

    // ---- Jitter ----

    #[test]
    fn jitter_counts_post_settle_fires_per_minute() {
        // Trial: duration 1800. quiet_window=300. settle_buffer=120. min_settled=600.
        // Fires: 60 (settle), 700, 1000, 1500. 60 followed by 700 (>360=quiet_end), CONVERGED at 60.
        // Settled window: [180, 1800] = 1620s = 27 min. Fires in window: 700, 1000, 1500 → 3 fires.
        // Jitter: 3 / 27 ≈ 0.111.
        let t = trial_with_fires(1800, &[60, 700, 1000, 1500]);
        let j = jitter_for_trial(&t, 300, 120, 600).unwrap();
        assert!((j - 3.0 / 27.0).abs() < 1e-6, "jitter = {j}");
    }

    #[test]
    fn jitter_zero_when_no_settled_fires() {
        // Single fire then complete silence.
        let t = trial_with_fires(1800, &[60]);
        let j = jitter_for_trial(&t, 300, 120, 600).unwrap();
        assert_eq!(j, 0.0);
    }

    #[test]
    fn jitter_none_when_trial_did_not_converge() {
        let fires: Vec<u64> = (0..30).map(|i| (i + 1) * 60).collect();
        let t = trial_with_fires(1800, &fires);
        assert!(jitter_for_trial(&t, 600, 120, 600).is_none());
    }

    // ---- Reaction ----

    #[test]
    fn reaction_time_finds_first_post_event_fire_in_window() {
        // Event at 900. Fires before and after.
        let t = trial_with_fires(1800, &[60, 500, 950, 1100]);
        assert_eq!(reaction_time_for_trial(&t, 900, 300), Some(50));
    }

    #[test]
    fn reaction_time_none_when_no_fire_in_window() {
        // Event at 900, react_window=120. Next fire at 1100 (delta 200, > window).
        let t = trial_with_fires(1800, &[60, 1100]);
        assert!(reaction_time_for_trial(&t, 900, 120).is_none());
    }

    #[test]
    fn reaction_time_ignores_fires_at_or_before_event() {
        // Fire exactly at event time: not a reaction (must be > event_at_secs).
        let t = trial_with_fires(1800, &[900, 1500]);
        // 900 itself doesn't count; 1500 is way past react_window=300 from 900.
        assert!(reaction_time_for_trial(&t, 900, 300).is_none());
    }

    #[test]
    fn reaction_sensitivity_aggregates_to_fraction() {
        let trials = vec![
            trial_with_fires(1800, &[1000]), // reacts
            trial_with_fires(1800, &[1100]), // reacts
            trial_with_fires(1800, &[60]),   // no post-event fire
            trial_with_fires(1800, &[]),     // no fires at all
        ];
        let sensitivity = reaction_sensitivity(&trials, 900, 300);
        assert!((sensitivity - 0.5).abs() < 1e-9);
    }
}
