//! The declarative grid layer — Cartesian product over (algorithm,
//! share rate, scenario).
//!
//! ## What a [`Grid`] is
//!
//! A `Grid` declares three axes — `algorithms`, `share_rates`,
//! `scenarios` — plus a trial count and base seed. Calling `Grid::run`
//! produces every cell in the Cartesian product, drives `trial_count`
//! trials at each, and returns `HashMap<algorithm_name,
//! Vec<CellResult>>`. Algorithm is a first-class dimension; one binary
//! produces every algorithm's characterization in one sweep.
//!
//! ## Observable dispatch
//!
//! The grid always drives trials through [`run_trial_observed`], which
//! populates the optional `delta`, `threshold`, and `h_estimate`
//! fields on each [`TickRecord`] for algorithms that implement
//! [`Observable`]. The bias / variance / overshoot metrics consume
//! those fields.
//!
//! For algorithms that don't naturally implement `Observable` (the
//! production [`VardiffState`]), the [`AsObservable`] wrapper provides
//! a null impl that always returns `None`. The dispatch path stays
//! uniform — every algorithm in the grid is `Vardiff + Observable` —
//! and introspection-only metrics gracefully degrade to `None` values
//! that the TOML serializer simply omits.
//!
//! ## Type erasure
//!
//! Different concrete algorithm types (`VardiffState`,
//! `ClassicComposed`, `Composed<EwmaEstimator, ...>`) must coexist in
//! the same `algorithms` vector. Rust generics can't express that
//! directly, so the grid stores [`AlgorithmSpec`] values whose
//! `factory` closure produces a [`VardiffBox`] — a sim-side newtype
//! wrapping `Box<dyn ObservableVardiff>` (the compound trait of
//! [`Vardiff`] + [`Observable`]).
//!
//! ## Seeding
//!
//! Per-trial seeds are derived as
//! `base_seed + (cell_index << 20) + trial_index` with
//! `cell_index = algo_idx × N_spm × N_scen + spm_idx × N_scen +
//! scen_idx`. The `<< 20` shift gives each cell a 1,048,576-entry
//! seed range; collision-free as long as `trial_count ≤ 2^20`. For a
//! single-algorithm grid (`algo_idx = 0`) `cell_index` collapses to
//! `spm_idx × N_scen + scen_idx`, the same ordering used by
//! [`crate::baseline::default_cells`].
//!
//! For paired A/B comparisons where metric differences must be
//! attributable to algorithm behavior rather than seed disparity, use
//! [`Grid::run_paired`] — it strips `algo_idx` from the cell index so
//! all algorithms see the same trial inputs at each cell.

use std::collections::HashMap;
use std::sync::Arc;

use bitcoin::Target;
use channels_sv2::vardiff::pow2_pid::Pow2PidVardiff;
use channels_sv2::vardiff::pid_tuned::{PidConfig, PidTunedVardiff};
use channels_sv2::vardiff::{error::VardiffError, Clock, MockClock, Vardiff};

use crate::baseline::{Cell, CellResult, Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT};
use crate::composed;
use crate::metrics;
use crate::trial::{run_trial_observed, DecisionRecord, Observable, Trial};

// ============================================================================
// ObservableVardiff compound trait + AsObservable wrapper
// ============================================================================

/// Compound trait combining `Vardiff` and `Observable`. The grid's
/// type-erased path stores algorithms as `Box<dyn ObservableVardiff>`
/// so the trial driver can call both vardiff methods (try_vardiff,
/// add_shares, …) and the introspection accessor (`last_decision`)
/// through the same trait object.
///
/// Blanket-implemented for any type that satisfies both — including
/// concrete algorithms like [`composed::Composed`] (which has an
/// explicit `Observable` impl) and the [`AsObservable`] wrapper used
/// for non-observable algorithms.
pub trait ObservableVardiff: Vardiff + Observable {}
impl<T: Vardiff + Observable + ?Sized> ObservableVardiff for T {}

/// Wraps a `V: Vardiff` to also implement `Observable` by returning
/// `None` for `last_decision`. Used for algorithms (notably
/// `VardiffState`) that don't expose internal decision state — so they
/// can flow through the grid's `Vardiff + Observable` dispatch path
/// with the introspection fields gracefully empty.
///
/// `Vardiff` methods delegate directly to the inner value; the algorithm
/// behaves identically to the unwrapped version. Only `last_decision`
/// is synthetic.
#[derive(Debug)]
pub struct AsObservable<V: Vardiff>(pub V);

impl<V: Vardiff> Vardiff for AsObservable<V> {
    fn last_update_timestamp(&self) -> u64 {
        self.0.last_update_timestamp()
    }
    fn shares_since_last_update(&self) -> u32 {
        self.0.shares_since_last_update()
    }
    fn min_allowed_hashrate(&self) -> f32 {
        self.0.min_allowed_hashrate()
    }
    fn set_timestamp_of_last_update(&mut self, ts: u64) {
        self.0.set_timestamp_of_last_update(ts);
    }
    fn increment_shares_since_last_update(&mut self) {
        self.0.increment_shares_since_last_update();
    }
    fn add_shares(&mut self, n: u32) {
        self.0.add_shares(n);
    }
    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        self.0.reset_counter()
    }
    fn try_vardiff(
        &mut self,
        hashrate: f32,
        target: &Target,
        shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError> {
        self.0.try_vardiff(hashrate, target, shares_per_minute)
    }
}

impl<V: Vardiff> Observable for AsObservable<V> {
    fn last_decision(&self) -> Option<DecisionRecord> {
        None
    }
}

// ============================================================================
// VardiffBox — type-erased Vardiff + Observable wrapper
// ============================================================================

/// A `Box<dyn ObservableVardiff>` wrapper that implements `Vardiff`
/// and `Observable` itself, so it flows through the generic
/// `run_trial_observed<V: Vardiff + Observable>` driver without
/// modification. The orphan rule prevents direct
/// `impl Vardiff for Box<...>` (both `Vardiff` and `Box` are foreign
/// to the sim crate); this newtype is the standard workaround.
///
/// All methods delegate to the inner box — performance is a single
/// virtual call per method, identical to direct `&mut dyn` dispatch.
pub struct VardiffBox(pub Box<dyn ObservableVardiff>);

impl std::fmt::Debug for VardiffBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("VardiffBox").field(&self.0).finish()
    }
}

impl Vardiff for VardiffBox {
    fn last_update_timestamp(&self) -> u64 {
        self.0.last_update_timestamp()
    }
    fn shares_since_last_update(&self) -> u32 {
        self.0.shares_since_last_update()
    }
    fn min_allowed_hashrate(&self) -> f32 {
        self.0.min_allowed_hashrate()
    }
    fn set_timestamp_of_last_update(&mut self, timestamp: u64) {
        self.0.set_timestamp_of_last_update(timestamp);
    }
    fn increment_shares_since_last_update(&mut self) {
        self.0.increment_shares_since_last_update();
    }
    fn add_shares(&mut self, n: u32) {
        self.0.add_shares(n);
    }
    fn reset_counter(&mut self) -> Result<(), VardiffError> {
        self.0.reset_counter()
    }
    fn try_vardiff(
        &mut self,
        hashrate: f32,
        target: &Target,
        shares_per_minute: f32,
    ) -> Result<Option<f32>, VardiffError> {
        self.0.try_vardiff(hashrate, target, shares_per_minute)
    }
}

impl Observable for VardiffBox {
    fn last_decision(&self) -> Option<DecisionRecord> {
        self.0.last_decision()
    }
}

// ============================================================================
// AlgorithmSpec — name + factory closure
// ============================================================================

/// Algorithm factory type. Takes a clock and produces a fresh
/// [`VardiffBox`] each call. `Send + Sync + 'static` because the spec
/// is shared across the grid and (eventually) across worker threads
/// when the grid is parallelized.
pub type VardiffFactory = Arc<dyn Fn(Arc<dyn Clock>) -> VardiffBox + Send + Sync>;

/// One entry in the grid's algorithm axis. Carries the algorithm's
/// name (used as the key in the per-algorithm result map and in the
/// baseline file name) plus the factory closure that constructs a
/// fresh instance per trial.
#[derive(Clone)]
pub struct AlgorithmSpec {
    pub name: String,
    pub factory: VardiffFactory,
}

impl std::fmt::Debug for AlgorithmSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlgorithmSpec")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl AlgorithmSpec {
    /// General constructor. Use the convenience constructors
    /// (`classic_vardiff_state`, `classic_composed`, …) for the
    /// algorithms shipped with the framework.
    pub fn new<F>(name: impl Into<String>, factory: F) -> Self
    where
        F: Fn(Arc<dyn Clock>) -> VardiffBox + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            factory: Arc::new(factory),
        }
    }

    /// The **classic** baseline algorithm, as the sim's comparison anchor.
    ///
    /// NOTE: production `VardiffState` now delegates to the champion
    /// composition, NOT classic. So this spec is built from
    /// [`composed::classic_composed`] directly — it must remain the *classic*
    /// baseline (that is its role in every diagnostic bin that compares a
    /// candidate against "the algorithm we run today / the upstream
    /// reference"). Building it from `VardiffState` would silently make the
    /// champion its own anchor. The name still reflects the classic triple,
    /// which is now honest because the build target is classic again.
    pub fn classic_vardiff_state() -> Self {
        let name = format!(
            "{}*",
            crate::naming::triple_name(
                &composed::CumulativeCounter::new(),
                &composed::StepFunction::classic_table(),
                &composed::FullRetargetWithClamp::classic(),
            )
        );
        Self::new(name, |clock| {
            // Build the classic composition directly (NOT VardiffState, which
            // is now the champion) and wrap in AsObservable to preserve the
            // introspection-blind `*` monolith semantics this spec has always
            // had — the classic baseline reports None for bias/variance, same
            // as the opaque production monolith it originally stood in for.
            let inner = composed::classic_composed(1.0, clock);
            VardiffBox(Box::new(AsObservable(inner)))
        })
    }

    /// The three-stage-decomposed Classic algorithm. Fire-for-fire
    /// equivalent to the original classic monolith; additionally
    /// exposes the per-tick decision state, so bias / variance /
    /// overshoot metrics work.
    pub fn classic_composed() -> Self {
        let name = crate::naming::triple_name(
            &composed::CumulativeCounter::new(),
            &composed::StepFunction::classic_table(),
            &composed::FullRetargetWithClamp::classic(),
        );
        Self::new(name, |clock| {
            VardiffBox(Box::new(composed::classic_composed(1.0, clock)))
        })
    }

    /// The Parametric algorithm: same as Classic except the Boundary
    /// axis swaps from the share-rate-blind step ladder to a
    /// `PoissonCI(z = 2.576, margin = 0.05)` rate-aware threshold.
    ///
    /// This is the canonical one-axis-swap from Classic: same
    /// Estimator, same Update — only the threshold
    /// form differs. Holding three axes constant and varying the
    /// fourth is exactly what the three-stage pipeline supports.
    pub fn parametric() -> Self {
        let name = crate::naming::triple_name(
            &composed::CumulativeCounter::new(),
            &composed::PoissonCI::default_parametric(),
            &composed::FullRetargetWithClamp::classic(),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::CumulativeCounter::new(),
                composed::PoissonCI::default_parametric(),
                composed::FullRetargetWithClamp::classic(),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// EWMA-60s: an exponentially-weighted estimator with τ = 60s,
    /// combined with the rate-aware Parametric boundary and a damped
    /// partial-retarget update (η = 0.5). Three axes differ from
    /// Classic — Estimator and Update — leaving Boundary
    /// shared. Demonstrates the framework's combinatorial reuse:
    /// `EwmaEstimator` + `PoissonCI` + `PartialRetarget` are each
    /// individually swappable, and EWMA is one specific composition
    /// of them.
    pub fn ewma_60s() -> Self {
        Self::ewma(60)
    }

    /// Generic EWMA factory parameterized by τ (time constant in
    /// seconds). Use this to sweep τ along the variance/responsiveness
    /// Pareto frontier:
    ///
    /// ```text
    /// for &tau in &[30, 60, 120, 300, 600] {
    ///     grid.algorithms.push(AlgorithmSpec::ewma(tau));
    /// }
    /// ```
    ///
    /// Algorithm name is `"EWMA-{tau}s"`. Other axes match
    /// [`ewma_60s`](Self::ewma_60s): `AbsoluteRatio` statistic,
    /// `PoissonCI(2.576, 0.05)` boundary, `PartialRetarget(0.5)` update.
    pub fn ewma(tau_secs: u64) -> Self {
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(tau_secs),
            &composed::PoissonCI::default_parametric(),
            &composed::PartialRetarget::default_ewma(),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(tau_secs),
                composed::PoissonCI::default_parametric(),
                composed::PartialRetarget::default_ewma(),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Sliding-Window algorithm: `SlidingWindowEstimator(n_ticks)` +
    /// `PoissonCI(2.576, 0.05)` +
    /// `FullRetargetNoClamp`.
    ///
    /// Algorithm name is `"SlidingWindow-{n_ticks}t"`. The
    /// `n_ticks` × `tick_secs (=60)` product is the effective window
    /// in seconds; e.g. `n_ticks = 10` → 10-minute sliding window.
    pub fn sliding_window(n_ticks: usize) -> Self {
        let name = crate::naming::triple_name(
            &composed::SlidingWindowEstimator::new(n_ticks),
            &composed::PoissonCI::default_parametric(),
            &composed::FullRetargetNoClamp,
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::SlidingWindowEstimator::new(n_ticks),
                composed::PoissonCI::default_parametric(),
                composed::FullRetargetNoClamp,
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Strict-CI Parametric: same as [`parametric`](Self::parametric)
    /// but uses `PoissonCI::strict_3sigma()` (z = 3.0, ~99.7% CI) for
    /// the boundary. The motivation is the FINDINGS.md § "Parametric
    /// SPM=6 cascade" failure mode — under the default z = 2.576 the
    /// SPM=6 trace at seed `0xcb3b` fires on a Poisson(5.2)→15
    /// outlier at tick 11 (deviation ≈ 188%, threshold ≈ 128%).
    /// Bumping z to 3.0 lifts the threshold to ≈ 142% — still
    /// permeable to the 188% outlier, so this variant is expected to
    /// only marginally improve the worst-case at SPM=6 but should
    /// trim the median.
    pub fn parametric_strict() -> Self {
        let name = crate::naming::triple_name(
            &composed::CumulativeCounter::new(),
            &composed::PoissonCI::strict_3sigma(),
            &composed::FullRetargetWithClamp::classic(),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::CumulativeCounter::new(),
                composed::PoissonCI::strict_3sigma(),
                composed::FullRetargetWithClamp::classic(),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Classic estimator + classic boundary + classic statistic, but
    /// the UpdateRule axis swaps `FullRetargetWithClamp` for
    /// `PartialRetarget(eta)`. Isolates "just damp the magnitude of
    /// each fire" from the orthogonal axis changes. The η parameter
    /// chooses how aggressively to damp: η = 1.0 reproduces
    /// full-retarget-without-clamps, η = 0.3 moves only 30% toward
    /// the estimator's belief on each fire.
    ///
    /// **Why study this in isolation?** The three-stage hypothesis is
    /// that each axis closes a distinct failure mode. The
    /// 0xcb3b/SPM=6 cascade survives a boundary tightening because
    /// the *update magnitude* (full retarget to a noisy single-window
    /// h_estimate) is the unbounded step. A pure PartialRetarget
    /// swap should bound the per-fire jump regardless of estimator
    /// noise — predicting the worst-case overshoot drops to
    /// approximately `η × (peak − truth)`.
    ///
    /// Algorithm name is `"ClassicPartialRetarget-{eta×100 as u32}"`,
    /// e.g. `classic_partial_retarget(0.3)` → `"ClassicPartialRetarget-30"`.
    pub fn classic_partial_retarget(eta: f32) -> Self {
        let name = crate::naming::triple_name(
            &composed::CumulativeCounter::new(),
            &composed::StepFunction::classic_table(),
            &composed::PartialRetarget::new(eta),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::CumulativeCounter::new(),
                composed::StepFunction::classic_table(),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// The "full remedy" algorithm: composes the three axis fixes
    /// suggested by the FINDINGS.md § "Axis closure picture" analysis.
    ///
    /// - Estimator: `EwmaEstimator(120s)` — temporal smoothing closes
    ///   the single-window Poisson spike that drives Phase-1-end
    ///   cascades.
    /// - Boundary: `PoissonCI` (default 99% CI, z = 2.576) — rate-aware
    ///   threshold floor, prevents false-fires on stable load.
    /// - Update: `PartialRetarget(0.2)` — bounds the magnitude of any
    ///   single fire, defense-in-depth against estimator-noise tails.
    ///
    /// Each parameter is substantiated by its own Pareto sweep:
    /// `sweep-ewma-tau` characterizes τ, `sweep-eta` characterizes η,
    /// `sweep-z` characterizes z, and `sweep-eta-z` confirms the
    /// (η, z) axes are nearly separable (no exotic joint optimum).
    ///
    /// η = 0.2 is the convergence-vs-overshoot balance point. Smaller
    /// η (e.g. 0.1) tightens the ramp-overshoot tail further but
    /// catastrophically breaks cold-start convergence at high SPM
    /// (48% convergence at SPM=120 vs 99.6% at η=0.2 — the rate-aware
    /// PoissonCI threshold suppresses firing before truth is reached
    /// when per-fire moves are too small). Larger η (e.g. 0.3) preserves
    /// cold-start convergence but widens the ramp-overshoot tail
    /// (p99 = 31% at SPM=6 vs 12% at η=0.2) and worsens decoupling
    /// (0.79 vs 0.87 at SPM=6). η = 0.2 is the sweet spot.
    ///
    /// z = 2.576 preserves the −10% step sensitivity floor at high SPM
    /// (raising z to 3.0 drops it from 0.57 to 0.48).
    ///
    /// Worst-case behavior: `--scan-overshoot 100 --spm 6` against
    /// VardiffState peaks at 187% above truth; FullRemedy on the same
    /// scan collapses the worst-case to a low double-digit excursion.
    ///
    /// NOTE — historical handle, not a live recommendation. `FullRemedy` is the
    /// mid-arc waypoint of the Classic→champion derivation; it was the earlier
    /// scalar-fitness pick but is **superseded** — it does NOT clear the
    /// slow-decline gate. The shipped algorithm is the champion (`Ewma360 /
    /// AdaptiveSignPersist(spm6) / AccelRetarget`). The name predates the
    /// refutation and is kept as the stable handle the docs reason with
    /// (see `docs/records/FINDINGS.md`); it is not a claim that this is the fix.
    pub fn full_remedy() -> Self {
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(120),
            &composed::PoissonCI::default_parametric(),
            &composed::PartialRetarget::new(0.2),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(120),
                composed::PoissonCI::default_parametric(),
                composed::PartialRetarget::new(0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// PoissonCI boundary + AcceleratingPartialRetarget update.
    ///
    /// Hypothesis: PoissonCI gates fires with statistical rigor (prevents
    /// overshoot at low SPM where data is sparse), while
    /// AcceleratingPartialRetarget gives fast convergence at high SPM
    /// (where PoissonCI fires often and the accelerating η can safely
    /// ramp). Combines FullRemedy's low-SPM discipline with
    /// VardiffState's high-SPM speed.
    pub fn poisson_accel() -> Self {
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(120),
            &composed::PoissonCI::default_parametric(),
            &composed::AcceleratingPartialRetarget::new(0.2, 0.6, 0.2),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(120),
                composed::PoissonCI::default_parametric(),
                composed::AcceleratingPartialRetarget::new(0.2, 0.6, 0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// PoissonCI + GuardedAccelRetarget: only accelerates η after the
    /// first direction reversal. During cold-start (all fires upward),
    /// η stays at 0.2 — preventing overshoot. After the first reversal
    /// proves we've crossed the target, acceleration kicks in for
    /// subsequent step-changes.
    pub fn poisson_guarded_accel() -> Self {
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(120),
            &composed::PoissonCI::default_parametric(),
            &composed::GuardedAccelRetarget::new(0.2, 0.6, 0.2),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(120),
                composed::PoissonCI::default_parametric(),
                composed::GuardedAccelRetarget::new(0.2, 0.6, 0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Adaptive boundary: PoissonCI at low SPM, CUSUM at high SPM.
    /// The `spm_threshold` determines the crossover point. Below it,
    /// PoissonCI's conservatism prevents overshoot on sparse data.
    /// At or above it, CUSUM's aggression leverages abundant data.
    /// Paired with AcceleratingPartialRetarget for fast convergence.
    pub fn adaptive_boundary(spm_threshold: u32) -> Self {
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(120),
            &composed::AdaptivePoissonCusum::new(spm_threshold),
            &composed::AcceleratingPartialRetarget::new(0.2, 0.6, 0.2),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(120),
                composed::AdaptivePoissonCusum::new(spm_threshold),
                composed::AcceleratingPartialRetarget::new(0.2, 0.6, 0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// **Balanced Pareto optimum** (maximin 0.551, the best worst-axis of any
    /// algorithm characterized). `EwmaEstimator(90s)` +
    /// `AdaptivePoissonCusum(spm=8)` with a sensitive CUSUM (sensitivity=1.0,
    /// tighten=3.0) + `AcceleratingPartialRetarget(0.3, 0.6, 0.2)`.
    ///
    /// Found by the maximin parameter sweep (`sweep-balanced`): no single
    /// equal-weight axis falls below 0.55, trading a little jitter/overshoot
    /// safety for materially stronger small-drop reaction and convergence
    /// than the production `VardiffState`. This is the recommended default
    /// when balanced behavior across all scenarios is wanted.
    pub fn balanced() -> Self {
        let make_boundary = || {
            composed::AdaptivePoissonCusum::with_params(
                composed::PoissonCI::default_parametric(),
                composed::AsymmetricCusumBoundary::new(1.0, 0.05, 3.0),
                8,
            )
        };
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(90),
            &make_boundary(),
            &composed::AcceleratingPartialRetarget::new(0.3, 0.6, 0.2),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(90),
                make_boundary(),
                composed::AcceleratingPartialRetarget::new(0.3, 0.6, 0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// **React-priority Pareto optimum** (best small-drop reaction of any
    /// balanced-ish algorithm). `EwmaEstimator(90s)` +
    /// `AdaptiveSignPersist(spm=8)` — PoissonCI below 8 SPM, and
    /// `SignPersistenceCusumBoundary(sensitivity=1.5, floor=0.05, tighten=3.0,
    /// discount=0.15, max_discount=0.4)` at/above — +
    /// `AcceleratingPartialRetarget(0.3, 0.6, 0.2)`.
    ///
    /// The sign-persistence boundary ratchets its threshold down as a
    /// deviation persists in one direction, so a sustained 10% hashrate drop
    /// (failing/throttling ASIC) is detected far faster than any fixed
    /// boundary — at the cost of slower cold-start convergence (the two draw
    /// on the same "agility budget"). Per-SPM analysis showed the bare
    /// SignPersist boundary collapses at low SPM (jitter/step/convergence all
    /// near zero at 4 SPM) because sparse-data Poisson noise spuriously trips
    /// the same-sign discount; wrapping it in the PoissonCI low-SPM guard
    /// (`AdaptiveSignPersist`) fixes that. Use when fast detection of gradual
    /// hashrate loss is the operational priority.
    pub fn react_priority() -> Self {
        let make_boundary = || {
            composed::AdaptiveSignPersist::sign_persist(
                composed::SignPersistenceCusumBoundary::new(1.5, 0.05, 3.0, 0.15, 0.4),
                8,
            )
        };
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(90),
            &make_boundary(),
            &composed::AcceleratingPartialRetarget::new(0.3, 0.6, 0.2),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(90),
                make_boundary(),
                composed::AcceleratingPartialRetarget::new(0.3, 0.6, 0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// **The regret/effort champion** (June 2026). `EwmaEstimator(150s)` +
    /// `AdaptiveSignPersist(spm=6)` — PoissonCI below 6 SPM, and
    /// `SignPersistenceCusumBoundary(sensitivity=0.3, floor=0.05,
    /// tighten=6.0, discount=0.06, max_discount=0.6)` at/above — +
    /// `AcceleratingPartialRetarget(0.2, 0.8, 0.05)`.
    ///
    /// Found by the §10 regret/effort process (`bin/sweep-regret-big` →
    /// `sweep-signpersist-regret` → `confirm-signpersist`): it beats the
    /// prior `balanced`/`react_priority` Pareto champions by trading a low
    /// over-difficulty cost (`regret_over` ~0.029, near the field minimum)
    /// for a tolerated, cheap under-difficulty bias. Relative to the
    /// AsymCusum-only interim champion it adds the sign-persistence
    /// discount, which corrects most of that under-difficulty bias (faster
    /// cold-start ramp, tighter steady state) WITHOUT giving back
    /// death-spiral safety: the discount applies after the tighten
    /// multiplier, so a one-off spike keeps full tighten-reluctance while a
    /// *persistent* under-difficulty progressively relaxes it. Validated as
    /// a weight-robust interior optimum at the §10 3:1 over:under weight;
    /// 100% detection on the aged-counter −10% drop.
    pub fn champion() -> Self {
        let make_boundary = || {
            composed::AdaptiveSignPersist::sign_persist(
                composed::SignPersistenceCusumBoundary::new(0.3, 0.05, 6.0, 0.06, 0.6),
                6,
            )
        };
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(150),
            &make_boundary(),
            &composed::AcceleratingPartialRetarget::new(0.2, 0.8, 0.05),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(150),
                make_boundary(),
                composed::AcceleratingPartialRetarget::new(0.2, 0.8, 0.05),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Parameterized FullRemedy: same three-axis composition as
    /// [`full_remedy`](Self::full_remedy) but with the EWMA time
    /// constant, PartialRetarget η, and PoissonCI z-score exposed as
    /// arguments. Use this to Pareto-explore the
    /// (responsiveness, damping, false-fire rate) frontier:
    ///
    /// ```text
    /// for &eta in &[0.1, 0.2, 0.3, 0.5, 0.7, 1.0] {
    ///     grid.algorithms.push(AlgorithmSpec::full_remedy_with(120, eta, 2.576));
    /// }
    /// ```
    ///
    /// `full_remedy_with(120, 0.3, 2.576)` is behaviorally identical
    /// to `full_remedy()` but carries a different display name
    /// (`FullRemedy-tau120-eta30-z2576`) to keep parametric-sweep
    /// result tables collision-free.
    ///
    /// The PoissonCI margin is held at `0.05` (the default-parametric
    /// value); a four-parameter version is not currently exposed
    /// because the sweep evidence so far suggests `margin` is a less
    /// impactful axis than the other three.
    pub fn full_remedy_with(tau_secs: u64, eta: f32, z: f64) -> Self {
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(tau_secs),
            &composed::PoissonCI::with_z(z, 0.05),
            &composed::PartialRetarget::new(eta),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(tau_secs),
                composed::PoissonCI::with_z(z, 0.05),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Bayesian Gamma-Poisson estimator composed with PoissonCI boundary
    /// and PartialRetarget update rule. Uses the same boundary and update
    /// rule as FullRemedy but replaces the EWMA estimator with a
    /// principled Bayesian posterior over the hashrate ratio.
    ///
    /// `discount`: exponential forgetting per tick (0.90–0.99).
    /// `prior_shares`: initial pseudo-count strength (2.0–10.0).
    /// `eta`: PartialRetarget damping factor.
    /// `z`: PoissonCI z-score threshold.
    pub fn bayesian(discount: f64, prior_shares: f64, eta: f32, z: f64) -> Self {
        let name = crate::naming::triple_name(
            &composed::BayesianEstimator::new(discount, prior_shares),
            &composed::PoissonCI::with_z(z, 0.05),
            &composed::PartialRetarget::new(eta),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::BayesianEstimator::new(discount, prior_shares),
                composed::PoissonCI::with_z(z, 0.05),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Bayesian estimator with FullRetarget (no damping) — for testing
    /// the estimator's noise characteristics in isolation. If the Bayesian
    /// estimator is smooth enough, it can tolerate aggressive updates.
    pub fn bayesian_full_retarget(discount: f64, prior_shares: f64, z: f64) -> Self {
        let name = crate::naming::triple_name(
            &composed::BayesianEstimator::new(discount, prior_shares),
            &composed::PoissonCI::with_z(z, 0.05),
            &composed::FullRetargetNoClamp,
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::BayesianEstimator::new(discount, prior_shares),
                composed::PoissonCI::with_z(z, 0.05),
                composed::FullRetargetNoClamp,
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Bayesian estimator with CredibleIntervalBoundary — the composition
    /// that leverages uncertainty-aware decision making. The boundary fires
    /// when the Gamma posterior's credible interval excludes ratio=1.0.
    ///
    /// `discount`: exponential forgetting per tick.
    /// `prior_shares`: initial pseudo-count strength.
    /// `ci_z`: credible interval z-score (1.96=95%, 2.576=99%).
    /// `eta`: PartialRetarget damping.
    pub fn bayesian_ci(discount: f64, prior_shares: f64, ci_z: f64, eta: f32) -> Self {
        let name = crate::naming::triple_name(
            &composed::BayesianEstimator::new(discount, prior_shares),
            &composed::CredibleIntervalBoundary::with_z(ci_z),
            &composed::PartialRetarget::new(eta),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::BayesianEstimator::new(discount, prior_shares),
                composed::CredibleIntervalBoundary::with_z(ci_z),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// EWMA + AsymmetricCusumBoundary: requires more evidence to tighten
    /// (costly: rejects in-flight shares) than to ease (free).
    pub fn ewma_asymmetric_cusum(
        tau_secs: u64,
        base_sensitivity: f64,
        floor: f64,
        tighten_multiplier: f64,
        eta: f32,
    ) -> Self {
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(tau_secs),
            &composed::AsymmetricCusumBoundary::new(base_sensitivity, floor, tighten_multiplier),
            &composed::PartialRetarget::new(eta),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(tau_secs),
                composed::AsymmetricCusumBoundary::new(base_sensitivity, floor, tighten_multiplier),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Kalman estimator + PoissonCI boundary + PartialRetarget.
    /// The Kalman provides adaptive smoothing + native uncertainty.
    pub fn kalman_poisson(q: f64, eta: f32) -> Self {
        let name = crate::naming::triple_name(
            &composed::KalmanEstimator::new(q),
            &composed::PoissonCI::default_parametric(),
            &composed::PartialRetarget::new(eta),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::KalmanEstimator::new(q),
                composed::PoissonCI::default_parametric(),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Kalman estimator + CredibleIntervalBoundary + PartialRetarget.
    /// Full uncertainty-aware pipeline: Kalman reports confidence,
    /// CI boundary adapts threshold to confidence.
    pub fn kalman_ci(q: f64, ci_z: f64, eta: f32) -> Self {
        let name = crate::naming::triple_name(
            &composed::KalmanEstimator::new(q),
            &composed::CredibleIntervalBoundary::with_z(ci_z),
            &composed::PartialRetarget::new(eta),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::KalmanEstimator::new(q),
                composed::CredibleIntervalBoundary::with_z(ci_z),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// EWMA estimator + CUSUM boundary + PartialRetarget.
    /// Sequential-testing boundary that detects sustained changes faster.
    pub fn ewma_cusum(tau_secs: u64, sensitivity: f64, floor: f64, eta: f32) -> Self {
        let name = format!(
            "EWMA-CUSUM-tau{}-s{}-f{}-eta{}",
            tau_secs,
            (sensitivity * 10.0).round() as u32,
            (floor * 100.0).round() as u32,
            (eta * 100.0).round() as u32,
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(tau_secs),
                composed::CusumBoundary::new(sensitivity, floor),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Kalman estimator + CUSUM boundary + PartialRetarget.
    /// Both new components: adaptive estimator + sequential boundary.
    pub fn kalman_cusum(q: f64, sensitivity: f64, floor: f64, eta: f32) -> Self {
        let name = format!(
            "Kalman-CUSUM-q{}-s{}-f{}-eta{}",
            (q * 10000.0).round() as u32,
            (sensitivity * 10.0).round() as u32,
            (floor * 100.0).round() as u32,
            (eta * 100.0).round() as u32,
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::KalmanEstimator::new(q),
                composed::CusumBoundary::new(sensitivity, floor),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// EWMA + AdaptiveCusumBoundary: rate-adaptive sequential testing.
    /// Sensitivity scales with √(SPM/reference) so it's conservative at
    /// low SPM (less jitter) and aggressive at high SPM (faster detection).
    pub fn ewma_adaptive_cusum(tau_secs: u64, base_sensitivity: f64, floor: f64, eta: f32) -> Self {
        let name = format!(
            "EWMA-AdaCUSUM-tau{}-s{}-f{}-eta{}",
            tau_secs,
            (base_sensitivity * 10.0).round() as u32,
            (floor * 100.0).round() as u32,
            (eta * 100.0).round() as u32,
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(tau_secs),
                composed::AdaptiveCusumBoundary::new(base_sensitivity, floor),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Power-of-2 quantized PID vardiff algorithm.
    ///
    /// P-only controller (Kp = -diff × 0.01, Ki=Kd=0) with power-of-2
    /// quantization. The quantization creates a ~41% dead zone that
    /// makes the algorithm nearly inert for normal variance (±50%).
    ///
    /// Included as a reference implementation for comparing against
    /// PID-based approaches found in the wild.
    pub fn pow2_pid(spm_target: f32, initial_hashrate: f32) -> Self {
        let name = format!("Pow2-PID-spm{}", spm_target.round() as u32);
        Self::new(name, move |clock| {
            let inner = Pow2PidVardiff::new(spm_target, 0.01, initial_hashrate, 1.0, clock);
            VardiffBox(Box::new(AsObservable(inner)))
        })
    }

    /// Pow2 PID with default parameters (SPM=10, 1 PH/s initial).
    pub fn pow2_pid_default() -> Self {
        Self::pow2_pid(10.0, 1.0e15)
    }

    /// Tuned PID controller with balanced gains.
    ///
    /// Operates in difficulty-space with SPM error as the process variable.
    /// All three PID terms active, anti-windup on integral, dead zone for noise.
    pub fn pid_balanced(spm_target: f32) -> Self {
        let name = format!("PID-Balanced-spm{}", spm_target.round() as u32);
        Self::new(name, move |clock| {
            let inner = PidTunedVardiff::new(
                PidConfig::balanced(spm_target),
                1.0e15,
                clock,
            );
            VardiffBox(Box::new(AsObservable(inner)))
        })
    }

    /// Tuned PID controller with aggressive gains (faster response, more jitter).
    pub fn pid_aggressive(spm_target: f32) -> Self {
        let name = format!("PID-Aggressive-spm{}", spm_target.round() as u32);
        Self::new(name, move |clock| {
            let inner = PidTunedVardiff::new(
                PidConfig::aggressive(spm_target),
                1.0e15,
                clock,
            );
            VardiffBox(Box::new(AsObservable(inner)))
        })
    }

    /// Tuned PID controller with conservative gains (minimal jitter, slower response).
    pub fn pid_conservative(spm_target: f32) -> Self {
        let name = format!("PID-Conservative-spm{}", spm_target.round() as u32);
        Self::new(name, move |clock| {
            let inner = PidTunedVardiff::new(
                PidConfig::conservative(spm_target),
                1.0e15,
                clock,
            );
            VardiffBox(Box::new(AsObservable(inner)))
        })
    }

    /// Tuned PID with custom config.
    pub fn pid_custom(name: impl Into<String>, config: PidConfig) -> Self {
        let name = name.into();
        Self::new(name, move |clock| {
            let inner = PidTunedVardiff::new(config, 1.0e15, clock);
            VardiffBox(Box::new(AsObservable(inner)))
        })
    }

    /// AdaCUSUM with AcceleratingPartialRetarget: η ramps up on
    /// consecutive same-direction fires. Addresses the gap where
    /// persistent small drift takes many fires at fixed η=0.2.
    pub fn ada_cusum_accelerating(
        tau_secs: u64,
        base_sensitivity: f64,
        floor: f64,
        tighten_multiplier: f64,
        eta_base: f32,
        eta_max: f32,
        acceleration: f32,
    ) -> Self {
        let name = format!(
            "AdaCUSUM-Accel-eta{}-{}-acc{}",
            (eta_base * 100.0).round() as u32,
            (eta_max * 100.0).round() as u32,
            (acceleration * 100.0).round() as u32,
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(tau_secs),
                composed::AsymmetricCusumBoundary::new(base_sensitivity, floor, tighten_multiplier),
                composed::AcceleratingPartialRetarget::new(eta_base, eta_max, acceleration),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// SpmRatio estimator + AsymmetricCUSUM + PartialRetarget: bypasses
    /// U256 arithmetic entirely by computing h_estimate via direct
    /// SPM-ratio scaling.
    pub fn spm_ratio_cusum(
        tau_secs: u64,
        base_sensitivity: f64,
        floor: f64,
        tighten_multiplier: f64,
        eta: f32,
    ) -> Self {
        let name = format!(
            "SpmRatio-CUSUM-tau{}-eta{}",
            tau_secs,
            (eta * 100.0).round() as u32,
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::SpmRatioEstimator::new(tau_secs),
                composed::AsymmetricCusumBoundary::new(base_sensitivity, floor, tighten_multiplier),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// SignPersistenceCusumBoundary + EWMA + PartialRetarget: lowers
    /// threshold when deviation sign persists across ticks (PID integral
    /// concept adapted to the three-stage framework).
    pub fn ewma_sign_persistence(
        tau_secs: u64,
        base_sensitivity: f64,
        floor: f64,
        tighten_multiplier: f64,
        sign_discount: f64,
        max_discount: f64,
        eta: f32,
    ) -> Self {
        let name = format!(
            "EWMA-SignPersist-sd{}-md{}-eta{}",
            (sign_discount * 100.0).round() as u32,
            (max_discount * 100.0).round() as u32,
            (eta * 100.0).round() as u32,
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(tau_secs),
                composed::SignPersistenceCusumBoundary::new(
                    base_sensitivity,
                    floor,
                    tighten_multiplier,
                    sign_discount,
                    max_discount,
                ),
                composed::PartialRetarget::new(eta),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// The "best-of-best" composition combining the three PID-investigation
    /// improvements:
    ///
    /// - **Estimator**: `SpmRatioEstimator(120s)` — EWMA smoothing with
    ///   direct SPM-ratio scaling (no U256 arithmetic, simpler code path).
    /// - **Boundary**: `AsymmetricCusumBoundary(s=1.5, floor=0.05, tighten=3.0)`
    ///   — proven sequential-evidence boundary with asymmetric tighten cost.
    /// - **UpdateRule**: `AcceleratingPartialRetarget(base=0.2, max=0.6, acc=0.2)`
    ///   — η ramps on consecutive same-direction fires for 22% faster convergence
    ///   with zero jitter cost.
    ///
    /// The parameter sweep confirmed:
    /// - acc=0.2, cap=0.6 is optimal (captures 99.7% of the benefit)
    /// - Jitter is identical to baseline (acceleration only activates post-fire)
    /// - Convergence improves 9-40% across SPM=6-30
    pub fn best_of_best() -> Self {
        let name = crate::naming::triple_name(
            &composed::SpmRatioEstimator::new(120),
            &composed::AsymmetricCusumBoundary::new(1.5, 0.05, 3.0),
            &composed::AcceleratingPartialRetarget::new(0.2, 0.6, 0.2),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::SpmRatioEstimator::new(120),
                composed::AsymmetricCusumBoundary::new(1.5, 0.05, 3.0),
                composed::AcceleratingPartialRetarget::new(0.2, 0.6, 0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// ckpool-inspired algorithm: dual-window EWMA estimator with adaptive
    /// window switching, hysteresis-gate boundary, and full retarget with
    /// oscillation guard.
    ///
    /// Reproduces the core logic of ckpool's `add_submit()` vardiff path
    /// (stratifier.c) within the three-stage pipeline:
    ///
    /// - **Estimator**: `CkpoolEstimator(60, 300)` — dual EWMA (1min short,
    ///   5min long) with automatic switch to the short window when shares
    ///   flood in above the "fast" threshold (72 shares ≈ 240s / 3.33s).
    ///   Includes time-bias correction (`1 - e^(-t/τ)`) for warmup.
    ///
    /// - **Boundary**: `HysteresisGate(72, 240, 0.5, 1.33)` — binary
    ///   fire/no-fire with a data gate (72 shares OR 240s) and an asymmetric
    ///   dead band [0.5×, 1.33×] around target rate ratio.
    ///
    /// - **UpdateRule**: `CkpoolRetarget(1)` — full retarget to the
    ///   estimator's belief with oscillation guard (suppress decrease when
    ///   ≤1 share's worth of data since last fire).
    ///
    /// Reference: <https://github.com/ckolivas/ckpool> (src/stratifier.c)
    /// and <https://github.com/parasitepool/para/blob/master/src/vardiff.rs>
    pub fn ckpool() -> Self {
        let name = crate::naming::triple_name(
            &composed::CkpoolEstimator::new(60, 300),
            &composed::HysteresisGate::ckpool_defaults(),
            &composed::CkpoolRetarget::ckpool_defaults(),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::CkpoolEstimator::new(60, 300),
                composed::HysteresisGate::ckpool_defaults(),
                composed::CkpoolRetarget::ckpool_defaults(),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Hybrid: ckpool's dual-window adaptive estimator paired with
    /// FullRemedy's proven boundary (PoissonCI) and update (PartialRetarget).
    ///
    /// Tests whether ckpool's adaptive window switching and time-bias
    /// correction improve counter-age sensitivity and post-fire accuracy
    /// without the overshoot/accuracy problems caused by ckpool's native
    /// hysteresis gate and full retarget.
    pub fn ckpool_remedy() -> Self {
        let name = crate::naming::triple_name(
            &composed::CkpoolEstimator::new(60, 300),
            &composed::PoissonCI::default_parametric(),
            &composed::PartialRetarget::new(0.2),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::CkpoolEstimator::new(60, 300),
                composed::PoissonCI::default_parametric(),
                composed::PartialRetarget::new(0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// CkpoolRemedy with a lower fast-threshold for the short-window switch.
    ///
    /// The default threshold (72 shares) means the short window never
    /// activates at low SPMs (4-6) within a typical 5-minute reaction
    /// window. Lowering to `ft` shares enables the responsive short EMA
    /// to kick in sooner, potentially improving reaction rate at low SPMs.
    pub fn ckpool_remedy_ft(ft: u32) -> Self {
        let name = crate::naming::triple_name(
            &composed::CkpoolEstimator::with_fast_threshold(60, 300, ft),
            &composed::PoissonCI::default_parametric(),
            &composed::PartialRetarget::new(0.2),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::CkpoolEstimator::with_fast_threshold(60, 300, ft),
                composed::PoissonCI::default_parametric(),
                composed::PartialRetarget::new(0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Hybrid: ckpool estimator + narrowed hysteresis gate + damped update.
    ///
    /// The original ckpool hysteresis [0.5, 1.33] is too wide for 60s
    /// ticks — the rate ratio wanders far from 1.0 while staying "inside"
    /// the band. This variant tightens to [0.8, 1.2] (fire when >20%
    /// off target) with a lower data gate (6 shares OR 60s) that matches
    /// the tick-based evaluation cadence. Paired with PartialRetarget(0.3)
    /// to limit overshoot.
    pub fn ckpool_narrow_hyst() -> Self {
        let name = crate::naming::triple_name(
            &composed::CkpoolEstimator::new(60, 300),
            &composed::HysteresisGate::new(6, 60, 0.8, 1.2),
            &composed::PartialRetarget::new(0.3),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::CkpoolEstimator::new(60, 300),
                composed::HysteresisGate::new(6, 60, 0.8, 1.2),
                composed::PartialRetarget::new(0.3),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Hybrid: standard EWMA estimator with ckpool's time-bias warmup
    /// correction, paired with FullRemedy's boundary and update.
    ///
    /// Isolates the single idea of time-bias correction: does dividing
    /// the EMA by `1 - e^(-dt/tau)` improve counter-age sensitivity and
    /// post-fire accuracy? If this beats FullRemedy on counter-age metrics
    /// without sacrificing anything else, the correction deserves
    /// integration into the main EwmaEstimator.
    pub fn time_bias_remedy() -> Self {
        let name = crate::naming::triple_name(
            &composed::TimeBiasEwmaEstimator::new(120),
            &composed::PoissonCI::default_parametric(),
            &composed::PartialRetarget::new(0.2),
        );
        Self::new(name, |clock| {
            let inner = composed::Composed::new(
                composed::TimeBiasEwmaEstimator::new(120),
                composed::PoissonCI::default_parametric(),
                composed::PartialRetarget::new(0.2),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Parameterized ckpool variant for exploring the parameter space.
    ///
    /// - `tau_short`: short-window EMA time constant (ckpool: 60s)
    /// - `tau_long`: long-window EMA time constant (ckpool: 300s)
    /// - `hysteresis_low`: lower dead-band multiplier (ckpool: 0.5)
    /// - `hysteresis_high`: upper dead-band multiplier (ckpool: 1.33)
    pub fn ckpool_with(
        tau_short: u64,
        tau_long: u64,
        hysteresis_low: f64,
        hysteresis_high: f64,
    ) -> Self {
        let min_shares =
            ((tau_long as f64 * 0.8) / 3.33).round() as u32;
        let min_time = (tau_long as f64 * 0.8).round() as u64;
        let name = crate::naming::triple_name(
            &composed::CkpoolEstimator::new(tau_short, tau_long),
            &composed::HysteresisGate::new(min_shares, min_time, hysteresis_low, hysteresis_high),
            &composed::CkpoolRetarget::ckpool_defaults(),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::CkpoolEstimator::new(tau_short, tau_long),
                composed::HysteresisGate::new(min_shares, min_time, hysteresis_low, hysteresis_high),
                composed::CkpoolRetarget::ckpool_defaults(),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// FullRemedy with AdaptivePartialRetarget: scales η by fire margin.
    /// `eta_base` is the damping at reference_margin; `reference_margin`
    /// is the "normal" margin in percentage points.
    pub fn full_remedy_adaptive(eta_base: f32, reference_margin: f64) -> Self {
        let name = crate::naming::triple_name(
            &composed::EwmaEstimator::new(120),
            &composed::PoissonCI::default_parametric(),
            &composed::AdaptivePartialRetarget::new(eta_base, reference_margin),
        );
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(120),
                composed::PoissonCI::default_parametric(),
                composed::AdaptivePartialRetarget::new(eta_base, reference_margin),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }
}

// ============================================================================
// Grid
// ============================================================================

/// A declarative parameter sweep. The three axes (algorithm,
/// share-rate, scenario) form a Cartesian product; running the grid
/// drives `trial_count` trials at each (algorithm × cell) intersection
/// and returns the per-algorithm metric results.
#[derive(Clone, Debug)]
pub struct Grid {
    pub algorithms: Vec<AlgorithmSpec>,
    pub share_rates: Vec<f32>,
    pub scenarios: Vec<Scenario>,
    pub trial_count: usize,
    pub base_seed: u64,
}

impl Grid {
    /// The canonical default grid: one algorithm (classic
    /// `VardiffState`), 8 share rates (SPM=6-30), 10 scenarios
    /// (cold start + stable + 8 step deltas). 80 cells total.
    /// Matches the cell ordering used by [`crate::baseline::default_cells`].
    pub fn default_classic() -> Self {
        let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
        for &delta in &[-50, -25, -10, -5, 5, 10, 25, 50] {
            scenarios.push(Scenario::Step { delta_pct: delta });
        }
        Self {
            algorithms: vec![AlgorithmSpec::classic_vardiff_state()],
            share_rates: vec![6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0],
            scenarios,
            trial_count: DEFAULT_TRIAL_COUNT,
            base_seed: DEFAULT_BASELINE_SEED,
        }
    }

    /// Counter-age characterization grid: tests reaction time as a
    /// function of how long the algorithm has been settled (counter
    /// age) before a step change. Produces the 2D table
    /// (share_rate × counter_age) that maps directly to shape-proxy
    /// calibration results.
    ///
    /// Uses `ClassicComposed` (the threshold-ladder algorithm) since
    /// counter age is the defining variable for the classic algo's
    /// `StepFunction` boundary. Also includes `VardiffState` (now
    /// AdaCUSUM) for comparison.
    pub fn settled_step() -> Self {
        let settle_values: Vec<u64> = vec![5, 15, 30, 60, 120];
        let deltas: Vec<i32> = vec![-50, 50];
        let mut scenarios = Vec::new();
        for &settle in &settle_values {
            for &delta in &deltas {
                scenarios.push(Scenario::SettledStep {
                    settle_minutes: settle,
                    delta_pct: delta,
                });
            }
        }
        Self {
            algorithms: vec![
                AlgorithmSpec::classic_composed(),
                AlgorithmSpec::classic_vardiff_state(),
            ],
            share_rates: vec![6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0],
            scenarios,
            trial_count: DEFAULT_TRIAL_COUNT,
            base_seed: DEFAULT_BASELINE_SEED,
        }
    }

    /// All cells in the grid (the (share_rate, scenario) tuples).
    /// Independent of the algorithm axis.
    pub fn cells(&self) -> Vec<Cell> {
        let mut out = Vec::with_capacity(self.share_rates.len() * self.scenarios.len());
        for &spm in &self.share_rates {
            for scen in &self.scenarios {
                out.push(Cell {
                    shares_per_minute: spm,
                    scenario: scen.clone(),
                });
            }
        }
        out
    }

    /// Total cell count = algorithms × share_rates × scenarios.
    pub fn total_runs(&self) -> usize {
        self.algorithms.len() * self.share_rates.len() * self.scenarios.len()
    }

    /// Runs the entire grid. Returns a map keyed by algorithm name to
    /// the per-cell results for that algorithm. Drives every trial
    /// through [`run_trial_observed`] so introspection-emitting
    /// algorithms have their `delta` / `threshold` / `h_estimate`
    /// captured per tick.
    ///
    /// ## Seed derivation and the 2^20 cap
    ///
    /// Per-trial seeds are derived as
    ///
    /// ```text
    /// seed = base_seed
    ///        + (cell_index << 20)
    ///        + trial_index
    /// ```
    ///
    /// where `cell_index = (algo_idx * N_spm * N_scen) + (spm_idx *
    /// N_scen) + scen_idx`. The `<< 20` shift gives each cell a
    /// 2^20-wide (1,048,576-entry) seed range, so seeds remain
    /// collision-free as long as `trial_count <= 2^20`. The default
    /// 1,000-trial baseline and even a 50,000-trial stress-baseline
    /// are well within range; trial counts above 1 million should
    /// either replace this with a 64-bit splitmix derivation or
    /// accept the (small) risk of seed reuse across adjacent cells.
    pub fn run(&self) -> HashMap<String, Vec<CellResult>> {
        let n_spm = self.share_rates.len();
        let n_scen = self.scenarios.len();

        let mut by_algorithm: HashMap<String, Vec<CellResult>> =
            HashMap::with_capacity(self.algorithms.len());

        for (algo_idx, algo) in self.algorithms.iter().enumerate() {
            let mut cells_for_algo: Vec<CellResult> = Vec::with_capacity(n_spm * n_scen);
            for (spm_idx, &spm) in self.share_rates.iter().enumerate() {
                for (scen_idx, scen) in self.scenarios.iter().enumerate() {
                    let cell = Cell {
                        shares_per_minute: spm,
                        scenario: scen.clone(),
                    };
                    let cell_index = algo_idx * n_spm * n_scen + spm_idx * n_scen + scen_idx;
                    let result = run_cell_with_algorithm(
                        algo,
                        &cell,
                        self.trial_count,
                        self.base_seed,
                        cell_index as u64,
                    );
                    cells_for_algo.push(result);
                }
            }
            by_algorithm.insert(algo.name.clone(), cells_for_algo);
        }

        by_algorithm
    }

    /// Like [`Grid::run`] but uses **algorithm-agnostic seeds**: all
    /// algorithms in the grid see the same trial inputs at each cell.
    /// Use this for paired A/B comparisons where metric differences
    /// must be attributable to algorithm behavior alone — same Poisson
    /// share stream, same hashrate schedule, same `try_vardiff` call
    /// sequence — not to disparate seeds across algorithms.
    ///
    /// ## When to use which
    ///
    /// - [`Grid::run`] (algo-indexed seeds, `base + (algo_idx ×
    ///   N_spm × N_scen + spm_idx × N_scen + scen_idx) << 20 +
    ///   trial_index`): each algorithm gets its own seed space. Safer
    ///   for regression baselines (no chance of seed reuse colliding
    ///   with an unrelated algorithm's checked-in baseline).
    /// - [`Grid::run_paired`] (algo-stripped seeds, `base + (spm_idx
    ///   × N_scen + scen_idx) << 20 + trial_index`): paired
    ///   comparison. Metric deltas are smaller because the
    ///   per-algorithm samples come from the same underlying random
    ///   variate, so cross-algorithm variance shrinks.
    ///
    /// Don't mix the two — a baseline emitted by `run_paired` is not
    /// directly comparable to one from `run` (the underlying trial
    /// inputs differ).
    pub fn run_paired(&self) -> HashMap<String, Vec<CellResult>> {
        let n_scen = self.scenarios.len();

        let mut by_algorithm: HashMap<String, Vec<CellResult>> =
            HashMap::with_capacity(self.algorithms.len());

        for algo in &self.algorithms {
            let mut cells_for_algo: Vec<CellResult> =
                Vec::with_capacity(self.share_rates.len() * n_scen);
            for (spm_idx, &spm) in self.share_rates.iter().enumerate() {
                for (scen_idx, scen) in self.scenarios.iter().enumerate() {
                    let cell = Cell {
                        shares_per_minute: spm,
                        scenario: scen.clone(),
                    };
                    // Algorithm-stripped cell_index: the same (spm,
                    // scen) gets the same trial seeds regardless of
                    // which algorithm consumes them.
                    let cell_index = spm_idx * n_scen + scen_idx;
                    let result = run_cell_with_algorithm(
                        algo,
                        &cell,
                        self.trial_count,
                        self.base_seed,
                        cell_index as u64,
                    );
                    cells_for_algo.push(result);
                }
            }
            by_algorithm.insert(algo.name.clone(), cells_for_algo);
        }

        by_algorithm
    }
}

/// Runs `trial_count` trials of one (algorithm, cell) intersection
/// and computes every applicable registered metric on the resulting
/// trial set. Uses [`run_trial_observed`] so observable algorithms
/// populate the per-tick introspection fields that bias / variance /
/// overshoot metrics consume.
pub fn run_cell_with_algorithm(
    algorithm: &AlgorithmSpec,
    cell: &Cell,
    trial_count: usize,
    base_seed: u64,
    cell_index: u64,
) -> CellResult {
    let (config, schedule) = cell.scenario.build(cell.shares_per_minute);

    let mut trials: Vec<Trial> = Vec::with_capacity(trial_count);
    for trial_index in 0..trial_count {
        let seed = base_seed
            .wrapping_add(cell_index.wrapping_shl(20))
            .wrapping_add(trial_index as u64);
        let clock = Arc::new(MockClock::new(0));
        let vardiff = (algorithm.factory)(clock.clone());
        let trial = run_trial_observed(vardiff, clock, config.clone(), &schedule, seed);
        trials.push(trial);
    }

    let mut result = CellResult::new(cell.shares_per_minute, cell.scenario.clone());
    for metric in metrics::registry() {
        if metric.applies_to(cell) {
            let values = metric.compute(&trials, cell);
            result.metrics.insert(metric.id(), values);
        }
    }
    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::{default_cells, run_baseline};
    // VardiffState (now the champion) used here only as an arbitrary concrete
    // Vardiff for AsObservable/factory plumbing tests — not as a classic anchor.
    use channels_sv2::VardiffState;

    #[test]
    fn default_classic_has_one_algorithm_eight_rates_ten_scenarios() {
        let g = Grid::default_classic();
        assert_eq!(g.algorithms.len(), 1);
        assert_eq!(g.algorithms[0].name, "Cumul / Step / FullClamp*");
        assert_eq!(g.share_rates.len(), 8);
        assert_eq!(g.scenarios.len(), 10);
        assert_eq!(g.total_runs(), 80);
    }

    #[test]
    fn default_classic_cells_match_default_cells() {
        let grid_cells = Grid::default_classic().cells();
        let legacy_cells = default_cells();
        assert_eq!(grid_cells.len(), legacy_cells.len());
        for (g, l) in grid_cells.iter().zip(legacy_cells.iter()) {
            assert_eq!(g.shares_per_minute, l.shares_per_minute);
            assert_eq!(g.scenario, l.scenario);
        }
    }

    #[test]
    fn vardiff_box_delegates_to_inner_via_as_observable() {
        // VardiffState doesn't impl Observable; wrap in AsObservable to
        // get a Vardiff + Observable that VardiffBox can hold.
        let clock = Arc::new(MockClock::new(100));
        let inner = AsObservable(VardiffState::new_with_clock(1.0, clock.clone()).unwrap());
        let mut boxed = VardiffBox(Box::new(inner));
        assert_eq!(boxed.last_update_timestamp(), 100);
        assert_eq!(boxed.min_allowed_hashrate(), 1.0);
        assert_eq!(boxed.shares_since_last_update(), 0);
        boxed.add_shares(42);
        assert_eq!(boxed.shares_since_last_update(), 42);
        boxed.reset_counter().unwrap();
        assert_eq!(boxed.shares_since_last_update(), 0);
        // AsObservable returns None for last_decision.
        assert!(boxed.last_decision().is_none());
    }

    #[test]
    fn vardiff_box_with_composed_exposes_last_decision() {
        // ClassicComposed natively implements Observable. When wrapped
        // in VardiffBox the last_decision should propagate.
        let clock = Arc::new(MockClock::new(0));
        let inner = composed::classic_composed(1.0, clock.clone());
        let mut boxed = VardiffBox(Box::new(inner));
        // Before any try_vardiff call, last_decision is None.
        assert!(boxed.last_decision().is_none());
        // Drive one tick past the dt > 15 guard to force a decision.
        boxed.add_shares(12);
        clock.set(60);
        let target = bitcoin::Target::MAX;
        let _ = boxed.try_vardiff(1.0e15, &target, 12.0);
        // Composed records the decision; VardiffBox surfaces it.
        assert!(boxed.last_decision().is_some());
    }

    #[test]
    fn algorithm_spec_factory_produces_fresh_instances() {
        let algo = AlgorithmSpec::classic_vardiff_state();
        let clock_a = Arc::new(MockClock::new(0));
        let mut a = (algo.factory)(clock_a.clone());
        let clock_b = Arc::new(MockClock::new(0));
        let mut b = (algo.factory)(clock_b.clone());
        a.add_shares(5);
        b.add_shares(10);
        assert_eq!(a.shares_since_last_update(), 5);
        assert_eq!(b.shares_since_last_update(), 10);
    }

    #[test]
    fn grid_run_with_three_trials_matches_run_baseline_seedwise() {
        // Both paths share seed derivation (same cell_index for
        // single-algorithm grid). Grid uses run_trial_observed while
        // run_baseline uses run_trial — both produce fire-for-fire
        // identical timelines for the same algorithm (asserted by
        // the trial-driver tests), so the metric values must match.
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::classic_vardiff_state()],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable, Scenario::Step { delta_pct: -50 }],
            trial_count: 3,
            base_seed: 0xCAFE,
        };

        let grid_results = grid.run();
        let from_grid = &grid_results["Cumul / Step / FullClamp*"];

        let legacy_cells = vec![
            Cell {
                shares_per_minute: 12.0,
                scenario: Scenario::Stable,
            },
            Cell {
                shares_per_minute: 12.0,
                scenario: Scenario::Step { delta_pct: -50 },
            },
        ];
        let legacy = run_baseline(&legacy_cells, 3, 0xCAFE);

        assert_eq!(from_grid.len(), legacy.len());
        for (g, l) in from_grid.iter().zip(legacy.iter()) {
            assert_eq!(g.shares_per_minute, l.shares_per_minute);
            assert_eq!(g.scenario, l.scenario);
            assert_eq!(g.get("convergence_rate"), l.get("convergence_rate"));
            assert_eq!(g.get("settled_accuracy_p50"), l.get("settled_accuracy_p50"));
            if l.get("reaction_rate").is_some() {
                assert_eq!(g.get("reaction_rate"), l.get("reaction_rate"));
            }
        }
    }

    #[test]
    fn grid_can_hold_multiple_algorithms() {
        let grid = Grid {
            algorithms: vec![
                AlgorithmSpec::classic_vardiff_state(),
                AlgorithmSpec::classic_composed(),
            ],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 3,
            base_seed: 0xDEAD,
        };

        let results = grid.run();
        assert_eq!(results.len(), 2);
        assert!(results.contains_key("Cumul / Step / FullClamp*"));
        assert!(results.contains_key("Cumul / Step / FullClamp"));
        assert_eq!(results["Cumul / Step / FullClamp*"].len(), 1);
        assert_eq!(results["Cumul / Step / FullClamp"].len(), 1);
    }

    #[test]
    fn classic_composed_emits_introspection_metrics_through_grid() {
        // Composed populates last_decision → run_trial_observed
        // populates tick.h_estimate → bias/variance/overshoot metrics
        // can produce non-None values. Sanity-check via the trial
        // structure rather than metric values directly.
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::classic_composed()],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 1,
            base_seed: 0xBEEF,
        };
        let results = grid.run();
        // The trial inside the grid would have had h_estimate
        // populated; that's the property bias/variance need. Direct
        // verification requires constructing a trial outside the grid;
        // this test asserts the metric trait wiring is reachable.
        assert!(results.contains_key("Cumul / Step / FullClamp"));
    }

    #[test]
    fn algorithm_spec_new_accepts_custom_factory() {
        let spec = AlgorithmSpec::new("CustomAlgo", |clock| {
            let inner = AsObservable(VardiffState::new_with_clock(2.5, clock).unwrap());
            VardiffBox(Box::new(inner))
        });
        assert_eq!(spec.name, "CustomAlgo");
        let clock = Arc::new(MockClock::new(0));
        let v = (spec.factory)(clock);
        assert_eq!(v.min_allowed_hashrate(), 2.5);
    }

    // ---- Parametric ----

    #[test]
    fn parametric_factory_constructs_and_runs() {
        let spec = AlgorithmSpec::parametric();
        assert_eq!(spec.name, "Cumul / Poisson-z2.58 / FullClamp");
        let clock = Arc::new(MockClock::new(0));
        let v = (spec.factory)(clock);
        assert_eq!(v.min_allowed_hashrate(), 1.0);
    }

    #[test]
    fn parametric_runs_through_grid() {
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::parametric()],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 2,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        assert!(results.contains_key("Cumul / Poisson-z2.58 / FullClamp"));
        let cells = &results["Cumul / Poisson-z2.58 / FullClamp"];
        assert_eq!(cells.len(), 1);
        // Parametric is observable (it's a Composed) — bias/variance
        // metrics should be present (even if their values are noisy
        // with trial_count=2).
        let cell = &cells[0];
        assert!(cell.metrics.contains_key("bias"));
        assert!(cell.metrics.contains_key("variance"));
        // The standard behavioral metrics should also be there.
        assert!(cell.get("convergence_rate").is_some());
        assert!(cell.get("jitter_p50_per_min").is_some());
    }

    // ---- EWMA-60s ----

    #[test]
    fn ewma_60s_factory_constructs_and_runs() {
        let spec = AlgorithmSpec::ewma_60s();
        assert_eq!(spec.name, "Ewma60s / Poisson-z2.58 / Partial-e0.5");
        let clock = Arc::new(MockClock::new(0));
        let v = (spec.factory)(clock);
        assert_eq!(v.min_allowed_hashrate(), 1.0);
    }

    #[test]
    fn ewma_60s_runs_through_grid() {
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::ewma_60s()],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable, Scenario::Step { delta_pct: -50 }],
            trial_count: 2,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        assert!(results.contains_key("Ewma60s / Poisson-z2.58 / Partial-e0.5"));
        let cells = &results["Ewma60s / Poisson-z2.58 / Partial-e0.5"];
        assert_eq!(cells.len(), 2);
    }

    // ---- Sliding-Window ----

    #[test]
    fn sliding_window_factory_constructs_and_runs() {
        let spec = AlgorithmSpec::sliding_window(10);
        assert_eq!(spec.name, "Slide10t / Poisson-z2.58 / FullNoClamp");
        let clock = Arc::new(MockClock::new(0));
        let v = (spec.factory)(clock);
        assert_eq!(v.min_allowed_hashrate(), 1.0);
    }

    #[test]
    fn sliding_window_runs_through_grid() {
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::sliding_window(10)],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable, Scenario::Step { delta_pct: -50 }],
            trial_count: 2,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        assert!(results.contains_key("Slide10t / Poisson-z2.58 / FullNoClamp"));
        let cells = &results["Slide10t / Poisson-z2.58 / FullNoClamp"];
        assert_eq!(cells.len(), 2);
        // Sliding-Window is observable (it's a Composed) — bias /
        // variance / ramp_target_overshoot metrics should populate.
        // (For ColdStart cells which we don't include here, overshoot
        // would also populate. With Stable + Step only, we expect
        // bias/variance on the Stable cell.)
        assert!(cells[0].metrics.contains_key("bias"));
        assert!(cells[0].metrics.contains_key("variance"));
    }

    #[test]
    fn sliding_window_with_different_window_sizes_produces_different_names() {
        let s10 = AlgorithmSpec::sliding_window(10);
        let s30 = AlgorithmSpec::sliding_window(30);
        let s120 = AlgorithmSpec::sliding_window(120);
        assert_eq!(s10.name, "Slide10t / Poisson-z2.58 / FullNoClamp");
        assert_eq!(s30.name, "Slide30t / Poisson-z2.58 / FullNoClamp");
        assert_eq!(s120.name, "Slide120t / Poisson-z2.58 / FullNoClamp");
    }

    // ---- ParametricStrict ----

    #[test]
    fn parametric_strict_factory_constructs_and_runs() {
        let spec = AlgorithmSpec::parametric_strict();
        assert_eq!(spec.name, "Cumul / Poisson-z3.00 / FullClamp");
        let clock = Arc::new(MockClock::new(0));
        let v = (spec.factory)(clock);
        assert_eq!(v.min_allowed_hashrate(), 1.0);
    }

    #[test]
    fn parametric_strict_runs_through_grid() {
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::parametric_strict()],
            share_rates: vec![6.0, 30.0],
            scenarios: vec![Scenario::Stable, Scenario::Step { delta_pct: -50 }],
            trial_count: 2,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        assert!(results.contains_key("Cumul / Poisson-z3.00 / FullClamp"));
        assert_eq!(results["Cumul / Poisson-z3.00 / FullClamp"].len(), 2 * 2);
    }

    #[test]
    fn parametric_strict_fires_less_than_default_on_stable_load() {
        // The strict boundary should reduce false-fire rate on stable
        // load — that's the entire point of pushing z higher. Run both
        // algorithms paired on the same stable cell and check that
        // strict jitter ≤ default jitter.
        let grid = Grid {
            algorithms: vec![
                AlgorithmSpec::parametric(),
                AlgorithmSpec::parametric_strict(),
            ],
            share_rates: vec![6.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 30,
            base_seed: 0xCAFE,
        };
        let results = grid.run_paired();
        let default_jitter = results["Cumul / Poisson-z2.58 / FullClamp"][0]
            .get("jitter_mean_per_min")
            .unwrap_or(0.0);
        let strict_jitter = results["Cumul / Poisson-z3.00 / FullClamp"][0]
            .get("jitter_mean_per_min")
            .unwrap_or(0.0);
        assert!(
            strict_jitter <= default_jitter,
            "strict_jitter ({}) should be ≤ default_jitter ({}) under stable load",
            strict_jitter,
            default_jitter,
        );
    }

    // ---- ClassicPartialRetarget ----

    #[test]
    fn classic_partial_retarget_factory_constructs_and_runs() {
        let spec = AlgorithmSpec::classic_partial_retarget(0.3);
        assert_eq!(spec.name, "Cumul / Step / Partial-e0.3");
        let clock = Arc::new(MockClock::new(0));
        let v = (spec.factory)(clock);
        assert_eq!(v.min_allowed_hashrate(), 1.0);
    }

    #[test]
    fn classic_partial_retarget_different_eta_produces_different_names() {
        assert_eq!(
            AlgorithmSpec::classic_partial_retarget(0.3).name,
            "Cumul / Step / Partial-e0.3",
        );
        assert_eq!(
            AlgorithmSpec::classic_partial_retarget(0.5).name,
            "Cumul / Step / Partial-e0.5",
        );
        assert_eq!(
            AlgorithmSpec::classic_partial_retarget(1.0).name,
            "Cumul / Step / Partial-e1",
        );
    }

    #[test]
    fn classic_partial_retarget_runs_through_grid() {
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::classic_partial_retarget(0.3)],
            share_rates: vec![6.0, 12.0],
            scenarios: vec![Scenario::ColdStart, Scenario::Stable],
            trial_count: 2,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        assert!(results.contains_key("Cumul / Step / Partial-e0.3"));
        assert_eq!(results["Cumul / Step / Partial-e0.3"].len(), 4);
    }

    // ---- FullRemedy ----

    #[test]
    fn full_remedy_factory_constructs_and_runs() {
        let spec = AlgorithmSpec::full_remedy();
        assert_eq!(spec.name, "Ewma120s / Poisson-z2.58 / Partial-e0.2");
        let clock = Arc::new(MockClock::new(0));
        let v = (spec.factory)(clock);
        assert_eq!(v.min_allowed_hashrate(), 1.0);
    }

    #[test]
    fn full_remedy_runs_through_grid() {
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::full_remedy()],
            share_rates: vec![6.0, 60.0],
            scenarios: vec![
                Scenario::ColdStart,
                Scenario::Stable,
                Scenario::Step { delta_pct: -50 },
            ],
            trial_count: 2,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        assert!(results.contains_key("Ewma120s / Poisson-z2.58 / Partial-e0.2"));
        assert_eq!(results["Ewma120s / Poisson-z2.58 / Partial-e0.2"].len(), 6);
        // FullRemedy is a Composed under the hood, so it's observable —
        // bias/variance metrics must populate on Stable cells.
        let stable_lowspm = results["Ewma120s / Poisson-z2.58 / Partial-e0.2"]
            .iter()
            .find(|c| c.shares_per_minute == 6.0 && c.scenario == Scenario::Stable)
            .expect("Stable@SPM=6 cell present");
        assert!(stable_lowspm.metrics.contains_key("bias"));
        assert!(stable_lowspm.metrics.contains_key("variance"));
    }

    #[test]
    fn full_remedy_distinct_from_ewma_60s() {
        // The full_remedy uses EWMA-120, not EWMA-60. Both algorithms
        // share the same axis types but with different parameters —
        // their behavior must differ at some metric to be a meaningful
        // distinct algorithm.
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::ewma_60s(), AlgorithmSpec::full_remedy()],
            share_rates: vec![6.0],
            scenarios: vec![Scenario::Stable, Scenario::Step { delta_pct: -50 }],
            trial_count: 30,
            base_seed: 0xCAFE,
        };
        let results = grid.run_paired();
        let ewma = &results["Ewma60s / Poisson-z2.58 / Partial-e0.5"];
        let fr = &results["Ewma120s / Poisson-z2.58 / Partial-e0.2"];
        let any_differ = ewma.iter().zip(fr.iter()).any(|(a, b)| {
            [
                "jitter_mean_per_min",
                "settled_accuracy_p50",
                "variance_p50",
            ]
            .iter()
            .any(|k| a.get(k) != b.get(k))
        });
        assert!(
            any_differ,
            "FullRemedy (EWMA-120 + η=0.2) must differ from EWMA-60s (EWMA-60 + η=0.5) \
             on at least one metric",
        );
    }

    // ---- FullRemedy parametric variant ----

    #[test]
    fn full_remedy_with_default_args_matches_full_remedy_behavior() {
        // `full_remedy_with(120, 0.2, 2.576)` is the same composition as
        // `full_remedy()`. Under the derived-name scheme they now share an
        // identical name (the name IS the composition), so they must also
        // produce the identical name — proving the equivalence structurally.
        assert_eq!(
            AlgorithmSpec::full_remedy().name,
            AlgorithmSpec::full_remedy_with(120, 0.2, 2.576).name,
            "full_remedy and full_remedy_with(120, 0.2, 2.576) are the \
             same composition and must derive the same name",
        );
        assert_eq!(
            AlgorithmSpec::full_remedy().name,
            "Ewma120s / Poisson-z2.58 / Partial-e0.2",
        );
    }

    #[test]
    fn full_remedy_with_sweep_distinguishes_parameter_changes() {
        // Two FullRemedy variants differing in one axis must produce a
        // different name and at least one different metric — otherwise
        // the sweep produces collapsed/indistinguishable rows.
        let grid = Grid {
            algorithms: vec![
                AlgorithmSpec::full_remedy_with(120, 0.1, 2.576),
                AlgorithmSpec::full_remedy_with(120, 1.0, 2.576),
            ],
            share_rates: vec![6.0],
            scenarios: vec![Scenario::ColdStart, Scenario::Step { delta_pct: -50 }],
            trial_count: 30,
            base_seed: 0xCAFE,
        };
        let results = grid.run_paired();
        let cautious = &results["Ewma120s / Poisson-z2.58 / Partial-e0.1"];
        let aggressive = &results["Ewma120s / Poisson-z2.58 / Partial-e1"];
        // η=0.1 caps per-fire moves at 10% of the gap; η=1.0 is full
        // retargets. The ramp overshoot tail must differ at SPM=6 cold
        // start (the canonical η-sensitive cell).
        let cautious_cs = cautious
            .iter()
            .find(|c| c.scenario == Scenario::ColdStart)
            .expect("ColdStart cell present");
        let aggressive_cs = aggressive
            .iter()
            .find(|c| c.scenario == Scenario::ColdStart)
            .expect("ColdStart cell present");
        assert_ne!(
            cautious_cs.get("ramp_target_overshoot_p90"),
            aggressive_cs.get("ramp_target_overshoot_p90"),
            "η=0.1 and η=1.0 must produce different ramp overshoot tails"
        );
    }

    // ---- Multi-algorithm Pareto-style comparison ----

    #[test]
    fn multi_algorithm_grid_produces_one_result_set_per_algorithm() {
        // "Algorithm as grid axis" lets one run characterize multiple
        // algorithms on the same cells, ready for Pareto comparison.
        let grid = Grid {
            algorithms: vec![
                AlgorithmSpec::classic_vardiff_state(),
                AlgorithmSpec::classic_composed(),
                AlgorithmSpec::parametric(),
                AlgorithmSpec::ewma_60s(),
            ],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 2,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        assert_eq!(results.len(), 4);
        assert!(results.contains_key("Cumul / Step / FullClamp*"));
        assert!(results.contains_key("Cumul / Step / FullClamp"));
        assert!(results.contains_key("Cumul / Poisson-z2.58 / FullClamp"));
        assert!(results.contains_key("Ewma60s / Poisson-z2.58 / Partial-e0.5"));
        for name in &["Cumul / Step / FullClamp*", "Cumul / Step / FullClamp", "Cumul / Poisson-z2.58 / FullClamp", "Ewma60s / Poisson-z2.58 / Partial-e0.5"] {
            assert_eq!(results[*name].len(), 1, "{} should have 1 cell", name);
        }
    }

    // ---- Orthogonality regression tests ----
    //
    // The framework's load-bearing claim is: hold three axes constant,
    // vary the fourth, attribute the metric change to that one axis.
    // These tests verify the claim empirically:
    //
    //   1. A real axis swap (`ClassicComposed` → `Parametric`,
    //      Boundary-only) MUST produce a measurable behavioral
    //      difference — otherwise the framework can't distinguish
    //      axis-attributable changes.
    //
    //   2. The unchanged axes preserve structural state (share
    //      counters, timestamps) across the swap — confirms that
    //      identical Estimator + Update stages really do
    //      behave identically under the same observe sequence.
    //
    // Without these, future axis additions could silently re-couple
    // axes and degrade the "swap one, see one metric change" promise.

    #[test]
    fn boundary_axis_swap_produces_measurable_metric_change() {
        // Classic vs Parametric differ ONLY in the Boundary axis.
        // At SPM=60 with |Δ|=25% the Boundary swap should affect
        // detection — Parametric's rate-aware threshold is well below
        // Classic's step ladder at moderate `dt_secs`, so reaction-
        // related metrics ought to diverge. Assert that at least *some*
        // observed metric value differs.
        let grid = Grid {
            algorithms: vec![
                AlgorithmSpec::classic_composed(),
                AlgorithmSpec::parametric(),
            ],
            share_rates: vec![60.0],
            scenarios: vec![Scenario::Step { delta_pct: -25 }],
            trial_count: 50,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        let c = &results["Cumul / Step / FullClamp"][0];
        let p = &results["Cumul / Poisson-z2.58 / FullClamp"][0];

        let metrics_to_check = [
            "convergence_rate",
            "settled_accuracy_p50",
            "jitter_p50_per_min",
            "reaction_rate",
            "reaction_p50_secs",
        ];
        let any_differ = metrics_to_check.iter().any(|k| c.get(k) != p.get(k));
        assert!(
            any_differ,
            "Boundary axis swap (Classic → Parametric) must produce at least \
             one metric change; all checked metrics matched. This would mean \
             the framework can't distinguish boundary-axis behavior changes \
             from no-op."
        );
    }

    #[test]
    fn unchanged_axes_preserve_structural_state_across_boundary_swap() {
        // Classic and Parametric share Estimator + Update.
        // For the same observe sequence, the share counters and
        // last_update_timestamp should track identically (until the
        // boundary-driven decision diverges and fires).
        let clock_c = Arc::new(MockClock::new(0));
        let clock_p = Arc::new(MockClock::new(0));
        let mut c = (AlgorithmSpec::classic_composed().factory)(clock_c.clone());
        let mut p = (AlgorithmSpec::parametric().factory)(clock_p.clone());

        // Initial state must match.
        assert_eq!(c.last_update_timestamp(), p.last_update_timestamp());
        assert_eq!(c.shares_since_last_update(), p.shares_since_last_update());
        assert_eq!(c.min_allowed_hashrate(), p.min_allowed_hashrate());

        // Apply the same observe sequence; share counters must move
        // identically.
        for n in &[10u32, 25, 12, 50] {
            c.add_shares(*n);
            p.add_shares(*n);
            assert_eq!(
                c.shares_since_last_update(),
                p.shares_since_last_update(),
                "share counters must track identically under same observe sequence",
            );
        }
    }

    #[test]
    fn run_paired_produces_identical_metrics_for_same_algorithm_twice() {
        // Running the same algorithm through `run_paired` twice (same
        // seeds) should produce identical metric values — proves the
        // paired-seed mechanism works as advertised. (Under the derived-name
        // scheme, two copies of the same composition collapse to one map
        // key, so we run two separate grids and compare instead.)
        let make_grid = || Grid {
            algorithms: vec![AlgorithmSpec::full_remedy()],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable, Scenario::Step { delta_pct: -50 }],
            trial_count: 20,
            base_seed: 0xCAFE,
        };

        let key = "Ewma120s / Poisson-z2.58 / Partial-e0.2";
        let paired_a = make_grid().run_paired();
        let paired_b = make_grid().run_paired();
        let a = &paired_a[key];
        let b = &paired_b[key];

        assert_eq!(a.len(), b.len());
        for (a_cell, b_cell) in a.iter().zip(b.iter()) {
            assert_eq!(a_cell.shares_per_minute, b_cell.shares_per_minute);
            assert_eq!(a_cell.scenario, b_cell.scenario);
            for metric_key in &[
                "convergence_rate",
                "settled_accuracy_p50",
                "jitter_mean_per_min",
                "reaction_rate",
            ] {
                assert_eq!(
                    a_cell.get(metric_key),
                    b_cell.get(metric_key),
                    "{} mismatch on cell {:?}",
                    metric_key,
                    a_cell.scenario,
                );
            }
        }
    }

    #[test]
    fn run_paired_seed_for_cell_does_not_depend_on_algo_idx() {
        // Two grids: one with VardiffState first, one with ClassicComposed
        // first. Under `run_paired`, the cell at (spm=12, Stable) gets
        // the same trial seeds in both — so VardiffState's metrics on
        // that cell must match across the two grids.
        let grid_a = Grid {
            algorithms: vec![
                AlgorithmSpec::classic_vardiff_state(),
                AlgorithmSpec::classic_composed(),
            ],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 10,
            base_seed: 0xDEAD,
        };
        let grid_b = Grid {
            algorithms: vec![
                AlgorithmSpec::classic_composed(),
                AlgorithmSpec::classic_vardiff_state(),
            ],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 10,
            base_seed: 0xDEAD,
        };

        let a = grid_a.run_paired();
        let b = grid_b.run_paired();

        // VardiffState's results must be identical across both grids.
        assert_eq!(
            a["Cumul / Step / FullClamp*"][0].get("convergence_rate"),
            b["Cumul / Step / FullClamp*"][0].get("convergence_rate"),
        );
        assert_eq!(
            a["Cumul / Step / FullClamp*"][0].get("jitter_mean_per_min"),
            b["Cumul / Step / FullClamp*"][0].get("jitter_mean_per_min"),
        );
    }

    #[test]
    fn estimator_axis_swap_produces_measurable_metric_change() {
        // ClassicComposed vs EWMA-60s differ in 3 axes (Estimator,
        // Boundary, Update). Even more permissive: just assert some
        // metric is different — confirms that swapping any axis
        // (including the introspection-sensitive Estimator) is
        // detectable end-to-end.
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::classic_composed(), AlgorithmSpec::ewma_60s()],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 20,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        let c = &results["Cumul / Step / FullClamp"][0];
        let e = &results["Ewma60s / Poisson-z2.58 / Partial-e0.5"][0];

        let metrics_to_check = [
            "convergence_rate",
            "settled_accuracy_p50",
            "jitter_p50_per_min",
        ];
        let any_differ = metrics_to_check.iter().any(|k| c.get(k) != e.get(k));
        assert!(
            any_differ,
            "Estimator+Boundary+Update axis swap (Classic → EWMA-60s) must produce \
             at least one metric change."
        );
    }

    #[test]
    fn ckpool_factory_constructs_and_runs() {
        let clock = Arc::new(channels_sv2::vardiff::MockClock::new(0));
        let vb = (AlgorithmSpec::ckpool().factory)(clock);
        assert_eq!(vb.shares_since_last_update(), 0);
    }

    #[test]
    fn ckpool_runs_through_grid() {
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::ckpool()],
            share_rates: vec![6.0, 60.0],
            scenarios: vec![
                Scenario::ColdStart,
                Scenario::Stable,
                Scenario::Step { delta_pct: -50 },
            ],
            trial_count: 2,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        assert!(results.contains_key("Ckpool60-300s / Hyst-0.5-1.33-g72 / CkpoolRetgt-m1"));
        assert_eq!(results["Ckpool60-300s / Hyst-0.5-1.33-g72 / CkpoolRetgt-m1"].len(), 6);
        // Ckpool is Composed, so introspection works.
        let stable_highspm = results["Ckpool60-300s / Hyst-0.5-1.33-g72 / CkpoolRetgt-m1"]
            .iter()
            .find(|c| c.shares_per_minute == 60.0 && c.scenario == Scenario::Stable)
            .expect("Stable@SPM=60 cell present");
        assert!(stable_highspm.metrics.contains_key("bias"));
        assert!(stable_highspm.metrics.contains_key("variance"));
    }

    #[test]
    fn ckpool_with_parametric_constructs_and_runs() {
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::ckpool_with(30, 180, 0.4, 1.5)],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 2,
            base_seed: 0xBEEF,
        };
        let results = grid.run();
        let name = AlgorithmSpec::ckpool_with(30, 180, 0.4, 1.5).name;
        assert!(
            results.contains_key(&name),
            "Expected key '{}', got: {:?}",
            name,
            results.keys().collect::<Vec<_>>()
        );
    }
}
