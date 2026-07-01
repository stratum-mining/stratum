# PID Vardiff Investigation

> **Status: the EARLIEST of the three vardiff investigation docs (PID predates the
> ckpool port, the champion selection, and item 1) — most-stale, dated to its
> mid-arc state.** Its **method is exactly right** — decomposing into stages so each
> idea is evaluated on its own axis without confounding (the "Meta-observation"
> below) is the spine the whole arc validated. But its **verdicts are dated**;
> current truth is `information-floor.md` (the champion + the closed theory).
> Re-scored against the closed theory:
> - ① the `SignPersistenceCusumBoundary` **"discarded" verdict is SUPERSEDED** — the
>   component ships in the champion's dense-rate arm (corrected in place at §3,
>   "Sub-threshold Persistence");
> - ② **"acceleration=0.2 optimal" is superseded** — the champion uses 0.05 (flag at
>   §1, "Integral Term");
> - ③ the **production pointer is STALE** ("Proposed Composition" — it points at a
>   since-gate-failed ckpool rec);
> - ④ the **"optimized independently" thesis is over-claimed** (sharpened in place — it
>   contradicts the doc's own Meta-observation).
>
> The standing verdicts (AcceleratingPartialRetarget transferred, SpmRatioEstimator
> discarded, the pow2 dead-zone analysis) HOLD. Flagged-as-dated, NOT rewritten —
> same discipline as `THEORY.md`.

## Background

Some open-source pool implementations use a PID controller for variable
difficulty, operating in difficulty-space with power-of-2 quantization.
This document records our investigation of the PID approach, comparison
to our three-stage pipeline, and the improvements derived from the analysis.

## The Pow2-PID Pattern

A common open-source pattern uses the `pid` crate (v4.0.0) with:

- **Setpoint**: shares per minute target (typically 10.0)
- **Kp**: `-difficulty × 0.01` (negative: over-target SPM → lower difficulty)
- **Ki**: `0.0` (disabled in production)
- **Kd**: `0.0` (disabled in production)
- **Output limit**: `difficulty × 10.0`
- **Quantization**: `nearest_power_of_2()` on every retarget
- **Interval**: 120s (configurable)
- **Measurement window**: 20-60s sliding window of share timestamps
- **On retarget**: rebuilds PID with gains proportional to new difficulty

### Critical Flaw: Power-of-2 Dead Zone

The quantization creates a ~41% dead zone. For difficulty to change, the PID
output must push past the geometric midpoint between adjacent powers of 2:

```
|pid_output| > 0.414 × current_difficulty
With Kp = -0.01 × diff:
  |SPM_target - realized| > 41.4
```

At SPM target of 10, realized SPM must exceed ~51 or drop to ~0 to trigger
a retarget. This means **hashrate changes of ≤5× go undetected**.

> **Re-validated against live DMND source (2026-06-25) — confirmed, strengthened,
> and one claim sharpened.** When written, this doc described "a common open-source
> pattern" — pattern-inferred, unpinned. Checked against current
> `dmnd-pool/dmnd-client/src/translator/downstream/diff_management.rs`:
>
> - **pow2 is IN THE CONTROL LAW (hedge removed — pattern-inferred → source-verified).**
>   `update_difficulty_and_hashrate` (the production retarget path) computes a PID
>   delta (`pid.next_control_output(realized_share_per_min)`, :267), rounds the
>   *operating* difficulty via `quantize_downstream_difficulty` → `nearest_power_of_2`
>   (:268/:415/:389), and **stores the quantized value back as controller state,
>   rebuilding the PID around it** (:286–287). The quantized value FEEDS BACK — so the
>   dead zone is source-verified on shipping code, not inferred. (Contrast SV1-SRI's
>   pow2, which is at the wire / no feedback / benign — see the SV2-SRI handoff: DMND
>   and SRI are the two branches of the control-law-vs-wire fork.)
> - **The severity headline STANDS on the production gain, and is gain-specific —
>   here is the structure so it stays re-checkable.** `deadband = 0.414/Kp`. DMND's
>   **sole production gain is `Kp = −diff×0.01`** (:281, in the only non-test retarget
>   path) → `deadband = 0.414/0.01 = 41.4 SPM` → the headline holds. The `−diff×0.05`
>   and `0.0` gains elsewhere in the file are **test fixtures inside `#[cfg(test)]`
>   (begins :425), NOT shipped regimes.** IF DMND ever productionizes a different gain,
>   the deadband scales inversely (the `−0.05` fixture would give `0.414/0.05 = 8.3
>   SPM`, ~5× smaller) — detectable against this dated record. The dead-zone
>   *geometry* (√2 threshold, `log2().round()` at :389) is gain-independent and stands
>   regardless; only the *SPM severity* scales with the production Kp, which is `−0.01`
>   today.
> - **The decline side is not merely deadened — it is UNREACHABLE (the sharpest, and
>   most dangerous-in-our-terms, form of the headline).** Around setpoint 10 the
>   deadband is ±41.4 SPM: the *upper* crossing is 10+41.4 ≈ 51 (reachable — a 5×
>   hashrate rise), but the *lower* crossing is 10−41.4 = **−31.4 SPM, below zero and
>   therefore UNREACHABLE.** A falling miner can drop to literally **zero** hashrate
>   and the PID still will not ease, because clearing the deadband downward would
>   require negative realized SPM. So "reaction 0.000 on drops" is **structural, not
>   sluggish** — the controller *cannot follow a decline down* at this gain/setpoint.
>   **In our terms this is the dangerous-direction failure the decline-safety gate
>   exists to catch, in its maximal form:** not "lags the decline into over-difficulty"
>   (the champion's worst case, bounded) but "literally cannot react to the decline" —
>   over-difficulty with no path back. The pow2-in-loop dead zone doesn't just risk
>   the spiral; on the decline side it removes the controller's ability to avoid it.

### Simulation Results

Across all 50 cells (5 SPM × 10 scenarios), Pow2-PID:
- Reaction rate: **0.000** — never fires on any step ≤ ±50%
- Jitter: **0.000** — never fires at all
- Effectively a fixed-difficulty system

## Why PID Fails: Lack of Stage Separation

A PID controller conflates all three pipeline stages into a single
feedback loop, making it impossible to diagnose or fix individual
failure modes:

| PID Term | Conflated Stages | Problem |
|----------|-----------------|---------|
| P (proportional) | Estimator + Boundary | Gain (`Kp`) simultaneously controls how noisy the "belief" is AND how much deviation triggers action. Tuning Kp for low jitter (small gain) kills reaction rate. Tuning for fast reaction (large gain) causes noise-driven fires. |
| I (integral) | Boundary (persistence) + Update (magnitude) | Accumulates sub-threshold error — a boundary concern (evidence strength) — but its output adds directly to the control signal — an update concern (move magnitude). Anti-windup limits are simultaneously clamping "how much evidence to accumulate" and "how far to move." |
| D (derivative) | Estimator (smoothing) + Update (damping) | Acts as both a noise filter on the measurement AND a damping term on the actuator. Cannot tune measurement smoothing independently of move damping. |

### The Dead Zone as a Stage Confusion

The 41% dead zone from power-of-2 quantization is instructive. In our
framework, this is clearly a *boundary* problem — the threshold for
action is too high. But in the PID implementation, the dead zone arises
from the interaction of:
1. Gain magnitude (Kp = -0.01 × diff) — an estimator/boundary concern
2. Quantization rounding — a post-update concern
3. Output limit (10 × diff) — an update concern

Because these aren't separated, the developer cannot identify "the boundary
is too wide" as the root cause. They would instead try to increase Kp
(breaking jitter), add integral (breaking stability), or reduce the
quantization (breaking the power-of-2 invariant the system depends on).

### The Well-Tuned PID (`pid_tuned.rs`)

Our `PidTunedVardiff` implementation with all three terms active
demonstrates the ceiling of the PID approach when carefully tuned:
- Rate-aware gain scheduling (√SPM noise scaling)
- Anti-windup with exponential decay + hard clamp
- Dead zone to suppress noise-driven fires
- Configurable presets (balanced, aggressive, conservative)

Even with these improvements, it cannot escape the fundamental coupling:
the dead zone (a boundary parameter) interacts with the integral
accumulation (a persistence parameter) which interacts with the gain
schedule (an estimator parameter). Tuning one axis shifts the others.
The three-stage pipeline makes these interactions explicit and allows
each to be optimized independently.

> **Refined (the doc is its own witness — this contradicts the "Meta-observation"
> at the end of "What We Learned From PID" below).** "Optimized independently"
> overstates: the stages **interact** — the arc found two couplings: (a)
> dangerous-direction protection is **regime-dependent** (`information-floor.md`
> §6.1 — estimator protects at sparse, boundary at dense); (b) settled-e **depends
> on boundary type** (a ~15pp sign-flip at spm6–8 — the ckpool gate-test, recorded
> in `CKPOOL_INVESTIGATION.md`'s gate-test addendum and `information-floor.md` §8.3's
> scope-correction). Separation does **not** make the stages independent. What it
> does — and what the Meta-observation correctly states — is make the interactions
> **isolable and diagnosable** (vary one axis to *measure* the coupling). That value
> is real and large (it is the arc's whole method); it is *diagnosability-of-
> interaction*, not *independence*. Read "optimized independently" as the looser
> "exercisable on its own axis" the Meta-observation states.

## What We Learned From PID

Despite the broken quantization, the PID *concept* revealed gaps in our
framework. Crucially, decomposing each PID term into the three-stage
pipeline let us evaluate each idea **in isolation** — something the
conflated PID design fundamentally cannot do. Three candidates were
extracted; only one survived rigorous re-evaluation.

### 1. Integral Term → AcceleratingPartialRetarget *(transferred)*

PID's integral term accelerates correction when error persists in one
direction. Our `PartialRetarget(η=0.2)` always moves exactly 20% of the
gap regardless of history. The new `AcceleratingPartialRetarget` captures
this insight: η ramps from 0.2 → 0.4 → 0.6 on consecutive same-direction
fires.

**Parameter sweep results** (500 trials, 5 SPM × 10 scenarios):
- `acceleration=0.2, eta_max=0.6` is optimal
- Convergence improved 9-40% across SPM=6-30
- Jitter: zero cost (identical to baseline)

> **Superseded:** the champion uses `acceleration=0.05`, not 0.2
> (`AcceleratingPartialRetarget::new(0.2, 0.6, 0.05)`, `information-floor.md` §6.1/§9).
> This sweep optimized acceleration on *convergence alone*; the ckpool investigation
> found fast acceleration **overshoots at cold-start** (ramps η while the estimator is
> still noisy), and the slow-decline gate test found acceleration **gate-immaterial**
> — so it is a convergence lever on the *safe* axis, not a safety lever, and the
> champion backed off to gentle 0.05 to avoid the cold-start overshoot a
> convergence-only sweep can't see. "Jitter: zero cost" is the convergence-only view;
> the full picture includes the cold-start cost. (η_base=0.2 and η_max=0.6 are
> unchanged — only the *acceleration* param moved 0.2→0.05.)

This idea transferred cleanly because it addresses a concern within a
single stage (update-rule magnitude over time). No cross-stage calibration
is involved.

### 2. Operating in SPM-Space → SpmRatioEstimator *(discarded)*

PID operates on `realized_spm` directly without converting through
hashrate/target. Our `SpmRatioEstimator` did the same: EWMA smoothing
on the raw SPM signal, then `h_estimate = current_h × (realized/expected)`.

**Initial result**: Behaviourally indistinguishable from `EwmaEstimator`
on the head-to-head benchmark — the supposed benefit was code
simplification.

**Re-evaluation**: Further scenarios exposed regressions that the
paired-simulation harness missed. The component is retained in the
codebase as an experimental alternative but is **not** part of any
production composition.

### 3. Sub-threshold Persistence → SignPersistenceCusumBoundary *(discarded)*

In PID, errors below the dead zone still accumulate in the integral. Our
`SignPersistenceCusumBoundary` adapted this: when deviation sign persists
across ticks, the threshold decreases slightly.

**Initial result**: +6% detection rate on ±10% steps at the cost of +23%
jitter on stable load.

**Re-evaluation**: The jitter penalty outweighed the detection gain across
the full grid; tuning attempts could not move the Pareto frontier. The
component is retained for reference but is **not** part of any production
composition.

> **SUPERSEDED (see banner) — the component SHIPS; it was not discarded.** This
> rejection of `SignPersistenceCusumBoundary` as a **standalone, all-rates** boundary
> is **correct**: standalone it is genuinely too jittery — because it **collapses at
> low SPM** (`information-floor.md` §6.1). The +23% jitter measured here is that
> sparse-rate collapse. But the arc did **not** discard the component — it wrapped it
> in `AdaptiveBoundary`'s regime-switch (`AdaptiveSignPersist`: PoissonCI guards
> **below** spm6, this boundary runs **above**), and it is the **champion's
> DENSE-RATE aggressive arm** — item 1's **dense-rate** dangerous-direction protector
> (the `tighten_multiplier`, which §6.1 establishes is **switched OFF at sparse rate**,
> where the *estimator* carries the protection instead — NOT an all-rates protector).
> So: rejected for the role tested (**standalone, all-rates**), kept for the role the
> champion uses (**dense-rate** arm of a regime-split). Same struct — one definition
> (`boundary.rs:460`), and `AdaptiveSignPersist` wraps exactly it — two roles.
> "Discarded" is stale and reader-misleading: the champion ships it.

### Meta-observation

Decomposition into three stages is what *made the failure modes visible*.
Two of three extracted ideas looked promising in narrow tests and were
ultimately rejected only because each could be exercised on its own
boundary, estimator, or update axis without confounding the others. The
PID's monolithic structure offers no such diagnosis — its dead zone, gain
schedule, and integral windup all interact, so a failing parameter sweep
gives no actionable signal about which concern is broken.

## Proposed Composition (not adopted)

A speculative "BestOfBest" composition combined all three extracted ideas:

```rust
Composed::new(
    SpmRatioEstimator::new(120),
    AsymmetricCusumBoundary::new(1.5, 0.05, 3.0),
    AcceleratingPartialRetarget::new(0.2, 0.6, 0.2),
    min_allowed_hashrate,
    clock,
)
```

Head-to-head (1000 trials, 8 SPM × 10 scenarios) showed a tradeoff —
~2 min slower cold start for 2-3× better steady-state accuracy — that
initially looked favourable. However, once `SpmRatioEstimator` and
`SignPersistenceCusumBoundary` were independently rejected (see above),
this composition was abandoned. The production recommendation lives in
[`CKPOOL_INVESTIGATION.md`](./CKPOOL_INVESTIGATION.md): `EwmaEstimator` +
`PoissonCI` + `PartialRetarget(η=0.2)`. `AcceleratingPartialRetarget`
remains available for ad-hoc composition where extra cold-start
aggression is desired.

> **STALE (doubly).** (a) That referenced `CKPOOL_INVESTIGATION.md` rec
> (`EwmaEstimator(120)` + ...) is **itself superseded** — EWMA(120) is
> decline-gate-UNSAFE (slow-decline gate: +5.9% settled at the 2-spm cell, over the
> 5% gate; see that doc's gate-test addendum). (b) It says production uses plain
> `PartialRetarget(η=0.2)` with `AcceleratingPartialRetarget` "ad-hoc" — but the
> **champion's production update rule IS `AcceleratingPartialRetarget`** (the very
> component §1 says "transferred"). Current production:
> `Ewma360` + `AdaptiveSignPersist` + `AcceleratingPartialRetarget(0.2,0.6,0.05)`
> (`information-floor.md` §6.1/§9).

## Files Added

### Production components (`src/vardiff/`)

- `composed/update.rs` — `AcceleratingPartialRetarget` (new UpdateRule) — the
  single PID-derived idea that survived isolated re-evaluation

### Experimental / reference components (`src/vardiff/`)

- `pow2_pid.rs` — Reference Pow2-PID implementation for simulation
- `pid_tuned.rs` — Well-tuned PID implementation (P+I+D active)
- `composed/estimator.rs` — `SpmRatioEstimator` — discarded after
  re-evaluation; kept for reference
- `composed/boundary.rs` — `SignPersistenceCusumBoundary` — discarded after
  re-evaluation; kept for reference

### Simulation binaries (`sim/src/bin/`)

- `compare-pid.rs` — Pow2-PID and tuned-PID vs all algorithms
- `compare-best.rs` — BestOfBest vs production (final comparison)
- `convergence-time.rs` — Convergence time measurement
- `sweep-accelerating.rs` — Parameter sweep for AcceleratingPartialRetarget

### Grid registrations (`sim/src/grid.rs`)

- `AlgorithmSpec::pow2_pid(spm, hashrate)` / `pow2_pid_default()`
- `AlgorithmSpec::pid_balanced(spm)` / `pid_aggressive(spm)` / `pid_conservative(spm)`
- `AlgorithmSpec::ada_cusum_accelerating(tau, s, f, t, eta_base, eta_max, acc)`
- `AlgorithmSpec::spm_ratio_cusum(tau, s, f, t, eta)`
- `AlgorithmSpec::ewma_sign_persistence(tau, s, f, t, sd, md, eta)`
- `AlgorithmSpec::best_of_best()`
