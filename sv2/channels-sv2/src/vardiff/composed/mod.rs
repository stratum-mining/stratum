//! The four-axis vardiff decomposition.
//!
//! Four orthogonal extension traits plus a composing adapter that let
//! any (Estimator, Statistic, Boundary, UpdateRule) tuple be
//! automatically a valid [`Vardiff`] implementation. The classic
//! algorithm is one specific composition (see [`classic_composed`])
//! and is asserted fire-for-fire equivalent to
//! [`crate::vardiff::classic::VardiffState`] by the `vardiff_sim`
//! crate's equivalence-test suite.
//!
//! The recommended production composition is constructed via
//! [`crate::vardiff::classic::VardiffState::production_default`] (or
//! the equivalent factory in `vardiff_sim::AlgorithmSpec::full_remedy`):
//!
//! ```rust,ignore
//! use channels_sv2::vardiff::{
//!     classic::VardiffState,
//!     composed::{Composed, EwmaEstimator, AbsoluteRatio,
//!                PoissonCI, PartialRetarget},
//!     SystemClock,
//! };
//! use std::sync::Arc;
//!
//! let v = Composed::new(
//!     EwmaEstimator::new(120),
//!     AbsoluteRatio,
//!     PoissonCI::default_parametric(),
//!     PartialRetarget::new(0.3),
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
pub mod statistic;
pub mod update;

pub use boundary::{Boundary, PoissonCI, StepFunction};
pub use composed::{classic_composed, ClassicComposed, Composed};
pub use decision::DecisionRecord;
pub use estimator::{
    CumulativeCounter, Estimator, EstimatorContext, EstimatorSnapshot, EwmaEstimator,
    SlidingWindowEstimator,
};
pub use statistic::{AbsoluteRatio, Statistic};
pub use update::{FullRetargetNoClamp, FullRetargetWithClamp, PartialRetarget, UpdateRule};
