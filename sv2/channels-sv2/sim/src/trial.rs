//! Single-trial simulation execution.
//!
//! A trial drives a vardiff implementation through `duration_secs` of
//! simulated time, recording every retarget event. The simulation steps in
//! `tick_interval_secs` increments (matching the algorithm's ticker cadence
//! in production); at each tick a Poisson-distributed share count is sampled
//! and added to the vardiff state, then `try_vardiff` is called.
//!
//! This per-tick model is preferred over individual share-arrival sampling
//! for two reasons:
//!
//! 1. **Rate independence.** Poisson sampling works for any λ from near-zero
//!    to millions; inter-arrival sampling would need sub-second time
//!    resolution to model high share rates accurately.
//! 2. **Algorithm fidelity.** The algorithm only acts at tick boundaries, so
//!    within-tick share timing is irrelevant to its behavior. Per-tick
//!    sampling produces identical algorithm outcomes at a fraction of the
//!    cost.
//!
//! ## Schedule-change approximation
//!
//! The expected share count for a tick is computed from the [`HashrateSchedule`]
//! evaluated at the tick interval's **midpoint**. For schedules whose
//! transitions land at tick boundaries (the default scenarios in
//! [`crate::baseline`] all do — step at `15min = 900s = 15 × tick_interval`),
//! the midpoint correctly identifies the segment that covers the entire tick
//! and the simulation is exact.
//!
//! For schedules that transition *mid-tick* (a custom scenario where the
//! change time isn't a multiple of `tick_interval_secs`), the midpoint
//! approximation treats the entire tick as if it were at the rate of
//! whichever segment contains the midpoint. The error is bounded — at most
//! one tick's worth of shares charged to the wrong segment — but if a
//! caller cares about sub-tick-precision schedule changes, they should
//! either align changes to tick boundaries or extend `run_trial` to split
//! intervals at schedule boundaries.
//!
//! See [`run_trial`] for the entry point.

use crate::rng::{sample_poisson, XorShift64};
use crate::schedule::HashrateSchedule;
use bitcoin::Target;
use channels_sv2::target::hash_rate_to_target;
use channels_sv2::vardiff::{MockClock, Vardiff};
use std::sync::Arc;

/// Configuration parameters for a single simulation trial.
#[derive(Debug, Clone)]
pub struct TrialConfig {
    /// Duration of the trial in simulated seconds.
    pub duration_secs: u64,
    /// The algorithm's initial belief about miner hashrate (H/s). Determines
    /// the initial target. If far from the schedule's true hashrate, the
    /// trial exercises convergence (Phase 1).
    pub initial_hashrate: f32,
    /// Configured shares-per-minute target the algorithm tries to achieve.
    pub shares_per_minute: f32,
    /// Cadence at which the algorithm's tick fires, in simulated seconds.
    /// Defaults to 60; should match production for realistic measurement.
    pub tick_interval_secs: u64,
}

impl Default for TrialConfig {
    fn default() -> Self {
        Self {
            // 30 simulated minutes is enough to observe Phase 1 + Phase 3 in
            // the most common scenarios; long enough to be statistically useful,
            // short enough that 1000 trials complete in seconds of wall clock.
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e10,
            shares_per_minute: 12.0,
            tick_interval_secs: 60,
        }
    }
}

/// A single retarget event captured during a trial.
#[derive(Debug, Clone)]
pub struct FireEvent {
    /// Simulated time at which `try_vardiff` returned `Some(new_hashrate)`.
    pub at_secs: u64,
    /// The algorithm's hashrate estimate prior to this fire.
    pub old_hashrate: f32,
    /// The algorithm's hashrate estimate after this fire.
    pub new_hashrate: f32,
}

/// Result of running a single trial.
#[derive(Debug, Clone)]
pub struct Trial {
    /// The configuration the trial was run under.
    pub config: TrialConfig,
    /// The seed that produced this trial. Carried in the result so that a
    /// failing trial can be reproduced in isolation by replaying with the
    /// same `(config, schedule, seed)` triple.
    pub seed: u64,
    /// All retarget events, in chronological order.
    pub fires: Vec<FireEvent>,
    /// The algorithm's hashrate estimate at trial end.
    pub final_hashrate: f32,
    /// The true miner hashrate (from the schedule) at trial end.
    pub true_hashrate_at_end: f32,
}

/// Runs one simulation trial against a vardiff implementation.
///
/// `vardiff` is moved into the function and driven through the trial; its
/// final state is not returned (the necessary information is captured in
/// `Trial.fires` and `Trial.final_hashrate`). `clock` is the same
/// [`MockClock`] that `vardiff` was constructed with — the simulation
/// advances it forward via `clock.set()` as it processes each tick.
///
/// The trial loop steps in `tick_interval_secs` increments. At each tick:
///
/// 1. Compute the expected share count for the interval as
///    `λ = (true_hashrate / estimated_hashrate) * shares_per_minute *
///    (tick_interval_secs / 60)`. The schedule is evaluated at the interval
///    midpoint, which is exact for schedules that change at tick boundaries
///    (the default scenarios in [`crate::baseline`]) and a small
///    approximation otherwise.
/// 2. Sample `n_shares ~ Poisson(λ)` via [`sample_poisson`].
/// 3. Bulk-add the count to the vardiff state via
///    [`Vardiff::add_shares`].
/// 4. Advance the clock and call `try_vardiff`. If it returns
///    `Some(new_hashrate)`, record a [`FireEvent`], update the estimate, and
///    derive a new target via [`channels_sv2::target::hash_rate_to_target`].
///
/// Given a fixed `(config, schedule, seed)` triple, the trial is fully
/// reproducible — the same algorithm produces an identical fire timeline
/// every time.
pub fn run_trial<V: Vardiff>(
    mut vardiff: V,
    clock: Arc<MockClock>,
    config: TrialConfig,
    schedule: &HashrateSchedule,
    seed: u64,
) -> Trial {
    clock.set(0);

    let mut rng = XorShift64::new(seed);
    let mut current_hashrate = config.initial_hashrate;
    let mut current_target =
        hashrate_to_target_safe(current_hashrate, config.shares_per_minute);
    let mut fires: Vec<FireEvent> = Vec::new();

    let mut last_tick_at: u64 = 0;
    let mut tick_at: u64 = config.tick_interval_secs;

    while tick_at <= config.duration_secs {
        // Sample share count for this tick interval using the true hashrate
        // at the interval midpoint. For schedules that change at a tick
        // boundary (the default scenarios), the midpoint correctly identifies
        // the segment that covers the entire interval.
        let interval_midpoint = (last_tick_at + tick_at) / 2;
        let true_h = schedule.at(interval_midpoint) as f64;
        let est_h = current_hashrate as f64;
        let interval_secs = (tick_at - last_tick_at) as f64;

        let lambda = if est_h > 0.0 {
            (true_h / est_h) * (config.shares_per_minute as f64) * (interval_secs / 60.0)
        } else {
            0.0
        };
        let n_shares = sample_poisson(&mut rng, lambda);
        vardiff.add_shares(n_shares);

        // Advance the clock to the tick boundary and invoke the algorithm.
        clock.set(tick_at);
        if let Ok(Some(new_h)) =
            vardiff.try_vardiff(current_hashrate, &current_target, config.shares_per_minute)
        {
            fires.push(FireEvent {
                at_secs: tick_at,
                old_hashrate: current_hashrate,
                new_hashrate: new_h,
            });
            current_hashrate = new_h;
            current_target = hashrate_to_target_safe(new_h, config.shares_per_minute);
        }

        last_tick_at = tick_at;
        tick_at = tick_at.saturating_add(config.tick_interval_secs);
    }

    Trial {
        true_hashrate_at_end: schedule.at(config.duration_secs),
        config,
        seed,
        fires,
        final_hashrate: current_hashrate,
    }
}

/// Wraps `hash_rate_to_target` with floor values on inputs so degenerate edge
/// cases (zero or negative hashrate or share rate, which can arise from
/// extreme algorithm output during testing) don't propagate errors.
///
/// Hashrate is floored at 1.0 H/s — the smallest physically meaningful value
/// and well below any miner's true rate, so this clamp can't influence
/// realistic trials. Share rate is floored at 0.001 spm to prevent division
/// by zero in the underlying conversion.
fn hashrate_to_target_safe(hashrate: f32, shares_per_minute: f32) -> Target {
    hash_rate_to_target(hashrate.max(1.0) as f64, shares_per_minute.max(0.001) as f64)
        .expect("hash_rate_to_target with positive inputs should not fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use channels_sv2::VardiffState;

    fn make_vardiff() -> (VardiffState, Arc<MockClock>) {
        let clock = Arc::new(MockClock::new(0));
        let vardiff =
            VardiffState::new_with_clock(1.0, clock.clone()).expect("vardiff state should build");
        (vardiff, clock)
    }

    #[test]
    fn smoke_test_stable_load_produces_a_trial() {
        let (vardiff, clock) = make_vardiff();
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::stable(1.0e15);
        let trial = run_trial(vardiff, clock, config, &schedule, 0xCAFE);
        assert_eq!(trial.true_hashrate_at_end, 1.0e15);
        // Phase 1 ramp-up from default 1e10 → 1e15 should produce multiple fires.
        assert!(
            !trial.fires.is_empty(),
            "Expected at least one fire during Phase 1 ramp, got {}",
            trial.fires.len()
        );
    }

    #[test]
    fn trial_is_deterministic_for_same_seed() {
        let (v1, c1) = make_vardiff();
        let (v2, c2) = make_vardiff();
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::stable(1.0e15);
        let t1 = run_trial(v1, c1, config.clone(), &schedule, 12345);
        let t2 = run_trial(v2, c2, config, &schedule, 12345);
        assert_eq!(t1.fires.len(), t2.fires.len());
        for (a, b) in t1.fires.iter().zip(t2.fires.iter()) {
            assert_eq!(a.at_secs, b.at_secs);
            // Float bit-equality holds for deterministic execution paths in
            // the same toolchain.
            assert_eq!(a.new_hashrate.to_bits(), b.new_hashrate.to_bits());
        }
        assert_eq!(t1.final_hashrate.to_bits(), t2.final_hashrate.to_bits());
    }

    #[test]
    fn different_seeds_produce_different_trials() {
        let (v1, c1) = make_vardiff();
        let (v2, c2) = make_vardiff();
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::stable(1.0e15);
        let t1 = run_trial(v1, c1, config.clone(), &schedule, 1);
        let t2 = run_trial(v2, c2, config, &schedule, 2);
        let same = t1.fires.len() == t2.fires.len()
            && t1
                .fires
                .iter()
                .zip(t2.fires.iter())
                .all(|(a, b)| a.at_secs == b.at_secs);
        assert!(!same, "Two seeds produced identical fire timelines; RNG broken?");
    }

    #[test]
    fn step_change_in_schedule_is_observable() {
        let (vardiff, clock) = make_vardiff();
        let config = TrialConfig {
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e15, // start aligned with the pre-change rate
            shares_per_minute: 12.0,
            tick_interval_secs: 60,
        };
        // Halve hashrate at 15 min — algorithm should respond.
        let schedule = HashrateSchedule::step(1.0e15, 5.0e14, 15 * 60);
        let trial = run_trial(vardiff, clock, config, &schedule, 9001);
        let post_step_fires = trial
            .fires
            .iter()
            .filter(|f| f.at_secs > 15 * 60)
            .count();
        assert!(
            post_step_fires >= 1,
            "Expected at least one fire after 50% load drop at 15 min; got {} post-step fires (total {})",
            post_step_fires,
            trial.fires.len()
        );
    }

    /// Regression test for the share-rate-cap bug fixed in Phase 4.5. At high
    /// share rates and a cold start, the previous inter-arrival simulation
    /// produced ~1 share/sec regardless of configured rate, leaving the
    /// algorithm permanently stuck at the initial estimate. With per-tick
    /// Poisson sampling the algorithm should converge correctly.
    #[test]
    fn high_share_rate_cold_start_converges() {
        let (vardiff, clock) = make_vardiff();
        let config = TrialConfig {
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e10, // 10 GH/s
            shares_per_minute: 120.0, // high rate where the old simulation broke
            tick_interval_secs: 60,
        };
        let schedule = HashrateSchedule::stable(1.0e15); // 1 PH/s
        let trial = run_trial(vardiff, clock, config, &schedule, 0xC0FFEE);

        // The algorithm should fire multiple times during Phase 1 ramp.
        assert!(
            trial.fires.len() >= 5,
            "Expected ≥ 5 fires during 5-order-of-magnitude ramp; got {}",
            trial.fires.len()
        );

        // The final estimate should be within an order of magnitude of truth.
        // Tight accuracy is a separate concern characterized by the metric
        // layer; here we just verify the algorithm got close.
        let ratio = trial.final_hashrate as f64 / trial.true_hashrate_at_end as f64;
        assert!(
            ratio > 0.1 && ratio < 10.0,
            "Final estimate {} is more than 10× off from truth {}; ratio = {}",
            trial.final_hashrate,
            trial.true_hashrate_at_end,
            ratio,
        );
    }
}
