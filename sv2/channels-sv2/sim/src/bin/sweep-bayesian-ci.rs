//! Bayesian + CredibleIntervalBoundary sweep. Explores the parameter
//! space of the uncertainty-aware composition to find configurations
//! that match or beat FullRemedy.
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin sweep-bayesian-ci
//! ```
//!
//! ## Strategy
//!
//! Sweeps (discount, prior_shares, ci_z, eta) with FullRemedy as reference.
//! The CI boundary adapts to the estimator's reported uncertainty, so the
//! key parameters are:
//! - discount: controls how fast the posterior adapts (lower = faster)
//! - prior_shares: controls post-fire confidence (lower = more uncertain after fire)
//! - ci_z: how many σ required to fire (lower = more sensitive)
//! - eta: PartialRetarget damping

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{CellResult, Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT};
use vardiff_sim::grid::{AlgorithmSpec, Grid};
use vardiff_sim::metrics::{DecouplingScore, DerivedMetric};

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_BCI_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_BCI_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_BCI_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &d in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: d });
    }
    let share_rates = vec![6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0];

    // Sweep: (discount, ci_z, eta) with fixed prior=4.0
    let discounts = vec![0.70, 0.85, 0.90, 0.95];
    let ci_zs = vec![1.5, 1.96, 2.5, 3.0];
    let etas: Vec<f32> = vec![0.2, 0.3, 0.5];
    let prior = 4.0;

    let mut algorithms: Vec<AlgorithmSpec> = Vec::new();
    for &d in &discounts {
        for &z in &ci_zs {
            for &eta in &etas {
                algorithms.push(AlgorithmSpec::bayesian_ci(d, prior, z, eta));
            }
        }
    }
    algorithms.push(AlgorithmSpec::full_remedy());

    let grid = Grid {
        algorithms,
        share_rates: share_rates.clone(),
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "BayesianCI sweep: {} configs + FullRemedy, {} cells × {} trials = {} total",
        discounts.len() * ci_zs.len() * etas.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
    );

    let started = Instant::now();
    let results = grid.run_paired();
    eprintln!(
        "Sweep complete in {:.2}s\n",
        started.elapsed().as_secs_f64()
    );

    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("bayesian_ci_sweep.md");

    let report = build_report(
        &discounts,
        &ci_zs,
        &etas,
        prior,
        &share_rates,
        &results,
        trial_count,
        base_seed,
    );
    fs::write(&out_path, &report)?;
    eprintln!("Wrote {}", out_path.display());

    Ok(())
}

fn build_report(
    discounts: &[f64],
    ci_zs: &[f64],
    etas: &[f32],
    prior: f64,
    share_rates: &[f32],
    results: &HashMap<String, Vec<CellResult>>,
    trial_count: usize,
    base_seed: u64,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# BayesianCI sweep ({} trials/cell, base_seed = {:#x})\n\n",
        trial_count, base_seed
    ));
    out.push_str(&format!(
        "Bayesian estimator + CredibleIntervalBoundary. Fixed prior={}.\n\n",
        prior
    ));

    // Find top configurations by decoupling score at SPM=6 (hardest case)
    let mut scored: Vec<(String, f64)> = Vec::new();
    for (name, cells) in results.iter() {
        if name == "FullRemedy" {
            continue;
        }
        let ds = DecouplingScore.compute(cells);
        if let Some((_, mv)) = ds.iter().find(|(s, _)| *s == 6.0) {
            if let Some(score) = mv.get("score") {
                scored.push((name.clone(), score));
            }
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    out.push_str("## Top 10 configurations by decoupling score at SPM=6\n\n");
    out.push_str("| Rank | Algorithm | SPM=6 | SPM=12 | SPM=30 | SPM=60 | SPM=120 |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");

    let top_n = scored.iter().take(10).collect::<Vec<_>>();
    for (rank, (name, _)) in top_n.iter().enumerate() {
        out.push_str(&format!("| {} | {} |", rank + 1, name));
        for &spm in share_rates {
            let s = get_decoupling(results, name, spm);
            out.push_str(&match s {
                Some(v) => format!(" {:.3} |", v),
                None => " — |".to_string(),
            });
        }
        out.push('\n');
    }
    // Add FullRemedy reference row
    out.push_str("| ref | **FullRemedy** |");
    for &spm in share_rates {
        let s = get_decoupling(results, "FullRemedy", spm);
        out.push_str(&match s {
            Some(v) => format!(" **{:.3}** |", v),
            None => " — |".to_string(),
        });
    }
    out.push_str("\n\n");

    // Detailed metrics for top 5
    out.push_str("## Detailed metrics for top 5\n\n");
    for (rank, (name, _)) in top_n.iter().take(5).enumerate() {
        out.push_str(&format!("### #{} — `{}`\n\n", rank + 1, name));

        out.push_str("| SPM | Decoupling | Reaction -50% | Jitter | Convergence | Overshoot p99 | Settled p50 |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for &spm in share_rates {
            let dc = get_decoupling(results, name, spm);
            let rr = get_metric(
                results,
                name,
                spm,
                "step_minus_50_at_15min",
                "reaction_rate",
            );
            let jt = get_metric(results, name, spm, "stable_1ph", "jitter_mean_per_min");
            let cv = get_metric(
                results,
                name,
                spm,
                "cold_start_10gh_to_1ph",
                "convergence_rate",
            );
            let ov = get_metric(
                results,
                name,
                spm,
                "cold_start_10gh_to_1ph",
                "ramp_target_overshoot_p99",
            );
            let sa = get_metric(results, name, spm, "stable_1ph", "settled_accuracy_p50");
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                spm as u32,
                fmt_opt3(dc),
                fmt_opt3(rr),
                fmt_opt3(jt),
                fmt_pct(cv),
                fmt_pct(ov),
                fmt_pct(sa),
            ));
        }
        out.push('\n');
    }

    // FullRemedy reference
    out.push_str("### Reference — `FullRemedy`\n\n");
    out.push_str("| SPM | Decoupling | Reaction -50% | Jitter | Convergence | Overshoot p99 | Settled p50 |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for &spm in share_rates {
        let dc = get_decoupling(results, "FullRemedy", spm);
        let rr = get_metric(
            results,
            "FullRemedy",
            spm,
            "step_minus_50_at_15min",
            "reaction_rate",
        );
        let jt = get_metric(
            results,
            "FullRemedy",
            spm,
            "stable_1ph",
            "jitter_mean_per_min",
        );
        let cv = get_metric(
            results,
            "FullRemedy",
            spm,
            "cold_start_10gh_to_1ph",
            "convergence_rate",
        );
        let ov = get_metric(
            results,
            "FullRemedy",
            spm,
            "cold_start_10gh_to_1ph",
            "ramp_target_overshoot_p99",
        );
        let sa = get_metric(
            results,
            "FullRemedy",
            spm,
            "stable_1ph",
            "settled_accuracy_p50",
        );
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            spm as u32,
            fmt_opt3(dc),
            fmt_opt3(rr),
            fmt_opt3(jt),
            fmt_pct(cv),
            fmt_pct(ov),
            fmt_pct(sa),
        ));
    }
    out.push('\n');

    out
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
    scenario: &str,
    key: &str,
) -> Option<f64> {
    results
        .get(name)?
        .iter()
        .find(|c| c.shares_per_minute == spm && c.scenario_key() == scenario)
        .and_then(|c| c.get(key))
}

fn fmt_opt3(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.3}", x),
        None => "—".to_string(),
    }
}

fn fmt_pct(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.1}%", x * 100.0),
        None => "—".to_string(),
    }
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
