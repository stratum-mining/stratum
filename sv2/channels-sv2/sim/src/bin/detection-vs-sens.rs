//! Does gentler (higher-sensitivity) firing give up the high-r* detection
//! lead? Detection EXCESS = P[fire≤15min|−10% drop] − P[fire≤15min|no drop]
//! for the champion family across sensitivity, at 60 spm (where detection
//! discriminates). The production re-score wants gentler; this checks the
//! cost of gentler on the detection axis the high-r* regime cares about.
use std::sync::Arc;
use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

fn family(sens: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("s{sens}"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(150),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(sens, 0.05, 6.0, 0.06, 0.6), 6),
            AcceleratingPartialRetarget::new(0.2, 0.8, 0.05), 1.0, clock,
        )))
    })
}
fn frac(mk: &AlgorithmSpec, spm: f32, delta: i32, w: u64, seed: u64, n: usize) -> f64 {
    let (cfg, sched) = Scenario::SettledStep { settle_minutes: 60, delta_pct: delta }.build(spm);
    let config = TrialConfig { tick_interval_secs: 60, ..cfg };
    let event = 60 * 60u64;
    let mut fired = 0u32;
    for i in 0..n {
        let clock = Arc::new(MockClock::new(0));
        let v = (mk.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        if t.ticks.iter().any(|tk| tk.fired && tk.t_secs > event && tk.t_secs <= event + w) { fired += 1; }
    }
    fired as f64 / n as f64
}
fn main() {
    let n = 2000usize;
    let w = 15 * 60u64;
    println!("## Detection EXCESS vs sensitivity, 60 spm, 15-min window, {} trials.", n);
    println!("Higher EXCESS = better small-drop detection. Production re-score wants HIGH sens (gentle); does it cost detection?\n");
    println!("| sens | P[fire|−10%] | P[fire|no drop] | EXCESS |");
    println!("| --- | --- | --- | --- |");
    for s in [0.3f64, 0.6, 1.0, 1.5, 2.0] {
        let a = family(s);
        let det = frac(&a, 60.0, -10, w, DEFAULT_BASELINE_SEED, n);
        let ctrl = frac(&a, 60.0, 0, w, DEFAULT_BASELINE_SEED ^ 0x9E37, n);
        println!("| {} | {:.3} | {:.3} | {:+.3} |", s, det, ctrl, det - ctrl);
    }
    println!("\n(s0.3 = current champion. If EXCESS falls steeply with sens, gentler trades away");
    println!("the high-r* detection lead — the two-regime tension to document.)");
}
