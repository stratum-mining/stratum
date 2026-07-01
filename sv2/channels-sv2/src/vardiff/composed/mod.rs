//! The three-stage vardiff pipeline.
//!
//! Three sequential stages plus a composing adapter that let any
//! (Estimator, Boundary, UpdateRule) triple be automatically a valid
//! [`Vardiff`] implementation. The classic algorithm is one specific
//! composition (see [`classic_composed`]) and is asserted fire-for-fire
//! equivalent to [`crate::vardiff::classic::VardiffState`] by the
//! `vardiff_sim` crate's equivalence-test suite.
//!
//! The recommended production composition (FullRemedy):
//!
//! ```rust,ignore
//! use channels_sv2::vardiff::composed::{
//!     Composed, EwmaEstimator, PoissonCI, PartialRetarget,
//! };
//! use channels_sv2::vardiff::SystemClock;
//! use std::sync::Arc;
//!
//! let v = Composed::new(
//!     EwmaEstimator::new(120),
//!     PoissonCI::default_parametric(),
//!     PartialRetarget::new(0.2),
//!     /* min_hashrate */ 1.0,
//!     Arc::new(SystemClock),
//! );
//! ```
//!
//! See `sim/docs/DESIGN.md` and `sim/docs/FINDINGS.md` for the
//! architectural rationale and the empirical case for `FullRemedy`.
//!
//! [`Vardiff`]: crate::vardiff::Vardiff

pub mod boundary;
pub mod composed;
pub mod decision;
pub mod estimator;
pub mod update;

pub use boundary::{
    AdaptiveBoundary, AdaptiveCusumBoundary, AdaptivePoissonCusum, AdaptiveSignPersist,
    AsymmetricCusumBoundary, AsymmetricPoissonCI, Boundary, CredibleIntervalBoundary,
    CusumBoundary, HysteresisGate, PoissonCI, SignPersistenceCusumBoundary, StepFunction,
    VolatilityAdaptiveBoundary, WarmupBoundary,
};
pub use composed::{
    champion_composed, champion_composed_seeded, classic_composed, ChampionComposed,
    ClassicComposed, Composed,
};
pub use decision::DecisionRecord;
pub use estimator::Uncertainty;
pub use estimator::{
    BayesianEstimator, CkpoolEstimator, CumulativeCounter, DebiasEstimator, Estimator,
    EstimatorContext, EstimatorSnapshot, EwmaEstimator, KalmanEstimator, ShareIndexedEstimator,
    SlidingWindowEstimator, SpmRatioEstimator, TimeBiasEwmaEstimator,
};
pub use update::{
    AcceleratingPartialRetarget, AdaptivePartialRetarget, CkpoolRetarget, FullRetargetNoClamp,
    FullRetargetWithClamp, GuardedAccelRetarget, PartialRetarget, UpdateRule,
};
