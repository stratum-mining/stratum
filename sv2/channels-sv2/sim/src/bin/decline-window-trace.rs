//! MECHANISM TRACE — does the share-indexed window STRETCH as shares fall on a
//! decline (the liability the tick-floor probe suggested), and does worse-on-
//! decline hold at the GROUNDED n_span (not just the matched-to-τ probe)?
//!
//! The probe showed share-indexed carries more over-difficulty than time-EWMA at
//! matched windows on declines. Proposed mechanism: per-share decay
//! alpha=exp(-pending/n_span) → on a decline shares FALL → pending/tick falls →
//! alpha→1 → the window STRETCHES → it holds the stale (pre-decline, too-high)
//! estimate longer → eases slower → more over-difficulty. The "holds on empty
//! ticks" property (a feature at steady low rate) is a LIABILITY on a decline.
//!
//! This does NOT scan — it traces ONE decline, tick by tick, printing for each
//! arm: the true H, the controller's belief, e=ln(belief/H), and for the
//! share-indexed arm the per-tick pending_shares and effective alpha (the window
//! state). If alpha climbs toward 1 as the decline deepens, the stretch mechanism
//! is CONFIRMED (not an artifact of matched-window comparison). Grounded n_span
//! (r_ref=12, tau=360 → n_span=72) so it's the study's actual config, not matched.
//!
//! Usage: cargo run --release --bin decline-window-trace
//! Env: VARDIFF_DWT_SPM (default 30 — the worst railing rate), VARDIFF_DWT_RATE_PPH (default 20).

use std::env;
use std::sync::Arc;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    ShareIndexedEstimator, SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const SENS: f64 = 1.5;
const TICK: u64 = 60;

fn ewma_cfg(tau: u64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{tau}"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(SENS, 0.05, 8.0, 0.06, 0.6), 6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05), 1.0, clock,
        )))
    })
}
fn share_cfg(n_span: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("ShareIdx{}", n_span as u64), move |clock| {
        VardiffBox(Box::new(Composed::new(
            ShareIndexedEstimator::new(n_span),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(SENS, 0.05, 8.0, 0.06, 0.6), 6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05), 1.0, clock,
        )))
    })
}

/// Trace one decline; return per-tick (t_min, true_h, belief, e_pct, n_shares).
/// belief = current_hashrate_before (the operating point the controller acts on).
fn trace(a: &AlgorithmSpec, rate_pph: f32, spm: f32, seed: u64)
    -> Vec<(f64, f64, f64, f64, u32)> {
    let mature = 60u64;
    let rate = rate_pph / 100.0 / 60.0;
    let target = 0.50f32;
    let observe = 90u64;
    let mut phases = vec![Phase::Hold { secs: mature * 60, h: TRUE_HASHRATE }];
    let mut dm = 0u64;
    for m in 0..300u64 {
        let frac = (rate * (m as f32 + 1.0)).min(target);
        phases.push(Phase::Hold { secs: 60, h: TRUE_HASHRATE * (1.0 - frac) });
        dm = m + 1;
        if frac >= target { break; }
    }
    let floor_h = TRUE_HASHRATE * (1.0 - (rate * dm as f32).min(target));
    phases.push(Phase::Hold { secs: observe * 60, h: floor_h });
    let scen = Scenario::Custom { name: "decline".into(), phases, initial_estimate: None };
    let (proto, sched) = scen.build(spm);
    let config = TrialConfig { tick_interval_secs: TICK, ..proto };
    let clock = Arc::new(MockClock::new(0));
    let v = (a.factory)(clock.clone());
    let t = run_trial_observed(v, clock, config, &sched, seed);
    t.ticks.iter().map(|tk| {
        let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
        let belief = tk.current_hashrate_before as f64;
        let e = (belief / h_true).ln() * 100.0;
        (tk.t_secs as f64 / 60.0, h_true, belief, e, tk.n_shares)
    }).collect()
}

fn main() {
    let spm: f32 = env::var("VARDIFF_DWT_SPM").ok().and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let rate_pph: f32 = env::var("VARDIFF_DWT_RATE_PPH").ok().and_then(|s| s.parse().ok()).unwrap_or(20.0);
    let seed = DEFAULT_BASELINE_SEED ^ 0xDEC_11E7;

    // Grounded share-indexed: n_span = r_ref(12)·tau(360)/60 = 72. The study's config.
    let n_span = 12.0 * 360.0 / 60.0;
    let ewma = ewma_cfg(360); // champion
    let share = share_cfg(n_span);

    let te = trace(&ewma, rate_pph, spm, seed);
    let ts = trace(&share, rate_pph, spm, seed);

    println!("\n## DECLINE WINDOW TRACE — spm={}, decline {}%/hr, grounded n_span={}, champion τ=360.", spm as u32, rate_pph as u32, n_span as u64);
    println!("Mechanism question: does share-indexed's effective alpha climb toward 1 (window STRETCHES) as shares fall on the decline?");
    println!("alpha_share = exp(-pending/n_span); alpha_ewma = exp(-60/360)=0.846 (fixed). Window stretches ⟺ alpha→1 ⟺ holds stale belief.\n");
    println!("| t(min) | true H (norm) | n_shares | α_share=exp(-n/{}) | EWMA e% | SHARE e% | Δe (share−ewma) |", n_span as u64);
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    let h0 = TRUE_HASHRATE as f64;
    let n = te.len().min(ts.len());
    // print every other tick in the decline+early-floor region to keep it readable
    for i in (0..n).step_by(2) {
        let (tmin, h_true, _be, e_ew, _ne) = te[i];
        let (_, _, _bs, e_sh, n_sh) = ts[i];
        let alpha_share = (-(n_sh as f64) / n_span).exp();
        let dde = e_sh - e_ew;
        // only the decline + first part of floor (skip the long flat tail)
        if tmin > 240.0 { break; }
        println!("| {:.0} | {:.2} | {} | {:.3} | {:+.1} | {:+.1} | {:+.1} |",
            tmin, h_true / h0, n_sh, alpha_share, e_ew, e_sh, dde);
    }

    // summary: peak over-difficulty and over-diff area for each arm
    let area = |tr: &[(f64,f64,f64,f64,u32)]| -> f64 {
        tr.iter().filter(|(t,..)| *t > 60.0).map(|(_,_,_,e,_)| e.max(0.0)).sum::<f64>()
    };
    let peak = |tr: &[(f64,f64,f64,f64,u32)]| -> f64 {
        tr.iter().filter(|(t,..)| *t > 60.0).map(|(_,_,_,e,_)| *e).fold(f64::MIN, f64::max)
    };
    // alpha at decline start vs deepest (the stretch, quantified)
    let alpha_at = |tr: &[(f64,f64,f64,f64,u32)], tmin_lo: f64, tmin_hi: f64| -> f64 {
        let xs: Vec<f64> = tr.iter().filter(|(t,..)| *t >= tmin_lo && *t < tmin_hi)
            .map(|(_,_,_,_,n)| (-(*n as f64)/n_span).exp()).collect();
        if xs.is_empty() { f64::NAN } else { xs.iter().sum::<f64>()/xs.len() as f64 }
    };
    println!("\nSHARE-INDEXED window stretch (the mechanism): α early-decline (60–90min) = {:.3}, deep-decline (last 30min of decline) = {:.3}.",
        alpha_at(&ts, 60.0, 90.0), alpha_at(&ts, 60.0 + 0.0, 240.0));
    println!("(α_ewma fixed at 0.846. If α_share climbs ABOVE that and toward 1 as the decline deepens, the window stretches — mechanism CONFIRMED.)");
    println!("\nOver-difficulty: EWMA peak {:+.1}% area {:.0} | SHARE peak {:+.1}% area {:.0}.  Share worse ⟺ area_share > area_ewma at the GROUNDED n_span.",
        peak(&te), area(&te), peak(&ts), area(&ts));
    println!("VERDICT: if α_share climbs toward 1 on the decline AND share area > ewma area at grounded n_span, the worse-on-decline finding is");
    println!("MECHANISM-CONFIRMED (not a matched-window artifact): per-share decay stretches the window when shares fall, holding stale-high, easing slower.");
}
