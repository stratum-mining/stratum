//! GRADUAL-DECLINE RESPONSIVENESS — SRI is decline-SAFE against an abrupt cut
//! (it eases to meet the miner), but is it SLUGGISH-to-stuck in a SLOW decline?
//!
//! ## The question, precisely
//!
//! Abrupt-cut safety (validated elsewhere) is about the END STATE: does the
//! controller eventually follow the miner down? Gradual-decline responsiveness is
//! about the TRANSIENT: while the miner is steadily declining, how far BEHIND does
//! the controller's difficulty lag? A controller can be abrupt-safe (reaches the
//! floor) yet chronically lag a gradual decline — sitting at sustained
//! over-difficulty the whole way down, taxing the miner exactly as §4 describes,
//! just without ever fully freezing.
//!
//! The measure is TIME-AVERAGED over-difficulty DURING the decline phase:
//!   e = ln(belief / true_hashrate),  averaged over the decline window, e>0 = over.
//! (NOT the settled endpoint — that's the abrupt-safety quantity. Mean-e over the
//! ramp is the lag: 0 = tracks perfectly, large = chronically behind.)
//!
//! ## The non-obvious part (why the answer isn't just "slower = worse")
//!
//! A SLOWER decline means the miner dwells LONGER at each intermediate hashrate,
//! so the controller gets MORE shares to measure at each level — per-step, slower
//! is EASIER to track. But the controller's window (champion EWMA τ=360s) has a
//! fixed memory, so against a steady ramp it chases a moving target and settles to
//! a roughly CONSTANT lag set by (ramp speed × window). The empirical question is
//! which effect dominates across realistic curtailment speeds, and whether either
//! shipped SRI controller's lag gets large enough to matter.
//!
//! ## Arms (the SHIPPED SRI controllers — this is a claim about deployed code)
//!   - `champion` = `champion_composed` (Ewma360/s1.5) — SRI on Eric's branch.
//!     NOTE: the SHIPPED champion, NOT `AlgorithmSpec::champion()` (that's the old
//!     s0.3 variant).
//!   - `classic`  = `classic_composed` — SRI on main (cumulative counter).
//!
//! ## Scenario
//! Decline rate is swept (the independent variable). Each: mature 60min on-target,
//! decline at `rate` %/hr to 50% loss, then hold. Mean-e is read over the decline
//! phase only. Production share rate (spm 6).
//!
//! Usage: cargo run --release --bin gradual-responsiveness
//! Env: VARDIFF_GR_TRIALS (default 300), VARDIFF_GR_SPM (6), VARDIFF_GR_SEED.

use std::env;
use std::sync::Arc;

use channels_sv2::vardiff::composed::{champion_composed, classic_composed};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::DEFAULT_BASELINE_SEED;
use vardiff_sim::decline_safety::{decline_scenario, DECLINE_TICK_SECS as TICK};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// The SHIPPED champion (Ewma360/s1.5) — champion_composed, NOT the old s0.3
/// `AlgorithmSpec::champion()`. Built directly so this tracks production.
fn shipped_champion() -> AlgorithmSpec {
    AlgorithmSpec::new("champion", |clock| VardiffBox(Box::new(champion_composed(1.0, clock))))
}
/// The classic controller (SRI main) — cumulative counter.
fn shipped_classic() -> AlgorithmSpec {
    AlgorithmSpec::new("classic", |clock| VardiffBox(Box::new(classic_composed(1.0, clock))))
}

/// For one (arm, decline-rate): median over trials of mean over-difficulty `e`
/// during the decline phase, plus max-e and the settled endpoint for context.
/// Returns (mean_e_pct, max_e_pct, settled_e_pct).
fn measure(
    make: &dyn Fn() -> AlgorithmSpec,
    rate: f32,
    spm: f32,
    trials: usize,
    base_seed: u64,
) -> (f64, f64, f64) {
    let (scen, d_start, d_end, trial_end) = decline_scenario(rate);
    let (cfg_proto, schedule) = scen.build(spm);
    let cfg = TrialConfig { tick_interval_secs: TICK, ..cfg_proto };

    let (mut means, mut maxes, mut settleds) = (vec![], vec![], vec![]);
    for i in 0..trials {
        let clock = Arc::new(MockClock::new(0));
        let v = (make().factory)(clock.clone());
        let t = run_trial_observed(v, clock, cfg.clone(), &schedule, base_seed.wrapping_add(i as u64));
        let (mut sum, mut n, mut mx, mut settled) = (0.0f64, 0u32, f64::MIN, 0.0f64);
        for tk in &t.ticks {
            let h_true = schedule.at(tk.t_secs.saturating_sub(TICK / 2)) as f64;
            let e = (tk.current_hashrate_before as f64 / h_true).ln();
            // decline phase: the lag measure.
            if tk.t_secs > d_start && tk.t_secs <= d_end {
                sum += e; // signed: chronic over-difficulty is the concern (e>0)
                n += 1;
                mx = mx.max(e);
            }
            // settled endpoint (abrupt-safety quantity), for context.
            if tk.t_secs > d_end && tk.t_secs <= trial_end {
                settled = e;
            }
        }
        if n > 0 {
            means.push(sum / n as f64 * 100.0);
            maxes.push(mx * 100.0);
            settleds.push(settled * 100.0);
        }
    }
    (median(means), median(maxes), median(settleds))
}

fn main() {
    let trials: usize = env::var("VARDIFF_GR_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let spm: f32 = env::var("VARDIFF_GR_SPM").ok().and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let seed: u64 = env::var("VARDIFF_GR_SEED")
        .ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(DEFAULT_BASELINE_SEED);

    // Decline-rate sweep (%/hr). Spans very slow (grid demand-response ramp over
    // hours) to fast (near-abrupt). 50%-loss scenario, so e.g. 3%/hr ≈ capped at
    // the 240min ceiling (slowest resolvable), 100%/hr = 30min, 600%/hr ≈ 5min.
    let rates: [f32; 8] = [3.0, 6.0, 12.5, 25.0, 50.0, 100.0, 300.0, 600.0];

    let arms: [(&str, fn() -> AlgorithmSpec); 2] =
        [("champion", shipped_champion), ("classic", shipped_classic)];

    eprintln!(
        "gradual-responsiveness: {} arms × {} rates × {} trials, spm {} (SHIPPED champion_composed + classic_composed)",
        arms.len(), rates.len(), trials, spm,
    );

    // CSV: rate_pph, decline_min, then per-arm mean_e / max_e / settled_e.
    print!("rate_pph,decline_min");
    for (name, _) in &arms {
        print!(",{name}_mean_e,{name}_max_e,{name}_settled_e");
    }
    println!();

    // Also collect for the trend read.
    let mut champ_means = Vec::new();
    let mut classic_means = Vec::new();
    for &rate in &rates {
        // decline_scenario caps decline at 240min; recompute the actual span.
        let decline_min = ((0.50 / (rate / 100.0)) * 60.0).min(240.0) as u64;
        print!("{rate},{decline_min}");
        for (name, make) in &arms {
            let (mean_e, max_e, settled_e) = measure(make, rate, spm, trials, seed);
            print!(",{mean_e:.2},{max_e:.2},{settled_e:.2}");
            if *name == "champion" { champ_means.push((rate, mean_e)); }
            if *name == "classic" { classic_means.push((rate, mean_e)); }
        }
        println!();
    }

    // ---- Trend read: is mean-e (lag) worse at SLOW decline (sluggish) or FAST? -
    eprintln!("\n## Lag trend — mean over-difficulty during the decline (e%>0 = chronically behind)");
    eprintln!("Read: if mean-e RISES as rate FALLS (moving left), gradual decline is the sluggish regime.");
    eprintln!("| rate %/hr | decline min | champion mean-e% | classic mean-e% |");
    eprintln!("| --- | --- | --- | --- |");
    for (i, &rate) in rates.iter().enumerate() {
        let decline_min = ((0.50 / (rate / 100.0)) * 60.0).min(240.0) as u64;
        eprintln!("| {} | {} | {:+.2} | {:+.2} |", rate, decline_min, champ_means[i].1, classic_means[i].1);
    }
    // Slowest-vs-fastest deltas make the direction explicit.
    let champ_slow = champ_means.first().map(|x| x.1).unwrap_or(f64::NAN);
    let champ_fast = champ_means.last().map(|x| x.1).unwrap_or(f64::NAN);
    let classic_slow = classic_means.first().map(|x| x.1).unwrap_or(f64::NAN);
    let classic_fast = classic_means.last().map(|x| x.1).unwrap_or(f64::NAN);
    eprintln!(
        "\nchampion: slowest({}%/hr) mean-e {:+.2}% vs fastest({}%/hr) {:+.2}%  → {}",
        rates[0], champ_slow, rates[rates.len()-1], champ_fast,
        if champ_slow > champ_fast + 1.0 { "SLUGGISH in gradual (lag rises as decline slows)" }
        else if champ_fast > champ_slow + 1.0 { "worse when FAST (gradual is the easy regime)" }
        else { "flat (lag ~rate-independent)" }
    );
    eprintln!(
        "classic:  slowest {:+.2}% vs fastest {:+.2}%  → {}",
        classic_slow, classic_fast,
        if classic_slow > classic_fast + 1.0 { "SLUGGISH in gradual" }
        else if classic_fast > classic_slow + 1.0 { "worse when FAST" }
        else { "flat" }
    );
    eprintln!("\nContext: mean-e is the DECLINE-PHASE lag, not the settled endpoint (abrupt-safety).");
    eprintln!("A large positive mean-e at slow rates = decline-safe-but-sluggish: the headline nuance.");
}
