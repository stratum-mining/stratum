//! # Vardiff simulation framework
//!
//! Deterministic in-process simulation harness for characterizing the
//! behavioral attributes of any [`channels_sv2::vardiff::Vardiff`]
//! implementation. The framework decomposes a vardiff algorithm into
//! four orthogonal axes (Estimator, Statistic, Boundary, UpdateRule)
//! and characterizes each in isolation so metric changes can be
//! attributed to the axis that changed. See `sim/docs/DESIGN.md` for
//! the architectural reference and `sim/docs/FINDINGS.md` for the
//! cross-algorithm characterization results.
//!
//! ## What this crate provides
//!
//! - [`run_trial`]: drives a single [`channels_sv2::vardiff::Vardiff`]
//!   implementation through `duration_secs` of simulated time against a
//!   Poisson share stream and a programmable hashrate schedule, recording
//!   a dense per-tick [`TickRecord`] timeline.
//! - [`run_trial_observed`]: as above, but also populates the optional
//!   introspection fields (δ, θ, H̃) for algorithms that implement
//!   [`Observable`] (e.g. `Composed<E, S, B, U>` from the
//!   [`composed`] module).
//! - [`HashrateSchedule`]: step-function description of the miner's true
//!   hashrate over time — supports stable, step-change, and arbitrary
//!   piecewise-constant scenarios.
//! - [`XorShift64`]: deterministic RNG used for share-arrival sampling.
//!   Trials are fully reproducible from a `(config, schedule, seed)` triple.
//! - [`metrics`]: distribution-computing functions over `Vec<Trial>` —
//!   convergence time, settled accuracy, steady-state jitter, reaction time,
//!   reaction sensitivity. Each metric returns a [`Distribution`] supporting
//!   percentile queries (p10–p99), mean, and count.
//! - [`composed`]: the four-axis decomposition (Estimator, Statistic,
//!   Boundary, UpdateRule) and the `Composed<E, S, B, U>` adapter that
//!   carries `impl Vardiff`.
//!
//! ## Unit conventions
//!
//! Two kinds of "percentage-like" quantities flow through the
//! framework. They use different conventions, so the same numeric
//! value `0.10` means different things depending on where you see it:
//!
//! | Location | Convention | Example |
//! | --- | --- | --- |
//! | Statistic / Boundary (`δ`, `θ` in [`composed`]) | percentage points | `δ = 60.0` ⇔ 60% |
//! | `bias_*`, `ramp_target_overshoot_*` in [`MetricValues`] | fraction | `bias_mean = 0.10` ⇔ +10% |
//! | `settled_accuracy_*` in [`MetricValues`] | fraction | `0.04` ⇔ 4% off truth |
//! | `convergence_rate`, `reaction_rate` | fraction in [0, 1] | `0.95` ⇔ 95% |
//! | `jitter_*_per_min` | rate (fires/minute) | `0.04` ⇔ 0.04 fires/min |
//! | `convergence_p*_secs`, `reaction_p*_secs` | seconds | `420.0` ⇔ 420 seconds |
//! | `variance_*` | dimensionless (population variance of `H̃/H_true`) | `0.01` ⇔ σ²(H̃/H) = 0.01 |
//!
//! The Statistic/Boundary percentage-point convention is preserved for
//! bit-equivalence with `VardiffState`'s internal formula
//! (`hashrate_delta_percentage = ... * 100.0`). Everywhere else uses
//! fractions because the underlying math is fractional. Key names with
//! `_secs` / `_per_min` suffixes carry their units in the name; others
//! are documented in the metric impl that emits them.
//!
//! ## Quickstart
//!
//! ```ignore
//! use std::sync::Arc;
//! use channels_sv2::vardiff::MockClock;
//! use channels_sv2::VardiffState;
//! use vardiff_sim::{run_trial, HashrateSchedule, TrialConfig};
//!
//! let clock = Arc::new(MockClock::new(0));
//! let vardiff = VardiffState::new_with_clock(1.0, clock.clone()).unwrap();
//! let schedule = HashrateSchedule::stable(1.0e15); // 1 PH/s constant
//! let config = TrialConfig::default();
//! let trial = run_trial(vardiff, clock, config, &schedule, /* seed */ 0xDEADBEEF);
//! println!("Algorithm fired {} times", trial.fire_count());
//! ```

pub mod baseline;
pub mod composed;
pub mod grid;
pub mod metrics;
pub mod regression;
pub mod rng;
pub mod schedule;
pub mod trial;

pub use baseline::{phases_to_trial, Cell, CellResult, Phase, Scenario, RAMP_SEGMENTS};

pub use grid::{
    run_cell_with_algorithm, AlgorithmSpec, AsObservable, Grid, ObservableVardiff, VardiffBox,
    VardiffFactory,
};

pub use composed::{
    classic_composed, AbsoluteRatio, Boundary, ClassicComposed, Composed, CumulativeCounter,
    Estimator, EstimatorContext, EstimatorSnapshot, EwmaEstimator, FullRetargetNoClamp,
    FullRetargetWithClamp, PartialRetarget, PoissonCI, SlidingWindowEstimator, Statistic,
    StepFunction, UpdateRule,
};

pub use metrics::{
    bootstrap_percentile_ci, convergence_time_distribution, convergence_time_for_trial,
    derived_registry, jitter_distribution, jitter_for_trial, reaction_sensitivity,
    reaction_time_distribution, reaction_time_for_trial, registry, registry_by_id,
    settled_accuracy_distribution, settled_accuracy_for_trial, BaselineValue, Bias,
    ConvergenceTime, DecouplingScore, DerivedMetric, Direction, Distribution, Jitter, Metric,
    MetricCategory, MetricClass, MetricValues, RampTargetOvershoot, ReactionAsymmetry,
    ReactionTime, ScenarioFilter, SettledAccuracy, SummaryFmt, SummarySpec, Tolerance,
    ToleranceCheck, Variance, CI_SEED, DEFAULT_CI_RESAMPLES, DEFAULT_JITTER_CEILING_PER_MIN,
};
pub use rng::{sample_exponential, sample_poisson, XorShift64};
pub use schedule::HashrateSchedule;
pub use trial::{
    run_trial, run_trial_observed, DecisionRecord, Observable, TickRecord, Trial, TrialConfig,
};
