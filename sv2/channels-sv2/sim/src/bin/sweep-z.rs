//! PoissonCI z-parameter sweep for the FullRemedy family. Holds
//! `EwmaEstimator(τ = 120s)` and `PartialRetarget(η = 0.2)` fixed and
//! varies z ∈ {1.96, 2.326, 2.576, 3.0, 3.5}, characterizing the
//! false-fire vs. reaction-sensitivity Pareto frontier under PoissonCI.
//! η is held at the FullRemedy default; the joint `sweep-eta-z`
//! confirmed (η, z) are nearly separable so the z marginal is stable
//! across η.
//!
//! The z values correspond to common confidence levels under the
//! normal approximation:
//!
//! - z = 1.96 — 95% two-sided CI
//! - z = 2.326 — 98% two-sided CI
//! - z = 2.576 — 99% two-sided CI (FullRemedy default)
//! - z = 3.0 — ~99.7% (3-sigma)
//! - z = 3.5 — ~99.95%
//!
//! Higher z widens the threshold, suppressing false fires under stable
//! load at the cost of slower reaction on small steps. This sweep
//! characterizes whether z = 2.576 is Pareto-optimal under the current
//! metric definitions.
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin sweep-z
//! ```
//!
//! Output: `z_sweep.md` in the current directory (configurable via
//! `VARDIFF_Z_OUT_DIR`). Default sweep is 5 × 50 cells × 1000 trials
//! = 250,000 trials, ~90 seconds in release mode. Reduce via
//! `VARDIFF_Z_TRIALS=100` for fast iteration.
//!
//! ## Environment
//!
//! - `VARDIFF_Z_TRIALS` — trials per cell (default 1000).
//! - `VARDIFF_Z_SEED` — base seed (default `0xDEAD_BEEF_CAFE_F00D`).
//! - `VARDIFF_Z_OUT_DIR` — output directory (default `.`).
//! - `VARDIFF_Z_VALUES` — comma-separated z values (default
//!   `1.96,2.326,2.576,3.0,3.5`).
//!
//! ## Paired vs un-paired
//!
//! Uses `Grid::run_paired` so cross-z metric differences are
//! attributable to the z change alone, not to seed disparity.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT};
use vardiff_sim::grid::{AlgorithmSpec, Grid};
use vardiff_sim::metrics::{DecouplingScore, DerivedMetric};

/// Fixed axes for the z sweep — varied independently from z so the
/// reader can attribute any frontier movement to z alone.
const SWEEP_TAU_SECS: u64 = 120;
const SWEEP_ETA: f32 = 0.2;

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_Z_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_Z_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_Z_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let zs: Vec<f64> = env::var("VARDIFF_Z_VALUES")
        .ok()
        .and_then(|s| {
            let v: Vec<f64> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        })
        .unwrap_or_else(|| vec![1.96, 2.326, 2.576, 3.0, 3.5]);

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &d in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: d });
    }

    let grid = Grid {
        algorithms: zs
            .iter()
            .map(|&z| AlgorithmSpec::full_remedy_with(SWEEP_TAU_SECS, SWEEP_ETA, z))
            .collect(),
        share_rates: vec![6.0, 12.0, 30.0, 60.0, 120.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "FullRemedy z sweep: z ∈ {{{}}} × {} cells × {} trials = {} total trials, \
         τ={}s, η={}, base_seed = {:#x}",
        zs.iter()
            .map(|z| format!("{:.3}", z))
            .collect::<Vec<_>>()
            .join(", "),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
        SWEEP_TAU_SECS,
        SWEEP_ETA,
        base_seed,
    );

    let started = Instant::now();
    let results = grid.run_paired();
    eprintln!(
        "Sweep complete in {:.2}s\n",
        started.elapsed().as_secs_f64()
    );

    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("z_sweep.md");

    let report = build_report(&zs, &grid.share_rates, &results, trial_count, base_seed);
    fs::write(&out_path, &report)?;
    eprintln!("Wrote {}", out_path.display());

    Ok(())
}

fn algorithm_name(z: f64) -> String {
    format!(
        "FullRemedy-tau{}-eta{}-z{}",
        SWEEP_TAU_SECS,
        (SWEEP_ETA * 100.0).round() as u32,
        (z * 1000.0).round() as u32,
    )
}

fn build_report(
    zs: &[f64],
    share_rates: &[f32],
    results: &std::collections::HashMap<String, Vec<vardiff_sim::baseline::CellResult>>,
    trial_count: usize,
    base_seed: u64,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# FullRemedy z sweep ({} trials/cell, base_seed = {:#x})\n\n",
        trial_count, base_seed
    ));
    out.push_str(&format!(
        "Pareto-explore the PoissonCI z parameter on the FullRemedy \
         family. Holds the other two FullRemedy axes fixed \
         (`EwmaEstimator(τ = {}s)`, `PartialRetarget(η = {})`) and \
         varies only z. Higher z widens the threshold, suppressing \
         false fires under stable load at the cost of slower reaction \
         on small steps.\n\n",
        SWEEP_TAU_SECS, SWEEP_ETA,
    ));

    let lookup = |z: f64, spm: f32, scenario_key: &str, key: &str| -> Option<f64> {
        let name = algorithm_name(z);
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
    emit_grid_header(&mut out, zs);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &z in zs {
            let name = algorithm_name(z);
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
        "Fires per minute, post-convergence. Higher z directly raises \
         the threshold floor, suppressing stable-load fires — this \
         is the metric z was introduced to bound.\n\n",
    );
    emit_grid_header(&mut out, zs);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &z in zs {
            let v = lookup(z, spm, "stable_1ph", "jitter_mean_per_min");
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
         true hashrate. Higher z lifts the threshold; if it lifts past \
         the post-step δ at low SPM, reaction rate collapses.\n\n",
    );
    emit_grid_header(&mut out, zs);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &z in zs {
            let v = lookup(z, spm, "step_minus_50_at_15min", "reaction_rate");
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
        "Small-step sensitivity. A 10% drop is the hardest signal to \
         distinguish from Poisson noise; z controls how readily the \
         algorithm fires on it.\n\n",
    );
    emit_grid_header(&mut out, zs);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &z in zs {
            let v = lookup(z, spm, "step_minus_10_at_15min", "reaction_rate");
            out.push_str(&match v {
                Some(x) => format!(" {:.2} |", x),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Ramp target overshoot p90 (cold start) ----
    out.push_str("## Ramp target overshoot p90 — cold start (lower = better)\n\n");
    out.push_str(
        "Higher z means the algorithm fires less readily on the \
         Phase-1-end Poisson spike; this trades against responsiveness \
         on slow ramps.\n\n",
    );
    emit_grid_header(&mut out, zs);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &z in zs {
            let v = lookup(z, spm, "cold_start_10gh_to_1ph", "ramp_target_overshoot_p90");
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
    out.push_str("Worst-trial tail of the ramp overshoot distribution.\n\n");
    emit_grid_header(&mut out, zs);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &z in zs {
            let v = lookup(z, spm, "cold_start_10gh_to_1ph", "ramp_target_overshoot_p99");
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

fn emit_grid_header(out: &mut String, zs: &[f64]) {
    out.push_str("| SPM |");
    for &z in zs {
        out.push_str(&format!(" z={:.3} |", z));
    }
    out.push('\n');
    out.push_str("| ---");
    for _ in zs {
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
