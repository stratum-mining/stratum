//! Single-trial simulation execution.
//!
//! A trial drives a vardiff implementation through `duration_secs` of
//! simulated time, recording every tick. The simulation steps in
//! `tick_interval_secs` increments (matching the algorithm's ticker
//! cadence in production); at each tick a Poisson-distributed share count
//! is sampled and added to the vardiff state, then `try_vardiff` is
//! called, then a [`TickRecord`] is pushed onto the trial.
//!
//! This per-tick model is preferred over individual share-arrival sampling
//! for two reasons:
//!
//! 1. **Rate independence.** Poisson sampling works for any λ from
//!    near-zero to millions; inter-arrival sampling would need sub-second
//!    time resolution to model high share rates accurately.
//! 2. **Algorithm fidelity.** The algorithm only acts at tick boundaries,
//!    so within-tick share timing is irrelevant to its behavior. Per-tick
//!    sampling produces identical algorithm outcomes at a fraction of the
//!    cost.
//!
//! ## Glass-box recording
//!
//! Every tick is recorded as a [`TickRecord`], not just the moments
//! the algorithm fires. Each record carries universal observables
//! (timestamp, share count, fire flag, new hashrate) plus optional
//! introspection fields (δ, θ, H̃). For algorithms that implement
//! [`Observable`] the introspection fields are populated by
//! [`run_trial_observed`]; for algorithms that don't, the optional
//! fields are `None` and only the universal observables are
//! available.
//!
//! Behavioral metrics (convergence, settled accuracy, jitter,
//! reaction time, ramp_target_overshoot) operate on `trial.fires()` —
//! a filtered view over `trial.ticks` selecting only `fired == true`.
//! Introspection-only metrics (bias, variance) consume the optional
//! δ/θ/H̃ fields populated by `run_trial_observed` for algorithms
//! that implement `Observable`.
//!
//! ## Schedule-change approximation
//!
//! The expected share count for a tick is computed from the
//! [`HashrateSchedule`] evaluated at the tick interval's **midpoint**.
//! For schedules whose transitions land at tick boundaries (the default
//! scenarios in [`crate::baseline`] all do — step at `15min = 900s =
//! 15 × tick_interval`), the midpoint correctly identifies the segment
//! that covers the entire tick and the simulation is exact.
//!
//! For schedules that transition *mid-tick*, the midpoint approximation
//! treats the entire tick as if it were at the rate of whichever segment
//! contains the midpoint. The error is bounded — at most one tick's
//! worth of shares charged to the wrong segment — but if a caller cares
//! about sub-tick-precision schedule changes, they should either align
//! changes to tick boundaries or extend `run_trial` to split intervals at
//! schedule boundaries.
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
    /// The algorithm's initial belief about miner hashrate (H/s).
    /// Determines the initial target. If far from the schedule's true
    /// hashrate, the trial exercises convergence (Phase 1).
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
            // 30 simulated minutes is enough to observe Phase 1 + Phase 3
            // in the most common scenarios; long enough to be statistically
            // useful, short enough that 1000 trials complete in seconds of
            // wall clock.
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e10,
            shares_per_minute: 12.0,
            tick_interval_secs: 60,
        }
    }
}

/// Re-exported from production: a snapshot of the algorithm's
/// decision at a single tick.
///
/// Captured by [`Observable::last_decision`] after each `try_vardiff`
/// call and pasted into the corresponding [`TickRecord`] by
/// [`run_trial_observed`]. `None` for ticks where the algorithm
/// returned early before computing a decision (i.e., `dt_secs <= 15`).
///
/// The concrete struct lives in
/// [`channels_sv2::vardiff::composed::DecisionRecord`] so production
/// `Composed` instances can populate it without depending on the sim
/// crate.
pub use channels_sv2::vardiff::composed::DecisionRecord;

/// Algorithms that expose introspection of their last decision.
///
/// Implemented by `Composed<E, S, B, U>` (in the [`crate::composed`]
/// module). Production `Vardiff` impls such as `VardiffState` do *not*
/// implement this trait — they have no internal decision to expose. The
/// trial driver [`run_trial_observed`] is generic over `V: Vardiff +
/// Observable` and uses [`Observable::last_decision`] to populate the
/// optional δ / θ / H̃ fields on each [`TickRecord`].
///
/// The contract: after a call to [`Vardiff::try_vardiff`], the implementor
/// returns:
///
/// - `Some(record)` if try_vardiff reached the decision point (computed
///   δ and θ before deciding to fire or hold).
/// - `None` if try_vardiff returned early before computing the decision
///   (e.g., `dt_secs <= 15` guard).
pub trait Observable {
    fn last_decision(&self) -> Option<DecisionRecord>;
}

/// A single per-tick record captured during a trial.
///
/// Universal fields (timestamp, share count, fire flag, new hashrate,
/// current hashrate) are always populated. Introspection fields
/// (δ, θ, H̃) are populated only when the trial is driven by
/// [`run_trial_observed`] against an [`Observable`] vardiff
/// implementation; otherwise they're `None`.
#[derive(Debug, Clone)]
pub struct TickRecord {
    /// Simulated time at the end of this tick.
    pub t_secs: u64,
    /// Number of shares observed in the interval ending at this tick.
    pub n_shares: u32,
    /// True iff `try_vardiff` returned `Ok(Some(_))` on this tick.
    pub fired: bool,
    /// The new hashrate the algorithm set; `Some` iff `fired`.
    pub new_hashrate: Option<f32>,
    /// The algorithm's hashrate entering this tick (i.e., before any
    /// fire occurring at this tick).
    pub current_hashrate_before: f32,

    /// Test statistic δ at this tick. `None` for non-observable
    /// algorithms, or for ticks where try_vardiff returned early before
    /// computing δ (i.e., `dt_secs <= 15`).
    pub delta: Option<f64>,
    /// Decision threshold θ at this tick. `None` for non-observable
    /// algorithms or early-return ticks.
    pub threshold: Option<f64>,
    /// Estimator's belief H̃ at this tick. `None` for non-observable
    /// algorithms or early-return ticks.
    pub h_estimate: Option<f32>,
}

/// Result of running a single trial.
#[derive(Debug, Clone)]
pub struct Trial {
    /// The configuration the trial was run under.
    pub config: TrialConfig,
    /// The seed that produced this trial. Carried in the result so that
    /// a failing trial can be reproduced in isolation by replaying with
    /// the same `(config, schedule, seed)` triple.
    pub seed: u64,
    /// Per-tick records, in chronological order. Length is
    /// `duration_secs / tick_interval_secs`. Fires are the subset where
    /// `fired == true`; access them ergonomically via [`Trial::fires`].
    pub ticks: Vec<TickRecord>,
    /// The algorithm's hashrate estimate at trial end.
    pub final_hashrate: f32,
    /// The true miner hashrate (from the schedule) at trial end.
    pub true_hashrate_at_end: f32,
}

impl Trial {
    /// Returns the subset of `ticks` where the algorithm fired, as a
    /// collected `Vec<&TickRecord>` for ergonomic indexed access in
    /// metric functions. One allocation per call; insignificant in
    /// practice for trials with ~30 ticks.
    pub fn fires(&self) -> Vec<&TickRecord> {
        self.ticks.iter().filter(|t| t.fired).collect()
    }

    /// Number of fire events in this trial.
    pub fn fire_count(&self) -> usize {
        self.ticks.iter().filter(|t| t.fired).count()
    }
}

/// Runs one simulation trial against a vardiff implementation that does
/// not expose introspection. The trial captures fires and universal
/// observables; the introspection fields of [`TickRecord`] (δ, θ, H̃)
/// are all `None`.
///
/// For algorithms that implement [`Observable`] (e.g., `Composed<...>`),
/// prefer [`run_trial_observed`] to populate the introspection fields.
///
/// Given a fixed `(config, schedule, seed)` triple, the trial is fully
/// reproducible — the same algorithm produces an identical tick timeline
/// every time.
pub fn run_trial<V: Vardiff>(
    vardiff: V,
    clock: Arc<MockClock>,
    config: TrialConfig,
    schedule: &HashrateSchedule,
    seed: u64,
) -> Trial {
    run_trial_with_observer(vardiff, clock, config, schedule, seed, |_| None)
}

/// Runs one simulation trial against a vardiff implementation that
/// exposes introspection. Identical to [`run_trial`] except that the
/// δ / θ / H̃ fields on each [`TickRecord`] are populated from
/// [`Observable::last_decision`] after each `try_vardiff` call.
///
/// Use this driver to characterize algorithms via the richer metrics
/// (bias, variance, overshoot, etc.) that consume the introspection
/// fields. Bit-equivalence with [`run_trial`] holds on the universal
/// fields by construction — the only difference is whether the
/// optional fields are `Some` or `None`.
pub fn run_trial_observed<V: Vardiff + Observable>(
    vardiff: V,
    clock: Arc<MockClock>,
    config: TrialConfig,
    schedule: &HashrateSchedule,
    seed: u64,
) -> Trial {
    run_trial_with_observer(vardiff, clock, config, schedule, seed, |v| {
        v.last_decision()
    })
}

/// Internal trial driver shared by [`run_trial`] and
/// [`run_trial_observed`]. The `observe` closure is called once per
/// tick, after `try_vardiff` returns, and its result populates the
/// introspection fields of the corresponding [`TickRecord`].
///
/// At each tick:
///
/// 1. Compute the expected share count for the interval as
///    `λ = (true_hashrate / estimated_hashrate) * shares_per_minute *
///    (tick_interval_secs / 60)`. The schedule is evaluated at the
///    interval midpoint, which is exact for schedules that change at
///    tick boundaries.
/// 2. Sample `n_shares ~ Poisson(λ)`.
/// 3. Bulk-add the count to the vardiff state via [`Vardiff::add_shares`].
/// 4. Advance the clock to the tick boundary, call `try_vardiff`.
/// 5. Invoke the observer (`None` for [`run_trial`], `Some(decision)`
///    for [`run_trial_observed`] when the algorithm computed a decision).
/// 6. Push a [`TickRecord`] capturing everything.
fn run_trial_with_observer<V: Vardiff, F: FnMut(&V) -> Option<DecisionRecord>>(
    mut vardiff: V,
    clock: Arc<MockClock>,
    config: TrialConfig,
    schedule: &HashrateSchedule,
    seed: u64,
    mut observe: F,
) -> Trial {
    clock.set(0);

    let mut rng = XorShift64::new(seed);
    let mut current_hashrate = config.initial_hashrate;
    let mut current_target = hashrate_to_target_safe(current_hashrate, config.shares_per_minute);
    let n_ticks_hint = (config.duration_secs / config.tick_interval_secs.max(1)) as usize;
    let mut ticks: Vec<TickRecord> = Vec::with_capacity(n_ticks_hint);

    let mut last_tick_at: u64 = 0;
    let mut tick_at: u64 = config.tick_interval_secs;

    while tick_at <= config.duration_secs {
        // Sample share count for this tick interval using the true
        // hashrate at the interval midpoint. For schedules that change
        // at a tick boundary (the default scenarios), the midpoint
        // correctly identifies the segment that covers the entire
        // interval.
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
        let hashrate_before = current_hashrate;
        clock.set(tick_at);
        let result =
            vardiff.try_vardiff(current_hashrate, &current_target, config.shares_per_minute);

        // Snapshot the algorithm's last decision (if observable). The
        // observer is called *after* try_vardiff so that the decision
        // record reflects the call that just completed.
        let decision = observe(&vardiff);

        let (fired, new_hashrate_opt) = match &result {
            Ok(Some(new_h)) => (true, Some(*new_h)),
            _ => (false, None),
        };

        ticks.push(TickRecord {
            t_secs: tick_at,
            n_shares,
            fired,
            new_hashrate: new_hashrate_opt,
            current_hashrate_before: hashrate_before,
            delta: decision.as_ref().map(|d| d.delta),
            threshold: decision.as_ref().map(|d| d.threshold),
            h_estimate: decision.as_ref().map(|d| d.h_estimate),
        });

        if let Some(new_h) = new_hashrate_opt {
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
        ticks,
        final_hashrate: current_hashrate,
    }
}

/// Wraps `hash_rate_to_target` with floor values on inputs so degenerate
/// edge cases (zero or negative hashrate or share rate, which can arise
/// from extreme algorithm output during testing) don't propagate errors.
///
/// Hashrate is floored at 1.0 H/s — the smallest physically meaningful
/// value and well below any miner's true rate, so this clamp can't
/// influence realistic trials. Share rate is floored at 0.001 spm to
/// prevent division by zero in the underlying conversion.
fn hashrate_to_target_safe(hashrate: f32, shares_per_minute: f32) -> Target {
    hash_rate_to_target(
        hashrate.max(1.0) as f64,
        shares_per_minute.max(0.001) as f64,
    )
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
        // Phase 1 ramp-up from default 1e10 → 1e15 should produce
        // multiple fires.
        assert!(
            trial.fire_count() >= 1,
            "Expected at least one fire during Phase 1 ramp, got {}",
            trial.fire_count()
        );
        // The tick count should match the trial duration / interval.
        assert_eq!(trial.ticks.len(), 30);
    }

    #[test]
    fn trial_is_deterministic_for_same_seed() {
        let (v1, c1) = make_vardiff();
        let (v2, c2) = make_vardiff();
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::stable(1.0e15);
        let t1 = run_trial(v1, c1, config.clone(), &schedule, 12345);
        let t2 = run_trial(v2, c2, config, &schedule, 12345);
        assert_eq!(t1.ticks.len(), t2.ticks.len());
        for (a, b) in t1.ticks.iter().zip(t2.ticks.iter()) {
            assert_eq!(a.t_secs, b.t_secs);
            assert_eq!(a.fired, b.fired);
            assert_eq!(
                a.new_hashrate.map(f32::to_bits),
                b.new_hashrate.map(f32::to_bits)
            );
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
        let fires_1 = t1.fires();
        let fires_2 = t2.fires();
        let same = fires_1.len() == fires_2.len()
            && fires_1
                .iter()
                .zip(fires_2.iter())
                .all(|(a, b)| a.t_secs == b.t_secs);
        assert!(
            !same,
            "Two seeds produced identical fire timelines; RNG broken?"
        );
    }

    #[test]
    fn step_change_in_schedule_is_observable() {
        let (vardiff, clock) = make_vardiff();
        let config = TrialConfig {
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e15,
            shares_per_minute: 12.0,
            tick_interval_secs: 60,
        };
        // Halve hashrate at 15 min — algorithm should respond.
        let schedule = HashrateSchedule::step(1.0e15, 5.0e14, 15 * 60);
        let trial = run_trial(vardiff, clock, config, &schedule, 9001);
        let post_step_fires = trial
            .ticks
            .iter()
            .filter(|t| t.fired && t.t_secs > 15 * 60)
            .count();
        assert!(
            post_step_fires >= 1,
            "Expected at least one fire after 50% load drop at 15 min; got {} post-step fires (total {})",
            post_step_fires,
            trial.fire_count(),
        );
    }

    /// Regression test for the share-rate-cap bug fixed in Phase 4.5. At
    /// high share rates and a cold start, the previous inter-arrival
    /// simulation produced ~1 share/sec regardless of configured rate,
    /// leaving the algorithm permanently stuck at the initial estimate.
    /// With per-tick Poisson sampling the algorithm should converge
    /// correctly.
    #[test]
    fn high_share_rate_cold_start_converges() {
        let (vardiff, clock) = make_vardiff();
        let config = TrialConfig {
            duration_secs: 30 * 60,
            initial_hashrate: 1.0e10,
            shares_per_minute: 120.0,
            tick_interval_secs: 60,
        };
        let schedule = HashrateSchedule::stable(1.0e15);
        let trial = run_trial(vardiff, clock, config, &schedule, 0xC0FFEE);

        assert!(
            trial.fire_count() >= 5,
            "Expected ≥ 5 fires during 5-order-of-magnitude ramp; got {}",
            trial.fire_count()
        );

        let ratio = trial.final_hashrate as f64 / trial.true_hashrate_at_end as f64;
        assert!(
            ratio > 0.1 && ratio < 10.0,
            "Final estimate {} is more than 10× off from truth {}; ratio = {}",
            trial.final_hashrate,
            trial.true_hashrate_at_end,
            ratio,
        );
    }

    #[test]
    fn non_observable_algorithm_leaves_introspection_fields_none() {
        // VardiffState does not implement Observable. Running it via the
        // regular run_trial driver must populate the universal fields
        // and leave delta / threshold / h_estimate as None.
        let (vardiff, clock) = make_vardiff();
        let config = TrialConfig::default();
        let schedule = HashrateSchedule::stable(1.0e15);
        let trial = run_trial(vardiff, clock, config, &schedule, 42);
        for tick in &trial.ticks {
            assert!(
                tick.delta.is_none(),
                "delta should be None for non-observable algorithm"
            );
            assert!(tick.threshold.is_none(), "threshold should be None");
            assert!(tick.h_estimate.is_none(), "h_estimate should be None");
        }
    }
}
