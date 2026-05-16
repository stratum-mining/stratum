//! The `Composed` adapter — bundles the four axis traits into a single
//! `Vardiff` implementation that the production trial driver can run.
//!
//! `Composed<E, S, B, U>` carries a blanket `impl Vardiff` so any
//! composition of (Estimator, Statistic, Boundary, UpdateRule) is
//! automatically a valid production `Vardiff` and can be substituted
//! into the existing trial driver without any further wrapping.
//!
//! The classic algorithm composed as
//! `Composed<CumulativeCounter, AbsoluteRatio, StepFunction(classic_table),
//! FullRetargetWithClamp(classic)>` is asserted fire-for-fire equivalent
//! to `VardiffState` in the `equivalence_tests` module below. That
//! equivalence is load-bearing: every axis swap from the
//! all-four-axes-classic baseline is a clean intervention on one axis,
//! because the baseline is *known* to reproduce `VardiffState`.

use std::sync::Arc;

use bitcoin::Target;
use channels_sv2::vardiff::{error::VardiffError, Clock, Vardiff};

use crate::trial::{DecisionRecord, Observable};

use super::boundary::{Boundary, StepFunction};
use super::estimator::{CumulativeCounter, Estimator, EstimatorContext};
use super::statistic::{AbsoluteRatio, Statistic};
use super::update::{FullRetargetWithClamp, UpdateRule};

/// A vardiff algorithm composed of four orthogonal axes.
///
/// `Composed<E, S, B, U>` is a drop-in replacement for `VardiffState`
/// for any production code path that holds a `Box<dyn Vardiff>` or
/// accepts an `impl Vardiff`. The four type parameters correspond to
/// the four axes (see `DESIGN.md`):
///
/// - `E`: how observations accumulate; how `H̃` is computed at snapshot.
/// - `S`: how δ is computed from a snapshot.
/// - `B`: how θ is computed from `dt_secs` and the configured rate.
/// - `U`: how the new target is computed when the algorithm fires.
///
/// The state held directly by `Composed` (timestamp, min hashrate, clock)
/// is the same shape `VardiffState` holds — only the algorithm logic is
/// factored into the four axis impls.
#[derive(Debug)]
pub struct Composed<E: Estimator, S: Statistic, B: Boundary, U: UpdateRule> {
    pub estimator: E,
    pub statistic: S,
    pub boundary: B,
    pub update: U,
    /// Unix timestamp (seconds) of the last difficulty adjustment.
    pub timestamp_of_last_update: u64,
    /// Lowest hashrate the algorithm will allow. Matches the role of
    /// `VardiffState::min_allowed_hashrate`.
    pub min_allowed_hashrate: f32,
    /// Source of "current time". Defaults to `SystemClock` in production
    /// callers; injected as `MockClock` by the simulation harness.
    pub clock: Arc<dyn Clock>,
    /// Snapshot of the last decision computed inside `try_vardiff`.
    /// Cleared at the start of every call; set just after δ and θ are
    /// computed; remains `None` if the call returned early (dt ≤ 15s).
    /// Read by [`Observable::last_decision`] and consumed by
    /// [`crate::trial::run_trial_observed`].
    pub last_decision: Option<DecisionRecord>,
}

impl<E, S, B, U> Composed<E, S, B, U>
where
    E: Estimator,
    S: Statistic,
    B: Boundary,
    U: UpdateRule,
{
    /// Constructs a `Composed` from four axis impls plus the system-level
    /// parameters (`min_allowed_hashrate`, `clock`). The
    /// `timestamp_of_last_update` is initialized from `clock.now_secs()`.
    pub fn new(
        estimator: E,
        statistic: S,
        boundary: B,
        update: U,
        min_allowed_hashrate: f32,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let timestamp_of_last_update = clock.now_secs();
        Self {
            estimator,
            statistic,
            boundary,
            update,
            timestamp_of_last_update,
            min_allowed_hashrate,
            clock,
            last_decision: None,
        }
    }
}

impl<E, S, B, U> Vardiff for Composed<E, S, B, U>
where
    E: Estimator,
    S: Statistic,
    B: Boundary,
    U: UpdateRule,
{
    fn last_update_timestamp(&self) -> u64 {
        self.timestamp_of_last_update
    }

    fn shares_since_last_update(&self) -> u32 {
        self.estimator.shares_count()
    }

    fn min_allowed_hashrate(&self) -> f32 {
        self.min_allowed_hashrate
    }

    fn set_timestamp_of_last_update(&mut self, timestamp: u64) {
        self.timestamp_of_last_update = timestamp;
    }

    fn increment_shares_since_last_update(&mut self) {
        self.estimator.observe(1);
    }

    fn add_shares(&mut self, n: u32) {
        self.estimator.observe(n);
    }

    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        self.timestamp_of_last_update = self.clock.now_secs();
        self.estimator.reset();
        Ok(())
    }

    fn try_vardiff(
        &mut self,
        hashrate: f32,
        target: &Target,
        shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError> {
        // Clear the introspection slot at entry. If we return early
        // (dt ≤ 15) the observer reads `None` for this tick. If we
        // reach the decision point we set it to `Some(...)` before
        // either firing or holding.
        self.last_decision = None;

        let now = self.clock.now_secs();
        let dt = now.saturating_sub(self.timestamp_of_last_update);

        // Guard mirroring VardiffState::try_vardiff: discard rapid
        // re-entries within 15s of the last fire.
        if dt <= 15 {
            return Ok(None);
        }

        // Axis 1: snapshot the estimator's belief.
        let ctx = EstimatorContext {
            current_hashrate: hashrate,
            current_target: target,
            shares_per_minute,
        };
        let snap = self.estimator.snapshot(dt, &ctx);

        // Axis 2: compute the test statistic.
        let delta = self.statistic.delta(&snap, hashrate, shares_per_minute);
        // Axis 3: compute the decision threshold.
        let threshold = self.boundary.threshold(dt, shares_per_minute);

        // Record the decision for [`Observable::last_decision`]. We
        // capture it regardless of fire/hold so post-step holds (the
        // most common case when reaction sensitivity is the metric of
        // interest) are also introspectable.
        self.last_decision = Some(DecisionRecord {
            delta,
            threshold,
            h_estimate: snap.h_estimate,
        });

        // The decision itself: fire iff δ ≥ θ. The < (not <=) matches
        // VardiffState's `pct if pct >= ...` arms (which fire on equality).
        if delta < threshold {
            return Ok(None);
        }

        // Axis 4: compute the new target.
        let mut new_hashrate = self
            .update
            .next_hashrate(&snap, hashrate, delta, shares_per_minute);

        // The min_allowed_hashrate floor is a Composed-level invariant
        // (a system constraint, not an UpdateRule design choice).
        if new_hashrate < self.min_allowed_hashrate {
            new_hashrate = self.min_allowed_hashrate;
        }

        self.reset_counter()?;
        Ok(Some(new_hashrate))
    }
}

impl<E, S, B, U> Observable for Composed<E, S, B, U>
where
    E: Estimator,
    S: Statistic,
    B: Boundary,
    U: UpdateRule,
{
    fn last_decision(&self) -> Option<DecisionRecord> {
        self.last_decision
    }
}

// ============================================================================
// Convenience aliases and constructors
// ============================================================================

/// The classic algorithm as a four-axis composition. Construct via
/// [`classic_composed`].
pub type ClassicComposed =
    Composed<CumulativeCounter, AbsoluteRatio, StepFunction, FullRetargetWithClamp>;

/// Constructs the classic algorithm as a four-axis composition. Produces
/// behavior fire-for-fire identical to
/// `VardiffState::new_with_clock(min_allowed_hashrate, clock)` — asserted
/// by the `equivalence_tests` module below.
pub fn classic_composed(
    min_allowed_hashrate: f32,
    clock: Arc<dyn Clock>,
) -> ClassicComposed {
    Composed::new(
        CumulativeCounter::new(),
        AbsoluteRatio,
        StepFunction::classic_table(),
        FullRetargetWithClamp::classic(),
        min_allowed_hashrate,
        clock,
    )
}

// ============================================================================
// Equivalence tests — the load-bearing fire-for-fire assertion.
// ============================================================================
//
// These tests run both VardiffState and ClassicComposed against the same
// trial inputs (config + schedule + seed) and assert fire-for-fire
// identical output: same number of fires, same timestamps, same
// new_hashrate values down to the bit pattern. This is the property
// that makes every single-axis swap a clean intervention rather than
// a rewrite.

#[cfg(test)]
mod equivalence_tests {
    use super::*;
    use crate::schedule::HashrateSchedule;
    use crate::trial::{run_trial, run_trial_observed, Trial, TrialConfig};
    use channels_sv2::vardiff::MockClock;
    use channels_sv2::VardiffState;

    fn run_both(
        config: TrialConfig,
        schedule: HashrateSchedule,
        seed: u64,
    ) -> (Trial, Trial) {
        let clock_a = Arc::new(MockClock::new(0));
        let state = VardiffState::new_with_clock(1.0, clock_a.clone()).unwrap();
        let trial_a = run_trial(state, clock_a, config.clone(), &schedule, seed);

        let clock_b = Arc::new(MockClock::new(0));
        let composed = classic_composed(1.0, clock_b.clone());
        let trial_b = run_trial(composed, clock_b, config, &schedule, seed);

        (trial_a, trial_b)
    }

    fn assert_identical_fires(a: &Trial, b: &Trial, ctx: &str) {
        let fires_a = a.fires();
        let fires_b = b.fires();
        assert_eq!(
            fires_a.len(),
            fires_b.len(),
            "{ctx}: fire counts differ — VardiffState fired {} times, ClassicComposed fired {}",
            fires_a.len(),
            fires_b.len()
        );
        for (i, (fa, fb)) in fires_a.iter().zip(fires_b.iter()).enumerate() {
            assert_eq!(
                fa.t_secs, fb.t_secs,
                "{ctx}: fire #{i}: timestamps differ ({} vs {})",
                fa.t_secs, fb.t_secs
            );
            // Both runs went through run_trial (not run_trial_observed),
            // so new_hashrate is `Some(_)` on fire ticks for both. The
            // unwrap is therefore safe; if it panics, that's a real bug
            // (a fire tick with no new_hashrate is impossible by
            // construction in run_trial_with_observer).
            let new_a = fa.new_hashrate.expect("fire tick must carry new_hashrate");
            let new_b = fb.new_hashrate.expect("fire tick must carry new_hashrate");
            assert_eq!(
                new_a.to_bits(),
                new_b.to_bits(),
                "{ctx}: fire #{i}: new_hashrate differs ({new_a} vs {new_b})"
            );
            assert_eq!(
                fa.current_hashrate_before.to_bits(),
                fb.current_hashrate_before.to_bits(),
                "{ctx}: fire #{i}: current_hashrate_before differs ({} vs {})",
                fa.current_hashrate_before, fb.current_hashrate_before
            );
        }
        assert_eq!(
            a.final_hashrate.to_bits(),
            b.final_hashrate.to_bits(),
            "{ctx}: final hashrate differs ({} vs {})",
            a.final_hashrate, b.final_hashrate
        );
    }

    #[test]
    fn matches_on_stable_load_at_default_spm() {
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::stable(1.0e15);
        for &seed in &[1u64, 42, 0xCAFE, 0xDEAD_BEEF] {
            let (a, b) = run_both(config.clone(), schedule.clone(), seed);
            assert_identical_fires(&a, &b, &format!("stable_load seed={seed:#x}"));
        }
    }

    #[test]
    fn matches_on_cold_start() {
        // Five-orders-of-magnitude cold start — exercises the Phase 1
        // ramp clamp branch of FullRetargetWithClamp heavily.
        let config = TrialConfig {
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e10,
            shares_per_minute: 12.0,
            tick_interval_secs: 60,
        };
        let schedule = HashrateSchedule::stable(1.0e15);
        for &seed in &[1u64, 42, 0xCAFE, 0xDEAD_BEEF] {
            let (a, b) = run_both(config.clone(), schedule.clone(), seed);
            assert_identical_fires(&a, &b, &format!("cold_start seed={seed:#x}"));
        }
    }

    #[test]
    fn matches_on_step_change() {
        // 50% drop at 15min — exercises the reaction-window pathway.
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::step(1.0e15, 5.0e14, 15 * 60);
        for &seed in &[1u64, 42, 0xCAFE, 0xDEAD_BEEF] {
            let (a, b) = run_both(config.clone(), schedule.clone(), seed);
            assert_identical_fires(&a, &b, &format!("step_change seed={seed:#x}"));
        }
    }

    #[test]
    fn matches_across_share_rates() {
        // Sweeps the operational range to catch any rate-dependent
        // divergence in the conversion path.
        let schedule = HashrateSchedule::stable(1.0e15);
        for &spm in &[6.0f32, 12.0, 30.0, 60.0, 120.0] {
            let config = TrialConfig {
                duration_secs: 30 * 60,
                initial_hashrate: 1.0e10,
                shares_per_minute: spm,
                tick_interval_secs: 60,
            };
            for &seed in &[1u64, 42, 0xCAFE] {
                let (a, b) = run_both(config.clone(), schedule.clone(), seed);
                assert_identical_fires(&a, &b, &format!("spm={spm} seed={seed:#x}"));
            }
        }
    }

    #[test]
    fn matches_on_throttle_recovery() {
        // Schedule: 1 PH/s → 700 TH/s for 5min → 1 PH/s. Exercises the
        // transient-recovery pathway.
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::throttle(1.0e15, 7.0e14, 900, 1200);
        for &seed in &[1u64, 42, 0xCAFE] {
            let (a, b) = run_both(config.clone(), schedule.clone(), seed);
            assert_identical_fires(&a, &b, &format!("throttle seed={seed:#x}"));
        }
    }

    #[test]
    fn matches_on_step_up() {
        // 50% rise at 15min — symmetric to the step_change test but
        // exercises the post-step "shares too rare for current target"
        // branch instead of the "too frequent" one.
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::step(1.0e15, 1.5e15, 15 * 60);
        for &seed in &[1u64, 42, 0xCAFE] {
            let (a, b) = run_both(config.clone(), schedule.clone(), seed);
            assert_identical_fires(&a, &b, &format!("step_up seed={seed:#x}"));
        }
    }

    /// Asserts that `run_trial` and `run_trial_observed` produce the
    /// same fire timeline on a Composed algorithm. The introspection
    /// fields differ (None vs Some) but the universal fields — and in
    /// particular the fires — must agree.
    #[test]
    fn observed_and_non_observed_produce_identical_fires() {
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::step(1.0e15, 5.0e14, 15 * 60);
        for &seed in &[1u64, 42, 0xCAFE] {
            let clock_a = Arc::new(MockClock::new(0));
            let composed_a = classic_composed(1.0, clock_a.clone());
            let trial_a = run_trial(composed_a, clock_a, config.clone(), &schedule, seed);

            let clock_b = Arc::new(MockClock::new(0));
            let composed_b = classic_composed(1.0, clock_b.clone());
            let trial_b = run_trial_observed(composed_b, clock_b, config.clone(), &schedule, seed);

            assert_identical_fires(
                &trial_a,
                &trial_b,
                &format!("observed_vs_not seed={seed:#x}"),
            );
        }
    }

    /// Asserts that `run_trial_observed` populates the introspection
    /// fields (delta, threshold, h_estimate) on every tick where the
    /// algorithm reached the decision point — and leaves them `None`
    /// for any early-return ticks (dt ≤ 15s, which shouldn't happen at
    /// the default 60s tick interval but is checked explicitly).
    #[test]
    fn observed_populates_introspection_fields() {
        let clock = Arc::new(MockClock::new(0));
        let composed = classic_composed(1.0, clock.clone());
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::stable(1.0e15);
        let trial = run_trial_observed(composed, clock, config, &schedule, 0xCAFE);

        // Every tick at dt > 15s should have introspection. At the
        // default 60s tick interval, every post-first-fire tick
        // qualifies (the first fire resets the timestamp).
        let mut populated = 0;
        let mut none_count = 0;
        for tick in &trial.ticks {
            if tick.delta.is_some() {
                assert!(tick.threshold.is_some(), "threshold None despite delta Some");
                assert!(tick.h_estimate.is_some(), "h_estimate None despite delta Some");
                populated += 1;
            } else {
                assert!(tick.threshold.is_none());
                assert!(tick.h_estimate.is_none());
                none_count += 1;
            }
        }
        // Sanity: most ticks should be populated. The first tick is
        // dt=60s, well above the 15s guard.
        assert!(
            populated >= trial.ticks.len() - 1,
            "expected most ticks populated; got {}/{} ({} none)",
            populated,
            trial.ticks.len(),
            none_count,
        );
    }

    /// Sanity check on the recorded δ/θ: on fire ticks, the recorded δ
    /// must be ≥ the recorded θ (otherwise the algorithm wouldn't have
    /// fired); on non-fire ticks where the decision was reached, δ
    /// must be < θ.
    #[test]
    fn observed_decision_is_consistent_with_fire_flag() {
        let clock = Arc::new(MockClock::new(0));
        let composed = classic_composed(1.0, clock.clone());
        let config = TrialConfig {
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e10, // cold start — many fires
            shares_per_minute: 12.0,
            tick_interval_secs: 60,
        };
        let schedule = HashrateSchedule::stable(1.0e15);
        let trial = run_trial_observed(composed, clock, config, &schedule, 0xDEAD_BEEF);

        for tick in &trial.ticks {
            if let (Some(delta), Some(threshold)) = (tick.delta, tick.threshold) {
                if tick.fired {
                    assert!(
                        delta >= threshold,
                        "tick t={}: fired but δ={} < θ={}",
                        tick.t_secs, delta, threshold,
                    );
                } else {
                    assert!(
                        delta < threshold,
                        "tick t={}: not fired but δ={} ≥ θ={}",
                        tick.t_secs, delta, threshold,
                    );
                }
            }
        }
    }
}
