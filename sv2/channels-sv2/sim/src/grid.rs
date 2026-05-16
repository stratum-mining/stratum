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
use channels_sv2::vardiff::{error::VardiffError, Clock, MockClock, Vardiff};
use channels_sv2::VardiffState;

use crate::baseline::{
    Cell, CellResult, Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT,
};
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

    /// The production `VardiffState`. Wrapped in [`AsObservable`] so
    /// it satisfies the grid's `Vardiff + Observable` requirement;
    /// `last_decision` always returns `None`, so introspection-only
    /// metrics (bias, variance, overshoot) gracefully report `None`
    /// for this algorithm.
    pub fn classic_vardiff_state() -> Self {
        Self::new("VardiffState", |clock| {
            let inner = VardiffState::new_with_clock(1.0, clock)
                .expect("VardiffState construction should never fail");
            VardiffBox(Box::new(AsObservable(inner)))
        })
    }

    /// The four-axis-decomposed Classic algorithm. Asserted
    /// fire-for-fire equivalent to `VardiffState`; additionally
    /// exposes the per-tick decision state, so bias / variance /
    /// overshoot metrics work.
    pub fn classic_composed() -> Self {
        Self::new("ClassicComposed", |clock| {
            VardiffBox(Box::new(composed::classic_composed(1.0, clock)))
        })
    }

    /// The Parametric algorithm: same as Classic except the Boundary
    /// axis swaps from the share-rate-blind step ladder to a
    /// `PoissonCI(z = 2.576, margin = 0.05)` rate-aware threshold.
    ///
    /// This is the canonical one-axis-swap from Classic: same
    /// Estimator, same Statistic, same Update — only the threshold
    /// form differs. Holding three axes constant and varying the
    /// fourth is exactly what the four-axis decomposition supports.
    pub fn parametric() -> Self {
        Self::new("Parametric", |clock| {
            let inner = composed::Composed::new(
                composed::CumulativeCounter::new(),
                composed::AbsoluteRatio,
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
    /// Classic — Estimator, Boundary, Update — leaving Statistic
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
        let name = format!("EWMA-{tau_secs}s");
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(tau_secs),
                composed::AbsoluteRatio,
                composed::PoissonCI::default_parametric(),
                composed::PartialRetarget::default_ewma(),
                1.0,
                clock,
            );
            VardiffBox(Box::new(inner))
        })
    }

    /// Sliding-Window algorithm: `SlidingWindowEstimator(n_ticks)` +
    /// `AbsoluteRatio` + `PoissonCI(2.576, 0.05)` +
    /// `FullRetargetNoClamp`.
    ///
    /// Algorithm name is `"SlidingWindow-{n_ticks}t"`. The
    /// `n_ticks` × `tick_secs (=60)` product is the effective window
    /// in seconds; e.g. `n_ticks = 10` → 10-minute sliding window.
    pub fn sliding_window(n_ticks: usize) -> Self {
        let name = format!("SlidingWindow-{n_ticks}t");
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::SlidingWindowEstimator::new(n_ticks),
                composed::AbsoluteRatio,
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
        Self::new("ParametricStrict", |clock| {
            let inner = composed::Composed::new(
                composed::CumulativeCounter::new(),
                composed::AbsoluteRatio,
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
    /// **Why study this in isolation?** The four-axis hypothesis is
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
        let name = format!("ClassicPartialRetarget-{}", (eta * 100.0).round() as u32);
        Self::new(name, move |clock| {
            let inner = composed::Composed::new(
                composed::CumulativeCounter::new(),
                composed::AbsoluteRatio,
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
    /// - Boundary: `PoissonCI` (default 99% CI) — rate-aware threshold
    ///   floor, prevents false-fires on stable load.
    /// - Update: `PartialRetarget(0.3)` — bounds the magnitude of any
    ///   single fire, defense-in-depth against estimator-noise tails.
    ///
    /// τ = 120s rather than 60s because the τ sweep
    /// (`sweep-ewma-tau.rs`) showed τ = 120 wins the SPM=6 decoupling
    /// Pareto cleanly. η = 0.3 rather than 0.5 because the EWMA
    /// already smooths — a smaller η compounds the temporal damping
    /// rather than fighting it.
    ///
    /// Prediction: worst-case `--scan-overshoot 100 --spm 6` should
    /// land below 30%, vs the 187% under VardiffState and Parametric
    /// and 56% under EWMA-60s.
    pub fn full_remedy() -> Self {
        Self::new("FullRemedy", |clock| {
            let inner = composed::Composed::new(
                composed::EwmaEstimator::new(120),
                composed::AbsoluteRatio,
                composed::PoissonCI::default_parametric(),
                composed::PartialRetarget::new(0.3),
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
    /// `VardiffState`), 5 share rates, 10 scenarios (cold start +
    /// stable + 8 step deltas). 50 cells total. Matches the cell
    /// ordering used by [`crate::baseline::default_cells`].
    pub fn default_classic() -> Self {
        let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
        for &delta in &[-50, -25, -10, -5, 5, 10, 25, 50] {
            scenarios.push(Scenario::Step { delta_pct: delta });
        }
        Self {
            algorithms: vec![AlgorithmSpec::classic_vardiff_state()],
            share_rates: vec![6.0, 12.0, 30.0, 60.0, 120.0],
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
    /// where `cell_index = algo_idx × N_spm × N_scen + spm_idx × N_scen
    /// + scen_idx`. The `<< 20` shift gives each cell a 2^20-wide
    /// (1,048,576-entry) seed range, so seeds remain collision-free as
    /// long as `trial_count ≤ 2^20`. The default 1,000-trial baseline
    /// and even a 50,000-trial stress-baseline are well within range;
    /// trial counts above 1 million should either replace this with a
    /// 64-bit splitmix derivation or accept the (small) risk of seed
    /// reuse across adjacent cells.
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
                    let cell_index =
                        algo_idx * n_spm * n_scen + spm_idx * n_scen + scen_idx;
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

    #[test]
    fn default_classic_has_one_algorithm_five_rates_ten_scenarios() {
        let g = Grid::default_classic();
        assert_eq!(g.algorithms.len(), 1);
        assert_eq!(g.algorithms[0].name, "VardiffState");
        assert_eq!(g.share_rates.len(), 5);
        assert_eq!(g.scenarios.len(), 10);
        assert_eq!(g.total_runs(), 50);
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
        let from_grid = &grid_results["VardiffState"];

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
        assert!(results.contains_key("VardiffState"));
        assert!(results.contains_key("ClassicComposed"));
        assert_eq!(results["VardiffState"].len(), 1);
        assert_eq!(results["ClassicComposed"].len(), 1);
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
        assert!(results.contains_key("ClassicComposed"));
    }

    #[test]
    fn algorithm_spec_new_accepts_custom_factory() {
        let spec = AlgorithmSpec::new("CustomAlgo", |clock| {
            let inner = AsObservable(
                VardiffState::new_with_clock(2.5, clock).unwrap(),
            );
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
        assert_eq!(spec.name, "Parametric");
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
        assert!(results.contains_key("Parametric"));
        let cells = &results["Parametric"];
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
        assert_eq!(spec.name, "EWMA-60s");
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
        assert!(results.contains_key("EWMA-60s"));
        let cells = &results["EWMA-60s"];
        assert_eq!(cells.len(), 2);
    }

    // ---- Sliding-Window ----

    #[test]
    fn sliding_window_factory_constructs_and_runs() {
        let spec = AlgorithmSpec::sliding_window(10);
        assert_eq!(spec.name, "SlidingWindow-10t");
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
        assert!(results.contains_key("SlidingWindow-10t"));
        let cells = &results["SlidingWindow-10t"];
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
        assert_eq!(s10.name, "SlidingWindow-10t");
        assert_eq!(s30.name, "SlidingWindow-30t");
        assert_eq!(s120.name, "SlidingWindow-120t");
    }

    // ---- ParametricStrict ----

    #[test]
    fn parametric_strict_factory_constructs_and_runs() {
        let spec = AlgorithmSpec::parametric_strict();
        assert_eq!(spec.name, "ParametricStrict");
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
        assert!(results.contains_key("ParametricStrict"));
        assert_eq!(results["ParametricStrict"].len(), 2 * 2);
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
        let default_jitter = results["Parametric"][0]
            .get("jitter_mean_per_min")
            .unwrap_or(0.0);
        let strict_jitter = results["ParametricStrict"][0]
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
        assert_eq!(spec.name, "ClassicPartialRetarget-30");
        let clock = Arc::new(MockClock::new(0));
        let v = (spec.factory)(clock);
        assert_eq!(v.min_allowed_hashrate(), 1.0);
    }

    #[test]
    fn classic_partial_retarget_different_eta_produces_different_names() {
        assert_eq!(
            AlgorithmSpec::classic_partial_retarget(0.3).name,
            "ClassicPartialRetarget-30",
        );
        assert_eq!(
            AlgorithmSpec::classic_partial_retarget(0.5).name,
            "ClassicPartialRetarget-50",
        );
        assert_eq!(
            AlgorithmSpec::classic_partial_retarget(1.0).name,
            "ClassicPartialRetarget-100",
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
        assert!(results.contains_key("ClassicPartialRetarget-30"));
        assert_eq!(results["ClassicPartialRetarget-30"].len(), 4);
    }

    // ---- FullRemedy ----

    #[test]
    fn full_remedy_factory_constructs_and_runs() {
        let spec = AlgorithmSpec::full_remedy();
        assert_eq!(spec.name, "FullRemedy");
        let clock = Arc::new(MockClock::new(0));
        let v = (spec.factory)(clock);
        assert_eq!(v.min_allowed_hashrate(), 1.0);
    }

    #[test]
    fn full_remedy_runs_through_grid() {
        let grid = Grid {
            algorithms: vec![AlgorithmSpec::full_remedy()],
            share_rates: vec![6.0, 60.0],
            scenarios: vec![Scenario::ColdStart, Scenario::Stable, Scenario::Step { delta_pct: -50 }],
            trial_count: 2,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        assert!(results.contains_key("FullRemedy"));
        assert_eq!(results["FullRemedy"].len(), 6);
        // FullRemedy is a Composed under the hood, so it's observable —
        // bias/variance metrics must populate on Stable cells.
        let stable_lowspm = results["FullRemedy"]
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
            algorithms: vec![
                AlgorithmSpec::ewma_60s(),
                AlgorithmSpec::full_remedy(),
            ],
            share_rates: vec![6.0],
            scenarios: vec![Scenario::Stable, Scenario::Step { delta_pct: -50 }],
            trial_count: 30,
            base_seed: 0xCAFE,
        };
        let results = grid.run_paired();
        let ewma = &results["EWMA-60s"];
        let fr = &results["FullRemedy"];
        let any_differ = ewma.iter().zip(fr.iter()).any(|(a, b)| {
            ["jitter_mean_per_min", "settled_accuracy_p50", "variance_p50"]
                .iter()
                .any(|k| a.get(k) != b.get(k))
        });
        assert!(
            any_differ,
            "FullRemedy (EWMA-120 + η=0.3) must differ from EWMA-60s (EWMA-60 + η=0.5) \
             on at least one metric",
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
        assert!(results.contains_key("VardiffState"));
        assert!(results.contains_key("ClassicComposed"));
        assert!(results.contains_key("Parametric"));
        assert!(results.contains_key("EWMA-60s"));
        for name in &["VardiffState", "ClassicComposed", "Parametric", "EWMA-60s"] {
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
    //      identical Estimator + Statistic + Update axes really do
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
        let c = &results["ClassicComposed"][0];
        let p = &results["Parametric"][0];

        let metrics_to_check = [
            "convergence_rate",
            "settled_accuracy_p50",
            "jitter_p50_per_min",
            "reaction_rate",
            "reaction_p50_secs",
        ];
        let any_differ = metrics_to_check
            .iter()
            .any(|k| c.get(k) != p.get(k));
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
        // Classic and Parametric share Estimator + Statistic + Update.
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
    fn run_paired_produces_identical_metrics_for_equivalent_algorithms() {
        // ClassicComposed is asserted fire-for-fire equivalent to
        // VardiffState in composed::equivalence_tests. Under
        // `run_paired` (same seeds across algorithms) the metric values
        // should match *exactly* — proves the paired-seed mechanism
        // works as advertised.
        //
        // Under regular `Grid::run` (algo-indexed seeds) they would
        // differ at the noise level (~1-2%) because each algorithm
        // gets its own seed set. The contrast is the point of the test.
        let grid = Grid {
            algorithms: vec![
                AlgorithmSpec::classic_vardiff_state(),
                AlgorithmSpec::classic_composed(),
            ],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable, Scenario::Step { delta_pct: -50 }],
            trial_count: 20,
            base_seed: 0xCAFE,
        };

        let paired = grid.run_paired();
        let vs = &paired["VardiffState"];
        let cc = &paired["ClassicComposed"];

        assert_eq!(vs.len(), cc.len());
        for (vs_cell, cc_cell) in vs.iter().zip(cc.iter()) {
            assert_eq!(vs_cell.shares_per_minute, cc_cell.shares_per_minute);
            assert_eq!(vs_cell.scenario, cc_cell.scenario);
            // Critical assertion: same algorithm + same trials =
            // identical metric values.
            for metric_key in &[
                "convergence_rate",
                "settled_accuracy_p50",
                "settled_accuracy_p90",
                "jitter_p50_per_min",
                "jitter_mean_per_min",
                "reaction_rate",
                "reaction_p50_secs",
            ] {
                assert_eq!(
                    vs_cell.get(metric_key),
                    cc_cell.get(metric_key),
                    "{} mismatch on cell {:?}",
                    metric_key,
                    vs_cell.scenario,
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
            a["VardiffState"][0].get("convergence_rate"),
            b["VardiffState"][0].get("convergence_rate"),
        );
        assert_eq!(
            a["VardiffState"][0].get("jitter_mean_per_min"),
            b["VardiffState"][0].get("jitter_mean_per_min"),
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
            algorithms: vec![
                AlgorithmSpec::classic_composed(),
                AlgorithmSpec::ewma_60s(),
            ],
            share_rates: vec![12.0],
            scenarios: vec![Scenario::Stable],
            trial_count: 20,
            base_seed: 0xCAFE,
        };
        let results = grid.run();
        let c = &results["ClassicComposed"][0];
        let e = &results["EWMA-60s"][0];

        let metrics_to_check = [
            "convergence_rate",
            "settled_accuracy_p50",
            "jitter_p50_per_min",
        ];
        let any_differ = metrics_to_check
            .iter()
            .any(|k| c.get(k) != e.get(k));
        assert!(
            any_differ,
            "Estimator+Boundary+Update axis swap (Classic → EWMA-60s) must produce \
             at least one metric change."
        );
    }
}
