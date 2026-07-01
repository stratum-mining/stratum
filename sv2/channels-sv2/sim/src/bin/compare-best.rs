//! Head-to-head comparison of the new "best-of-best" composition against
//! the current production VardiffState (AdaCUSUM) and the previous
//! AsymCUSUM baseline.
//!
//! ```text
//! cargo run --release --bin compare-best
//! ```

use std::env;

use vardiff_sim::{baseline::Scenario, grid::AlgorithmSpec, grid::Grid};

fn main() {
    let trial_count: usize = env::var("VARDIFF_COMPARE_TRIALS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);

    let base_seed: u64 = 0xDEAD_BEEF_CAFE_F00D;

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &delta in &[-50, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: delta });
    }

    let grid = Grid {
        algorithms: vec![
            AlgorithmSpec::classic_vardiff_state(),
            AlgorithmSpec::ewma_asymmetric_cusum(120, 1.5, 0.05, 3.0, 0.2),
            AlgorithmSpec::best_of_best(),
        ],
        share_rates: vec![6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "Running {} algorithms × {} SPM × {} scenarios × {} trials = {} total",
        grid.algorithms.len(),
        grid.share_rates.len(),
        grid.scenarios.len(),
        grid.trial_count,
        grid.total_runs() * grid.trial_count,
    );

    let results = grid.run_paired();

    println!("# Best-of-Best vs Production — Final Comparison\n");
    println!("Trials per cell: {}\n", trial_count);
    println!("## Compositions\n");
    println!("| Name | Estimator | Boundary | UpdateRule |");
    println!("|------|-----------|----------|------------|");
    println!("| VardiffState (prod) | EwmaEstimator(120s) | AsymCUSUM(s=1.5, f=0.05, t=3.0) | PartialRetarget(η=0.5) |");
    println!("| AsymCUSUM-baseline | EwmaEstimator(120s) | AsymCUSUM(s=1.5, f=0.05, t=3.0) | PartialRetarget(η=0.2) |");
    println!("| **BestOfBest** | SpmRatioEstimator(120s) | AsymCUSUM(s=1.5, f=0.05, t=3.0) | AcceleratingRetarget(base=0.2, max=0.6, acc=0.2) |");
    println!();

    // Summary table
    println!("## Summary (all SPM averaged)\n");
    println!("| Algorithm | Stable Jitter | ±50% React | ±50% Conv | ±25% React | ±25% Conv | ±10% React | ±10% Conv | ±5% React | Cold Conv |");
    println!("|-----------|---------------|------------|-----------|------------|-----------|------------|-----------|-----------|-----------|");

    let mut algo_names: Vec<&String> = results.keys().collect();
    algo_names.sort();

    for algo_name in &algo_names {
        let cells = &results[*algo_name];

        let mut stable_jitter = Vec::new();
        let mut step50_react = Vec::new();
        let mut step50_conv = Vec::new();
        let mut step25_react = Vec::new();
        let mut step25_conv = Vec::new();
        let mut step10_react = Vec::new();
        let mut step10_conv = Vec::new();
        let mut step5_react = Vec::new();
        let mut cold_conv = Vec::new();

        for cell in cells {
            match &cell.scenario {
                Scenario::Stable => {
                    if let Some(v) = cell.get("jitter_mean_per_min") {
                        stable_jitter.push(v);
                    }
                }
                Scenario::Step { delta_pct: 50 } | Scenario::Step { delta_pct: -50 } => {
                    if let Some(v) = cell.get("reaction_rate") { step50_react.push(v); }
                    step50_conv.push(cell.get("convergence_rate").unwrap_or(0.0));
                }
                Scenario::Step { delta_pct: 25 } | Scenario::Step { delta_pct: -25 } => {
                    if let Some(v) = cell.get("reaction_rate") { step25_react.push(v); }
                    step25_conv.push(cell.get("convergence_rate").unwrap_or(0.0));
                }
                Scenario::Step { delta_pct: 10 } | Scenario::Step { delta_pct: -10 } => {
                    if let Some(v) = cell.get("reaction_rate") { step10_react.push(v); }
                    step10_conv.push(cell.get("convergence_rate").unwrap_or(0.0));
                }
                Scenario::Step { delta_pct: 5 } | Scenario::Step { delta_pct: -5 } => {
                    if let Some(v) = cell.get("reaction_rate") { step5_react.push(v); }
                }
                Scenario::ColdStart => {
                    cold_conv.push(cell.get("convergence_rate").unwrap_or(0.0));
                }
                _ => {}
            }
        }

        let mean = |v: &[f64]| -> f64 {
            if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 }
        };

        println!(
            "| {} | {:.4} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            algo_name,
            mean(&stable_jitter),
            mean(&step50_react),
            mean(&step50_conv),
            mean(&step25_react),
            mean(&step25_conv),
            mean(&step10_react),
            mean(&step10_conv),
            mean(&step5_react),
            mean(&cold_conv),
        );
    }

    // Per-SPM convergence on ±50% (the biggest improvement axis)
    println!("\n## Step ±50% Convergence by SPM\n");
    println!("| Algorithm | SPM=6 | SPM=8 | SPM=10 | SPM=12 | SPM=15 | SPM=20 | SPM=25 | SPM=30 |");
    println!("|-----------|-------|-------|--------|--------|--------|--------|--------|--------|");

    for algo_name in &algo_names {
        let cells = &results[*algo_name];
        let mut row = format!("| {} ", algo_name);

        for &spm in &[6.0f32, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0] {
            let values: Vec<f64> = cells
                .iter()
                .filter(|c| {
                    (c.shares_per_minute - spm).abs() < 0.1
                        && matches!(c.scenario, Scenario::Step { delta_pct: 50 } | Scenario::Step { delta_pct: -50 })
                })
                .map(|c| c.get("convergence_rate").unwrap_or(0.0))
                .collect();
            let avg = if values.is_empty() { f64::NAN } else { values.iter().sum::<f64>() / values.len() as f64 };
            row.push_str(&format!("| {:.3} ", avg));
        }
        row.push('|');
        println!("{}", row);
    }

    // Per-SPM jitter
    println!("\n## Stable-Load Jitter by SPM (fires/min)\n");
    println!("| Algorithm | SPM=6 | SPM=8 | SPM=10 | SPM=12 | SPM=15 | SPM=20 | SPM=25 | SPM=30 |");
    println!("|-----------|-------|-------|--------|--------|--------|--------|--------|--------|");

    for algo_name in &algo_names {
        let cells = &results[*algo_name];
        let mut row = format!("| {} ", algo_name);

        for &spm in &[6.0f32, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0] {
            let values: Vec<f64> = cells
                .iter()
                .filter(|c| {
                    (c.shares_per_minute - spm).abs() < 0.1
                        && matches!(c.scenario, Scenario::Stable)
                })
                .filter_map(|c| c.get("jitter_mean_per_min"))
                .collect();
            let avg = if values.is_empty() { f64::NAN } else { values.iter().sum::<f64>() / values.len() as f64 };
            row.push_str(&format!("| {:.4} ", avg));
        }
        row.push('|');
        println!("{}", row);
    }

    // Settled accuracy
    println!("\n## Settled Accuracy p50 by SPM (Stable scenario)\n");
    println!("| Algorithm | SPM=6 | SPM=8 | SPM=10 | SPM=12 | SPM=15 | SPM=20 | SPM=25 | SPM=30 |");
    println!("|-----------|-------|-------|--------|--------|--------|--------|--------|--------|");

    for algo_name in &algo_names {
        let cells = &results[*algo_name];
        let mut row = format!("| {} ", algo_name);

        for &spm in &[6.0f32, 8.0, 10.0, 12.0, 15.0, 20.0, 25.0, 30.0] {
            let values: Vec<f64> = cells
                .iter()
                .filter(|c| {
                    (c.shares_per_minute - spm).abs() < 0.1
                        && matches!(c.scenario, Scenario::Stable)
                })
                .filter_map(|c| c.get("settled_accuracy_p50"))
                .collect();
            let avg = if values.is_empty() { f64::NAN } else { values.iter().sum::<f64>() / values.len() as f64 };
            row.push_str(&format!("| {:.4} ", avg));
        }
        row.push('|');
        println!("{}", row);
    }
}
