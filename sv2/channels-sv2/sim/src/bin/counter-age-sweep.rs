//! Counter-age characterization: measures reaction time as a function
//! of how long the algorithm has been settled before a step change.
//!
//! This directly addresses the calibration gap: the default grid only
//! tests with a ~5-min counter, while real pools spend most time with
//! counters aged 20-120+ minutes.
//!
//! ## Usage
//!
//! ```text
//! cargo run --release --bin counter-age-sweep
//! ```
//!
//! ## Configuration via environment
//!
//! - `VARDIFF_COUNTER_AGE_TRIALS` — trials per cell (default 1000).
//! - `VARDIFF_COUNTER_AGE_SEED` — base seed (default
//!   `0xDEAD_BEEF_CAFE_F00D`).
//! - `VARDIFF_COUNTER_AGE_OUT_DIR` — output directory (default `.`).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{
    serialize_markdown, serialize_toml, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT,
};
use vardiff_sim::grid::Grid;

fn main() -> std::io::Result<()> {
    let trial_count: usize = env_or("VARDIFF_COUNTER_AGE_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_COUNTER_AGE_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_COUNTER_AGE_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    let mut grid = Grid::settled_step();
    grid.trial_count = trial_count;
    grid.base_seed = base_seed;

    let cells = grid.cells();
    eprintln!(
        "Counter-age sweep: {} cells × {} trials = {} total trials",
        cells.len(),
        trial_count,
        cells.len() * trial_count,
    );

    let started = Instant::now();
    let results_by_algo = grid.run();
    let elapsed = started.elapsed();
    eprintln!("Sweep complete in {:.2}s", elapsed.as_secs_f64());

    fs::create_dir_all(&out_dir)?;

    for (algo_name, results) in &results_by_algo {
        let toml = serialize_toml(results, algo_name, trial_count, base_seed);
        let md = serialize_markdown(results, algo_name, trial_count, base_seed);

        let toml_path = out_dir.join(format!("counter_age_{}.toml", algo_name));
        let md_path = out_dir.join(format!("counter_age_{}.md", algo_name));
        fs::write(&toml_path, &toml)?;
        fs::write(&md_path, &md)?;
        eprintln!("Wrote {}", toml_path.display());
        eprintln!("Wrote {}", md_path.display());
    }

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
