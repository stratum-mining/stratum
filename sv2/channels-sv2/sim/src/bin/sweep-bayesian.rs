//! Bayesian Gamma-Poisson discount-factor sweep. Explores the
//! (discount, prior_shares, eta, z) parameter space to find the
//! Pareto-optimal composition for the Bayesian estimator.
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin sweep-bayesian
//! ```
//!
//! Output: `bayesian_sweep.md` in the current directory.
//!
//! ## Environment
//!
//! - `VARDIFF_BAYES_TRIALS` — trials per cell (default 1000).
//! - `VARDIFF_BAYES_SEED` — base seed (default `0xDEAD_BEEF_CAFE_F00D`).
//! - `VARDIFF_BAYES_OUT_DIR` — output directory (default `.`).
//!
//! ## Strategy
//!
//! The sweep explores three dimensions sequentially:
//!
//! 1. **Discount factor** (with fixed eta=0.2, z=2.576, prior=4.0):
//!    Tests the fundamental memory/responsiveness trade-off.
//!
//! 2. **Prior strength** (with best discount from step 1):
//!    Tests initial uncertainty sensitivity.
//!
//! 3. **Joint (discount, eta)** at fixed z=2.576:
//!    Tests whether Bayesian smoothness allows more aggressive updates.
//!
//! The output includes FullRemedy as a reference column for direct
//! comparison.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{CellResult, Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT};
use vardiff_sim::grid::{AlgorithmSpec, Grid};
use vardiff_sim::metrics::{DecouplingScore, DerivedMetric};

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_BAYES_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_BAYES_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_BAYES_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &d in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: d });
    }
    let share_rates = vec![6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0];

    // ====================================================================
    // Phase 1: Discount factor sweep (fixed eta=0.2, z=2.576, prior=0.5)
    // Using very low prior so the model adapts fast after reset.
    // ====================================================================
    let discounts = vec![0.50, 0.70, 0.85, 0.90, 0.95, 0.99];
    let fixed_prior = 0.5;
    let fixed_eta = 0.2f32;
    let fixed_z = 2.576;

    let mut algorithms: Vec<AlgorithmSpec> = discounts
        .iter()
        .map(|&d| AlgorithmSpec::bayesian(d, fixed_prior, fixed_eta, fixed_z))
        .collect();
    // Add FullRemedy as reference
    algorithms.push(AlgorithmSpec::full_remedy());

    let grid = Grid {
        algorithms,
        share_rates: share_rates.clone(),
        scenarios: scenarios.clone(),
        trial_count,
        base_seed,
    };

    eprintln!(
        "Phase 1: Discount sweep ({} configs + FullRemedy) × {} cells × {} trials",
        discounts.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
    );

    let started = Instant::now();
    let results_phase1 = grid.run_paired();
    eprintln!(
        "Phase 1 complete in {:.2}s\n",
        started.elapsed().as_secs_f64()
    );

    // ====================================================================
    // Phase 2: Prior strength sweep (with discount=0.85 from phase 1)
    // ====================================================================
    let priors = vec![0.1, 0.5, 1.0, 2.0, 4.0];
    let best_discount = 0.85;

    let mut algorithms: Vec<AlgorithmSpec> = priors
        .iter()
        .map(|&p| AlgorithmSpec::bayesian(best_discount, p, fixed_eta, fixed_z))
        .collect();
    algorithms.push(AlgorithmSpec::full_remedy());

    let grid = Grid {
        algorithms,
        share_rates: share_rates.clone(),
        scenarios: scenarios.clone(),
        trial_count,
        base_seed,
    };

    eprintln!(
        "Phase 2: Prior sweep ({} configs + FullRemedy) × {} cells × {} trials",
        priors.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
    );

    let started = Instant::now();
    let results_phase2 = grid.run_paired();
    eprintln!(
        "Phase 2 complete in {:.2}s\n",
        started.elapsed().as_secs_f64()
    );

    // ====================================================================
    // Phase 3: Joint (discount, eta) sweep
    // ====================================================================
    let etas: Vec<f32> = vec![0.2, 0.3, 0.5, 0.7, 1.0];
    let discounts_joint = vec![0.50, 0.70, 0.85];

    let mut algorithms: Vec<AlgorithmSpec> = Vec::new();
    for &d in &discounts_joint {
        for &eta in &etas {
            algorithms.push(AlgorithmSpec::bayesian(d, fixed_prior, eta, fixed_z));
        }
    }
    algorithms.push(AlgorithmSpec::full_remedy());

    let grid = Grid {
        algorithms,
        share_rates: share_rates.clone(),
        scenarios: scenarios.clone(),
        trial_count,
        base_seed,
    };

    eprintln!(
        "Phase 3: Joint (discount, eta) sweep ({} configs + FullRemedy) × {} cells × {} trials",
        discounts_joint.len() * etas.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
    );

    let started = Instant::now();
    let results_phase3 = grid.run_paired();
    eprintln!(
        "Phase 3 complete in {:.2}s\n",
        started.elapsed().as_secs_f64()
    );

    // ====================================================================
    // Build report
    // ====================================================================
    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("bayesian_sweep.md");

    let report = build_report(
        &discounts,
        &priors,
        &discounts_joint,
        &etas,
        &share_rates,
        &results_phase1,
        &results_phase2,
        &results_phase3,
        trial_count,
        base_seed,
    );
    fs::write(&out_path, &report)?;
    eprintln!("Wrote {}", out_path.display());

    Ok(())
}

fn build_report(
    discounts: &[f64],
    priors: &[f64],
    discounts_joint: &[f64],
    etas: &[f32],
    share_rates: &[f32],
    results_phase1: &HashMap<String, Vec<CellResult>>,
    results_phase2: &HashMap<String, Vec<CellResult>>,
    results_phase3: &HashMap<String, Vec<CellResult>>,
    trial_count: usize,
    base_seed: u64,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Bayesian Gamma-Poisson sweep ({} trials/cell, base_seed = {:#x})\n\n",
        trial_count, base_seed
    ));
    out.push_str(
        "Explores the Bayesian estimator's parameter space. The Bayesian model \
         maintains a Gamma posterior over the hashrate ratio, updated with \
         exponential discounting each tick. Lower discount = faster adaptation \
         but more noise.\n\n",
    );

    // ---- Phase 1: Discount sweep ----
    out.push_str("## Phase 1: Discount factor sweep\n\n");
    out.push_str(
        "Fixed: prior_shares=0.5, eta=0.2, z=2.576. FullRemedy (EWMA τ=120s) for reference.\n\n",
    );

    // Decoupling score
    out.push_str("### Decoupling score (higher = better)\n\n");
    emit_header_with_ref(&mut out, discounts, "d");
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &d in discounts {
            let name = bayes_name(d, 0.5, 0.2, 2.576);
            let score = get_decoupling(results_phase1, &name, spm);
            out.push_str(&fmt_opt(score));
        }
        let fr_score = get_decoupling(results_phase1, "FullRemedy", spm);
        out.push_str(&fmt_opt(fr_score));
        out.push('\n');
    }
    out.push('\n');

    // Reaction rate -50%
    out.push_str("### Reaction rate at −50% step (higher = better)\n\n");
    emit_header_with_ref(&mut out, discounts, "d");
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &d in discounts {
            let name = bayes_name(d, 0.5, 0.2, 2.576);
            let v = get_metric(
                results_phase1,
                &name,
                spm,
                "step_minus_50_at_15min",
                "reaction_rate",
            );
            out.push_str(&fmt_opt(v));
        }
        let v = get_metric(
            results_phase1,
            "FullRemedy",
            spm,
            "step_minus_50_at_15min",
            "reaction_rate",
        );
        out.push_str(&fmt_opt(v));
        out.push('\n');
    }
    out.push('\n');

    // Jitter
    out.push_str("### Stable-load jitter mean (lower = better)\n\n");
    emit_header_with_ref(&mut out, discounts, "d");
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &d in discounts {
            let name = bayes_name(d, 0.5, 0.2, 2.576);
            let v = get_metric(
                results_phase1,
                &name,
                spm,
                "stable_1ph",
                "jitter_mean_per_min",
            );
            out.push_str(&fmt_opt(v));
        }
        let v = get_metric(
            results_phase1,
            "FullRemedy",
            spm,
            "stable_1ph",
            "jitter_mean_per_min",
        );
        out.push_str(&fmt_opt(v));
        out.push('\n');
    }
    out.push('\n');

    // Settled accuracy
    out.push_str("### Settled accuracy p50 (lower = better)\n\n");
    emit_header_with_ref(&mut out, discounts, "d");
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &d in discounts {
            let name = bayes_name(d, 0.5, 0.2, 2.576);
            let v = get_metric(
                results_phase1,
                &name,
                spm,
                "stable_1ph",
                "settled_accuracy_p50",
            );
            out.push_str(&fmt_opt_pct(v));
        }
        let v = get_metric(
            results_phase1,
            "FullRemedy",
            spm,
            "stable_1ph",
            "settled_accuracy_p50",
        );
        out.push_str(&fmt_opt_pct(v));
        out.push('\n');
    }
    out.push('\n');

    // Cold-start convergence
    out.push_str("### Cold-start convergence rate (higher = better)\n\n");
    emit_header_with_ref(&mut out, discounts, "d");
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &d in discounts {
            let name = bayes_name(d, 0.5, 0.2, 2.576);
            let v = get_metric(
                results_phase1,
                &name,
                spm,
                "cold_start_10gh_to_1ph",
                "convergence_rate",
            );
            out.push_str(&fmt_opt_pct(v));
        }
        let v = get_metric(
            results_phase1,
            "FullRemedy",
            spm,
            "cold_start_10gh_to_1ph",
            "convergence_rate",
        );
        out.push_str(&fmt_opt_pct(v));
        out.push('\n');
    }
    out.push('\n');

    // Ramp overshoot p99
    out.push_str("### Ramp target overshoot p99 — cold start (lower = better)\n\n");
    emit_header_with_ref(&mut out, discounts, "d");
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &d in discounts {
            let name = bayes_name(d, 0.5, 0.2, 2.576);
            let v = get_metric(
                results_phase1,
                &name,
                spm,
                "cold_start_10gh_to_1ph",
                "ramp_target_overshoot_p99",
            );
            out.push_str(&fmt_opt_pct(v));
        }
        let v = get_metric(
            results_phase1,
            "FullRemedy",
            spm,
            "cold_start_10gh_to_1ph",
            "ramp_target_overshoot_p99",
        );
        out.push_str(&fmt_opt_pct(v));
        out.push('\n');
    }
    out.push('\n');

    // ---- Phase 2: Prior sweep ----
    out.push_str("## Phase 2: Prior strength sweep (discount=0.95)\n\n");
    out.push_str("Fixed: discount=0.85, eta=0.2, z=2.576.\n\n");

    out.push_str("### Decoupling score\n\n");
    emit_header_priors(&mut out, priors);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &p in priors {
            let name = bayes_name(0.95, p, 0.2, 2.576);
            let score = get_decoupling(results_phase2, &name, spm);
            out.push_str(&fmt_opt(score));
        }
        let fr_score = get_decoupling(results_phase2, "FullRemedy", spm);
        out.push_str(&fmt_opt(fr_score));
        out.push('\n');
    }
    out.push('\n');

    out.push_str("### Reaction rate at −50% step\n\n");
    emit_header_priors(&mut out, priors);
    for &spm in share_rates {
        out.push_str(&format!("| {} |", spm as u32));
        for &p in priors {
            let name = bayes_name(0.95, p, 0.2, 2.576);
            let v = get_metric(
                results_phase2,
                &name,
                spm,
                "step_minus_50_at_15min",
                "reaction_rate",
            );
            out.push_str(&fmt_opt(v));
        }
        let v = get_metric(
            results_phase2,
            "FullRemedy",
            spm,
            "step_minus_50_at_15min",
            "reaction_rate",
        );
        out.push_str(&fmt_opt(v));
        out.push('\n');
    }
    out.push('\n');

    // ---- Phase 3: Joint (discount, eta) ----
    out.push_str("## Phase 3: Joint (discount × eta) sweep\n\n");
    out.push_str("Fixed: prior=0.5, z=2.576. Each cell is decoupling score.\n\n");

    for &d in discounts_joint {
        out.push_str(&format!("### discount = {:.2}\n\n", d));
        out.push_str("| SPM |");
        for &eta in etas {
            out.push_str(&format!(" η={:.1} |", eta));
        }
        out.push_str(" FullRemedy |\n| ---");
        for _ in etas {
            out.push_str(" | ---");
        }
        out.push_str(" | --- |\n");
        for &spm in share_rates {
            out.push_str(&format!("| {} |", spm as u32));
            for &eta in etas {
                let name = bayes_name(d, 0.5, eta, 2.576);
                let score = get_decoupling(results_phase3, &name, spm);
                out.push_str(&fmt_opt(score));
            }
            let fr_score = get_decoupling(results_phase3, "FullRemedy", spm);
            out.push_str(&fmt_opt(fr_score));
            out.push('\n');
        }
        out.push('\n');

        // Reaction rate for this discount
        out.push_str(&format!("#### Reaction rate at −50% (d={:.2})\n\n", d));
        out.push_str("| SPM |");
        for &eta in etas {
            out.push_str(&format!(" η={:.1} |", eta));
        }
        out.push_str(" FullRemedy |\n| ---");
        for _ in etas {
            out.push_str(" | ---");
        }
        out.push_str(" | --- |\n");
        for &spm in share_rates {
            out.push_str(&format!("| {} |", spm as u32));
            for &eta in etas {
                let name = bayes_name(d, 0.5, eta, 2.576);
                let v = get_metric(
                    results_phase3,
                    &name,
                    spm,
                    "step_minus_50_at_15min",
                    "reaction_rate",
                );
                out.push_str(&fmt_opt(v));
            }
            let v = get_metric(
                results_phase3,
                "FullRemedy",
                spm,
                "step_minus_50_at_15min",
                "reaction_rate",
            );
            out.push_str(&fmt_opt(v));
            out.push('\n');
        }
        out.push('\n');
    }

    out
}

fn bayes_name(discount: f64, prior: f64, eta: f32, z: f64) -> String {
    format!(
        "Bayesian-d{}-p{}-eta{}-z{}",
        (discount * 1000.0).round() as u32,
        (prior * 10.0).round() as u32,
        (eta * 100.0).round() as u32,
        (z * 1000.0).round() as u32,
    )
}

fn get_decoupling(results: &HashMap<String, Vec<CellResult>>, name: &str, spm: f32) -> Option<f64> {
    let cells = results.get(name)?;
    let scored = DecouplingScore.compute(cells);
    scored
        .iter()
        .find(|(s, _)| *s == spm)
        .and_then(|(_, mv)| mv.get("score"))
}

fn get_metric(
    results: &HashMap<String, Vec<CellResult>>,
    name: &str,
    spm: f32,
    scenario_key: &str,
    metric_key: &str,
) -> Option<f64> {
    results
        .get(name)?
        .iter()
        .find(|c| c.shares_per_minute == spm && c.scenario_key() == scenario_key)
        .and_then(|c| c.get(metric_key))
}

fn fmt_opt(v: Option<f64>) -> String {
    match v {
        Some(x) => format!(" {:.3} |", x),
        None => " — |".to_string(),
    }
}

fn fmt_opt_pct(v: Option<f64>) -> String {
    match v {
        Some(x) => format!(" {:.1}% |", x * 100.0),
        None => " — |".to_string(),
    }
}

fn emit_header_with_ref(out: &mut String, discounts: &[f64], prefix: &str) {
    out.push_str("| SPM |");
    for &d in discounts {
        out.push_str(&format!(" {}={:.2} |", prefix, d));
    }
    out.push_str(" FullRemedy |\n| ---");
    for _ in discounts {
        out.push_str(" | ---");
    }
    out.push_str(" | --- |\n");
}

fn emit_header_priors(out: &mut String, priors: &[f64]) {
    out.push_str("| SPM |");
    for &p in priors {
        out.push_str(&format!(" p={:.0} |", p));
    }
    out.push_str(" FullRemedy |\n| ---");
    for _ in priors {
        out.push_str(" | ---");
    }
    out.push_str(" | --- |\n");
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
