//! PartialRetarget η-parameter sweep for the FullRemedy family. Holds
//! `EwmaEstimator(τ = 120s)` and `PoissonCI(z = 2.576, margin = 0.05)`
//! fixed and varies η ∈ {0.1, 0.2, 0.3, 0.5, 0.7, 1.0}, characterizing
//! the responsiveness/damping Pareto frontier with the same metric
//! suite as `sweep-ewma-tau`.
//!
//! η = 1.0 is the "no damping" boundary — full retarget on every fire.
//! η = 0.2 is the FullRemedy default, chosen via this sweep plus the
//! joint (η, z) sweep (`sweep-eta-z`). Smaller η than 0.2 (e.g. 0.1)
//! tightens overshoot further but catastrophically breaks cold-start
//! convergence at higher SPM — each fire moves too small a fraction of
//! the gap for the algorithm to traverse the cold-start ramp before
//! the rate-aware PoissonCI threshold suppresses firing. η = 0.2 is
//! the smallest value that preserves cold-start convergence rate at
//! ≥99% across every SPM.
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin sweep-eta
//! ```
//!
//! Output: `eta_sweep.md` in the current directory (configurable via
//! `VARDIFF_ETA_OUT_DIR`). Default sweep is 6 × 50 cells × 1000 trials
//! = 300,000 trials, ~2 minutes in release mode. Reduce via
//! `VARDIFF_ETA_TRIALS=100` for fast iteration.
//!
//! ## Environment
//!
//! - `VARDIFF_ETA_TRIALS` — trials per cell (default 1000).
//! - `VARDIFF_ETA_SEED` — base seed (default `0xDEAD_BEEF_CAFE_F00D`).
//! - `VARDIFF_ETA_OUT_DIR` — output directory (default `.`).
//! - `VARDIFF_ETAS` — comma-separated η values (default
//!   `0.1,0.2,0.3,0.5,0.7,1.0`).
//!
//! ## Output format
//!
//! One table per headline metric, η as columns and SPM as rows. The
//! Pareto-optimal η is generally the smallest η whose ramp-overshoot
//! tail and reaction rate are both acceptable — smaller η means tighter
//! overshoot bounds at the cost of slower reaction.
//!
//! ## Paired vs un-paired
//!
//! Uses `Grid::run_paired` so cross-η metric differences are
//! attributable to the η change alone, not to seed disparity.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT};
use vardiff_sim::grid::{AlgorithmSpec, Grid};
use vardiff_sim::metrics::{DecouplingScore, DerivedMetric};

/// Fixed axes for the η sweep — varied independently from η so the
/// reader can attribute any frontier movement to η alone.
const SWEEP_TAU_SECS: u64 = 120;
const SWEEP_Z: f64 = 2.576;

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_ETA_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_ETA_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_ETA_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let etas: Vec<f32> = env::var("VARDIFF_ETAS")
        .ok()
        .and_then(|s| {
            let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        })
        .unwrap_or_else(|| vec![0.1, 0.2, 0.3, 0.5, 0.7, 1.0]);

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &d in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: d });
    }

    let grid = Grid {
        algorithms: etas
            .iter()
            .map(|&eta| AlgorithmSpec::full_remedy_with(SWEEP_TAU_SECS, eta, SWEEP_Z))
            .collect(),
        share_rates: vec![6.0, 12.0, 30.0, 60.0, 120.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "FullRemedy η sweep: η ∈ {{{}}} × {} cells × {} trials = {} total trials, \
         τ={}s, z={}, base_seed = {:#x}",
        etas.iter()
            .map(|e| format!("{:.2}", e))
            .collect::<Vec<_>>()
            .join(", "),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
        SWEEP_TAU_SECS,
        SWEEP_Z,
        base_seed,
    );

    let started = Instant::now();
    let results = grid.run_paired();
    eprintln!(
        "Sweep complete in {:.2}s\n",
        started.elapsed().as_secs_f64()
    );

    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("eta_sweep.md");

    let report = build_report(&etas, &grid.share_rates, &results, trial_count, base_seed);
    fs::write(&out_path, &report)?;
    eprintln!("Wrote {}", out_path.display());

    Ok(())
}

fn algorithm_name(eta: f32) -> String {
    format!(
        "FullRemedy-tau{}-eta{}-z{}",
        SWEEP_TAU_SECS,
        (eta * 100.0).round() as u32,
        (SWEEP_Z * 1000.0).round() as u32,
    )
}

fn build_report(
    etas: &[f32],
    share_rates: &[f32],
    results: &std::collections::HashMap<String, Vec<vardiff_sim::baseline::CellResult>>,
    trial_count: usize,
    base_seed: u64,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# FullRemedy η sweep ({} trials/cell, base_seed = {:#x})\n\n",
        trial_count, base_seed
    ));
    out.push_str(&format!(
        "Pareto-explore the PartialRetarget η parameter on the \
         FullRemedy family. Holds the other two FullRemedy axes fixed \
         (`EwmaEstimator(τ = {}s)`, `PoissonCI(z = {}, margin = 0.05)`) \
         and varies only η. Smaller η means each fire moves a smaller \
         fraction of the gap from `current_h` to the estimator's \
         belief — tighter overshoot bounds at the cost of slower \
         reaction to large step changes.\n\n",
        SWEEP_TAU_SECS, SWEEP_Z,
    ));

    let lookup = |eta: f32, spm: f32, scenario_key: &str, key: &str| -> Option<f64> {
        let name = algorithm_name(eta);
        results
            .get(&name)?
            .iter()
            .find(|c| c.shares_per_minute == spm && c.scenario_key() == scenario_key)
            .and_then(|c| c.get(key))
    };

    // ---- Decoupling score ----
    out.push_str("## Decoupling score (higher = better)\n\n");
    out.push_str(
        "`reaction_rate(Step−50) × clamp(1 − jitter_p50 / J_max, 0, 1)`, \
         J_max = 0.50 fires/min. 1.0 = perfect; > 0.8 = strong; < 0.3 \
         = poor.\n\n",
    );
    emit_grid_header(&mut out, etas);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &eta in etas {
            let name = algorithm_name(eta);
            let cells = match results.get(&name) {
                Some(c) => c.clone(),
                None => continue,
            };
            let scored = DecouplingScore.compute(&cells);
            let s = scored
                .iter()
                .find(|(s, _)| *s == spm)
                .and_then(|(_, mv)| mv.get("score"));
            out.push_str(&match s {
                Some(v) => format!(" {:.3} |", v),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Jitter mean ----
    out.push_str("## Jitter mean under stable load (lower = better)\n\n");
    out.push_str(
        "Fires per minute, post-convergence. Smaller = less active \
         stable-load tracking. FullRemedy by design fires under stable \
         load (the cost of active tracking) so absolute jitter scales \
         with η as well as with the boundary.\n\n",
    );
    emit_grid_header(&mut out, etas);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &eta in etas {
            let v = lookup(eta, spm, "stable_1ph", "jitter_mean_per_min");
            out.push_str(&match v {
                Some(x) => format!(" {:.3} |", x),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Reaction rate at −50% step ----
    out.push_str("## Reaction rate at −50% step (higher = better)\n\n");
    out.push_str(
        "Fraction of trials that fire within 5 min of a 50% drop in \
         true hashrate. Smaller η damps the per-fire move so multiple \
         fires may be needed to traverse the post-step gap — this \
         table captures whether smaller η costs reactive sensitivity.\n\n",
    );
    emit_grid_header(&mut out, etas);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &eta in etas {
            let v = lookup(eta, spm, "step_minus_50_at_15min", "reaction_rate");
            out.push_str(&match v {
                Some(x) => format!(" {:.2} |", x),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Reaction rate at −10% step ----
    out.push_str("## Reaction rate at −10% step (higher = better)\n\n");
    out.push_str(
        "Small-step sensitivity: a 10% drop is closer to Poisson noise \
         and harder to distinguish from chance. Smaller η means each \
         fire moves less, requiring more fires to traverse the gap — \
         which costs sensitivity on shallow steps.\n\n",
    );
    emit_grid_header(&mut out, etas);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &eta in etas {
            let v = lookup(eta, spm, "step_minus_10_at_15min", "reaction_rate");
            out.push_str(&match v {
                Some(x) => format!(" {:.2} |", x),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Settled accuracy p50 ----
    out.push_str("## Settled accuracy p50 under stable load (lower = better)\n\n");
    out.push_str(
        "`|final_hashrate / true_hashrate − 1|` at trial end. Smaller \
         = closer to truth.\n\n",
    );
    emit_grid_header(&mut out, etas);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &eta in etas {
            let v = lookup(eta, spm, "stable_1ph", "settled_accuracy_p50");
            out.push_str(&match v {
                Some(x) => format!(" {:.1}% |", x * 100.0),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Ramp target overshoot p50 (cold start) ----
    out.push_str("## Ramp target overshoot p50 — cold start (lower = better)\n\n");
    out.push_str(
        "`max(new_hashrate over fires) / H_true − 1`. Smaller η directly \
         caps the per-fire jump; this is the metric η was introduced \
         to bound.\n\n",
    );
    emit_grid_header(&mut out, etas);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &eta in etas {
            let v = lookup(eta, spm, "cold_start_10gh_to_1ph", "ramp_target_overshoot_p50");
            out.push_str(&match v {
                Some(x) => format!(" {:.1}% |", x * 100.0),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Ramp target overshoot p90 (cold start) ----
    out.push_str("## Ramp target overshoot p90 — cold start (lower = better)\n\n");
    out.push_str(
        "Tail of the same distribution. Captures unlucky-Poisson ramp \
         trajectories. The η-sensitivity headline metric.\n\n",
    );
    emit_grid_header(&mut out, etas);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &eta in etas {
            let v = lookup(eta, spm, "cold_start_10gh_to_1ph", "ramp_target_overshoot_p90");
            out.push_str(&match v {
                Some(x) => format!(" {:.1}% |", x * 100.0),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Ramp target overshoot p99 (cold start) ----
    out.push_str("## Ramp target overshoot p99 — cold start (lower = better)\n\n");
    out.push_str(
        "Worst-trial tail. This is the metric that the original axis \
         analysis identified PartialRetarget as the closure for.\n\n",
    );
    emit_grid_header(&mut out, etas);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &eta in etas {
            let v = lookup(eta, spm, "cold_start_10gh_to_1ph", "ramp_target_overshoot_p99");
            out.push_str(&match v {
                Some(x) => format!(" {:.1}% |", x * 100.0),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    out
}

fn emit_grid_header(out: &mut String, etas: &[f32]) {
    out.push_str("| SPM |");
    for &e in etas {
        out.push_str(&format!(" η={:.2} |", e));
    }
    out.push('\n');
    out.push_str("| ---");
    for _ in etas {
        out.push_str(" | ---");
    }
    out.push_str(" |\n");
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
