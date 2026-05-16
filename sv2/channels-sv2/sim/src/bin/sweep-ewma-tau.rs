//! EWMA τ-parameter sweep. Runs EWMA across five time constants
//! (30 / 60 / 120 / 300 / 600 seconds), each against the canonical
//! 5 × 10 cell grid, then emits a single Markdown comparison table
//! letting you pick the variance/responsiveness Pareto-optimal τ for
//! your operational regime.
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin sweep-ewma-tau
//! ```
//!
//! Output: `ewma_tau_sweep.md` in the current directory (configurable
//! via `VARDIFF_EWMA_OUT_DIR`). The sweep takes 5 × 50 × 1000 = 250,000
//! trials, ~90 seconds in release mode. Reduce via
//! `VARDIFF_EWMA_TRIALS=100` for fast iteration.
//!
//! ## Environment
//!
//! - `VARDIFF_EWMA_TRIALS` — trials per cell (default 1000).
//! - `VARDIFF_EWMA_SEED` — base seed (default `0xDEAD_BEEF_CAFE_F00D`).
//! - `VARDIFF_EWMA_OUT_DIR` — output directory (default `.`).
//! - `VARDIFF_EWMA_TAUS` — comma-separated τ values in seconds
//!   (default `30,60,120,300,600`).
//!
//! ## Output format
//!
//! The Markdown report has one table per key metric:
//!
//! - **Decoupling score** by τ × SPM (higher = better trade-off)
//! - **Jitter p50** by τ × SPM (lower = better)
//! - **Reaction rate at −50% step** by τ × SPM (higher = better)
//! - **Settled accuracy p50** by τ × SPM (lower = better)
//!
//! Reading column-wise gives you "how does τ affect this metric at
//! one SPM?". The Pareto-optimal τ is generally the one that
//! maximizes decoupling without inflating settled accuracy too much.
//!
//! ## Paired vs un-paired
//!
//! Uses `Grid::run_paired` so cross-τ metric differences are
//! attributable to the τ change alone, not to seed disparity.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT};
use vardiff_sim::grid::{AlgorithmSpec, Grid};
use vardiff_sim::metrics::{DecouplingScore, DerivedMetric};

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_EWMA_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_EWMA_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_EWMA_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let taus: Vec<u64> = env::var("VARDIFF_EWMA_TAUS")
        .ok()
        .and_then(|s| {
            let v: Vec<u64> = s
                .split(',')
                .filter_map(|t| t.trim().parse().ok())
                .collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        })
        .unwrap_or_else(|| vec![30, 60, 120, 300, 600]);

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &d in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: d });
    }

    let grid = Grid {
        algorithms: taus.iter().map(|&t| AlgorithmSpec::ewma(t)).collect(),
        share_rates: vec![6.0, 12.0, 30.0, 60.0, 120.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "EWMA τ sweep: τ ∈ {{{}}} × {} cells × {} trials = {} total trials, base_seed = {:#x}",
        taus.iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
        base_seed,
    );

    let started = Instant::now();
    let results = grid.run_paired();
    eprintln!("Sweep complete in {:.2}s\n", started.elapsed().as_secs_f64());

    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("ewma_tau_sweep.md");

    let report = build_report(&taus, &grid.share_rates, &results, trial_count, base_seed);
    fs::write(&out_path, &report)?;
    eprintln!("Wrote {}", out_path.display());

    Ok(())
}

fn build_report(
    taus: &[u64],
    share_rates: &[f32],
    results: &std::collections::HashMap<String, Vec<vardiff_sim::baseline::CellResult>>,
    trial_count: usize,
    base_seed: u64,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# EWMA τ sweep ({} trials/cell, base_seed = {:#x})\n\n",
        trial_count, base_seed
    ));
    out.push_str(
        "Pareto-explore the EWMA time constant τ. Lower τ → more \
         responsive but more jittery; higher τ → smoother but slower \
         to react. The right τ is the one that maximizes decoupling \
         score without inflating settled accuracy beyond your \
         tolerance.\n\n",
    );

    // Helper: lookup a metric value at (τ, spm, scenario_key).
    let lookup = |tau: u64, spm: f32, scenario_key: &str, key: &str| -> Option<f64> {
        let name = format!("EWMA-{tau}s");
        results
            .get(&name)?
            .iter()
            .find(|c| c.shares_per_minute == spm && c.scenario_key() == scenario_key)
            .and_then(|c| c.get(key))
    };

    // ---- Decoupling score per τ ----
    out.push_str("## Decoupling score (higher = better)\n\n");
    out.push_str(
        "`reaction_rate(Step−50) × clamp(1 − jitter_p50 / J_max, 0, 1)`, \
         J_max = 0.50 fires/min. 1.0 = perfect; > 0.8 = strong; < 0.3 \
         = poor.\n\n",
    );
    emit_grid_header(&mut out, taus);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &tau in taus {
            let name = format!("EWMA-{tau}s");
            let cells = match results.get(&name) {
                Some(c) => c.clone(),
                None => continue,
            };
            // Per-algorithm decoupling at this spm.
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

    // ---- Jitter p50 under stable load ----
    out.push_str("## Jitter p50 under stable load (lower = better)\n\n");
    out.push_str(
        "Fires per minute, post-convergence. Smaller is better; 0 = \
         no fires under stable load.\n\n",
    );
    emit_grid_header(&mut out, taus);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &tau in taus {
            let v = lookup(tau, spm, "stable_1ph", "jitter_p50_per_min");
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
         true hashrate.\n\n",
    );
    emit_grid_header(&mut out, taus);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &tau in taus {
            let v = lookup(tau, spm, "step_minus_50_at_15min", "reaction_rate");
            out.push_str(&match v {
                Some(x) => format!(" {:.2} |", x),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Settled accuracy p50 ----
    out.push_str("## Settled accuracy p50 (lower = better)\n\n");
    out.push_str(
        "`|final_hashrate / true_hashrate − 1|` at trial end. Smaller \
         = closer to truth.\n\n",
    );
    emit_grid_header(&mut out, taus);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &tau in taus {
            let v = lookup(tau, spm, "stable_1ph", "settled_accuracy_p50");
            out.push_str(&match v {
                Some(x) => format!(" {:.1}% |", x * 100.0),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Estimator variance under stable load ----
    out.push_str("## Estimator variance p50 under stable load (lower = better)\n\n");
    out.push_str(
        "Population variance of `H̃ / H_true` over post-settle ticks. \
         Indicates how noisy the estimator's belief is.\n\n",
    );
    emit_grid_header(&mut out, taus);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &tau in taus {
            let v = lookup(tau, spm, "stable_1ph", "variance_p50");
            out.push_str(&match v {
                Some(x) => format!(" {:.4} |", x),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    out.push('\n');

    // ---- Ramp target overshoot p50 (cold start) ----
    out.push_str("## Ramp target overshoot p50 — cold start (lower = better)\n\n");
    out.push_str(
        "`max(new_hashrate over fires) / H_true − 1`. Larger τ → more \
         smoothing → larger overshoot when the EWMA finally catches \
         up; this table quantifies that trade-off.\n\n",
    );
    emit_grid_header(&mut out, taus);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &tau in taus {
            let v = lookup(tau, spm, "cold_start_10gh_to_1ph", "ramp_target_overshoot_p50");
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
         trajectories.\n\n",
    );
    emit_grid_header(&mut out, taus);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &tau in taus {
            let v = lookup(tau, spm, "cold_start_10gh_to_1ph", "ramp_target_overshoot_p90");
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

fn emit_grid_header(out: &mut String, taus: &[u64]) {
    out.push_str("| SPM |");
    for &t in taus {
        out.push_str(&format!(" τ={}s |", t));
    }
    out.push('\n');
    out.push_str("| ---");
    for _ in taus {
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
