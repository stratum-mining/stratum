//! Baseline characterization: parameterized sweep of cells × trials,
//! producing a structured result that can be serialized to TOML
//! (machine-readable, for regression assertions) and Markdown
//! (human-readable, for PR review).
//!
//! A **cell** is one tuple of `(algorithm, share_rate, scenario)`. The
//! default grid is 5 share rates × 10 scenarios = 50 cells. With
//! `N=1000` trials per cell that's 50,000 trials and ~20 seconds of
//! wall clock at release-mode speed.
//!
//! ## Step-3 refactor
//!
//! Metric computation and tolerance policy now live in the [`crate::metrics`]
//! module. This file is responsible for:
//!
//! - Scenario / Cell / Grid definitions
//! - Driving trials per cell
//! - Calling each registry metric on those trials and storing the
//!   results in [`CellResult::metrics`]
//! - Serializing the (cell, metric values) bag to TOML and Markdown
//!
//! All per-metric switch statements have moved out of this file. The
//! serializers do iterate metrics by id (because Markdown wants
//! section-by-section grouping), but they read values through the
//! key-based accessor [`CellResult::get`] rather than through hardcoded
//! struct fields.

use std::collections::HashMap;
use std::sync::Arc;

use channels_sv2::vardiff::MockClock;
use channels_sv2::VardiffState;

use crate::metrics::{self, MetricValues};
use crate::schedule::HashrateSchedule;
use crate::trial::{run_trial, Trial, TrialConfig};

/// Default trial-count per cell. Overridable via the
/// `VARDIFF_BASELINE_TRIALS` environment variable when running
/// `generate-baseline`.
pub const DEFAULT_TRIAL_COUNT: usize = 1000;

/// Default base seed. Overridable via `VARDIFF_BASELINE_SEED`.
pub const DEFAULT_BASELINE_SEED: u64 = 0xDEAD_BEEF_CAFE_F00D;

/// Quiet-window for convergence detection (seconds).
pub const QUIET_WINDOW_SECS: u64 = 300;

/// Settle-buffer for jitter measurement (seconds).
pub const SETTLE_BUFFER_SECS: u64 = 120;

/// Minimum settled-window for jitter to be reported (seconds).
pub const MIN_SETTLED_WINDOW_SECS: u64 = 600;

/// Reaction window for step-change scenarios (seconds).
pub const REACT_WINDOW_SECS: u64 = 300;

/// Step-change scenarios fire their event at this offset from trial
/// start.
pub const STEP_EVENT_AT_SECS: u64 = 15 * 60;

/// Trial duration (seconds).
pub const TRIAL_DURATION_SECS: u64 = 30 * 60;

/// The "true" miner hashrate used by every scenario (1 PH/s). This is
/// held constant across the grid so cells differ only in share rate
/// and scenario shape, not in absolute scale.
pub const TRUE_HASHRATE: f32 = 1.0e15;

/// Default initial estimate for cold-start scenarios (10 GH/s — five
/// orders of magnitude below truth).
pub const COLD_START_INITIAL_HASHRATE: f32 = 1.0e10;

/// A primitive of the scenario DSL. Phases compose into a
/// piecewise-constant (or piecewise-linear, for [`Phase::Ramp`])
/// hashrate trajectory over a trial. The [`Phase::build`] path turns a
/// `Vec<Phase>` into a [`HashrateSchedule`].
///
/// The named scenarios ([`Scenario::ColdStart`], [`Scenario::Stable`],
/// [`Scenario::Step`]) are implemented as predefined phase lists. The
/// trial-level output is byte-identical to a hand-coded `build` arm
/// for each scenario (asserted by `tests::phase_dsl_matches_legacy_*`).
///
/// New scenarios (stall recovery, sustained noise, transient spike,
/// slow ramp) compose the same primitives without requiring a new
/// `Scenario` variant. Use [`Scenario::Custom`] for those.
#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    /// Constant hashrate `h` for `secs` seconds.
    Hold { secs: u64, h: f32 },
    /// Linear ramp from `from` to `to` over `secs` seconds. Approximated
    /// as `RAMP_SEGMENTS`-many piecewise-constant segments inside
    /// `HashrateSchedule`, which is itself piecewise-constant.
    ///
    /// Discretization: each segment is `secs / RAMP_SEGMENTS` long
    /// (integer division). If `secs % RAMP_SEGMENTS != 0` the
    /// remainder is silently truncated and the ramp ends slightly
    /// short of `secs`. Aligning `secs` to a multiple of
    /// `RAMP_SEGMENTS` (e.g., 600s, 1200s) avoids this entirely.
    Ramp { secs: u64, from: f32, to: f32 },
    /// Zero hashrate for `secs` seconds. Tests the algorithm's stall
    /// detection — the share rate goes to zero and the algorithm has
    /// to react via the zero-shares branch.
    Stall { secs: u64 },
}

/// Number of piecewise-constant segments used to approximate a
/// [`Phase::Ramp`]. Larger = smoother but more segments in the
/// schedule. 10 is enough for the tick-interval-aligned scenarios used
/// in characterization.
pub const RAMP_SEGMENTS: u64 = 10;

/// Converts a phase list into a `(TrialConfig, HashrateSchedule)` pair.
/// The total duration is the sum of phase durations. `initial_estimate`
/// becomes `TrialConfig::initial_hashrate`; `None` defaults to
/// `TRUE_HASHRATE` (used by the `Stable` and `Step` named scenarios,
/// which start aligned with truth).
pub fn phases_to_trial(
    phases: &[Phase],
    initial_estimate: Option<f32>,
    shares_per_minute: f32,
) -> (TrialConfig, HashrateSchedule) {
    let mut segments: Vec<(u64, f32)> = Vec::new();
    let mut t: u64 = 0;
    for phase in phases {
        match *phase {
            Phase::Hold { secs, h } => {
                segments.push((t, h));
                t = t.saturating_add(secs);
            }
            Phase::Stall { secs } => {
                // True hashrate = 0 → λ = 0 → no shares are sampled.
                // The algorithm sees an empty window and reacts via
                // its zero-shares branch.
                segments.push((t, 0.0));
                t = t.saturating_add(secs);
            }
            Phase::Ramp { secs, from, to } => {
                // Approximate as N piecewise-constant segments. Each
                // covers secs/N of the ramp. Sample N points evenly
                // across [from, to]; segment k starts at `t + k * step`
                // and runs to the next segment's start.
                let n = RAMP_SEGMENTS.max(2);
                let step_secs = secs / n;
                for k in 0..n {
                    let alpha = k as f32 / (n - 1) as f32;
                    let h = from + (to - from) * alpha;
                    segments.push((t, h));
                    t = t.saturating_add(step_secs);
                }
            }
        }
    }

    let initial_hashrate = initial_estimate.unwrap_or(TRUE_HASHRATE);
    let total_duration = t;

    let config = TrialConfig {
        duration_secs: total_duration,
        initial_hashrate,
        shares_per_minute,
        tick_interval_secs: 60,
    };
    let schedule = HashrateSchedule::new(segments);
    (config, schedule)
}

/// What kind of trial to run. The `Default` impl picks `Stable` so
/// `CellResult::default()` is well-formed.
///
/// ## Implementation note: phase-DSL backed
///
/// The named variants (`ColdStart`, `Stable`, `Step`) are kept for
/// ergonomics — they're how `default_cells()` and the regression
/// baseline name themselves. Internally each variant is just a
/// shorthand for a predefined `Vec<Phase>` plus an optional initial
/// estimate; the `build` method routes through [`phases_to_trial`]
/// uniformly. `tests::phase_dsl_matches_*` asserts the phase-list
/// path produces the same `(TrialConfig, HashrateSchedule)` a
/// hand-coded `build` arm would.
///
/// `Custom { name, phases, initial_estimate }` lets new scenarios
/// compose primitives directly without adding a variant. A
/// stall-recovery scenario, for example, is:
///
/// ```text
/// Scenario::Custom {
///     name: "stall_recovery_5min".into(),
///     phases: vec![
///         Phase::Hold { secs: 600, h: TRUE_HASHRATE },
///         Phase::Stall { secs: 300 },
///         Phase::Hold { secs: 900, h: TRUE_HASHRATE },
///     ],
///     initial_estimate: None,
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Scenario {
    /// Algorithm's initial belief is far below true hashrate; tests
    /// convergence.
    ColdStart,
    /// Algorithm starts aligned with truth; tests steady-state jitter.
    #[default]
    Stable,
    /// Hashrate steps by `delta_pct` at [`STEP_EVENT_AT_SECS`]; tests
    /// reaction time and sensitivity.
    Step { delta_pct: i32 },
    /// A scenario described directly as a phase list. Lets new
    /// scenarios (stall, sustained noise, transient spike, slow ramp)
    /// compose the [`Phase`] primitives without adding a new variant
    /// to this enum.
    Custom {
        name: String,
        phases: Vec<Phase>,
        initial_estimate: Option<f32>,
    },
}

impl Scenario {
    /// Translates a scenario into its (phases, initial_estimate) form.
    /// Named variants expand to predefined phase lists; `Custom`
    /// returns its stored phases directly.
    fn to_phases_and_initial(&self) -> (Vec<Phase>, Option<f32>) {
        match self {
            Scenario::ColdStart => (
                vec![Phase::Hold {
                    secs: TRIAL_DURATION_SECS,
                    h: TRUE_HASHRATE,
                }],
                Some(COLD_START_INITIAL_HASHRATE),
            ),
            Scenario::Stable => (
                vec![Phase::Hold {
                    secs: TRIAL_DURATION_SECS,
                    h: TRUE_HASHRATE,
                }],
                None,
            ),
            Scenario::Step { delta_pct } => {
                let post = TRUE_HASHRATE * (1.0 + *delta_pct as f32 / 100.0);
                (
                    vec![
                        Phase::Hold {
                            secs: STEP_EVENT_AT_SECS,
                            h: TRUE_HASHRATE,
                        },
                        Phase::Hold {
                            secs: TRIAL_DURATION_SECS - STEP_EVENT_AT_SECS,
                            h: post,
                        },
                    ],
                    None,
                )
            }
            Scenario::Custom {
                phases,
                initial_estimate,
                ..
            } => (phases.clone(), *initial_estimate),
        }
    }

    /// Stable, machine-readable identifier suitable for use as a key
    /// in the TOML output.
    pub fn key(&self) -> String {
        match self {
            Scenario::ColdStart => "cold_start_10gh_to_1ph".to_string(),
            Scenario::Stable => "stable_1ph".to_string(),
            Scenario::Step { delta_pct } => {
                if *delta_pct >= 0 {
                    format!("step_plus_{}_at_15min", delta_pct.unsigned_abs())
                } else {
                    format!("step_minus_{}_at_15min", delta_pct.unsigned_abs())
                }
            }
            Scenario::Custom { name, .. } => name.clone(),
        }
    }

    /// Reverse of [`Scenario::key`] for the named variants. Used by the
    /// regression comparator to reconstruct `Cell` from a baseline TOML
    /// cell-key. Returns `None` for unrecognized strings; `Custom`
    /// scenarios that the framework didn't generate are not
    /// reconstructible from a key alone (the phase list would be lost).
    pub fn from_key(s: &str) -> Option<Self> {
        if s == "cold_start_10gh_to_1ph" {
            return Some(Self::ColdStart);
        }
        if s == "stable_1ph" {
            return Some(Self::Stable);
        }
        if let Some(rest) = s.strip_prefix("step_plus_") {
            let num = rest.split('_').next()?;
            let d: u32 = num.parse().ok()?;
            return Some(Self::Step {
                delta_pct: d as i32,
            });
        }
        if let Some(rest) = s.strip_prefix("step_minus_") {
            let num = rest.split('_').next()?;
            let d: u32 = num.parse().ok()?;
            return Some(Self::Step {
                delta_pct: -(d as i32),
            });
        }
        None
    }

    /// Builds the `TrialConfig` and `HashrateSchedule` for this
    /// scenario at a given share rate. Routes through
    /// [`phases_to_trial`] for all variants.
    pub fn build(&self, shares_per_minute: f32) -> (TrialConfig, HashrateSchedule) {
        let (phases, initial) = self.to_phases_and_initial();
        phases_to_trial(&phases, initial, shares_per_minute)
    }
}

/// A single (algorithm, share_rate, scenario) cell. The algorithm is
/// currently implicit — only `VardiffState` is characterized by this
/// build of the framework — but the cell key in the output baseline
/// carries scenario and rate explicitly.
#[derive(Debug, Clone)]
pub struct Cell {
    pub shares_per_minute: f32,
    pub scenario: Scenario,
}

impl Cell {
    /// Stable, machine-readable identifier in the form
    /// `spm_<RATE>.<scenario_key>`.
    pub fn key(&self) -> String {
        format!(
            "spm_{}.{}",
            self.shares_per_minute as u32,
            self.scenario.key()
        )
    }
}

/// The full set of metric values computed for one cell.
///
/// Metric-keyed rather than field-flat: each registered metric stores
/// its emitted values under its own id (e.g. `convergence_time`,
/// `jitter`). Look up specific values by key with [`CellResult::get`]
/// (which searches across all metrics — keys are unique by convention).
#[derive(Debug, Clone, Default)]
pub struct CellResult {
    pub shares_per_minute: f32,
    pub scenario: Scenario,
    /// Metric id → emitted values. Populated by [`run_cell`] from the
    /// [`crate::metrics::registry`].
    pub metrics: HashMap<&'static str, MetricValues>,
}

impl CellResult {
    /// Constructs an empty CellResult for the given cell coordinates.
    /// Convenience for tests; production code uses [`run_cell`].
    pub fn new(shares_per_minute: f32, scenario: Scenario) -> Self {
        Self {
            shares_per_minute,
            scenario,
            metrics: HashMap::new(),
        }
    }

    /// Looks up a metric value by key. Searches across all stored
    /// metrics; returns the first hit. Keys are unique by convention
    /// (`convergence_rate`, `jitter_p50_per_min`, etc.) so order is
    /// irrelevant in practice.
    pub fn get(&self, key: &str) -> Option<f64> {
        self.metrics.values().find_map(|m| m.get(key))
    }

    /// String key form of the scenario for output paths.
    pub fn scenario_key(&self) -> String {
        self.scenario.key()
    }

    /// Inserts a metric's emitted values. Convenience for tests.
    pub fn insert(&mut self, metric_id: &'static str, values: MetricValues) {
        self.metrics.insert(metric_id, values);
    }
}

/// Runs a single cell: builds the scenario, runs `trial_count` trials
/// with deterministic seeds, computes each registered metric, returns
/// a [`CellResult`]. Metric impls in [`crate::metrics::registry`]
/// decide which cells they apply to.
pub fn run_cell(cell: &Cell, trial_count: usize, base_seed: u64, cell_index: u64) -> CellResult {
    let (config, schedule) = cell.scenario.build(cell.shares_per_minute);

    let mut trials: Vec<Trial> = Vec::with_capacity(trial_count);
    for trial_index in 0..trial_count {
        let seed = base_seed
            .wrapping_add(cell_index.wrapping_shl(20))
            .wrapping_add(trial_index as u64);
        let clock = Arc::new(MockClock::new(0));
        let vardiff = VardiffState::new_with_clock(1.0, clock.clone())
            .expect("VardiffState construction should never fail");
        let trial = run_trial(vardiff, clock, config.clone(), &schedule, seed);
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

/// The default characterization grid: 5 share rates × 10 scenarios =
/// 50 cells.
pub fn default_cells() -> Vec<Cell> {
    let rates: [f32; 5] = [6.0, 12.0, 30.0, 60.0, 120.0];
    let deltas: [i32; 8] = [-50, -25, -10, -5, 5, 10, 25, 50];
    let mut cells = Vec::new();
    for &spm in &rates {
        cells.push(Cell {
            shares_per_minute: spm,
            scenario: Scenario::ColdStart,
        });
        cells.push(Cell {
            shares_per_minute: spm,
            scenario: Scenario::Stable,
        });
        for &delta_pct in &deltas {
            cells.push(Cell {
                shares_per_minute: spm,
                scenario: Scenario::Step { delta_pct },
            });
        }
    }
    cells
}

/// Runs every cell in `cells` against the classic [`VardiffState`]
/// algorithm. Sequential; rayon parallelism is a future optimization.
pub fn run_baseline(cells: &[Cell], trial_count: usize, base_seed: u64) -> Vec<CellResult> {
    cells
        .iter()
        .enumerate()
        .map(|(idx, cell)| run_cell(cell, trial_count, base_seed, idx as u64))
        .collect()
}

// ============================================================================
// Serialization: TOML
// ============================================================================

/// Serializes a baseline result set to TOML. The key order matches
/// the registry's metric order, and within each metric the key order
/// matches the metric impl's insertion order. The output ordering is
/// stable across runs given the same registry and the same algorithm
/// — relevant when diffing baselines.
pub fn serialize_toml(
    results: &[CellResult],
    meta_algorithm: &str,
    trial_count: usize,
    base_seed: u64,
) -> String {
    let mut out = String::new();

    out.push_str("# Vardiff baseline characterization. Regenerate with\n");
    out.push_str("# `cargo run --release --bin generate-baseline` from the sim crate.\n\n");

    out.push_str("[meta]\n");
    out.push_str(&format!("algorithm = \"{}\"\n", meta_algorithm));
    out.push_str(&format!("trial_count = {}\n", trial_count));
    out.push_str(&format!("base_seed = {}\n", base_seed));
    out.push_str(&format!("quiet_window_secs = {}\n", QUIET_WINDOW_SECS));
    out.push_str(&format!("settle_buffer_secs = {}\n", SETTLE_BUFFER_SECS));
    out.push_str(&format!(
        "min_settled_window_secs = {}\n",
        MIN_SETTLED_WINDOW_SECS
    ));
    out.push_str(&format!("react_window_secs = {}\n", REACT_WINDOW_SECS));
    out.push_str(&format!("step_event_at_secs = {}\n", STEP_EVENT_AT_SECS));
    out.push_str(&format!("trial_duration_secs = {}\n", TRIAL_DURATION_SECS));
    out.push('\n');

    let registry = metrics::registry();

    for result in results {
        let scenario_key = result.scenario_key();
        out.push_str(&format!(
            "[cell.spm_{}.{}]\n",
            result.shares_per_minute as u32, scenario_key
        ));
        out.push_str(&format!(
            "shares_per_minute = {}\n",
            result.shares_per_minute
        ));
        out.push_str(&format!("scenario = \"{}\"\n", scenario_key));

        for metric in &registry {
            if let Some(mv) = result.metrics.get(metric.id()) {
                for (key, value, ci) in mv.iter() {
                    if let Some(v) = value {
                        out.push_str(&format!("{} = {}\n", key, v));
                    }
                    if let Some((lo, hi)) = ci {
                        out.push_str(&format!("{}_ci_low = {}\n", key, lo));
                        out.push_str(&format!("{}_ci_high = {}\n", key, hi));
                    }
                    // `None` values are omitted; the parser treats
                    // an absent key as `None` on the read side.
                }
            }
        }
        out.push('\n');
    }

    // Derived metrics — per-share-rate cross-cell aggregations
    // (decoupling score, reaction asymmetry). One section per
    // (metric_id, spm) pair.
    for derived in metrics::derived_registry() {
        for (spm, mv) in derived.compute(results) {
            out.push_str(&format!(
                "[derived.{}.spm_{}]\n",
                derived.id(),
                spm as u32
            ));
            for (key, value, ci) in mv.iter() {
                if let Some(v) = value {
                    out.push_str(&format!("{} = {}\n", key, v));
                }
                if let Some((lo, hi)) = ci {
                    out.push_str(&format!("{}_ci_low = {}\n", key, lo));
                    out.push_str(&format!("{}_ci_high = {}\n", key, hi));
                }
            }
            out.push('\n');
        }
    }

    out
}

// ============================================================================
// Serialization: Markdown
// ============================================================================

/// Serializes a baseline result set to a human-readable Markdown
/// summary.
///
/// Registry-driven: iterates `metrics::registry()` and calls each
/// metric's `Metric::render_markdown` to emit its section(s), then
/// iterates `metrics::derived_registry()` for the cross-cell sections
/// (decoupling score, reaction asymmetry).
///
/// Adding a new metric to either registry automatically adds its
/// section to the report — no edits to this function required.
pub fn serialize_markdown(
    results: &[CellResult],
    meta_algorithm: &str,
    trial_count: usize,
    base_seed: u64,
) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "# Vardiff baseline characterization — `{}`\n\n",
        meta_algorithm
    ));
    out.push_str(&format!(
        "*Generated by `cargo run --release --bin generate-baseline` from the \
        vardiff_sim crate. {} trials per cell, base seed `{:#x}`.*\n\n",
        trial_count, base_seed
    ));

    render_summary(results, &mut out);

    for metric in metrics::registry() {
        metric.render_markdown(results, &mut out);
    }

    for derived in metrics::derived_registry() {
        derived.render_markdown(results, &mut out);
    }

    out
}

/// Renders the TL;DR "## Summary" section at the top of the report.
/// Each row is a headline declared by a metric or derived metric via
/// `summary_specs`. For each spec we scan all matching cells and
/// emit the best / worst values across share rates.
fn render_summary(results: &[CellResult], w: &mut String) {
    use metrics::{Direction, ScenarioFilter, SummaryFmt, SummarySpec};

    // Collect (label, direction, best_value@spm, worst_value@spm, fmt)
    // for every spec across both registries.
    let mut rows: Vec<(SummarySpec, Option<(f32, f64)>, Option<(f32, f64)>)> = Vec::new();

    let cell_matches = |scen: &Scenario, f: &ScenarioFilter| match f {
        ScenarioFilter::Any => true,
        ScenarioFilter::Stable => matches!(scen, Scenario::Stable),
        ScenarioFilter::ColdStart => matches!(scen, Scenario::ColdStart),
        ScenarioFilter::StepDelta(d) => matches!(scen, Scenario::Step { delta_pct } if delta_pct == d),
    };

    // Per-cell metric specs read from the cell's MetricValues via
    // `r.get(key)`. Derived-metric specs require the same `compute`
    // pass the renderer uses; we run it once and inspect the spm/key
    // values.
    let collect = |spec: SummarySpec, candidates: Vec<(f32, f64)>| {
        let mut best: Option<(f32, f64)> = None;
        let mut worst: Option<(f32, f64)> = None;
        for (spm, v) in candidates {
            match spec.direction {
                Direction::HigherIsBetter => {
                    if best.map_or(true, |(_, b)| v > b) {
                        best = Some((spm, v));
                    }
                    if worst.map_or(true, |(_, w)| v < w) {
                        worst = Some((spm, v));
                    }
                }
                Direction::LowerIsBetter => {
                    if best.map_or(true, |(_, b)| v < b) {
                        best = Some((spm, v));
                    }
                    if worst.map_or(true, |(_, w)| v > w) {
                        worst = Some((spm, v));
                    }
                }
                Direction::Either => {
                    // For Either, "best" is closest to zero, "worst" is farthest.
                    if best.map_or(true, |(_, b)| v.abs() < b.abs()) {
                        best = Some((spm, v));
                    }
                    if worst.map_or(true, |(_, w)| v.abs() > w.abs()) {
                        worst = Some((spm, v));
                    }
                }
            }
        }
        (spec, best, worst)
    };

    for metric in metrics::registry() {
        for spec in metric.summary_specs() {
            let candidates: Vec<(f32, f64)> = results
                .iter()
                .filter(|r| cell_matches(&r.scenario, &spec.scenario_filter))
                .filter_map(|r| r.get(spec.key).map(|v| (r.shares_per_minute, v)))
                .collect();
            if !candidates.is_empty() {
                rows.push(collect(spec, candidates));
            }
        }
    }

    for derived in metrics::derived_registry() {
        let computed = derived.compute(results);
        for spec in derived.summary_specs() {
            let candidates: Vec<(f32, f64)> = computed
                .iter()
                .filter_map(|(spm, mv)| mv.get(spec.key).map(|v| (*spm, v)))
                .collect();
            if !candidates.is_empty() {
                rows.push(collect(spec, candidates));
            }
        }
    }

    if rows.is_empty() {
        return;
    }

    w.push_str("## Summary\n\n");
    w.push_str("Headline per-metric values at each share rate's best and worst cells. ↑ = higher is better; ↓ = lower is better; ↔ = near-zero is better.\n\n");
    w.push_str("| Metric | Dir | Best | Worst |\n");
    w.push_str("| --- | :-: | --- | --- |\n");
    for (spec, best, worst) in rows {
        let arrow = match spec.direction {
            metrics::Direction::HigherIsBetter => "↑",
            metrics::Direction::LowerIsBetter => "↓",
            metrics::Direction::Either => "↔",
        };
        w.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            spec.label,
            arrow,
            fmt_summary_cell(best, spec.fmt),
            fmt_summary_cell(worst, spec.fmt),
        ));
    }
    w.push('\n');
}

fn fmt_summary_cell(v: Option<(f32, f64)>, fmt: metrics::SummaryFmt) -> String {
    use metrics::SummaryFmt;
    let Some((spm, value)) = v else {
        return "—".to_string();
    };
    let formatted = match fmt {
        SummaryFmt::Percentage => format!("{:.1}%", value * 100.0),
        SummaryFmt::Duration => fmt_duration(Some(value)),
        SummaryFmt::Float3 => format!("{:.3}", value),
        SummaryFmt::RatePerMin => format!("{:.3}/min", value),
    };
    format!("{} @ SPM={}", formatted, spm as u32)
}

/// Returns the distinct share rates present in `results`, sorted
/// ascending. Used by metric `render_markdown` impls to build column
/// headers for per-rate tables. Public so metric impls (which live in
/// `crate::metrics`) can call it.
pub fn unique_rates(results: &[CellResult]) -> Vec<u32> {
    let mut rates: Vec<u32> = results.iter().map(|r| r.shares_per_minute as u32).collect();
    rates.sort_unstable();
    rates.dedup();
    rates
}

/// Finds the cell at `(spm, scenario_key)` in a result set. Used by
/// metric `render_markdown` impls to locate the specific cells they
/// render. Public so metric impls can call it.
pub fn find_cell<'a>(
    results: &'a [CellResult],
    spm: u32,
    scenario_key: &str,
) -> Option<&'a CellResult> {
    results
        .iter()
        .find(|r| r.shares_per_minute as u32 == spm && r.scenario_key() == scenario_key)
}

/// Formats a duration in seconds as a human-readable string ("30s",
/// "5m", "5m30s"). `None` renders as "—". Public for metric impls.
pub fn fmt_duration(secs: Option<f64>) -> String {
    match secs {
        None => "—".to_string(),
        Some(s) => {
            let total = s.round() as u64;
            if total < 60 {
                format!("{}s", total)
            } else {
                let m = total / 60;
                let s = total % 60;
                if s == 0 {
                    format!("{}m", m)
                } else {
                    format!("{}m{:02}s", m, s)
                }
            }
        }
    }
}

/// Formats a fractional value as a percentage with one decimal place
/// ("12.3%"). `None` renders as "—". For values *already* in
/// percentage points use [`fmt_f`] (or divide by 100 first).
pub fn fmt_pct(v: Option<f64>) -> String {
    match v {
        None => "—".to_string(),
        Some(f) => format!("{:.1}%", f * 100.0),
    }
}

/// Formats a bare float with three decimal places ("0.123"). `None`
/// renders as "—".
pub fn fmt_f(v: Option<f64>) -> String {
    match v {
        None => "—".to_string(),
        Some(f) => format!("{:.3}", f),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cells_has_50_entries() {
        assert_eq!(default_cells().len(), 50);
    }

    #[test]
    fn scenario_keys_are_distinct() {
        let cells = default_cells();
        let mut keys: Vec<String> = cells.iter().map(|c| c.key()).collect();
        let total = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), total, "cell keys must be unique");
    }

    #[test]
    fn scenario_from_key_round_trips() {
        for cell in default_cells() {
            let key = cell.scenario.key();
            let parsed = Scenario::from_key(&key).expect("should parse");
            assert_eq!(parsed, cell.scenario, "round-trip failed for {key}");
        }
    }

    #[test]
    fn small_cell_run_produces_reasonable_results() {
        let cells = vec![
            Cell {
                shares_per_minute: 12.0,
                scenario: Scenario::Stable,
            },
            Cell {
                shares_per_minute: 12.0,
                scenario: Scenario::Step { delta_pct: -50 },
            },
        ];
        let results = run_baseline(&cells, 3, 0xCAFE);
        assert_eq!(results.len(), 2);
        // Convergence rate is computed for every cell and is always
        // present.
        assert!(results[0].get("convergence_rate").is_some());
        // Stable doesn't emit reaction metrics.
        assert!(results[0].get("reaction_rate").is_none());
        // Step does.
        assert!(results[1].get("reaction_rate").is_some());
    }

    #[test]
    fn toml_serialization_includes_meta_and_cells() {
        let cells = vec![Cell {
            shares_per_minute: 12.0,
            scenario: Scenario::Stable,
        }];
        let results = run_baseline(&cells, 3, 0xCAFE);
        let toml = serialize_toml(&results, "VardiffState", 3, 0xCAFE);
        assert!(toml.contains("[meta]"));
        assert!(toml.contains("algorithm = \"VardiffState\""));
        assert!(toml.contains("[cell.spm_12.stable_1ph]"));
        assert!(toml.contains("convergence_rate ="));
    }

    #[test]
    fn markdown_serialization_includes_section_headers() {
        let cells = vec![
            Cell {
                shares_per_minute: 12.0,
                scenario: Scenario::ColdStart,
            },
            Cell {
                shares_per_minute: 12.0,
                scenario: Scenario::Stable,
            },
            Cell {
                shares_per_minute: 12.0,
                scenario: Scenario::Step { delta_pct: -50 },
            },
        ];
        let results = run_baseline(&cells, 3, 0xCAFE);
        let md = serialize_markdown(&results, "VardiffState", 3, 0xCAFE);
        assert!(md.contains("# Vardiff baseline characterization"));
        assert!(md.contains("## Convergence time"));
        assert!(md.contains("## Settled accuracy"));
        assert!(md.contains("## Steady-state jitter"));
        assert!(md.contains("## Reaction time"));
        assert!(md.contains("## Reaction sensitivity"));
    }

    #[test]
    fn fmt_duration_renders_minutes_and_seconds() {
        assert_eq!(fmt_duration(Some(30.0)), "30s");
        assert_eq!(fmt_duration(Some(60.0)), "1m");
        assert_eq!(fmt_duration(Some(125.0)), "2m05s");
        assert_eq!(fmt_duration(None), "—");
    }

    // ---- Phase DSL byte-equivalence ----

    /// Helper: sample a schedule at the timestamps that matter for the
    /// trial driver (every tick boundary plus the trial endpoints).
    fn schedule_samples(s: &HashrateSchedule, duration: u64) -> Vec<(u64, f32)> {
        let mut out = vec![];
        let mut t = 0u64;
        while t <= duration {
            out.push((t, s.at(t)));
            t = t.saturating_add(60);
        }
        out
    }

    #[test]
    fn phase_dsl_matches_legacy_stable_schedule() {
        let (config, schedule) = Scenario::Stable.build(12.0);
        let legacy = HashrateSchedule::stable(TRUE_HASHRATE);
        assert_eq!(config.duration_secs, TRIAL_DURATION_SECS);
        assert_eq!(config.initial_hashrate, TRUE_HASHRATE);
        assert_eq!(
            schedule_samples(&schedule, TRIAL_DURATION_SECS),
            schedule_samples(&legacy, TRIAL_DURATION_SECS),
        );
    }

    #[test]
    fn phase_dsl_matches_legacy_cold_start_schedule() {
        let (config, schedule) = Scenario::ColdStart.build(12.0);
        let legacy = HashrateSchedule::stable(TRUE_HASHRATE);
        assert_eq!(config.duration_secs, TRIAL_DURATION_SECS);
        assert_eq!(config.initial_hashrate, COLD_START_INITIAL_HASHRATE);
        assert_eq!(
            schedule_samples(&schedule, TRIAL_DURATION_SECS),
            schedule_samples(&legacy, TRIAL_DURATION_SECS),
        );
    }

    #[test]
    fn phase_dsl_matches_legacy_step_schedule_negative() {
        let (config, schedule) = Scenario::Step { delta_pct: -50 }.build(12.0);
        let post = TRUE_HASHRATE * 0.5;
        let legacy = HashrateSchedule::step(TRUE_HASHRATE, post, STEP_EVENT_AT_SECS);
        assert_eq!(config.duration_secs, TRIAL_DURATION_SECS);
        assert_eq!(config.initial_hashrate, TRUE_HASHRATE);
        assert_eq!(
            schedule_samples(&schedule, TRIAL_DURATION_SECS),
            schedule_samples(&legacy, TRIAL_DURATION_SECS),
        );
    }

    #[test]
    fn phase_dsl_matches_legacy_step_schedule_positive() {
        let (_, schedule) = Scenario::Step { delta_pct: 25 }.build(12.0);
        let post = TRUE_HASHRATE * 1.25;
        let legacy = HashrateSchedule::step(TRUE_HASHRATE, post, STEP_EVENT_AT_SECS);
        assert_eq!(
            schedule_samples(&schedule, TRIAL_DURATION_SECS),
            schedule_samples(&legacy, TRIAL_DURATION_SECS),
        );
    }

    #[test]
    fn custom_scenario_with_stall_phase_produces_zero_hashrate_window() {
        let scenario = Scenario::Custom {
            name: "stall_recovery".to_string(),
            phases: vec![
                Phase::Hold {
                    secs: 600,
                    h: TRUE_HASHRATE,
                },
                Phase::Stall { secs: 300 },
                Phase::Hold {
                    secs: 900,
                    h: TRUE_HASHRATE,
                },
            ],
            initial_estimate: None,
        };
        let (config, schedule) = scenario.build(12.0);
        assert_eq!(config.duration_secs, 1800);
        assert_eq!(config.initial_hashrate, TRUE_HASHRATE);
        // Before stall: true hashrate.
        assert_eq!(schedule.at(0), TRUE_HASHRATE);
        assert_eq!(schedule.at(599), TRUE_HASHRATE);
        // During stall: zero.
        assert_eq!(schedule.at(600), 0.0);
        assert_eq!(schedule.at(899), 0.0);
        // After recovery: back to true hashrate.
        assert_eq!(schedule.at(900), TRUE_HASHRATE);
        assert_eq!(schedule.at(1799), TRUE_HASHRATE);
    }

    #[test]
    fn custom_scenario_with_ramp_phase_interpolates_linearly() {
        let scenario = Scenario::Custom {
            name: "linear_ramp".to_string(),
            phases: vec![Phase::Ramp {
                secs: 1000,
                from: 0.0,
                to: 1000.0,
            }],
            initial_estimate: None,
        };
        let (_, schedule) = scenario.build(12.0);
        // RAMP_SEGMENTS = 10. Each segment is 100s. Values evenly
        // spaced from 0.0 to 1000.0 across the N samples.
        // First sample: alpha = 0/9 = 0 → 0.0.
        // Last sample: alpha = 9/9 = 1 → 1000.0.
        assert_eq!(schedule.at(0), 0.0);
        // At the boundary just before the last segment.
        assert!(schedule.at(900) > 800.0);
        assert!(schedule.at(900) <= 1000.0);
        // At the last segment's start (t=900), value should be the
        // ramp's terminal value.
        assert_eq!(schedule.at(900), 1000.0);
    }

    #[test]
    fn custom_scenario_carries_initial_estimate_through_to_trial_config() {
        let scenario = Scenario::Custom {
            name: "cold_custom".to_string(),
            phases: vec![Phase::Hold {
                secs: 1800,
                h: TRUE_HASHRATE,
            }],
            initial_estimate: Some(1.0e10),
        };
        let (config, _) = scenario.build(12.0);
        assert_eq!(config.initial_hashrate, 1.0e10);
    }

    #[test]
    fn custom_scenario_key_returns_name() {
        let scenario = Scenario::Custom {
            name: "my_unique_scenario_name".to_string(),
            phases: vec![Phase::Hold {
                secs: 60,
                h: 1.0e15,
            }],
            initial_estimate: None,
        };
        assert_eq!(scenario.key(), "my_unique_scenario_name");
    }

    #[test]
    fn phases_to_trial_handles_mixed_phase_kinds() {
        // A trial that does a hold, then ramps down, then stalls, then
        // recovers. Exercises all three primitives in sequence.
        let phases = vec![
            Phase::Hold {
                secs: 300,
                h: 1.0e15,
            },
            Phase::Ramp {
                secs: 200,
                from: 1.0e15,
                to: 5.0e14,
            },
            Phase::Stall { secs: 200 },
            Phase::Hold {
                secs: 300,
                h: 1.0e15,
            },
        ];
        let (config, schedule) = phases_to_trial(&phases, None, 12.0);
        assert_eq!(config.duration_secs, 1000);
        assert_eq!(schedule.at(0), 1.0e15);
        // At t=500 we should be inside the stall (300 hold + 200 ramp = 500).
        assert_eq!(schedule.at(500), 0.0);
        // At t=700 we should be back to 1e15 (after stall).
        assert_eq!(schedule.at(700), 1.0e15);
    }
}
