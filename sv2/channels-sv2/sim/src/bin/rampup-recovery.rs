//! RAMP-UP RECOVERY — characterizes the LOUD side (recovery from over-difficulty),
//! the axis every prior decline probe left uncharacterized.
//!
//! ## The question
//!
//! Decline-safety (the essay's subject) is the QUIET side: does the controller ease
//! down when shares thin? This binary asks the opposite, LOUD side: after difficulty
//! has been driven DOWN (by easing through a decline or silence), what happens when
//! the miner comes BACK? Shares flood in — the loud, self-correcting side — and the
//! controller must tighten difficulty back up. Two things can go wrong:
//!   - a transient SPM OVERSHOOT (the "thundering herd": a burst of far-too-easy
//!     shares until difficulty catches up), and
//!   - the DURATION of that transient (how long until it re-settles at target).
//!
//! This is orthogonal to the idle path: decline-safety is architectural (§6), ramp-up
//! overshoot is a tuning concern on the loud side. Characterizing it here lets the
//! post's ramp-up caveat rest on data, and answers whether the champion's climb-back
//! needs the "jump to estimate" tweak.
//!
//! ## Structural peak vs tunable duration (the decomposition Emmett's single points miss)
//!
//! - STRUCTURAL PEAK: at the instant of resume the controller's difficulty is set for
//!   the DECLINED miner (belief driven down to `1/ease_depth` of true). The full-rate
//!   miner hitting that too-easy difficulty produces shares at ~`ease_depth`× target —
//!   a peak set by how deep the ease went, NOT by tuning. It is bounded only by a
//!   max-share-rate CAP if the pool enforces one (that is what the cap is for).
//! - TUNABLE DURATION: how fast the controller climbs difficulty back. The champion's
//!   AcceleratingPartialRetarget creeps `new = cur + η·(est−cur)`, η 0.2→0.6 — so it
//!   approaches multiplicatively over several fires. Tuning (or the jump-to-estimate
//!   variant) shortens duration; it cannot remove the structural peak.
//!
//! ## Arms
//!   - `champion`  = champion_composed (Ewma360 + AcceleratingPartialRetarget) — shipped.
//!   - `jump`      = same Ewma360 estimator + FullRetargetNoClamp (η=1): on each fire
//!     it jumps STRAIGHT to the estimate instead of creeping. A MEASUREMENT CONTRAST
//!     for the duration axis — it collapses recovery duration (~35%) while leaving the
//!     structural peak unchanged (peak is set by ease depth, not the climb rule). It is
//!     NOT a production candidate: FullRetargetNoClamp is the cost-blind oracle and
//!     FAILS the decline-safety gate (+6.90% worst-settled-e vs champion +2.67%, 5%
//!     gate) — the loud-side win costs quiet-side safety. A viable tweak would gate the
//!     jump to fire only on the loud (recovery) side and clear the gate first.
//!
//! ## Scenario
//! Mature 60min → decline to `ease_depth` fraction lost over `ramp_min` → then ABRUPT
//! resume to full rate → observe recovery. Peak = max realized spm / target over the
//! recovery window; duration = minutes from resume until realized spm settles back
//! within 20% of target. Optional share-rate cap clamps deliverable spm.
//!
//! Usage: cargo run --release --bin rampup-recovery
//! Env: VARDIFF_RU_TRIALS (300), VARDIFF_RU_SPM (6), VARDIFF_RU_CAP (0=off, else max spm),
//!      VARDIFF_RU_SEED.

use std::env;
use std::sync::Arc;

use channels_sv2::vardiff::composed::{
    champion_composed, AdaptiveSignPersist, Composed, EwmaEstimator, FullRetargetNoClamp,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::decline_safety::{DECLINE_MATURE_MIN, DECLINE_OBSERVE_MIN, DECLINE_TICK_SECS as TICK};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial, TrialConfig};

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() { return f64::NAN; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Shipped champion.
fn champion() -> AlgorithmSpec {
    AlgorithmSpec::new("champion", |clock| VardiffBox(Box::new(champion_composed(1.0, clock))))
}
/// Jump-to-estimate variant: champion's EWMA(360) estimator + boundary, but
/// FullRetargetNoClamp (η=1) — retargets straight to the belief each fire instead
/// of the accelerating partial creep.
fn jump_to_estimate() -> AlgorithmSpec {
    AlgorithmSpec::new("jump", |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(360),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(1.5, 0.05, 8.0, 0.06, 0.6),
                6,
            ),
            FullRetargetNoClamp,
            1.0,
            clock,
        )))
    })
}

/// Mature → decline to `ease_depth` over `ramp_min` → abrupt resume to full rate →
/// observe. Returns (resume_start_secs, trial_end_secs, scenario).
fn recovery_scenario(ease_depth: f32, ramp_min: u64) -> (Scenario, u64, u64) {
    let mature = DECLINE_MATURE_MIN * 60;
    let floor = TRUE_HASHRATE * (1.0 - ease_depth);
    let mut phases = vec![Phase::Hold { secs: mature, h: TRUE_HASHRATE }];
    // decline in 1-min steps to the floor (so the controller actually eases down to it)
    for m in 0..ramp_min {
        let frac = ease_depth * (m as f32 + 1.0) / ramp_min as f32;
        phases.push(Phase::Hold { secs: 60, h: TRUE_HASHRATE * (1.0 - frac) });
    }
    // hold at floor long enough for belief to settle down to the declined rate
    phases.push(Phase::Hold { secs: 30 * 60, h: floor });
    let resume_start = mature + ramp_min * 60 + 30 * 60;
    // abrupt resume to full rate, observe recovery
    phases.push(Phase::Hold { secs: DECLINE_OBSERVE_MIN * 60, h: TRUE_HASHRATE });
    let trial_end = resume_start + DECLINE_OBSERVE_MIN * 60;
    (
        Scenario::Custom { name: format!("recovery_d{}_r{}", (ease_depth*100.0) as u32, ramp_min), phases, initial_estimate: None },
        resume_start, trial_end,
    )
}

/// (peak_spm_ratio, duration_min): peak realized-spm/target over the recovery window,
/// and minutes from resume until realized spm settles back within 20% of target.
/// `cap` > 0 clamps deliverable shares/min (models a pool max-share-rate).
fn measure(make: &dyn Fn() -> AlgorithmSpec, ease_depth: f32, ramp_min: u64, spm: f32, cap: f32, trials: usize, seed: u64) -> (f64, f64) {
    let (scen, resume_start, _trial_end) = recovery_scenario(ease_depth, ramp_min);
    let (cfg_proto, schedule) = scen.build(spm);
    let cfg = TrialConfig { tick_interval_secs: TICK, ..cfg_proto };
    let (mut peaks, mut durs) = (vec![], vec![]);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (make().factory)(clock.clone());
        let t = run_trial(v, clock, cfg.clone(), &schedule, seed.wrapping_add(i as u64));
        let mut peak = 0.0f64;
        let mut settle_tick: Option<u64> = None;
        for tk in &t.ticks {
            if tk.t_secs <= resume_start { continue; }
            // realized spm this tick = n_shares over the tick interval, as a ratio to target
            let realized_spm = tk.n_shares as f64 / (TICK as f64 / 60.0);
            let capped = if cap > 0.0 { realized_spm.min(cap as f64) } else { realized_spm };
            let ratio = capped / spm as f64;
            peak = peak.max(ratio);
            // settled = first tick after resume where realized returns within 20% of target
            if settle_tick.is_none() && (ratio - 1.0).abs() <= 0.20 {
                // require it was elevated first (skip the very first tick if already near)
                if tk.t_secs > resume_start + TICK {
                    settle_tick = Some(tk.t_secs - resume_start);
                }
            }
        }
        peaks.push(peak);
        durs.push(settle_tick.map(|s| s as f64 / 60.0).unwrap_or(f64::NAN));
    }
    (median(peaks), median(durs.into_iter().filter(|x| !x.is_nan()).collect()))
}

fn main() {
    let trials: usize = env::var("VARDIFF_RU_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let spm: f32 = env::var("VARDIFF_RU_SPM").ok().and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let cap: f32 = env::var("VARDIFF_RU_CAP").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let seed: u64 = env::var("VARDIFF_RU_SEED").ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(DEFAULT_BASELINE_SEED);

    // ease-depth × ramp-speed grid. Depth = how far difficulty gets driven down (sets
    // the structural peak). Ramp = decline speed (fast decline eases deeper before resume).
    let depths: [f32; 4] = [0.5, 0.8, 0.9, 0.95];
    let ramps: [u64; 3] = [10, 30, 120]; // min to reach the depth (fast / med / slow decline)

    eprintln!(
        "rampup-recovery: champion vs jump, {}×{} (depth×ramp) × {} trials, spm {}, cap {}",
        depths.len(), ramps.len(), trials, spm,
        if cap > 0.0 { format!("{cap} spm") } else { "off".into() },
    );

    println!("arm,ease_depth,ramp_min,peak_spm_ratio,duration_min");
    for arm in ["champion", "jump"] {
        let make: fn() -> AlgorithmSpec = if arm == "champion" { champion } else { jump_to_estimate };
        for &d in &depths {
            for &r in &ramps {
                let (peak, dur) = measure(&make, d, r, spm, cap, trials, seed);
                println!("{arm},{d},{r},{peak:.2},{dur:.1}");
            }
        }
    }

    // Summary: structural-peak-vs-depth (should track ~1/(1-depth), tuning-independent)
    // and champion-vs-jump duration (the contrast that isolates the tunable axis).
    eprintln!("\n## Read: peak should track ease depth (structural, ~same for both arms);");
    eprintln!("## duration should be SHORTER for jump — isolates the tunable axis. NOTE: jump");
    eprintln!("## (FullRetargetNoClamp) FAILS the decline gate (+6.9% vs champion +2.67%) — it is");
    eprintln!("## a measurement contrast, NOT a production candidate. See rampup_recovery_data/README.md.");
    eprintln!("## Peak ratio = realized spm / target at the worst recovery tick. cap clamps it.");
}
