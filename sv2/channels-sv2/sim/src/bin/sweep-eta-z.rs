//! Joint (η, z) sweep for the FullRemedy family. The independent
//! single-axis sweeps (`sweep-eta` and `sweep-z`) suggest that smaller
//! η and slightly higher z each Pareto-improve different metrics; this
//! binary characterizes their joint behavior to surface any cross-axis
//! coupling that the marginal sweeps would miss.
//!
//! Holds `EwmaEstimator(τ = 120s)` fixed (its sweep characterization
//! lives in `sweep-ewma-tau`) and varies η × z over a 3 × 3 grid:
//!
//! - η ∈ {0.1, 0.2, 0.3}
//! - z ∈ {2.576, 3.000, 3.500}
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin sweep-eta-z
//! ```
//!
//! Output: `eta_z_joint_sweep.md` in the current directory
//! (configurable via `VARDIFF_EZ_OUT_DIR`). Default sweep is 9 × 50
//! cells × 1000 trials = 450,000 trials, ~3 minutes in release mode.
//! Reduce via `VARDIFF_EZ_TRIALS=100` for fast iteration.
//!
//! ## Environment
//!
//! - `VARDIFF_EZ_TRIALS` — trials per cell (default 1000).
//! - `VARDIFF_EZ_SEED` — base seed (default `0xDEAD_BEEF_CAFE_F00D`).
//! - `VARDIFF_EZ_OUT_DIR` — output directory (default `.`).
//! - `VARDIFF_EZ_ETAS` — comma-separated η values
//!   (default `0.1,0.2,0.3`).
//! - `VARDIFF_EZ_Z_VALUES` — comma-separated z values
//!   (default `2.576,3.0,3.5`).
//!
//! ## Paired vs un-paired
//!
//! Uses `Grid::run_paired` so cross-(η, z) metric differences are
//! attributable to the parameter change alone, not to seed disparity.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT};
use vardiff_sim::grid::{AlgorithmSpec, Grid};
use vardiff_sim::metrics::{DecouplingScore, DerivedMetric};

const SWEEP_TAU_SECS: u64 = 120;

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_EZ_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_EZ_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_EZ_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let etas: Vec<f32> = env::var("VARDIFF_EZ_ETAS")
        .ok()
        .and_then(|s| {
            let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        })
        .unwrap_or_else(|| vec![0.1, 0.2, 0.3]);
    let zs: Vec<f64> = env::var("VARDIFF_EZ_Z_VALUES")
        .ok()
        .and_then(|s| {
            let v: Vec<f64> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        })
        .unwrap_or_else(|| vec![2.576, 3.0, 3.5]);

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &d in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: d });
    }

    // Build the algorithm list as the Cartesian product (η × z).
    let mut algorithms = Vec::new();
    for &eta in &etas {
        for &z in &zs {
            algorithms.push(AlgorithmSpec::full_remedy_with(SWEEP_TAU_SECS, eta, z));
        }
    }

    let grid = Grid {
        algorithms,
        share_rates: vec![6.0, 12.0, 30.0, 60.0, 120.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "FullRemedy joint (η, z) sweep: η ∈ {{{}}} × z ∈ {{{}}} = {} algorithms × {} cells × {} trials = {} total trials, \
         τ={}s, base_seed = {:#x}",
        etas.iter().map(|e| format!("{:.2}", e)).collect::<Vec<_>>().join(", "),
        zs.iter().map(|z| format!("{:.3}", z)).collect::<Vec<_>>().join(", "),
        etas.len() * zs.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
        SWEEP_TAU_SECS,
        base_seed,
    );

    let started = Instant::now();
    let results = grid.run_paired();
    eprintln!(
        "Sweep complete in {:.2}s\n",
        started.elapsed().as_secs_f64()
    );

    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("eta_z_joint_sweep.md");

    let report = build_report(&etas, &zs, &grid.share_rates, &results, trial_count, base_seed);
    fs::write(&out_path, &report)?;
    eprintln!("Wrote {}", out_path.display());

    Ok(())
}

fn algorithm_name(eta: f32, z: f64) -> String {
    format!(
        "FullRemedy-tau{}-eta{}-z{}",
        SWEEP_TAU_SECS,
        (eta * 100.0).round() as u32,
        (z * 1000.0).round() as u32,
    )
}

#[derive(Clone, Copy)]
enum Direction {
    Higher,
    Lower,
}

#[derive(Clone, Copy)]
struct MetricSpec {
    title: &'static str,
    scenario_key: &'static str,
    metric_key: &'static str,
    direction: Direction,
    format: ValueFormat,
    description: &'static str,
}

#[derive(Clone, Copy)]
enum ValueFormat {
    Decimal3,
    Percent1,
    PerMin3,
    Reaction2,
}

impl ValueFormat {
    fn fmt(&self, v: f64) -> String {
        match self {
            Self::Decimal3 => format!("{:.3}", v),
            Self::Percent1 => format!("{:.1}%", v * 100.0),
            Self::PerMin3 => format!("{:.3}", v),
            Self::Reaction2 => format!("{:.2}", v),
        }
    }
}

fn build_report(
    etas: &[f32],
    zs: &[f64],
    share_rates: &[f32],
    results: &std::collections::HashMap<String, Vec<vardiff_sim::baseline::CellResult>>,
    trial_count: usize,
    base_seed: u64,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# FullRemedy joint (η, z) sweep ({} trials/cell, base_seed = {:#x})\n\n",
        trial_count, base_seed
    ));
    out.push_str(&format!(
        "Joint Pareto exploration of the PartialRetarget η and \
         PoissonCI z axes on the FullRemedy family. Holds \
         `EwmaEstimator(τ = {}s)` fixed and varies (η, z) over a {} × {} \
         grid. Each row in the per-metric tables is a share rate; each \
         column is one (η, z) point. The single-axis sweeps \
         (`eta_sweep.md`, `z_sweep.md`) characterize the marginal \
         effects; this report surfaces any cross-axis coupling — \
         specifically, whether the joint optimum differs from the \
         per-axis marginal optima.\n\n",
        SWEEP_TAU_SECS,
        etas.len(),
        zs.len(),
    ));

    // ---- Cross-axis metric tables ----
    let metrics = [
        MetricSpec {
            title: "Decoupling score",
            scenario_key: "", // computed via DecouplingScore::compute
            metric_key: "score",
            direction: Direction::Higher,
            format: ValueFormat::Decimal3,
            description: "`reaction_rate(Step−50) × clamp(1 − jitter_p50 / J_max, 0, 1)`, \
                          J_max = 0.50 fires/min.",
        },
        MetricSpec {
            title: "Jitter mean (stable load)",
            scenario_key: "stable_1ph",
            metric_key: "jitter_mean_per_min",
            direction: Direction::Lower,
            format: ValueFormat::PerMin3,
            description: "Fires per minute, post-convergence.",
        },
        MetricSpec {
            title: "Reaction rate at −50% step",
            scenario_key: "step_minus_50_at_15min",
            metric_key: "reaction_rate",
            direction: Direction::Higher,
            format: ValueFormat::Reaction2,
            description: "Fraction of trials that fire within 5 min of a 50% drop in true hashrate.",
        },
        MetricSpec {
            title: "Reaction rate at −10% step",
            scenario_key: "step_minus_10_at_15min",
            metric_key: "reaction_rate",
            direction: Direction::Higher,
            format: ValueFormat::Reaction2,
            description: "Small-step sensitivity: a 10% drop is closer to Poisson noise.",
        },
        MetricSpec {
            title: "Settled accuracy p50 (stable)",
            scenario_key: "stable_1ph",
            metric_key: "settled_accuracy_p50",
            direction: Direction::Lower,
            format: ValueFormat::Percent1,
            description: "`|final_hashrate / true_hashrate − 1|` at trial end.",
        },
        MetricSpec {
            title: "Ramp target overshoot p90 (cold start)",
            scenario_key: "cold_start_10gh_to_1ph",
            metric_key: "ramp_target_overshoot_p90",
            direction: Direction::Lower,
            format: ValueFormat::Percent1,
            description: "`max(new_hashrate over fires) / H_true − 1` — tail of the distribution.",
        },
        MetricSpec {
            title: "Ramp target overshoot p99 (cold start)",
            scenario_key: "cold_start_10gh_to_1ph",
            metric_key: "ramp_target_overshoot_p99",
            direction: Direction::Lower,
            format: ValueFormat::Percent1,
            description: "Worst-trial tail of the ramp overshoot distribution.",
        },
    ];

    for metric in &metrics {
        out.push_str(&format!("## {}\n\n", metric.title));
        out.push_str(metric.description);
        out.push('\n');
        out.push('\n');
        emit_joint_table(&mut out, metric, etas, zs, share_rates, results);
        out.push('\n');
    }

    // ---- Pareto summary at SPM=6 ----
    out.push_str("## Pareto summary at SPM=6\n\n");
    out.push_str(
        "Concentrates the trade-off picture at the cell where the \
         FullRemedy parameters have the largest cross-axis coupling \
         (SPM=6 is the hardest cell — sparsest Poisson signal, longest \
         ramp, widest threshold band). For each metric, the table lists \
         the (η, z) point that wins. Reading down: if one (η, z) point \
         wins multiple rows, it is a strong candidate for the new \
         default.\n\n",
    );
    out.push_str("| Metric | Best (η, z) at SPM=6 | Value |\n");
    out.push_str("| --- | --- | --- |\n");
    for metric in &metrics {
        if let Some((eta, z, value)) = best_at_spm(metric, etas, zs, 6.0, results) {
            out.push_str(&format!(
                "| {} | η={:.2}, z={:.3} | {} |\n",
                metric.title,
                eta,
                z,
                metric.format.fmt(value),
            ));
        }
    }
    out.push('\n');

    // ---- Pareto summary at SPM=120 (small-step sensitivity floor) ----
    out.push_str("## Pareto summary at SPM=120\n\n");
    out.push_str(
        "The high-SPM cell. Small-step sensitivity matters most here \
         (the reaction-rate tables in this section drive the operational \
         small-step floor). A new default must not regress \
         meaningfully on the SPM=120 metrics — particularly `reaction \
         rate at −10% step`, which `sweep-z` showed is the canary for \
         excessive z.\n\n",
    );
    out.push_str("| Metric | Best (η, z) at SPM=120 | Value |\n");
    out.push_str("| --- | --- | --- |\n");
    for metric in &metrics {
        if let Some((eta, z, value)) = best_at_spm(metric, etas, zs, 120.0, results) {
            out.push_str(&format!(
                "| {} | η={:.2}, z={:.3} | {} |\n",
                metric.title,
                eta,
                z,
                metric.format.fmt(value),
            ));
        }
    }
    out.push('\n');

    out
}

fn emit_joint_table(
    out: &mut String,
    metric: &MetricSpec,
    etas: &[f32],
    zs: &[f64],
    share_rates: &[f32],
    results: &std::collections::HashMap<String, Vec<vardiff_sim::baseline::CellResult>>,
) {
    // Header.
    out.push_str("| SPM |");
    for &eta in etas {
        for &z in zs {
            out.push_str(&format!(" η{:.2}/z{:.3} |", eta, z));
        }
    }
    out.push('\n');
    out.push_str("| ---");
    for _ in 0..(etas.len() * zs.len()) {
        out.push_str(" | ---");
    }
    out.push_str(" |\n");

    // Rows.
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &eta in etas {
            for &z in zs {
                let v = lookup_metric_value(metric, eta, z, spm, results);
                out.push_str(&match v {
                    Some(x) => format!(" {} |", metric.format.fmt(x)),
                    None => " — |".to_string(),
                });
            }
        }
        out.push('\n');
    }
}

fn lookup_metric_value(
    metric: &MetricSpec,
    eta: f32,
    z: f64,
    spm: f32,
    results: &std::collections::HashMap<String, Vec<vardiff_sim::baseline::CellResult>>,
) -> Option<f64> {
    let name = algorithm_name(eta, z);
    let cells = results.get(&name)?;
    if metric.title == "Decoupling score" {
        // DecouplingScore is a DerivedMetric, computed across cells.
        let scored = DecouplingScore.compute(cells);
        return scored
            .iter()
            .find(|(s, _)| *s == spm)
            .and_then(|(_, mv)| mv.get(metric.metric_key));
    }
    cells
        .iter()
        .find(|c| c.shares_per_minute == spm && c.scenario_key() == metric.scenario_key)
        .and_then(|c| c.get(metric.metric_key))
}

fn best_at_spm(
    metric: &MetricSpec,
    etas: &[f32],
    zs: &[f64],
    spm: f32,
    results: &std::collections::HashMap<String, Vec<vardiff_sim::baseline::CellResult>>,
) -> Option<(f32, f64, f64)> {
    let mut best: Option<(f32, f64, f64)> = None;
    for &eta in etas {
        for &z in zs {
            if let Some(v) = lookup_metric_value(metric, eta, z, spm, results) {
                let is_better = match metric.direction {
                    Direction::Higher => best.map(|(_, _, b)| v > b).unwrap_or(true),
                    Direction::Lower => best.map(|(_, _, b)| v < b).unwrap_or(true),
                };
                if is_better {
                    best = Some((eta, z, v));
                }
            }
        }
    }
    best
}

fn env_or<T: std::str::FromStr>(var: &str, default: T) -> T {
    env::var(var)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_or_seed(var: &str, default: u64) -> u64 {
    if let Ok(s) = env::var(var) {
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            return u64::from_str_radix(hex, 16).unwrap_or(default);
        }
        return s.parse().unwrap_or(default);
    }
    default
}
