//! Runs the default baseline grid against the classic [`VardiffState`]
//! algorithm and writes `vardiff_baseline.toml` (machine-readable, consumed
//! by regression assertions) and `vardiff_baseline.md` (human-readable
//! summary) to the current working directory.
//!
//! ## Usage
//!
//! From the sim crate root:
//!
//! ```text
//! cargo run --release --bin generate-baseline
//! ```
//!
//! ## Configuration via environment
//!
//! - `VARDIFF_BASELINE_TRIALS` — trials per cell (default 1000). Set to a
//!   small number like 50 for fast local iteration.
//! - `VARDIFF_BASELINE_SEED` — base seed for trial derivation (default
//!   `0xDEAD_BEEF_CAFE_F00D`). Change only when re-running for variance
//!   inspection; the checked-in baseline assumes the default.
//! - `VARDIFF_BASELINE_OUT_DIR` — directory to write outputs (default `.`).
//!
//! ## Runtime
//!
//! At default 1000 trials per cell × 50 cells × ~360 µs per trial,
//! the sweep is ~18 seconds of wall clock plus a few hundred ms of
//! reporting. Use `--release` — debug-mode is 5–10× slower.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use vardiff_sim::baseline::{
    default_cells, run_baseline, serialize_markdown, serialize_toml,
    DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT,
};

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_BASELINE_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_BASELINE_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_BASELINE_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    let cells = default_cells();
    eprintln!(
        "Running baseline: {} cells × {} trials = {} total trials, base_seed = {:#x}",
        cells.len(),
        trial_count,
        cells.len() * trial_count,
        base_seed,
    );
    eprintln!("Output directory: {}", out_dir.display());

    let started = Instant::now();
    let results = run_baseline(&cells, trial_count, base_seed);
    let elapsed = started.elapsed();
    eprintln!("Baseline run complete in {:.2}s", elapsed.as_secs_f64());

    let toml = serialize_toml(&results, "VardiffState", trial_count, base_seed);
    let md = serialize_markdown(&results, "VardiffState", trial_count, base_seed);

    fs::create_dir_all(&out_dir)?;
    let toml_path = out_dir.join("vardiff_baseline.toml");
    let md_path = out_dir.join("vardiff_baseline.md");
    fs::write(&toml_path, toml)?;
    fs::write(&md_path, md)?;
    eprintln!("Wrote {}", toml_path.display());
    eprintln!("Wrote {}", md_path.display());

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
        // Accept either decimal or 0x-prefixed hex.
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            return u64::from_str_radix(hex, 16).unwrap_or(default);
        }
        return s.parse().unwrap_or(default);
    }
    default
}
