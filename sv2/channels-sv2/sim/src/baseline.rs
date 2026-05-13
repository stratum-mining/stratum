//! Baseline characterization: parameterized sweep of cells × trials, producing
//! a structured result that can be serialized to TOML (machine-readable, for
//! regression assertions) and Markdown (human-readable, for PR review).
//!
//! A **cell** is one tuple of `(algorithm, share_rate, scenario)`. The default
//! grid is 5 share rates × 10 scenarios = 50 cells. With `N=1000` trials per
//! cell that's 50,000 trials and ~20 seconds of wall clock at release-mode
//! speed.
//!
//! Scenarios are constructed to exercise specific algorithm behaviors:
//! - `ColdStart`: algorithm's initial belief is 5 orders of magnitude below
//!   truth. Tests Phase 1 ramp-up + Phase 2 settling.
//! - `Stable`: algorithm starts aligned with truth. Tests Phase 3 jitter under
//!   stable load.
//! - `Step { delta_pct }`: algorithm starts aligned, hashrate steps by
//!   `delta_pct` at 15 minutes. Tests reaction time and reaction sensitivity.
//!   The default grid includes deltas at {-50, -25, -10, -5, +5, +10, +25, +50}
//!   so the full sensitivity curve is captured.

use std::sync::Arc;

use channels_sv2::vardiff::MockClock;
use channels_sv2::VardiffState;

use crate::metrics::{
    convergence_time_distribution, jitter_distribution, reaction_time_distribution,
    settled_accuracy_distribution,
};
use crate::schedule::HashrateSchedule;
use crate::trial::{run_trial, Trial, TrialConfig};

/// Default trial-count per cell. Overridable via the `VARDIFF_BASELINE_TRIALS`
/// environment variable when running `generate-baseline`.
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

/// Step-change scenarios fire their event at this offset from trial start.
pub const STEP_EVENT_AT_SECS: u64 = 15 * 60;

/// Trial duration (seconds).
pub const TRIAL_DURATION_SECS: u64 = 30 * 60;

/// The "true" miner hashrate used by every scenario (1 PH/s). This is held
/// constant across the grid so cells differ only in share rate and scenario
/// shape, not in absolute scale.
pub const TRUE_HASHRATE: f32 = 1.0e15;

/// Default initial estimate for cold-start scenarios (10 GH/s — five orders of
/// magnitude below truth).
pub const COLD_START_INITIAL_HASHRATE: f32 = 1.0e10;

/// What kind of trial to run.
#[derive(Debug, Clone, PartialEq)]
pub enum Scenario {
    /// Algorithm's initial belief is far below true hashrate; tests
    /// convergence.
    ColdStart,
    /// Algorithm starts aligned with truth; tests steady-state jitter.
    Stable,
    /// Hashrate steps by `delta_pct` at [`STEP_EVENT_AT_SECS`]; tests
    /// reaction time and sensitivity. `delta_pct` is signed (e.g., `-50`
    /// is a 50% drop, `+25` is a 25% rise).
    Step { delta_pct: i32 },
}

impl Scenario {
    /// Stable, machine-readable identifier suitable for use as a key in the
    /// TOML output. Distinct scenarios produce distinct keys.
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
        }
    }

    /// Builds the `TrialConfig` and `HashrateSchedule` for this scenario at a
    /// given share rate. All scenarios share the same trial duration and tick
    /// cadence.
    pub fn build(&self, shares_per_minute: f32) -> (TrialConfig, HashrateSchedule) {
        let common = TrialConfig {
            duration_secs: TRIAL_DURATION_SECS,
            initial_hashrate: TRUE_HASHRATE,
            shares_per_minute,
            tick_interval_secs: 60,
        };
        match self {
            Scenario::ColdStart => {
                let config = TrialConfig {
                    initial_hashrate: COLD_START_INITIAL_HASHRATE,
                    ..common
                };
                let schedule = HashrateSchedule::stable(TRUE_HASHRATE);
                (config, schedule)
            }
            Scenario::Stable => {
                let schedule = HashrateSchedule::stable(TRUE_HASHRATE);
                (common, schedule)
            }
            Scenario::Step { delta_pct } => {
                let post = TRUE_HASHRATE * (1.0 + *delta_pct as f32 / 100.0);
                let schedule = HashrateSchedule::step(TRUE_HASHRATE, post, STEP_EVENT_AT_SECS);
                (common, schedule)
            }
        }
    }
}

/// A single (algorithm, share_rate, scenario) cell. The algorithm is currently
/// implicit — only `VardiffState` is characterized by this build of the
/// framework — but the cell key in the output baseline carries scenario and
/// rate explicitly.
#[derive(Debug, Clone)]
pub struct Cell {
    pub shares_per_minute: f32,
    pub scenario: Scenario,
}

impl Cell {
    /// Stable, machine-readable identifier suitable for use as a key in the
    /// TOML output, in the form `spm_<RATE>.<scenario_key>`.
    pub fn key(&self) -> String {
        format!("spm_{}.{}", self.shares_per_minute as u32, self.scenario.key())
    }
}

/// The full set of metrics computed for one cell. All time values are in
/// seconds; all rates and percentile values are dimensionless.
#[derive(Debug, Clone, Default)]
pub struct CellResult {
    pub shares_per_minute: f32,
    pub scenario_key: String,

    /// Fraction of trials that converged in `[0.0, 1.0]`.
    pub convergence_rate: f64,
    pub convergence_p10_secs: Option<f64>,
    pub convergence_p50_secs: Option<f64>,
    pub convergence_p90_secs: Option<f64>,
    pub convergence_p95_secs: Option<f64>,
    pub convergence_p99_secs: Option<f64>,

    pub settled_accuracy_p10: Option<f64>,
    pub settled_accuracy_p50: Option<f64>,
    pub settled_accuracy_p90: Option<f64>,
    pub settled_accuracy_p95: Option<f64>,
    pub settled_accuracy_p99: Option<f64>,

    pub jitter_p50_per_min: Option<f64>,
    pub jitter_p90_per_min: Option<f64>,
    pub jitter_p95_per_min: Option<f64>,
    pub jitter_p99_per_min: Option<f64>,
    pub jitter_mean_per_min: Option<f64>,

    /// Only populated for `Step` scenarios. `None` for `ColdStart`/`Stable`.
    pub reaction_rate: Option<f64>,
    pub reaction_p10_secs: Option<f64>,
    pub reaction_p50_secs: Option<f64>,
    pub reaction_p90_secs: Option<f64>,
    pub reaction_p99_secs: Option<f64>,
}

/// Runs a single cell: builds the scenario, runs `trial_count` trials with
/// deterministic seeds, computes metric distributions, returns a `CellResult`.
pub fn run_cell(
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
        let vardiff = VardiffState::new_with_clock(1.0, clock.clone())
            .expect("VardiffState construction should never fail");
        let trial = run_trial(vardiff, clock, config.clone(), &schedule, seed);
        trials.push(trial);
    }

    let (convergence_rate, conv_dist) = convergence_time_distribution(&trials, QUIET_WINDOW_SECS);
    let accuracy = settled_accuracy_distribution(&trials);
    let jitter = jitter_distribution(
        &trials,
        QUIET_WINDOW_SECS,
        SETTLE_BUFFER_SECS,
        MIN_SETTLED_WINDOW_SECS,
    );

    let (reaction_rate_opt, reaction_dist) = match cell.scenario {
        Scenario::Step { .. } => {
            let (rate, dist) =
                reaction_time_distribution(&trials, STEP_EVENT_AT_SECS, REACT_WINDOW_SECS);
            (Some(rate), Some(dist))
        }
        _ => (None, None),
    };

    CellResult {
        shares_per_minute: cell.shares_per_minute,
        scenario_key: cell.scenario.key(),
        convergence_rate,
        convergence_p10_secs: conv_dist.p10(),
        convergence_p50_secs: conv_dist.p50(),
        convergence_p90_secs: conv_dist.p90(),
        convergence_p95_secs: conv_dist.p95(),
        convergence_p99_secs: conv_dist.p99(),
        settled_accuracy_p10: accuracy.p10(),
        settled_accuracy_p50: accuracy.p50(),
        settled_accuracy_p90: accuracy.p90(),
        settled_accuracy_p95: accuracy.p95(),
        settled_accuracy_p99: accuracy.p99(),
        jitter_p50_per_min: jitter.p50(),
        jitter_p90_per_min: jitter.p90(),
        jitter_p95_per_min: jitter.p95(),
        jitter_p99_per_min: jitter.p99(),
        jitter_mean_per_min: jitter.mean(),
        reaction_rate: reaction_rate_opt,
        reaction_p10_secs: reaction_dist.as_ref().and_then(|d| d.p10()),
        reaction_p50_secs: reaction_dist.as_ref().and_then(|d| d.p50()),
        reaction_p90_secs: reaction_dist.as_ref().and_then(|d| d.p90()),
        reaction_p99_secs: reaction_dist.as_ref().and_then(|d| d.p99()),
    }
}

/// The default characterization grid: 5 share rates × 10 scenarios.
///
/// Share rates: 6, 12, 30, 60, 120 — covering the operational range from
/// "current production default" through "high-throughput pool".
///
/// Scenarios:
/// - `ColdStart` and `Stable` (jitter / convergence)
/// - `Step` at deltas ±50, ±25, ±10, ±5 (reaction sensitivity curve)
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

/// Runs every cell in `cells` against the classic [`VardiffState`] algorithm.
/// Sequential; rayon parallelism is a future optimization.
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

/// Serializes a baseline result set to TOML.
///
/// Hand-written rather than via `serde` + `toml` to avoid adding dependencies
/// to the sim crate's lockfile (which we want to keep minimal given the
/// project's pinned 1.75 toolchain). The schema is small and stable enough
/// that the maintenance cost of hand-writing is low.
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

    for result in results {
        let key = format!(
            "spm_{}.{}",
            result.shares_per_minute as u32, result.scenario_key
        );
        out.push_str(&format!("[cell.{}]\n", key));
        out.push_str(&format!(
            "shares_per_minute = {}\n",
            result.shares_per_minute
        ));
        out.push_str(&format!("scenario = \"{}\"\n", result.scenario_key));
        out.push_str(&format!("convergence_rate = {}\n", result.convergence_rate));
        write_opt(&mut out, "convergence_p10_secs", result.convergence_p10_secs);
        write_opt(&mut out, "convergence_p50_secs", result.convergence_p50_secs);
        write_opt(&mut out, "convergence_p90_secs", result.convergence_p90_secs);
        write_opt(&mut out, "convergence_p95_secs", result.convergence_p95_secs);
        write_opt(&mut out, "convergence_p99_secs", result.convergence_p99_secs);
        write_opt(&mut out, "settled_accuracy_p10", result.settled_accuracy_p10);
        write_opt(&mut out, "settled_accuracy_p50", result.settled_accuracy_p50);
        write_opt(&mut out, "settled_accuracy_p90", result.settled_accuracy_p90);
        write_opt(&mut out, "settled_accuracy_p95", result.settled_accuracy_p95);
        write_opt(&mut out, "settled_accuracy_p99", result.settled_accuracy_p99);
        write_opt(&mut out, "jitter_p50_per_min", result.jitter_p50_per_min);
        write_opt(&mut out, "jitter_p90_per_min", result.jitter_p90_per_min);
        write_opt(&mut out, "jitter_p95_per_min", result.jitter_p95_per_min);
        write_opt(&mut out, "jitter_p99_per_min", result.jitter_p99_per_min);
        write_opt(&mut out, "jitter_mean_per_min", result.jitter_mean_per_min);
        if let Some(r) = result.reaction_rate {
            out.push_str(&format!("reaction_rate = {}\n", r));
        }
        write_opt(&mut out, "reaction_p10_secs", result.reaction_p10_secs);
        write_opt(&mut out, "reaction_p50_secs", result.reaction_p50_secs);
        write_opt(&mut out, "reaction_p90_secs", result.reaction_p90_secs);
        write_opt(&mut out, "reaction_p99_secs", result.reaction_p99_secs);
        out.push('\n');
    }

    out
}

fn write_opt(out: &mut String, key: &str, value: Option<f64>) {
    if let Some(v) = value {
        out.push_str(&format!("{} = {}\n", key, v));
    }
}

// ============================================================================
// Serialization: Markdown
// ============================================================================

/// Serializes a baseline result set to a human-readable markdown summary.
///
/// Tables are grouped by metric type (convergence / accuracy / jitter /
/// reaction time / reaction sensitivity) so a reviewer can scan one
/// property at a time across the operational range.
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

    let rates = unique_rates(results);

    // ---- Convergence ----
    out.push_str("## Convergence time (cold start: 10 GH/s → 1 PH/s)\n\n");
    out.push_str("| share/min | rate | p10 | p50 | p90 | p99 |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for spm in &rates {
        if let Some(r) = find_cell(results, *spm, "cold_start_10gh_to_1ph") {
            out.push_str(&format!(
                "| {} | {:.1}% | {} | {} | {} | {} |\n",
                spm,
                r.convergence_rate * 100.0,
                fmt_duration(r.convergence_p10_secs),
                fmt_duration(r.convergence_p50_secs),
                fmt_duration(r.convergence_p90_secs),
                fmt_duration(r.convergence_p99_secs),
            ));
        }
    }
    out.push('\n');

    // ---- Settled accuracy ----
    out.push_str("## Settled accuracy (stable load, post-convergence)\n\n");
    out.push_str("`|final_hashrate / true_hashrate - 1|` at trial end. Smaller is better.\n\n");
    out.push_str("| share/min | p10 | p50 | p90 | p99 |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for spm in &rates {
        if let Some(r) = find_cell(results, *spm, "stable_1ph") {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                spm,
                fmt_pct(r.settled_accuracy_p10),
                fmt_pct(r.settled_accuracy_p50),
                fmt_pct(r.settled_accuracy_p90),
                fmt_pct(r.settled_accuracy_p99),
            ));
        }
    }
    out.push('\n');

    // ---- Jitter ----
    out.push_str("## Steady-state jitter (fires per minute)\n\n");
    out.push_str(
        "Post-convergence rate of vardiff fires. Smaller is better — \
        ideal is zero under stable load.\n\n",
    );
    out.push_str("| share/min | p50 | p90 | p99 | mean |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for spm in &rates {
        if let Some(r) = find_cell(results, *spm, "stable_1ph") {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                spm,
                fmt_f(r.jitter_p50_per_min),
                fmt_f(r.jitter_p90_per_min),
                fmt_f(r.jitter_p99_per_min),
                fmt_f(r.jitter_mean_per_min),
            ));
        }
    }
    out.push('\n');

    // ---- Reaction time (50% drop) ----
    out.push_str("## Reaction time to a 50% drop (step at 15 min)\n\n");
    out.push_str("| share/min | reacted | p10 | p50 | p90 | p99 |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for spm in &rates {
        if let Some(r) = find_cell(results, *spm, "step_minus_50_at_15min") {
            out.push_str(&format!(
                "| {} | {:.1}% | {} | {} | {} | {} |\n",
                spm,
                r.reaction_rate.unwrap_or(0.0) * 100.0,
                fmt_duration(r.reaction_p10_secs),
                fmt_duration(r.reaction_p50_secs),
                fmt_duration(r.reaction_p90_secs),
                fmt_duration(r.reaction_p99_secs),
            ));
        }
    }
    out.push('\n');

    // ---- Reaction sensitivity curve ----
    out.push_str("## Reaction sensitivity (P[fire within 5 min of step change])\n\n");
    out.push_str("| Δ% |");
    for spm in &rates {
        out.push_str(&format!(" {} |", spm));
    }
    out.push_str("\n| ---");
    for _ in &rates {
        out.push_str(" | ---");
    }
    out.push_str(" |\n");

    let deltas: [i32; 8] = [-50, -25, -10, -5, 5, 10, 25, 50];
    for delta in &deltas {
        let scenario_key = if *delta >= 0 {
            format!("step_plus_{}_at_15min", delta.unsigned_abs())
        } else {
            format!("step_minus_{}_at_15min", delta.unsigned_abs())
        };
        let sign = if *delta >= 0 { "+" } else { "" };
        out.push_str(&format!("| {}{}% |", sign, delta));
        for spm in &rates {
            let pct = find_cell(results, *spm, &scenario_key)
                .and_then(|r| r.reaction_rate)
                .map(|r| format!(" {:.2} |", r))
                .unwrap_or_else(|| " — |".to_string());
            out.push_str(&pct);
        }
        out.push('\n');
    }
    out.push('\n');

    out
}

fn unique_rates(results: &[CellResult]) -> Vec<u32> {
    let mut rates: Vec<u32> = results
        .iter()
        .map(|r| r.shares_per_minute as u32)
        .collect();
    rates.sort_unstable();
    rates.dedup();
    rates
}

fn find_cell<'a>(results: &'a [CellResult], spm: u32, scenario_key: &str) -> Option<&'a CellResult> {
    results
        .iter()
        .find(|r| r.shares_per_minute as u32 == spm && r.scenario_key == scenario_key)
}

fn fmt_duration(secs: Option<f64>) -> String {
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

fn fmt_pct(v: Option<f64>) -> String {
    match v {
        None => "—".to_string(),
        Some(f) => format!("{:.1}%", f * 100.0),
    }
}

fn fmt_f(v: Option<f64>) -> String {
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
        // 5 share rates × (1 cold + 1 stable + 8 step deltas) = 50 cells
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
    fn small_cell_run_produces_reasonable_results() {
        // Run a tiny baseline (3 trials per cell) to exercise the full pipeline.
        // This is a smoke test, not a characterization run.
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
        assert!(results[0].convergence_rate >= 0.0 && results[0].convergence_rate <= 1.0);
        assert_eq!(results[0].reaction_rate, None); // Stable has no reaction metric
        assert!(results[1].reaction_rate.is_some()); // Step does
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
}
