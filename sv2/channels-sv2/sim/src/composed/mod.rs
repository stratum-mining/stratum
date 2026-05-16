//! The four-axis decomposition — sim-side extension traits.
//!
//! Four orthogonal extension traits plus a composing adapter that
//! together let the framework characterize vardiff algorithms
//! axis-by-axis without changing the production
//! `channels_sv2::vardiff::Vardiff` trait. The architectural rationale
//! is in `sim/docs/DESIGN.md`.
//!
//! ## The four axes
//!
//! - [`Estimator`]: how observations accumulate between ticks; how `H̃`
//!   (the algorithm's belief about miner hashrate) is computed.
//!   *Theory: statistical estimation. Pressure: responsiveness vs
//!   stability.*
//! - [`Statistic`]: how the scalar δ ("how surprised should I be?") is
//!   computed from an [`EstimatorSnapshot`]. *Theory: hypothesis testing.
//!   Pressure: power vs simplicity.*
//! - [`Boundary`]: how the decision threshold θ is computed. *Theory:
//!   decision theory. Pressure: false-fire rate vs missed-detection
//!   rate; rate-awareness lives here.*
//! - [`UpdateRule`]: how the new target is computed on fire. *Theory:
//!   control theory. Pressure: convergence speed vs steady-state
//!   stability.*
//!
//! ## The composing adapter
//!
//! [`Composed<E, S, B, U>`] bundles four axis impls into a single
//! `impl channels_sv2::vardiff::Vardiff`, so any composition is a
//! drop-in `Vardiff` for the existing trial driver and production
//! callers. The classic algorithm composed as
//! `Composed<CumulativeCounter, AbsoluteRatio, StepFunction(classic_table),
//! FullRetargetWithClamp(classic)>` is fire-for-fire equivalent to
//! `channels_sv2::VardiffState` — asserted by tests in
//! `composed::equivalence_tests`.
//!
//! Construct the classic composition via [`classic_composed`].

pub mod boundary;
pub mod composed;
pub mod estimator;
pub mod statistic;
pub mod update;

pub use boundary::{Boundary, PoissonCI, StepFunction};
pub use composed::{classic_composed, ClassicComposed, Composed};
pub use estimator::{
    CumulativeCounter, Estimator, EstimatorContext, EstimatorSnapshot, EwmaEstimator,
    SlidingWindowEstimator,
};
pub use statistic::{AbsoluteRatio, Statistic};
pub use update::{FullRetargetNoClamp, FullRetargetWithClamp, PartialRetarget, UpdateRule};
