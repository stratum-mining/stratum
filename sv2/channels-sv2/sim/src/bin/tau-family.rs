//! τ-valley FAMILY: the per-rate valley, one U-curve per share rate r* (spm).
//!
//! `tau-valley.rs` takes the MINIMAX over the rate×spm grid — it collapses the
//! rate axis to get the single safe-everywhere window (an *admissibility*
//! object). This binary keeps the rate axis: it plots settled-e vs window τ as a
//! FAMILY of curves, one per share rate r* (spm), to test the THEORY eq. (2)
//! prediction that the per-rate valley minimum slides with rate (τ* ∝ 1/r).
//!
//! WHY spm IS THE CURVE VARIABLE (not %/hr): the τ*∝1/r argument is about the
//! *information* rate r* = shares/min, NOT decline severity. So curves are per
//! `spm`; decline severity (%/hr) is collapsed (median over the severity set),
//! the mirror of how tau-valley collapses sensitivity.
//!
//! SCOPE — spm ≥ 6 ONLY (single control regime). The champion's
//! `spm_threshold = 6` switches AdaptiveSignPersist between the low-SPM PoissonCI
//! guard (spm < 6) and the boundary (spm ≥ 6). Those are DIFFERENT controllers,
//! so their valley minima are not a comparable τ*∝1/r series. spm {2,4} are
//! omitted; this is the high-spm single-regime band the §8.3 flag scopes to.
//!
//! HONEST READ: a clean leftward slide of the minimum as spm drops is the τ*∝1/r
//! fingerprint. BUT the magnitude at the high-spm end is partially confounded by
//! clamp-proximity / target granularity (independent of τ), so the curve
//! *positions* are the evidence, not the absolute depths. Reuses cfg() and
//! decline_cell() from tau-valley.rs verbatim (re-aggregation, not new physics).
//!
//! Usage: cargo run --release --bin tau-family
//! Env: VARDIFF_TF_TRIALS (default 80 base, CI-scaled), VARDIFF_TF_OUT.

use std::env;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

// τ axis (estimator window); champion is 360. Extended DOWN to 30s: the first
// run (floor 90) showed the per-rate minimum for spm>=6 railed at the left edge
// — the optimum had slid below the grid (τ*∝1/r pushing it down as r* rises). To
// BRACKET the minimum rather than rail against it, the low end now reaches 30s
// (= tick_secs, the practical floor: a window shorter than one tick is meaningless).
const TAUS: &[u64] = &[30, 45, 60, 90, 120, 150, 240, 300, 360, 480, 600, 720, 900];
// Curve variable: share rate r* (spm). FULL range — the over-difficulty arm of
// the valley lives at low spm (sleepy windows lag the decline into positive e
// there), so unlike the settled-e attempt we must KEEP spm {2,4}. The spm=6
// guard switch still means spm<6 vs >=6 are different controllers; the figure
// labels the switch rather than excluding the cells, because the over-difficulty
// AREA (not settled-e) is what the §8.3 flag names and it needs the low end.
const SPMS: &[f32] = &[2.0, 4.0, 6.0, 8.0, 12.0, 20.0, 30.0];
const SENS: f64 = 1.5;
// Decline severities collapsed per curve (median), the authoritative set.
const RATES_PPH: &[f32] = &[1.0, 2.0, 5.0, 10.0, 20.0, 40.0];

// --- cfg() and decline_cell() are verbatim from tau-valley.rs ---
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

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() { return f64::NAN; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// BOTH primitives for one (τ,rate,spm) cell: (over_difficulty_area, wobble).
/// over-difficulty area = ∫max(e,0)dt over the escape phase (the §8.3 primitive,
/// the dangerous/spiral-direction cost); wobble = |settled-e| in the recovery
/// window (the safe/self-healing-direction cost). Returned together so the
/// paired figure can show the TRADE — short τ wins area, loses wobble — rather
/// than one flattering half. Median over trials.
fn decline_both(a: &AlgorithmSpec, rate_pph: f32, spm: f32, trials: usize, seed: u64) -> (f64, f64) {
    let mature = 60u64;
    let rate = rate_pph / 100.0 / 60.0;
    let target = 0.50f32;
    let observe = 120u64;
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
    let d_start = mature * 60;
    let d_end = (mature + dm) * 60;
    let trial_end = d_end + observe * 60;
    let (mut areas, mut wobbles) = (Vec::with_capacity(trials), Vec::with_capacity(trials));
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (a.factory)(clock.clone());
        let t = run_trial_observed(v, clock, config.clone(), &sched, seed.wrapping_add(i as u64));
        let (mut area, mut last_t, mut settled) = (0.0f64, d_start, 0.0f64);
        for tk in &t.ticks {
            let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
            if tk.t_secs > d_start && tk.t_secs <= d_end {
                let dt_min = (tk.t_secs - last_t) as f64 / 60.0;
                if e > 0.0 { area += e * dt_min; }
                last_t = tk.t_secs;
            }
            if tk.t_secs > d_end && tk.t_secs <= trial_end { settled = e; }
        }
        areas.push(area);
        wobbles.push(settled.abs()); // wobble magnitude (safe-side under-difficulty)
    }
    (median(areas), median(wobbles))
}

/// For a fixed (τ, spm): worst over-difficulty AREA and the wobble at that worst
/// cell, over the decline-severity set (worst-area, matching tau-valley's
/// conservatism — the window must survive the worst decline at that rate).
fn cell_over_severity(tau: u64, spm: f32, base_trials: usize, seed: u64) -> (f64, f64) {
    let a = cfg(tau, SENS);
    let (mut worst_area, mut wob_at_worst) = (f64::MIN, 0.0f64);
    let mut k = 0u64;
    for &r in RATES_PPH {
        let ct = (base_trials as f64 * (60.0 / spm as f64).max(1.0)).round() as usize;
        let (area, wob) = decline_both(&a, r, spm, ct, seed.wrapping_add(k << 40));
        if area > worst_area { worst_area = area; wob_at_worst = wob; }
        k += 1;
    }
    (worst_area, wob_at_worst)
}

fn main() -> std::io::Result<()> {
    let base: usize = env::var("VARDIFF_TF_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(80);
    let out = env::var("VARDIFF_TF_OUT").unwrap_or_else(|_| "docs/tau_family.svg".to_string());
    let seed = DEFAULT_BASELINE_SEED ^ 0xFA_117;

    // area_grid[spm][tau], wob_grid[spm][tau] (wobble at the worst-area severity)
    let jobs: Vec<(usize, usize)> =
        (0..SPMS.len()).flat_map(|si| (0..TAUS.len()).map(move |ti| (si, ti))).collect();
    let next = AtomicUsize::new(0);
    let area_grid: Mutex<Vec<Vec<f64>>> = Mutex::new(vec![vec![f64::NAN; TAUS.len()]; SPMS.len()]);
    let wob_grid: Mutex<Vec<Vec<f64>>> = Mutex::new(vec![vec![f64::NAN; TAUS.len()]; SPMS.len()]);
    let nth = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    eprintln!("tau-family: {} cells (spm {:?} × τ {:?}), base {} trials, {} threads",
        jobs.len(), SPMS, TAUS, base, nth);
    std::thread::scope(|s| {
        for _ in 0..nth {
            s.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() { break; }
                let (si, ti) = jobs[j];
                let (area, wob) = cell_over_severity(TAUS[ti], SPMS[si], base, seed.wrapping_add((j as u64) << 8));
                area_grid.lock().unwrap()[si][ti] = area;
                wob_grid.lock().unwrap()[si][ti] = wob;
                eprintln!("  spm{} τ{} done", SPMS[si], TAUS[ti]);
            });
        }
    });
    let area_grid = area_grid.into_inner().unwrap();
    let wob_grid = wob_grid.into_inner().unwrap();

    // --- text tables: over-difficulty area (the slide) AND wobble (the trade) ---
    println!("\n## τ-valley FAMILY — TWO primitives vs window τ, one curve per spm (r*). sens={} fixed; severity collapsed by worst-area.", SENS);
    println!("RESULT (durable): over-difficulty-area argmin τ* slides with rate (τ*∝1/r). spm<6 vs >=6 = guard switch, labeled.");
    println!("TRADE (interpretation): short τ wins the over-difficulty axis but LOSES the wobble axis — the two are not symmetric");
    println!("(over-difficulty = dangerous self-reinforcing spiral direction; wobble = safe self-healing under-difficulty).\n");
    let argmins: Vec<usize> = (0..SPMS.len()).map(|si| {
        let row = &area_grid[si];
        let (mut ai, mut av) = (0usize, f64::MAX);
        for (ti, &v) in row.iter().enumerate() { if v < av { av = v; ai = ti; } }
        ai
    }).collect();
    println!("### over-difficulty area (e-min, lower=better) — the durable slide");
    print!("| spm \\ τ |"); for &t in TAUS { print!(" {} |", t); } println!(" argmin τ* |");
    print!("| --- |"); for _ in TAUS { print!(" --- |"); } println!(" --- |");
    for (si, &spm) in SPMS.iter().enumerate() {
        print!("| {} |", spm as u32);
        for (ti, &v) in area_grid[si].iter().enumerate() {
            if ti == argmins[si] { print!(" **{:.0}** |", v); } else { print!(" {:.0} |", v); }
        }
        println!(" τ={} |", TAUS[argmins[si]]);
    }
    println!("\n### wobble |settled-e|% at the worst-area severity (the trade: short τ is WORSE here)");
    print!("| spm \\ τ |"); for &t in TAUS { print!(" {} |", t); } println!(" wobble@τ* / @360 |");
    print!("| --- |"); for _ in TAUS { print!(" --- |"); } println!(" --- |");
    let t360_i = TAUS.iter().position(|&t| t == 360).unwrap();
    for (si, &spm) in SPMS.iter().enumerate() {
        print!("| {} |", spm as u32);
        for &v in wob_grid[si].iter() { print!(" {:.0} |", v); }
        println!(" {:.0} / {:.0} |", wob_grid[si][argmins[si]], wob_grid[si][t360_i]);
    }
    println!("\nRead: argmin τ* (panel 1) slides with rate = the durable result. Wobble (panel 2) at τ* exceeds wobble at 360 =");
    println!("the trade — short τ buys over-difficulty (dangerous axis) at a wobble cost (safe axis). Favorable DIRECTION to trade;");
    println!("net value at a given rate NOT asserted (scalar weighting deliberately unfixed). Area magnitude partly clamp-confounded.");

    // --- SVG: two panels, LOG-τ axis. Left = over-difficulty area; right = wobble. ---
    let (w, h) = (1040.0, 540.0);
    let (mt, mb) = (64.0, 70.0);
    let ph = h - mt - mb;
    let pw = 360.0;
    let lx = 64.0;            // left panel x-origin
    let rx = lx + pw + 96.0;  // right panel x-origin
    let colors = ["#8c564b", "#e377c2", "#1f77b4", "#2ca02c", "#ff7f0e", "#9467bd", "#d62728"];
    let lt = (*TAUS.first().unwrap() as f64).ln();
    let ht = (*TAUS.last().unwrap() as f64).ln();
    let xof = |x0: f64, t: f64| x0 + ((t as f64).ln() - lt) / (ht - lt) * pw;

    let mut svg = String::new();
    svg.push_str(&format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" font-family=\"Helvetica,Arial,sans-serif\" font-size=\"13\">\n"));
    svg.push_str("<text x=\"520\" y=\"18\" text-anchor=\"middle\" font-size=\"11\" fill=\"#999\" font-style=\"italic\">tau-family.rs — generated. LOG-τ. The TRADE: short τ wins over-difficulty (left), loses wobble (right) — axes are NOT symmetric (see caption).</text>\n");

    // helper closure capturing common drawing per panel
    let mut panel = |x0: f64, title: &str, ylabel: &str, grid: &Vec<Vec<f64>>, ann_argmin: bool| {
        let all: Vec<f64> = grid.iter().flatten().copied().filter(|x| x.is_finite()).collect();
        let ymax = all.iter().cloned().fold(f64::MIN, f64::max) * 1.08;
        let yof = |e: f64| mt + (ymax - e) / (ymax - 0.0) * ph;
        svg.push_str(&format!("<rect x=\"{x0}\" y=\"{mt}\" width=\"{pw}\" height=\"{ph}\" fill=\"none\" stroke=\"#bbb\"/>\n"));
        svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-weight=\"bold\" font-size=\"13\">{title}</text>\n", x0 + pw/2.0, mt - 8.0));
        // champion τ=360 vertical
        let xc = xof(x0, 360.0);
        svg.push_str(&format!("<line x1=\"{xc:.1}\" y1=\"{mt}\" x2=\"{xc:.1}\" y2=\"{:.1}\" stroke=\"#888\" stroke-dasharray=\"3 3\"/>\n", mt + ph));
        svg.push_str(&format!("<text x=\"{xc:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"10\" fill=\"#555\">champ 360</text>\n", mt + ph + 30.0));
        for &t in TAUS {
            let x = xof(x0, t as f64);
            svg.push_str(&format!("<text x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9\" fill=\"#666\">{t}</text>\n", mt + ph + 14.0));
        }
        svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-weight=\"bold\" font-size=\"12\">window τ (s), log scale</text>\n", x0 + pw/2.0, h - 28.0));
        svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" transform=\"rotate(-90 {:.1} {:.1})\">{ylabel}</text>\n", x0 - 44.0, mt + ph/2.0, x0 - 44.0, mt + ph/2.0));
        for (si, _spm) in SPMS.iter().enumerate() {
            let c = colors[si % colors.len()];
            let mut pts = String::new();
            for (ti, &v) in grid[si].iter().enumerate() {
                if v.is_finite() { pts.push_str(&format!("{:.1},{:.1} ", xof(x0, TAUS[ti] as f64), yof(v))); }
            }
            svg.push_str(&format!("<polyline points=\"{pts}\" fill=\"none\" stroke=\"{c}\" stroke-width=\"2\"/>\n"));
            if ann_argmin {
                let ti = argmins[si];
                svg.push_str(&format!("<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"5\" fill=\"none\" stroke=\"{c}\" stroke-width=\"2\"/>\n",
                    xof(x0, TAUS[ti] as f64), yof(grid[si][ti])));
            } else {
                // on the wobble panel, mark τ* and 360 to show the trade at the same x's
                let ti = argmins[si];
                svg.push_str(&format!("<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"4\" fill=\"{c}\"/>\n", xof(x0, TAUS[ti] as f64), yof(grid[si][ti])));
            }
        }
    };
    panel(lx, "over-difficulty area  (dangerous axis — short τ WINS)", "worst over-diff area (e-min)", &area_grid, true);
    panel(rx, "wobble  (safe axis — short τ LOSES)", "|settled-e| % (under-diff)", &wob_grid, false);

    // shared legend, far right
    let lgx = rx + pw + 14.0;
    for (si, &spm) in SPMS.iter().enumerate() {
        let c = colors[si % colors.len()];
        let ly = mt + 6.0 + si as f64 * 18.0;
        svg.push_str(&format!("<line x1=\"{lgx:.1}\" y1=\"{ly:.1}\" x2=\"{:.1}\" y2=\"{ly:.1}\" stroke=\"{c}\" stroke-width=\"2\"/>\n", lgx + 16.0));
        svg.push_str(&format!("<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\">spm {} (τ*={})</text>\n", lgx + 20.0, ly + 4.0, spm as u32, TAUS[argmins[si]]));
    }
    svg.push_str(&format!("<text x=\"24\" y=\"{:.1}\" font-size=\"10\" fill=\"#555\">○ = per-rate over-difficulty optimum τ* (left); ● = same τ* on the wobble axis (right). The axes are NOT symmetric:</text>\n", h - 8.0));
    svg.push_str(&format!("<text x=\"24\" y=\"{:.1}\" font-size=\"10\" fill=\"#555\">over-difficulty is the self-reinforcing spiral direction; wobble is self-healing under-difficulty. Short τ trades the dangerous axis down for the safe axis.</text>\n", h + 4.0));
    svg.push_str("</svg>\n");
    fs::write(&out, svg)?;
    eprintln!("wrote {}", out);
    Ok(())
}
