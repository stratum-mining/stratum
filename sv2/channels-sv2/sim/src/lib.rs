//! # Vardiff simulation framework
//!
//! Deterministic in-process simulation harness for characterizing the behavioral
//! attributes of any [`channels_sv2::vardiff::Vardiff`] implementation.
//!
//! The framework's purpose is to surface the operationally-important properties
//! of the vardiff algorithm in concrete, measurable terms so that any future
//! algorithmic improvement can be evaluated against a checked-in baseline. See
//! `VARDIFF_SIMULATION_FRAMEWORK.md` alongside this crate's `Cargo.toml` for
//! the full design.
//!
//! ## What this crate provides
//!
//! - [`run_trial`]: drives a single [`channels_sv2::vardiff::Vardiff`]
//!   implementation through `duration_secs` of simulated time against a
//!   Poisson share stream and a programmable hashrate schedule, recording
//!   every retarget event.
//! - [`HashrateSchedule`]: step-function description of the miner's true
//!   hashrate over time — supports stable, step-change, and arbitrary
//!   piecewise-constant scenarios.
//! - [`XorShift64`]: deterministic RNG used for share-arrival sampling.
//!   Trials are fully reproducible from a `(config, schedule, seed)` triple.
//! - [`metrics`]: distribution-computing functions over `Vec<Trial>` —
//!   convergence time, settled accuracy, steady-state jitter, reaction time,
//!   reaction sensitivity. Each metric returns a [`Distribution`] supporting
//!   percentile queries (p10–p99), mean, and count.
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
//! println!("Algorithm fired {} times", trial.fires.len());
//! ```

pub mod baseline;
pub mod metrics;
pub mod regression;
pub mod rng;
pub mod schedule;
pub mod trial;

pub use metrics::{
    convergence_time_distribution, convergence_time_for_trial, jitter_distribution,
    jitter_for_trial, reaction_sensitivity, reaction_time_distribution, reaction_time_for_trial,
    settled_accuracy_distribution, settled_accuracy_for_trial, Distribution,
};
pub use rng::{sample_exponential, sample_poisson, XorShift64};
pub use schedule::HashrateSchedule;
pub use trial::{run_trial, FireEvent, Trial, TrialConfig};
