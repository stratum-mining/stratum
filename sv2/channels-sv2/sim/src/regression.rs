//! Regression testing against a checked-in baseline.
//!
//! Loads the committed `vardiff_baseline.toml`, re-runs the same characterization
//! grid against the current algorithm, and asserts each metric is within
//! tolerance of the recorded baseline. Catches:
//!
//! - Changes to `VardiffState` that affect behavior
//! - Changes to the simulation that affect trial outcomes
//! - Changes to metric computation that change reported values
//! - Changes to the cell grid in [`crate::baseline`] that perturb measurements
//!
//! ## Tolerance policy
//!
//! The thresholds derive from the design proposal's CI assertion policy:
//!
//! - **Convergence rate**: `current >= baseline - 0.01`
//! - **Convergence p90**: `current <= baseline * 1.10`
//! - **Settled accuracy p50 / p90**: `current <= baseline * 1.15`
//! - **Jitter p50**: `current <= baseline + 0.02` (absolute; baseline can be near zero)
//! - **Jitter p95**: `current <= baseline * 1.25`
//! - **Reaction rate**: `current >= baseline - 0.02`
//! - **Reaction p50**: `current <= baseline * 1.20`
//! - **Sensitivity at large |Δ| (|Δ| >= 50%)**: `current >= baseline - 0.02`
//! - **Sensitivity at small |Δ| (|Δ| <= 5%)**: `current <= baseline + 0.05`
//!
//! Mid-range deltas (10-25%) are reported in the baseline but not asserted on
//! — they're where legitimate algorithmic tradeoffs live, and a reviewer
//! should look at the full delta in PR review rather than the test pretending
//! these are pass/fail.
//!
//! ## Running
//!
//! The regression test is `#[ignore]`-d by default because it runs the full
//! ~5-second baseline sweep on every invocation. Run it explicitly:
//!
//! ```text
//! cargo test --release -- --ignored
//! ```

use std::collections::HashMap;

/// Parsed baseline document — the in-memory representation of
/// `vardiff_baseline.toml`.
#[derive(Debug, Clone)]
pub struct BaselineDoc {
    pub meta: BaselineMeta,
    /// Keyed by the full cell key (e.g., `spm_12.stable_1ph`).
    pub cells: HashMap<String, CellBaseline>,
}

#[derive(Debug, Clone)]
pub struct BaselineMeta {
    pub algorithm: String,
    pub trial_count: usize,
    pub base_seed: u64,
}

/// Subset of `CellResult` fields tracked in the baseline. All metric values
/// are `Option<f64>` because the emitter omits absent fields (e.g.,
/// reaction_* on non-Step scenarios, percentile fields on empty distributions).
#[derive(Debug, Clone, Default)]
pub struct CellBaseline {
    pub shares_per_minute: f32,
    pub scenario: String,
    pub convergence_rate: Option<f64>,
    pub convergence_p90_secs: Option<f64>,
    pub settled_accuracy_p50: Option<f64>,
    pub settled_accuracy_p90: Option<f64>,
    pub jitter_p50_per_min: Option<f64>,
    pub jitter_p95_per_min: Option<f64>,
    pub reaction_rate: Option<f64>,
    pub reaction_p50_secs: Option<f64>,
}

/// A single tolerance violation found by comparing a current measurement
/// against the baseline.
#[derive(Debug, Clone)]
pub struct Discrepancy {
    pub cell_key: String,
    pub metric: String,
    pub baseline: f64,
    pub current: f64,
    pub tolerance: String,
}

impl std::fmt::Display for Discrepancy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} = {:.4} vs baseline {:.4} (tolerance: {})",
            self.cell_key, self.metric, self.current, self.baseline, self.tolerance,
        )
    }
}

/// Result of comparing a fresh baseline run against the checked-in document.
#[derive(Debug, Default)]
pub struct ComparisonReport {
    pub failures: Vec<Discrepancy>,
    pub baseline_cells_not_in_current: Vec<String>,
    pub current_cells_not_in_baseline: Vec<String>,
}

impl ComparisonReport {
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
            && self.baseline_cells_not_in_current.is_empty()
            && self.current_cells_not_in_baseline.is_empty()
    }
}

// ============================================================================
// TOML parsing
// ============================================================================

/// Parses a baseline TOML document.
///
/// Implements the small subset of TOML our serializer emits: top-level
/// `[meta]` section, per-cell `[cell.spm_X.SCENARIO]` sections, key-value
/// pairs with integer / float / quoted-string / hex-integer values, and
/// `#` comments. Not a general TOML parser — intentionally limited to keep
/// the regression-testing infrastructure dependency-free.
pub fn parse_baseline_toml(input: &str) -> Result<BaselineDoc, ParseError> {
    let mut current_section: Option<String> = None;
    let mut meta_kv: HashMap<String, RawValue> = HashMap::new();
    let mut cells_kv: HashMap<String, HashMap<String, RawValue>> = HashMap::new();

    for (line_no, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current_section = Some(section.to_string());
            continue;
        }
        let (key, raw_value) = line
            .split_once('=')
            .ok_or_else(|| ParseError::MalformedLine(line_no + 1, line.to_string()))?;
        let key = key.trim().to_string();
        let value = parse_value(raw_value.trim()).ok_or_else(|| {
            ParseError::MalformedValue(line_no + 1, raw_value.trim().to_string())
        })?;
        match current_section.as_deref() {
            Some("meta") => {
                meta_kv.insert(key, value);
            }
            Some(s) if s.starts_with("cell.") => {
                let cell_key = s.strip_prefix("cell.").unwrap().to_string();
                cells_kv.entry(cell_key).or_default().insert(key, value);
            }
            Some(other) => {
                return Err(ParseError::UnknownSection(line_no + 1, other.to_string()));
            }
            None => {
                return Err(ParseError::OrphanKey(line_no + 1, key));
            }
        }
    }

    let meta = BaselineMeta {
        algorithm: meta_kv
            .get("algorithm")
            .and_then(RawValue::as_string)
            .ok_or(ParseError::MissingMetaKey("algorithm"))?,
        trial_count: meta_kv
            .get("trial_count")
            .and_then(RawValue::as_int)
            .ok_or(ParseError::MissingMetaKey("trial_count"))? as usize,
        base_seed: meta_kv
            .get("base_seed")
            .and_then(RawValue::as_u64)
            .ok_or(ParseError::MissingMetaKey("base_seed"))?,
    };

    let mut cells = HashMap::with_capacity(cells_kv.len());
    for (key, kv) in cells_kv {
        let cell = CellBaseline {
            shares_per_minute: kv
                .get("shares_per_minute")
                .and_then(RawValue::as_float)
                .ok_or(ParseError::MissingCellKey(key.clone(), "shares_per_minute"))?
                as f32,
            scenario: kv
                .get("scenario")
                .and_then(RawValue::as_string)
                .ok_or(ParseError::MissingCellKey(key.clone(), "scenario"))?,
            convergence_rate: kv.get("convergence_rate").and_then(RawValue::as_float),
            convergence_p90_secs: kv.get("convergence_p90_secs").and_then(RawValue::as_float),
            settled_accuracy_p50: kv.get("settled_accuracy_p50").and_then(RawValue::as_float),
            settled_accuracy_p90: kv.get("settled_accuracy_p90").and_then(RawValue::as_float),
            jitter_p50_per_min: kv.get("jitter_p50_per_min").and_then(RawValue::as_float),
            jitter_p95_per_min: kv.get("jitter_p95_per_min").and_then(RawValue::as_float),
            reaction_rate: kv.get("reaction_rate").and_then(RawValue::as_float),
            reaction_p50_secs: kv.get("reaction_p50_secs").and_then(RawValue::as_float),
        };
        cells.insert(key, cell);
    }

    Ok(BaselineDoc { meta, cells })
}

#[derive(Debug, Clone)]
enum RawValue {
    Int(i64),
    /// Unsigned integer wider than `i64::MAX` (e.g., `base_seed` which can be
    /// any `u64`). Stored separately so `as_u64` is exact rather than going
    /// through `f64`, which loses precision past 2^53.
    Uint(u64),
    Float(f64),
    Str(String),
}

impl RawValue {
    fn as_float(&self) -> Option<f64> {
        match self {
            RawValue::Int(i) => Some(*i as f64),
            RawValue::Uint(u) => Some(*u as f64),
            RawValue::Float(f) => Some(*f),
            RawValue::Str(_) => None,
        }
    }
    fn as_int(&self) -> Option<i64> {
        match self {
            RawValue::Int(i) => Some(*i),
            RawValue::Uint(u) if *u <= i64::MAX as u64 => Some(*u as i64),
            _ => None,
        }
    }
    fn as_u64(&self) -> Option<u64> {
        match self {
            RawValue::Uint(u) => Some(*u),
            RawValue::Int(i) if *i >= 0 => Some(*i as u64),
            _ => None,
        }
    }
    fn as_string(&self) -> Option<String> {
        match self {
            RawValue::Str(s) => Some(s.clone()),
            _ => None,
        }
    }
}

fn parse_value(s: &str) -> Option<RawValue> {
    if let Some(quoted) = s.strip_prefix('"').and_then(|q| q.strip_suffix('"')) {
        return Some(RawValue::Str(quoted.to_string()));
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        // Use u64::from_str_radix so values up to u64::MAX (e.g., seeds with
        // the high bit set) parse exactly. Conversion to i64 happens later
        // only if a caller specifically requests `as_int`.
        return u64::from_str_radix(hex, 16).ok().map(RawValue::Uint);
    }
    if let Ok(i) = s.parse::<i64>() {
        return Some(RawValue::Int(i));
    }
    // Decimal integer that exceeds i64::MAX but fits in u64 (e.g., the default
    // base_seed when emitted as decimal).
    if let Ok(u) = s.parse::<u64>() {
        return Some(RawValue::Uint(u));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Some(RawValue::Float(f));
    }
    None
}

#[derive(Debug, Clone)]
pub enum ParseError {
    MalformedLine(usize, String),
    MalformedValue(usize, String),
    UnknownSection(usize, String),
    OrphanKey(usize, String),
    MissingMetaKey(&'static str),
    MissingCellKey(String, &'static str),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::MalformedLine(l, s) => write!(f, "line {l}: malformed line {s:?}"),
            ParseError::MalformedValue(l, s) => write!(f, "line {l}: malformed value {s:?}"),
            ParseError::UnknownSection(l, s) => write!(f, "line {l}: unknown section [{s}]"),
            ParseError::OrphanKey(l, k) => write!(f, "line {l}: key {k:?} outside any section"),
            ParseError::MissingMetaKey(k) => write!(f, "missing required meta key {k:?}"),
            ParseError::MissingCellKey(c, k) => {
                write!(f, "cell {c:?}: missing required key {k:?}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

// ============================================================================
// Comparison
// ============================================================================

use crate::baseline::CellResult;

/// Compares a fresh baseline run against the checked-in baseline. Per-metric
/// tolerances are encoded directly in this function — they implement the
/// policy documented at the top of this module.
pub fn compare_to_baseline(current: &[CellResult], baseline: &BaselineDoc) -> ComparisonReport {
    let mut report = ComparisonReport::default();

    let current_keys: HashMap<String, &CellResult> = current
        .iter()
        .map(|r| {
            (
                format!("spm_{}.{}", r.shares_per_minute as u32, r.scenario_key),
                r,
            )
        })
        .collect();

    for (key, b) in &baseline.cells {
        let Some(c) = current_keys.get(key) else {
            report.baseline_cells_not_in_current.push(key.clone());
            continue;
        };

        // Convergence
        compare_min(
            &mut report,
            key,
            "convergence_rate",
            b.convergence_rate,
            Some(c.convergence_rate),
            0.01,
            "current >= baseline - 0.01",
        );
        compare_max_mul(
            &mut report,
            key,
            "convergence_p90_secs",
            b.convergence_p90_secs,
            c.convergence_p90_secs,
            1.10,
            "current <= baseline * 1.10",
        );

        // Settled accuracy
        compare_max_mul(
            &mut report,
            key,
            "settled_accuracy_p50",
            b.settled_accuracy_p50,
            c.settled_accuracy_p50,
            1.15,
            "current <= baseline * 1.15",
        );
        compare_max_mul(
            &mut report,
            key,
            "settled_accuracy_p90",
            b.settled_accuracy_p90,
            c.settled_accuracy_p90,
            1.15,
            "current <= baseline * 1.15",
        );

        // Jitter — absolute tolerance on p50 (baseline can be near zero),
        // multiplicative on p95.
        compare_max_abs(
            &mut report,
            key,
            "jitter_p50_per_min",
            b.jitter_p50_per_min,
            c.jitter_p50_per_min,
            0.02,
            "current <= baseline + 0.02",
        );
        compare_max_mul(
            &mut report,
            key,
            "jitter_p95_per_min",
            b.jitter_p95_per_min,
            c.jitter_p95_per_min,
            1.25,
            "current <= baseline * 1.25",
        );

        // Reaction (only present on Step scenarios)
        if let (Some(_), Some(_)) = (b.reaction_rate, c.reaction_rate) {
            // Sensitivity assertions split by Δ magnitude.
            //
            // For |Δ| >= 50% (the "must respond" floor) we assert the rate
            // is at least baseline - 0.02 — catches an algorithm that fails
            // to fire on genuine large changes.
            //
            // For |Δ| <= 5% (the "must not fire on noise" ceiling) we assert
            // the rate is at most baseline + 0.05 — catches an algorithm that
            // fires too eagerly on noise.
            //
            // For mid-range Δ (10-25%) we don't assert — that's where the
            // legitimate algorithmic tradeoffs live and a reviewer should
            // judge by looking at the full delta.
            let abs_delta = extract_delta_magnitude(&b.scenario);
            match abs_delta {
                Some(d) if d >= 50 => {
                    compare_min(
                        &mut report,
                        key,
                        "reaction_rate",
                        b.reaction_rate,
                        c.reaction_rate,
                        0.02,
                        "current >= baseline - 0.02 (|Δ|>=50%)",
                    );
                }
                Some(d) if d <= 5 => {
                    compare_max_abs(
                        &mut report,
                        key,
                        "reaction_rate",
                        b.reaction_rate,
                        c.reaction_rate,
                        0.05,
                        "current <= baseline + 0.05 (|Δ|<=5%)",
                    );
                }
                _ => { /* mid-range: not asserted */ }
            }

            // Reaction time p50 — slower is a regression.
            compare_max_mul(
                &mut report,
                key,
                "reaction_p50_secs",
                b.reaction_p50_secs,
                c.reaction_p50_secs,
                1.20,
                "current <= baseline * 1.20",
            );
        }
    }

    for key in current_keys.keys() {
        if !baseline.cells.contains_key(key) {
            report.current_cells_not_in_baseline.push(key.clone());
        }
    }

    report
}

/// Asserts `current >= baseline - abs_tolerance`. Records a discrepancy if not.
fn compare_min(
    report: &mut ComparisonReport,
    cell_key: &str,
    metric: &str,
    baseline: Option<f64>,
    current: Option<f64>,
    abs_tolerance: f64,
    tolerance_desc: &str,
) {
    if let (Some(b), Some(c)) = (baseline, current) {
        if c < b - abs_tolerance {
            report.failures.push(Discrepancy {
                cell_key: cell_key.to_string(),
                metric: metric.to_string(),
                baseline: b,
                current: c,
                tolerance: tolerance_desc.to_string(),
            });
        }
    }
}

/// Asserts `current <= baseline * mul_tolerance`. Skips if baseline is zero
/// (multiplicative tolerance is meaningless there — use `compare_max_abs`).
fn compare_max_mul(
    report: &mut ComparisonReport,
    cell_key: &str,
    metric: &str,
    baseline: Option<f64>,
    current: Option<f64>,
    mul_tolerance: f64,
    tolerance_desc: &str,
) {
    if let (Some(b), Some(c)) = (baseline, current) {
        if b == 0.0 {
            // Fall back to a small absolute tolerance — multiplying zero by
            // any factor still yields zero.
            if c > 0.01 {
                report.failures.push(Discrepancy {
                    cell_key: cell_key.to_string(),
                    metric: metric.to_string(),
                    baseline: b,
                    current: c,
                    tolerance: format!("{} (baseline was 0; current must be ≤ 0.01)", tolerance_desc),
                });
            }
            return;
        }
        if c > b * mul_tolerance {
            report.failures.push(Discrepancy {
                cell_key: cell_key.to_string(),
                metric: metric.to_string(),
                baseline: b,
                current: c,
                tolerance: tolerance_desc.to_string(),
            });
        }
    }
}

/// Asserts `current <= baseline + abs_tolerance`.
fn compare_max_abs(
    report: &mut ComparisonReport,
    cell_key: &str,
    metric: &str,
    baseline: Option<f64>,
    current: Option<f64>,
    abs_tolerance: f64,
    tolerance_desc: &str,
) {
    if let (Some(b), Some(c)) = (baseline, current) {
        if c > b + abs_tolerance {
            report.failures.push(Discrepancy {
                cell_key: cell_key.to_string(),
                metric: metric.to_string(),
                baseline: b,
                current: c,
                tolerance: tolerance_desc.to_string(),
            });
        }
    }
}

/// Extracts the absolute delta from a `step_plus_NN_at_15min` or
/// `step_minus_NN_at_15min` scenario key. Returns `None` for non-step
/// scenarios.
fn extract_delta_magnitude(scenario_key: &str) -> Option<u32> {
    let rest = scenario_key
        .strip_prefix("step_plus_")
        .or_else(|| scenario_key.strip_prefix("step_minus_"))?;
    let num = rest.split('_').next()?;
    num.parse::<u32>().ok()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::{default_cells, run_baseline};

    #[test]
    fn parser_handles_minimal_document() {
        let toml = r#"
# A comment
[meta]
algorithm = "VardiffState"
trial_count = 1000
base_seed = 0xDEADBEEF

[cell.spm_12.stable_1ph]
shares_per_minute = 12
scenario = "stable_1ph"
convergence_rate = 0.95
jitter_p50_per_min = 0.04
        "#;
        let doc = parse_baseline_toml(toml).expect("should parse");
        assert_eq!(doc.meta.algorithm, "VardiffState");
        assert_eq!(doc.meta.trial_count, 1000);
        assert_eq!(doc.meta.base_seed, 0xDEADBEEF);
        let cell = &doc.cells["spm_12.stable_1ph"];
        assert_eq!(cell.shares_per_minute, 12.0);
        assert_eq!(cell.scenario, "stable_1ph");
        assert_eq!(cell.convergence_rate, Some(0.95));
        assert_eq!(cell.jitter_p50_per_min, Some(0.04));
        assert_eq!(cell.reaction_rate, None);
    }

    #[test]
    fn parser_handles_u64_base_seed_beyond_i64_max() {
        // 0xDEADBEEFCAFEF00D = 16045690984503111693, exceeds i64::MAX.
        // Tests both the hex path and the decimal path.
        let hex_toml = r#"
[meta]
algorithm = "VardiffState"
trial_count = 1000
base_seed = 0xDEADBEEFCAFEF00D
        "#;
        let doc = parse_baseline_toml(hex_toml).expect("hex base_seed should parse");
        assert_eq!(doc.meta.base_seed, 0xDEAD_BEEF_CAFE_F00D);

        let decimal_toml = r#"
[meta]
algorithm = "VardiffState"
trial_count = 1000
base_seed = 16045690984503111693
        "#;
        let doc = parse_baseline_toml(decimal_toml).expect("large decimal base_seed should parse");
        assert_eq!(doc.meta.base_seed, 16045690984503111693u64);
    }

    #[test]
    fn parser_rejects_orphan_key() {
        let toml = "orphan = 1\n";
        assert!(matches!(
            parse_baseline_toml(toml).unwrap_err(),
            ParseError::OrphanKey(_, _)
        ));
    }

    #[test]
    fn parser_rejects_missing_required_meta_keys() {
        let toml = "[meta]\nalgorithm = \"X\"\n";
        assert!(matches!(
            parse_baseline_toml(toml).unwrap_err(),
            ParseError::MissingMetaKey("trial_count")
        ));
    }

    #[test]
    fn extract_delta_magnitude_parses_step_scenarios() {
        assert_eq!(extract_delta_magnitude("step_minus_50_at_15min"), Some(50));
        assert_eq!(extract_delta_magnitude("step_plus_10_at_15min"), Some(10));
        assert_eq!(extract_delta_magnitude("stable_1ph"), None);
        assert_eq!(extract_delta_magnitude("cold_start_10gh_to_1ph"), None);
    }

    #[test]
    fn comparison_reports_clean_for_identical_run() {
        // Tiny synthetic baseline; current with identical numbers should not
        // produce failures.
        let toml = r#"
[meta]
algorithm = "VardiffState"
trial_count = 1
base_seed = 1

[cell.spm_12.stable_1ph]
shares_per_minute = 12
scenario = "stable_1ph"
convergence_rate = 0.95
jitter_p50_per_min = 0.04
jitter_p95_per_min = 0.20
        "#;
        let baseline = parse_baseline_toml(toml).unwrap();
        let current = vec![CellResult {
            shares_per_minute: 12.0,
            scenario_key: "stable_1ph".to_string(),
            convergence_rate: 0.95,
            jitter_p50_per_min: Some(0.04),
            jitter_p95_per_min: Some(0.20),
            ..Default::default()
        }];
        let report = compare_to_baseline(&current, &baseline);
        assert!(report.is_clean(), "Expected clean report, got {report:#?}");
    }

    #[test]
    fn comparison_flags_convergence_rate_drop() {
        let toml = r#"
[meta]
algorithm = "VardiffState"
trial_count = 1
base_seed = 1

[cell.spm_12.stable_1ph]
shares_per_minute = 12
scenario = "stable_1ph"
convergence_rate = 0.95
        "#;
        let baseline = parse_baseline_toml(toml).unwrap();
        let current = vec![CellResult {
            shares_per_minute: 12.0,
            scenario_key: "stable_1ph".to_string(),
            convergence_rate: 0.80, // dropped by 15pp
            ..Default::default()
        }];
        let report = compare_to_baseline(&current, &baseline);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].metric, "convergence_rate");
    }

    /// The slow regression test. Runs the full 50-cell baseline at the same
    /// seed / trial_count as the checked-in baseline and asserts every cell
    /// is within tolerance. Marked `#[ignore]` because it runs ~5-15s in
    /// release mode; CI should invoke this via `cargo test --release --
    /// --ignored`.
    #[test]
    #[ignore = "slow regression test; run with `cargo test --release -- --ignored`"]
    fn classic_algorithm_no_regression() {
        let baseline_str = include_str!("../vardiff_baseline.toml");
        let baseline = parse_baseline_toml(baseline_str).expect("baseline parses");

        let cells = default_cells();
        let current = run_baseline(&cells, baseline.meta.trial_count, baseline.meta.base_seed);

        let report = compare_to_baseline(&current, &baseline);

        if !report.is_clean() {
            let mut msg = String::new();
            if !report.failures.is_empty() {
                msg.push_str(&format!("\n{} tolerance failures:\n", report.failures.len()));
                for d in &report.failures {
                    msg.push_str(&format!("  {}\n", d));
                }
            }
            if !report.baseline_cells_not_in_current.is_empty() {
                msg.push_str("\nbaseline cells not in current:\n");
                for k in &report.baseline_cells_not_in_current {
                    msg.push_str(&format!("  {}\n", k));
                }
            }
            if !report.current_cells_not_in_baseline.is_empty() {
                msg.push_str("\ncurrent cells not in baseline:\n");
                for k in &report.current_cells_not_in_baseline {
                    msg.push_str(&format!("  {}\n", k));
                }
            }
            panic!("Regression detected:{}", msg);
        }
    }
}
