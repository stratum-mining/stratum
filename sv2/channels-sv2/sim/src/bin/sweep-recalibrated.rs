//! Recalibrated champion hunt after the §6(ii) retraction.
//!
//! The lost-in-flight-work premise was retracted (a retarget rejects no
//! in-flight shares — per-job target snapshot; churn is value-neutral). That
//! lands surgically on the cost function's effort-DIRECTION weighting:
//!   - W_OVER:W_UNDER (regret direction) — SURVIVES (operating-point safety, §6(i))
//!   - RHO_UP:RHO_DOWN (effort direction) — RETIRED → forced to 1:1 (no value
//!     basis to charge a tighten more than an ease of equal size)
//!   - λ (linear Σ|s| churn term) — DEFLATED to soft usability cost (not value);
//!     the cadence cap already owns anti-churn, so its keep/drop is now a question
//!
//! Pre-registered three-way (forced change isolated from optional cleanup):
//!   (A) RHO 3:1, λ-swept   — the CURRENT baseline (reproduces today's champion)
//!   (B) RHO 1:1, λ-swept   — the FORCED consequence of the retraction
//!   (C) RHO 1:1, λ-dropped — the OPTIONAL cleanup (cap owns churn)
//! A→B is mandatory; champion moving there is the headline ("the killed premise
//! moved the champion"). B→C is the optional λ ablation.
//!
//! Pre-registered outcome rules (committed BEFORE seeing numbers):
//!   - same champion λ-swept and λ-dropped → DROP λ (it earned removal; a term
//!     that doesn't change the answer but needs a defended weight is liability).
//!   - λ-kept ≠ λ-dropped champion → λ-DROPPED is the default; λ-kept wins only
//!     with a named concrete usability harm (else the term defends its own life).
//!   - EVERY surviving champion RE-CLEARS the decline gate from scratch — safety
//!     is NOT inherited from Ewma360/s1.5, because the cost fn that selected it
//!     changed (the RHO 3:1 effort weight was doing incidental safety work via
//!     reluctant-tighten; removing it means safety must come from the boundary's
//!     tighten_multiplier + this gate, not the effort term).
//!
//! Effort is stored SPLIT BY DIRECTION so RHO is applied at scoring time — all
//! three conditions are free re-scorings of one trial run.
//!
//! Usage: cargo run --release --bin sweep-recalibrated
//! Env: VARDIFF_RC_TRIALS (default 300), VARDIFF_SWEEP_SEED.

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const FLOOR: f64 = 0.05;
const W_OVER: f64 = 3.0;
const W_UNDER: f64 = 1.0;
const RHO: f64 = 0.5;
const LAMBDAS: [f64; 4] = [0.0, 0.5, 1.0, 2.0];
const BAND: [f32; 4] = [4.0, 6.0, 12.0, 30.0];
const ANCHOR: f32 = 60.0;
const GATE_PCT: f64 = 5.0; // decline-gate runaway threshold (matches slow-decline.rs)

#[derive(Clone, Copy)]
struct P {
    tau: u64,
    sens: f64,
    tighten: f64,
    eta_max: f32,
}

fn spec(p: P) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{}/s{}/t{}", p.tau, p.sens, p.tighten), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(p.tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(p.sens, FLOOR, p.tighten, 0.06, 0.6),
                6,
            ),
            AcceleratingPartialRetarget::new(0.2, p.eta_max, 0.05),
            1.0,
            clock,
        )))
    })
}

/// Components with effort SPLIT BY DIRECTION (un-weighted), so any RHO_UP:RHO_DOWN
/// can be applied at scoring time. Returns:
///   (reg_over, reg_under, quad_up, quad_down, lin_up, lin_down)
fn components(mk: &dyn Fn() -> AlgorithmSpec, spm: f32, trials: usize, seed: u64) -> [f64; 6] {
    let scens = [
        Scenario::Stable,
        Scenario::Step { delta_pct: -50 },
        Scenario::Step { delta_pct: -10 },
        Scenario::Step { delta_pct: 10 },
        Scenario::Step { delta_pct: 50 },
    ];
    let (mut ro, mut ru) = (0.0, 0.0);
    let (mut qu, mut qd, mut lu, mut ld) = (0.0, 0.0, 0.0, 0.0);
    let mut n = 0u32;
    for (si, scen) in scens.iter().enumerate() {
        let (cfg, sched) = scen.build(spm);
        let config = TrialConfig { tick_interval_secs: 60, ..cfg };
        for i in 0..trials {
            let clock = Arc::new(MockClock::new(0));
            let v = (mk)().factory.clone()(clock.clone());
            let t = run_trial_observed(v, clock, config.clone(), &sched,
                seed.wrapping_add(((si as u64) << 20) + ((spm as u64) << 32) + i as u64));
            let mut last_t = 0u64;
            let (mut tro, mut tru, mut cov) = (0.0, 0.0, 0.0);
            for tk in &t.ticks {
                let dt = (tk.t_secs - last_t) as f64; last_t = tk.t_secs;
                let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
                let h_est = tk.current_hashrate_before as f64;
                if dt > 0.0 && h_true > 0.0 && h_est > 0.0 {
                    let e = (h_est / h_true).ln();
                    if e >= 0.0 { tro += dt * e.abs(); } else { tru += dt * e.abs(); }
                    cov += dt;
                }
                if tk.fired {
                    if let Some(nh) = tk.new_hashrate {
                        let old = tk.current_hashrate_before as f64;
                        if old > 0.0 && nh as f64 > 0.0 {
                            let dl = (nh as f64 / old).ln();
                            if dl >= 0.0 { qu += dl * dl; lu += dl.abs(); }
                            else { qd += dl * dl; ld += dl.abs(); }
                        }
                    }
                }
            }
            if cov > 0.0 { ro += tro / cov; ru += tru / cov; }
            n += 1;
        }
    }
    let nf = n.max(1) as f64;
    [ro / nf, ru / nf, qu / nf, qd / nf, lu / nf, ld / nf]
}

/// Cost with RHO_UP/RHO_DOWN and λ applied at scoring time. `drop_lambda`
/// forces the linear term out entirely (condition C).
fn cost(c: &[f64; 6], rho_up: f64, rho_down: f64, lambda: f64, drop_lambda: bool) -> f64 {
    let quad = rho_up * c[2] + rho_down * c[3];
    let lin = if drop_lambda { 0.0 } else { lambda * (rho_up * c[4] + rho_down * c[5]) };
    W_OVER * c[0] + W_UNDER * c[1] + RHO * (quad + lin)
}

/// Decline gate: median settled e% after a 10%/hr decline at 6 spm, recovery
/// window observed. Replicates slow-decline.rs / sweep-corrected.rs. Pass iff
/// settled e ≤ GATE_PCT.
fn decline_settled_e_pct(p: P, trials: usize, seed: u64) -> f64 {
    let spm = 6.0f32;
    let mature = 60u64;
    let rate = 0.10f32 / 60.0;
    let target_drop = 0.50f32;
    let observe = 120u64;
    let mut phases = vec![Phase::Hold { secs: mature * 60, h: TRUE_HASHRATE }];
    let mut dm = 0u64;
    for m in 0..300u64 {
        let frac = (rate * (m as f32 + 1.0)).min(target_drop);
        phases.push(Phase::Hold { secs: 60, h: TRUE_HASHRATE * (1.0 - frac) });
        dm = m + 1;
        if frac >= target_drop { break; }
    }
    let floor_h = TRUE_HASHRATE * (1.0 - (rate * dm as f32).min(target_drop));
    phases.push(Phase::Hold { secs: observe * 60, h: floor_h });
    let scen = Scenario::Custom { name: "gate_decline".into(), phases, initial_estimate: None };
    let (cfg, sched) = scen.build(spm);
    let config = TrialConfig { tick_interval_secs: 60, ..cfg };
    let d_end = (mature + dm) * 60;
    let trial_end = d_end + observe * 60;
    let a = spec(p);
    let mut settled: Vec<f64> = Vec::with_capacity(trials);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        let mut se = 0.0f64;
        for tk in &t.ticks {
            if tk.t_secs > d_end && tk.t_secs <= trial_end {
                let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
                se = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
            }
        }
        settled.push(se);
    }
    settled.sort_by(|a, b| a.partial_cmp(b).unwrap());
    settled[settled.len() / 2]
}

fn main() {
    let base_trials: usize = env::var("VARDIFF_RC_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(DEFAULT_BASELINE_SEED);
    let trials_for = |spm: f32| (base_trials as f64 * (12.0 / spm as f64).clamp(1.0, 4.0)).round() as usize;

    // Field: tau×sens grid + classic anchor (same as sweep-minimax).
    let taus = [120u64, 150, 240, 360, 480, 720];
    let senss = [0.3f64, 0.6, 1.0, 1.5, 2.0];
    let mut field: Vec<(String, P)> = Vec::new();
    for &tau in &taus {
        for &sens in &senss {
            let p = P { tau, sens, tighten: 8.0, eta_max: 0.6 };
            field.push((spec(p).name.clone(), p));
        }
    }
    let n_cfg = field.len();
    let rates: Vec<f32> = BAND.iter().copied().chain([ANCHOR]).collect();
    let n_rate = rates.len();

    eprintln!(
        "sweep-recalibrated: {} configs × {} rates × 5 scen, base {} trials. Three-way: (A 3:1 λ-swept)(B 1:1 λ-swept)(C 1:1 λ-drop).",
        n_cfg, n_rate, base_trials
    );

    // Collect direction-split components per (config, rate).
    let jobs: Vec<(usize, usize)> = (0..n_cfg).flat_map(|ci| (0..n_rate).map(move |ri| (ci, ri))).collect();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let comps: Mutex<Vec<Vec<[f64; 6]>>> = Mutex::new(vec![vec![[0.0; 6]; n_rate]; n_cfg]);
    let started = Instant::now();
    let nthreads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..nthreads {
            s.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (ci, ri) = jobs[j];
                let p = field[ci].1;
                let spm = rates[ri];
                let c = components(&|| spec(p), spm, trials_for(spm), seed ^ (spm as u64));
                comps.lock().unwrap()[ci][ri] = c;
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 32 == 0 { eprintln!("  {}/{} ({:.0}s)", d, jobs.len(), started.elapsed().as_secs_f64()); }
            });
        }
    });
    let comps = comps.into_inner().unwrap();
    eprintln!("components done in {:.0}s", started.elapsed().as_secs_f64());

    let band_idx: Vec<usize> = (0..n_rate).filter(|&ri| rates[ri] != ANCHOR).collect();

    // The production champion was selected by a GATE-CONSTRAINED rule, not raw
    // minimax: the raw winner is a cost-tied cluster (sleepy long-window corner),
    // and the decline gate breaks the tie — the champion is the GENTLEST
    // gate-PASSING config (Ewma360/s1.5), not the raw cost-minimizer. So
    // recalibration must apply the gate as the SELECTOR across the whole field,
    // per condition. Gate every config once (gate is cost-independent), reuse.
    eprintln!("gating the full field (cost-independent, computed once)...");
    let gate_field_trials = (base_trials / 2).max(120);
    let gnext = AtomicUsize::new(0);
    let gate_e: Mutex<Vec<f64>> = Mutex::new(vec![f64::NAN; n_cfg]);
    let gstarted = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..nthreads {
            s.spawn(|| loop {
                let ci = gnext.fetch_add(1, Ordering::Relaxed);
                if ci >= n_cfg { break; }
                let g = decline_settled_e_pct(field[ci].1, gate_field_trials, seed ^ 0xDEC11E);
                gate_e.lock().unwrap()[ci] = g;
            });
        }
    });
    let gate_e = gate_e.into_inner().unwrap();
    eprintln!("field gate done in {:.0}s ({} of {} pass ≤{}%)",
        gstarted.elapsed().as_secs_f64(),
        gate_e.iter().filter(|&&g| g <= GATE_PCT).count(), n_cfg, GATE_PCT);

    // Raw (un-gated) minimax winner — what the cost alone picks.
    let raw_winner_of = |rho_up: f64, rho_down: f64, lambda: f64, drop_lambda: bool| -> (usize, f64) {
        let mut frontier = vec![f64::MAX; n_rate];
        for ci in 0..n_cfg {
            for ri in 0..n_rate {
                let cst = cost(&comps[ci][ri], rho_up, rho_down, lambda, drop_lambda);
                if cst < frontier[ri] { frontier[ri] = cst; }
            }
        }
        let mut best = (0usize, f64::MAX);
        for ci in 0..n_cfg {
            let mut worst = f64::MIN;
            for &ri in &band_idx {
                let ratio = cost(&comps[ci][ri], rho_up, rho_down, lambda, drop_lambda) / frontier[ri];
                if ratio > worst { worst = ratio; }
            }
            if worst < best.1 { best = (ci, worst); }
        }
        best
    };

    // GATE-CONSTRAINED minimax winner — the actual production selection rule:
    // among configs that PASS the decline gate, the minimax-best (frontier is
    // still over the FULL field, so the ratio is comparable; we just restrict
    // WHICH config we may pick to the gate-passers).
    let winner_of = |rho_up: f64, rho_down: f64, lambda: f64, drop_lambda: bool| -> (usize, f64) {
        let mut frontier = vec![f64::MAX; n_rate];
        for ci in 0..n_cfg {
            for ri in 0..n_rate {
                let cst = cost(&comps[ci][ri], rho_up, rho_down, lambda, drop_lambda);
                if cst < frontier[ri] { frontier[ri] = cst; }
            }
        }
        let mut best = (usize::MAX, f64::MAX);
        for ci in 0..n_cfg {
            if !(gate_e[ci] <= GATE_PCT) { continue; } // gate constraint
            let mut worst = f64::MIN;
            for &ri in &band_idx {
                let ratio = cost(&comps[ci][ri], rho_up, rho_down, lambda, drop_lambda) / frontier[ri];
                if ratio > worst { worst = ratio; }
            }
            if worst < best.1 { best = (ci, worst); }
        }
        best
    };

    // ===== The three-way: RAW winner (cost cluster) vs GATE-CONSTRAINED winner =====
    // The gate-constrained winner is the production selection rule. The raw
    // winner is shown alongside to expose the cost-tied cluster the gate breaks.
    println!("\n#### Recalibrated champion hunt — three-way (forced A→B, optional B→C)");
    println!("Each line: λ → RAW minimax winner (cost only) | GATE-CONSTRAINED winner (the champion rule)\n");
    let report = |label: &str, rho_up: f64, rho_down: f64, drop_lambda: bool| {
        println!("## {label}  (RHO_UP:RHO_DOWN = {rho_up}:{rho_down}{})", if drop_lambda { ", λ DROPPED" } else { "" });
        let lams: &[f64] = if drop_lambda { &[0.0] } else { &LAMBDAS };
        for &lambda in lams {
            let (rci, rw) = raw_winner_of(rho_up, rho_down, lambda, drop_lambda);
            let (gci, gw) = winner_of(rho_up, rho_down, lambda, drop_lambda);
            let gname = if gci == usize::MAX { "(none pass gate)".into() } else { format!("{} (r{:.3})", field[gci].0, gw) };
            println!("  λ={lambda}: RAW {} (r{:.3})  |  GATED {}", field[rci].0, rw, gname);
        }
        println!();
    };
    report("(A) CURRENT baseline", 3.0, 1.0, false);
    report("(B) FORCED: effort symmetric", 1.0, 1.0, false);
    report("(C) OPTIONAL: λ dropped", 1.0, 1.0, true);

    // ===== Headline: gate-constrained champion across conditions =====
    let g_at = |ru: f64, rd: f64, l: f64, dl: bool| winner_of(ru, rd, l, dl).0;
    println!("#### HEADLINE — GATE-CONSTRAINED champion (the production rule)");
    println!("  Production champion (from slow-decline gate-tiebreak): Ewma360/s1.5");
    for &lambda in &LAMBDAS {
        let a = g_at(3.0, 1.0, lambda, false);
        let b = g_at(1.0, 1.0, lambda, false);
        let an = if a==usize::MAX {"(none)".into()} else {field[a].0.clone()};
        let bn = if b==usize::MAX {"(none)".into()} else {field[b].0.clone()};
        println!("  λ={lambda}: A(3:1)={}  B(1:1)={}  [{}]", an, bn,
            if a==b {"RHO flip: no change"} else {"RHO flip: MOVED"});
    }
    let cc = g_at(1.0, 1.0, 0.0, true);
    println!("  C (1:1, λ-dropped): {}", if cc==usize::MAX {"(none pass)".into()} else {field[cc].0.clone()});
    println!("\n  Reading: compare the GATED winners (not raw) to Ewma360/s1.5. If the");
    println!("  gate-constrained champion holds across the forced RHO flip and is stable");
    println!("  to the now-soft λ, the §6 retraction is CONFIRMATORY. If it moves, that is");
    println!("  the finding. λ-drop (C) is the pre-registered default selection.");

    // ===== Gate detail on the relevant configs already computed in gate_e =====
    println!("\n#### Gate detail (settled e%, ≤{}% passes) for the named winners + production champion", GATE_PCT);
    let mut named: Vec<usize> = LAMBDAS.iter().flat_map(|&l| [g_at(3.0,1.0,l,false), g_at(1.0,1.0,l,false)]).collect();
    named.push(cc);
    named.retain(|&i| i != usize::MAX);
    named.sort(); named.dedup();
    for &ci in &named {
        println!("  {}: {:+.1}%  [{}]", field[ci].0, gate_e[ci], if gate_e[ci] <= GATE_PCT {"pass"} else {"FAIL"});
    }
}
