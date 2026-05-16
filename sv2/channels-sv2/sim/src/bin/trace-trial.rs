//! Single-trial tracer. Runs ONE trial with a chosen algorithm,
//! scenario, and seed, then dumps the per-tick state (timestamp,
//! shares, current hashrate, δ, θ, H̃, fired, new hashrate) as a
//! CSV-ish table to stdout. The output is small enough that you can
//! inspect it directly in a terminal or paste it into a spreadsheet
//! for plotting.
//!
//! Designed for investigating one-off anomalies surfaced by the
//! comparison sweep — e.g., "why does Parametric have 45.8%
//! overshoot at SPM=30 cold start in all 1000 trials?".
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin trace-trial -- \
//!     --algorithm parametric \
//!     --scenario cold_start \
//!     --spm 30 \
//!     --seed 0xCAFE
//! ```
//!
//! ## Algorithms
//!
//! - `vardiff_state` — the production algorithm (not observable; δ/θ/H̃
//!   columns will show `—`).
//! - `classic_composed` — four-axis-decomposed Classic. Observable.
//! - `parametric` — Classic with PoissonCI boundary.
//! - `ewma_60s` — EwmaEstimator(60s) + PoissonCI + PartialRetarget.
//!
//! ## Scenarios
//!
//! - `cold_start` — initial 1e10, true 1e15, schedule stable.
//! - `stable` — initial 1e15, true 1e15.
//! - `step_minus_50` — true 1e15 → 5e14 at 15min.
//! - `step_plus_50` — true 1e15 → 1.5e15 at 15min.
//! - `step:<delta>` — generic step at 15min with the given Δ% (e.g.,
//!   `step:-25` for −25%).
//!
//! ## Investigations enabled
//!
//! - **SPM=30 cold-start overshoot** (`--algorithm parametric
//!   --scenario cold_start --spm 30`): look at H̃ column. Does the peak
//!   come from Phase 1 ramp or post-settle Poisson noise?
//! - **VardiffState convergence failure at SPM=6** (`--algorithm
//!   vardiff_state --scenario cold_start --spm 6`): trace 30 trials
//!   with different seeds; identify which scenarios fail to converge.
//! - **Asymmetric step response** (`--algorithm vardiff_state
//!   --scenario step:+50 --spm 120`): see whether the algorithm fires
//!   at all post-step.

use std::env;
use std::sync::Arc;

use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::Scenario;
use vardiff_sim::grid::AlgorithmSpec;
use vardiff_sim::trial::run_trial_observed;

struct Args {
    algorithm: String,
    scenario: String,
    spm: f32,
    seed: u64,
    /// If Some(N), scan N seeds starting from `seed`, find the one
    /// with the largest target overshoot, then trace it.
    scan_overshoot: Option<usize>,
}

fn parse_args() -> Args {
    let mut algorithm = String::from("parametric");
    let mut scenario = String::from("cold_start");
    let mut spm: f32 = 30.0;
    let mut seed: u64 = 0xCAFE;
    let mut scan_overshoot: Option<usize> = None;

    let argv: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        let key = &argv[i];
        let val = argv.get(i + 1).cloned().unwrap_or_default();
        match key.as_str() {
            "--algorithm" | "-a" => algorithm = val,
            "--scenario" | "-s" => scenario = val,
            "--spm" => spm = val.parse().unwrap_or(spm),
            "--seed" => {
                seed = if let Some(hex) = val.strip_prefix("0x").or_else(|| val.strip_prefix("0X"))
                {
                    u64::from_str_radix(hex, 16).unwrap_or(seed)
                } else {
                    val.parse().unwrap_or(seed)
                };
            }
            "--scan-overshoot" => {
                scan_overshoot = val.parse::<usize>().ok();
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {}
        }
        i += 2;
    }

    Args {
        algorithm,
        scenario,
        spm,
        seed,
        scan_overshoot,
    }
}

fn print_help() {
    eprintln!(
        "Usage: trace-trial [options]
Options:
  --algorithm, -a       one of: vardiff_state, classic_composed, parametric,
                        parametric_strict, classic_partial_retarget, ewma_60s,
                        sliding_window, full_remedy (default: parametric)
  --scenario, -s        one of: cold_start, stable, step_minus_50, step_plus_50, step:<delta>
                        (default: cold_start)
  --spm                 shares per minute (default: 30)
  --seed                trial seed; hex (0x...) or decimal (default: 0xCAFE).
                        When --scan-overshoot is set, this is the base seed
                        from which N consecutive seeds are scanned.
  --scan-overshoot N    scan N seeds, find the one with the largest ramp
                        target overshoot, then trace it. Useful for hunting
                        worst-case ramp behavior (e.g., VardiffState SPM=6
                        p99=145% from FINDINGS.md §3.4).
  --help, -h            print this help

Algorithm shorthand:
  parametric_strict         Parametric with z=3.0 (99.7% CI) — stricter boundary.
  classic_partial_retarget  Classic boundary + PartialRetarget(η=0.3) — isolates
                            the UpdateRule axis: bounds per-fire magnitude.
  full_remedy               EwmaEstimator(120s) + PoissonCI + PartialRetarget(0.3) —
                            the composed three-axis remedy predicted to close
                            the SPM=6 Phase-1 cascade.
"
    );
}

fn algorithm_factory(name: &str) -> Result<AlgorithmSpec, String> {
    Ok(match name {
        "vardiff_state" | "vardiff" => AlgorithmSpec::classic_vardiff_state(),
        "classic_composed" | "classic" => AlgorithmSpec::classic_composed(),
        "parametric" => AlgorithmSpec::parametric(),
        "parametric_strict" | "strict" => AlgorithmSpec::parametric_strict(),
        "classic_partial_retarget" | "classic_pr" | "cpr" => {
            AlgorithmSpec::classic_partial_retarget(0.3)
        }
        "ewma_60s" | "ewma" => AlgorithmSpec::ewma_60s(),
        "sliding_window" | "sliding" => AlgorithmSpec::sliding_window(10),
        "full_remedy" | "remedy" => AlgorithmSpec::full_remedy(),
        other => return Err(format!("unknown algorithm: {other}")),
    })
}

fn scenario_from_str(s: &str) -> Result<Scenario, String> {
    Ok(match s {
        "cold_start" | "cold" => Scenario::ColdStart,
        "stable" => Scenario::Stable,
        "step_minus_50" => Scenario::Step { delta_pct: -50 },
        "step_plus_50" => Scenario::Step { delta_pct: 50 },
        other => {
            if let Some(num) = other.strip_prefix("step:") {
                let d: i32 = num.parse().map_err(|_| format!("bad step delta: {num}"))?;
                Scenario::Step { delta_pct: d }
            } else {
                return Err(format!("unknown scenario: {other}"));
            }
        }
    })
}

/// Returns the peak ramp target overshoot (max new_hashrate / true_h
/// − 1, clamped at 0) for one trial. Used by --scan-overshoot to
/// identify the worst-case trial across many seeds.
fn target_overshoot(trial: &vardiff_sim::Trial) -> f64 {
    let true_h = trial.true_hashrate_at_end as f64;
    if true_h <= 0.0 {
        return 0.0;
    }
    let peak = trial
        .ticks
        .iter()
        .filter_map(|t| t.new_hashrate.map(|h| h as f64))
        .fold(f64::NEG_INFINITY, f64::max);
    if peak.is_finite() && peak > 0.0 {
        ((peak / true_h) - 1.0).max(0.0)
    } else {
        0.0
    }
}

fn main() -> Result<(), String> {
    let args = parse_args();

    let algorithm = algorithm_factory(&args.algorithm)?;
    let scenario = scenario_from_str(&args.scenario)?;
    let (config, schedule) = scenario.build(args.spm);

    let chosen_seed = if let Some(n) = args.scan_overshoot {
        eprintln!(
            "scanning {} seeds for max ramp target overshoot: algorithm={}, scenario={}, spm={}, base_seed={:#x}",
            n, algorithm.name, scenario.key(), args.spm, args.seed,
        );

        let mut scored: Vec<(f64, u64)> = Vec::with_capacity(n);
        for i in 0..n {
            let seed = args.seed.wrapping_add(i as u64);
            let clock = Arc::new(MockClock::new(0));
            let vardiff = (algorithm.factory)(clock.clone());
            let trial = run_trial_observed(vardiff, clock, config.clone(), &schedule, seed);
            scored.push((target_overshoot(&trial), seed));
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        eprintln!("\nTop 10 trials by target overshoot:");
        for (overshoot, seed) in scored.iter().take(10) {
            eprintln!("  {:>7.2}% overshoot @ seed {:#x}", overshoot * 100.0, seed);
        }

        let best_seed = scored[0].1;
        let best_overshoot = scored[0].0;
        eprintln!(
            "\nTracing seed {:#x} (max overshoot {:.2}%):\n",
            best_seed,
            best_overshoot * 100.0,
        );
        best_seed
    } else {
        eprintln!(
            "tracing: algorithm={}, scenario={}, spm={}, seed={:#x}",
            algorithm.name, scenario.key(), args.spm, args.seed,
        );
        args.seed
    };

    eprintln!(
        "  duration={}s, initial_hashrate={:.3e}, true_hashrate_at_end={:.3e}",
        config.duration_secs,
        config.initial_hashrate,
        schedule.at(config.duration_secs),
    );

    let clock = Arc::new(MockClock::new(0));
    let vardiff = (algorithm.factory)(clock.clone());
    let trial = run_trial_observed(vardiff, clock, config.clone(), &schedule, chosen_seed);

    // Header.
    println!(
        "{:>5} {:>4} {:>6} {:>12} {:>10} {:>10} {:>12} {:>6} {:>12} {:>11}",
        "tick", "t", "shares", "current_h", "delta", "thresh", "h_estimate", "fired", "new_h", "true_h_now"
    );
    println!("{}", "-".repeat(106));

    let true_h_end = trial.true_hashrate_at_end as f64;
    let mut peak_h_estimate: f64 = f64::NEG_INFINITY;
    let mut peak_h_estimate_tick = 0usize;
    let mut peak_new_hashrate: f64 = f64::NEG_INFINITY;
    let mut peak_new_hashrate_tick = 0usize;
    for (i, t) in trial.ticks.iter().enumerate() {
        let true_h_now = schedule.at(t.t_secs);
        if let Some(h) = t.h_estimate.map(|h| h as f64) {
            if h > peak_h_estimate {
                peak_h_estimate = h;
                peak_h_estimate_tick = i + 1;
            }
        }
        if let Some(h) = t.new_hashrate.map(|h| h as f64) {
            if h > peak_new_hashrate {
                peak_new_hashrate = h;
                peak_new_hashrate_tick = i + 1;
            }
        }
        println!(
            "{:>5} {:>4} {:>6} {:>12.3e} {:>10} {:>10} {:>12} {:>6} {:>12} {:>11.3e}",
            i + 1,
            t.t_secs,
            t.n_shares,
            t.current_hashrate_before,
            fmt_pct_opt(t.delta),
            fmt_pct_opt(t.threshold),
            fmt_hashrate_opt(t.h_estimate),
            if t.fired { "YES" } else { "" },
            fmt_hashrate_opt(t.new_hashrate),
            true_h_now,
        );
    }

    println!();
    println!("Summary:");
    println!("  total fires:                  {}", trial.fire_count());
    println!("  final hashrate:               {:.3e}", trial.final_hashrate);
    println!("  true_hashrate_at_end:         {:.3e}", trial.true_hashrate_at_end);
    if peak_h_estimate.is_finite() {
        let ratio = peak_h_estimate / true_h_end;
        println!(
            "  peak h_estimate:              {:.3e} at tick {} ({:.1}% overshoot)",
            peak_h_estimate,
            peak_h_estimate_tick,
            (ratio - 1.0) * 100.0,
        );
    }
    if peak_new_hashrate.is_finite() {
        let ratio = peak_new_hashrate / true_h_end;
        println!(
            "  peak new_hashrate (target):   {:.3e} at tick {} ({:.1}% overshoot)",
            peak_new_hashrate,
            peak_new_hashrate_tick,
            (ratio - 1.0).max(0.0) * 100.0,
        );
    }
    println!(
        "  settled accuracy:             {:.4} ({:.2}%)",
        (trial.final_hashrate as f64 / true_h_end - 1.0).abs(),
        (trial.final_hashrate as f64 / true_h_end - 1.0).abs() * 100.0,
    );

    Ok(())
}

fn fmt_pct_opt(v: Option<f64>) -> String {
    match v {
        None => "—".to_string(),
        Some(f) => format!("{:.2}%", f),
    }
}

fn fmt_hashrate_opt(v: Option<f32>) -> String {
    match v {
        None => "—".to_string(),
        Some(h) => format!("{:.3e}", h),
    }
}
