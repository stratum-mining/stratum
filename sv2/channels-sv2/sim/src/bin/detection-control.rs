//! No-drop control for the detection metric (reviewer check 1), field-wide.
//!
//! detection = P[fire within W | matured, then −g drop] has NO false-alarm
//! axis. The no-drop arm (delta_pct = 0: matured counter, NO drop) measures
//! P[fire within W | no drop] — the false-alarm baseline. The signal that
//! matters is EXCESS = detection(−g) − detection(0): the false-alarm-
//! corrected detection. If EXCESS ≈ 0, the raw number is an artifact (the
//! window straddles a scheduled settling fire, drop or no drop).
//!
//! Run across the field × spm grid, at two windows (60min as scored, and a
//! tighter 15min), and at high r* (60 spm) where Theorem 2 says detection
//! SHOULD discriminate. Decides whether the corrected detection term carries
//! any ranking signal at production rates, or only at high r*.

use std::sync::Arc;
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::AlgorithmSpec;
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

/// Fraction of trials that fired within `window_secs` after the event, for
/// a given algorithm / spm / drop. settle=60min matured counter.
fn frac_fired(
    mk: &dyn Fn() -> AlgorithmSpec,
    trials_n: usize,
    spm: f32,
    delta_pct: i32,
    settle_min: u64,
    window_secs: u64,
    base_seed: u64,
) -> f64 {
    let scen = Scenario::SettledStep { settle_minutes: settle_min, delta_pct };
    let (cfg, sched) = scen.build(spm);
    let config = TrialConfig { tick_interval_secs: 60, ..cfg };
    let event_at = settle_min * 60;
    let mut fired = 0u32;
    for i in 0..trials_n {
        let clock = Arc::new(MockClock::new(0));
        let v = (mk)().factory.clone()(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, base_seed.wrapping_add(i as u64));
        if t.ticks.iter().any(|tk| tk.fired && tk.t_secs > event_at && tk.t_secs <= event_at + window_secs) {
            fired += 1;
        }
    }
    fired as f64 / trials_n as f64
}

fn main() {
    let trials = 1000usize;
    let settle = 60u64;
    let algos: Vec<(&str, fn() -> AlgorithmSpec)> = vec![
        ("champion", AlgorithmSpec::champion),
        ("balanced", AlgorithmSpec::balanced),
        ("react_priority", AlgorithmSpec::react_priority),
        ("classic", AlgorithmSpec::classic_composed),
    ];
    // (window label, window secs)
    let windows = [("60min", 60 * 60u64), ("15min", 15 * 60u64)];
    // production rates + a high-r* row where detection should discriminate.
    let spms = [4.0f32, 6.0, 30.0, 60.0];

    for (wl, wsecs) in &windows {
        println!("\n## −10% small-drop detection EXCESS (det − no-drop control). window={}, settle=60min, {} trials.", wl, trials);
        println!("EXCESS≈0 ⇒ saturated (artifact). Positive ⇒ genuine small-drop sensitivity.\n");
        println!("| spm | algo | P[fire|−10%] | P[fire|no drop] | EXCESS |");
        println!("| --- | --- | --- | --- | --- |");
        for &spm in &spms {
            for (name, mk) in &algos {
                let det = frac_fired(mk, trials, spm, -10, settle, *wsecs, DEFAULT_BASELINE_SEED);
                let ctrl = frac_fired(mk, trials, spm, 0, settle, *wsecs, DEFAULT_BASELINE_SEED ^ 0x9E37);
                println!("| {} | {} | {:.3} | {:.3} | {:+.3} |", spm as u32, name, det, ctrl, det - ctrl);
            }
        }
    }
    println!("\nVerdict guide: if EXCESS is ~0 for ALL algos at production spm (4–6) but");
    println!("positive at 60 spm, detection is information-floor-limited at production");
    println!("rates (Theorem 2) and discriminates only at high r* — so it should be a");
    println!("separate floor-limited diagnostic, not a ranking term in the production scalar.");
}
