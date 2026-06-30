//! WORST-CELL FAITHFULNESS — is the +220% spm2/320%/90% corner REAL evidence or
//! numerically degenerate? The mirror of the tick-floor check: not "window too
//! short" but "stream too sparse." A "+220% peak → +2% settle = recovered" label is
//! the strongest anti-spiral evidence in the arc IF the escape is driven by real
//! share events and real fires; it is an ARTIFACT if the stream is so collapsed
//! (e^(−2.2)≈0.11 ⇒ ~0.2 sh/min at spm2) that the recovery is coarse-grained
//! arithmetic over a handful of share events, below the sim's faithful resolution.
//! Same "recovered" label, opposite meaning — this distinguishes them.
//!
//! Traces the worst cell tick-by-tick: per tick n_shares (real arrivals), e, fire,
//! belief. Counts share EVENTS and FIRES between the +220% peak and the +2% settle.
//! Many shares + fires tracking them ⇒ FAITHFUL (strongest evidence). A handful of
//! shares + e swinging +220→+2 across them ⇒ DEGENERATE (exclude the corner; close
//! on the resolvable envelope).
//!
//! Usage: cargo run --release --bin worst-cell-faithful

use std::sync::Arc;
use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Phase, Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

fn champion() -> AlgorithmSpec {
    AlgorithmSpec::new(format!("champion"), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(360),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(1.5, 0.05, 8.0, 0.06, 0.6), 6,
            ),
            AcceleratingPartialRetarget::new(0.2, 0.6, 0.05), 1.0, clock,
        )))
    })
}

fn main() {
    let spm = 2.0f32;
    let rate_pph = 320.0f32; // steepest
    let drop = 0.9f32;       // deepest — the worst cell
    let seed = DEFAULT_BASELINE_SEED ^ 0x5B1_2A1u64; // same family as the sweep

    let mature = 60u64;
    let rate = rate_pph / 100.0 / 60.0;
    let mut phases = vec![Phase::Hold { secs: mature * 60, h: TRUE_HASHRATE }];
    let mut dm = 0u64;
    for m in 0..600u64 {
        let frac = (rate * (m as f32 + 1.0)).min(drop);
        phases.push(Phase::Hold { secs: 60, h: TRUE_HASHRATE * (1.0 - frac) });
        dm = m + 1;
        if frac >= drop { break; }
    }
    let floor_h = TRUE_HASHRATE * (1.0 - (rate * dm as f32).min(drop));
    phases.push(Phase::Hold { secs: 300 * 60, h: floor_h });
    let scen = Scenario::Custom { name: "decline".into(), phases, initial_estimate: None };
    let (proto, sched) = scen.build(spm);
    let config = TrialConfig { tick_interval_secs: 60, ..proto };
    let clock = Arc::new(MockClock::new(0));
    let v = (a_factory())(clock.clone());
    let d_end = (mature + dm) * 60;
    let trial_end = d_end + 300 * 60;
    let t = run_trial_observed(v, clock, config, &sched, seed);

    println!("# WORST-CELL FAITHFULNESS — spm2 / 320%/hr / 90%-drop. Decline ends t={}min.\n", d_end/60);
    println!("Expected collapsed rate at +220%: spm·e^(−2.2) = 2·0.111 = {:.2} sh/min (~1 share / {:.0} min).", 2.0*(-2.2f64).exp(), 1.0/(2.0*(-2.2f64).exp()));
    println!("FAITHFUL iff the escape has MANY share events and fires TRACK them; DEGENERATE iff a handful of shares swing e +220→+2.\n");

    // find the peak-e tick and the recovery (first tick e<=5 after peak), count shares + fires between.
    let mut peak_e = f64::MIN; let mut peak_t = 0u64;
    let mut rows: Vec<(u64, f64, u32, bool, f64)> = Vec::new();
    for tk in &t.ticks {
        let h_true = sched.at(tk.t_secs.saturating_sub(30)) as f64;
        let e = (tk.current_hashrate_before as f64 / h_true).ln() * 100.0;
        if tk.t_secs > mature*60 && tk.t_secs <= trial_end {
            rows.push((tk.t_secs/60, e, tk.n_shares, tk.fired, tk.current_hashrate_before as f64 / TRUE_HASHRATE as f64));
            if e > peak_e { peak_e = e; peak_t = tk.t_secs; }
        }
    }
    // recovery tick: first e<=5 after peak
    let rec_t = rows.iter().find(|(tmin, e, ..)| *tmin * 60 > peak_t && *e <= 5.0).map(|(tmin,..)| *tmin*60);

    // counts between peak and recovery (the escape proper)
    let (mut sh_escape, mut fires_escape, mut ticks_escape) = (0u32, 0u32, 0u32);
    let (mut sh_total, mut fires_total) = (0u32, 0u32);
    for (tmin, _e, nsh, fired, _b) in &rows {
        sh_total += nsh; if *fired { fires_total += 1; }
        let ts = tmin*60;
        if ts >= peak_t && rec_t.map_or(true, |r| ts <= r) {
            sh_escape += nsh; if *fired { fires_escape += 1; } ticks_escape += 1;
        }
    }

    println!("PEAK e = {:+.0}% at t={}min.  RECOVERY (e≤5%) at t={}.",
        peak_e, peak_t/60, rec_t.map(|r| format!("{}min", r/60)).unwrap_or_else(|| "never".into()));
    println!("\nBETWEEN PEAK AND RECOVERY (the escape proper): {} ticks, {} SHARE EVENTS, {} FIRES.", ticks_escape, sh_escape, fires_escape);
    println!("WHOLE post-mature run: {} share events, {} fires.\n", sh_total, fires_total);

    // print the escape region tick-by-tick (peak−5 .. recovery+5)
    println!("| t(min) | e% | n_shares | FIRED | belief (norm) |");
    println!("| --- | --- | --- | --- | --- |");
    let lo = peak_t.saturating_sub(5*60);
    let hi = rec_t.unwrap_or(trial_end) + 10*60;
    for (tmin, e, nsh, fired, b) in &rows {
        let ts = tmin*60;
        if ts >= lo && ts <= hi {
            println!("| {} | {:+.0} | {} | {} | {:.2} |", tmin, e, nsh, if *fired {"**YES**"} else {"no"}, b);
        }
    }

    println!("\nVERDICT on faithfulness:");
    println!("  FAITHFUL  ⇔ escape has many share events (dozens) AND fires track them ⇒ +220→+2 is REAL anti-spiral, strongest evidence.");
    println!("  DEGENERATE ⇔ escape has a handful of share events (e swings across ~3-5 shares) ⇒ corner is below sim resolution, EXCLUDE it;");
    println!("              close on the resolvable envelope (≤80%/hr,≤70% drop) which is clean regardless.");
    let _ = (sh_escape, fires_escape); // read from the printed counts
}

// factory helper (champion built once)
fn a_factory() -> impl Fn(Arc<MockClock>) -> VardiffBox {
    move |clock| (champion().factory)(clock)
}
