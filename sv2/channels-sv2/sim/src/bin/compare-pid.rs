//! Compares PID-based vardiff approaches against the production AdaCUSUM
//! algorithm across the standard scenario grid.
//!
//! Demonstrates quantitatively how power-of-2 quantization creates a dead
//! zone that makes naive PID implementations unable to track normal hashrate
//! variance (±50%), while AdaCUSUM detects and adjusts within minutes.
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin compare-pid
//! ```
//!
//! ## Configuration
//!
//! - `VARDIFF_COMPARE_TRIALS` — trials per cell (default 1000).

use std::env;

use vardiff_sim::{baseline::Scenario, grid::AlgorithmSpec, grid::Grid};

fn main() {
    let trial_count: usize = env::var("VARDIFF_COMPARE_TRIALS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);

    let base_seed: u64 = 0xDEAD_BEEF_CAFE_F00D;

    // Scenarios that exercise the algorithm's tracking ability
    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &delta in &[-50, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: delta });
    }

    let grid = Grid {
        algorithms: vec![
            // Baselines
            AlgorithmSpec::classic_vardiff_state(),
            AlgorithmSpec::ewma_asymmetric_cusum(120, 1.5, 0.05, 3.0, 0.2),
            // New idea #1: AcceleratingPartialRetarget
            AlgorithmSpec::ada_cusum_accelerating(120, 1.5, 0.05, 3.0, 0.2, 0.6, 0.1),
            // New idea #2: SpmRatio estimator (bypass U256)
            AlgorithmSpec::spm_ratio_cusum(120, 1.5, 0.05, 3.0, 0.2),
            // New idea #3: SignPersistence boundary
            AlgorithmSpec::ewma_sign_persistence(120, 1.5, 0.05, 3.0, 0.03, 0.4, 0.2),
        ],
        share_rates: vec![6.0, 10.0, 12.0, 20.0, 30.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "Running {} algorithms × {} SPM × {} scenarios × {} trials = {} total trials",
        grid.algorithms.len(),
        grid.share_rates.len(),
        grid.scenarios.len(),
        grid.trial_count,
        grid.total_runs() * grid.trial_count,
    );

    let results = grid.run_paired();

    println!("# PID Approaches vs Production Algorithms — Comparison\n");
    println!("Trial count: {}\n", trial_count);

    for algo_name in results.keys() {
        println!("## {}\n", algo_name);
        let cells = &results[algo_name];

        println!(
            "| SPM | Scenario | Conv.Rate | Settled.Acc.p50 | Jitter(fires/min) | React.Rate | React.p50(s) |"
        );
        println!("|-----|----------|-----------|-----------------|-------------------|------------|--------------|");

        for cell in cells {
            let conv = cell.get("convergence_rate").unwrap_or(f64::NAN);
            let acc = cell.get("settled_accuracy_p50").unwrap_or(f64::NAN);
            let jitter = cell.get("jitter_mean_per_min").unwrap_or(f64::NAN);
            let react_rate = cell.get("reaction_rate").unwrap_or(f64::NAN);
            let react_time = cell.get("reaction_p50_secs").unwrap_or(f64::NAN);

            println!(
                "| {:.0} | {:?} | {:.3} | {:.4} | {:.4} | {:.3} | {:.0} |",
                cell.shares_per_minute,
                cell.scenario,
                conv,
                acc,
                jitter,
                react_rate,
                react_time,
            );
        }
        println!();
    }

    // Print focused comparison: Step scenarios only
    println!("## Head-to-Head: Step ±50% Reaction\n");
    println!("| Algorithm | SPM | Step | Reaction Rate | Reaction p50 (s) | Convergence |");
    println!("|-----------|-----|------|---------------|------------------|-------------|");

    for (algo_name, cells) in &results {
        for cell in cells {
            let is_step_50 = matches!(
                cell.scenario,
                Scenario::Step { delta_pct: 50 } | Scenario::Step { delta_pct: -50 }
            );
            if !is_step_50 {
                continue;
            }
            let react_rate = cell.get("reaction_rate").unwrap_or(f64::NAN);
            let react_time = cell.get("reaction_p50_secs").unwrap_or(f64::NAN);
            let conv = cell.get("convergence_rate").unwrap_or(f64::NAN);
            println!(
                "| {} | {:.0} | {:?} | {:.3} | {:.0} | {:.3} |",
                algo_name, cell.shares_per_minute, cell.scenario, react_rate, react_time, conv,
            );
        }
    }
}
