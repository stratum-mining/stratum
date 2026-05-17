//! The `Composed` adapter — bundles the four axis traits into a single
//! `Vardiff` implementation.
//!
//! `Composed<E, S, B, U>` carries a blanket `impl Vardiff` so any
//! composition of (Estimator, Statistic, Boundary, UpdateRule) is
//! automatically a valid production `Vardiff`. The `vardiff_sim` crate
//! adds an `impl Observable for Composed` extension to expose the
//! per-tick decision state for characterization runs.

use std::sync::Arc;

use bitcoin::Target;

use crate::vardiff::{error::VardiffError, Clock, Vardiff};

use super::boundary::{Boundary, StepFunction};
use super::decision::DecisionRecord;
use super::estimator::{CumulativeCounter, Estimator, EstimatorContext};
use super::statistic::{AbsoluteRatio, Statistic};
use super::update::{FullRetargetWithClamp, UpdateRule};

/// A vardiff algorithm composed of four orthogonal axes.
///
/// `Composed<E, S, B, U>` is a drop-in replacement for `VardiffState`
/// for any production code path that holds a `Box<dyn Vardiff>` or
/// accepts an `impl Vardiff`. The four type parameters correspond to
/// the four axes:
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
    /// The simulation harness reads this via its `Observable` extension
    /// trait; production callers can ignore it.
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

        // Record the decision for the sim crate's Observable extension.
        // We capture it regardless of fire/hold so post-step holds (the
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
/// by `vardiff_sim`'s equivalence-test suite.
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
