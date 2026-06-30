//! The `Composed` adapter — bundles the three pipeline stages into a
//! single `Vardiff` implementation.
//!
//! `Composed<E, B, U>` carries a blanket `impl Vardiff` so any
//! composition of (Estimator, Boundary, UpdateRule) is automatically a
//! valid production `Vardiff`. The `vardiff_sim` crate adds an
//! `impl Observable for Composed` extension to expose the per-tick
//! decision state for characterization runs.

use std::sync::Arc;

use bitcoin::Target;

use crate::vardiff::{error::VardiffError, Clock, Vardiff};

use super::boundary::{AdaptiveSignPersist, Boundary, SignPersistenceCusumBoundary, StepFunction};
use super::decision::DecisionRecord;
use super::estimator::{CumulativeCounter, Estimator, EstimatorContext, EwmaEstimator};
use super::update::{AcceleratingPartialRetarget, FullRetargetWithClamp, UpdateRule};

/// A vardiff algorithm composed of three sequential pipeline stages.
///
/// `Composed<E, B, U>` is a drop-in replacement for `VardiffState`
/// for any production code path that holds a `Box<dyn Vardiff>` or
/// accepts an `impl Vardiff`. The three type parameters correspond to:
///
/// - `E`: Estimator — how observations accumulate; produces belief about hashrate.
/// - `B`: Boundary — decides whether the deviation from target is signal or noise.
/// - `U`: UpdateRule — computes the new target when the algorithm fires.
///
/// The deviation statistic (|h_estimate / current_h - 1| × 100) is
/// computed inline by the adapter — it's a fixed normalization step,
/// not a configurable axis.
#[derive(Debug)]
pub struct Composed<E: Estimator, B: Boundary, U: UpdateRule> {
    pub estimator: E,
    pub boundary: B,
    pub update: U,
    pub timestamp_of_last_update: u64,
    pub min_allowed_hashrate: f32,
    pub clock: Arc<dyn Clock>,
    pub last_decision: Option<DecisionRecord>,
}

impl<E, B, U> Composed<E, B, U>
where
    E: Estimator,
    B: Boundary,
    U: UpdateRule,
{
    pub fn new(
        estimator: E,
        boundary: B,
        update: U,
        min_allowed_hashrate: f32,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let timestamp_of_last_update = clock.now_secs();
        Self {
            estimator,
            boundary,
            update,
            timestamp_of_last_update,
            min_allowed_hashrate,
            clock,
            last_decision: None,
        }
    }
}

impl<E, B, U> Vardiff for Composed<E, B, U>
where
    E: Estimator,
    B: Boundary,
    U: UpdateRule,
{
    fn last_update_timestamp(&self) -> u64 {
        self.timestamp_of_last_update
    }
    fn shares_since_last_update(&self) -> u32 {
        self.estimator.shares_count()
    }
    fn set_timestamp_of_last_update(&mut self, ts: u64) {
        self.timestamp_of_last_update = ts;
    }
    fn increment_shares_since_last_update(&mut self) {
        self.estimator.observe(1);
    }
    fn add_shares(&mut self, n: u32) {
        self.estimator.observe(n);
    }
    fn min_allowed_hashrate(&self) -> f32 {
        self.min_allowed_hashrate
    }

    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        self.timestamp_of_last_update = self.clock.now_secs();
        self.estimator.on_fire(0.0, 0.0);
        Ok(())
    }

    fn try_vardiff(
        &mut self,
        hashrate: f32,
        target: &Target,
        shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError> {
        self.last_decision = None;

        let now = self.clock.now_secs();
        let dt = now.saturating_sub(self.timestamp_of_last_update);

        if dt <= 15 {
            return Ok(None);
        }

        // Stage 1: Estimator produces its belief.
        let ctx = EstimatorContext {
            current_hashrate: hashrate,
            current_target: target,
            shares_per_minute,
        };
        let snap = self.estimator.snapshot(dt, &ctx);

        // Deviation: fixed normalization (|ratio - 1| × 100).
        let delta = if hashrate > 0.0 {
            ((snap.h_estimate as f64 / hashrate as f64) - 1.0).abs() * 100.0
        } else {
            0.0
        };

        // Stage 2: Boundary decides threshold.
        let threshold = self.boundary.threshold(dt, shares_per_minute, &snap);

        self.last_decision = Some(DecisionRecord {
            delta,
            threshold,
            h_estimate: snap.h_estimate,
            uncertainty: snap.uncertainty,
        });

        // Per-decision instrumentation. Paper coordinate (§1): `e = ln(Ĥ/H)`,
        // where Ĥ is the controller's CURRENT set-point belief and H is the
        // true delivered hashrate. In this pipeline:
        //   Ĥ = `hashrate` (the nominal/set-point the controller currently holds,
        //       fed back from the previous fire via update_channel), and
        //   H  = `snap.h_estimate` (fresh, share-derived from the observed stream
        //       at the current target — the best available proxy for delivered H).
        // So paper-e = ln(hashrate / h_estimate). (An earlier version logged the
        // reciprocal and mislabeled the operands, which inverted the sign.)
        //
        // CHECKABLE INVARIANT (verify against the raw observable, never the
        // algebra): e < 0  ⟺  under-difficulty  ⟺  realized_spm > r*. The
        // realized rate is logged alongside so the sign can be confirmed against
        // a quantity that does not pass through hash_rate_from_target.
        let e = if hashrate > 0.0 && snap.h_estimate > 0.0 {
            (hashrate as f64 / snap.h_estimate as f64).ln()
        } else {
            f64::NAN
        };
        tracing::debug!(
            target: "channels_sv2::vardiff",
            e,
            realized_spm = snap.realized_share_per_min,
            r_star = shares_per_minute,
            // RAW per-window share count (un-smoothed). Var(raw_count) is the
            // quantity Theorem 2 bounds (1/(r*τ)); the lever/band-scaling claim
            // must be tested against the SD of THIS across a flat-belief window,
            // NOT against the SD of `e` (which is smoothed by the EWMA and so is
            // silent on the floor's rate-scaling). Expect SD(raw_count) to scale
            // ≈ √(r*τ) with rate at fixed window.
            raw_count = snap.n_shares,
            dt_secs = snap.dt_secs,
            delta, threshold,
            h_belief = hashrate,
            h_delivered_est = snap.h_estimate,
            will_fire = delta >= threshold,
            "vardiff decision (e=ln(Ĥ/H)<0 ⟺ under-difficulty ⟺ realized_spm>r_star)"
        );

        if delta < threshold {
            return Ok(None);
        }

        // Stage 3: UpdateRule computes the new target.
        let mut new_hashrate =
            self.update
                .next_hashrate(&snap, hashrate, delta, threshold, shares_per_minute);

        if new_hashrate < self.min_allowed_hashrate {
            new_hashrate = self.min_allowed_hashrate;
        }

        // Notify estimator of the fire.
        self.timestamp_of_last_update = now;
        self.estimator.on_fire(new_hashrate, hashrate);

        // `s = ln(Ĥ⁺/Ĥ⁻)` is the log-step (§1): the change in the controller's
        // SET-POINT belief across the fire — new set-point over OLD set-point,
        // i.e. ln(new_hashrate / hashrate). (`s>0` tightens — raises difficulty;
        // `s<0` eases.) NOTE the operand: the prior belief is `hashrate` (the
        // current set-point), NOT `snap.h_estimate` (the share-derived delivered
        // estimate); an earlier version used h_estimate, producing a mixed ratio
        // whose sign was not a clean tighten/ease signal.
        let s = if new_hashrate > 0.0 && hashrate > 0.0 {
            (new_hashrate as f64 / hashrate as f64).ln()
        } else {
            f64::NAN
        };
        tracing::debug!(
            target: "channels_sv2::vardiff",
            s, new_hashrate, h_belief_prev = hashrate,
            "vardiff fire (s=ln(Ĥ⁺/Ĥ⁻); s>0 tighten, s<0 ease)"
        );

        Ok(Some(new_hashrate))
    }
}

// ============================================================================
// ClassicComposed — the three-stage representation of VardiffState
// ============================================================================

/// The Classic algorithm expressed as a `Composed` triple. Asserted
/// fire-for-fire equivalent to `VardiffState` by the sim crate's
/// equivalence test suite.
pub type ClassicComposed = Composed<CumulativeCounter, StepFunction, FullRetargetWithClamp>;

/// Constructs the Classic algorithm as a `Composed` triple.
pub fn classic_composed(min_hashrate: f32, clock: Arc<dyn Clock>) -> ClassicComposed {
    Composed::new(
        CumulativeCounter::new(),
        StepFunction::classic_table(),
        FullRetargetWithClamp::classic(),
        min_hashrate,
        clock,
    )
}

// ============================================================================
// ChampionComposed — the production champion selected by the simulation study
// ============================================================================

/// The champion algorithm as a `Composed` triple.
///
/// `EwmaEstimator(τ=360s) + AdaptiveSignPersist(SignPersistenceCusumBoundary,
/// spm_threshold=6) + AcceleratingPartialRetarget(base=0.2, max=0.6,
/// acc=0.05)`. Selected by minimax over the target share rate with a
/// decline-safety constraint (see `sim/docs/METRIC_DERIVATION.md`): the
/// gentlest configuration that stays decline-safe across the rate band.
pub type ChampionComposed =
    Composed<EwmaEstimator, AdaptiveSignPersist, AcceleratingPartialRetarget>;

/// Constructs the champion algorithm as a `Composed` triple. The parameters
/// are the exact values the simulation framework's `slow-decline`,
/// `sweep-minimax`, and `steady-transient` binaries identify and clear:
/// `Ewma360 / SignPersist(s1.5, floor0.05, t8, d0.06, dm0.6) over spm6 /
/// Accel(0.2, 0.6, 0.05)`.
pub fn champion_composed(min_hashrate: f32, clock: Arc<dyn Clock>) -> ChampionComposed {
    Composed::new(
        EwmaEstimator::new(360),
        AdaptiveSignPersist::sign_persist(
            SignPersistenceCusumBoundary::new(1.5, 0.05, 8.0, 0.06, 0.6),
            6,
        ),
        AcceleratingPartialRetarget::new(0.2, 0.6, 0.05),
        min_hashrate,
        clock,
    )
}

/// The champion, but with its estimator SEEDED from the channel's declared
/// `nominal_hash_rate` at open — collapsing the §8.5 cold-start ramp.
///
/// At channel open the difficulty is set from the declared nominal
/// (`D = nominal/r*`), so an accurate declaration produces ≈ `r*` shares/min at
/// that difficulty; seeding the EWMA to that rate makes the controller believe
/// ≈ the declared nominal from cycle one instead of climbing from the floor.
///
/// `shares_per_minute` is the target `r*` the difficulty was set against;
/// `prior_ticks` is the seed's weight (small ⇒ shares overwrite it within ~τ;
/// see [`EwmaEstimator::new_seeded`] for the safety rationale — the seed is a
/// clamped, overwritable prior, not blind trust). The estimator is the ONLY
/// stage that changes; boundary and update rule are identical to the champion,
/// so a seeded channel and an unseeded one converge to the same steady state.
///
/// This needs NO channel/protocol API change: the caller (the pool's
/// open-channel handler) already has `nominal_hash_rate` and `shares_per_minute`
/// in scope when it builds the vardiff.
pub fn champion_composed_seeded(
    min_hashrate: f32,
    shares_per_minute: f64,
    prior_ticks: u32,
    clock: Arc<dyn Clock>,
) -> ChampionComposed {
    Composed::new(
        EwmaEstimator::new_seeded(360, shares_per_minute, prior_ticks),
        AdaptiveSignPersist::sign_persist(
            SignPersistenceCusumBoundary::new(1.5, 0.05, 8.0, 0.06, 0.6),
            6,
        ),
        AcceleratingPartialRetarget::new(0.2, 0.6, 0.05),
        min_hashrate,
        clock,
    )
}
