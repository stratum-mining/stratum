//! Downward-hint fusion measurement (Path B, the only surviving payoff).
//!
//! THE HUNCH (pre-registered, BEFORE reading any numbers): on a mid-run
//! hashrate DROP, an `UpdateChannel` carrying the lower nominal arrives ~at the
//! drop, long before the slowing share stream reveals it (~τ + boundary lag).
//! If the pool eases the OPERATING POINT (Ĥ, = the channel's nominal_hashrate
//! register, the `hashrate` arg to try_vardiff) toward the declared value at the
//! drop, the over-difficulty excursion e=ln(Ĥ/H)>0 should collapse in time
//! WITHOUT retuning the share-driven controller. This is a pool-loop write to
//! the SAME register the fire-path already writes (update_channel) — NOT a
//! belief injection (that was the retracted seed) and NOT a trait change.
//!
//! THE SUBTLETY THIS BINARY EXISTS TO SETTLE: a normal controller fire calls
//! `estimator.on_fire(new,old)`, which RESCALES the EWMA rate by the target
//! ratio so h_estimate stays consistent across the difficulty change
//! (estimator.rs:360). A pool-external operating-point write does NOT call
//! on_fire. So after a pool ease the estimator's smoothed `rate` is left
//! un-rescaled and inconsistent with the new easier target for ~τ. Predicted
//! consequence: h_estimate transiently UNDER-reads → controller thinks it's
//! still over-difficulty → OVER-eases slightly → a small UNDER-difficulty
//! (e<0, SAFE-side) wobble that self-corrects. The question is whether that
//! wobble is benign (→ downward-only truly needs zero trait change) or whether
//! the ease must also resync the estimator (→ a trait touch).
//!
//! THREE ARMS (champion Ewma360/s1.5 throughout; identical seeds per trial):
//!   (a) shares-only     — baseline. No hint. The status quo decline reaction.
//!   (b) ease-no-rescale — pool writes Ĥ←declared at the drop; estimator NOT
//!                         resynced. The zero-trait-change candidate.
//!   (c) ease-rescale    — pool writes Ĥ←declared AND calls estimator.on_fire
//!                         to rescale (the "do it properly" variant; would need
//!                         a trait hook in production).
//!
//! PRE-REGISTERED PREDICTIONS (so the result can't be retrofitted):
//!   - max over-difficulty (peak e>0) and over-difficulty area: a ≫ b ≈ c.
//!     The hint kills the over-difficulty excursion in both b and c.
//!   - under-difficulty wobble (min e<0 after the ease): b shows a SAFE-side
//!     dip that c does not (or c's is smaller). If b's dip is small (say |e|<5%)
//!     and self-corrects within ~τ, downward-only is clean → zero trait change.
//!     If b's dip is large or persistent, the rescale (c, trait touch) earns its
//!     keep.
//!   - settled e (end of window): all three converge to the same steady state
//!     (the hint is transient-only; steady state is share-driven, unchanged).
//!     If they DON'T converge, the hint is contaminating steady state — a bug in
//!     the mechanism, not a feature.
//!
//! Metric: e = ln(Ĥ / H_true), Ĥ = operating point before the tick's decision,
//! H_true = scheduled hashrate at the interval midpoint. Reported per (rate,
//! spm): peak over-diff e%, over-diff area (e-min, e>0 only), worst under-diff
//! e% after the ease, ticks-to-recover (e back within ±2%), settled e%.
//!
//! ===========================================================================
//! PRE-REGISTRATION — the ship-number sweep (damped blend × noise × lag × gate)
//! ===========================================================================
//! Written BEFORE running the sweep. The arms above are the perfect-telemetry
//! CEILING (hard-set, exact hint). Production telemetry is noisy, stale, and the
//! ease-magnitude gate is a free parameter. This sweep finds the ship number and
//! — more importantly — commits its interpretation in advance. Each prediction
//! below is shaped so it CAN fail; a prediction that can only "confirm" is worth
//! nothing.
//!
//! SCORING IS UNCHANGED. We score the SAME two primitives used throughout the
//! whole arc: (1) over-difficulty area saved (e-min, the costly leg) and
//! (2) under-difficulty wobble (worst e<0 after the ease, the safe-side cost).
//! NO new metric is introduced. Any single-number "α* winner" is reported ONLY
//! as an explicitly-labeled aggregate over those two primitives, never as a new
//! cost axis — so champion selection and everything upstream stays on the same
//! footing.
//!
//! THE DAMPED BLEND. Instead of hard-set (Ĥ ← declared), ease PARTWAY:
//!     Ĥ ← (1−α)·Ĥ + α·declared,   α ∈ {0.25, 0.5, 0.75, 1.0}.
//! α=1 reproduces the hard-set arm above. No estimator on_fire (Q1 closed:
//! retaining the EWMA's banked smoothing is correct; the rescale was a strict
//! precision give-back).
//!
//! NOISE MODEL — MULTIPLICATIVE IN H, stated explicitly because it determines
//! the whole result. The declared value the pool acts on is
//!     declared = H_true_post · (1 + ε),   ε ~ Normal(0, σ),   σ ∈ {0, 0.05, 0.15, 0.30}.
//! Multiplicative (a fraction of reading) is the realistic firmware-telemetry
//! error and is ~scale-invariant in TARGET space. NOT additive — additive noise
//! would manufacture a low-spm artifact that is about the noise model, not the
//! controller.
//!
//! TWO DECLINE SHAPES, each scoped to the predictions only IT can falsify (NOT
//! redundancy — a single shape cannot carry the whole sweep):
//!   - STEP (abrupt 50% loss, clean post-drop plateau): the death-spiral-entry
//!     shape the hint exists for, and the right shape for P1/P3/P4 — those are
//!     about α and the gate under measurement NOISE on a SETTLING target, and
//!     the flat post-drop plateau is what makes α*'s cost legible. Used for the
//!     σ (noise) axis and the gate sweep.
//!   - RAMP (sustained linear decline): the ONLY shape that can falsify P2's lag
//!     axis. Under a step, lag is brief-then-PERFECT, so "damping barely helps
//!     lag" would confirm because the bias is TRANSIENT — P2 confirmed for the
//!     wrong reason. P2's real content is "α down-weights but cannot CORRECT a
//!     biased signal," and only a sustained descent sustains the bias. Used for
//!     the lag axis.
//!
//! LAG MODEL — SEPARATE AXIS from noise, on the RAMP. Lag = the hint reports a
//! STALE H (where the miner was `lag_ticks` ago). On a ramp this stays BIASED
//! HIGH by ≈ decline_rate · lag for the whole descent. Axis: lag_ticks ∈ {0,1,3}
//! at σ=0. (σ swept at lag=0 on the step.) Separate axes, not one blurred dial.
//!
//! SINGLE FIXED RAMP SLOPE on the lag axis = 20%/hr (RAMP_RATE_PCT_HR). decline_
//! rate is held CONSTANT across the lag axis so the ONLY varying thing is lag.
//! This is required by the per-minute metric (below): a steeper ramp makes more
//! bias/min, so a per-minute residual would scale with slope and could confirm
//! P2's (decline_rate·lag) proportionality BY CONSTRUCTION if slope varied. At
//! fixed slope, residual ∝ (decline_rate·lag) reduces to residual ∝ lag — the
//! clean, falsifiable claim. The decline_rate factor itself is NOT tested by
//! this sweep (it would need a slope axis with its own fire-time/window issues,
//! and there is no hardware to validate a decline-rate law against): it is
//! ASSERTED-NOT-TESTED — the dimensionally-forced bias-accumulation mechanism,
//! not an empirical claim here. (Same split-honesty as the open-guard status.)
//!
//! METRIC ON THE LAG AXIS — PER-MINUTE bias-RATE, not integrated area. This is
//! the closure of the position confound (equal-length windows at DIFFERENT
//! descent positions still see different bias unless the ramp is perfectly
//! linear AND they align by luck). Integrated area ∫(cost rate)dt entangles
//! "where the window sat" with "how much bias α faced"; the per-minute rate
//! (mean cost rate over the window) DIVIDES position out — it asks "given the
//! bias present, how much did α reduce the cost RATE," which is position-
//! invariant on a pure-descent window. This changes the QUANTITY rather than
//! bounding position's effect, which is why the regress stops here: a position-
//! invariant measure cannot be confounded with position.
//!
//! P2_POSTFIRE_WIN_MIN = 45 min — the fixed post-fire window (see P2), DERIVED
//! not chosen, NOT swept (a swept length would be a tuning knob → confound by
//! selection). The bracket ENDPOINTS each trace to an independent physical
//! quantity (τ and slope) — NOT to "the range that admits 45" — so 45's position
//! inside it is verifiable, not circular:
//!   LOWER = 5τ = 30 min. Provenance: τ=360s=6min is the champion EWMA time
//!     constant (fixed, not chosen here); 5τ is the standard "substantially
//!     expressed" horizon for an exponential response (>99% settled). Derived
//!     from τ alone, independent of the window value.
//!   UPPER = descent − latest_fire = 150 − 18 = 132 min. Provenance: descent =
//!     drop_frac/rate·60 = 0.5/0.20·60 = 150 min is fixed by slope + drop depth;
//!     latest_fire = max_lag(3min) + gate_cross(0.05/0.20·60 = 15min) = 18 min is
//!     fixed by lag axis + gate. Derived from slope/gate/lag alone, independent
//!     of the window value.
//! 45 sits inside [30, 132] with +15min margin below and +87min above. It is the
//! shortest value clearing the lower bound (maximizes clearance from the bottom,
//! minimizes position spread). 45min = 7.5τ. The 20%/hr slope was chosen because
//! a steeper slope tightens the bracket (50%/hr → [30,51], no clean margin) — the
//! REJECTION of the slope that would make the window fragile is the tell that the
//! selection was on bracket-integrity, not on outcome.
//!
//! MAGNITUDE GATE — IN THE SWEEP, not buried. The ease fires only if the
//! declared drop exceeds HINT_DROP_GATE. This is itself a noise defense (a
//! higher gate ignores telemetry jitter) and a missed-small-decline risk (too
//! high re-exposes the over-diff area). Swept: gate ∈ {0.05, 0.10, 0.20} ON THE
//! STEP (for P3). On the RAMP/lag axis the gate is HELD at its LOWEST level
//! (0.05) — see the P2 confound note below.
//!
//! PRE-REGISTERED PREDICTIONS (each individually falsifiable):
//!   P1 (α crossover SHAPE, stated at the resolution the grid supports). α* is
//!      reported as INDICATIVE DIRECTION ONLY, never a measured optimum (the
//!      coarse {0.25,0.5,0.75,1.0} grid cannot pin a sub-step α*; a finer grid
//!      would only trade quantization for CI-jitter at fixed trial count). P1
//!      confirms IFF BOTH:
//!        (a) [falsifiable CORE, grid-INDEPENDENT] interior α (<1) beats α=1 on
//!            over-diff-area-saved at some σ>0 with NON-OVERLAPPING CIs; AND
//!        (b) the indicative α*(σ) sequence is NON-INCREASING IN σ, ties allowed
//!            (monotone-or-flat between adjacent grid points — the grid can't
//!            resolve sub-step movement, so only a REVERSAL counts as failure).
//!      FAILS IF: no interior α ever CI-separates from α=1 (the wobble isn't from
//!      over-commitment → the mechanism story is wrong), OR α*(σ) REVERSES
//!      (rises then the structure is real, not quantization). Clause (b) with
//!      ties cannot fail by quantization; clause (a) is grid-free. σ=0/α=1 is
//!      context (shown last round), not load-bearing.
//!   P2 (noise vs lag are DIFFERENT — RESIDUAL-RATE ∝ lag, fixed slope, RAMP):
//!      damping (lowering α) materially reduces the σ-axis cost (zero-mean
//!      misread, α averages it out) but does NOT fix the lag-axis cost — a stale
//!      value is BIASED, and α down-weights bias without correcting it.
//!      Falsifiable shape at FIXED slope (decline_rate held constant on the lag
//!      axis, so residual ∝ decline_rate·lag reduces to residual ∝ lag): the
//!      residual per-minute bias-RATE after damping RISES LINEARLY WITH lag_ticks
//!      and is ≈ FLAT IN α. FAILS IF: damping flattens the lag→residual-rate
//!      slope (α reducing the lag-rate ⇒ α is fixing bias, contra the claim), OR
//!      damping helps the lag axis as much as the noise axis. The decline_rate
//!      factor is ASSERTED-NOT-TESTED (no slope axis; dimensionally forced —
//!      see lag-model note).
//!      METRIC: PER-MINUTE bias-rate (mean cost rate over the window), NOT
//!      integrated area — divides descent POSITION out of the compared quantity
//!      (see metric note). WINDOW: fixed-duration P2_POSTFIRE_WIN_MIN=45min,
//!      start = the tick the ease first fired (NOT drop onset), same length every
//!      lag cell — a laggier hint crosses the gate later, so a drop-aligned
//!      window would read a window-START artifact as a lag effect (the clock
//!      confound). Per-minute + post-fire-aligned + fixed-length together let lag
//!      enter ONLY through bias magnitude. Trials that never fire are excluded
//!      (conditional-on-fired), not zero-filled.
//!   P3 (gate ⟷ α partial substitutes): raising HINT_DROP_GATE FLATTENS α's
//!      advantage (a higher gate is itself a noise filter, partially substituting
//!      for damping). FAILS IF: gate and α are independent (α's advantage is the
//!      same at every gate) or complementary (raising the gate widens α's gap).
//!      CONSTRUCTION GUARD: only counted at gate levels where the trigger still
//!      fires on a STRONG majority of trials (fire-rate ≥ FIRE_RATE_FLOOR=0.80,
//!      reported per cell regardless so exclusions are visible, not silent). Why
//!      0.80 not 0.60: a no-action trial contributes baseline (un-eased) cost
//!      that dilutes α's measured advantage toward flat — that dilution IS
//!      partial fire-collapse leaking in. At 0.60, up to 40% of a cell's cost can
//!      be no-action contamination; 0.80 caps it at 20%, small enough that an
//!      observed flattening is α/gate substitution, not absence-of-action.
//!      Excluding the high-gate cells that can't clear 0.80 is the POINT — those
//!      are the regime P3 is not about. If α's advantage flattens only because
//!      the fire rate COLLAPSED, that is the trivial case, excluded. P3 confirmed
//!      only if α flattens WHILE fire-rate stays ≥ 0.80.
//!   P4 (spm scaling — as a SEPARATION-ONSET shift, grid-robust). Because noise
//!      is multiplicative-in-H and the over-diff area is worst at sparse rates
//!      (established), the σ at which interior α first CI-separates from α=1 (the
//!      P1(a) crossover ONSET) appears at a LOWER σ for low spm than high spm —
//!      a given fractional misread buys a longer-lived excursion at sparse rates,
//!      so damping starts paying off sooner. Stated as WHERE the separation
//!      appears across spm, NOT as an α* point-value comparison (which the grid
//!      can't support). FAILS IF: the separation onset is flat across spm, or
//!      appears at a lower σ for HIGH spm. (Reading off the same CI-separation
//!      structure as P1(a), so a grid too coarse to pin α* still resolves it.)
//!
//! P2 ⟷ GATE CONFOUND (the one that can confirm P2 by construction — guarded).
//! On a RAMP, the hint descends gradually, so a HIGH gate delays the ease until
//! the (laggy, biased-high) hint has fallen far enough to cross it — which
//! delays ALL easing and suppresses the lag-axis cost difference between α
//! values. Then "damping barely moves the lag cost" confirms because THE GATE
//! GATED OUT THE ACTION, not because α can't fix bias — P2 confirmed for a
//! reason that has nothing to do with P2. GUARD (pre-registered): on the
//! ramp/lag axis the gate is HELD at its lowest level (0.05) AND P2's α-effect
//! is measured CONDITIONAL ON THE TRIGGER HAVING FIRED (not marginal over ramps
//! where nothing fired). Marginal-over-all-ramps cannot see what P2 is about.
//!
//! PLAUSIBILITY FLOOR — OBSERVABLE-KEYED, not a magic constant. MIN_PLAUSIBLE is
//! NOT a hardcoded H. A declared nominal is self-refuting if, against the CURRENT
//! target, it predicts a share rate below a tiny fraction of r* — it cannot be
//! producing the shares being observed. So the floor is keyed to the same
//! physics as the open-target derivation (D = nominal/r*) and survives across
//! hardware generations. Rejection ⇒ ignore the hint (trigger) / fall back to a
//! sane default open target (open), never act on the rejected value.
//!
//! TWO ENTRY POINTS, ONE GUARD (consolidation — the open-time guard folded in so
//! it isn't orphaned). The same plausibility floor runs at BOTH:
//!   (i)  the mid-run trigger (before easing), and
//!   (ii) channel OPEN, BEFORE hash_rate_to_target — because D_open is DERIVED
//!        from the nominal, a sentinel open nominal produces a sentinel open
//!        target directly (the slot-2 lesson at open). Reject ⇒ default target.
//!
//! SPLIT VALIDATION STATUS (flagged so the eventual doc doesn't conflate them):
//!   - The TRIGGER (decline ease) is sim-only now but GETS DATA when native-sv2
//!     miners point in (real UpdateChannel traffic) — near horizon.
//!   - The OPEN-TIME GUARD is sim-DEMONSTRABLE (we can show sentinel→sentinel-
//!     target and the guard intercepting it) but UNFALSIFIABLE on the current
//!     topology: one aggregate translator channel, no per-device open
//!     declarations to validate against — a more distant horizon.
//!   These two halves do NOT share a validation status.
//!
//! RESIDUAL HOLE (on the books, unaddressed by anything in scope): an
//! OVER-declared open (firmware/config too HIGH → D_open too high → starvation)
//! is not a drop (downward hint can't touch it) and is the upward-corroboration
//! leg we declined. Bounded by the max_target clamp + the n_ticks==0 fast-path,
//! but exposure scales with over-declaration magnitude under a slow first share.
//! ===========================================================================
//!
//! Usage: cargo run --release --bin downward-hint
//! Env: VARDIFF_DH_TRIALS (default 300), VARDIFF_DH_THREADS, VARDIFF_SWEEP_SEED.
//!      Sweep axes (set to enter the sweep; unset = the original 3-arm ceiling):
//!      VARDIFF_DH_ALPHA, VARDIFF_DH_SIGMA, VARDIFF_DH_LAG, VARDIFF_DH_GATE.

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use channels_sv2::target::hash_rate_to_target;
use channels_sv2::vardiff::composed::{champion_composed, Estimator};
use channels_sv2::vardiff::{Clock, MockClock, Vardiff};
use vardiff_sim::baseline::TRUE_HASHRATE;
use vardiff_sim::rng::{sample_poisson, XorShift64};
use vardiff_sim::schedule::HashrateSchedule;
use bitcoin::Target;

const TICK: u64 = 60;
const MATURE_MIN: u64 = 60; // mature on-target so the counter is settled
const DROP_AT_MIN: u64 = 60; // the step DOWN happens at the end of maturation
// The hashrate RECOVERS back to full this many minutes after the drop. Long
// enough that the decline reaction has fully settled before recovery starts, so
// the two windows don't bleed into each other. Env-overridable.
fn recover_at_min() -> u64 {
    env::var("VARDIFF_DH_RECOVER_AT_MIN").ok().and_then(|s| s.parse().ok()).unwrap_or(120)
}
// Observe this many minutes AFTER recovery, so the upward (share-driven) leg
// fully settles too. Env-overridable for the convergence tail check.
fn observe_min() -> u64 {
    env::var("VARDIFF_DH_OBSERVE_MIN").ok().and_then(|s| s.parse().ok()).unwrap_or(180)
}
const DROP_FRAC: f32 = 0.50; // 50% hashrate drop (the slow-decline gate's depth)
const POST_EASE_PROBE_TICKS: u64 = 5; // Q1: ticks after the ease to sum |e| over
// The pool eases when declared < belief by more than this (a real downward
// revision, not noise). The hint is assumed to carry the true post-drop H
// (perfect telemetry) — the BEST case; a noisy-hint arm is future work.
//
// IMPORTANT — the gate is MAGNITUDE-only here; it is NOT the full trigger. The
// production trigger must gate on PLAUSIBILITY, not share-corroboration: ease
// is the safe direction (a false ease costs only the benign wobble measured
// below, self-correcting within τ), so we eager-ease on a *plausible* downward
// declaration with no share-wait — waiting to corroborate a drop means waiting
// to observe FEWER shares, which is exactly the detection gap the hint exists
// to skip. The real work is done by a static plausibility FLOOR (the slot-2
// lesson): "drop to nominal=1" is downward and would pass this magnitude gate,
// but is a sentinel — easing to it floors the estimator and collapses
// difficulty. So production = {reject implausible-downward outright; eager-ease
// on plausible-downward}. This sim assumes plausible perfect telemetry, so it
// models only the magnitude gate; the plausibility floor is specced separately.
const HINT_DROP_GATE: f32 = 0.10;

// ===== ship-number sweep constants (pre-registered; see header) =====
const ALPHA_GRID: [f64; 4] = [0.25, 0.5, 0.75, 1.0]; // damped-blend coefficient
// Multiplicative-in-H noise (step axis). 0–0.30 is the pre-registered realistic
// range; 0.45/0.60 are BOUNDARY points added post-hoc (clearly labeled) to test
// whether the α=1 advantage — which SHRINKS across 0–0.30 — continues toward a
// crossover or flattens near the edge. They extend, not alter, the locked grid.
const SIGMA_GRID: [f64; 6] = [0.0, 0.05, 0.15, 0.30, 0.45, 0.60];
const LAG_GRID: [u64; 3] = [0, 1, 3]; // stale-hint ticks (ramp axis)
const GATE_GRID: [f64; 3] = [0.05, 0.10, 0.20]; // magnitude gate (step axis, P3)
const FIRE_RATE_FLOOR: f64 = 0.80; // P3: cells below this fire-rate are excluded
const RAMP_RATE_PCT_HR: f64 = 20.0; // single fixed slope on the lag axis (P2)
const P2_POSTFIRE_WIN_MIN: u64 = 45; // fixed post-fire window on the lag axis (P2)
const SWEEP_GATE_LOW: f64 = 0.05; // gate held low on the ramp/lag axis (P2 confound guard)

#[derive(Clone, Copy, PartialEq)]
enum Arm {
    SharesOnly,
    EaseNoRescale,
    EaseRescale,
}
impl Arm {
    fn label(&self) -> &'static str {
        match self {
            Arm::SharesOnly => "a:shares-only",
            Arm::EaseNoRescale => "b:ease-no-rescale",
            Arm::EaseRescale => "c:ease-rescale",
        }
    }
}

fn to_target(h: f32, spm: f32) -> Target {
    hash_rate_to_target(h.max(1.0) as f64, spm.max(0.001) as f64)
        .expect("hash_rate_to_target positive inputs")
}

struct Trace {
    // --- decline window (drop → recovery): the over-difficulty leg ---
    peak_over_e: f64,   // max e>0 (%) — over-difficulty / starvation
    over_area: f64,     // ∫ max(e,0) dt  (e-min)
    // --- recovery window (recovery → end): the under-difficulty leg ---
    // After recovery, true H jumps back up while Ĥ is still LOW (the hint eased
    // it), so e goes NEGATIVE (under-difficulty, safe side). The share-driven
    // controller must tighten back. This is where arm b's desynced estimator
    // could bite — measured here, not assumed.
    rec_worst_under_e: f64, // min e<0 (%) in recovery (depth of under-diff)
    rec_under_area: f64,    // ∫ |min(e,0)| dt (e-min) in recovery
    rec_settle_min: f64,    // minutes after recovery until |e| ≤ 3% and stays
    settled_e: f64,         // e (%) at very end (must converge across arms)
    post_ease_amp: f64,     // Q1: Σ|e| over the first ticks after the ease
}

/// One decline trial for one arm. Replicates the sim trial loop inline so the
/// pool's mid-run operating-point write can be injected; share sampling and
/// target update match trial.rs exactly (Poisson(λ), λ from true/belief ratio).
fn run_decline(arm: Arm, spm: f32, seed: u64) -> Trace {
    let clock = Arc::new(MockClock::new(0));
    let mut v = champion_composed(1.0, clock.clone() as Arc<dyn Clock>);

    // Schedule: mature at TRUE, step DOWN by DROP_FRAC at drop_at, then RECOVER
    // back to full TRUE at recover_at. The recovery is share-driven in every arm
    // (the hint is downward-only), but arm b carries a desynced estimator out of
    // the ease — the recovery window is where that could bite.
    let h_post = TRUE_HASHRATE * (1.0 - DROP_FRAC);
    let drop_at = DROP_AT_MIN * 60;
    let recover_at = (DROP_AT_MIN + recover_at_min()) * 60;
    let schedule = HashrateSchedule::new(vec![
        (0, TRUE_HASHRATE),
        (drop_at, h_post),
        (recover_at, TRUE_HASHRATE),
    ]);
    let total = recover_at + observe_min() * 60;

    let mut rng = XorShift64::new(seed);
    let mut belief = TRUE_HASHRATE; // operating point Ĥ (= channel nominal reg)
    let mut target = to_target(belief, spm);

    let mut last_t = 0u64;
    let mut tick_at = TICK;
    let mut hinted = false; // pool applies the downward hint once, at the drop

    // decline-window (drop_at < t ≤ recover_at)
    let mut peak_over = 0.0f64;
    let mut over_area = 0.0f64;
    // recovery-window (t > recover_at)
    let mut rec_worst_under = 0.0f64;
    let mut rec_under_area = 0.0f64;
    let mut rec_settle_tick: Option<u64> = None;
    let mut settled = 0.0f64;
    // Q1 DIAGNOSTIC (b-vs-c mechanism): the post-ease transient amplitude =
    // Σ|e| over the first POST_EASE_PROBE_TICKS after the hint fires. Hypothesis
    // (precision give-back): c resyncs the estimator, DISCARDING the EWMA's
    // banked ~1/√(r*τ) smoothing for nominal-tracking the operating-point ease
    // already provided → c should be briefly NOISIER (larger amplitude) than b,
    // which retains the smoothing. If amp_c > amp_b, Q1 closes: the rescale is a
    // strict precision give-back, which is why it doesn't earn its keep.
    let mut hint_tick: Option<u64> = None;
    let mut post_ease_amp = 0.0f64;

    while tick_at <= total {
        // --- sample shares for this interval (matches trial.rs) ---
        let mid = (last_t + tick_at) / 2;
        let true_h = schedule.at(mid) as f64;
        let est_h = belief as f64;
        let secs = (tick_at - last_t) as f64;
        let lambda = if est_h > 0.0 {
            (true_h / est_h) * (spm as f64) * (secs / 60.0)
        } else {
            0.0
        };
        let n = sample_poisson(&mut rng, lambda);
        v.add_shares(n);

        // --- POOL PRE-STEP: apply a downward hint once, at the drop tick ---
        // Models UpdateChannel(new_nominal = true post-drop H) arriving ~at the
        // drop. Eager-ease: act only DOWNWARD, only when the revision exceeds
        // the gate. This is the pool writing the operating-point register
        // before try_vardiff — exactly the no-trait-change move.
        if arm != Arm::SharesOnly && !hinted && tick_at > drop_at {
            let declared = h_post; // perfect telemetry (best case)
            if (declared as f64) < (belief as f64) * (1.0 - HINT_DROP_GATE as f64) {
                let old = belief;
                belief = declared;
                target = to_target(belief, spm);
                if arm == Arm::EaseRescale {
                    // Resync the estimator the way a real fire would, so the
                    // smoothed rate stays consistent with the new easier target.
                    v.estimator.on_fire(belief, old);
                }
                hinted = true;
                hint_tick = Some(tick_at);
            }
        }

        // --- advance clock, run the controller (share-driven leg) ---
        let belief_before = belief;
        clock.set(tick_at);
        let res = v.try_vardiff(belief, &target, spm);
        if let Ok(Some(new_h)) = res {
            belief = new_h;
            target = to_target(new_h, spm);
        }

        // --- record e for this tick (using belief BEFORE the decision) ---
        let h_true_tick = schedule.at(tick_at.saturating_sub(TICK / 2)) as f64;
        let e = (belief_before as f64 / h_true_tick).ln() * 100.0;
        // DECLINE window: drop → recovery. The over-difficulty (costly) leg.
        if tick_at > drop_at && tick_at <= recover_at {
            if e > peak_over {
                peak_over = e;
            }
            if e > 0.0 {
                over_area += e * (secs / 60.0);
            }
        }
        // Q1 probe: Σ|e| over the first POST_EASE_PROBE_TICKS after the ease.
        if let Some(ht) = hint_tick {
            if tick_at > ht && tick_at <= ht + POST_EASE_PROBE_TICKS * TICK {
                post_ease_amp += e.abs();
            }
        }
        // RECOVERY window: after H jumps back to full. The under-difficulty
        // (safe) leg — Ĥ is low, true H is high, e<0, controller tightens back.
        if tick_at > recover_at {
            if e < rec_worst_under {
                rec_worst_under = e;
            }
            if e < 0.0 {
                rec_under_area += (-e) * (secs / 60.0);
            }
            // first tick |e| ≤ 3% and stays (3% > the ~1-2% steady jitter band)
            if e.abs() <= 3.0 {
                if rec_settle_tick.is_none() {
                    rec_settle_tick = Some(tick_at);
                }
            } else {
                rec_settle_tick = None;
            }
            settled = e;
        }

        last_t = tick_at;
        tick_at += TICK;
    }

    let rec_settle_min = rec_settle_tick
        .map(|t| (t.saturating_sub(recover_at)) as f64 / 60.0)
        .unwrap_or(f64::NAN);

    Trace {
        peak_over_e: peak_over,
        over_area,
        rec_worst_under_e: rec_worst_under,
        rec_under_area,
        rec_settle_min,
        settled_e: settled,
        post_ease_amp,
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.retain(|x| !x.is_nan());
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Mean and 95% CI half-width (1.96·SE) over a sample — for the CI-separation
/// clauses (P1(a), P4). NaNs dropped. Returns (mean, ci_half, n).
fn mean_ci(v: &[f64]) -> (f64, f64, usize) {
    let xs: Vec<f64> = v.iter().copied().filter(|x| !x.is_nan()).collect();
    let n = xs.len();
    if n == 0 {
        return (f64::NAN, f64::NAN, 0);
    }
    let mean = xs.iter().sum::<f64>() / n as f64;
    if n < 2 {
        return (mean, f64::NAN, n);
    }
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);
    let se = (var / n as f64).sqrt();
    (mean, 1.96 * se, n)
}

/// Gaussian sample via Box-Muller from the sim's XorShift64 (no Date/rand dep).
fn sample_normal(rng: &mut XorShift64, mean: f64, sd: f64) -> f64 {
    let u1 = rng.next_f64().max(1e-12);
    let u2 = rng.next_f64();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mean + sd * z
}

// ---------------------------------------------------------------------------
// STEP-NOISE arm (P1/P3/P4): a 50% STEP drop, damped-blend ease with a NOISY
// declared value (multiplicative-in-H), at a given (spm, α, σ, gate). Returns
// per-trial (over_diff_area_saved_vs_sharesonly, worst_under_wobble, fired).
// over-diff area saved is scored against a paired shares-only baseline on the
// SAME seed (paired-difference → tighter CIs, the comparability the arc keeps).
// ---------------------------------------------------------------------------
struct StepResult {
    over_area: f64,   // e-min over the decline window (e>0 only)
    worst_under: f64, // worst e<0 after the ease (%)
    fired: bool,
}

fn run_step_noise(spm: f32, alpha: f64, sigma: f64, gate: f64, ease: bool, seed: u64) -> StepResult {
    let clock = Arc::new(MockClock::new(0));
    let mut v = champion_composed(1.0, clock.clone() as Arc<dyn Clock>);
    let h_post = TRUE_HASHRATE * (1.0 - DROP_FRAC);
    let drop_at = DROP_AT_MIN * 60;
    let schedule = HashrateSchedule::new(vec![(0, TRUE_HASHRATE), (drop_at, h_post)]);
    let total = drop_at + 120 * 60; // 120-min post-drop plateau (settles even at 2spm)

    let mut rng = XorShift64::new(seed);
    let mut belief = TRUE_HASHRATE;
    let mut target = to_target(belief, spm);
    let (mut last_t, mut tick_at) = (0u64, TICK);
    let mut hinted = false;
    let mut fired = false;
    let (mut over_area, mut worst_under) = (0.0f64, 0.0f64);

    while tick_at <= total {
        let mid = (last_t + tick_at) / 2;
        let true_h = schedule.at(mid) as f64;
        let secs = (tick_at - last_t) as f64;
        let lambda = if belief > 0.0 {
            (true_h / belief as f64) * (spm as f64) * (secs / 60.0)
        } else { 0.0 };
        v.add_shares(sample_poisson(&mut rng, lambda));

        // POOL pre-step: eager-ease on a plausible downward declaration. The
        // declared value carries multiplicative-in-H noise: declared = H_true·(1+ε).
        if ease && !hinted && tick_at > drop_at {
            let noisy = (h_post as f64) * (1.0 + sample_normal(&mut rng, 0.0, sigma));
            let declared = noisy.max(1.0) as f32;
            if (declared as f64) < (belief as f64) * (1.0 - gate) {
                // damped blend toward declared (α=1 ⇒ hard set).
                belief = ((1.0 - alpha) * belief as f64 + alpha * declared as f64) as f32;
                target = to_target(belief, spm);
                hinted = true;
                fired = true;
            }
        }

        let belief_before = belief;
        clock.set(tick_at);
        if let Ok(Some(new_h)) = v.try_vardiff(belief, &target, spm) {
            belief = new_h;
            target = to_target(new_h, spm);
        }
        let h_true_tick = schedule.at(tick_at.saturating_sub(TICK / 2)) as f64;
        let e = (belief_before as f64 / h_true_tick).ln() * 100.0;
        if tick_at > drop_at {
            if e > 0.0 { over_area += e * (secs / 60.0); }
            if hinted && e < worst_under { worst_under = e; }
        }
        last_t = tick_at;
        tick_at += TICK;
    }
    StepResult { over_area, worst_under, fired }
}

// ---------------------------------------------------------------------------
// RAMP-LAG arm (P2): a sustained 20%/hr decline, damped-blend ease with a
// LAGGED (stale, biased-high) declared value, gate held LOW. Scores the
// PER-MINUTE bias-rate over a fixed-duration post-fire window, CONDITIONAL ON
// the ease having fired. Returns (per_min_bias_rate, fired).
// ---------------------------------------------------------------------------
struct RampResult {
    per_min_bias_rate: f64, // mean |e| per minute over the post-fire window (e>0)
    fired: bool,
}

fn run_ramp_lag(spm: f32, alpha: f64, lag_ticks: u64, seed: u64) -> RampResult {
    let clock = Arc::new(MockClock::new(0));
    let mut v = champion_composed(1.0, clock.clone() as Arc<dyn Clock>);
    let rate = RAMP_RATE_PCT_HR / 100.0 / 60.0; // fraction lost per minute
    let mature = MATURE_MIN * 60;
    // descent to 50% then hold; build a fine 1-min ramp schedule.
    let descent_min = ((DROP_FRAC as f64 / (RAMP_RATE_PCT_HR / 100.0)) * 60.0) as u64;
    let mut segs: Vec<(u64, f32)> = vec![(0, TRUE_HASHRATE)];
    for m in 0..descent_min {
        let frac = (rate * (m as f64 + 1.0)).min(DROP_FRAC as f64);
        segs.push((mature + (m + 1) * 60, TRUE_HASHRATE * (1.0 - frac as f32)));
    }
    let floor_h = TRUE_HASHRATE * (1.0 - DROP_FRAC);
    let bottom_t = mature + descent_min * 60;
    segs.push((bottom_t, floor_h));
    let schedule = HashrateSchedule::new(segs);
    let total = bottom_t + 30 * 60;

    let mut rng = XorShift64::new(seed);
    let mut belief = TRUE_HASHRATE;
    let mut target = to_target(belief, spm);
    let (mut last_t, mut tick_at) = (0u64, TICK);
    let mut fired_at: Option<u64> = None;
    let (mut bias_sum, mut bias_min) = (0.0f64, 0.0f64);

    while tick_at <= total {
        let mid = (last_t + tick_at) / 2;
        let true_h = schedule.at(mid) as f64;
        let secs = (tick_at - last_t) as f64;
        let lambda = if belief > 0.0 {
            (true_h / belief as f64) * (spm as f64) * (secs / 60.0)
        } else { 0.0 };
        v.add_shares(sample_poisson(&mut rng, lambda));

        // POOL pre-step: ease toward a LAGGED declared value (stale H from
        // lag_ticks ago — biased HIGH on a descent). Gate held LOW (SWEEP_GATE_LOW).
        // Re-fires each tick the gate is crossed (a ramp keeps declaring lower);
        // window/scoring keys off the FIRST fire.
        if tick_at > mature {
            let stale_t = tick_at.saturating_sub(lag_ticks * TICK);
            let declared = schedule.at(stale_t.saturating_sub(TICK / 2)).max(1.0);
            if (declared as f64) < (belief as f64) * (1.0 - SWEEP_GATE_LOW) {
                belief = ((1.0 - alpha) * belief as f64 + alpha * declared as f64) as f32;
                target = to_target(belief, spm);
                if fired_at.is_none() { fired_at = Some(tick_at); }
            }
        }

        let belief_before = belief;
        clock.set(tick_at);
        if let Ok(Some(new_h)) = v.try_vardiff(belief, &target, spm) {
            belief = new_h;
            target = to_target(new_h, spm);
        }
        // PER-MINUTE bias accumulation over the fixed post-fire window.
        if let Some(ft) = fired_at {
            if tick_at > ft && tick_at <= ft + P2_POSTFIRE_WIN_MIN * 60 {
                let h_true_tick = schedule.at(tick_at.saturating_sub(TICK / 2)) as f64;
                let e = (belief_before as f64 / h_true_tick).ln() * 100.0;
                if e > 0.0 { bias_sum += e; } // over-difficulty bias (e-% per tick)
                bias_min += 1.0;
            }
        }
        last_t = tick_at;
        tick_at += TICK;
    }
    let per_min = if bias_min > 0.0 { bias_sum / bias_min } else { f64::NAN };
    RampResult { per_min_bias_rate: per_min, fired: fired_at.is_some() }
}

struct Row {
    spm: f32,
    arm: &'static str,
    peak_over: f64,
    over_area: f64,
    rec_worst_under: f64,
    rec_under_area: f64,
    rec_settle_min: f64,
    settled: f64,
    post_ease_amp: f64,
}

fn main() {
    let trials: usize = env::var("VARDIFF_DH_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let base_seed: u64 = env::var("VARDIFF_SWEEP_SEED")
        .ok()
        .and_then(|s| s.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).or_else(|| s.parse().ok()))
        .unwrap_or(0xD0_1D_FACE);
    let n_threads: usize = env::var("VARDIFF_DH_THREADS")
        .ok().and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)).max(1);

    // The pre-registered ship-number sweep (P1-P4). VARDIFF_DH_SWEEP=1 runs it;
    // unset leaves the perfect-telemetry 3-arm ceiling (committed dd420026).
    if env::var("VARDIFF_DH_SWEEP").ok().as_deref() == Some("1") {
        run_sweep(trials, base_seed, n_threads);
        return;
    }

    // Slot rates (3=6spm, 4=30spm) + the sub-guard 2spm (worst decline-safety
    // regime, where the over-difficulty lag is largest) + a mid anchor.
    let spms = [2.0f32, 6.0, 12.0, 30.0];
    let arms = [Arm::SharesOnly, Arm::EaseNoRescale, Arm::EaseRescale];
    let jobs: Vec<(f32, Arm)> = spms.iter().flat_map(|&s| arms.iter().map(move |&a| (s, a))).collect();

    eprintln!(
        "downward-hint: {} cells × {} trials, {} threads. 50% drop at {}min, recover at +{}min, observe +{}min.",
        jobs.len(), trials, n_threads, DROP_AT_MIN, recover_at_min(), observe_min()
    );

    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Row>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= jobs.len() {
                    break;
                }
                let (spm, arm) = jobs[j];
                let (mut po, mut oa, mut rwu, mut rua, mut rsm, mut st, mut pea) =
                    (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
                for i in 0..trials {
                    let t = run_decline(arm, spm, base_seed.wrapping_add((j * 100003 + i) as u64));
                    po.push(t.peak_over_e);
                    oa.push(t.over_area);
                    rwu.push(t.rec_worst_under_e);
                    rua.push(t.rec_under_area);
                    rsm.push(t.rec_settle_min);
                    st.push(t.settled_e);
                    pea.push(t.post_ease_amp);
                }
                out.lock().unwrap().push(Row {
                    spm, arm: arm.label(),
                    peak_over: median(po), over_area: median(oa),
                    rec_worst_under: median(rwu), rec_under_area: median(rua),
                    rec_settle_min: median(rsm), settled: median(st),
                    post_ease_amp: median(pea),
                });
            });
        }
    });

    let mut rows = out.into_inner().unwrap();
    rows.sort_by(|a, b| (a.spm as u32, a.arm).partial_cmp(&(b.spm as u32, b.arm)).unwrap());

    println!("\n## Downward-hint fusion: DECLINE (50% drop) then RECOVERY (back to full). e=ln(Ĥ/H_true).");
    println!("Hint = pool writes operating-point Ĥ←declared at the drop (eager-ease, downward-only). Champion Ewma360/s1.5.");
    println!("Decline leg: e>0 over-difficulty (costly). Recovery leg: e<0 under-difficulty (safe) while Ĥ catches up.\n");
    println!("| spm | arm | DECLINE peak over-e% | decline over-area | REC worst under-e% | rec under-area | rec settle(min) | settled e% |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for r in &rows {
        println!(
            "| {} | {} | {:+.1} | {:.1} | {:+.1} | {:.1} | {:.0} | {:+.1} |",
            r.spm as u32, r.arm, r.peak_over, r.over_area,
            r.rec_worst_under, r.rec_under_area, r.rec_settle_min, r.settled
        );
    }

    println!("\n## RECOVERY comparison — does the downward hint slow/distort the upward leg?");
    println!("The hint is downward-only, so recovery is share-driven in ALL arms. Q: does arm b's desynced");
    println!("estimator (eased Ĥ without on_fire rescale) recover WORSE than shares-only (a) or rescale (c)?");
    println!("| spm | a rec-settle | b rec-settle | c rec-settle | a under-area | b under-area | c under-area |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for &spm in &spms {
        let get = |label: &str| rows.iter().find(|r| r.spm == spm && r.arm == label);
        if let (Some(a), Some(b), Some(c)) =
            (get("a:shares-only"), get("b:ease-no-rescale"), get("c:ease-rescale"))
        {
            println!(
                "| {} | {:.0}m | {:.0}m | {:.0}m | {:.1} | {:.1} | {:.1} |",
                spm as u32,
                a.rec_settle_min, b.rec_settle_min, c.rec_settle_min,
                a.rec_under_area, b.rec_under_area, c.rec_under_area
            );
        }
    }
    println!("\nKEY: arm 'a' (shares-only) recovery is the BASELINE the controller was selected against — it");
    println!("recovers from a 50% drop with no hint. If b's rec-settle and under-area ≈ a's, the hint leaves");
    println!("recovery UNTOUCHED (it only acted on the decline). If b ≫ a, the eased-but-desynced state has a");
    println!("recovery cost the over-difficulty saving must be weighed against.");

    println!("\n## Q1 mechanism — post-ease transient amplitude (Σ|e| over first {} ticks after the ease).", POST_EASE_PROBE_TICKS);
    println!("Precision-give-back hypothesis: c resyncs (discards EWMA banked smoothing) → c should be NOISIER");
    println!("than b (which retains it). If amp_c > amp_b, the rescale is a strict precision give-back → Q1 closes.");
    println!("| spm | b amp (no-rescale) | c amp (rescale) | c − b | verdict |");
    println!("| --- | --- | --- | --- | --- |");
    for &spm in &spms {
        let get = |label: &str| rows.iter().find(|r| r.spm == spm && r.arm == label);
        if let (Some(b), Some(c)) = (get("b:ease-no-rescale"), get("c:ease-rescale")) {
            let d = c.post_ease_amp - b.post_ease_amp;
            let verdict = if d > 1.0 { "c noisier (give-back ✓)" } else if d < -1.0 { "b noisier (✗ refutes)" } else { "≈ (inconclusive)" };
            println!(
                "| {} | {:.1} | {:.1} | {:+.1} | {} |",
                spm as u32, b.post_ease_amp, c.post_ease_amp, d, verdict
            );
        }
    }
}

// ===========================================================================
// SHIP-NUMBER SWEEP (P1-P4). Reads off CI-separation structure; reports
// surviving-N for every cell (per the execution-honesty ask). Two tables,
// partitioned by which decline shape can falsify which prediction.
// ===========================================================================
fn run_sweep(trials: usize, base_seed: u64, n_threads: usize) {
    eprintln!(
        "downward-hint SWEEP: {} trials/cell, {} threads. step-noise (P1/P3/P4) + ramp-lag (P2).",
        trials, n_threads
    );
    let spms = [2.0f32, 6.0, 30.0];

    // ---- STEP-NOISE jobs: (spm, alpha, sigma, gate) over the α/σ grid (gate
    // fixed 0.05 for P1/P4) PLUS the gate grid at σ=0.15 (for P3). Each scored
    // as paired over-diff-area-SAVED vs a shares-only baseline on the same seed.
    #[derive(Clone, Copy)]
    struct StepJob { spm: f32, alpha: f64, sigma: f64, gate: f64, kind: u8 } // kind 0=P1/P4 grid, 1=P3 gate grid
    let mut step_jobs: Vec<StepJob> = Vec::new();
    for &spm in &spms {
        for &alpha in &ALPHA_GRID {
            for &sigma in &SIGMA_GRID {
                step_jobs.push(StepJob { spm, alpha, sigma, gate: GATE_GRID[0], kind: 0 });
            }
            // P3 gate sweep at a representative mid noise (σ=0.15).
            for &gate in &GATE_GRID[1..] {
                step_jobs.push(StepJob { spm, alpha, sigma: 0.15, gate, kind: 1 });
            }
        }
    }
    // ---- RAMP-LAG jobs: (spm, alpha, lag), gate held low, per-minute metric.
    #[derive(Clone, Copy)]
    struct RampJob { spm: f32, alpha: f64, lag: u64 }
    let mut ramp_jobs: Vec<RampJob> = Vec::new();
    for &spm in &spms {
        for &alpha in &ALPHA_GRID {
            for &lag in &LAG_GRID {
                ramp_jobs.push(RampJob { spm, alpha, lag });
            }
        }
    }

    // Step results: (job_index) -> (mean saved, ci, n, fire_rate, wobble_median).
    struct StepOut { spm: f32, alpha: f64, sigma: f64, gate: f64, kind: u8,
                     saved_mean: f64, saved_ci: f64, n: usize, fire_rate: f64, wobble: f64 }
    let step_out: Mutex<Vec<StepOut>> = Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| loop {
                let j = next.fetch_add(1, Ordering::Relaxed);
                if j >= step_jobs.len() { break; }
                let job = step_jobs[j];
                let mut saved = Vec::with_capacity(trials);
                let mut wob = Vec::with_capacity(trials);
                let mut fires = 0usize;
                for i in 0..trials {
                    let seed = base_seed.wrapping_add((j as u64) * 1_000_003 + i as u64);
                    // paired baseline (no ease) and treatment (ease) on the SAME seed.
                    let base = run_step_noise(job.spm, job.alpha, job.sigma, job.gate, false, seed);
                    let treat = run_step_noise(job.spm, job.alpha, job.sigma, job.gate, true, seed);
                    if treat.fired {
                        saved.push(base.over_area - treat.over_area);
                        wob.push(treat.worst_under);
                        fires += 1;
                    }
                }
                let (m, ci, n) = mean_ci(&saved);
                let fire_rate = fires as f64 / trials as f64;
                step_out.lock().unwrap().push(StepOut {
                    spm: job.spm, alpha: job.alpha, sigma: job.sigma, gate: job.gate, kind: job.kind,
                    saved_mean: m, saved_ci: ci, n, fire_rate, wobble: median(wob),
                });
            });
        }
    });

    // Ramp results.
    struct RampOut { spm: f32, alpha: f64, lag: u64, rate_mean: f64, rate_ci: f64, n: usize, fire_rate: f64 }
    let ramp_out: Mutex<Vec<RampOut>> = Mutex::new(Vec::new());
    let next2 = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| loop {
                let j = next2.fetch_add(1, Ordering::Relaxed);
                if j >= ramp_jobs.len() { break; }
                let job = ramp_jobs[j];
                let mut rates = Vec::with_capacity(trials);
                let mut fires = 0usize;
                for i in 0..trials {
                    let seed = base_seed.wrapping_add((j as u64) * 2_000_029 + i as u64);
                    let r = run_ramp_lag(job.spm, job.alpha, job.lag, seed);
                    if r.fired {
                        rates.push(r.per_min_bias_rate); // conditional-on-fired
                        fires += 1;
                    }
                }
                let (m, ci, n) = mean_ci(&rates);
                ramp_out.lock().unwrap().push(RampOut {
                    spm: job.spm, alpha: job.alpha, lag: job.lag,
                    rate_mean: m, rate_ci: ci, n, fire_rate: fires as f64 / trials as f64,
                });
            });
        }
    });

    // ---- P1/P4 table: over-diff-area SAVED (mean ± 95% CI, n) per α × σ, gate=0.05.
    let mut so = step_out.into_inner().unwrap();
    so.sort_by(|a, b| (a.kind, a.spm as u32, (a.sigma*100.0) as u32, (a.gate*100.0) as u32, (a.alpha*100.0) as u32)
        .partial_cmp(&(b.kind, b.spm as u32, (b.sigma*100.0) as u32, (b.gate*100.0) as u32, (b.alpha*100.0) as u32)).unwrap());

    println!("\n## P1/P4 — over-diff area SAVED (e-min, mean ±95%CI, surviving n) vs α × σ. Gate=0.05.");
    println!("P1(a): does interior α (<1) beat α=1 with NON-OVERLAPPING CIs at some σ? P4: does that onset σ shift with spm?");
    for &spm in &spms {
        println!("\n### spm={}", spm as u32);
        println!("| σ \\ α | 0.25 | 0.50 | 0.75 | 1.00 | interior-beats-α1? |");
        println!("| --- | --- | --- | --- | --- | --- |");
        for &sigma in &SIGMA_GRID {
            let cell = |a: f64| so.iter().find(|r| r.kind==0 && r.spm==spm && (r.sigma-sigma).abs()<1e-9 && (r.alpha-a).abs()<1e-9);
            let fmt = |a: f64| cell(a).map(|r| format!("{:.0}±{:.0}(n{})", r.saved_mean, r.saved_ci, r.n)).unwrap_or("—".into());
            // P1(a): best interior α's lower CI > α=1's upper CI ?
            let a1 = cell(1.0);
            let sep = [0.25,0.5,0.75].iter().any(|&a| {
                if let (Some(ri), Some(r1)) = (cell(a), a1) {
                    (ri.saved_mean - ri.saved_ci) > (r1.saved_mean + r1.saved_ci)
                } else { false }
            });
            println!("| {:.2} | {} | {} | {} | {} | {} |", sigma, fmt(0.25), fmt(0.5), fmt(0.75), fmt(1.0),
                if sep { "YES (interior CI-separates)" } else { "no" });
        }
    }

    // ---- P3 table: SAVED per α × gate at σ=0.15, with FIRE-RATE and surviving-N.
    println!("\n## P3 — over-diff area SAVED vs α × gate (σ=0.15). Cells with fire-rate < {:.2} are EXCLUDED (shown but flagged).", FIRE_RATE_FLOOR);
    println!("P3: does raising the gate FLATTEN α's advantage WHILE fire-rate stays ≥ floor? (flattening via fire-collapse = trivial, excluded.)");
    for &spm in &spms {
        println!("\n### spm={}", spm as u32);
        println!("| gate \\ α | 0.25 | 0.50 | 0.75 | 1.00 | fire-rate(α=.5) | usable? |");
        println!("| --- | --- | --- | --- | --- | --- | --- |");
        let gates = [GATE_GRID[0], GATE_GRID[1], GATE_GRID[2]];
        for &gate in &gates {
            // gate=0.05 lives in kind=0 (σ grid); 0.10/0.20 in kind=1 (σ=0.15). Match σ=0.15 row for 0.05 too.
            let cell = |a: f64| so.iter().find(|r| r.spm==spm && (r.gate-gate).abs()<1e-9 && (r.sigma-0.15).abs()<1e-9 && (r.alpha-a).abs()<1e-9);
            let fmt = |a: f64| cell(a).map(|r| format!("{:.0}±{:.0}(n{})", r.saved_mean, r.saved_ci, r.n)).unwrap_or("—".into());
            let fr = cell(0.5).map(|r| r.fire_rate).unwrap_or(f64::NAN);
            let usable = if fr >= FIRE_RATE_FLOOR { "yes" } else { "EXCLUDED (fire<floor)" };
            println!("| {:.2} | {} | {} | {} | {} | {:.2} | {} |", gate, fmt(0.25), fmt(0.5), fmt(0.75), fmt(1.0), fr, usable);
        }
    }

    // ---- P2 table: per-minute bias-RATE vs α × lag (ramp, gate low, conditional-on-fired).
    let mut ro = ramp_out.into_inner().unwrap();
    ro.sort_by(|a, b| (a.spm as u32, a.lag, (a.alpha*100.0) as u32).partial_cmp(&(b.spm as u32, b.lag, (b.alpha*100.0) as u32)).unwrap());
    println!("\n## P2 — per-minute over-diff bias-RATE (mean ±95%CI, surviving n) vs α × lag. Ramp {}%/hr, gate={:.2}, win={}min.",
        RAMP_RATE_PCT_HR, SWEEP_GATE_LOW, P2_POSTFIRE_WIN_MIN);
    println!("P2: does the rate RISE ~linearly with lag and stay ~FLAT in α? (α flat ⇒ α can't fix bias, as claimed.)");
    for &spm in &spms {
        println!("\n### spm={}", spm as u32);
        println!("| lag \\ α | 0.25 | 0.50 | 0.75 | 1.00 | flat-in-α? |");
        println!("| --- | --- | --- | --- | --- | --- |");
        for &lag in &LAG_GRID {
            let cell = |a: f64| ro.iter().find(|r| r.spm==spm && r.lag==lag && (r.alpha-a).abs()<1e-9);
            let fmt = |a: f64| cell(a).map(|r| format!("{:.2}±{:.2}(n{})", r.rate_mean, r.rate_ci, r.n)).unwrap_or("—".into());
            // flat-in-α: do all four α CIs overlap the α=1 CI?
            let a1 = cell(1.0);
            let flat = [0.25,0.5,0.75].iter().all(|&a| {
                if let (Some(ri), Some(r1)) = (cell(a), a1) {
                    !((ri.rate_mean - ri.rate_ci) > (r1.rate_mean + r1.rate_ci)
                      || (ri.rate_mean + ri.rate_ci) < (r1.rate_mean - r1.rate_ci))
                } else { true }
            });
            println!("| {} | {} | {} | {} | {} | {} |", lag, fmt(0.25), fmt(0.5), fmt(0.75), fmt(1.0),
                if flat { "flat (P2-consistent)" } else { "α-dependent (P2 at risk)" });
        }
    }

    println!("\n## READ (locked predictions — confirm/fail/indeterminate, bring to review before doc/memory):");
    println!("P1: confirm iff interior α CI-separates from α=1 at some σ AND α*(σ) non-increasing (ties ok), else fail.");
    println!("P2: confirm iff rate rises ~linearly with lag AND ~flat in α; fail if α flattens the lag-slope.");
    println!("P3: confirm iff α's advantage flattens with gate WHILE fire-rate ≥ {:.2}; indeterminate where excluded.", FIRE_RATE_FLOOR);
    println!("P4: confirm iff the P1(a) separation onset σ is LOWER at low spm than high spm.");
    println!("A clean confirm folds straight in; a fail/indeterminate is a model-vs-null judgment to make TOGETHER, not defaulted.");
}
