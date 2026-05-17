//! Sim-side facade over [`channels_sv2::vardiff::composed`].
//!
//! The four-axis types and the `Composed` adapter live in the
//! production crate (`channels_sv2::vardiff::composed`) so production
//! call sites can construct compositions directly. This module:
//!
//! - re-exports the production types so existing sim-internal callers
//!   keep working with their `use crate::composed::*;` paths
//! - adds the sim-only [`crate::trial::Observable`] extension trait
//!   to `Composed<E, S, B, U>`, exposing the `last_decision` field
//!   that the trial driver reads for per-tick introspection
//! - hosts the fire-for-fire equivalence-test suite that asserts
//!   `classic_composed()` reproduces `VardiffState`'s trajectory bit
//!   exactly — load-bearing for the framework's "swap one axis,
//!   attribute the change" capability

pub use channels_sv2::vardiff::composed::*;

use crate::trial::Observable;

// The four trait bounds and `Composed` resolve through the glob
// re-export above. `DecisionRecord` is the same type as the one in
// `crate::trial`, which itself re-exports from
// `channels_sv2::vardiff::composed`.
impl<E, S, B, U> Observable for Composed<E, S, B, U>
where
    E: Estimator,
    S: Statistic,
    B: Boundary,
    U: UpdateRule,
{
    fn last_decision(&self) -> Option<crate::trial::DecisionRecord> {
        self.last_decision
    }
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
    use std::sync::Arc;

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
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::step(1.0e15, 5.0e14, 15 * 60);
        for &seed in &[1u64, 42, 0xCAFE, 0xDEAD_BEEF] {
            let (a, b) = run_both(config.clone(), schedule.clone(), seed);
            assert_identical_fires(&a, &b, &format!("step_change seed={seed:#x}"));
        }
    }

    #[test]
    fn matches_across_share_rates() {
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
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::throttle(1.0e15, 7.0e14, 900, 1200);
        for &seed in &[1u64, 42, 0xCAFE] {
            let (a, b) = run_both(config.clone(), schedule.clone(), seed);
            assert_identical_fires(&a, &b, &format!("throttle seed={seed:#x}"));
        }
    }

    #[test]
    fn matches_on_step_up() {
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::step(1.0e15, 1.5e15, 15 * 60);
        for &seed in &[1u64, 42, 0xCAFE] {
            let (a, b) = run_both(config.clone(), schedule.clone(), seed);
            assert_identical_fires(&a, &b, &format!("step_up seed={seed:#x}"));
        }
    }

    /// Asserts that `run_trial` and `run_trial_observed` produce the
    /// same fire timeline on a Composed algorithm.
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
    /// algorithm reached the decision point.
    #[test]
    fn observed_populates_introspection_fields() {
        let clock = Arc::new(MockClock::new(0));
        let composed = classic_composed(1.0, clock.clone());
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::stable(1.0e15);
        let trial = run_trial_observed(composed, clock, config, &schedule, 0xCAFE);

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
        assert!(
            populated >= trial.ticks.len() - 1,
            "expected most ticks populated; got {}/{} ({} none)",
            populated,
            trial.ticks.len(),
            none_count,
        );
    }

    /// Sanity check on the recorded δ/θ: on fire ticks, the recorded
    /// δ must be ≥ the recorded θ (otherwise the algorithm wouldn't
    /// have fired); on non-fire ticks where the decision was reached,
    /// δ must be < θ.
    #[test]
    fn observed_decision_is_consistent_with_fire_flag() {
        let clock = Arc::new(MockClock::new(0));
        let composed = classic_composed(1.0, clock.clone());
        let config = TrialConfig {
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e10,
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
