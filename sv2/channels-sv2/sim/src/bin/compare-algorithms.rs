//! Runs the canonical 5 × 10 cell grid against every shipped algorithm
//! and writes a per-algorithm `(toml, md)` pair for each.
//!
//! Algorithms covered:
//! - `VardiffState` — production reference.
//! - `ClassicComposed` — four-axis decomposition of `VardiffState`.
//! - `Parametric` — Classic with `PoissonCI` boundary.
//! - `ParametricStrict` — Parametric with z=3.0 (99.7% CI).
//! - `ClassicPartialRetarget-30` — Classic with `PartialRetarget(η=0.3)`
//!   replacing `FullRetargetWithClamp`.
//! - `EWMA-60s` — `EwmaEstimator(60s)` + `PoissonCI` +
//!   `PartialRetarget(η=0.5)`.
//! - `SlidingWindow-10t` — `SlidingWindowEstimator(10)` + `PoissonCI` +
//!   `FullRetargetNoClamp`.
//! - `FullRemedy` — `EwmaEstimator(120s)` + `PoissonCI` +
//!   `PartialRetarget(η=0.3)`. The "three-axis fix" composition.
//!
//! Algorithm is a first-class grid axis, so a single command produces
//! all baselines that can be diff'd against each other to see,
//! axis-by-axis, where each algorithm wins or loses.
//!
//! ## Usage
//!
//! From the sim crate root:
//!
//! ```text
//! cargo run --release --bin compare-algorithms
//! ```
//!
//! Output files (written to the current directory by default):
//! one `baseline_{AlgorithmName}.toml` / `.md` pair per algorithm.
//!
//! ## Configuration via environment
//!
//! - `VARDIFF_COMPARE_TRIALS` — trials per cell (default 1000).
//! - `VARDIFF_COMPARE_SEED` — base seed (default
//!   `0xDEAD_BEEF_CAFE_F00D`).
//! - `VARDIFF_COMPARE_OUT_DIR` — output directory (default `.`).
//! - `VARDIFF_COMPARE_PAIRED` — when set to `1`/`true`, use
//!   `Grid::run_paired` so all algorithms see the same trial inputs
//!   per cell (smaller cross-algorithm noise, but baselines emitted
//!   here are not directly comparable to the algo-indexed ones
//!   produced by `generate-baseline`).
//!
//! ## Runtime
//!
//! 8 algos × 5 SPM × 10 scenarios × 1000 trials = 400,000 trials,
//! ~2–3 minutes at release speed. Debug mode is 5–10× slower.
//! Reduce `VARDIFF_COMPARE_TRIALS` to 50 for fast iteration.
//!
//! ## What's in the output
//!
//! Each algorithm's `.toml` carries the standard 7 metrics (convergence,
//! settled accuracy, jitter, reaction time, bias, variance, phase 1
//! overshoot) per cell where applicable. The `.md` renders each metric
//! as a table plus the decoupling-score summary at the end. For
//! observable algorithms (everything except `VardiffState`), bias /
//! variance / overshoot show real values; for `VardiffState` they're
//! omitted (the algorithm doesn't expose introspection).
//!
//! For Pareto comparison, diff the per-algorithm `.md` files:
//!
//! ```text
//! diff baseline_VardiffState.md baseline_FullRemedy.md | less
//! diff baseline_Parametric.md baseline_FullRemedy.md | less
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{
    serialize_markdown, serialize_toml, Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT,
};
use vardiff_sim::grid::{AlgorithmSpec, Grid};

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_COMPARE_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_COMPARE_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_COMPARE_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    // The canonical scenario set: cold start + stable + 8 step deltas.
    // Matches `Grid::default_classic` and the existing checked-in
    // baseline so seed derivation stays consistent.
    let mut scenarios = vec![Scenario::ColdStart, Scenario::Stable];
    for &delta in &[-50i32, -25, -10, -5, 5, 10, 25, 50] {
        scenarios.push(Scenario::Step { delta_pct: delta });
    }

    let paired = env::var("VARDIFF_COMPARE_PAIRED")
        .map(|s| matches!(s.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false);

    let grid = Grid {
        algorithms: vec![
            AlgorithmSpec::classic_vardiff_state(),
            AlgorithmSpec::classic_composed(),
            AlgorithmSpec::parametric(),
            AlgorithmSpec::parametric_strict(),
            AlgorithmSpec::classic_partial_retarget(0.3),
            AlgorithmSpec::ewma_60s(),
            AlgorithmSpec::sliding_window(10),
            AlgorithmSpec::full_remedy(),
        ],
        share_rates: vec![6.0, 12.0, 30.0, 60.0, 120.0],
        scenarios,
        trial_count,
        base_seed,
    };

    eprintln!(
        "Comparing {} algorithms × {} cells × {} trials = {} total trials, base_seed = {:#x}",
        grid.algorithms.len(),
        grid.share_rates.len() * grid.scenarios.len(),
        trial_count,
        grid.total_runs() * trial_count,
        base_seed,
    );
    eprintln!("Output directory: {}", out_dir.display());
    eprintln!(
        "Seeding mode: {}",
        if paired {
            "paired (Grid::run_paired)"
        } else {
            "algo-indexed (Grid::run)"
        }
    );

    let started = Instant::now();
    let results = if paired { grid.run_paired() } else { grid.run() };
    let elapsed = started.elapsed();
    eprintln!("Sweep complete in {:.2}s", elapsed.as_secs_f64());

    fs::create_dir_all(&out_dir)?;

    // Sorted for stable output ordering across runs.
    let mut algo_names: Vec<&String> = results.keys().collect();
    algo_names.sort();

    for name in algo_names {
        let cells = &results[name];
        let toml_path = out_dir.join(format!("baseline_{}.toml", name));
        let md_path = out_dir.join(format!("baseline_{}.md", name));
        fs::write(
            &toml_path,
            serialize_toml(cells, name, trial_count, base_seed),
        )?;
        fs::write(
            &md_path,
            serialize_markdown(cells, name, trial_count, base_seed),
        )?;
        eprintln!("  {}", toml_path.display());
        eprintln!("  {}", md_path.display());
    }

    eprintln!("\nDone. Use `diff baseline_X.md baseline_Y.md` for axis-by-axis comparison.");
    Ok(())
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
