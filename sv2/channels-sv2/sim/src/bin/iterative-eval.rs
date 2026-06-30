//! Algorithm comparison report. Ranks algorithms by operational_fitness
//! and highlights per-metric winners at each share rate.
//!
//! ```text
//! cargo run --release --bin iterative-eval
//! ```

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{CellResult, Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT};
use vardiff_sim::grid::{AlgorithmSpec, Grid};
use vardiff_sim::metrics::{ComprehensiveFitness, DerivedMetric, OperationalFitness};

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_ITER_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_ITER_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_ITER_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &d in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: d });
    }
    for &settle in &[5u64, 60] {
        for &delta in &[-50i32, -10] {
            scenarios.push(Scenario::SettledStep {
                settle_minutes: settle,
                delta_pct: delta,
            });
        }
    }
    let share_rates = vec![6.0, 8.0, 10.0, 12.0, 15.0, 20.0, 30.0];

    let algorithms = vec![
        AlgorithmSpec::full_remedy(),
        AlgorithmSpec::ewma_adaptive_cusum(120, 1.5, 0.05, 0.5),
        // Asymmetric: cautious tighten, quick ease
        AlgorithmSpec::ewma_asymmetric_cusum(120, 1.5, 0.05, 1.5, 0.5),
        AlgorithmSpec::ewma_asymmetric_cusum(120, 1.5, 0.05, 2.0, 0.5),
        AlgorithmSpec::ewma_asymmetric_cusum(120, 1.5, 0.05, 3.0, 0.5),
    ];

    let grid = Grid {
        algorithms,
        share_rates: share_rates.clone(),
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "Evaluation: {} algorithms × {} cells × {} trials",
        grid.algorithms.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
    );

    let started = Instant::now();
    let results = grid.run_paired();
    eprintln!("Complete in {:.2}s\n", started.elapsed().as_secs_f64());

    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("iterative_eval.md");
    let report = build_report(&share_rates, &results, trial_count);
    fs::write(&out_path, &report)?;
    eprintln!("Wrote {}", out_path.display());

    Ok(())
}

struct MetricSpec {
    title: &'static str,
    higher_is_better: bool,
    scenario: &'static str,
    key: &'static str,
    fmt: Fmt,
}

#[derive(Clone, Copy)]
enum Fmt {
    Frac3,
    Pct1,
    Secs,
}

fn format_val(v: f64, fmt: Fmt) -> String {
    match fmt {
        Fmt::Frac3 => format!("{:.3}", v),
        Fmt::Pct1 => format!("{:.1}%", v * 100.0),
        Fmt::Secs => {
            let mins = v as u64 / 60;
            let secs = v as u64 % 60;
            if secs == 0 {
                format!("{}m", mins)
            } else {
                format!("{}m{:02}s", mins, secs)
            }
        }
    }
}

fn build_report(
    share_rates: &[f32],
    results: &HashMap<String, Vec<CellResult>>,
    trial_count: usize,
) -> String {
    // Compute average comprehensive fitness across all SPM to rank algorithms
    let mut algo_fitness: Vec<(String, f64, f64)> = results
        .keys()
        .map(|name| {
            let cells = &results[name];
            let comp_scores = ComprehensiveFitness.compute(cells);
            let op_scores = OperationalFitness.compute(cells);
            let comp_avg: f64 = if comp_scores.is_empty() {
                0.0
            } else {
                comp_scores
                    .iter()
                    .filter_map(|(_, mv)| mv.get("score"))
                    .sum::<f64>()
                    / comp_scores.len() as f64
            };
            let op_avg: f64 = if op_scores.is_empty() {
                0.0
            } else {
                op_scores
                    .iter()
                    .filter_map(|(_, mv)| mv.get("score"))
                    .sum::<f64>()
                    / op_scores.len() as f64
            };
            (name.clone(), comp_avg, op_avg)
        })
        .collect();
    algo_fitness.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let sorted_names: Vec<&str> = algo_fitness.iter().map(|(n, _, _)| n.as_str()).collect();

    let mut out = String::new();
    out.push_str(&format!(
        "# Algorithm comparison ({} trials/cell, SPM={}–{})\n\n",
        trial_count,
        share_rates.first().unwrap_or(&0.0) as &f32,
        share_rates.last().unwrap_or(&0.0) as &f32,
    ));
    out.push_str("Algorithms sorted by mean comprehensive fitness (best first).\n");
    out.push_str("**Bold** = best value at that SPM for that metric.\n\n");

    // Ranking table
    out.push_str("## Overall ranking\n\n");
    out.push_str("| Rank | Algorithm | Comprehensive | Operational |\n");
    out.push_str("| --- | --- | --- | --- |\n");
    for (i, (name, comp, op)) in algo_fitness.iter().enumerate() {
        let marker = if i == 0 { " ★" } else { "" };
        out.push_str(&format!(
            "| {} | {}{} | {:.3} | {:.3} |\n",
            i + 1,
            name,
            marker,
            comp,
            op
        ));
    }
    out.push('\n');

    // Metric tables
    let metrics = vec![
        MetricSpec {
            title: "Reaction rate at -10% step",
            higher_is_better: true,
            scenario: "step_minus_10_at_15min",
            key: "reaction_rate",
            fmt: Fmt::Pct1,
        },
        MetricSpec {
            title: "Reaction rate at -50% step",
            higher_is_better: true,
            scenario: "step_minus_50_at_15min",
            key: "reaction_rate",
            fmt: Fmt::Pct1,
        },
        MetricSpec {
            title: "Cold-start convergence time p50",
            higher_is_better: false,
            scenario: "cold_start_10gh_to_1ph",
            key: "convergence_p50_secs",
            fmt: Fmt::Secs,
        },
        MetricSpec {
            title: "Cold-start convergence rate",
            higher_is_better: true,
            scenario: "cold_start_10gh_to_1ph",
            key: "convergence_rate",
            fmt: Fmt::Pct1,
        },
        MetricSpec {
            title: "Stable-load jitter (fires/min)",
            higher_is_better: false,
            scenario: "stable_1ph",
            key: "jitter_mean_per_min",
            fmt: Fmt::Frac3,
        },
        MetricSpec {
            title: "Ramp target overshoot p99",
            higher_is_better: false,
            scenario: "cold_start_10gh_to_1ph",
            key: "ramp_target_overshoot_p99",
            fmt: Fmt::Pct1,
        },
        MetricSpec {
            title: "Settled accuracy p50",
            higher_is_better: false,
            scenario: "stable_1ph",
            key: "settled_accuracy_p50",
            fmt: Fmt::Pct1,
        },
    ];

    for spec in &metrics {
        let dir = if spec.higher_is_better {
            "higher = better"
        } else {
            "lower = better"
        };
        out.push_str(&format!("## {} ({})\n\n", spec.title, dir));

        // Header
        out.push_str("| SPM |");
        for name in &sorted_names {
            out.push_str(&format!(" {} |", name));
        }
        out.push_str("\n| --- |");
        for _ in &sorted_names {
            out.push_str(" --- |");
        }
        out.push('\n');

        // Rows
        for &spm in share_rates {
            out.push_str(&format!("| {} |", spm as u32));

            // Collect values for this row to find the winner
            let values: Vec<Option<f64>> = sorted_names
                .iter()
                .map(|name| get_metric(results, name, spm, spec.scenario, spec.key))
                .collect();

            let best = if spec.higher_is_better {
                values
                    .iter()
                    .filter_map(|v| *v)
                    .fold(f64::NEG_INFINITY, f64::max)
            } else {
                values
                    .iter()
                    .filter_map(|v| *v)
                    .filter(|v| *v > 0.0 || spec.key == "jitter_mean_per_min")
                    .fold(f64::INFINITY, f64::min)
            };

            for v in &values {
                match v {
                    Some(val) => {
                        let is_best = (val - best).abs() < 1e-6
                            || (spec.higher_is_better && *val >= best * 0.99)
                            || (!spec.higher_is_better && *val <= best * 1.01);
                        let formatted = format_val(*val, spec.fmt);
                        if is_best {
                            out.push_str(&format!(" **{}** |", formatted));
                        } else {
                            out.push_str(&format!(" {} |", formatted));
                        }
                    }
                    None => out.push_str(" — |"),
                }
            }
            out.push('\n');
        }
        out.push('\n');
    }

    out
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
