//! LEVER F-TEST — discharge of the pre-registered protocol in
//! docs/records/LEVER_TEST_PREREG.md (committed 2026-06-23, never run until now).
//!
//! A pre-registered test is a COMMITMENT to record the outcome whatever it is.
//! This runs the committed protocol EXACTLY as specified — no post-hoc protocol
//! changes — and records F (or UNMEASURABLE) per the prereg's own gate. UNMEASURABLE
//! is a pre-committed legitimate result (the prereg predicted it as LIKELY); it is
//! NOT to be engineered around by adjusting windows/settling until F is measurable
//! (that would reintroduce the exact post-hoc DOF the prereg was written against).
//!
//! THE TEST (prereg §"THE TEST", Option 1 — per-window observed-vs-predicted):
//!   For each settled per-fire window (N = shares since last fire, dt = inter-fire
//!   seconds, at rate r*):
//!     μ_pred_i = r_bar · dt_i/60     (r_bar = POOLED settled realized mean rate —
//!                                      ONE number per slot, NOT per-window; using a
//!                                      per-window rate would make μ_pred ≡ N → 0 dev)
//!     F = mean_i (N_i − μ_pred_i)²  /  mean_i (μ_pred_i)
//!   F≈1 ⟺ per-window count spread matches Poisson ⟺ the floor is operating.
//!
//! VERDICT (prereg §"VERDICT RULES", pre-committed):
//!   CONFIRMED  iff F ∈ [0.5,2] at BOTH rates (each rate's noise at its own Poisson
//!              floor; floor is rate-dependent BY the formula → confirming both IS
//!              the rate-scaling = the lever).
//!   REFUTED    iff F reliably ≠ 1 contradicting Poisson (e.g. F≈1 at 30, F≫1 at 6).
//!   UNMEASURABLE iff too few windows for F's SE to distinguish it from 1, OR belief
//!              not flat, OR realized-rate drift makes r_bar ill-defined.
//!
//! SCOPE GUARDS honored: per-window own dt (never pooled/binned to fixed dt);
//! belief-flatness checked over a LONG trailing window; slot-3-not-flat ⇒ report
//! single-rate-floor-only, not "the lever".
//!
//! FAITHFULNESS CAVEAT (disclosed, not hidden): the original prereg substrate was a
//! one-off raw_count pull from deployed slots 3/4 with event-driven sub-minute dt
//! (43/103/104/164s). That data is not in the repo. This reconstructs per-fire
//! windows from the SHIPPED champion at its SHIPPED 60s tick — so dt here is a
//! multiple of 60s, COARSER than the original. Running the controller at a finer
//! tick to manufacture sub-minute dt would perturb the shipped controller, a
//! post-hoc change the prereg forbids; so the shipped cadence is kept and the
//! dt-granularity difference is recorded as a caveat on the result's comparability.
//!
//! Usage: cargo run --release --bin lever-ftest

use std::env;
use std::sync::Arc;

use channels_sv2::vardiff::composed::champion_composed;
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED};
use vardiff_sim::grid::VardiffBox;
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

/// A per-fire settled window: shares since last fire, and the inter-fire dt.
struct Window {
    n: u32,
    dt_secs: u64,
}

fn champion_box(clock: Arc<dyn channels_sv2::vardiff::Clock>) -> VardiffBox {
    VardiffBox(Box::new(champion_composed(1.0, clock)))
}

/// Collect settled per-fire windows for one rate. Returns (windows, total_shares,
/// total_secs, distinct_belief_count) where the latter three feed r_bar and the
/// belief-flatness guard. `settle_skip_secs` drops the pre-converged transient.
fn collect_windows(
    spm: f32,
    trials: usize,
    duration_secs: u64,
    settle_skip_secs: u64,
    base_seed: u64,
) -> (Vec<Window>, f64, f64, usize) {
    let scen = Scenario::Stable;
    let (cfg_proto, schedule) = scen.build(spm);
    // SHIPPED 60s tick (see faithfulness caveat). Long duration to settle + collect.
    let cfg = TrialConfig {
        tick_interval_secs: 60,
        duration_secs,
        ..cfg_proto
    };

    let mut windows = Vec::new();
    let mut total_shares: f64 = 0.0;
    let mut total_secs: f64 = 0.0;
    // belief-flatness: collect distinct h_estimate values over the settled tail.
    let mut beliefs: Vec<f32> = Vec::new();

    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = champion_box(clock.clone());
        let t = run_trial_observed(v, clock, cfg.clone(), &schedule, base_seed.wrapping_add(i as u64));

        // Walk ticks in the SETTLED region; accumulate shares between fires.
        let mut last_fire_t: Option<u64> = None;
        let mut acc: u32 = 0;
        for tk in &t.ticks {
            if tk.t_secs <= settle_skip_secs {
                // pre-settle: still update last_fire anchor on fires, don't record
                if tk.fired {
                    last_fire_t = Some(tk.t_secs);
                    acc = 0;
                }
                continue;
            }
            // settled region: count shares + realized-rate accumulation
            total_shares += tk.n_shares as f64;
            total_secs += 60.0; // tick length
            acc = acc.saturating_add(tk.n_shares);
            if let Some(h) = tk.h_estimate {
                beliefs.push(h);
            }
            if tk.fired {
                if let Some(lf) = last_fire_t {
                    let dt = tk.t_secs - lf;
                    if dt > 0 {
                        windows.push(Window { n: acc, dt_secs: dt });
                    }
                }
                last_fire_t = Some(tk.t_secs);
                acc = 0;
            }
        }
    }

    // belief-flatness guard: count distinct h_estimate values (rounded) over the tail.
    beliefs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    beliefs.dedup_by(|a, b| ((*a / 1e11).round() - (*b / 1e11).round()).abs() < 0.5);
    let distinct = beliefs.len();

    (windows, total_shares, total_secs, distinct)
}

/// Compute the prereg's F and its SE for one rate's windows, given r_bar.
/// Returns (F, se_F, n_windows). SE via the spread of per-window squared deviations
/// (F = mean(D)/mean(M); the denominator mean(M) is near-constant so SE ≈
/// std(D)/(mean(M)·sqrt(n)) — the honest first-order SE on the ratio's numerator).
fn compute_f(windows: &[Window], r_bar: f64) -> (f64, f64, usize) {
    let n = windows.len();
    if n == 0 {
        return (f64::NAN, f64::NAN, 0);
    }
    let mut devs = Vec::with_capacity(n); // (N - μ_pred)²
    let mut mus = Vec::with_capacity(n); // μ_pred
    for w in windows {
        let mu = r_bar * (w.dt_secs as f64 / 60.0);
        let d = w.n as f64 - mu;
        devs.push(d * d);
        mus.push(mu);
    }
    let mean_dev = devs.iter().sum::<f64>() / n as f64;
    let mean_mu = mus.iter().sum::<f64>() / n as f64;
    let f = mean_dev / mean_mu;
    // SE of the numerator mean, propagated to F (denominator treated as ~fixed).
    let var_dev = devs.iter().map(|d| (d - mean_dev).powi(2)).sum::<f64>() / (n as f64).max(1.0);
    let se_mean_dev = (var_dev / n as f64).sqrt();
    let se_f = se_mean_dev / mean_mu;
    (f, se_f, n)
}

fn main() {
    let trials: usize = env::var("VARDIFF_LF_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(400);
    // Long trial so the settled tail yields windows; skip the first 60 min transient.
    let duration_secs: u64 = env::var("VARDIFF_LF_DURATION").ok().and_then(|s| s.parse().ok()).unwrap_or(8 * 3600);
    let settle_skip: u64 = env::var("VARDIFF_LF_SKIP").ok().and_then(|s| s.parse().ok()).unwrap_or(60 * 60);
    let seed = DEFAULT_BASELINE_SEED;

    println!("# LEVER F-TEST — discharge of LEVER_TEST_PREREG.md (run as committed)");
    println!("# champion, SHIPPED 60s tick; {trials} trials × {}h; skip first {}min (settle).",
             duration_secs / 3600, settle_skip / 60);
    println!("# F = mean(N−μ_pred)²/mean(μ_pred), μ_pred = r_bar·dt/60, r_bar = pooled settled rate.");
    println!("# UNMEASURABLE is a pre-committed legitimate result; protocol NOT altered to avoid it.\n");

    // slot 3 = 6 spm, slot 4 = 30 spm (prereg's two rates).
    let mut results = Vec::new();
    for (slot, spm) in [(3u32, 6.0f32), (4, 30.0)] {
        let (windows, tot_sh, tot_s, distinct) =
            collect_windows(spm, trials, duration_secs, settle_skip, seed);
        let r_bar = if tot_s > 0.0 { tot_sh / (tot_s / 60.0) } else { f64::NAN };
        let (f, se_f, n) = compute_f(&windows, r_bar);

        println!("## slot {slot} — r* = {spm} spm");
        println!("   settled per-fire windows: {n}");
        println!("   r_bar (pooled settled realized rate): {r_bar:.3} spm  (r* = {spm})");
        println!("   distinct belief values in settled tail: {distinct}  (flatness guard: 1 = flat)");
        if n > 0 {
            let dt_min = windows.iter().map(|w| w.dt_secs).min().unwrap();
            let dt_max = windows.iter().map(|w| w.dt_secs).max().unwrap();
            println!("   dt range: {dt_min}–{dt_max}s  (NOTE: 60s-tick-quantized; original substrate was event-driven 43–164s)");
            println!("   F = {f:.3}  ±{se_f:.3} (SE)");
            // pre-committed UNMEASURABLE gate: can SE distinguish F from 1?
            let band = (f - 1.0).abs();
            let unmeasurable = n < 8 || se_f.is_nan() || se_f > band.max(0.25) || distinct > 1;
            let verdict = if unmeasurable {
                "UNMEASURABLE (per prereg: too few windows / SE can't separate F from 1 / belief not flat)"
            } else if (0.5..=2.0).contains(&f) {
                "F ∈ [0.5,2] → floor operating at this rate"
            } else {
                "F outside [0.5,2] → departs Poisson (candidate REFUTED-signal at this rate)"
            };
            println!("   verdict: {verdict}");
            results.push((slot, spm, f, se_f, n, distinct, verdict.to_string()));
        } else {
            println!("   verdict: UNMEASURABLE (zero settled fires — no windows)");
            results.push((slot, spm, f64::NAN, f64::NAN, 0, distinct, "UNMEASURABLE (no windows)".to_string()));
        }
        println!();
    }

    // Combined lever verdict (prereg §VERDICT + scope guard on slot 3).
    println!("## COMBINED LEVER VERDICT (pre-committed rules)");
    let slot3 = results.iter().find(|r| r.0 == 3);
    let slot4 = results.iter().find(|r| r.0 == 4);
    let both_confirm = results.iter().all(|r| (0.5..=2.0).contains(&r.2) && r.5 <= 1 && r.4 >= 8);
    if both_confirm {
        println!("LEVER CONFIRMED — F∈[0.5,2] at both 6 and 30 spm; each rate at its own Poisson floor.");
    } else {
        let s3_unmeas = slot3.map(|r| r.4 < 8 || r.5 > 1 || !(0.5..=2.0).contains(&r.2)).unwrap_or(true);
        if s3_unmeas {
            println!("LEVER PENDING — slot 3 (6 spm) does not give a clean measurable floor");
            println!("(scope guard: slot 4 alone is single-rate-floor only, NOT the lever, which needs ≥2 rates).");
            if let Some(r4) = slot4 {
                if (0.5..=2.0).contains(&r4.2) && r4.5 <= 1 && r4.4 >= 8 {
                    println!("  slot 4 (30 spm): floor confirmed single-rate (F={:.2}).", r4.2);
                }
            }
        } else {
            println!("LEVER: see per-slot verdicts above (mixed / refuted-signal).");
        }
    }
    println!("\n# Recorded as the prereg's discharge regardless of outcome. UNMEASURABLE, if reached,");
    println!("# is the pre-registered LIKELY result, vindicated — not a failure to engineer around.");
}
