//! BELIEF-vs-OPERATING-POINT TRACE — verify the §6.1 mechanism conjunct directly:
//! does the SLOW estimator (Ewma360) suppress sparse-rate self-deepening because its
//! belief STAYS BELOW the operating point through the escape (spike-non-presentation),
//! or is the 0-vs-4 tighten-fire count confounded by the operating-point LEVEL (the
//! fast arm eases faster → lower OP → same spikes exceed it more easily)?
//!
//! ===========================================================================
//! WHY. eager-ease-mechanism gave a near-controlled comparison: at spm2 BOTH champion
//! (Ewma360) and estimator-jumpy (Ewma30) run PoissonCI (below the spm6 switch), so
//! the only difference is estimator speed — champion 0 self-deepening fires,
//! estimator-jumpy 4. That verifies the BEHAVIORAL claim (slow estimator suppresses
//! self-deepening at sparse). But a tighten-fire is definitionally belief-above-OP, and
//! the count alone can't separate "slow belief doesn't spike above OP" (the §6.1
//! mechanism) from "fast arm's OP sits lower so the same spikes exceed it." This trace
//! separates them by logging, per tick through a sparse escape, BOTH arms':
//!   belief (h_estimate), operating point (current_hashrate_before), and the GAP
//!   (belief − OP, normalized). The mechanism question: does the FAST belief rise
//!   ABOVE its OP (gap>0) while the SLOW belief stays BELOW (gap<0) — at COMPARABLE OP
//!   levels? If the fast belief crosses above OP on up-spikes while the slow doesn't,
//!   AND the OP levels are comparable, §6.1's spike-non-presentation mechanism stands.
//!   If the fast arm's OP is dramatically lower (so its crossings are an OP-level
//!   effect not a belief-spike effect), §6.1's mechanism conjunct needs softening.
//!
//! READ: report, per arm, how often belief>OP (the flip that enables a tighten-fire),
//! the median OP level (to check the confound), and the belief volatility (std of
//! tick-to-tick belief change, normalized — the direct "does it present spikes"
//! measure). Slow arm: few/no flips, LOW belief volatility. Fast arm: flips, HIGH
//! belief volatility. If volatility differs sharply AND OP levels are comparable, the
//! mechanism is belief-spike-presentation (§6.1 stands). The belief-volatility
//! comparison is the DIRECT spike-presentation measure — it doesn't go through OP at
//! all, so it's confound-free for "does the estimator present spikes."
//!
//! Usage: cargo run --release --bin belief-vs-op
//! ===========================================================================

use std::sync::Arc;
use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::schedule::HashrateSchedule;
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const SPARSE_SPM: f32 = 2.0;
const DECLINE_PPH: f32 = 40.0;
const DROP: f32 = 0.7;

fn variant(tau: u64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{tau}"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(1.5, 0.05, 8.0, 0.06, 0.6), 6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05), 1.0, clock,
        )))
    })
}

fn escape(a: &AlgorithmSpec, seed: u64) -> (vardiff_sim::trial::Trial, HashrateSchedule, u64, u64) {
    let mature = 60u64;
    let observe = 200u64;
    let rate = DECLINE_PPH / 100.0 / 60.0;
    let mut phases = vec![Phase::Hold { secs: mature * 60, h: TRUE_HASHRATE }];
    let mut dm = 0u64;
    for m in 0..600u64 {
        let frac = (rate * (m as f32 + 1.0)).min(DROP);
        phases.push(Phase::Hold { secs: 60, h: TRUE_HASHRATE * (1.0 - frac) });
        dm = m + 1;
        if frac >= DROP { break; }
    }
    let floor_h = TRUE_HASHRATE * (1.0 - DROP);
    phases.push(Phase::Hold { secs: observe * 60, h: floor_h });
    let scen = Scenario::Custom { name: "x".into(), phases, initial_estimate: None };
    let (proto, sched) = scen.build(SPARSE_SPM);
    let config = TrialConfig { tick_interval_secs: 60, ..proto };
    let clock = Arc::new(MockClock::new(0));
    let v = (a.factory)(clock.clone());
    let d_end = (mature + dm) * 60;
    let trial_end = d_end + observe * 60;
    let t = run_trial_observed(v, clock, config, &sched, seed);
    (t, sched, d_end, trial_end)
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() { return f64::NAN; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// per arm over N seeds: (% escape ticks with belief>OP, median OP/TRUE_H, belief
/// volatility = median |Δbelief|/belief per tick). belief volatility is the DIRECT,
/// confound-free spike-presentation measure (doesn't go through OP).
fn stats(a: &AlgorithmSpec, seeds: u64) -> (f64, f64, f64) {
    let (mut flip_frac_acc, mut op_acc, mut vol_acc) = (Vec::new(), Vec::new(), Vec::new());
    for s in 0..seeds {
        let (t, _sched, d_end, trial_end) = escape(a, DEFAULT_BASELINE_SEED ^ 0xB0B ^ s);
        let (mut ticks, mut flips) = (0u32, 0u32);
        let mut ops = Vec::new();
        let mut prev_belief: Option<f64> = None;
        let mut vols = Vec::new();
        for tk in &t.ticks {
            if tk.t_secs <= d_end || tk.t_secs > trial_end { continue; }
            let op = tk.current_hashrate_before as f64;
            let belief = match tk.h_estimate { Some(b) => b as f64, None => continue };
            ticks += 1;
            if belief > op { flips += 1; }
            ops.push(op / TRUE_HASHRATE as f64);
            if let Some(pb) = prev_belief {
                if pb > 0.0 { vols.push((belief - pb).abs() / pb); }
            }
            prev_belief = Some(belief);
        }
        if ticks > 0 { flip_frac_acc.push(flips as f64 / ticks as f64); }
        if !ops.is_empty() { op_acc.push(median(ops)); }
        if !vols.is_empty() { vol_acc.push(median(vols)); }
    }
    (median(flip_frac_acc), median(op_acc), median(vol_acc))
}

fn main() {
    println!("# BELIEF-vs-OPERATING-POINT — does the slow estimator suppress sparse self-deepening by");
    println!("# NOT PRESENTING SPIKES (§6.1 mechanism), or is 0-vs-4 an operating-point-level confound?\n");
    println!("Sparse spm{}, decline {}%/hr drop {}%. Both arms run PoissonCI (below spm6 switch) — only estimator speed differs.\n",
        SPARSE_SPM as u32, DECLINE_PPH as u32, (DROP*100.0) as u32);
    let seeds = 200u64;
    println!("| arm | % escape ticks belief>OP (flip-enabled) | median OP / true_H | belief volatility (med |Δ|/belief) |");
    println!("| --- | --- | --- | --- |");
    for tau in [360u64, 30] {
        let a = variant(tau);
        let (flip, op, vol) = stats(&a, seeds);
        println!("| Ewma{} | {:.0}% | {:.2} | {:.3} |", tau, flip * 100.0, op, vol);
    }
    println!("\nREAD: the CONFOUND-FREE measure is BELIEF VOLATILITY (median |Δbelief|/belief) — it doesn't go through OP, so it");
    println!("directly answers 'does the estimator present spikes'. If Ewma30 volatility >> Ewma360 volatility, the fast estimator");
    println!("presents spikes the slow one doesn't — §6.1's spike-non-presentation mechanism STANDS (regardless of OP levels).");
    println!("Cross-check the confound: if median OP/true_H is COMPARABLE across arms, the flip-difference isn't an OP-level effect.");
    println!("If Ewma30's OP is dramatically LOWER, part of the flip-difference is OP-level and §6.1's mechanism conjunct needs softening");
    println!("to 'belief volatility + OP interaction' rather than pure spike-non-presentation. Volatility is the load-bearing read.");
}
