//! Empirical validation of the conservation-law theory in
//! `docs/THEORY.md`. Post-processes raw trial trajectories (no new
//! metric machinery) into:
//!
//!   - regret  = time-averaged  e²  where  e = ln(H_est / H_true)
//!               split into regret_over (e>0, over-difficulty) and
//!               regret_under (e<0, under-difficulty)
//!   - effort  = Σ (Δ ln D)²  over fires, where D = H_est / r*, so
//!               Δ ln D = ln(new_hashrate / old_hashrate); split into
//!               effort_up (tightening) and effort_down (easing).
//!
//! Both quantities use only the UNIVERSAL TickRecord fields
//! (`current_hashrate_before`, `new_hashrate`), so they are computed
//! identically for the introspectable Composed algorithms and the
//! opaque production monolith.
//!
//! It then answers the three decisive questions from the theory note:
//!
//!   Q1 (binding?)  settled mean-e² vs the Poisson information floor
//!                  1/(r*·τ). If settled e² ≈ floor, the conservation
//!                  law is the operative constraint; if ≫ floor, the
//!                  law is true but slack and something else dominates.
//!   Q2 (δ² cancel?) post-step transient regret as a function of |δ|.
//!                  Flat in δ ⇒ the magnitude-cancellation result holds
//!                  and React−10/React−50 double-count one quantity.
//!   Q3 (real wall?) the contenders placed in the (regret, effort)
//!                  plane — do they pile against a frontier (a real
//!                  trade-off) or spread out (the 0.55 ceiling was a
//!                  metric artifact)?
//!
//! Usage: `cargo run --release --bin regret-effort`
//! Env: VARDIFF_RE_TRIALS (default 1000), VARDIFF_RE_SEED.

use std::collections::BTreeMap;
use std::env;
use std::sync::Arc;

use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED, DEFAULT_TRIAL_COUNT};
use vardiff_sim::grid::AlgorithmSpec;
use vardiff_sim::schedule::HashrateSchedule;
use vardiff_sim::trial::{run_trial_observed, Trial};

/// Time at which step scenarios change hashrate (mirrors
/// baseline::STEP_EVENT_AT_SECS). Pre-step is settled; post-step is
/// the transient we attribute the step's regret to.
const STEP_EVENT_AT_SECS: u64 = 15 * 60;
/// Settle cutoff for the stable scenario: ignore the first 12 min so
/// the convergence transient doesn't contaminate the steady-state
/// floor comparison (mirrors the introspection settle in metrics.rs).
const SETTLE_AFTER_SECS: u64 = 12 * 60;

#[derive(Default, Clone, Copy)]
struct Quad {
    regret_over: f64,
    regret_under: f64,
    effort_up: f64,
    effort_down: f64,
    // Linear-loss companions: time-averaged |e| (not e²), split by sign.
    // Used by the §10 loss-shape comparison to show how a linear penalty
    // re-weights small persistent errors vs the quadratic regret above.
    lin_over: f64,
    lin_under: f64,
}

impl Quad {
    fn regret(&self) -> f64 {
        self.regret_over + self.regret_under
    }
    fn effort(&self) -> f64 {
        self.effort_up + self.effort_down
    }
    fn lin(&self) -> f64 {
        self.lin_over + self.lin_under
    }
    fn add(&mut self, o: &Quad) {
        self.regret_over += o.regret_over;
        self.regret_under += o.regret_under;
        self.effort_up += o.effort_up;
        self.effort_down += o.effort_down;
        self.lin_over += o.lin_over;
        self.lin_under += o.lin_under;
    }
    fn scale(&self, k: f64) -> Quad {
        Quad {
            regret_over: self.regret_over * k,
            regret_under: self.regret_under * k,
            effort_up: self.effort_up * k,
            effort_down: self.effort_down * k,
            lin_over: self.lin_over * k,
            lin_under: self.lin_under * k,
        }
    }
}

/// Computes time-averaged e² (split by sign) over the half-open
/// window `(t_lo, t_hi]`, plus fire effort (split by direction) over
/// the same window. Time-averaging (dividing by window length)
/// yields a dimensionless mean-square error directly comparable to a
/// variance floor.
fn analyze_window(trial: &Trial, schedule: &HashrateSchedule, t_lo: u64, t_hi: u64) -> Quad {
    let mut q = Quad::default();
    let mut last_t = 0u64;
    let mut covered_secs = 0.0f64;
    for tick in &trial.ticks {
        let interval_start = last_t;
        let interval_end = tick.t_secs;
        last_t = tick.t_secs;
        // Clip the interval to the requested window.
        let lo = interval_start.max(t_lo);
        let hi = interval_end.min(t_hi);
        if hi <= lo {
            continue;
        }
        let dt = (hi - lo) as f64;
        let mid = (interval_start + interval_end) / 2;
        let h_true = schedule.at(mid) as f64;
        let h_est = tick.current_hashrate_before as f64;
        if h_true > 0.0 && h_est > 0.0 {
            let e = (h_est / h_true).ln();
            let c = dt * e * e;
            let cl = dt * e.abs();
            if e >= 0.0 {
                q.regret_over += c;
                q.lin_over += cl;
            } else {
                q.regret_under += c;
                q.lin_under += cl;
            }
            covered_secs += dt;
        }
        // Effort: a fire is stamped at tick.t_secs; count it if that
        // instant lies in the window.
        if tick.fired && tick.t_secs > t_lo && tick.t_secs <= t_hi {
            if let Some(nh) = tick.new_hashrate {
                let old = tick.current_hashrate_before as f64;
                if old > 0.0 && nh as f64 > 0.0 {
                    let dlog = (nh as f64 / old).ln();
                    let c = dlog * dlog;
                    if dlog >= 0.0 {
                        q.effort_up += c; // tightening (raises difficulty)
                    } else {
                        q.effort_down += c; // easing (lowers difficulty)
                    }
                }
            }
        }
    }
    // Convert the integrals to time-averages (mean e², mean |e|).
    if covered_secs > 0.0 {
        q.regret_over /= covered_secs;
        q.regret_under /= covered_secs;
        q.lin_over /= covered_secs;
        q.lin_under /= covered_secs;
    }
    q
}

fn mean_quad(trials: &[Trial], schedule: &HashrateSchedule, t_lo: u64, t_hi: u64) -> Quad {
    let mut acc = Quad::default();
    for t in trials {
        acc.add(&analyze_window(t, schedule, t_lo, t_hi));
    }
    acc.scale(1.0 / trials.len() as f64)
}

fn run_cell(algo: &AlgorithmSpec, scenario: &Scenario, spm: f32, trials: usize, seed: u64) -> Vec<Trial> {
    let (config, schedule) = scenario.build(spm);
    let mut out = Vec::with_capacity(trials);
    for i in 0..trials {
        let s = seed.wrapping_add(i as u64);
        let clock = Arc::new(MockClock::new(0));
        let vardiff = (algo.factory)(clock.clone());
        out.push(run_trial_observed(vardiff, clock, config.clone(), &schedule, s));
    }
    out
}

fn main() {
    let trials: usize = env::var("VARDIFF_RE_TRIALS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TRIAL_COUNT);
    let base_seed: u64 = env::var("VARDIFF_RE_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_BASELINE_SEED);

    let algos = vec![
        AlgorithmSpec::classic_vardiff_state(),
        AlgorithmSpec::balanced(),
        AlgorithmSpec::react_priority(),
    ];
    let spms: [f32; 7] = [4.0, 6.0, 8.0, 12.0, 15.0, 20.0, 30.0];
    let deltas: [i32; 8] = [-50, -25, -10, -5, 5, 10, 25, 50];

    eprintln!(
        "regret-effort: {} algos × {} SPM, {} trials/cell",
        algos.len(),
        spms.len(),
        trials
    );

    // --- Q1: settled mean-e² vs Poisson floor (Stable scenario) -----------
    println!("\n## Q1 — Is the conservation law binding? (Stable scenario, t > {}min)\n", SETTLE_AFTER_SECS / 60);
    println!("settled mean-e² (nats²) vs Poisson floors. floor_1t = 1/r* (single 1-min tick);");
    println!("floor_τ = 1/(r*·1.5) (a 90s effective window). e²/floor_1t ratio in brackets.\n");
    println!("| algo | SPM | settled e² | floor_1t | floor_τ | e²/floor_1t |");
    println!("| --- | --- | --- | --- | --- | --- |");
    let mut settle_ratio_by_algo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for algo in &algos {
        for &spm in &spms {
            let seed = base_seed.wrapping_add(hash_label(&algo.name, spm, 1));
            let ts = run_cell(algo, &Scenario::Stable, spm, trials, seed);
            let (_, sched) = Scenario::Stable.build(spm);
            let q = mean_quad(&ts, &sched, SETTLE_AFTER_SECS, u64::MAX);
            let e2 = q.regret();
            let floor_1t = 1.0 / spm as f64;
            let floor_tau = 1.0 / (spm as f64 * 1.5);
            let ratio = e2 / floor_1t;
            settle_ratio_by_algo
                .entry(algo.name.clone())
                .or_default()
                .push(ratio);
            println!(
                "| {} | {} | {:.5} | {:.5} | {:.5} | {:.2}× |",
                short(&algo.name),
                spm as u32,
                e2,
                floor_1t,
                floor_tau,
                ratio
            );
        }
    }

    // --- Q2: post-step transient regret vs |δ| ----------------------------
    println!("\n## Q2 — Does δ² cancel? (post-step regret, integrated nats²·min, t > 15min)\n");
    println!("If detection time ∝ 1/δ², integrated post-step regret should be ~flat in |δ|.");
    println!("Reported as the integral (not time-averaged) so it reflects total transient cost.\n");
    print!("| algo | SPM |");
    for d in &deltas {
        print!(" δ={}% |", d);
    }
    println!();
    print!("| --- | --- |");
    for _ in &deltas {
        print!(" --- |");
    }
    println!();
    for algo in &algos {
        for &spm in &[6.0f32, 12.0, 30.0] {
            print!("| {} | {} |", short(&algo.name), spm as u32);
            for &d in &deltas {
                let scen = Scenario::Step { delta_pct: d };
                let seed = base_seed.wrapping_add(hash_label(&algo.name, spm, d as i64 + 1000));
                let ts = run_cell(algo, &scen, spm, trials, seed);
                let (_, sched) = scen.build(spm);
                // Integrated (un-averaged) post-step regret: multiply
                // mean-e² by the post-step window length in minutes.
                let q = mean_quad(&ts, &sched, STEP_EVENT_AT_SECS, u64::MAX);
                let window_min = (vardiff_sim::baseline::TRIAL_DURATION_SECS - STEP_EVENT_AT_SECS) as f64 / 60.0;
                print!(" {:.3} |", q.regret() * window_min);
            }
            println!();
        }
    }

    // --- Q3 + asymmetry: (regret, effort) plane, BROKEN OUT by class -----
    // The smoke test showed a naive all-scenario aggregate is dominated
    // by the one-time cold-start ramp (huge under-difficulty regret +
    // upward effort), which hides the step asymmetry. So report four
    // scenario classes separately:
    //   cold   — ColdStart (one-time convergence ramp)
    //   stable — Stable settled steady state
    //   drop   — mean over negative-δ steps (→ over-difficulty transient)
    //   rise   — mean over positive-δ steps (→ under-difficulty transient)
    println!("\n## Q3 — (regret, effort) by scenario class (mean over all SPM)\n");
    println!("regret = time-avg e² (nats²); effort = Σ(Δln D)² per trial. The death-spiral");
    println!("asymmetry (§5.2) predicts drop-regret ≫ rise-regret for equal |δ|.\n");
    println!("| algo | class | regret | reg_over | reg_under | effort | eff_up | eff_down |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for algo in &algos {
        let classes: [(&str, Vec<Scenario>); 4] = [
            ("cold", vec![Scenario::ColdStart]),
            ("stable", vec![Scenario::Stable]),
            (
                "drop",
                deltas
                    .iter()
                    .filter(|&&d| d < 0)
                    .map(|&d| Scenario::Step { delta_pct: d })
                    .collect(),
            ),
            (
                "rise",
                deltas
                    .iter()
                    .filter(|&&d| d > 0)
                    .map(|&d| Scenario::Step { delta_pct: d })
                    .collect(),
            ),
        ];
        for (label, scens) in &classes {
            let mut agg = Quad::default();
            let mut n = 0u32;
            for &spm in &spms {
                for (si, scen) in scens.iter().enumerate() {
                    let seed =
                        base_seed.wrapping_add(hash_label(&algo.name, spm, si as i64 + 5000));
                    let ts = run_cell(algo, scen, spm, trials, seed);
                    let (_, sched) = scen.build(spm);
                    // For steps, attribute only the post-step transient.
                    let t_lo = if matches!(scen, Scenario::Step { .. }) {
                        STEP_EVENT_AT_SECS
                    } else {
                        0
                    };
                    agg.add(&mean_quad(&ts, &sched, t_lo, u64::MAX));
                    n += 1;
                }
            }
            let q = agg.scale(1.0 / n as f64);
            println!(
                "| {} | {} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} |",
                short(&algo.name),
                label,
                q.regret(),
                q.regret_over,
                q.regret_under,
                q.effort(),
                q.effort_up,
                q.effort_down,
            );
        }
    }

    // --- Q4: log-matched asymmetry — tests the e^(2a) prediction ----------
    // Tweak 2 (THEORY.md §8): if info rate is r*·e^(−e), a drop landing
    // at e=+a and a LOG-MATCHED rise landing at e=−a have equal e_step²
    // but regret ratio ≈ e^(2a). The default grid never tested matched
    // pairs (−50% is ln2 but +50% is only ln1.5). These are matched:
    //   ×0.5  / ×2.0  → −50% / +100%,  a=ln2 =0.693, predict ratio 4.00
    //   ×0.667/ ×1.5  → −33% / +50%,   a=ln1.5=0.405, predict ratio 2.25
    //   ×0.8  / ×1.25 → −20% / +25%,   a=ln1.25=0.223, predict ratio 1.56
    println!("\n## Q4 — Log-matched asymmetry: does regret_drop/regret_rise ≈ e^(2a)?\n");
    println!("Each row is a log-matched (drop, rise) pair with equal |e_step|=a.");
    println!("Tweak-2 predicts the integrated-regret ratio ≈ e^(2a), independent of algo.\n");
    let matched: [(i32, i32); 3] = [(-50, 100), (-33, 50), (-20, 25)];
    let window_min =
        (vardiff_sim::baseline::TRIAL_DURATION_SECS - STEP_EVENT_AT_SECS) as f64 / 60.0;
    for algo in &algos {
        println!("**{}**\n", short(&algo.name));
        println!("| drop% | rise% | a=|e_step| | predict e^2a | SPM6 | SPM12 | SPM30 |");
        println!("| --- | --- | --- | --- | --- | --- | --- |");
        for &(dn, up) in &matched {
            let a = (1.0 + dn as f64 / 100.0).ln().abs();
            print!(
                "| {} | +{} | {:.3} | {:.2} |",
                dn,
                up,
                a,
                (2.0 * a).exp()
            );
            for &spm in &[6.0f32, 12.0, 30.0] {
                let rd = step_regret(algo, dn, spm, trials, base_seed) * window_min;
                let ru = step_regret(algo, up, spm, trials, base_seed) * window_min;
                let ratio = if ru > 0.0 { rd / ru } else { f64::INFINITY };
                print!(" {:.2} |", ratio);
            }
            println!();
        }
        println!();
    }

    // --- Q5: bandwidth–variance product — tests V_ss·K·r* ≈ const --------
    // Tweak 1 (THEORY.md §8): the law that survives the data is not a
    // precision floor but a PRODUCT. V_ss = settled mean-e² (shrinks with
    // the algorithm's chosen window τ). K = transient regret coefficient
    // = integrated_regret / e_step² (grows with τ). If V_ss ∝ 1/(r*τ) and
    // K ∝ τ, then V_ss·K·r* is independent of τ — i.e. ~constant across
    // algorithms AT A FIXED SPM (and ∝ 1/r* across SPM). K is measured on
    // the −25% drop (moderate, window-limited). Wide spread across algos
    // ⇒ no product law; tight cluster ⇒ the conserved trade-off is real.
    println!("\n## Q5 — Bandwidth–variance product: is V_ss·K·r* ~constant across algos?\n");
    println!("V_ss = settled mean-e² (stable). K = post-step regret(−25%)/e_step². ");
    println!("Tweak-1 predicts V_ss·K·r* clusters across algos at each SPM.\n");
    println!("| SPM | algo | V_ss | K (min) | V_ss·K·r* |");
    println!("| --- | --- | --- | --- | --- |");
    let e25_sq = (1.0f64 + (-25.0) / 100.0).ln().powi(2);
    for &spm in &[6.0f32, 12.0, 30.0] {
        for algo in &algos {
            let seed = base_seed.wrapping_add(hash_label(&algo.name, spm, 1));
            let ts = run_cell(algo, &Scenario::Stable, spm, trials, seed);
            let (_, sched) = Scenario::Stable.build(spm);
            let v_ss = mean_quad(&ts, &sched, SETTLE_AFTER_SECS, u64::MAX).regret();
            let k = step_regret(algo, -25, spm, trials, base_seed) * window_min / e25_sq;
            println!(
                "| {} | {} | {:.5} | {:.4} | {:.4} |",
                spm as u32,
                short(&algo.name),
                v_ss,
                k,
                v_ss * k * spm as f64
            );
        }
    }

    // --- Q6: overshoot signature — the §8.4 prediction --------------------
    // §8.4 corrected law: regret ∝ e_step²·exp(2·⟨e⟩⁺), penalty tracks
    // OVER-DIFFICULTY STATE, not step direction. Falsifiable test: a big
    // UPWARD step (+100%) should manufacture over-difficulty via correction
    // overshoot, and that effect should GROW with SPM (detection cheap →
    // correction dynamics dominate). Metric: reg_over / regret on the
    // post-step transient. If over-fraction of +100% rises climbs with SPM
    // while −50% drops stay ~all-over, the state-dependent law holds.
    println!("\n## Q6 — Overshoot signature: over-difficulty fraction of the transient\n");
    println!("reg_over/regret on the post-step window. §8.4 predicts the +100% RISE");
    println!("over-fraction climbs with SPM (overshoot manufactures e>0); the −50% DROP");
    println!("stays ~1.0 at all SPM (a drop is over-difficulty by construction).\n");
    println!("| algo | step | SPM4 | SPM8 | SPM15 | SPM30 |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for algo in &algos {
        for &d in &[-50i32, 100] {
            print!("| {} | {:+}% |", short(&algo.name), d);
            for &spm in &[4.0f32, 8.0, 15.0, 30.0] {
                let scen = Scenario::Step { delta_pct: d };
                let seed = base_seed.wrapping_add(hash_label(&algo.name, spm, d as i64 + 13000));
                let ts = run_cell(algo, &scen, spm, trials, seed);
                let (_, sched) = scen.build(spm);
                let q = mean_quad(&ts, &sched, STEP_EVENT_AT_SECS, u64::MAX);
                let frac = if q.regret() > 0.0 {
                    q.regret_over / q.regret()
                } else {
                    f64::NAN
                };
                print!(" {:.2} |", frac);
            }
            println!();
        }
    }

    // --- Q7: counter-age cells — the missing regime (§8.2/§8.5 caveat) ----
    // Every steady-state-anchored number flatters the sluggish monolith
    // because it's measured on a YOUNG counter. SettledStep ages the
    // counter `settle_minutes` before stepping, then observes 60 min. The
    // monolith's reaction time is dominated by counter age (70fcb260): a
    // −50% drop is fast at a 5-min counter, glacial at 120 min. Integrated
    // post-step regret should EXPLODE for the monolith as the counter ages,
    // and stay flat for the age-independent EWMA contenders.
    println!("\n## Q7 — Counter-age regret (SettledStep −50%, integrated nats²·min over 60-min window)\n");
    println!("Ages the counter `settle` minutes, then drops 50%. §8.5: steady-state-anchored");
    println!("metrics miss this. Monolith regret should grow with counter age; EWMAs stay flat.\n");
    println!("| algo | SPM | settle=5m | settle=30m | settle=60m | settle=120m |");
    println!("| --- | --- | --- | --- | --- | --- |");
    let settles: [u64; 4] = [5, 30, 60, 120];
    for algo in &algos {
        for &spm in &[6.0f32, 12.0, 30.0] {
            print!("| {} | {} |", short(&algo.name), spm as u32);
            for &settle in &settles {
                let scen = Scenario::SettledStep {
                    settle_minutes: settle,
                    delta_pct: -50,
                };
                let seed =
                    base_seed.wrapping_add(hash_label(&algo.name, spm, settle as i64 + 17000));
                let ts = run_cell(algo, &scen, spm, trials, seed);
                let (_, sched) = scen.build(spm);
                // Post-step window is the 60-min observation phase.
                let event_at = settle * 60;
                let q = mean_quad(&ts, &sched, event_at, u64::MAX);
                print!(" {:.2} |", q.regret() * 60.0);
            }
            println!();
        }
    }

    // --- Q8: latency companion — adjudicates the Q7 reading -------------
    // Q7 showed counter-age regret is flat, against prediction. Two
    // readings: (1) bounded-window regret SATURATES and hides slow
    // detection, or (2) the monolith genuinely reacts fine. Decisive
    // test: measure mean time-to-first-fire after the step directly. If
    // latency GROWS with counter age while regret stayed flat → reading
    // (1), the regret metric is blind to slow detection (a real metric
    // defect). If latency is also flat → reading (2). Non-reactors are
    // counted at the full 60-min window (no survivor drop).
    println!("\n## Q8 — Reaction latency vs counter age (SettledStep −50%, mean min to first fire)\n");
    println!("Adjudicates Q7. Non-reacting trials counted at 60min (the window cap, not dropped).");
    println!("Latency growing with age + flat regret (Q7) ⇒ regret metric is saturation-blind.\n");
    println!("| algo | SPM | settle=5m | settle=30m | settle=60m | settle=120m |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for algo in &algos {
        for &spm in &[6.0f32, 12.0, 30.0] {
            print!("| {} | {} |", short(&algo.name), spm as u32);
            for &settle in &settles {
                let scen = Scenario::SettledStep {
                    settle_minutes: settle,
                    delta_pct: -50,
                };
                let seed =
                    base_seed.wrapping_add(hash_label(&algo.name, spm, settle as i64 + 21000));
                let ts = run_cell(algo, &scen, spm, trials, seed);
                let event_at = settle * 60;
                let window_cap = 60 * 60u64;
                let mut sum_lat = 0.0f64;
                for t in &ts {
                    let first = t
                        .ticks
                        .iter()
                        .find(|tk| tk.fired && tk.t_secs > event_at)
                        .map(|tk| (tk.t_secs - event_at) as f64)
                        .unwrap_or(window_cap as f64);
                    sum_lat += first;
                }
                print!(" {:.1} |", sum_lat / ts.len() as f64 / 60.0);
            }
            println!();
        }
    }

    // --- Q9: the REAL counter-age cross-check (§9.2 reconciliation) -----
    // Re-reading 70fcb260: the age-BLINDNESS it reports is for
    // ClassicComposed (3–30%, "functionally blind at high SPM") and
    // Parametric (<1%), NOT for VardiffState/the monolith (which it lists
    // as 100% age-independent). Q7/Q8 tested the monolith → no
    // contradiction, just the wrong algorithm. Q9 tests the algorithms
    // the commit actually flagged, using its methodology:
    //   detection RATE = P[fire within 60min] (not mean latency), and
    //   actual counter age at step (time since last fire) — the variable
    //   the blindness depends on. If detection collapses at high SPM AND
    //   the −10% mature detection is near-zero, 70fcb260 reproduces and
    //   regret should track it; if not, THAT is the contradiction.
    println!("\n## Q9 — Counter-age detection rate for the algorithms 70fcb260 flagged\n");
    println!("det = P[fire within 60min of step]; age = actual counter age at step (min);");
    println!("reg = integrated post-step regret. settle=60min, −10% drop (the commit's headline).\n");
    println!("| algo | SPM | det% | actual_age(min) | regret |");
    println!("| --- | --- | --- | --- | --- |");
    let blind_algos = vec![
        AlgorithmSpec::classic_composed(),
        AlgorithmSpec::parametric(),
        AlgorithmSpec::classic_vardiff_state(),
    ];
    for algo in &blind_algos {
        for &spm in &[6.0f32, 12.0, 30.0] {
            let scen = Scenario::SettledStep {
                settle_minutes: 60,
                delta_pct: -10,
            };
            let seed = base_seed.wrapping_add(hash_label(&algo.name, spm, 25000));
            let ts = run_cell(algo, &scen, spm, trials, seed);
            let (_, sched) = scen.build(spm);
            let event_at = 60 * 60u64;
            let window_end = event_at + 60 * 60;
            let mut detected = 0u32;
            let mut sum_age = 0.0f64;
            for t in &ts {
                if t.ticks.iter().any(|tk| tk.fired && tk.t_secs > event_at && tk.t_secs <= window_end) {
                    detected += 1;
                }
                // Actual counter age: time since last fire at/before step.
                let last_fire = t
                    .ticks
                    .iter()
                    .filter(|tk| tk.fired && tk.t_secs <= event_at)
                    .map(|tk| tk.t_secs)
                    .max()
                    .unwrap_or(0);
                sum_age += (event_at - last_fire) as f64;
            }
            let det = 100.0 * detected as f64 / ts.len() as f64;
            let age = sum_age / ts.len() as f64 / 60.0;
            let reg = mean_quad(&ts, &sched, event_at, u64::MAX).regret() * 60.0;
            println!(
                "| {} | {} | {:.0}% | {:.1} | {:.3} |",
                short(&algo.name),
                spm as u32,
                det,
                age,
                reg
            );
        }
    }

    // --- §10: three metric philosophies, same data, different rankings ---
    // Build one workload profile per algorithm over a realistic mix, then
    // score it three ways to show how the loss-shape choice changes WHO
    // WINS. The mix deliberately includes the −10% aged-counter cell (the
    // slow-degradation case e² is blind to) so the rankings actually
    // diverge. Aggregates: Rq=mean e² regret, Rl=mean |e| regret,
    // Det=mean −10% mature detection rate, Eff=mean fire effort.
    // (display label, spec) — explicit labels avoid the truncation
    // collision between the monolith and ClassicComposed.
    let panel: Vec<(&str, AlgorithmSpec)> = vec![
        ("monolith(prod)", AlgorithmSpec::classic_vardiff_state()),
        ("balanced", AlgorithmSpec::balanced()),
        ("react_priority", AlgorithmSpec::react_priority()),
        ("ClassicComposed", AlgorithmSpec::classic_composed()),
        ("Parametric", AlgorithmSpec::parametric()),
    ];
    let panel_spms: [f32; 4] = [6.0, 12.0, 20.0, 30.0];
    println!("\n## §10 — Same data, three metric philosophies (do rankings diverge?)\n");
    println!("Workload mix per algo over SPM {{6,12,20,30}}: Stable, ±50% & ±10% steps,");
    println!("and the settle60/−10% aged-counter cell (cold-start excluded). Rq=mean e², Rl=mean|e|, ");
    println!("Det=−10% mature detection rate, Eff=fire effort. Lower R/Eff better, higher Det better.\n");
    println!("| algo | Rq (e²) | Rl (|e|) | Det% | Eff |");
    println!("| --- | --- | --- | --- | --- |");
    struct Row {
        name: String,
        rq: f64,
        rl: f64,
        det: f64,
        eff: f64,
    }
    let mut rows: Vec<Row> = Vec::new();
    for (label, algo) in &panel {
        let mut agg = Quad::default();
        let mut n = 0u32;
        let mut det_sum = 0.0f64;
        let mut det_n = 0u32;
        for &spm in &panel_spms {
            // steady + transient scenarios (cold-start excluded: it is a
            // one-time event that dominates Rq and washes out the
            // detection differences these philosophies are meant to expose)
            let mut scens = vec![Scenario::Stable];
            for &d in &[-50i32, -10, 10, 50] {
                scens.push(Scenario::Step { delta_pct: d });
            }
            for (si, scen) in scens.iter().enumerate() {
                let seed = base_seed.wrapping_add(hash_label(&algo.name, spm, si as i64 + 30000));
                let ts = run_cell(algo, scen, spm, trials, seed);
                let (_, sched) = scen.build(spm);
                let t_lo = if matches!(scen, Scenario::Step { .. }) {
                    STEP_EVENT_AT_SECS
                } else {
                    0
                };
                agg.add(&mean_quad(&ts, &sched, t_lo, u64::MAX));
                n += 1;
            }
            // aged-counter −10% detection (the e²-blind case)
            let scen = Scenario::SettledStep {
                settle_minutes: 60,
                delta_pct: -10,
            };
            let seed = base_seed.wrapping_add(hash_label(&algo.name, spm, 31000));
            let ts = run_cell(algo, &scen, spm, trials, seed);
            let event_at = 60 * 60u64;
            let window_end = event_at + 60 * 60;
            let detected = ts
                .iter()
                .filter(|t| {
                    t.ticks
                        .iter()
                        .any(|tk| tk.fired && tk.t_secs > event_at && tk.t_secs <= window_end)
                })
                .count();
            det_sum += detected as f64 / ts.len() as f64;
            det_n += 1;
        }
        let q = agg.scale(1.0 / n as f64);
        rows.push(Row {
            name: label.to_string(),
            rq: q.regret(),
            rl: q.lin(),
            det: 100.0 * det_sum / det_n as f64,
            eff: q.effort(),
        });
    }
    for r in &rows {
        println!(
            "| {} | {:.4} | {:.4} | {:.0}% | {:.4} |",
            r.name, r.rq, r.rl, r.det, r.eff
        );
    }
    // Show the three rankings explicitly.
    let rank = |key: &dyn Fn(&Row) -> f64, asc: bool| -> String {
        let mut idx: Vec<usize> = (0..rows.len()).collect();
        idx.sort_by(|&a, &b| {
            let (ka, kb) = (key(&rows[a]), key(&rows[b]));
            if asc {
                ka.partial_cmp(&kb).unwrap()
            } else {
                kb.partial_cmp(&ka).unwrap()
            }
        });
        idx.iter()
            .map(|&i| rows[i].name.clone())
            .collect::<Vec<_>>()
            .join(" > ")
    };
    println!("\n**Ranking by philosophy** (best → worst):\n");
    println!("- *Quadratic regret only* (Rq, asc): {}", rank(&|r| r.rq, true));
    println!("- *Linear regret only* (Rl, asc): {}", rank(&|r| r.rl, true));
    println!(
        "- *Detection-first* (Det desc, ties by Rq): {}",
        rank(&|r| r.det, false)
    );
    println!(
        "- *Composite* (Rq + 0.5·Eff − 0.01·Det, asc): {}",
        rank(&|r| r.rq + 0.5 * r.eff - 0.01 * r.det / 100.0, true)
    );

    println!("\n(interpretation in docs/THEORY.md §5 & §8; this binary is the validation pass.)");
}

/// Time-averaged post-step mean-e² (the transient regret) for one
/// (algo, δ, SPM) cell. Shared by Q4 and Q5.
fn step_regret(algo: &AlgorithmSpec, delta_pct: i32, spm: f32, trials: usize, base_seed: u64) -> f64 {
    let scen = Scenario::Step { delta_pct };
    let seed = base_seed.wrapping_add(hash_label(&algo.name, spm, delta_pct as i64 + 9000));
    let ts = run_cell(algo, &scen, spm, trials, seed);
    let (_, sched) = scen.build(spm);
    mean_quad(&ts, &sched, STEP_EVENT_AT_SECS, u64::MAX).regret()
}

/// Compact display name: first letters of the three slash-separated
/// parts, else the whole thing truncated.
fn short(name: &str) -> String {
    if name.len() <= 22 {
        return name.to_string();
    }
    name.chars().take(22).collect()
}

/// Deterministic per-(algo,spm,bucket) seed offset so different cells
/// don't share trial seeds (avoids accidental correlation) while
/// staying fully reproducible.
fn hash_label(name: &str, spm: f32, bucket: i64) -> u64 {
    let mut h = 1469598103934665603u64; // FNV-1a offset
    for b in name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h ^= (spm as u64).wrapping_mul(2654435761);
    h ^= bucket as u64;
    h
}
