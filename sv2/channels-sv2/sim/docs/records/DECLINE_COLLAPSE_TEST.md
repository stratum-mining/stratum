# Decline-safety: does one dimensionless group govern the floor? — specification

**Purpose.** Adjudicate, against the live harness, the one claim in the
"information phase transition" note that is *not* already settled in
`information-floor.md` / `THEORY.md`: that the decline behavior of the controller
field organizes around a single dimensionless ratio. Two of the note's three
load-bearing pieces are already closed (§0); this spec isolates the third — the
*sustained-decline* floor — frames it as the experiment it actually is, and pins
pass/fail to a **predicted curve** (self-consistent, convex), not an eyeball fit.

The headline correction over the note (and over an earlier draft of this spec):
the collapse onto a single group is **real but arm-dependent**, and the
interesting result is not whether the field collapses but **which group each
arm-class obeys**, because that ordering *is* the floor-vs-gap decomposition.

## 0. What is already settled — do not re-run this ground

The note frames two hypotheses (control-instability vs information-threshold) as
competitors and offers a similarity collapse + estimator-irrelevance as the
discriminators. Three of those pieces are already in the tree:

- **The information floor is derived, not analogized.** Theorem 2
  (`information-floor.md` §3): `Var(ê) ≥ 1/(r*τ)`, Cramér–Rao, for any *unbiased*
  estimator. The *dynamic* (declining-load) version is also derived:
  `L(τ_eff) ≈ ρ·τ_eff + z/√(r*·τ_eff)`, floored at
  `τ_eff ∝ (z/ρ)^{2/3}·r*^{−1/3}`. And the detection-time floor
  `T_d(δ) ≈ 2·log(1/α)/(r*·δ²)` (`THEORY.md` eq. 2). The note's "information
  threshold" *is* these results; it adds no derivation and converges on the
  terminology we already use ("information floor," not "phase transition").
- **The strong collapse was pre-registered and REFUTED.** `THEORY.md` §3a/§8.4
  proposed exactly the note's unification — magnitude cancels
  (`δ²·T_d = 2·log(1/α)/r*`), cost reduces to closeness-to-floor, controller
  identity drops out. §5.8 killed it: post-step regret rises **`∝ δ²`, not flat**;
  the field runs *far inside* the floor (it is "real but diagnostic, not a
  binding budget"); and `information-floor.md` §8.3's scope-correction shows
  settled-`e` **sign flips across boundary TYPE at fixed `(τ, r*)`** (−13% under
  SignPersistenceCusum vs +1.8% under AdaptivePoissonCusum at τ=120, spm6–8).
  Identity does **not** wash out. "Three things refused to be unified"
  (`THEORY.md` §10).
- **Detection is window-limited, not evidence-limited — the note assumes the
  opposite.** The note's `R = detection-time / disturbance-time` presumes
  *evidence-limited* detection (`T_d ∝ 1/δ²`, faster for bigger drops). §5.8
  measured the field to be **window-limited** (`T_d ≈ const` in `δ`) — which is
  *precisely why* the §3a collapse failed ("the data's `regret ∝ δ²` is the
  signature of window-limited detection"). **But read the scope of that result
  exactly: §5.8 measured *steps* on *fixed-window* controllers.** A sustained
  ramp is a different object (§1 below), and "window-limited" is a property of
  the fixed-window field, not a law about every estimator. This is the seam the
  open question lives in.

So the honest residue of the note is **one** open question, with two halves that
must be built separately because one is foregone and one is not.

## 1. The open question (STEP-refuted ≠ DECLINE-refuted; and L* is a *bias–variance* floor)

§3a/§8.4 were refuted on *step* and *aged-drop* scenarios. The regime the safety
gate actually binds on — and the one still sim-only for the present champion
(§9.4) — is the **sustained decline**, governed by the *dynamic* floor
`L(τ_eff)`, not the step detection-time `T_d`. Two refinements over the note, both
correcting a latent overclaim that is also in the source:

**(a) `L*` is a bias–variance floor, not an information floor — only half of it
is CRB-protected.** Write `L(τ) = ρ·τ + z/√(r*·τ)`. The second term is the
Cramér–Rao information floor (Theorem 2). The first term `ρ·τ` is a
**deterministic lag bias** — the estimator's window trailing a moving truth — and
it is **removable**: `information-floor.md` §10 falsifier (b) already hedges that
"biased estimators routinely beat the CRB on variance." So the statement "no
algorithm beats `L*` on a decline" — as written in the note's §1 *and* latent in
`information-floor.md` §3 — is **scoped to uniform-window (level) estimators**. An estimator that
*models the slope* (Holt / Kalman-velocity / a matched detector) subtracts the
`ρ·τ` bias and can sit **below** `L*`. The right name for `L*` is `THEORY.md`
§6's: a **bias–variance detectability floor**, not an unbeatable information
floor. This is what makes Experiment B (§3) a *live* question rather than a
foregone re-confirmation.

**(b) Minimizing `L` gives a single group with exponent 1/3 — on the
*envelope*.** Setting `dL/dτ = 0`:

```
        τ*  ∝  (z/ρ)^{2/3}·r*^{−1/3},        L*  ∝  (ρ/r*)^{1/3}.        (★)
```

Both terms of `L` collapse to `(ρ/r*)^{1/3}` *at* `τ*`. `ρ/r*` is dimensionless
once shares are a pure count: the **fractional decline accrued per share
arrival** — how fast the miner fades per unit of evidence. **But (★) is the cost
of the arm that sits *on* the floor (per-cell-optimal τ). A *fixed*-window arm
does not obey it** — and that is the crux of the redesign in §2.

**(c) The `e^{−e}` starvation correction makes (★) convex, not broken.** (★) used
the `e≈0` Fisher info `λ = r*τ`, but a decline is exactly where `e` is not small:
the realized rate is `λ = r*τ·e^{−e}` (`THEORY.md` §5.2 — the death-spiral living
inside the floor), and the lag *is* `e* ≈ ρ·τ*`. So the self-consistent floor
suppresses `r*` by the very lag it measures. The key point: **on the envelope
`e*` is itself leading-order `∝ (ρ/r*)^{1/3}`**, so the corrected cost is

```
        L*_corrected  ∝  (ρ/r*)^{1/3} · exp( c·(ρ/r*)^{1/3} ).            (★★)
```

This is **still a one-group function of `ρ/r*`** — the collapse does not fail. It
is merely **convex on log–log** (an upturn) rather than a straight slope-1/3 line.
The exact curve needs the self-consistent τ-solve and should be computed
numerically, not asserted in closed form. Consequence for the test: the failure
mode to guard against is **fitting a power law to a convex curve** and reading the
upturn — which coincides with the gate-binding sub-guard cells — as a *collapse
breakdown* or *controller identity*. It is neither; it is the spiral term, and it
is a sharp, free prediction (§2, criterion 2).

**Hypotheses, as a decomposition not a contest.** The note's two hypotheses are
two *components* of the decline failure budget:

- a **floor** component — the irreducible `L*` of (★)/(★★), the bias–variance
  detectability floor (the note's "information threshold," real but beatable by a
  slope-modeling estimator per (a));
- a **gap** component — everything a *realized fixed-window* controller sits
  *above* `L*`: the unspent-information gap (§2), boundary type, sign-persistence,
  the EWMA Jensen bias (`SLOW_DECLINE_TEST.md` §6 sub-guard, +5%), the τ choice.

The question is **how much of the decline budget is floor vs gap, where on the
`(ρ, r*)` plane, and whether a slope-aware arm can push *below* the floor.**

## 2. Experiment A — the three-arm collapse test (which group does each arm obey?)

The note (and the earlier spec draft) framed A as "does the field collapse onto
`ρ/r*`?" with three *candidate groups* and a fixed-arm field. That is the wrong
shape, because **the fixed arms collapse — just not onto `ρ/r*`.** For a fixed
window the floor-relative excess is, algebraically,

```
        (L − floor)/floor  =  (ρ/z)·√r*·τ^{3/2}                          (fixed τ)
```

— a clean one-group law on **`ρ·√r*`**, *not* `ρ/r*`, and a gap that **grows with
`r*`**: a fixed window wastes information it does not spend, and wastes more of it
the more there is. So the experiment is not "collapse vs no-collapse"; it is
**three arm-classes, each on its own group, stacked vertically on one `ρ/r*`
axis** — and the vertical structure *is* the floor-vs-gap decomposition §1 asks
for.

**The three arm-classes (one figure, one `ρ/r*` axis).**

1. **Fixed-window (the field: champion `Ewma360`, interim, classic).** Predicted
   to lie on **`ρ·√r*`**, above the floor, gap growing with `r*`. This arm is
   §8.3's `τ*∝1/r` slide re-expressed (the fixed 360 is deliberately off the
   per-rate optimum). Its raw material **already exists**: `slow-decline.rs` emits
   `mean_e_pct` (= `regret_over` over the decline) on the ρ∈{1..40}×spm∈{2..30}
   grid; `bin/collapse` is a post-processor over it plus the normalization.
2. **Per-cell-optimal-τ EWMA (the arm that traces `L*` by construction).** An
   oracle level estimator with `τ = τ*(ρ, r*)` from (★) — i.e. a level estimator
   sitting *at* its bias–variance balance in every cell. Predicted to lie on the
   **`(ρ/r*)^{1/3}` envelope (★)**, bending to the convex **(★★)** in the
   sub-guard corner. This is the one piece of genuinely **new measurement** A
   needs: the *unconfounded `L*` depth*. `tau-family.rs` deliberately does **not**
   supply it — it reads argmin *positions* to dodge the clamp-magnitude confound
   (§8.3 RESULT) — so the depth requires its own per-cell τ-optimal depth sweep
   with the clamp confound controlled.
3. **Ramp-aware / matched (the arm that can go *below* `L*`).** Built and raced in
   Experiment B; plotted here for the vertical picture. Per §1(a) it removes the
   `ρ·τ` bias and can sit under the (★) envelope.

**Pass/fail.**

1. *Which-group, per arm (graded).* Fit each arm-class against both `ρ/r*` and
   `ρ·√r*`. **Prediction: fixed arms collapse on `ρ·√r*`; the τ*-EWMA collapses
   on `ρ/r*`.** A fixed arm that collapses on `ρ/r*` instead, or a τ*-EWMA that
   does not, falsifies the floor/gap split. (This *replaces* the note's "report
   all three groups, don't pre-commit": the algebra pre-commits each arm to a
   group, and the test is whether the arm obeys its predicted group.)
2. *Envelope exponent + convexity (hard, arm 2).* Fit the τ*-EWMA on log–log.
   **Fit the self-consistent convex (★★), not a power law.** A straight slope-1/3
   over the whole range means the `e^{−e}` term is negligible (benign regime
   only); the **convex upturn in the sparse/fast cells, coinciding with the
   gate-binding sub-guard corner, is the prediction** — the spiral in the floor.
   Do *not* read the upturn as collapse-failure or identity (the §1(c) misread).
3. *Floor-vs-gap, by vertical separation (hard).* The gap between arm 1 (fixed)
   and arm 2 (τ*-EWMA) at fixed `ρ/r*` is the **unspent-information gap**, and §1
   predicts it *grows with `r*`* (since arm 1 ∝ `ρ√r*` and arm 2 ∝ `(ρ/r*)^{1/3}`
   diverge). Color arm 1 by controller: a controller-*ordered* residual within
   the fixed arm is the §8.3 sign-flip extended to declines (identity persists);
   an unordered one supports "fixed controllers cluster." Either is publishable;
   §8.3 predicts ordered.

This is the figure §8.4 would have become had it not been refuted on steps first
— drawn now in the one regime where it survives, and as a *layered* picture
(which group each arm obeys) rather than a single-curve collapse.

## 3. Experiment B — the matched-detector race (the genuinely-open half)

**This is the half that is NOT foregone**, and §1(a) is why. The window-limited
result (§5.8, `regret ∝ δ²`) is *steps on fixed windows*. A sustained ramp has a
**removable lag bias** `ρ·τ`, and **no arm in the field is ramp-aware** —
champion, interim, and classic are all *level* estimators. A slope-modeling arm
is the dynamic form of §10 falsifier (b), untested, and the only result here that
can **reopen** sparse-estimator work rather than re-confirm §8.3.

> Build a genuine slope-aware tracker — Holt double-exponential / Kalman with a
> velocity state / a Page-CUSUM matched to a Poisson rate-decline — and race it
> against the fixed-window champion on the §2 sustained-decline grid.

This is the controlled head-to-head `THEORY.md` §5.3 sketched analytically (SPRT
discussed, never built; the champion is EWMA-window).

**Pin the response variable to ONE floor (the point-4 correction).** §3 of the
note scored "ease-fire **latency**" against `L*` in one breath — a category error:
latency is a **detection-time** object (floored by `T_d`, eq. 2), `L*` is a
**tracking-regret** object. Choose, per measurement:

- **Latency → vs `T_d`.** If the question is "how fast does it fire," measure
  ease-fire latency per cell and compare to `T_d(δ_eff) ≈ 2·log(1/α)/(r*·δ_eff²)`.
- **`regret_over` → vs `L*`, controller-vs-controller.** If the question is "does
  it track the decline cheaper" (the one that matters for the floor), the matched
  detector **must carry an actuator** (an update rule) so it is a controller, not
  a bare detector, and its `regret_over` races the champion's against the `L*`
  envelope of §2 arm 2.

**Use oracle-`ρ` for the clean ceiling.** Give the slope-aware arm the true ramp
rate. The clean read is: *if even the best-case (oracle) slope-aware arm does not
beat the champion's `regret_over` by more than the §3 noise band, the
floor-limited conclusion is airtight* — Theorem 2 hardens dynamically and the
slow window is not leaving tracking on the table. If the oracle arm *does* beat it
materially, run the estimated-`ρ` version to see how much of the win survives a
real slope estimate.

**Pass/fail (thesis-level).**

- **Floor-limited (note's thesis HOLDS, *and it is the stronger statement*):** the
  oracle slope-aware arm does **not** beat the champion's decline `regret_over` by
  more than the §3 band ⇒ the removable bias is not worth removing at these rates,
  `L*` binds even slope-aware arms, estimator R&D on declines is dead-ended by
  physics. This is *stronger* than §6.1's "the slow estimator is safe" — it would
  show no estimator is materially *faster* either.
- **Estimator-limited (the reopen):** the slope-aware arm substantially beats the
  champion ⇒ the decline regime is gap-dominated, the level-estimator window
  leaves detectability on the table, and `AsymmetricPoissonCI` / a slope-modeling
  sparse estimator are back on the table (the §10 reopen-trigger). Note this would
  reopen the champion at the margin and owe an spm≥6 re-confirm (§9.4 / §10(d)
  discipline).

Either outcome is decision-relevant; the current docs assert *safety* of the slow
estimator, not *optimality* of its latency or tracking.

## 4. What each outcome buys

| Result | Floor-dominated | Gap-dominated |
| --- | --- | --- |
| **A: which-group** | τ*-EWMA on `(ρ/r*)^{1/3}`, fixed arms on `ρ√r*` → the floor/gap split is real and the dynamic floor is publishable as Theorem 2's companion | a fixed arm collapses on `ρ/r*`, or τ*-EWMA doesn't → the algebra of §1–2 is wrong; investigate |
| **A: envelope** | τ*-EWMA fits convex (★★), upturn in the sub-guard corner → the spiral-in-the-floor is measured; publish | straight slope-1/3 everywhere → `e^{−e}` negligible in-grid; weaker but clean |
| **A: gap ordering** | unordered residual in the fixed arm → "fixed controllers cluster" extends to decline safety | ordered residual → §8.3 sign-flip extends to declines; identity persists (likely) |
| **B: detector** | oracle slope-aware arm does NOT beat champion → strongest safety claim available; estimator R&D dead-ended | slope-aware arm wins → champion's level window is suboptimal on a ramp; reopens sparse/slope estimator + boundary asymmetry |

## 5. Hardware version (shape-proxy)

**Scope tightened by the §7.1 result — the floor race does NOT need iron, only
the ring does.** Experiment B answered the tracking question in sim (floor-limited,
no reopen), and that answer rests on the *information floor*, which the hardware
cannot move — so re-running the floor race on iron confirms nothing new. The one
thing B made a live ship/no-ship question is the **direction-gate variance ring**
(§7.1): the sim says `holt-est` produces `s>0` (tightening) fires in 8 named
sub-guard cells, and the only thing iron can tell us that sim can't is whether
that ring **survives on hardware or is an artifact of the Poisson observation
model**. That is the narrow test:

> Champion vs `holt-est` side-by-side on an S21/testnet4, at **one or two of the 8
> ring cells** (the sparsest, e.g. `40%/hr × 2–4 spm`, where the fingerprint is
> strongest), driving the shape-proxy **Ramp**. Watch Grafana for `s>0`
> (difficulty-up) fires during the monotonic decline. A tighten on iron at a ring
> cell = the ring is real and any slope-aware controller owes a fix before ship; no
> tighten = the ring is a Poisson-model artifact and the risk is sim-only.

This is a cheap overnight run on specific cells, **not** the broad decline sweep
the original §5 framing (written for Experiment A's collapse overlay) implied. The
A-style two-point same-`ρ/r*` overlay — predicted to *differ* for the level
champion since it obeys `ρ√r*` not `ρ/r*` (§2) — remains valid but is
characterization, not a gate, and is deprioritized behind the ring check.

## 6. Framing note (for the white paper, if any of this lands)

Keep the note's own good advice and our existing choice: **do not call it a phase
transition.** What Experiment A characterizes is a **bias–variance detectability
floor** (§1(a) — the right name), not a critical phenomenon: there is no order
parameter, the "transition" is graded (`SLOW_DECLINE_TEST.md` shows bounded
transient lag, not a recovery cliff; even classic recovers within the window), and
§1(c) shows the dangerous corner is a *convex upturn* of one smooth curve, not a
discontinuity. One quantitative refinement worth stating *if* the envelope holds:
the **width** of the transition region should scale as `1/√(r*τ)` — a finite-count
rounding of the floor — so it narrows as `r*` rises. That is a second, independent
prediction the same grid tests for free.

**The through-line (the strongest claim the result licenses).** This is now the
**second** time the tree has answered "does a better estimator help?" with "no, and
here is the floor it cannot beat" — first **static** (Theorem 2; the ~12% flat
field across the operating band, `information-floor.md` §8), now **dynamic** (§7.1: oracle ×0.95
against the self-consistent `L*` on a decline). Two *independent regimes*, the same
structural answer, is a stronger claim than either alone: it is evidence that the
**information floor is the governing constraint across the operating envelope**, not
an artifact of one scenario class. That is the opposite of the note's "build a
magical estimator" instinct — now refuted in **both** forms by measurement rather
than argument. State it as the spine if any of this reaches the white paper.

## 7. Build order and results (`bin/collapse`, `bin/matched-detector`)

**Build order — B over A** (inverting the earlier lean):

- **A is characterization only.** The fixed-arm collapse is §8.3 re-expressed
  (`ρ√r*`, not new), and the one new number — the unconfounded `L*` depth (arm 2)
  — does not reopen the champion: the rate-aware closure (§8.3 Net status) already
  settled that the window has *no admissibility content*. Build A only if you want
  the dynamic floor as the paper's companion to Theorem 2. It is a post-processor
  over `slow-decline.rs` / `tau-family.rs` plus one per-cell-optimal-τ depth
  sweep (clamp confound controlled).
- **B is the live question.** The window-limited result is steps-on-fixed-windows;
  a ramp has a removable bias and the field has no slope-aware arm. B is the only
  experiment here that can *reopen* rather than re-confirm. If one thing is built,
  build B, scoped as in §3 (one floor per response variable; actuator on the
  detector; oracle-ρ for the ceiling).

### 7.1 Experiment B — RESULT (`bin/matched-detector`, 200 trials/cell, sim)

**Verdict: FLOOR-LIMITED — the decline regime is not gap-dominated, and a
slope-aware estimator does *not* reopen sparse-estimator work.** The strong form
of the §3 "information-limited" outcome holds, and it is a *stronger* statement
than the docs currently make.

**Build.** `decline_floor.rs` (the measured `L*` + the (★★) convex cross-check +
the oracle decline slope); `holt.rs` (the slope-aware arm — champion EWMA level +
a *decoupled* multiplicative de-lag `ĥ = h_level·exp(b·L_eff)`, so `b=0` is
bit-identical to the champion — pin 4); `bin/matched-detector.rs` (the race + the
open-loop calibration gate + the direction-gate 2×2). All arms share the champion
boundary/update/clamp/tick; only the estimator differs.

**Primary (regret_over vs the measured bias–variance floor).** Each arm swept over
its own τ-ladder and scored at its own-best window (the floor is `min_τ` of a
level-only EWMA; `L* = min_τ(lag+noise)` is per-arm). In the 13 *substantial*
cells (floor ≥ 1% over-difficulty — the sub-guard/fast corner where the floor
question lives; the rest are ≈0-floor where the ratio is noise):

```
  median arm/floor ratio:  oracle ×0.95,  estimated ×1.04
```

The **oracle** slope-aware arm — handed the true ρ, level still estimated — beats
the level floor by only ~5%, and the **estimated** arm (must learn the slope from
Poisson data) sits *at or above* the floor. So even a best-case slope handed for
free does not materially beat the uniform-window floor on a decline: the removable
`ρτ` bias is real (§1(a)) but at these rates it is **not worth removing** — the
slow window is not leaving tracking on the table. This is stronger than §6.1's
"the slow estimator is *safe*": it is "no estimator is materially *faster*."

**Calibration gate (open-loop, pin 2) — PASSED.** Driving the estimator alone on
the known ramp (belief fixed, exact dithered counts, no boundary/actuator), the
oracle de-lag removes ≥70% of the `+ρτ` lag with ≤0.5% residual across the grid,
**once the de-lag horizon is the DISCRETE-EWMA mean lag `α/(1−α)` ticks, not `τ`**
(the continuous-`τ` value over-corrected ~9%; the gate caught it at high rate and
it was re-spec'd per pin 2, not nudged). The two corners that don't fully null are
named physics the linear de-lag *structurally cannot* remove — the `e^{−e}` convex
floor (★★) at 40%/sparse, and the sparse Jensen *offset* (not lag) at 1–2%/2spm —
exactly where the spec predicts. So the ×0.95 is a **real de-bias, not a horizon
artifact**.

**Direction gate (pin 5) — the genuine new risk, located.** The 2×2, closed
against a level-EWMA-at-the-oracle's-own-window control (`lvl@orcτ`):
- The **de-lag never tightens more than `lvl@orcτ`** — where the oracle tightens,
  it is the *long window* mis-reading sparse data (the champion's own documented
  sparse-noise tightens, `SLOW_DECLINE_TEST.md` §6), **not** a de-lag fault.
- The **estimated arm tightens more than the oracle in 8 sub-guard cells** =
  *trend-estimator variance*. Fingerprint confirmed: `frac(b̂>0) ≈ 1.0`
  co-located with low `r_obs/r*` (down to 0.85) and rising `e%` — a
  starvation-driven ring (pin 1's two-channel signature). **This is the new
  direction risk slope-awareness introduces, and the thing hardware would have to
  clear before any slope-aware controller could ship.**

**Net.** B answers its own question: the decline regime is **floor-limited, not
estimator-limited**. A slope-aware tracker buys ~5% (oracle, free slope) to ~0%
(estimated) on tracking, at the cost of a new sparse-rate tightening risk. The §10
reopen-trigger does **not** fire; the slow level estimator stands as not just safe
but latency-optimal-to-within-the-band on a decline. (`bin/matched-detector`,
`VARDIFF_MD_TRIALS=200`; sim only — HW unrun.)

**Why a Kalman-velocity follow-up is *not* live.** Holt was the right probe
precisely because its decoupled trend leg *is* the `ρτ` term in isolation — so
"de-lagging the lag doesn't help" is as clean a test of "is the lag the binding
cost" as the design space allows. A variance-optimal Kalman-velocity arm would be
answering a question §7.1 has shown isn't open. It re-opens **only if the rate
regime moves**: at higher `r*` the variance floor `1/√(r*τ)` recedes and the bias
term's *relative* weight grows — the one corner where the §8.4 lever and this
result interact. Until then, no slope-aware follow-up is warranted.

**Method note — the §6.1 trap, hit 3× and escaped 3× (the transferable part).**
Three times during this build a mechanism was *plausible from the algebra and wrong
in the dynamics*, the exact failure `information-floor.md` §6.1 documents: (1) an early claim that
the share-maturing horizon `L_eff/(r_obs/r*)` would over-correct in the dangerous
corner — inverted on the sign check (`e>0` ⇒ `r_obs < r*`, shares *slow*, lag
*grows*, a safe-side under-correction, not the predicted tighten); (2) the
"oracle's fixed `b<0` cannot tighten" 2×2 premise — false, a long de-lagged window
over-corrects deterministically, caught only by adding the `lvl@orcτ` control; (3)
the de-lag horizon `L_eff = τ` — wrong, it is the *discrete*-EWMA mean lag
`α/(1−α)` ticks, a 9% error the **closed-loop gate structurally could not surface**
(it took feeding the estimator a known ramp open-loop, pin 2 doing its actual job:
catching a misspecification, not confirming a belief). The escape was identical
each time: **measure the channel rather than reason about it.** The
`α/(1−α)`-not-`τ` fact is general to de-lagging any time-EWMA on this tree, not
specific to Holt — carry it forward. The variance-ring fingerprint's power is the
same discipline in positive form: either channel alone (`b̂>0`, or low `r_obs/r*`)
is ambiguous; their *co-location* is what makes "starvation-driven slope-estimation
noise" unambiguous rather than a plausible story.

### 7.2 Experiment A — not built (characterization only)

Per the build lean above, A was not built: the fixed-arm collapse is §8.3
re-expressed and reopens nothing. The one number A would have added — the
unconfounded `L*` depth — was produced as a *by-product* of B (the measured floor
column, `bin/matched-detector`), and the (★★) convex shape is confirmed as the
cross-check in `decline_floor` (the floor depth bends up super-linearly in `ρ/r*`,
near-collapsing the two equal-group cells). The HW two-point same-`ρ/r*` overlay
(§5) remains unrun.
