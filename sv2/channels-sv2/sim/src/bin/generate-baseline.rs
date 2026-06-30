//! Generates the tracked **CI regression-fixture** baselines over the default
//! grid and writes `baseline_<algorithm>.toml` (machine-readable, consumed by
//! the regression assertions) and `baseline_<algorithm>.md` (human-readable
//! summary) to the current working directory.
//!
//! Two fixtures are generated:
//!   - `classic`  — `classic_composed`, the comparison anchor ("the algorithm
//!     we used to run"; not the shipped default).
//!   - `champion` — `champion_composed`, the SHIPPED production algorithm
//!     (built from the production constructor directly, so the fixture pins the
//!     real shipped behavior, not a re-spelled copy of its parameters).
//!
//! Note: this grid is cold-start/stable/step only — it does NOT cover the
//! decline-safety margin (the champion's selection criterion). That margin is
//! guarded separately by the `decline_safety::champion_clears_decline_gate`
//! assertion. This fixture catches gross drift in typical behavior.
//!
//! ## Usage
//!
//! From the sim crate root (generates both fixtures):
//!
//! ```text
//! cargo run --release --bin generate-baseline
//! ```
//!
//! Restrict to one with `VARDIFF_BASELINE_ALGO=classic` (or `champion`).
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

use channels_sv2::vardiff::composed::{champion_composed, classic_composed};
use vardiff_sim::baseline::{
    default_cells, run_baseline_with, serialize_markdown, serialize_toml, CellResult,
    DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT,
};

fn main() -> std::io::Result<()> {
    let trial_count = env_or("VARDIFF_BASELINE_TRIALS", DEFAULT_TRIAL_COUNT);
    let base_seed = env_or_seed("VARDIFF_BASELINE_SEED", DEFAULT_BASELINE_SEED);
    let out_dir = env::var("VARDIFF_BASELINE_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    // `classic`, `champion`, or unset/`all` for both.
    let only = env::var("VARDIFF_BASELINE_ALGO").ok();

    let cells = default_cells();
    fs::create_dir_all(&out_dir)?;

    let want = |name: &str| only.as_deref().map_or(true, |o| o == name || o == "all");

    if want("classic") {
        let results = run_one("classic", &cells, trial_count, base_seed, |clock| {
            classic_composed(1.0, clock)
        });
        write_baseline(&out_dir, "classic", &results, trial_count, base_seed)?;
    }
    if want("champion") {
        let results = run_one("champion", &cells, trial_count, base_seed, |clock| {
            champion_composed(1.0, clock)
        });
        write_baseline(&out_dir, "champion", &results, trial_count, base_seed)?;
    }

    Ok(())
}

fn run_one<V: channels_sv2::vardiff::Vardiff>(
    label: &str,
    cells: &[vardiff_sim::baseline::Cell],
    trial_count: usize,
    base_seed: u64,
    build: impl Fn(std::sync::Arc<channels_sv2::vardiff::MockClock>) -> V + Copy,
) -> Vec<CellResult> {
    eprintln!(
        "Running {} baseline: {} cells × {} trials = {} total trials, base_seed = {:#x}",
        label,
        cells.len(),
        trial_count,
        cells.len() * trial_count,
        base_seed,
    );
    let started = Instant::now();
    let results = run_baseline_with(cells, trial_count, base_seed, build);
    eprintln!(
        "{} baseline run complete in {:.2}s",
        label,
        started.elapsed().as_secs_f64()
    );
    results
}

fn write_baseline(
    out_dir: &std::path::Path,
    algorithm: &str,
    results: &[CellResult],
    trial_count: usize,
    base_seed: u64,
) -> std::io::Result<()> {
    let toml = serialize_toml(results, algorithm, trial_count, base_seed);
    let md = serialize_markdown(results, algorithm, trial_count, base_seed);
    let toml_path = out_dir.join(format!("baseline_{algorithm}.toml"));
    let md_path = out_dir.join(format!("baseline_{algorithm}.md"));
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
