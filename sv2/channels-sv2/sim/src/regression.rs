//! Regression testing against a checked-in baseline.
//!
//! Loads a committed baseline TOML, re-runs the same characterization
//! grid against the current algorithm, and asserts each metric is
//! within tolerance of the recorded value.
//!
//! ## Registry-driven comparator
//!
//! Tolerance policy lives on each [`crate::metrics::Metric`] impl —
//! `compare_to_baseline` iterates [`crate::metrics::registry`], pulls
//! [`crate::metrics::Metric::tolerance_checks`] for each cell, and
//! applies them uniformly. Adding a metric is one new `impl Metric`
//! with no edits here.
//!
//! The parser is correspondingly uniform: it reads every numeric cell
//! field into a flat `HashMap<String, f64>` and lets each metric
//! decide which keys it cares about, rather than parsing into a typed
//! `CellBaseline` struct with per-metric fields.

use std::collections::HashMap;

use crate::baseline::{Cell, CellResult, Scenario};
use crate::metrics;

/// Parsed baseline document — the in-memory representation of a
/// `baseline_<AlgorithmName>.toml` file.
#[derive(Debug, Clone)]
pub struct BaselineDoc {
    pub meta: BaselineMeta,
    /// Keyed by the full cell key (e.g., `spm_12.stable_1ph`).
    pub cells: HashMap<String, CellBaseline>,
    /// Derived-metric values, keyed by `<metric_id>.spm_<X>` (e.g.,
    /// `decoupling_score.spm_6`). Each value is a flat key→f64 map of
    /// the derived metric's outputs at that share rate.
    pub derived: HashMap<String, HashMap<String, f64>>,
}

#[derive(Debug, Clone)]
pub struct BaselineMeta {
    pub algorithm: String,
    pub trial_count: usize,
    pub base_seed: u64,
}

/// A single cell's baseline values. Carries a flat `raw` map of
/// every numeric field in the TOML; the metric impls decide which
/// keys they want to compare. The two cell-coordinate fields
/// (`shares_per_minute`, `scenario`) are extracted explicitly so the
/// comparator can reconstruct a [`Cell`] for `metric.applies_to(...)`
/// and `metric.tolerance_checks(...)`.
#[derive(Debug, Clone, Default)]
pub struct CellBaseline {
    pub shares_per_minute: f32,
    pub scenario: String,
    /// Every numeric (`f64`) field from this cell's TOML section,
    /// keyed by its TOML name (`convergence_rate`,
    /// `jitter_p50_per_min`, etc.). Looked up by metric tolerance
    /// checks.
    pub raw: HashMap<String, f64>,
}

/// A single tolerance violation found by comparing a current
/// measurement against the baseline.
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

/// Parses a baseline TOML document. Implements the small subset of
/// TOML our serializer emits.
pub fn parse_baseline_toml(input: &str) -> Result<BaselineDoc, ParseError> {
    let mut current_section: Option<String> = None;
    let mut meta_kv: HashMap<String, RawValue> = HashMap::new();
    let mut cells_kv: HashMap<String, HashMap<String, RawValue>> = HashMap::new();
    let mut derived_kv: HashMap<String, HashMap<String, RawValue>> = HashMap::new();

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
        let value = parse_value(raw_value.trim())
            .ok_or_else(|| ParseError::MalformedValue(line_no + 1, raw_value.trim().to_string()))?;
        match current_section.as_deref() {
            Some("meta") => {
                meta_kv.insert(key, value);
            }
            Some(s) if s.starts_with("cell.") => {
                let cell_key = s.strip_prefix("cell.").unwrap().to_string();
                cells_kv.entry(cell_key).or_default().insert(key, value);
            }
            Some(s) if s.starts_with("derived.") => {
                let derived_key = s.strip_prefix("derived.").unwrap().to_string();
                derived_kv.entry(derived_key).or_default().insert(key, value);
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
        let shares_per_minute = kv
            .get("shares_per_minute")
            .and_then(RawValue::as_float)
            .ok_or(ParseError::MissingCellKey(key.clone(), "shares_per_minute"))?
            as f32;
        let scenario = kv
            .get("scenario")
            .and_then(RawValue::as_string)
            .ok_or(ParseError::MissingCellKey(key.clone(), "scenario"))?;

        // Every other numeric field is shoveled into `raw` as f64. The
        // metric tolerance checks look up by key name; metrics that
        // don't care about a particular key simply don't reference it.
        let mut raw: HashMap<String, f64> = HashMap::new();
        for (k, v) in &kv {
            if k == "shares_per_minute" || k == "scenario" {
                continue;
            }
            if let Some(f) = v.as_float() {
                raw.insert(k.clone(), f);
            }
        }

        cells.insert(
            key,
            CellBaseline {
                shares_per_minute,
                scenario,
                raw,
            },
        );
    }

    // Convert derived RawValue maps to flat HashMap<String, f64>,
    // dropping non-numeric values (shouldn't occur for derived
    // sections in practice but defended against just in case).
    let derived: HashMap<String, HashMap<String, f64>> = derived_kv
        .into_iter()
        .map(|(k, kv)| {
            let flat: HashMap<String, f64> = kv
                .into_iter()
                .filter_map(|(k2, v)| v.as_float().map(|f| (k2, f)))
                .collect();
            (k, flat)
        })
        .collect();

    Ok(BaselineDoc {
        meta,
        cells,
        derived,
    })
}

#[derive(Debug, Clone)]
enum RawValue {
    Int(i64),
    /// Unsigned integer wider than `i64::MAX` (e.g., `base_seed` which
    /// can be any `u64`).
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
        return u64::from_str_radix(hex, 16).ok().map(RawValue::Uint);
    }
    if let Ok(i) = s.parse::<i64>() {
        return Some(RawValue::Int(i));
    }
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

/// Compares a fresh baseline run against the checked-in baseline.
///
/// Iterates [`metrics::registry`] for each cell and applies the
/// declared `tolerance_checks`. Adding a new metric is one new
/// `impl Metric` in `metrics.rs` — no edit here required.
pub fn compare_to_baseline(current: &[CellResult], baseline: &BaselineDoc) -> ComparisonReport {
    let mut report = ComparisonReport::default();

    // Index current results by cell key for O(1) lookup.
    let current_by_key: HashMap<String, &CellResult> = current
        .iter()
        .map(|r| {
            (
                format!("spm_{}.{}", r.shares_per_minute as u32, r.scenario_key()),
                r,
            )
        })
        .collect();

    let registry = metrics::registry();

    for (cell_key, b) in &baseline.cells {
        let Some(c) = current_by_key.get(cell_key) else {
            report.baseline_cells_not_in_current.push(cell_key.clone());
            continue;
        };

        // Reconstruct the Cell so each metric can decide applies_to /
        // tolerance_checks. We trust the baseline's recorded
        // `shares_per_minute` and `scenario` here — if the parsed
        // scenario string doesn't map to a known Scenario variant we
        // skip the cell with a warning rather than fail (the comparator
        // is a regression check, not a schema validator).
        let Some(scenario) = Scenario::from_key(&b.scenario) else {
            // Unrecognized scenario in baseline — skip rather than panic.
            // This shouldn't happen with the checked-in baseline but
            // guards against schema drift.
            continue;
        };
        let cell = Cell {
            shares_per_minute: b.shares_per_minute,
            scenario,
        };

        for metric in &registry {
            if !metric.applies_to(&cell) {
                continue;
            }
            for check in metric.tolerance_checks(&cell) {
                let baseline_point = b.raw.get(check.key).copied();
                let ci_low_key = format!("{}_ci_low", check.key);
                let ci_high_key = format!("{}_ci_high", check.key);
                let baseline_ci_low = b.raw.get(&ci_low_key).copied();
                let baseline_ci_high = b.raw.get(&ci_high_key).copied();
                let current_val = c.get(check.key);
                if let (Some(point), Some(cv)) = (baseline_point, current_val) {
                    let bv = metrics::BaselineValue {
                        point,
                        ci_low: baseline_ci_low,
                        ci_high: baseline_ci_high,
                    };
                    if let Some(rule_desc) = check.tolerance.apply(bv, cv) {
                        report.failures.push(Discrepancy {
                            cell_key: cell_key.clone(),
                            metric: check.key.to_string(),
                            baseline: point,
                            current: cv,
                            tolerance: rule_desc,
                        });
                    }
                }
            }
        }
    }

    for cell_key in current_by_key.keys() {
        if !baseline.cells.contains_key(cell_key) {
            report.current_cells_not_in_baseline.push(cell_key.clone());
        }
    }

    // Derived metrics: iterate derived_registry(), compute current
    // values from the current cell results, look up the matching
    // baseline section, and apply per-derived-metric tolerance with
    // baseline CI bounds.
    for derived in metrics::derived_registry() {
        let current_per_spm = derived.compute(current);
        for (spm, current_mv) in current_per_spm {
            let baseline_key = format!("{}.spm_{}", derived.id(), spm as u32);
            let baseline_kv = match baseline.derived.get(&baseline_key) {
                Some(kv) => kv,
                None => continue, // baseline lacks this spm; not a failure
            };
            for check in derived.tolerance_checks(spm) {
                let current_val = current_mv.get(check.key);
                let baseline_point = baseline_kv.get(check.key).copied();
                let baseline_ci_low =
                    baseline_kv.get(&format!("{}_ci_low", check.key)).copied();
                let baseline_ci_high =
                    baseline_kv.get(&format!("{}_ci_high", check.key)).copied();
                if let (Some(point), Some(cv)) = (baseline_point, current_val) {
                    let bv = metrics::BaselineValue {
                        point,
                        ci_low: baseline_ci_low,
                        ci_high: baseline_ci_high,
                    };
                    if let Some(rule_desc) = check.tolerance.apply(bv, cv) {
                        report.failures.push(Discrepancy {
                            cell_key: baseline_key.clone(),
                            metric: check.key.to_string(),
                            baseline: point,
                            current: cv,
                            tolerance: rule_desc,
                        });
                    }
                }
            }
        }
    }

    report
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::{default_cells, run_baseline};
    use crate::metrics::MetricValues;

    /// Builder shorthand used by the comparator tests.
    fn cell_result_with(
        spm: f32,
        scenario: Scenario,
        metric_id: &'static str,
        kvs: &[(&'static str, f64)],
    ) -> CellResult {
        let mut cr = CellResult::new(spm, scenario);
        let mut mv = MetricValues::new();
        for &(k, v) in kvs {
            mv.set(k, Some(v));
        }
        cr.metrics.insert(metric_id, mv);
        cr
    }

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
        assert_eq!(cell.raw.get("convergence_rate"), Some(&0.95));
        assert_eq!(cell.raw.get("jitter_p50_per_min"), Some(&0.04));
        assert!(cell.raw.get("reaction_rate").is_none());
    }

    #[test]
    fn parser_handles_u64_base_seed_beyond_i64_max() {
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
    fn comparison_reports_clean_for_identical_run() {
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

        let mut cr = CellResult::new(12.0, Scenario::Stable);
        let mut conv = MetricValues::new();
        conv.set("convergence_rate", Some(0.95));
        cr.metrics.insert("convergence_time", conv);
        let mut jit = MetricValues::new();
        jit.set("jitter_p50_per_min", Some(0.04));
        jit.set("jitter_p95_per_min", Some(0.20));
        cr.metrics.insert("jitter", jit);

        let report = compare_to_baseline(&[cr], &baseline);
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
        let cr = cell_result_with(
            12.0,
            Scenario::Stable,
            "convergence_time",
            &[("convergence_rate", 0.80)], // dropped by 15pp
        );
        let report = compare_to_baseline(&[cr], &baseline);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].metric, "convergence_rate");
    }

    #[test]
    fn comparison_flags_jitter_increase_beyond_absolute_tolerance() {
        let toml = r#"
[meta]
algorithm = "VardiffState"
trial_count = 1
base_seed = 1

[cell.spm_12.stable_1ph]
shares_per_minute = 12
scenario = "stable_1ph"
jitter_p50_per_min = 0.04
        "#;
        let baseline = parse_baseline_toml(toml).unwrap();
        let cr = cell_result_with(
            12.0,
            Scenario::Stable,
            "jitter",
            &[("jitter_p50_per_min", 0.10)], // 0.04 + 0.06 > 0.04 + 0.02
        );
        let report = compare_to_baseline(&[cr], &baseline);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].metric, "jitter_p50_per_min");
    }

    #[test]
    fn comparison_flags_reaction_rate_drop_at_large_delta() {
        let toml = r#"
[meta]
algorithm = "VardiffState"
trial_count = 1
base_seed = 1

[cell.spm_12.step_minus_50_at_15min]
shares_per_minute = 12
scenario = "step_minus_50_at_15min"
reaction_rate = 0.90
        "#;
        let baseline = parse_baseline_toml(toml).unwrap();
        let cr = cell_result_with(
            12.0,
            Scenario::Step { delta_pct: -50 },
            "reaction_time",
            &[("reaction_rate", 0.80)], // -0.10 vs -0.02 tolerance
        );
        let report = compare_to_baseline(&[cr], &baseline);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].metric, "reaction_rate");
    }

    #[test]
    fn comparison_does_not_flag_mid_range_reaction_rate_changes() {
        // |Δ| = 25 is the mid-range with no rate assertion. A 0.20pp
        // drop is large but should not fail.
        let toml = r#"
[meta]
algorithm = "VardiffState"
trial_count = 1
base_seed = 1

[cell.spm_12.step_minus_25_at_15min]
shares_per_minute = 12
scenario = "step_minus_25_at_15min"
reaction_rate = 0.90
        "#;
        let baseline = parse_baseline_toml(toml).unwrap();
        let cr = cell_result_with(
            12.0,
            Scenario::Step { delta_pct: -25 },
            "reaction_time",
            &[("reaction_rate", 0.70)],
        );
        let report = compare_to_baseline(&[cr], &baseline);
        assert!(
            report.failures.is_empty(),
            "mid-range Δ should not gate; got failures: {:#?}",
            report.failures
        );
    }

    #[test]
    fn comparison_reports_missing_cells_on_both_sides() {
        let toml = r#"
[meta]
algorithm = "VardiffState"
trial_count = 1
base_seed = 1

[cell.spm_12.stable_1ph]
shares_per_minute = 12
scenario = "stable_1ph"
        "#;
        let baseline = parse_baseline_toml(toml).unwrap();
        let cr = cell_result_with(12.0, Scenario::ColdStart, "convergence_time", &[]);
        let report = compare_to_baseline(&[cr], &baseline);
        assert_eq!(report.baseline_cells_not_in_current.len(), 1);
        assert_eq!(report.current_cells_not_in_baseline.len(), 1);
    }

    /// The slow regression test. Runs the full 50-cell baseline at the
    /// same seed / trial_count as the checked-in baseline and asserts
    /// every cell is within tolerance.
    #[test]
    #[ignore = "slow regression test; run with `cargo test --release -- --ignored`"]
    fn classic_algorithm_no_regression() {
        let baseline_str = include_str!("../baseline_VardiffState.toml");
        let baseline = parse_baseline_toml(baseline_str).expect("baseline parses");

        let cells = default_cells();
        let current = run_baseline(&cells, baseline.meta.trial_count, baseline.meta.base_seed);

        let report = compare_to_baseline(&current, &baseline);

        if !report.is_clean() {
            let mut msg = String::new();
            if !report.failures.is_empty() {
                msg.push_str(&format!(
                    "\n{} tolerance failures:\n",
                    report.failures.len()
                ));
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
