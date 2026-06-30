//! Is the champion's stable-load fire rate genuine FALSE ALARMS or residual
//! settling? The per-config ARL0 gate result rests on "stable fire = false
//! alarm". A false alarm fires when the estimate is ALREADY at truth and
//! Poisson noise alone trips the boundary (e≈0 at fire). A settling/real fire
//! fires when the estimate is still off and correctly closing the gap
//! (|e|>0 at fire). On truly flat H with a matured counter, we measure the
//! error e AT each fire, well past cold-start warmup.
//!
//! If fires cluster at e≈0 → genuine false alarms, ARL0 is honest, gate holds.
//! If fires cluster at |e|>0 → settling miscounted as false alarms, the true
//! false-alarm rate is LOWER, the floor LONGER, and the champion's ratio
//! WORSE (admission at 4spm even thinner).

use std::sync::Arc;
use channels_sv2::vardiff::composed::{
    AcceleratingPartialRetarget, AdaptiveSignPersist, Composed, EwmaEstimator,
    SignPersistenceCusumBoundary,
};
use channels_sv2::vardiff::MockClock;
use vardiff_sim::baseline::{Scenario, DEFAULT_BASELINE_SEED, TRUE_HASHRATE};
use vardiff_sim::grid::{AlgorithmSpec, VardiffBox};
use vardiff_sim::trial::{run_trial_observed, TrialConfig};

#[derive(Clone, Copy)]
struct Cfg { tau: u64, sens: f64, tighten: f64, eta_max: f32 }
fn spec(c: Cfg) -> AlgorithmSpec {
    AlgorithmSpec::new(format!("Ewma{}/s{}", c.tau, c.sens), move |clock| {
        VardiffBox(Box::new(Composed::new(
            EwmaEstimator::new(c.tau),
            AdaptiveSignPersist::sign_persist(
                SignPersistenceCusumBoundary::new(c.sens, 0.05, c.tighten, 0.06, 0.6), 6),
            AcceleratingPartialRetarget::new(0.2, c.eta_max, 0.05), 1.0, clock,
        )))
    })
}

fn main() {
    let trials = 3000usize;
    let dur = 6 * 60 * 60u64;
    let warmup = 60 * 60u64; // 1h: well past any cold-start settling
    let configs = [
        ("champion(s0.3)", Cfg{tau:150,sens:0.3,tighten:6.0,eta_max:0.8}),
        ("corner(s2,t720)", Cfg{tau:720,sens:2.0,tighten:8.0,eta_max:0.6}),
    ];
    for spm in [4u32, 6] {
        let (cfg, sched) = Scenario::Custom {
            name: "flat6h".into(),
            phases: vec![vardiff_sim::baseline::Phase::Hold { secs: dur, h: TRUE_HASHRATE }],
            initial_estimate: None, // starts AT truth — no cold-start gap at all
        }.build(spm as f32);
        let config = TrialConfig { tick_interval_secs: 60, ..cfg };
        println!("\n## spm={spm}: error |e| AT each post-warmup stable fire (truth-aligned start, flat H)");
        println!("| config | fires/trial | mean |e| at fire | p50 |e| | p90 |e| | frac fires |e|<2% |");
        println!("| --- | --- | --- | --- | --- | --- |");
        for (name, c) in &configs {
            let a = spec(*c);
            let mut es: Vec<f64> = Vec::new();
            let mut nfire = 0u64;
            for i in 0..trials {
                let clock = Arc::new(MockClock::new(0));
                let v = (a.factory)(clock.clone());
                let t = run_trial_observed(v, clock, config.clone(), &sched, DEFAULT_BASELINE_SEED.wrapping_add(i as u64));
                for tk in &t.ticks {
                    if tk.fired && tk.t_secs > warmup {
                        // e at the moment of firing (before the retarget takes effect)
                        let e = (tk.current_hashrate_before as f64 / TRUE_HASHRATE as f64).ln().abs();
                        es.push(e * 100.0);
                        nfire += 1;
                    }
                }
            }
            es.sort_by(|a,b| a.partial_cmp(b).unwrap());
            let mean = if es.is_empty() {0.0} else { es.iter().sum::<f64>()/es.len() as f64 };
            let p = |q: f64| if es.is_empty() {f64::NAN} else { es[((es.len()as f64*q) as usize).min(es.len()-1)] };
            let frac_small = if es.is_empty() {0.0} else { es.iter().filter(|&&x| x<2.0).count() as f64/es.len() as f64 };
            println!("| {} | {:.2} | {:.2}% | {:.2}% | {:.2}% | {:.0}% |",
                name, nfire as f64/trials as f64, mean, p(0.5), p(0.9), frac_small*100.0);
        }
        println!("  |e|≈0 at fire ⇒ genuine false alarm (estimate already at truth, noise tripped it).");
        println!("  |e|≫0 at fire ⇒ settling/correcting (miscounted as false alarm → true ARL0 longer).");
    }
}
