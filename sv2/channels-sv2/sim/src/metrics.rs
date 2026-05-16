//! Metric computation over collections of [`Trial`] results.
//!
//! ## Registry-driven metrics
//!
//! Every metric is a `Box<dyn Metric>` in the [`registry`]. Each metric
//! impl owns four things about itself:
//!
//! 1. **Computation**: [`Metric::compute`] takes the trials for one cell
//!    and returns a [`MetricValues`] — a flat list of named `Option<f64>`
//!    values that this metric emits.
//! 2. **Tolerance policy**: [`Metric::tolerance_checks`] declares which
//!    keys are compared against baseline and by what rule. The regression
//!    comparator iterates the registry and applies each declared check
//!    uniformly; there is no hardcoded per-metric logic in
//!    `regression.rs`.
//! 3. **Cell applicability**: [`Metric::applies_to`] gates whether the
//!    metric runs on a given cell. Reaction metrics return `true` only
//!    for `Step` scenarios; the other metrics apply to every cell.
//! 4. **Classification**: [`Metric::category`] and [`Metric::class`]
//!    surface what kind of metric this is and whether failures are
//!    must-have / should-have / report-only. Used by the comparator and
//!    the markdown renderer.
//!
//! Adding a new metric (bias, overshoot, decoupling score, etc.) is one
//! new `impl Metric` with no edits to baseline.rs's serializer or
//! regression.rs's comparator — they read everything they need through
//! the trait.
//!
//! ## Free-function entry points
//!
//! Per-trial and per-distribution metric implementations are also
//! exposed as free functions (`convergence_time_for_trial`,
//! `convergence_time_distribution`, etc.) for callers that want
//! direct access without going through the `Metric` trait. The
//! trait impls dispatch to these.

use crate::baseline::{Cell, Scenario};
use crate::trial::Trial;
use std::collections::HashMap;
use std::fmt::Debug;

// ============================================================================
// Distribution
// ============================================================================

/// Default number of bootstrap resamples used to compute CIs for
/// percentiles emitted by metric implementations. 1000 is enough for
/// stable 95% CIs at our typical N=1000 trial counts.
pub const DEFAULT_CI_RESAMPLES: usize = 1000;

/// Deterministic seed for the bootstrap RNG used inside metric CI
/// computation. Fixed so every regenerate produces byte-identical
/// CI bounds.
pub const CI_SEED: u64 = 0xC1_C1_C1_C1_C1_C1_C1_C1;

/// A sorted collection of numeric trial-derived values, plus accessors
/// for summary statistics.
#[derive(Clone)]
pub struct Distribution {
    /// Sorted ascending. NaN values are filtered out at construction.
    sorted: Vec<f64>,
}

impl Distribution {
    /// Constructs a distribution from a vector of values. NaN values
    /// are silently dropped.
    pub fn new(values: Vec<f64>) -> Self {
        let mut sorted: Vec<f64> = values.into_iter().filter(|x| !x.is_nan()).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Self { sorted }
    }

    /// Returns the value at the given percentile rank `p ∈ [0, 100]`
    /// using nearest-rank interpolation. `None` if empty.
    pub fn percentile(&self, p: f64) -> Option<f64> {
        if self.sorted.is_empty() {
            return None;
        }
        let n = self.sorted.len();
        let idx = ((p / 100.0) * (n as f64 - 1.0)).round() as usize;
        Some(self.sorted[idx.min(n - 1)])
    }

    /// Returns the arithmetic mean. `None` if empty.
    pub fn mean(&self) -> Option<f64> {
        if self.sorted.is_empty() {
            return None;
        }
        Some(self.sorted.iter().sum::<f64>() / self.sorted.len() as f64)
    }

    /// Number of values.
    pub fn count(&self) -> usize {
        self.sorted.len()
    }

    pub fn p10(&self) -> Option<f64> {
        self.percentile(10.0)
    }
    pub fn p25(&self) -> Option<f64> {
        self.percentile(25.0)
    }
    pub fn p50(&self) -> Option<f64> {
        self.percentile(50.0)
    }
    pub fn p75(&self) -> Option<f64> {
        self.percentile(75.0)
    }
    pub fn p90(&self) -> Option<f64> {
        self.percentile(90.0)
    }
    pub fn p95(&self) -> Option<f64> {
        self.percentile(95.0)
    }
    pub fn p99(&self) -> Option<f64> {
        self.percentile(99.0)
    }

    /// Returns the percentile point estimate plus its bootstrap 95% CI
    /// bounds: `(point, ci_low, ci_high)`. Each component is
    /// independently `Option` — empty distribution yields `(None, None,
    /// None)`; very small distributions yield a point estimate without
    /// CI bounds.
    ///
    /// Uses [`bootstrap_percentile_ci`] internally with `n_resamples`
    /// and the deterministic [`CI_SEED`].
    pub fn percentile_ci(
        &self,
        p: f64,
        n_resamples: usize,
    ) -> (Option<f64>, Option<f64>, Option<f64>) {
        let point = self.percentile(p);
        if self.sorted.len() < 2 {
            return (point, None, None);
        }
        let mut rng = crate::rng::XorShift64::new(CI_SEED);
        let (lo, hi) = bootstrap_percentile_ci(&self.sorted, p, n_resamples, &mut rng);
        (point, lo, hi)
    }

    /// Convenience: compute the percentile and its bootstrap CI, then
    /// record both in `mv` under the given `key`. Equivalent to
    /// `mv.set_with_ci(key, point, ci_low, ci_high)`. Used by every
    /// percentile-emitting metric impl.
    pub fn record_percentile(&self, mv: &mut MetricValues, key: &'static str, p: f64) {
        let (point, lo, hi) = self.percentile_ci(p, DEFAULT_CI_RESAMPLES);
        mv.set_with_ci(key, point, lo, hi);
    }
}

impl std::fmt::Debug for Distribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Distribution(n={}", self.count())?;
        if let Some(p) = self.p50() {
            write!(f, " p50={p:.3}")?;
        }
        if let Some(p) = self.p90() {
            write!(f, " p90={p:.3}")?;
        }
        if let Some(p) = self.p99() {
            write!(f, " p99={p:.3}")?;
        }
        if let Some(m) = self.mean() {
            write!(f, " mean={m:.3}")?;
        }
        write!(f, ")")
    }
}

// ============================================================================
// The Metric trait and supporting types
// ============================================================================

/// Which conceptual category a metric belongs to. Used by the
/// Markdown renderer to group sections and by the regression
/// comparator to classify failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricCategory {
    /// Directly observable behavioral property (convergence,
    /// reactivity, accuracy, jitter).
    Behavioral,
    /// Statistical estimator property derived from many trials (bias,
    /// variance, MSE).
    Estimator,
    /// Static or per-impl property (cost, state size, complexity).
    Structural,
    /// Behavior under stress conditions (stall, overshoot, transient
    /// recovery).
    Robustness,
}

/// Whether a regression on this metric should fail CI, warn, or just be
/// reported. Made explicit so the implicit policy in the previous
/// hardcoded comparator becomes inspectable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricClass {
    /// Failures fail CI.
    MustHave,
    /// Failures emit a warning but don't fail CI.
    ShouldHave,
    /// Always reported, never asserted.
    ReportOnly,
}

/// Flat key → value bag emitted by a metric for a single cell. Keys are
/// `&'static str` because every metric knows its key set at compile
/// time. The order matches insertion order so the TOML serializer can
/// produce deterministic output.
///
/// Each entry optionally carries a `(ci_low, ci_high)` pair so a
/// percentile estimate can sit alongside its bootstrap CI bounds.
/// The serializer emits these as separate `<key>_ci_low` /
/// `<key>_ci_high` lines mechanically; metrics record them via
/// [`MetricValues::set_with_ci`].
#[derive(Debug, Clone, Default)]
pub struct MetricValues {
    /// `(key, value, ci)` triples in insertion order. The CI is
    /// optional per entry — only percentile metrics emit it.
    pub values: Vec<(&'static str, Option<f64>, Option<(f64, f64)>)>,
}

impl MetricValues {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a value with no CI bound. Use `None` for "not
    /// computable in this cell" (e.g., a percentile of an empty
    /// distribution).
    pub fn set(&mut self, key: &'static str, value: Option<f64>) {
        self.values.push((key, value, None));
    }

    /// Records a percentile point estimate plus its bootstrap CI
    /// bounds. The serializer emits the CI as additional `_ci_low` /
    /// `_ci_high` lines. If either bound is `None`, the corresponding
    /// line is omitted.
    pub fn set_with_ci(
        &mut self,
        key: &'static str,
        value: Option<f64>,
        ci_low: Option<f64>,
        ci_high: Option<f64>,
    ) {
        let ci = match (ci_low, ci_high) {
            (Some(lo), Some(hi)) => Some((lo, hi)),
            _ => None,
        };
        self.values.push((key, value, ci));
    }

    /// Looks up a value by key. Returns `None` if the key is absent or
    /// its stored value is `None`.
    pub fn get(&self, key: &str) -> Option<f64> {
        self.values
            .iter()
            .find(|(k, _, _)| *k == key)
            .and_then(|(_, v, _)| *v)
    }

    /// Looks up the CI bounds for a key. Returns `None` if absent or
    /// no CI was recorded.
    pub fn get_ci(&self, key: &str) -> Option<(f64, f64)> {
        self.values
            .iter()
            .find(|(k, _, _)| *k == key)
            .and_then(|(_, _, ci)| *ci)
    }

    /// Iterates over `(key, value, ci)` triples in insertion order.
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&'static str, Option<f64>, Option<(f64, f64)>)> + '_ {
        self.values.iter().copied()
    }
}

/// Direction of improvement for a metric. The regression comparator
/// uses this to decide which side of the baseline distribution is
/// "regressing."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Smaller values are better (jitter, settled-accuracy error,
    /// overshoot, latency percentiles).
    LowerIsBetter,
    /// Larger values are better (convergence_rate, reaction_rate,
    /// decoupling score).
    HigherIsBetter,
    /// Either direction away from baseline is a regression. Used for
    /// signed quantities like `reaction_asymmetry` where moving toward
    /// or away from zero both signal a behavioral shift.
    Either,
}

/// One side of the baseline's value with an optional bootstrap CI
/// bound. The comparator constructs this from three lookups in the
/// baseline TOML: the point estimate at `<key>` plus the CI bounds at
/// `<key>_ci_low` and `<key>_ci_high`.
///
/// When CIs are absent (older baseline, or distribution too small to
/// bootstrap), the comparator falls back to point-estimate-based
/// tolerance.
#[derive(Debug, Clone, Copy)]
pub struct BaselineValue {
    pub point: f64,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
}

/// A regression tolerance policy. The comparator applies the policy
/// between a [`BaselineValue`] (point + CI bounds) and a current
/// scalar; a `Some(description)` return marks a failure.
///
/// Semantics: the metric defines a *regressing direction* via the
/// embedded [`Direction`]. The check fails iff the current value is
/// outside the baseline's CI envelope *plus* an extra slack budget,
/// in the regressing direction. The extra slack is either an absolute
/// floor (`extra_abs`) or a multiplicative band on top of the CI
/// bound — whichever is larger.
///
/// When the baseline has no CI bounds (older format), the check
/// degrades gracefully: the CI envelope collapses to a point, so
/// "outside the envelope" becomes "more than `extra_abs` (or
/// `extra_mul × point`) away from the point estimate in the
/// regressing direction."
#[derive(Debug, Clone, Copy)]
pub enum Tolerance {
    /// Statistical-noise-aware bound.
    WithinCi {
        direction: Direction,
        /// Additional absolute slack beyond the CI bound before
        /// flagging. Always applied. For metrics like jitter where
        /// the baseline can be exactly zero, `extra_abs` is the only
        /// thing the check effectively does.
        extra_abs: f64,
        /// Optional additional multiplicative slack (`extra_mul ×
        /// baseline_point`). Useful for naturally multiplicative
        /// metrics (latency percentiles, overshoot magnitudes) where
        /// "20% above baseline" makes sense regardless of the
        /// absolute scale. If `Some(0.10)`, allow up to 10% beyond
        /// the CI bound. `None` → only `extra_abs` applies.
        extra_mul: Option<f64>,
    },
    /// No assertion. Diff is emitted in the report but never fails CI.
    ReportOnly,
}

impl Tolerance {
    /// Returns `None` if the tolerance is satisfied, `Some(description)`
    /// otherwise.
    pub fn apply(&self, baseline: BaselineValue, current: f64) -> Option<String> {
        match self {
            Tolerance::ReportOnly => None,
            Tolerance::WithinCi {
                direction,
                extra_abs,
                extra_mul,
            } => {
                // The CI envelope. If absent, collapse to a point.
                let lo = baseline.ci_low.unwrap_or(baseline.point);
                let hi = baseline.ci_high.unwrap_or(baseline.point);

                // Slack added on top of the CI bound. We compute the
                // larger of the absolute and multiplicative slack
                // against the relevant endpoint.
                let abs_at_hi = extra_mul
                    .map(|m| (hi.abs()) * m)
                    .unwrap_or(0.0)
                    .max(*extra_abs);
                let abs_at_lo = extra_mul
                    .map(|m| (lo.abs()) * m)
                    .unwrap_or(0.0)
                    .max(*extra_abs);

                let upper_bound = hi + abs_at_hi;
                let lower_bound = lo - abs_at_lo;

                let outside_high = current > upper_bound;
                let outside_low = current < lower_bound;

                let regressed = match direction {
                    Direction::LowerIsBetter => outside_high,
                    Direction::HigherIsBetter => outside_low,
                    Direction::Either => outside_high || outside_low,
                };

                if regressed {
                    Some(format!(
                        "{:?}: current = {:.4} outside [{:.4}, {:.4}] \
                         (CI [{:.4}, {:.4}] + slack ±max(abs={}, mul={:?}))",
                        direction,
                        current,
                        lower_bound,
                        upper_bound,
                        lo,
                        hi,
                        extra_abs,
                        extra_mul,
                    ))
                } else {
                    None
                }
            }
        }
    }
}

/// A single (key, tolerance) check declared by a metric for a cell.
#[derive(Debug, Clone, Copy)]
pub struct ToleranceCheck {
    /// The MetricValues key to look up in both baseline and current.
    pub key: &'static str,
    pub tolerance: Tolerance,
}

/// The framework's central metric trait. Each metric is a
/// self-contained module owning its computation, classification,
/// applicability, and tolerance policy. The regression comparator and
/// baseline serializer iterate `registry()` and ask each metric for
/// what they need; there is no per-metric switch anywhere else in the
/// crate.
pub trait Metric: Send + Sync + Debug {
    /// Short identifier — used as the key into [`crate::baseline::CellResult::metrics`].
    fn id(&self) -> &'static str;

    /// Conceptual category.
    fn category(&self) -> MetricCategory;

    /// Regression-policy class.
    fn class(&self) -> MetricClass;

    /// Whether this metric should be computed for the given cell.
    /// Default: yes. Reaction-related metrics override to gate on
    /// `Step` scenarios.
    fn applies_to(&self, _cell: &Cell) -> bool {
        true
    }

    /// Compute the aggregate values for this cell.
    fn compute(&self, trials: &[Trial], cell: &Cell) -> MetricValues;

    /// Declarative tolerance policy. Returns the checks to apply
    /// between baseline and current values for this cell.
    fn tolerance_checks(&self, _cell: &Cell) -> Vec<ToleranceCheck> {
        vec![]
    }

    /// Optional Poisson-noise-floor reference for a specific key.
    /// Returns `None` by default; metrics with a closed-form lower
    /// bound (SettledAccuracy, RampTargetOvershoot, Jitter) override.
    fn fundamental_limit(&self, _cell: &Cell, _key: &str) -> Option<f64> {
        None
    }

    /// Declarative summary specs for the top-of-MD TL;DR section.
    /// Default returns empty — metrics that want to surface a
    /// headline (key, direction, scenario filter) override.
    fn summary_specs(&self) -> Vec<SummarySpec> {
        Vec::new()
    }

    /// Renders this metric's contribution to the Markdown report. May
    /// emit zero, one, or multiple `## section` blocks. Default impl
    /// is a no-op so metrics that haven't migrated yet (or that have
    /// no sensible Markdown rendering) don't appear in the report.
    ///
    /// The order of section emission across the report is the order
    /// of [`registry`]. Within a section a metric chooses how to
    /// arrange its data (typically rows = cells, columns =
    /// percentiles, but reaction-time also emits a 2D
    /// rates × deltas sensitivity table).
    fn render_markdown(&self, _results: &[crate::baseline::CellResult], _w: &mut String) {}
}

/// One headline row in the TL;DR summary table at the top of each
/// algorithm's Markdown report. A metric declares one or more of
/// these via [`Metric::summary_specs`]; the renderer iterates the
/// matching cells and finds the best / worst value across share
/// rates.
#[derive(Debug, Clone, Copy)]
pub struct SummarySpec {
    /// Human-readable label shown in the summary table.
    pub label: &'static str,
    /// The MetricValues key to look up.
    pub key: &'static str,
    /// Whether smaller / larger values are better.
    pub direction: Direction,
    /// Which cells to consider when finding best/worst.
    pub scenario_filter: ScenarioFilter,
    /// How to render the value in the summary table.
    pub fmt: SummaryFmt,
}

/// Cell-scenario filter for a [`SummarySpec`]. The summary renderer
/// only considers cells whose scenario matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioFilter {
    /// Any cell with the key — e.g. for derived/cross-cell metrics.
    Any,
    Stable,
    ColdStart,
    StepDelta(i32),
}

/// Value-formatting choice for a [`SummarySpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryFmt {
    /// `12.3%` (multiplied by 100).
    Percentage,
    /// `12m` or `12m 34s`.
    Duration,
    /// `0.034` with 3 significant figures.
    Float3,
    /// `0.04` fires/min — same as Float3 plus `/min` suffix.
    RatePerMin,
}

/// The canonical metric registry. Used by `baseline::run_cell` to
/// drive computation and by `baseline::serialize_*` / `regression::compare_to_baseline`
/// to drive iteration.
///
/// The constants here MUST match `crate::baseline::{QUIET_WINDOW_SECS,
/// SETTLE_BUFFER_SECS, MIN_SETTLED_WINDOW_SECS, STEP_EVENT_AT_SECS,
/// REACT_WINDOW_SECS}` — they're duplicated here to avoid a
/// metrics → baseline import cycle at construction time. If those
/// constants change, update this function.
pub fn registry() -> Vec<Box<dyn Metric>> {
    use crate::baseline::{
        MIN_SETTLED_WINDOW_SECS, QUIET_WINDOW_SECS, REACT_WINDOW_SECS, SETTLE_BUFFER_SECS,
        STEP_EVENT_AT_SECS,
    };
    // Settle-after for the introspection-driven metrics (bias /
    // variance / overshoot): pick something well past Phase 1 ramp
    // (~5 min for 5-OOM cold start) and the post-fire settle buffer.
    // 12 min = 720s leaves the last 18 min of a 30-min trial for the
    // estimator to stabilize before we evaluate its bias/variance.
    const INTROSPECTION_SETTLE_AFTER_SECS: u64 = 12 * 60;

    vec![
        Box::new(ConvergenceTime {
            quiet_window_secs: QUIET_WINDOW_SECS,
        }),
        Box::new(SettledAccuracy),
        Box::new(Jitter {
            quiet_window_secs: QUIET_WINDOW_SECS,
            settle_buffer_secs: SETTLE_BUFFER_SECS,
            min_settled_window_secs: MIN_SETTLED_WINDOW_SECS,
        }),
        Box::new(ReactionTime {
            event_at_secs: STEP_EVENT_AT_SECS,
            react_window_secs: REACT_WINDOW_SECS,
        }),
        Box::new(Bias {
            settle_after_secs: INTROSPECTION_SETTLE_AFTER_SECS,
        }),
        Box::new(Variance {
            settle_after_secs: INTROSPECTION_SETTLE_AFTER_SECS,
        }),
        Box::new(RampTargetOvershoot),
    ]
}

/// Convenience for the comparator and serializer: build a
/// `HashMap<metric_id → &dyn Metric>` from the registry so a lookup by
/// id is O(1).
pub fn registry_by_id() -> HashMap<&'static str, Box<dyn Metric>> {
    registry().into_iter().map(|m| (m.id(), m)).collect()
}

// ============================================================================
// Convergence time — per-trial + distribution helpers
// ============================================================================

/// Per-trial convergence time. See module docs for the precise
/// definition.
pub fn convergence_time_for_trial(trial: &Trial, quiet_window_secs: u64) -> Option<u64> {
    let fires = trial.fires();
    if fires.is_empty() {
        return Some(0);
    }
    for (i, fire) in fires.iter().enumerate() {
        let quiet_end = fire.t_secs.saturating_add(quiet_window_secs);
        if quiet_end > trial.config.duration_secs {
            break;
        }
        let has_subsequent_in_window =
            fires[i + 1..].iter().any(|f2| f2.t_secs <= quiet_end);
        if !has_subsequent_in_window {
            return Some(fire.t_secs);
        }
    }
    None
}

/// Convergence-time distribution across a set of trials. Returns
/// `(rate, distribution)`.
pub fn convergence_time_distribution(
    trials: &[Trial],
    quiet_window_secs: u64,
) -> (f64, Distribution) {
    if trials.is_empty() {
        return (0.0, Distribution::new(vec![]));
    }
    let mut times: Vec<f64> = Vec::with_capacity(trials.len());
    let mut converged = 0usize;
    for trial in trials {
        if let Some(t) = convergence_time_for_trial(trial, quiet_window_secs) {
            converged += 1;
            times.push(t as f64);
        }
    }
    let rate = converged as f64 / trials.len() as f64;
    (rate, Distribution::new(times))
}

// ============================================================================
// Settled accuracy — per-trial + distribution helpers
// ============================================================================

pub fn settled_accuracy_for_trial(trial: &Trial) -> Option<f64> {
    let true_h = trial.true_hashrate_at_end as f64;
    if true_h <= 0.0 {
        return None;
    }
    let final_h = trial.final_hashrate as f64;
    Some((final_h / true_h - 1.0).abs())
}

pub fn settled_accuracy_distribution(trials: &[Trial]) -> Distribution {
    let values: Vec<f64> = trials
        .iter()
        .filter_map(settled_accuracy_for_trial)
        .collect();
    Distribution::new(values)
}

// ============================================================================
// Steady-state jitter — per-trial + distribution helpers
// ============================================================================

pub fn jitter_for_trial(
    trial: &Trial,
    quiet_window_secs: u64,
    settle_buffer_secs: u64,
    min_settled_window_secs: u64,
) -> Option<f64> {
    let convergence_time = convergence_time_for_trial(trial, quiet_window_secs)?;
    let start = convergence_time.saturating_add(settle_buffer_secs);
    let end = trial.config.duration_secs;
    if end < start.saturating_add(min_settled_window_secs) {
        return None;
    }
    let settled_fires = trial
        .ticks
        .iter()
        .filter(|t| t.fired && t.t_secs >= start && t.t_secs <= end)
        .count();
    let window_minutes = (end - start) as f64 / 60.0;
    if window_minutes <= 0.0 {
        return None;
    }
    Some(settled_fires as f64 / window_minutes)
}

pub fn jitter_distribution(
    trials: &[Trial],
    quiet_window_secs: u64,
    settle_buffer_secs: u64,
    min_settled_window_secs: u64,
) -> Distribution {
    let values: Vec<f64> = trials
        .iter()
        .filter_map(|t| {
            jitter_for_trial(
                t,
                quiet_window_secs,
                settle_buffer_secs,
                min_settled_window_secs,
            )
        })
        .collect();
    Distribution::new(values)
}

// ============================================================================
// Reaction time + sensitivity — per-trial + distribution helpers
// ============================================================================

pub fn reaction_time_for_trial(
    trial: &Trial,
    event_at_secs: u64,
    react_window_secs: u64,
) -> Option<u64> {
    let window_end = event_at_secs.saturating_add(react_window_secs);
    let first_post_event_fire = trial
        .ticks
        .iter()
        .find(|t| t.fired && t.t_secs > event_at_secs && t.t_secs <= window_end);
    first_post_event_fire.map(|t| t.t_secs - event_at_secs)
}

pub fn reaction_time_distribution(
    trials: &[Trial],
    event_at_secs: u64,
    react_window_secs: u64,
) -> (f64, Distribution) {
    if trials.is_empty() {
        return (0.0, Distribution::new(vec![]));
    }
    let mut times: Vec<f64> = Vec::with_capacity(trials.len());
    let mut reacted = 0usize;
    for trial in trials {
        if let Some(t) = reaction_time_for_trial(trial, event_at_secs, react_window_secs) {
            reacted += 1;
            times.push(t as f64);
        }
    }
    let rate = reacted as f64 / trials.len() as f64;
    (rate, Distribution::new(times))
}

pub fn reaction_sensitivity(
    trials: &[Trial],
    event_at_secs: u64,
    react_window_secs: u64,
) -> f64 {
    reaction_time_distribution(trials, event_at_secs, react_window_secs).0
}

// ============================================================================
// Metric implementations
// ============================================================================

/// Time from trial start to first quiet window. Emits convergence_rate
/// plus p10/p50/p90/p95/p99 of the converged-trial time distribution.
#[derive(Debug, Clone)]
pub struct ConvergenceTime {
    pub quiet_window_secs: u64,
}

impl Metric for ConvergenceTime {
    fn id(&self) -> &'static str {
        "convergence_time"
    }
    fn category(&self) -> MetricCategory {
        MetricCategory::Behavioral
    }
    fn class(&self) -> MetricClass {
        MetricClass::MustHave
    }
    fn compute(&self, trials: &[Trial], _cell: &Cell) -> MetricValues {
        let (rate, dist) = convergence_time_distribution(trials, self.quiet_window_secs);
        let mut v = MetricValues::new();
        v.set("convergence_rate", Some(rate));
        dist.record_percentile(&mut v, "convergence_p10_secs", 10.0);
        dist.record_percentile(&mut v, "convergence_p50_secs", 50.0);
        dist.record_percentile(&mut v, "convergence_p90_secs", 90.0);
        dist.record_percentile(&mut v, "convergence_p95_secs", 95.0);
        dist.record_percentile(&mut v, "convergence_p99_secs", 99.0);
        v
    }
    fn tolerance_checks(&self, _cell: &Cell) -> Vec<ToleranceCheck> {
        vec![
            ToleranceCheck {
                key: "convergence_rate",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::HigherIsBetter,
                    extra_abs: 0.02, // 2 percentage points beyond CI
                    extra_mul: None,
                },
            },
            ToleranceCheck {
                key: "convergence_p90_secs",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::LowerIsBetter,
                    extra_abs: 60.0, // 1 minute
                    extra_mul: Some(0.10), // 10% beyond CI
                },
            },
        ]
    }
    fn summary_specs(&self) -> Vec<SummarySpec> {
        vec![
            SummarySpec {
                label: "cold-start convergence rate",
                key: "convergence_rate",
                direction: Direction::HigherIsBetter,
                scenario_filter: ScenarioFilter::ColdStart,
                fmt: SummaryFmt::Percentage,
            },
            SummarySpec {
                label: "cold-start p90 time",
                key: "convergence_p90_secs",
                direction: Direction::LowerIsBetter,
                scenario_filter: ScenarioFilter::ColdStart,
                fmt: SummaryFmt::Duration,
            },
        ]
    }
    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String) {
        use crate::baseline::{find_cell, fmt_duration, unique_rates};
        w.push_str("## Convergence time (cold start: 10 GH/s → 1 PH/s)\n\n");
        w.push_str("| share/min | rate | p10 | p50 | p90 | p99 |\n");
        w.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for spm in unique_rates(results) {
            if let Some(r) = find_cell(results, spm, "cold_start_10gh_to_1ph") {
                w.push_str(&format!(
                    "| {} | {:.1}% | {} | {} | {} | {} |\n",
                    spm,
                    r.get("convergence_rate").unwrap_or(0.0) * 100.0,
                    fmt_duration(r.get("convergence_p10_secs")),
                    fmt_duration(r.get("convergence_p50_secs")),
                    fmt_duration(r.get("convergence_p90_secs")),
                    fmt_duration(r.get("convergence_p99_secs")),
                ));
            }
        }
        w.push('\n');
    }
}

/// `|final_hashrate / true_hashrate − 1|` at trial end. Emits p10–p99.
#[derive(Debug, Clone)]
pub struct SettledAccuracy;

impl SettledAccuracy {
    /// Poisson noise floor on `|realized_rate − true_rate| / true_rate`
    /// integrated over a `window_secs`-wide post-settle window at the
    /// given share rate. The relative stddev is `1/√λ̄` with `λ̄ = spm
    /// × window / 60`. Percentile floors map through the standard
    /// half-normal absolute-deviation distribution:
    /// `|X| ~ |Normal(0, σ²)|` has
    ///   `p50 = 0.674σ`, `p90 = 1.645σ`, `p99 = 2.576σ`.
    ///
    /// This is a *lower* bound — the actual algorithm's settled
    /// accuracy can be higher (worse) if the algorithm's estimator
    /// has additional bias, or lower (better) if it integrates across
    /// multiple windows (EWMA, sliding average). Use as
    /// order-of-magnitude reference, not exact prediction.
    pub fn poisson_floor(spm: f32, window_secs: u64, percentile: f64) -> Option<f64> {
        let lambda_bar = (spm as f64) * (window_secs as f64) / 60.0;
        if lambda_bar <= 0.0 {
            return None;
        }
        let sigma = 1.0 / lambda_bar.sqrt();
        // Half-normal percentile quantiles (Φ⁻¹((1+p)/2) for p ∈ [0, 1]):
        // p10 = 0.126σ, p50 = 0.674σ, p90 = 1.645σ, p95 = 1.960σ, p99 = 2.576σ.
        let z = match percentile as u32 {
            10 => 0.126,
            50 => 0.674,
            90 => 1.645,
            95 => 1.960,
            99 => 2.576,
            _ => return None,
        };
        Some(z * sigma)
    }
}

impl Metric for SettledAccuracy {
    fn id(&self) -> &'static str {
        "settled_accuracy"
    }
    fn category(&self) -> MetricCategory {
        MetricCategory::Behavioral
    }
    fn fundamental_limit(&self, cell: &Cell, key: &str) -> Option<f64> {
        if cell.scenario != Scenario::Stable {
            // Floors are meaningful for stable-load measurement only.
            return None;
        }
        let percentile = match key {
            "settled_accuracy_p10" => 10.0,
            "settled_accuracy_p50" => 50.0,
            "settled_accuracy_p90" => 90.0,
            "settled_accuracy_p95" => 95.0,
            "settled_accuracy_p99" => 99.0,
            _ => return None,
        };
        Self::poisson_floor(
            cell.shares_per_minute,
            crate::baseline::MIN_SETTLED_WINDOW_SECS,
            percentile,
        )
    }
    fn class(&self) -> MetricClass {
        MetricClass::MustHave
    }
    fn compute(&self, trials: &[Trial], _cell: &Cell) -> MetricValues {
        let dist = settled_accuracy_distribution(trials);
        let mut v = MetricValues::new();
        dist.record_percentile(&mut v, "settled_accuracy_p10", 10.0);
        dist.record_percentile(&mut v, "settled_accuracy_p50", 50.0);
        dist.record_percentile(&mut v, "settled_accuracy_p90", 90.0);
        dist.record_percentile(&mut v, "settled_accuracy_p95", 95.0);
        dist.record_percentile(&mut v, "settled_accuracy_p99", 99.0);
        v
    }
    fn tolerance_checks(&self, _cell: &Cell) -> Vec<ToleranceCheck> {
        vec![
            ToleranceCheck {
                key: "settled_accuracy_p50",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::LowerIsBetter,
                    extra_abs: 0.05,
                    extra_mul: Some(0.15),
                },
            },
            ToleranceCheck {
                key: "settled_accuracy_p90",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::LowerIsBetter,
                    extra_abs: 0.05,
                    extra_mul: Some(0.15),
                },
            },
        ]
    }
    fn summary_specs(&self) -> Vec<SummarySpec> {
        vec![
            SummarySpec {
                label: "settled accuracy p50 (stable)",
                key: "settled_accuracy_p50",
                direction: Direction::LowerIsBetter,
                scenario_filter: ScenarioFilter::Stable,
                fmt: SummaryFmt::Percentage,
            },
            SummarySpec {
                label: "settled accuracy p99 (stable)",
                key: "settled_accuracy_p99",
                direction: Direction::LowerIsBetter,
                scenario_filter: ScenarioFilter::Stable,
                fmt: SummaryFmt::Percentage,
            },
        ]
    }
    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String) {
        use crate::baseline::{find_cell, fmt_pct, unique_rates};
        w.push_str("## Settled accuracy (stable load, post-convergence)\n\n");
        w.push_str(
            "`|final_hashrate / true_hashrate - 1|` at trial end. Smaller is better.\n\n",
        );
        w.push_str("| share/min | p10 | p50 | p90 | p99 |\n");
        w.push_str("| --- | --- | --- | --- | --- |\n");
        for spm in unique_rates(results) {
            if let Some(r) = find_cell(results, spm, "stable_1ph") {
                w.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    spm,
                    fmt_pct(r.get("settled_accuracy_p10")),
                    fmt_pct(r.get("settled_accuracy_p50")),
                    fmt_pct(r.get("settled_accuracy_p90")),
                    fmt_pct(r.get("settled_accuracy_p99")),
                ));
            }
        }
        w.push('\n');
    }
}

/// Post-convergence fires per minute. Emits p50/p90/p95/p99 and mean.
#[derive(Debug, Clone)]
pub struct Jitter {
    pub quiet_window_secs: u64,
    pub settle_buffer_secs: u64,
    pub min_settled_window_secs: u64,
}

impl Metric for Jitter {
    fn id(&self) -> &'static str {
        "jitter"
    }
    fn category(&self) -> MetricCategory {
        MetricCategory::Behavioral
    }
    fn class(&self) -> MetricClass {
        MetricClass::MustHave
    }
    fn fundamental_limit(&self, cell: &Cell, _key: &str) -> Option<f64> {
        // A rate-aware boundary (PoissonCI with z = 2.576, 99% CI) has
        // a ~1% per-tick false-fire rate under H₀ on each tail, so
        // ~2% two-sided. At 1 tick / min that's ~0.02 fires/min as the
        // "well-calibrated boundary" floor. Independent of share rate
        // for PoissonCI-style boundaries; share-rate-blind boundaries
        // (Classic) violate this floor at low SPM.
        if cell.scenario == Scenario::Stable {
            Some(0.02)
        } else {
            None
        }
    }
    fn compute(&self, trials: &[Trial], _cell: &Cell) -> MetricValues {
        let dist = jitter_distribution(
            trials,
            self.quiet_window_secs,
            self.settle_buffer_secs,
            self.min_settled_window_secs,
        );
        let mut v = MetricValues::new();
        dist.record_percentile(&mut v, "jitter_p50_per_min", 50.0);
        dist.record_percentile(&mut v, "jitter_p90_per_min", 90.0);
        dist.record_percentile(&mut v, "jitter_p95_per_min", 95.0);
        dist.record_percentile(&mut v, "jitter_p99_per_min", 99.0);
        v.set("jitter_mean_per_min", dist.mean());
        v
    }
    fn tolerance_checks(&self, _cell: &Cell) -> Vec<ToleranceCheck> {
        vec![
            ToleranceCheck {
                key: "jitter_p50_per_min",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::LowerIsBetter,
                    extra_abs: 0.02,
                    extra_mul: None,
                },
            },
            ToleranceCheck {
                key: "jitter_p95_per_min",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::LowerIsBetter,
                    extra_abs: 0.05,
                    extra_mul: Some(0.25),
                },
            },
        ]
    }
    fn summary_specs(&self) -> Vec<SummarySpec> {
        vec![SummarySpec {
            label: "stable-load jitter (mean)",
            key: "jitter_mean_per_min",
            direction: Direction::LowerIsBetter,
            scenario_filter: ScenarioFilter::Stable,
            fmt: SummaryFmt::RatePerMin,
        }]
    }
    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String) {
        use crate::baseline::{find_cell, fmt_f, unique_rates};
        w.push_str("## Steady-state jitter (fires per minute)\n\n");
        w.push_str(
            "Post-convergence rate of vardiff fires. Smaller is better — \
             ideal is zero under stable load.\n\n",
        );
        w.push_str("| share/min | p50 | p90 | p99 | mean |\n");
        w.push_str("| --- | --- | --- | --- | --- |\n");
        for spm in unique_rates(results) {
            if let Some(r) = find_cell(results, spm, "stable_1ph") {
                w.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    spm,
                    fmt_f(r.get("jitter_p50_per_min")),
                    fmt_f(r.get("jitter_p90_per_min")),
                    fmt_f(r.get("jitter_p99_per_min")),
                    fmt_f(r.get("jitter_mean_per_min")),
                ));
            }
        }
        w.push('\n');
    }
}

/// First fire after a scheduled step change. Emits the reaction rate
/// (a.k.a. sensitivity) plus the p10/p50/p90/p99 of reaction time
/// across reacting trials.
///
/// Applies only to `Step` scenarios. The tolerance for `reaction_rate`
/// is split by |Δ| magnitude:
///
/// - `|Δ| ≥ 50%`: rate must be at least `baseline − 0.02` (algorithm
///   must detect genuine large changes).
/// - `|Δ| ≤ 5%`: rate must be at most `baseline + 0.05` (algorithm
///   must not fire on noise).
/// - mid-range (10–25%): no assertion. Legitimate algorithmic
///   trade-offs live here; reviewers judge by inspecting the curve.
///
/// `reaction_p50_secs` is always asserted at `baseline × 1.20`.
#[derive(Debug, Clone)]
pub struct ReactionTime {
    pub event_at_secs: u64,
    pub react_window_secs: u64,
}

impl Metric for ReactionTime {
    fn id(&self) -> &'static str {
        "reaction_time"
    }
    fn category(&self) -> MetricCategory {
        MetricCategory::Behavioral
    }
    fn class(&self) -> MetricClass {
        MetricClass::MustHave
    }
    fn applies_to(&self, cell: &Cell) -> bool {
        matches!(cell.scenario, Scenario::Step { .. })
    }
    fn compute(&self, trials: &[Trial], _cell: &Cell) -> MetricValues {
        let (rate, dist) =
            reaction_time_distribution(trials, self.event_at_secs, self.react_window_secs);
        let mut v = MetricValues::new();
        v.set("reaction_rate", Some(rate));
        dist.record_percentile(&mut v, "reaction_p10_secs", 10.0);
        dist.record_percentile(&mut v, "reaction_p50_secs", 50.0);
        dist.record_percentile(&mut v, "reaction_p90_secs", 90.0);
        dist.record_percentile(&mut v, "reaction_p99_secs", 99.0);
        v
    }
    fn tolerance_checks(&self, cell: &Cell) -> Vec<ToleranceCheck> {
        let mut checks = vec![];

        // Δ-magnitude-conditioned tolerance on reaction_rate:
        // - |Δ| ≥ 50%: algorithm must detect (HigherIsBetter, fails if
        //   reaction rate drops outside the baseline CI).
        // - |Δ| ≤ 5%: algorithm must not fire on noise (LowerIsBetter,
        //   fails if reaction rate climbs outside the baseline CI).
        // - mid-range (10–25%): no assertion; trade-off region.
        let delta_mag = match cell.scenario {
            Scenario::Step { delta_pct } => Some(delta_pct.unsigned_abs()),
            _ => None,
        };
        match delta_mag {
            Some(d) if d >= 50 => checks.push(ToleranceCheck {
                key: "reaction_rate",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::HigherIsBetter,
                    extra_abs: 0.02,
                    extra_mul: None,
                },
            }),
            Some(d) if d <= 5 => checks.push(ToleranceCheck {
                key: "reaction_rate",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::LowerIsBetter,
                    extra_abs: 0.05,
                    extra_mul: None,
                },
            }),
            _ => {}
        }

        checks.push(ToleranceCheck {
            key: "reaction_p50_secs",
            tolerance: Tolerance::WithinCi {
                direction: Direction::LowerIsBetter,
                extra_abs: 60.0,
                extra_mul: Some(0.20),
            },
        });
        checks
    }
    fn summary_specs(&self) -> Vec<SummarySpec> {
        vec![
            SummarySpec {
                label: "reaction rate at −50% step",
                key: "reaction_rate",
                direction: Direction::HigherIsBetter,
                scenario_filter: ScenarioFilter::StepDelta(-50),
                fmt: SummaryFmt::Percentage,
            },
            SummarySpec {
                label: "reaction rate at +50% step",
                key: "reaction_rate",
                direction: Direction::HigherIsBetter,
                scenario_filter: ScenarioFilter::StepDelta(50),
                fmt: SummaryFmt::Percentage,
            },
        ]
    }
    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String) {
        use crate::baseline::{find_cell, fmt_duration, unique_rates};
        let rates = unique_rates(results);

        // ---- Reaction time to a 50% drop ----
        w.push_str("## Reaction time to a 50% drop (step at 15 min)\n\n");
        w.push_str("| share/min | reacted | p10 | p50 | p90 | p99 |\n");
        w.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for spm in &rates {
            if let Some(r) = find_cell(results, *spm, "step_minus_50_at_15min") {
                w.push_str(&format!(
                    "| {} | {:.1}% | {} | {} | {} | {} |\n",
                    spm,
                    r.get("reaction_rate").unwrap_or(0.0) * 100.0,
                    fmt_duration(r.get("reaction_p10_secs")),
                    fmt_duration(r.get("reaction_p50_secs")),
                    fmt_duration(r.get("reaction_p90_secs")),
                    fmt_duration(r.get("reaction_p99_secs")),
                ));
            }
        }
        w.push('\n');

        // ---- Reaction sensitivity curve (all step deltas × rates) ----
        w.push_str("## Reaction sensitivity (P[fire within 5 min of step change])\n\n");
        w.push_str("| Δ% |");
        for spm in &rates {
            w.push_str(&format!(" {} |", spm));
        }
        w.push_str("\n| ---");
        for _ in &rates {
            w.push_str(" | ---");
        }
        w.push_str(" |\n");

        let deltas: [i32; 8] = [-50, -25, -10, -5, 5, 10, 25, 50];
        for delta in &deltas {
            let scenario_key = if *delta >= 0 {
                format!("step_plus_{}_at_15min", delta.unsigned_abs())
            } else {
                format!("step_minus_{}_at_15min", delta.unsigned_abs())
            };
            let sign = if *delta >= 0 { "+" } else { "" };
            w.push_str(&format!("| {}{}% |", sign, delta));
            for spm in &rates {
                let cell_value = find_cell(results, *spm, &scenario_key)
                    .and_then(|r| r.get("reaction_rate"))
                    .map(|r| format!(" {:.2} |", r))
                    .unwrap_or_else(|| " — |".to_string());
                w.push_str(&cell_value);
            }
            w.push('\n');
        }
        w.push('\n');
    }
}

/// Estimator bias: `E[H̃ − H_true] / H_true` averaged over post-settle
/// ticks. Positive = systematic over-estimate; negative = under-estimate.
///
/// Requires per-tick `h_estimate` (populated by `run_trial_observed`
/// against `Observable` algorithms). For non-observable algorithms
/// (`VardiffState` through `AsObservable`), `h_estimate` is `None` and
/// this metric emits `None` values which the TOML serializer omits.
///
/// Applies to `Stable` and `ColdStart` scenarios — true hashrate is
/// constant in both. For `Step` scenarios bias-vs-which-true is
/// ambiguous, so the metric doesn't apply.
#[derive(Debug, Clone)]
pub struct Bias {
    pub settle_after_secs: u64,
}

impl Bias {
    fn render_section(&self, w: &mut String, results: &[crate::baseline::CellResult], scenario_label: &str, scenario_key: &str) {
        use crate::baseline::{find_cell, fmt_pct, unique_rates};
        let rates = unique_rates(results);
        let mut wrote_header = false;
        for spm in &rates {
            let Some(r) = find_cell(results, *spm, scenario_key) else { continue; };
            // Skip cells that didn't emit bias at all (non-observable
            // algorithms; bias_mean key missing entirely).
            if r.metrics.get("bias").is_none() { continue; }
            if !wrote_header {
                w.push_str(&format!("### Bias — {}\n\n", scenario_label));
                w.push_str("| share/min | mean | p10 | p50 | p90 |\n");
                w.push_str("| --- | --- | --- | --- | --- |\n");
                wrote_header = true;
            }
            w.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                spm,
                fmt_pct(r.get("bias_mean")),
                fmt_pct(r.get("bias_p10")),
                fmt_pct(r.get("bias_p50")),
                fmt_pct(r.get("bias_p90")),
            ));
        }
        if wrote_header {
            w.push('\n');
        }
    }
}

impl Metric for Bias {
    fn id(&self) -> &'static str {
        "bias"
    }
    fn category(&self) -> MetricCategory {
        MetricCategory::Estimator
    }
    fn class(&self) -> MetricClass {
        MetricClass::ShouldHave
    }
    fn applies_to(&self, cell: &Cell) -> bool {
        matches!(cell.scenario, Scenario::Stable | Scenario::ColdStart)
    }
    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String) {
        // Bias requires observable algorithms; for non-observable ones
        // (VardiffState through AsObservable) every value is None and
        // the section is omitted entirely.
        let has_any = results.iter().any(|r| {
            r.get("bias_mean").is_some() || r.get("bias_p50").is_some()
        });
        if !has_any {
            return;
        }
        w.push_str("## Estimator bias\n\n");
        w.push_str(
            "`E[H̃ − H_true] / H_true` averaged over post-settle ticks. \
             Positive = systematic over-estimate. Populated only for \
             algorithms that expose introspection (e.g. ClassicComposed, \
             Parametric, EWMA-60s).\n\n",
        );
        self.render_section(w, results, "stable load", "stable_1ph");
        self.render_section(w, results, "cold start", "cold_start_10gh_to_1ph");
    }
    fn compute(&self, trials: &[Trial], _cell: &Cell) -> MetricValues {
        let mut per_trial_means: Vec<f64> = Vec::new();
        for trial in trials {
            let true_h = trial.true_hashrate_at_end as f64;
            if true_h <= 0.0 {
                continue;
            }
            let per_tick_bias: Vec<f64> = trial
                .ticks
                .iter()
                .filter(|t| t.t_secs >= self.settle_after_secs)
                .filter_map(|t| t.h_estimate.map(|h| (h as f64 - true_h) / true_h))
                .collect();
            if !per_tick_bias.is_empty() {
                let mean = per_tick_bias.iter().sum::<f64>() / per_tick_bias.len() as f64;
                per_trial_means.push(mean);
            }
        }
        let dist = Distribution::new(per_trial_means.clone());
        let overall_mean = if per_trial_means.is_empty() {
            None
        } else {
            Some(per_trial_means.iter().sum::<f64>() / per_trial_means.len() as f64)
        };
        let mut v = MetricValues::new();
        v.set("bias_mean", overall_mean);
        dist.record_percentile(&mut v, "bias_p10", 10.0);
        dist.record_percentile(&mut v, "bias_p50", 50.0);
        dist.record_percentile(&mut v, "bias_p90", 90.0);
        v
    }
}

/// Estimator variance: variance of `H̃ / H_true` over post-settle
/// ticks (relative variance, dimensionless). Same applicability
/// constraints as [`Bias`].
#[derive(Debug, Clone)]
pub struct Variance {
    pub settle_after_secs: u64,
}

impl Variance {
    fn render_section(&self, w: &mut String, results: &[crate::baseline::CellResult], scenario_label: &str, scenario_key: &str) {
        use crate::baseline::{find_cell, fmt_f, unique_rates};
        let rates = unique_rates(results);
        let mut wrote_header = false;
        for spm in &rates {
            let Some(r) = find_cell(results, *spm, scenario_key) else { continue; };
            if r.metrics.get("variance").is_none() { continue; }
            if !wrote_header {
                w.push_str(&format!("### Variance — {}\n\n", scenario_label));
                w.push_str("| share/min | mean | p10 | p50 | p90 |\n");
                w.push_str("| --- | --- | --- | --- | --- |\n");
                wrote_header = true;
            }
            w.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                spm,
                fmt_f(r.get("variance_mean")),
                fmt_f(r.get("variance_p10")),
                fmt_f(r.get("variance_p50")),
                fmt_f(r.get("variance_p90")),
            ));
        }
        if wrote_header {
            w.push('\n');
        }
    }
}

impl Metric for Variance {
    fn id(&self) -> &'static str {
        "variance"
    }
    fn category(&self) -> MetricCategory {
        MetricCategory::Estimator
    }
    fn class(&self) -> MetricClass {
        MetricClass::ShouldHave
    }
    fn applies_to(&self, cell: &Cell) -> bool {
        matches!(cell.scenario, Scenario::Stable | Scenario::ColdStart)
    }
    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String) {
        let has_any = results.iter().any(|r| r.get("variance_mean").is_some());
        if !has_any {
            return;
        }
        w.push_str("## Estimator variance\n\n");
        w.push_str(
            "Population variance of `H̃ / H_true` over post-settle ticks \
             (dimensionless). Populated only for algorithms that expose \
             introspection.\n\n",
        );
        self.render_section(w, results, "stable load", "stable_1ph");
        self.render_section(w, results, "cold start", "cold_start_10gh_to_1ph");
    }
    fn compute(&self, trials: &[Trial], _cell: &Cell) -> MetricValues {
        let mut per_trial_variances: Vec<f64> = Vec::new();
        for trial in trials {
            let true_h = trial.true_hashrate_at_end as f64;
            if true_h <= 0.0 {
                continue;
            }
            let normalized: Vec<f64> = trial
                .ticks
                .iter()
                .filter(|t| t.t_secs >= self.settle_after_secs)
                .filter_map(|t| t.h_estimate.map(|h| h as f64 / true_h))
                .collect();
            if normalized.len() < 2 {
                continue;
            }
            let mean = normalized.iter().sum::<f64>() / normalized.len() as f64;
            // Population variance: sum((x - mean)²) / N.
            let variance = normalized
                .iter()
                .map(|x| (x - mean).powi(2))
                .sum::<f64>()
                / normalized.len() as f64;
            per_trial_variances.push(variance);
        }
        let dist = Distribution::new(per_trial_variances.clone());
        let overall_mean = if per_trial_variances.is_empty() {
            None
        } else {
            Some(per_trial_variances.iter().sum::<f64>() / per_trial_variances.len() as f64)
        };
        let mut v = MetricValues::new();
        v.set("variance_mean", overall_mean);
        dist.record_percentile(&mut v, "variance_p10", 10.0);
        dist.record_percentile(&mut v, "variance_p50", 50.0);
        dist.record_percentile(&mut v, "variance_p90", 90.0);
        v
    }
}

/// Ramp target overshoot: `max(new_hashrate on fires) / H_true − 1`,
/// clamped at 0. Measures how far the algorithm's actual *target*
/// (not its internal belief) climbs above truth during the cold-start
/// ramp.
///
/// ## Reading the metric
///
/// `new_hashrate` is the value the algorithm actually sets as its
/// next target on a fire. Only fire-ticks contribute (non-fire ticks
/// have `new_hashrate = None`, filtered out). The clamp logic in
/// `FullRetargetWithClamp` bounds inflated `h_estimate` values into
/// the bounded ramp (3× max per fire), so `new_hashrate` reflects the
/// algorithm's *actual* target trajectory rather than estimator-noise
/// tails.
///
/// ## Works for any algorithm
///
/// This metric does NOT require introspection (`Observable`).
/// `VardiffState` populates `new_hashrate` on fires through the
/// standard `Vardiff::try_vardiff` return value, so this metric works
/// end-to-end for the production algorithm too.
///
/// ## Applicability
///
/// Cold-start only. For `Stable` the algorithm starts at truth and
/// doesn't ramp; for `Step` the relevant post-step transient is
/// captured by `reaction_*` metrics.
#[derive(Debug, Clone, Default)]
pub struct RampTargetOvershoot;

impl Metric for RampTargetOvershoot {
    fn id(&self) -> &'static str {
        "ramp_target_overshoot"
    }
    fn category(&self) -> MetricCategory {
        MetricCategory::Robustness
    }
    fn class(&self) -> MetricClass {
        MetricClass::ShouldHave
    }
    fn applies_to(&self, cell: &Cell) -> bool {
        matches!(cell.scenario, Scenario::ColdStart)
    }
    fn fundamental_limit(&self, cell: &Cell, key: &str) -> Option<f64> {
        // Floor: an algorithm that fires from a single-minute window
        // at the moment current_h ≈ truth will see Poisson(λ̄)
        // observations with relative stddev 1/√λ̄ where λ̄ = spm × 1
        // minute = spm. The p99 of the relative *upper* deviation is
        // z₉₉ × 1/√λ̄ with z₉₉ ≈ 2.326. Lower percentiles use the
        // corresponding upper-tail z.
        if cell.scenario != Scenario::ColdStart {
            return None;
        }
        let lambda_bar = cell.shares_per_minute as f64;
        if lambda_bar <= 0.0 {
            return None;
        }
        // Upper-tail standard-normal quantiles Φ⁻¹(p):
        let z = match key {
            "ramp_target_overshoot_p50" => 0.0,    // upper-tail median deviation is 0
            "ramp_target_overshoot_p90" => 1.282,  // Φ⁻¹(0.90)
            "ramp_target_overshoot_p99" => 2.326,  // Φ⁻¹(0.99)
            _ => return None,
        };
        Some(z / lambda_bar.sqrt())
    }
    fn compute(&self, trials: &[Trial], _cell: &Cell) -> MetricValues {
        let mut per_trial: Vec<f64> = Vec::new();
        for trial in trials {
            let true_h = trial.true_hashrate_at_end as f64;
            if true_h <= 0.0 {
                continue;
            }
            // Peak of new_hashrate over fire ticks. filter_map drops
            // non-fire ticks automatically (their new_hashrate is None).
            let peak = trial
                .ticks
                .iter()
                .filter_map(|t| t.new_hashrate.map(|h| h as f64))
                .fold(f64::NEG_INFINITY, f64::max);
            if peak.is_finite() && peak > 0.0 {
                per_trial.push(((peak / true_h) - 1.0).max(0.0));
            }
        }
        let dist = Distribution::new(per_trial);
        let mut v = MetricValues::new();
        dist.record_percentile(&mut v, "ramp_target_overshoot_p50", 50.0);
        dist.record_percentile(&mut v, "ramp_target_overshoot_p90", 90.0);
        dist.record_percentile(&mut v, "ramp_target_overshoot_p99", 99.0);
        v
    }
    fn tolerance_checks(&self, _cell: &Cell) -> Vec<ToleranceCheck> {
        vec![
            ToleranceCheck {
                key: "ramp_target_overshoot_p50",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::LowerIsBetter,
                    extra_abs: 0.05,
                    extra_mul: Some(0.20),
                },
            },
            ToleranceCheck {
                key: "ramp_target_overshoot_p99",
                tolerance: Tolerance::WithinCi {
                    direction: Direction::LowerIsBetter,
                    extra_abs: 0.10,
                    extra_mul: Some(0.20),
                },
            },
        ]
    }
    fn summary_specs(&self) -> Vec<SummarySpec> {
        vec![SummarySpec {
            label: "ramp target overshoot p99 (cold start)",
            key: "ramp_target_overshoot_p99",
            direction: Direction::LowerIsBetter,
            scenario_filter: ScenarioFilter::ColdStart,
            fmt: SummaryFmt::Percentage,
        }]
    }
    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String) {
        use crate::baseline::{find_cell, fmt_pct, unique_rates};
        let has_any = results
            .iter()
            .any(|r| r.get("ramp_target_overshoot_p50").is_some());
        if !has_any {
            return;
        }
        w.push_str("## Ramp target overshoot (cold start)\n\n");
        w.push_str(
            "`max(new_hashrate on fires) / H_true − 1`: how far the \
             algorithm's actual *target* (not its belief) climbs past \
             truth during the cold-start ramp. Reads `new_hashrate` \
             (the actual target the algorithm set), so it's unaffected \
             by estimator-belief noise. Works for any algorithm that \
             fires, including those without introspection.\n\n",
        );
        w.push_str("| share/min | p50 | p90 | p99 |\n");
        w.push_str("| --- | --- | --- | --- |\n");
        for spm in unique_rates(results) {
            if let Some(r) = find_cell(results, spm, "cold_start_10gh_to_1ph") {
                if r.metrics.get("ramp_target_overshoot").is_none() {
                    continue;
                }
                w.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    spm,
                    fmt_pct(r.get("ramp_target_overshoot_p50")),
                    fmt_pct(r.get("ramp_target_overshoot_p90")),
                    fmt_pct(r.get("ramp_target_overshoot_p99")),
                ));
            }
        }
        w.push('\n');
    }
}

// ============================================================================
// Bootstrap CI helper
// ============================================================================

/// Bootstrap confidence interval for a percentile. Resamples `values`
/// with replacement `n_resamples` times, computes the percentile for
/// each resample, and returns the (2.5%, 97.5%) bounds — a 95% CI
/// using the percentile method.
///
/// Returns `(None, None)` if `values` is empty.
///
/// Uses the framework's deterministic `XorShift64` RNG so CIs are
/// reproducible across runs given the same seed. Exposed as a free
/// function; a future refactor could integrate it into the [`Metric`]
/// trait so every percentile metric automatically
/// emits its CI bounds alongside.
///
/// **Host-width assumption.** The index sample
/// `(rng.next_u64() as usize) % n` truncates to `u32` on 32-bit
/// targets, which biases the sample distribution toward lower indices
/// for `n > 2^32`. In practice `values.len()` is the trial count
/// (typically 100–10000), so the truncation is harmless on every
/// host. If the sim crate is ever ported to a 32-bit target with
/// `values.len() > 2^32` (unlikely), the modulo logic needs widening.
pub fn bootstrap_percentile_ci(
    values: &[f64],
    percentile: f64,
    n_resamples: usize,
    rng: &mut crate::rng::XorShift64,
) -> (Option<f64>, Option<f64>) {
    if values.is_empty() {
        return (None, None);
    }
    let n = values.len();
    let mut bootstrap_samples: Vec<f64> = Vec::with_capacity(n_resamples);
    for _ in 0..n_resamples {
        let mut resample: Vec<f64> = Vec::with_capacity(n);
        for _ in 0..n {
            // 64-bit-host assumption: `as usize` is a no-op on 64-bit;
            // on 32-bit it truncates the high bits, which is fine for
            // `n` up to 2^32. See module docstring above.
            let idx = (rng.next_u64() as usize) % n;
            resample.push(values[idx]);
        }
        let d = Distribution::new(resample);
        if let Some(p) = d.percentile(percentile) {
            bootstrap_samples.push(p);
        }
    }
    let dist = Distribution::new(bootstrap_samples);
    (dist.percentile(2.5), dist.percentile(97.5))
}

// ============================================================================
// Derived-metric constants
// ============================================================================

/// The default upper bound on jitter (fires/min) above which the
/// algorithm is considered "noisy". Used by [`DecouplingScore`] to
/// normalize the jitter term.
pub const DEFAULT_JITTER_CEILING_PER_MIN: f64 = 0.5;

// ============================================================================
// Derived metrics: cross-cell aggregations
// ============================================================================

/// A derived metric is computed *across* cells of the grid rather than
/// from raw trials of a single cell. Each derived metric emits one
/// `MetricValues` per share rate; the serializer renders them in a
/// `[derived.<id>.spm_<X>]` TOML section, and the regression
/// comparator iterates the derived registry alongside the per-cell
/// registry.
pub trait DerivedMetric: Send + Sync + Debug {
    /// Stable identifier (used as the section name in TOML).
    fn id(&self) -> &'static str;

    /// `MustHave` / `ShouldHave` / `ReportOnly`.
    fn class(&self) -> MetricClass;

    /// Computes the metric across all cells in the result set and
    /// returns one `MetricValues` per share rate, ordered ascending
    /// by share rate. Share rates for which the derived inputs are
    /// missing are simply absent from the returned vector.
    fn compute(&self, results: &[crate::baseline::CellResult]) -> Vec<(f32, MetricValues)>;

    /// Tolerance checks applied at one share rate during regression.
    fn tolerance_checks(&self, spm: f32) -> Vec<ToleranceCheck>;

    /// Render the metric's section into the Markdown report.
    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String);

    /// Headline summary specs for the TL;DR section. Default empty.
    /// Derived metrics that surface a headline override.
    fn summary_specs(&self) -> Vec<SummarySpec> {
        Vec::new()
    }
}

/// The canonical derived-metric registry: decoupling score plus
/// reaction-asymmetry. The serializer and comparator iterate this in
/// order.
pub fn derived_registry() -> Vec<Box<dyn DerivedMetric>> {
    vec![Box::new(DecouplingScore), Box::new(ReactionAsymmetry)]
}

// ---- DecouplingScore -------------------------------------------------------

/// Per-share-rate summary of the variance-vs-detection trade-off:
///
/// ```text
/// score = reaction_rate(Step −50%) × clamp(1 − jitter_p50 / J_max, 0, 1)
/// ```
///
/// Higher is better. `1.0` indicates perfect decoupling (algorithm
/// reacts at large changes and has zero jitter under stable load).
/// Lower scores mean the trade-off is biting at that share rate.
#[derive(Debug, Clone, Default)]
pub struct DecouplingScore;

impl DerivedMetric for DecouplingScore {
    fn id(&self) -> &'static str {
        "decoupling_score"
    }

    fn class(&self) -> MetricClass {
        MetricClass::MustHave
    }

    fn compute(&self, results: &[crate::baseline::CellResult]) -> Vec<(f32, MetricValues)> {
        // Group inputs by share rate. The key is `shares_per_minute
        // as u32`, fine for integer-valued rates (canonical
        // 6/12/30/60/120). Fractional rates would collide.
        let mut by_spm: HashMap<u32, (Option<f64>, Option<f64>)> = HashMap::new();
        for r in results {
            let spm_key = r.shares_per_minute as u32;
            let entry = by_spm.entry(spm_key).or_insert((None, None));
            match r.scenario {
                Scenario::Stable => {
                    if let Some(j) = r.get("jitter_p50_per_min") {
                        entry.0 = Some(j);
                    }
                }
                Scenario::Step { delta_pct: -50 } => {
                    if let Some(rate) = r.get("reaction_rate") {
                        entry.1 = Some(rate);
                    }
                }
                _ => {}
            }
        }

        let mut entries: Vec<_> = by_spm.into_iter().collect();
        entries.sort_by_key(|&(spm, _)| spm);

        entries
            .into_iter()
            .filter_map(|(spm, (jitter, react))| match (jitter, react) {
                (Some(j), Some(r)) => {
                    let factor = (1.0 - (j / DEFAULT_JITTER_CEILING_PER_MIN)).clamp(0.0, 1.0);
                    let mut mv = MetricValues::new();
                    mv.set("score", Some(r * factor));
                    Some((spm as f32, mv))
                }
                _ => None,
            })
            .collect()
    }

    fn tolerance_checks(&self, _spm: f32) -> Vec<ToleranceCheck> {
        // Higher is better; flag if the score drops outside the
        // baseline CI by more than 0.05 (5 percentage points).
        vec![ToleranceCheck {
            key: "score",
            tolerance: Tolerance::WithinCi {
                direction: Direction::HigherIsBetter,
                extra_abs: 0.05,
                extra_mul: None,
            },
        }]
    }

    fn summary_specs(&self) -> Vec<SummarySpec> {
        vec![SummarySpec {
            label: "decoupling score",
            key: "score",
            direction: Direction::HigherIsBetter,
            scenario_filter: ScenarioFilter::Any,
            fmt: SummaryFmt::Float3,
        }]
    }

    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String) {
        let scores = self.compute(results);
        if scores.is_empty() {
            return;
        }
        w.push_str("## Decoupling score (per share rate)\n\n");
        w.push_str(&format!(
            "`reaction_rate(Step−50) × clamp(1 − jitter_p50 / J_max, 0, 1)` where \
             `J_max = {:.2}` fires/min. Higher is better. Score of 1.0 = perfect \
             decoupling between reactivity and jitter.\n\n",
            DEFAULT_JITTER_CEILING_PER_MIN
        ));
        w.push_str("| share/min | decoupling score |\n");
        w.push_str("| --- | --- |\n");
        for (spm, mv) in scores {
            let cell = match mv.get("score") {
                Some(s) => format!("{:.3}", s),
                None => "—".to_string(),
            };
            w.push_str(&format!("| {} | {} |\n", spm as u32, cell));
        }
        w.push('\n');
    }
}

// ---- ReactionAsymmetry -----------------------------------------------------

/// Per-share-rate measurement of direction-aware blindness:
///
/// ```text
/// asymmetry_at_δ = reaction_rate(Step +δ%) − reaction_rate(Step −δ%)
/// ```
///
/// for each `δ ∈ {5, 10, 25, 50}` present in the grid. Positive
/// values mean the algorithm reacts faster to up-steps than to
/// down-steps; negative values mean the reverse. Symmetric algorithms
/// score ≈ 0 at every magnitude.
///
/// Catches direction-aware regressions that don't move any single
/// reaction-rate metric outside its individual tolerance: an
/// algorithm that loses 5% on the down-side and gains 5% on the
/// up-side passes per-cell checks but shifts the asymmetry by 10
/// percentage points.
#[derive(Debug, Clone, Default)]
pub struct ReactionAsymmetry;

impl ReactionAsymmetry {
    /// Step magnitudes (absolute value of `delta_pct`) at which we
    /// compute asymmetry, paired with their static-string keys.
    /// Algorithm registry's canonical step grid has both +δ and −δ
    /// for each.
    const MAGNITUDES: &'static [(u32, &'static str)] = &[
        (5, "asymmetry_at_5"),
        (10, "asymmetry_at_10"),
        (25, "asymmetry_at_25"),
        (50, "asymmetry_at_50"),
    ];
}

impl DerivedMetric for ReactionAsymmetry {
    fn id(&self) -> &'static str {
        "reaction_asymmetry"
    }

    fn class(&self) -> MetricClass {
        MetricClass::ShouldHave
    }

    fn compute(&self, results: &[crate::baseline::CellResult]) -> Vec<(f32, MetricValues)> {
        // Build a lookup from (spm, delta_pct) → reaction_rate.
        let mut by_pair: HashMap<(u32, i32), f64> = HashMap::new();
        for r in results {
            if let Scenario::Step { delta_pct } = r.scenario {
                if let Some(rate) = r.get("reaction_rate") {
                    by_pair.insert((r.shares_per_minute as u32, delta_pct), rate);
                }
            }
        }

        // For each share rate, emit asymmetry at each magnitude that
        // has both +δ and −δ cells.
        let mut spms: Vec<u32> = by_pair.keys().map(|(spm, _)| *spm).collect();
        spms.sort_unstable();
        spms.dedup();

        spms.into_iter()
            .filter_map(|spm| {
                let mut mv = MetricValues::new();
                let mut any = false;
                for &(mag, key) in Self::MAGNITUDES {
                    let up = by_pair.get(&(spm, mag as i32));
                    let down = by_pair.get(&(spm, -(mag as i32)));
                    if let (Some(u), Some(d)) = (up, down) {
                        mv.set(key, Some(u - d));
                        any = true;
                    }
                }
                if any {
                    Some((spm as f32, mv))
                } else {
                    None
                }
            })
            .collect()
    }

    fn tolerance_checks(&self, _spm: f32) -> Vec<ToleranceCheck> {
        // Asymmetry is a signed quantity around zero. `Direction::Either`
        // makes the check two-sided: a movement of more than 0.10
        // (10 percentage points) in either direction beyond the
        // baseline CI signals a structural shift in the algorithm's
        // direction-aware response.
        Self::MAGNITUDES
            .iter()
            .map(|&(_, key)| ToleranceCheck {
                key,
                tolerance: Tolerance::WithinCi {
                    direction: Direction::Either,
                    extra_abs: 0.10,
                    extra_mul: None,
                },
            })
            .collect()
    }

    fn render_markdown(&self, results: &[crate::baseline::CellResult], w: &mut String) {
        let computed = self.compute(results);
        if computed.is_empty() {
            return;
        }
        w.push_str("## Reaction asymmetry (per share rate)\n\n");
        w.push_str(
            "`reaction_rate(+δ%) − reaction_rate(−δ%)`. Positive = \
             responds faster to up-steps; negative = responds faster \
             to down-steps; near zero = symmetric.\n\n",
        );
        // Header row.
        w.push_str("| share/min |");
        for &(mag, _) in Self::MAGNITUDES {
            w.push_str(&format!(" δ={}% |", mag));
        }
        w.push('\n');
        w.push_str("| ---");
        for _ in Self::MAGNITUDES {
            w.push_str(" | ---");
        }
        w.push_str(" |\n");
        // Data rows.
        for (spm, mv) in computed {
            w.push_str(&format!("| {} |", spm as u32));
            for &(_, key) in Self::MAGNITUDES {
                let cell = match mv.get(key) {
                    Some(v) => format!(" {:+.2} ", v),
                    None => " — ".to_string(),
                };
                w.push_str(&cell);
                w.push('|');
            }
            w.push('\n');
        }
        w.push('\n');
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::CellResult;
    use crate::trial::{TickRecord, TrialConfig};

    /// Constructs a trial whose ticks each carry an observed
    /// `h_estimate`. Used by the bias / variance / overshoot tests.
    /// `true_h` becomes `true_hashrate_at_end`; one tick per element
    /// at 60-second intervals.
    fn trial_with_observed_ticks(true_h: f32, h_estimates: &[f32]) -> Trial {
        let duration = (h_estimates.len() as u64) * 60;
        let config = TrialConfig {
            duration_secs: duration,
            ..TrialConfig::default()
        };
        let ticks: Vec<TickRecord> = h_estimates
            .iter()
            .enumerate()
            .map(|(i, &h)| TickRecord {
                t_secs: (i as u64 + 1) * 60,
                n_shares: 0,
                fired: false,
                new_hashrate: None,
                current_hashrate_before: true_h,
                delta: None,
                threshold: None,
                h_estimate: Some(h),
            })
            .collect();
        Trial {
            config,
            seed: 0,
            ticks,
            final_hashrate: true_h,
            true_hashrate_at_end: true_h,
        }
    }

    fn cold_start_cell() -> Cell {
        Cell {
            shares_per_minute: 12.0,
            scenario: Scenario::ColdStart,
        }
    }

    fn trial_with_fires(duration_secs: u64, fire_times: &[u64]) -> Trial {
        let config = TrialConfig {
            duration_secs,
            ..TrialConfig::default()
        };
        let ticks = fire_times
            .iter()
            .map(|&t| TickRecord {
                t_secs: t,
                n_shares: 0,
                fired: true,
                new_hashrate: Some(1.0),
                current_hashrate_before: 1.0,
                delta: None,
                threshold: None,
                h_estimate: None,
            })
            .collect();
        Trial {
            config,
            seed: 0,
            ticks,
            final_hashrate: 1.0e15,
            true_hashrate_at_end: 1.0e15,
        }
    }

    fn trial_with_final_and_true(final_h: f32, true_h: f32) -> Trial {
        let config = TrialConfig::default();
        Trial {
            config,
            seed: 0,
            ticks: vec![],
            final_hashrate: final_h,
            true_hashrate_at_end: true_h,
        }
    }

    fn stable_cell() -> Cell {
        Cell {
            shares_per_minute: 12.0,
            scenario: Scenario::Stable,
        }
    }

    fn step_cell(delta_pct: i32) -> Cell {
        Cell {
            shares_per_minute: 12.0,
            scenario: Scenario::Step { delta_pct },
        }
    }

    // ---- Distribution ----

    #[test]
    fn distribution_empty_returns_none_for_stats() {
        let d = Distribution::new(vec![]);
        assert!(d.percentile(50.0).is_none());
        assert!(d.mean().is_none());
        assert_eq!(d.count(), 0);
    }

    #[test]
    fn distribution_percentiles_use_nearest_rank() {
        let values: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let d = Distribution::new(values);
        let p50 = d.p50().unwrap();
        assert!(p50 == 50.0 || p50 == 51.0);
        assert_eq!(d.p99().unwrap(), 99.0);
        assert_eq!(d.p10().unwrap(), 11.0);
    }

    #[test]
    fn distribution_mean_is_arithmetic_mean() {
        let d = Distribution::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(d.mean().unwrap(), 3.0);
    }

    #[test]
    fn distribution_filters_nan() {
        let d = Distribution::new(vec![1.0, f64::NAN, 2.0]);
        assert_eq!(d.count(), 2);
        assert_eq!(d.mean().unwrap(), 1.5);
    }

    // ---- Convergence helper ----

    #[test]
    fn convergence_no_fires_means_converged_at_zero() {
        let t = trial_with_fires(1800, &[]);
        assert_eq!(convergence_time_for_trial(&t, 300), Some(0));
    }

    #[test]
    fn convergence_picks_first_fire_followed_by_quiet_window() {
        let t = trial_with_fires(1800, &[60, 300, 360, 420]);
        assert_eq!(convergence_time_for_trial(&t, 600), Some(420));
    }

    #[test]
    fn convergence_returns_none_when_fires_never_quiet_down() {
        let fires: Vec<u64> = (0..30).map(|i| (i + 1) * 60).collect();
        let t = trial_with_fires(1800, &fires);
        assert_eq!(convergence_time_for_trial(&t, 600), None);
    }

    #[test]
    fn convergence_late_fire_too_close_to_trial_end_is_dnf() {
        let t = trial_with_fires(1800, &[1700]);
        assert_eq!(convergence_time_for_trial(&t, 600), None);
    }

    #[test]
    fn convergence_rate_aggregates_correctly() {
        let trials = vec![
            trial_with_fires(1800, &[]),
            trial_with_fires(1800, &[60, 1100]),
            {
                let fires: Vec<u64> = (0..30).map(|i| (i + 1) * 60).collect();
                trial_with_fires(1800, &fires)
            },
        ];
        let (rate, dist) = convergence_time_distribution(&trials, 600);
        assert!((rate - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(dist.count(), 2);
    }

    // ---- Settled accuracy helper ----

    #[test]
    fn settled_accuracy_perfect_estimate_is_zero() {
        let t = trial_with_final_and_true(1.0e15, 1.0e15);
        assert_eq!(settled_accuracy_for_trial(&t).unwrap(), 0.0);
    }

    #[test]
    fn settled_accuracy_50_percent_over_is_half() {
        let t = trial_with_final_and_true(1.5e15, 1.0e15);
        assert!((settled_accuracy_for_trial(&t).unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn settled_accuracy_50_percent_under_is_half() {
        let t = trial_with_final_and_true(0.5e15, 1.0e15);
        assert!((settled_accuracy_for_trial(&t).unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn settled_accuracy_zero_truth_returns_none() {
        let t = trial_with_final_and_true(1.0e15, 0.0);
        assert!(settled_accuracy_for_trial(&t).is_none());
    }

    // ---- Jitter helper ----

    #[test]
    fn jitter_counts_post_settle_fires_per_minute() {
        let t = trial_with_fires(1800, &[60, 700, 1000, 1500]);
        let j = jitter_for_trial(&t, 300, 120, 600).unwrap();
        assert!((j - 3.0 / 27.0).abs() < 1e-6, "jitter = {j}");
    }

    #[test]
    fn jitter_zero_when_no_settled_fires() {
        let t = trial_with_fires(1800, &[60]);
        let j = jitter_for_trial(&t, 300, 120, 600).unwrap();
        assert_eq!(j, 0.0);
    }

    #[test]
    fn jitter_none_when_trial_did_not_converge() {
        let fires: Vec<u64> = (0..30).map(|i| (i + 1) * 60).collect();
        let t = trial_with_fires(1800, &fires);
        assert!(jitter_for_trial(&t, 600, 120, 600).is_none());
    }

    // ---- Reaction helpers ----

    #[test]
    fn reaction_time_finds_first_post_event_fire_in_window() {
        let t = trial_with_fires(1800, &[60, 500, 950, 1100]);
        assert_eq!(reaction_time_for_trial(&t, 900, 300), Some(50));
    }

    #[test]
    fn reaction_time_none_when_no_fire_in_window() {
        let t = trial_with_fires(1800, &[60, 1100]);
        assert!(reaction_time_for_trial(&t, 900, 120).is_none());
    }

    #[test]
    fn reaction_time_ignores_fires_at_or_before_event() {
        let t = trial_with_fires(1800, &[900, 1500]);
        assert!(reaction_time_for_trial(&t, 900, 300).is_none());
    }

    #[test]
    fn reaction_sensitivity_aggregates_to_fraction() {
        let trials = vec![
            trial_with_fires(1800, &[1000]),
            trial_with_fires(1800, &[1100]),
            trial_with_fires(1800, &[60]),
            trial_with_fires(1800, &[]),
        ];
        let sensitivity = reaction_sensitivity(&trials, 900, 300);
        assert!((sensitivity - 0.5).abs() < 1e-9);
    }

    // ---- Metric trait ----

    #[test]
    fn metric_values_set_and_get_round_trip() {
        let mut v = MetricValues::new();
        v.set("a", Some(1.5));
        v.set("b", None);
        v.set("c", Some(3.0));
        assert_eq!(v.get("a"), Some(1.5));
        assert_eq!(v.get("b"), None);
        assert_eq!(v.get("c"), Some(3.0));
        assert_eq!(v.get("missing"), None);
    }

    #[test]
    fn metric_values_iterate_in_insertion_order() {
        let mut v = MetricValues::new();
        v.set("first", Some(1.0));
        v.set("second", Some(2.0));
        v.set("third", Some(3.0));
        let keys: Vec<&str> = v.iter().map(|(k, _, _)| k).collect();
        assert_eq!(keys, vec!["first", "second", "third"]);
    }

    #[test]
    fn metric_values_set_with_ci_round_trips() {
        let mut v = MetricValues::new();
        v.set_with_ci("p50", Some(0.5), Some(0.45), Some(0.55));
        v.set_with_ci("p99", Some(0.9), None, None); // CI dropped if either side None
        assert_eq!(v.get("p50"), Some(0.5));
        assert_eq!(v.get_ci("p50"), Some((0.45, 0.55)));
        assert_eq!(v.get("p99"), Some(0.9));
        assert_eq!(v.get_ci("p99"), None);
    }

    /// Helper: synthesize a BaselineValue with no CI (collapsed to a
    /// point). Used by tolerance tests to exercise the point-based
    /// fallback path.
    fn bv_point(point: f64) -> BaselineValue {
        BaselineValue {
            point,
            ci_low: None,
            ci_high: None,
        }
    }

    /// Helper: synthesize a BaselineValue with an explicit CI envelope.
    fn bv_ci(point: f64, lo: f64, hi: f64) -> BaselineValue {
        BaselineValue {
            point,
            ci_low: Some(lo),
            ci_high: Some(hi),
        }
    }

    #[test]
    fn tolerance_higher_is_better_fails_when_current_below_ci_minus_slack() {
        let t = Tolerance::WithinCi {
            direction: Direction::HigherIsBetter,
            extra_abs: 0.01,
            extra_mul: None,
        };
        // CI [0.93, 0.97], extra_abs 0.01 → fails below 0.92.
        assert!(t.apply(bv_ci(0.95, 0.93, 0.97), 0.91).is_some());
        assert!(t.apply(bv_ci(0.95, 0.93, 0.97), 0.92).is_none()); // edge
        assert!(t.apply(bv_ci(0.95, 0.93, 0.97), 1.0).is_none());
    }

    #[test]
    fn tolerance_lower_is_better_fails_when_current_above_ci_plus_slack() {
        let t = Tolerance::WithinCi {
            direction: Direction::LowerIsBetter,
            extra_abs: 0.02,
            extra_mul: None,
        };
        // CI [0.03, 0.05], extra_abs 0.02 → fails above 0.07.
        assert!(t.apply(bv_ci(0.04, 0.03, 0.05), 0.08).is_some());
        assert!(t.apply(bv_ci(0.04, 0.03, 0.05), 0.07).is_none()); // edge
        assert!(t.apply(bv_ci(0.04, 0.03, 0.05), 0.03).is_none());
    }

    #[test]
    fn tolerance_with_no_ci_degrades_to_point_check() {
        // Without CI, the envelope collapses to the point. The check
        // is then "current within point ± extra_abs (in regressing
        // direction)".
        let t = Tolerance::WithinCi {
            direction: Direction::LowerIsBetter,
            extra_abs: 0.02,
            extra_mul: None,
        };
        assert!(t.apply(bv_point(0.04), 0.07).is_some());
        assert!(t.apply(bv_point(0.04), 0.06).is_none()); // edge
        assert!(t.apply(bv_point(0.04), 0.03).is_none());
    }

    #[test]
    fn tolerance_extra_mul_scales_with_baseline() {
        let t = Tolerance::WithinCi {
            direction: Direction::LowerIsBetter,
            extra_abs: 0.0,
            extra_mul: Some(0.10),
        };
        // No CI; point = 100; mul = 0.10 → fails above 110.
        assert!(t.apply(bv_point(100.0), 111.0).is_some());
        assert!(t.apply(bv_point(100.0), 110.0).is_none()); // edge
        assert!(t.apply(bv_point(100.0), 50.0).is_none());
    }

    #[test]
    fn tolerance_either_direction_is_two_sided() {
        let t = Tolerance::WithinCi {
            direction: Direction::Either,
            extra_abs: 0.05,
            extra_mul: None,
        };
        // Baseline 0.00 with no CI; symmetric ±0.05 budget.
        assert!(t.apply(bv_point(0.0), 0.06).is_some());
        assert!(t.apply(bv_point(0.0), -0.06).is_some());
        assert!(t.apply(bv_point(0.0), 0.04).is_none());
        assert!(t.apply(bv_point(0.0), -0.04).is_none());
    }

    #[test]
    fn tolerance_baseline_zero_with_only_extra_mul_still_caught_by_extra_abs() {
        // baseline=0, extra_mul=0.25, extra_abs=0.01 → max slack is
        // max(0×0.25=0, 0.01) = 0.01. So current=0.02 fails.
        let t = Tolerance::WithinCi {
            direction: Direction::LowerIsBetter,
            extra_abs: 0.01,
            extra_mul: Some(0.25),
        };
        assert!(t.apply(bv_point(0.0), 0.02).is_some());
        assert!(t.apply(bv_point(0.0), 0.005).is_none());
    }

    #[test]
    fn tolerance_report_only_never_fails() {
        let t = Tolerance::ReportOnly;
        assert!(t.apply(bv_point(1.0), 1000.0).is_none());
    }

    #[test]
    fn registry_returns_metrics_in_canonical_order() {
        let r = registry();
        let ids: Vec<&str> = r.iter().map(|m| m.id()).collect();
        assert_eq!(
            ids,
            vec![
                "convergence_time",
                "settled_accuracy",
                "jitter",
                "reaction_time",
                "bias",
                "variance",
                "ramp_target_overshoot",
            ]
        );
    }

    #[test]
    fn derived_registry_returns_metrics_in_canonical_order() {
        let r = derived_registry();
        let ids: Vec<&str> = r.iter().map(|m| m.id()).collect();
        assert_eq!(ids, vec!["decoupling_score", "reaction_asymmetry"]);
    }

    #[test]
    fn reaction_time_only_applies_to_step_scenarios() {
        let m = ReactionTime {
            event_at_secs: 900,
            react_window_secs: 300,
        };
        assert!(!m.applies_to(&stable_cell()));
        assert!(m.applies_to(&step_cell(-50)));
        assert!(m.applies_to(&step_cell(10)));
    }

    #[test]
    fn reaction_time_tolerance_splits_by_delta_magnitude() {
        let m = ReactionTime {
            event_at_secs: 900,
            react_window_secs: 300,
        };

        // |Δ| ≥ 50 → reaction_rate gets HigherIsBetter check (must detect).
        let checks = m.tolerance_checks(&step_cell(-50));
        assert!(checks.iter().any(|c| {
            c.key == "reaction_rate"
                && matches!(
                    c.tolerance,
                    Tolerance::WithinCi {
                        direction: Direction::HigherIsBetter,
                        ..
                    }
                )
        }));

        // |Δ| ≤ 5 → reaction_rate gets LowerIsBetter check (must not fire on noise).
        let checks = m.tolerance_checks(&step_cell(5));
        assert!(checks.iter().any(|c| {
            c.key == "reaction_rate"
                && matches!(
                    c.tolerance,
                    Tolerance::WithinCi {
                        direction: Direction::LowerIsBetter,
                        ..
                    }
                )
        }));

        // |Δ| = 25 (mid-range) → no rate check.
        let checks = m.tolerance_checks(&step_cell(25));
        assert!(!checks.iter().any(|c| c.key == "reaction_rate"));

        // p50_secs is always present.
        for delta in &[-50, -25, -10, -5, 5, 10, 25, 50] {
            let checks = m.tolerance_checks(&step_cell(*delta));
            assert!(checks.iter().any(|c| c.key == "reaction_p50_secs"));
        }
    }

    #[test]
    fn convergence_metric_compute_round_trips_through_metric_values() {
        let m = ConvergenceTime {
            quiet_window_secs: 600,
        };
        // Trial 1: no fires → converges at 0.
        // Trial 2: single fire at 1100 → converges at 1100 (no subsequent
        // fire, and 1100 + 600 = 1700 ≤ 1800 fits in trial).
        // The earlier `[60, 1100]` pattern *would* converge at 60 because
        // 1100 is past 60's 600s quiet window; that bug is what motivates
        // checking the round-trip explicitly here.
        let trials = vec![
            trial_with_fires(1800, &[]),
            trial_with_fires(1800, &[1100]),
        ];
        let v = m.compute(&trials, &stable_cell());
        assert_eq!(v.get("convergence_rate"), Some(1.0));
        // p50 of converged times {0, 1100}: nearest-rank idx = round(0.5*1) = 1 → 1100.
        assert_eq!(v.get("convergence_p50_secs"), Some(1100.0));
    }

    #[test]
    fn settled_accuracy_metric_emits_all_percentiles() {
        let m = SettledAccuracy;
        let trials = vec![
            trial_with_final_and_true(1.0e15, 1.0e15),
            trial_with_final_and_true(1.5e15, 1.0e15),
            trial_with_final_and_true(0.5e15, 1.0e15),
        ];
        let v = m.compute(&trials, &stable_cell());
        assert!(v.get("settled_accuracy_p50").is_some());
        assert!(v.get("settled_accuracy_p90").is_some());
    }

    // ---- Bias ----

    #[test]
    fn bias_zero_when_h_estimate_matches_true() {
        let trial = trial_with_observed_ticks(1.0e15, &[1.0e15; 20]);
        let m = Bias {
            settle_after_secs: 0,
        };
        let v = m.compute(&[trial], &stable_cell());
        assert_eq!(v.get("bias_mean"), Some(0.0));
        assert_eq!(v.get("bias_p50"), Some(0.0));
    }

    #[test]
    fn bias_positive_when_overestimating() {
        let trial = trial_with_observed_ticks(1.0e15, &[1.1e15; 20]);
        let m = Bias {
            settle_after_secs: 0,
        };
        let v = m.compute(&[trial], &stable_cell());
        let bias = v.get("bias_mean").unwrap();
        // f32 precision in (1.1e15 / 1.0e15 - 1.0) gives ~0.10000000149.
        // Loose tolerance to absorb the f32 rounding.
        assert!((bias - 0.1).abs() < 1e-5, "bias = {}", bias);
    }

    #[test]
    fn bias_none_when_no_observed_ticks() {
        // trial_with_fires builds TickRecords with h_estimate = None;
        // bias has no data and emits None values.
        let trial = trial_with_fires(1800, &[60]);
        let m = Bias {
            settle_after_secs: 0,
        };
        let v = m.compute(&[trial], &stable_cell());
        assert!(v.get("bias_mean").is_none());
        assert!(v.get("bias_p50").is_none());
    }

    #[test]
    fn bias_does_not_apply_to_step_scenarios() {
        let m = Bias {
            settle_after_secs: 0,
        };
        assert!(m.applies_to(&stable_cell()));
        assert!(m.applies_to(&cold_start_cell()));
        assert!(!m.applies_to(&step_cell(-50)));
        assert!(!m.applies_to(&step_cell(25)));
    }

    #[test]
    fn bias_respects_settle_after_window() {
        // 5 ticks: first 3 are below truth (Phase 1 ramp), last 2 are on
        // truth. With settle_after_secs cutting at 200, the first 3 are
        // excluded (they're at t=60, 120, 180 — all < 200) and the last
        // 2 (t=240, 300) contribute bias=0.
        let trial = trial_with_observed_ticks(
            1.0e15,
            &[0.5e15, 0.7e15, 0.9e15, 1.0e15, 1.0e15],
        );
        let m = Bias {
            settle_after_secs: 200,
        };
        let v = m.compute(&[trial], &stable_cell());
        assert_eq!(v.get("bias_mean"), Some(0.0));
    }

    // ---- Variance ----

    #[test]
    fn variance_zero_when_h_estimate_constant() {
        let trial = trial_with_observed_ticks(1.0e15, &[1.0e15; 20]);
        let m = Variance {
            settle_after_secs: 0,
        };
        let v = m.compute(&[trial], &stable_cell());
        assert_eq!(v.get("variance_mean"), Some(0.0));
    }

    #[test]
    fn variance_positive_when_h_estimate_varies() {
        // Alternate 0.9 and 1.1 of truth. Normalized values are 0.9 and 1.1,
        // mean = 1.0, population variance = ((0.9-1)² + (1.1-1)²) / 2 = 0.01.
        let mut estimates = Vec::with_capacity(20);
        for i in 0..20 {
            estimates.push(if i % 2 == 0 { 0.9e15 } else { 1.1e15 });
        }
        let trial = trial_with_observed_ticks(1.0e15, &estimates);
        let m = Variance {
            settle_after_secs: 0,
        };
        let v = m.compute(&[trial], &stable_cell());
        let var = v.get("variance_mean").unwrap();
        // f32 precision again — relax tolerance.
        assert!((var - 0.01).abs() < 1e-4, "variance = {}", var);
    }

    #[test]
    fn variance_none_when_too_few_ticks() {
        let trial = trial_with_observed_ticks(1.0e15, &[1.0e15]); // only 1 tick
        let m = Variance {
            settle_after_secs: 0,
        };
        let v = m.compute(&[trial], &stable_cell());
        assert!(v.get("variance_mean").is_none());
    }

    // ---- Phase 1 overshoot ----

    // ---- Ramp target overshoot ----
    //
    // The complementary metric that uses new_hashrate instead of
    // h_estimate, so it works for VardiffState (non-observable) too
    // and is unaffected by the U256 truncation artifact documented
    // in FINDINGS.md §3.

    /// Construct a trial whose only ticks are fires with specified
    /// `new_hashrate` values. Used by ramp_target_overshoot tests.
    fn trial_with_fire_targets(targets: &[(u64, f32)], true_h: f32) -> Trial {
        let config = TrialConfig {
            duration_secs: 1800,
            ..TrialConfig::default()
        };
        let ticks = targets
            .iter()
            .map(|&(t, h)| TickRecord {
                t_secs: t,
                n_shares: 0,
                fired: true,
                new_hashrate: Some(h),
                current_hashrate_before: 1.0e10,
                delta: None,
                threshold: None,
                h_estimate: None,
            })
            .collect();
        Trial {
            config,
            seed: 0,
            ticks,
            final_hashrate: targets.last().map(|t| t.1).unwrap_or(0.0),
            true_hashrate_at_end: true_h,
        }
    }

    #[test]
    fn ramp_target_overshoot_captures_peak_new_hashrate_above_truth() {
        // Targets that overshoot to 1.5e15 before settling at 1e15.
        // Peak/true = 1.5; overshoot = 0.5 = 50%.
        let trial = trial_with_fire_targets(
            &[(60, 1.0e14), (120, 1.5e15), (180, 1.0e15)],
            1.0e15,
        );
        let m = RampTargetOvershoot;
        let v = m.compute(&[trial], &cold_start_cell());
        let p50 = v.get("ramp_target_overshoot_p50").unwrap();
        assert!((p50 - 0.5).abs() < 1e-5, "p50 = {}", p50);
    }

    #[test]
    fn ramp_target_overshoot_zero_when_targets_below_truth() {
        // All targets below truth — no overshoot.
        let trial = trial_with_fire_targets(
            &[(60, 3.0e14), (120, 7.0e14), (180, 9.0e14)],
            1.0e15,
        );
        let m = RampTargetOvershoot;
        let v = m.compute(&[trial], &cold_start_cell());
        assert_eq!(v.get("ramp_target_overshoot_p50"), Some(0.0));
    }

    #[test]
    fn ramp_target_overshoot_works_without_h_estimate() {
        // The whole point: works for non-observable algorithms.
        // trial_with_fire_targets sets h_estimate = None on every
        // tick; the metric must still produce a value.
        let trial = trial_with_fire_targets(&[(60, 1.5e15)], 1.0e15);
        let m = RampTargetOvershoot;
        let v = m.compute(&[trial], &cold_start_cell());
        assert!(v.get("ramp_target_overshoot_p50").is_some());
        assert!((v.get("ramp_target_overshoot_p50").unwrap() - 0.5).abs() < 1e-5);
    }

    #[test]
    fn ramp_target_overshoot_none_when_no_fires() {
        // Trial with zero fires can't have a target peak.
        let trial = trial_with_fires(1800, &[]);
        let m = RampTargetOvershoot;
        let v = m.compute(&[trial], &cold_start_cell());
        assert!(v.get("ramp_target_overshoot_p50").is_none());
    }

    #[test]
    fn ramp_target_overshoot_only_applies_to_cold_start() {
        let m = RampTargetOvershoot;
        assert!(m.applies_to(&cold_start_cell()));
        assert!(!m.applies_to(&stable_cell()));
        assert!(!m.applies_to(&step_cell(-50)));
        assert!(!m.applies_to(&step_cell(50)));
    }

    #[test]
    fn ramp_target_overshoot_aggregates_across_trials() {
        // Three trials with different peak overshoots: 20%, 50%, 80%.
        let trials = vec![
            trial_with_fire_targets(&[(60, 1.2e15)], 1.0e15),
            trial_with_fire_targets(&[(60, 1.5e15)], 1.0e15),
            trial_with_fire_targets(&[(60, 1.8e15)], 1.0e15),
        ];
        let m = RampTargetOvershoot;
        let v = m.compute(&trials, &cold_start_cell());
        // Sorted ascending: [0.2, 0.5, 0.8]. Nearest-rank percentile
        // indexing: p50 → index round(0.5×2) = 1 → 0.5.
        let p50 = v.get("ramp_target_overshoot_p50").unwrap();
        assert!((p50 - 0.5).abs() < 1e-5);
        // p90 → index round(0.9×2) = 2 → 0.8.
        let p90 = v.get("ramp_target_overshoot_p90").unwrap();
        assert!((p90 - 0.8).abs() < 1e-5);
    }

    // ---- Bootstrap CI ----

    #[test]
    fn bootstrap_ci_returns_none_for_empty() {
        let mut rng = crate::rng::XorShift64::new(0xCAFE);
        let (lo, hi) = bootstrap_percentile_ci(&[], 50.0, 100, &mut rng);
        assert!(lo.is_none());
        assert!(hi.is_none());
    }

    #[test]
    fn bootstrap_ci_brackets_point_estimate_on_clean_data() {
        let values: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let point = Distribution::new(values.clone()).p50().unwrap();
        let mut rng = crate::rng::XorShift64::new(0xBEEF);
        let (lo, hi) = bootstrap_percentile_ci(&values, 50.0, 200, &mut rng);
        let lo = lo.unwrap();
        let hi = hi.unwrap();
        assert!(lo <= point, "lo={} > point={}", lo, point);
        assert!(hi >= point, "hi={} < point={}", hi, point);
        // Reasonable bracket width for 100-sample data with 200 resamples.
        assert!(hi - lo < 30.0, "ci width = {}", hi - lo);
    }

    #[test]
    fn bootstrap_ci_is_deterministic_for_fixed_seed() {
        let values: Vec<f64> = (1..=50).map(|i| i as f64).collect();
        let mut rng_a = crate::rng::XorShift64::new(0xDEAD);
        let mut rng_b = crate::rng::XorShift64::new(0xDEAD);
        let ci_a = bootstrap_percentile_ci(&values, 50.0, 100, &mut rng_a);
        let ci_b = bootstrap_percentile_ci(&values, 50.0, 100, &mut rng_b);
        assert_eq!(ci_a, ci_b);
    }

    // ---- Decoupling score ----

    /// Helper to extract `score` from a (spm, MetricValues) pair.
    fn score_at(scores: &[(f32, MetricValues)], spm: f32) -> Option<f64> {
        scores
            .iter()
            .find(|(s, _)| *s == spm)
            .and_then(|(_, mv)| mv.get("score"))
    }

    #[test]
    fn decoupling_score_combines_stable_jitter_and_step_reaction() {
        let mut stable = CellResult::new(12.0, Scenario::Stable);
        let mut jit = MetricValues::new();
        jit.set("jitter_p50_per_min", Some(0.1));
        stable.metrics.insert("jitter", jit);

        let mut step = CellResult::new(12.0, Scenario::Step { delta_pct: -50 });
        let mut react = MetricValues::new();
        react.set("reaction_rate", Some(0.8));
        step.metrics.insert("reaction_time", react);

        let scores = DecouplingScore.compute(&[stable, step]);
        assert_eq!(scores.len(), 1);
        // reaction_rate (0.8) × clamp(1 − 0.1/0.5, 0, 1) = 0.8 × 0.8 = 0.64
        let score = score_at(&scores, 12.0).unwrap();
        assert!((score - 0.64).abs() < 1e-9, "score = {}", score);
    }

    #[test]
    fn decoupling_score_omits_share_rate_when_one_input_missing() {
        let mut stable = CellResult::new(12.0, Scenario::Stable);
        let mut jit = MetricValues::new();
        jit.set("jitter_p50_per_min", Some(0.1));
        stable.metrics.insert("jitter", jit);
        // No Step cell at this rate.

        let scores = DecouplingScore.compute(&[stable]);
        assert!(scores.is_empty());
    }

    #[test]
    fn decoupling_score_clamps_jitter_above_ceiling_to_zero_factor() {
        // jitter at the ceiling: (1 - 0.5/0.5) = 0, factor clamps to 0,
        // score = reaction_rate * 0 = 0.
        let mut stable = CellResult::new(12.0, Scenario::Stable);
        let mut jit = MetricValues::new();
        jit.set("jitter_p50_per_min", Some(0.5));
        stable.metrics.insert("jitter", jit);

        let mut step = CellResult::new(12.0, Scenario::Step { delta_pct: -50 });
        let mut react = MetricValues::new();
        react.set("reaction_rate", Some(1.0));
        step.metrics.insert("reaction_time", react);

        let scores = DecouplingScore.compute(&[stable, step]);
        assert_eq!(score_at(&scores, 12.0), Some(0.0));
    }

    #[test]
    fn decoupling_scores_sorted_by_share_rate() {
        // Build cells out of order; expect sorted ascending output.
        let mut cells = Vec::new();
        for &spm in &[12.0f32, 60.0, 6.0] {
            let mut s = CellResult::new(spm, Scenario::Stable);
            let mut j = MetricValues::new();
            j.set("jitter_p50_per_min", Some(0.05));
            s.metrics.insert("jitter", j);
            cells.push(s);

            let mut st = CellResult::new(spm, Scenario::Step { delta_pct: -50 });
            let mut r = MetricValues::new();
            r.set("reaction_rate", Some(0.9));
            st.metrics.insert("reaction_time", r);
            cells.push(st);
        }
        let scores = DecouplingScore.compute(&cells);
        assert_eq!(scores.len(), 3);
        assert_eq!(scores[0].0, 6.0);
        assert_eq!(scores[1].0, 12.0);
        assert_eq!(scores[2].0, 60.0);
    }

    // ---- Reaction asymmetry ----

    fn step_cell_with_rate(spm: f32, delta_pct: i32, rate: f64) -> CellResult {
        let mut c = CellResult::new(spm, Scenario::Step { delta_pct });
        let mut mv = MetricValues::new();
        mv.set("reaction_rate", Some(rate));
        c.metrics.insert("reaction_time", mv);
        c
    }

    #[test]
    fn reaction_asymmetry_computes_signed_delta_per_magnitude() {
        let cells = vec![
            step_cell_with_rate(12.0, 50, 0.95),
            step_cell_with_rate(12.0, -50, 0.85),
            step_cell_with_rate(12.0, 25, 0.6),
            step_cell_with_rate(12.0, -25, 0.7),
        ];
        let out = ReactionAsymmetry.compute(&cells);
        assert_eq!(out.len(), 1);
        let (spm, mv) = &out[0];
        assert_eq!(*spm, 12.0);
        let a50 = mv.get("asymmetry_at_50").unwrap();
        let a25 = mv.get("asymmetry_at_25").unwrap();
        assert!((a50 - 0.10).abs() < 1e-9, "asymmetry_at_50 = {}", a50);
        assert!((a25 - (-0.10)).abs() < 1e-9, "asymmetry_at_25 = {}", a25);
    }

    #[test]
    fn reaction_asymmetry_omits_magnitudes_missing_one_side() {
        // Only +50 present; no −50 → no asymmetry_at_50 emitted.
        let cells = vec![step_cell_with_rate(12.0, 50, 0.95)];
        let out = ReactionAsymmetry.compute(&cells);
        assert!(out.is_empty(), "no symmetric pair → no spm row");
    }

    #[test]
    fn reaction_asymmetry_renders_per_magnitude_columns() {
        let cells = vec![
            step_cell_with_rate(12.0, 50, 0.95),
            step_cell_with_rate(12.0, -50, 0.85),
        ];
        let mut out = String::new();
        ReactionAsymmetry.render_markdown(&cells, &mut out);
        assert!(out.contains("Reaction asymmetry"));
        assert!(out.contains("δ=50%"));
        assert!(out.contains("+0.10")); // signed format
    }
}
