//! OPTION 4: the steady-vs-transient scatter at one rate, NO boundary line.
//!
//! Two hero premises have already died to measurement (steady-RMS floor sits
//! ABOVE the field; the field is not thin in steady-RMS). Before drawing any
//! frontier we scatter the field and READ whether one exists. Three reads,
//! per the spec:
//!   (1) do the points trace a convex lower-left envelope (a real frontier)?
//!   (2) are safe and unsafe configs SEPARATED along it (champion on the safe
//!       part) or INTERLEAVED (safety cuts across — messier story)?
//!   (3) is the champion ON the envelope (Pareto-optimal among safe) or
//!       INSIDE it (safe optimum but not Pareto — a different caption)?
//!
//! Axes (12 spm, the rate the trajectory plot used so it cross-checks):
//!   x = STEADY cost on a PURE stable stream: RMS(e) + ρ·effort. Purely
//!       steady — no step/transient content, so it can't double-count y.
//!   y = TRANSIENT lag: cold-start ramp-to-±10% (min) + aged −10% detection
//!       latency (min). Purely transient.
//!   color = decline-safety: settled e after a 10%/hr decline at 6 spm;
//!           FAIL if > GATE_PCT. Safety-failing points are drawn distinctly
//!           so the champion reads as the SAFE frontier, not the cost one.
//!
//! No τ-fitting, no boundary curve. This bin only places the points; the
//! boundary (option 3, theory curve checked against independently-measured
//! effective τ) comes ONLY IF this scatter shows a real frontier.
//!
//! Usage: cargo run --release --bin steady-transient
//! Env: VARDIFF_ST_TRIALS (default 400), VARDIFF_ST_SPM (default 12),
//!      VARDIFF_ST_OUT (svg, default docs/steady_transient.svg).

use std::env;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{
    Phase, Scenario, COLD_START_INITIAL_HASHRATE, DEFAULT_BASELINE_SEED, TRUE_HASHRATE,
};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::schedule::HashrateSchedule;
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

const RHO: f64 = 0.5;
const GATE_PCT: f64 = 5.0; // decline-safety: worst settled e above this = FAIL.
                           // Matches slow-decline.rs's runaway threshold (5%)
                           // EXACTLY — not the 2% used earlier, which was a
                           // different convention and would mis-color the band.

fn cfg(tau: u64, sens: f64) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{tau}/s{sens}"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(sens, 0.05, 8.0, 0.06, 0.6),
                6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05),
            1.0,
            clock,
        )))
    })
}
fn champion_new() -> AlgorithmSpec { cfg(360, 1.5) }

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() { return f64::NAN; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// x: steady cost on a pure stable stream = RMS(e)·100 + ρ·effort, where
/// effort = mean per-trial Σ(Δln D)² over the steady window (the actuation
/// the controller spends just to hold a stable load). Both from the same run.
fn steady_cost(a: &AlgorithmSpec, spm: f32, trials: usize, seed: u64) -> (f64, f64, f64) {
    let dur = 360 * 60u64;
    let warmup = 90 * 60u64;
    let (proto, sched) = Scenario::Stable.build(spm);
    let config = TrialConfig { duration_secs: dur, tick_interval_secs: 60, ..proto };
    let (mut sse, mut n) = (0.0f64, 0u64);
    let mut eff_trials: Vec<f64> = Vec::with_capacity(trials);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        let mut eff = 0.0f64;
        for tk in &t.ticks {
            if tk.t_secs <= warmup { continue; }
            let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln();
            sse += e * e; n += 1;
            if tk.fired {
                if let Some(nh) = tk.new_hashrate {
                    let old = tk.current_hashrate_before as f64;
                    if old > 0.0 && nh as f64 > 0.0 {
                        let dl = (nh as f64 / old).ln();
                        eff += dl * dl;
                    }
                }
            }
        }
        eff_trials.push(eff);
    }
    let rms = if n > 0 { (sse / n as f64).sqrt() * 100.0 } else { f64::NAN };
    let effort = median(eff_trials) * 100.0; // scale to be commensurate-ish with %
    (rms + RHO * effort, rms, effort)
}

/// y: transient lag = cold-start ramp-to-±10% (min) + aged −10% detection
/// latency (min). Cold start: estimate starts 1e5× low, true H flat; ramp =
/// first tick median within ±10% of truth. Detection: after a 60-min-aged
/// −10% drop, minutes until the median estimate falls halfway to the new truth.
fn transient_lag(a: &AlgorithmSpec, spm: f32, trials: usize, seed: u64) -> (f64, f64, f64) {
    let tick = 60u64;
    // --- cold-start ramp ---
    let ramp_end = 90 * 60u64;
    let sched_cold = HashrateSchedule::new(vec![(0, TRUE_HASHRATE)]);
    let cfg_cold = TrialConfig {
        duration_secs: ramp_end,
        initial_hashrate: COLD_START_INITIAL_HASHRATE,
        shares_per_minute: spm,
        tick_interval_secs: tick,
    };
    let n_cold = (ramp_end / tick) as usize;
    let mut cold_pt: Vec<Vec<f64>> = vec![Vec::with_capacity(trials); n_cold];
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, cfg_cold.clone(), &sched_cold, seed.wrapping_add(i as u64));
        for (ti, tk) in t.ticks.iter().enumerate() {
            if ti < n_cold { cold_pt[ti].push(tk.current_hashrate_before as f64); }
        }
    }
    let ramp_min = cold_pt.iter().enumerate()
        .find(|(_, c)| (median((*c).clone()) - TRUE_HASHRATE as f64).abs() <= 0.10 * TRUE_HASHRATE as f64)
        .map(|(ti, _)| (ti as f64 + 1.0) * tick as f64 / 60.0)
        .unwrap_or(f64::NAN);

    // --- aged −10% detection ---
    let drop_at = 60 * 60u64;
    let end = drop_at + 120 * 60u64;
    let drop_frac = 0.90f32;
    let mut phases = vec![Phase::Hold { secs: drop_at, h: TRUE_HASHRATE }];
    phases.push(Phase::Hold { secs: end - drop_at, h: TRUE_HASHRATE * drop_frac });
    let scen = Scenario::Custom { name: "aged_drop".into(), phases, initial_estimate: None };
    let (proto, sched_d) = scen.build(spm);
    let cfg_d = TrialConfig { tick_interval_secs: tick, ..proto };
    let n_d = (end / tick) as usize;
    let drop_tick = (drop_at / tick) as usize;
    let mut d_pt: Vec<Vec<f64>> = vec![Vec::with_capacity(trials); n_d];
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, cfg_d.clone(), &sched_d, seed.wrapping_add(0x5EED + i as u64));
        for (ti, tk) in t.ticks.iter().enumerate() {
            if ti < n_d { d_pt[ti].push(tk.current_hashrate_before as f64); }
        }
    }
    let med_d: Vec<f64> = d_pt.iter().map(|c| median(c.clone())).collect();
    let pre = median(med_d[drop_tick.saturating_sub(10)..drop_tick].to_vec());
    let drop_abs = TRUE_HASHRATE as f64 * (1.0 - drop_frac as f64);
    let threshold = pre - 0.5 * drop_abs;
    let detect_min = med_d.iter().enumerate().skip(drop_tick)
        .find(|(_, &h)| h <= threshold)
        .map(|(ti, _)| (ti - drop_tick) as f64 * tick as f64 / 60.0)
        .unwrap_or(f64::NAN); // NaN = never detected within window
    let detect_for_sum = if detect_min.is_nan() { 120.0 } else { detect_min }; // cap miss at window
    (ramp_min + detect_for_sum, ramp_min, detect_min)
}

/// Decline-safety: WORST settled e% over the rate range. MUST match the
/// authoritative slow-decline.rs EXACTLY — same rate×spm grid, same decline
/// profile, same recovery window — or the coloring reintroduces the very
/// one-rate-inconsistency this whole arc keeps catching. An earlier version
/// used a partial grid (max 20%/hr) and missed the 40%/hr·2spm cell where the
/// sleepy Ewma480/720 family actually fails (+5.6%), wrongly coloring them
/// safe. The full authoritative grid is rates {1,2,5,10,20,40}%/hr ×
/// spm {2,4,6,8,12,20,30}; the worst (most positive = most over-difficulty)
/// settled-e over the whole grid is the gate quantity.
fn decline_settled_e(a: &AlgorithmSpec, base_trials: usize, seed: u64) -> f64 {
    let rates = [1.0f32, 2.0, 5.0, 10.0, 20.0, 40.0];
    let spms = [2.0f32, 4.0, 6.0, 8.0, 12.0, 20.0, 30.0];
    let mut worst = f64::MIN;
    let mut k = 0u64;
    for &spm in &spms {
        // CI-match low spm like slow-decline.rs (median is robust, modest base).
        let cell_trials = (base_trials as f64 * (60.0 / spm as f64).max(1.0)).round() as usize;
        for &r in &rates {
            let e = decline_settled_e_cell(a, r, spm, cell_trials, seed.wrapping_add(k << 40));
            if e > worst { worst = e; }
            k += 1;
        }
    }
    worst
}

/// Settled e% after a (rate_pct_per_hr) decline at `spm`, after recovery window.
fn decline_settled_e_cell(a: &AlgorithmSpec, rate_pph: f32, spm: f32, trials: usize, seed: u64) -> f64 {
    let mature = 60u64; let rate = rate_pph / 100.0 / 60.0; let target = 0.50f32; let observe = 120u64;
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
    let config = TrialConfig { tick_interval_secs: 60, ..proto };
    let d_end = (mature + dm) * 60; let trial_end = d_end + observe * 60;
    let mut settled = Vec::with_capacity(trials);
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
    median(settled)
}

#[derive(Clone)]
struct Pt {
    label: String,
    x: f64, // steady cost
    y: f64, // transient lag
    rms: f64, effort: f64, ramp: f64, detect: f64,
    settled: f64,
    named: u8, // 0 field, 1 new champ, 2 old champ, 3 classic
}

fn main() -> std::io::Result<()> {
    let trials: usize = env::var("VARDIFF_ST_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(400);
    let spm: f32 = env::var("VARDIFF_ST_SPM").ok().and_then(|s| s.parse().ok()).unwrap_or(12.0);
    let out = env::var("VARDIFF_ST_OUT").unwrap_or_else(|_| "docs/steady_transient.svg".to_string());
    let seed = DEFAULT_BASELINE_SEED;

    let taus = [120u64, 150, 240, 360, 480, 720];
    let senss = [0.3f64, 0.6, 1.0, 1.5, 2.0];
    let mut specs: Vec<(AlgorithmSpec, u8)> = Vec::new();
    for &tau in &taus { for &sens in &senss {
        let named = if tau == 360 && (sens-1.5).abs()<1e-9 { 1 } else if tau==150 && (sens-0.3).abs()<1e-9 { 2 } else { 0 };
        specs.push((cfg(tau, sens), named));
    }}
    specs.push((AlgorithmSpec::classic_composed(), 3));
    // champion_new and old champion are already in the grid (360/s1.5, 150/s0.3);
    // keep the canonical champion() too in case its params differ from the grid cell.
    let _ = champion_new;

    eprintln!("steady-transient: {} configs, {} trials, spm {}", specs.len(), trials, spm);
    let next = AtomicUsize::new(0);
    let pts: Mutex<Vec<Pt>> = Mutex::new(Vec::new());
    let n_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..n_threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= specs.len() { break; }
                let (a, named) = &specs[i];
                let (x, rms, effort) = steady_cost(a, spm, trials, seed ^ 0x57EAD);
                let (y, ramp, detect) = transient_lag(a, spm, trials, seed ^ 0x712A);
                // Safety base-trials kept modest: 42 cells × CI-scaling already
                // makes this the dominant cost, and the median settled-e is
                // robust. 80 base (→ up to 2400 at 2spm) matches slow-decline's
                // robustness without a 10-minute scatter.
                let settled = decline_settled_e(a, 80, seed ^ 0xDEC11E);
                pts.lock().unwrap().push(Pt {
                    label: a.name.clone(), x, y, rms, effort, ramp, detect, settled, named: *named,
                });
                eprintln!("  {} done", a.name);
            });
        }
    });
    let mut pts = pts.into_inner().unwrap();
    pts.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());

    // ---- the three reads, as console output ----
    println!("\n## Steady-vs-transient scatter readout (spm {}). Lower-left = better on both.", spm);
    println!("safe = decline settled e ≤ {:.0}% (otherwise UNSAFE).\n", GATE_PCT);
    println!("| config | steady cost | (rms / effort) | transient lag | (ramp / detect) | settled e% | safe? |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for p in &pts {
        let safe = if p.settled <= GATE_PCT { "safe" } else { "UNSAFE" };
        let tag = match p.named { 1=>" «NEW", 2=>" «OLD", 3=>" «CLASSIC", _=>"" };
        let det = if p.detect.is_nan() { "MISS".to_string() } else { format!("{:.0}", p.detect) };
        println!("| {}{} | {:.2} | ({:.1}/{:.1}) | {:.0} | ({:.0}/{}) | {:+.1} | {} |",
            p.label, tag, p.x, p.rms, p.effort, p.y, p.ramp, det, p.settled, safe);
    }

    // READ 1: is there a convex lower-left envelope? Compute the Pareto-
    // minimal set (no other point is ≤ on BOTH axes) and report it.
    let mut pareto: Vec<&Pt> = Vec::new();
    for p in &pts {
        let dominated = pts.iter().any(|q|
            (q.x < p.x - 1e-9 && q.y <= p.y + 1e-9) || (q.y < p.y - 1e-9 && q.x <= p.x + 1e-9));
        if !dominated { pareto.push(p); }
    }
    pareto.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    println!("\n[READ 1 — frontier exists?] Pareto-minimal set ({} of {} configs):", pareto.len(), pts.len());
    for p in &pareto {
        let safe = if p.settled <= GATE_PCT { "safe" } else { "UNSAFE" };
        println!("   {:22} steady {:.2}, transient {:.0}  [{}]", p.label, p.x, p.y, safe);
    }
    // READ 2: are safe/unsafe separated along the frontier, or interleaved?
    let safe_on_front = pareto.iter().filter(|p| p.settled <= GATE_PCT).count();
    println!("\n[READ 2 — safety separation] {} of {} Pareto points are safe.", safe_on_front, pareto.len());
    println!("   (separated = safe points cluster on one part of the frontier; interleaved = mixed along it)");
    // READ 3: is the champion ON the Pareto front or INSIDE it?
    if let Some(champ) = pts.iter().find(|p| p.named == 1) {
        let on = pareto.iter().any(|p| std::ptr::eq(*p, champ));
        println!("\n[READ 3 — champion placement] new champion (Ewma360/s1.5): {} the Pareto front.",
            if on { "ON" } else { "INSIDE (not Pareto-optimal — different caption)" });
        // among SAFE configs only, is it Pareto-optimal?
        let dominated_by_safe = pts.iter().any(|q| q.settled <= GATE_PCT &&
            ((q.x < champ.x - 1e-9 && q.y <= champ.y + 1e-9) || (q.y < champ.y - 1e-9 && q.x <= champ.x + 1e-9)));
        println!("   Among SAFE configs only, the champion is {}.",
            if dominated_by_safe { "dominated (a safe config beats it on both)" } else { "Pareto-optimal (the safe frontier)" });
    }

    // ---- the reads the verdict TABLE cannot give (only the render can): ----
    let safe: Vec<&Pt> = pts.iter().filter(|p| p.settled <= GATE_PCT).collect();
    println!("\n[render-read A — is EVERY below-left point of the champion red?]");
    if let Some(champ) = pts.iter().find(|p| p.named == 1) {
        let below_left: Vec<&Pt> = pts.iter()
            .filter(|q| !std::ptr::eq(*q, champ) && q.x <= champ.x + 1e-9 && q.y <= champ.y + 1e-9
                        && (q.x < champ.x - 1e-9 || q.y < champ.y - 1e-9))
            .collect();
        let green_bl: Vec<&&Pt> = below_left.iter().filter(|q| q.settled <= GATE_PCT).collect();
        println!("   {} points sit below-left of the champion; {} of them are SAFE (green).",
            below_left.len(), green_bl.len());
        if green_bl.is_empty() {
            println!("   => champion is THE safe frontier (no safe point dominates it). Caption: 'the safe frontier'.");
        } else {
            println!("   => champion is A safe point, NOT the frontier. Safe points below-left:");
            for q in &green_bl { println!("      {:22} steady {:.2}, transient {:.0} (settled {:+.1})", q.label, q.x, q.y, q.settled); }
        }
    }
    // safe-subset Pareto envelope (convex & worth drawing?)
    let mut safe_front: Vec<&Pt> = safe.iter().filter(|p| {
        !safe.iter().any(|q| (q.x < p.x - 1e-9 && q.y <= p.y + 1e-9) || (q.y < p.y - 1e-9 && q.x <= p.x + 1e-9))
    }).cloned().collect();
    safe_front.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    println!("\n[render-read B — do the SAFE points form a clean envelope?] safe-Pareto set ({} of {} safe):", safe_front.len(), safe.len());
    for p in &safe_front { println!("   {:22} steady {:.2}, transient {:.0}", p.label, p.x, p.y); }
    // convexity check: is the safe frontier convex (monotone-decreasing slope)?
    let convex = {
        let mut ok = true;
        for w in safe_front.windows(3) {
            let (s1,s2) = ((w[1].y-w[0].y)/(w[1].x-w[0].x), (w[2].y-w[1].y)/(w[2].x-w[1].x));
            if s2 < s1 - 1e-6 { ok = false; }
        }
        ok
    };
    println!("   safe frontier is {} ({} points).", if convex {"CONVEX (clean envelope)"} else {"RAGGED (non-convex)"}, safe_front.len());
    println!("\n[render-read C — separation] reds vs greens: {} unsafe, {} safe of {} total.",
        pts.len()-safe.len(), safe.len(), pts.len());

    let svg = render(&pts, &safe_front, spm);
    fs::write(&out, &svg)?;
    eprintln!("Wrote {}", out);
    Ok(())
}

fn render(pts: &[Pt], safe_front: &[&Pt], spm: f32) -> String {
    let (ml, mr, mt, mb) = (74.0, 240.0, 78.0, 60.0);
    let (w, h) = (1040i64, 540i64);
    let pw = w as f64 - ml - mr;
    let ph = h as f64 - mt - mb;
    let xs: Vec<f64> = pts.iter().map(|p| p.x).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p.y).collect();
    let (x_lo, x_hi) = (0.0, xs.iter().cloned().fold(f64::MIN, f64::max) * 1.1);
    let (y_lo, y_hi) = (0.0, ys.iter().cloned().fold(f64::MIN, f64::max) * 1.1);
    let lx = |x: f64| ml + pw * (x - x_lo) / (x_hi - x_lo);
    let ly = |y: f64| mt + ph * (1.0 - (y - y_lo) / (y_hi - y_lo));

    let mut s = String::new();
    s.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" font-family="system-ui, sans-serif" font-size="13">
<rect width="100%" height="100%" fill="#fafafa"/>
<text x="{cx}" y="28" text-anchor="middle" font-size="16" font-weight="bold">The champion is the SAFE frontier: steady cost vs transient lag</text>
<text x="{cx}" y="46" text-anchor="middle" font-size="12" fill="#555">Each point a config at spm={spm}. x = steady tracking+effort, y = cold-start+detection lag. Lower-left better.</text>
<text x="{cx}" y="62" text-anchor="middle" font-size="12" fill="#555">Color = decline-safety (worst over the cross-rate grid). Dashed line = safe-config envelope.</text>
"##,
        cx = w / 2
    ));
    // gridlines
    for k in 0..=5 {
        let gx = x_lo + (x_hi - x_lo) * k as f64 / 5.0;
        let gy = y_lo + (y_hi - y_lo) * k as f64 / 5.0;
        s.push_str(&format!(
            r##"<line x1="{:.1}" y1="{mt:.1}" x2="{:.1}" y2="{:.1}" stroke="#ebebeb" stroke-width="1"/>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="10" fill="#aaa">{:.1}</text>
<line x1="{ml:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#ebebeb" stroke-width="1"/>
<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10" fill="#aaa">{:.0}</text>
"##,
            lx(gx), lx(gx), mt+ph, lx(gx), mt+ph+16.0, gx,
            ly(gy), ml+pw, ly(gy), ml-8.0, ly(gy)+4.0, gy
        ));
    }
    s.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#666">steady cost  (RMS tracking + ρ·effort, lower better) →</text>
<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#666" transform="rotate(-90 {:.1} {:.1})">transient lag  (cold-start + detection, min, lower better) →</text>
"##,
        ml + pw/2.0, mt+ph+40.0, ml-52.0, mt+ph/2.0, ml-52.0, mt+ph/2.0
    ));

    // safe-Pareto envelope (the empirical boundary — drawn through GREEN
    // points only; the caption states it is empirical, anchored on the
    // transient axis by the CUSUM floor, NOT a theory floor on steady error).
    if safe_front.len() >= 2 {
        let mut d = String::new();
        for (i, p) in safe_front.iter().enumerate() {
            d.push_str(&format!("{}{:.1},{:.1} ", if i==0 {"M"} else {"L"}, lx(p.x), ly(p.y)));
        }
        s.push_str(&format!(
            r##"<path d="{}" fill="none" stroke="#2e8b2e" stroke-width="1.6" stroke-opacity="0.5" stroke-dasharray="5 4"/>
"##, d.trim()));
    }

    // points
    for p in pts {
        // Guard non-finite transient lag (a config whose detection never
        // resolves at this rate — e.g. the long-window sleepy corner at high
        // spm). Such a point has no y position; drop it rather than emit a
        // cy="NaN" circle that breaks the SVG. (It is unsafe regardless and
        // not on the frontier; its absence does not change the claim.)
        if !p.x.is_finite() || !p.y.is_finite() {
            continue;
        }
        let (x, y) = (lx(p.x), ly(p.y));
        let safe = p.settled <= GATE_PCT;
        let (fill, stroke) = if safe { ("#2e8b2e", "#1d5e1d") } else { ("#fff", "#e41a1c") };
        let r = match p.named { 0 => 4.0, _ => 7.0 };
        s.push_str(&format!(
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="{r}" fill="{fill}" stroke="{stroke}" stroke-width="{}"/>
"##,
            if safe {1.0} else {2.0}
        ));
        if p.named != 0 {
            let txt = match p.named { 1=>"new champion", 2=>"old champion", 3=>"classic", _=>"" };
            s.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="11" font-weight="bold" fill="#222">{txt}</text>
"##,
                x + 10.0, y + 3.0
            ));
        }
    }
    // legend
    let lgx = ml + pw + 24.0; let mut lgy = mt + 10.0;
    s.push_str(&format!(
        r##"<circle cx="{:.1}" cy="{lgy:.1}" r="6" fill="#2e8b2e" stroke="#1d5e1d"/>
<text x="{:.1}" y="{:.1}" font-size="12">decline-safe</text>
"##, lgx+6.0, lgx+20.0, lgy+4.0));
    lgy += 24.0;
    s.push_str(&format!(
        r##"<circle cx="{:.1}" cy="{lgy:.1}" r="6" fill="#fff" stroke="#e41a1c" stroke-width="2"/>
<text x="{:.1}" y="{:.1}" font-size="12">UNSAFE (fails decline gate)</text>
"##, lgx+6.0, lgx+20.0, lgy+4.0));
    lgy += 30.0;
    s.push_str(&format!(
        r##"<line x1="{lgx:.1}" y1="{lgy:.1}" x2="{:.1}" y2="{lgy:.1}" stroke="#2e8b2e" stroke-width="1.6" stroke-opacity="0.6" stroke-dasharray="5 4"/>
<text x="{:.1}" y="{:.1}" font-size="12" fill="#555">safe-config envelope</text>
"##, lgx + 24.0, lgx + 30.0, lgy + 4.0));
    lgy += 30.0;
    s.push_str(&format!(
        r##"<text x="{lgx:.1}" y="{:.1}" font-size="11" fill="#555" font-style="italic">The champion is the lower-left-most</text>
<text x="{lgx:.1}" y="{:.1}" font-size="11" fill="#555" font-style="italic">SAFE point: every config that beats</text>
<text x="{lgx:.1}" y="{:.1}" font-size="11" fill="#555" font-style="italic">it on both axes fails the decline gate.</text>
"##, lgy, lgy+16.0, lgy+32.0));

    s.push_str("</svg>\n");
    s
}
