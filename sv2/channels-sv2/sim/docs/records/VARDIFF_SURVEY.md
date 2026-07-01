# Vardiff archetype survey — what's actually deployed in open source

**Question.** Our characterized set (classic, ratio-deadband [current MARA production], pow2-PID
[DMND], multi-window [ckpool], EWMA-asymmetric [champion]) derives **two failure
archetypes** — sleepy-symmetric (fails in the deep-and-fast corner) and
structural-deadband (fails on the depth half-plane). Is that a complete taxonomy
of *deployed* controllers? **No.** This survey reads the source of the most
widely-deployed open-source vardiffs to find archetypes our sample missed.

**Discipline (the load-bearing part).** "good/bad" is a binary *partition by
decline-safety*, complete by construction. The *failure mechanisms* are a
found-set, NOT exhaustive. And the config fingerprint (`targetTime` /
`variancePercent` / `retargetTime`) does **not** determine decline-safety — the
starvation-easing detail does, and that is a **source read, not a config read or a
memory call**. Every verdict below is anchored to a file read this turn; where a
body was not read, it is marked UNVERIFIED.

Sources pulled 2026-06-30 (raw GitHub, `master`):
- `zone117x/node-stratum-pool` `lib/varDiff.js` (126 lines, read verbatim)
- `oliverw/miningcore` `src/Miningcore/VarDiff/VarDiffManager.cs` (read verbatim)
  + `src/Miningcore/Mining/PoolBase.cs` (call-site, read verbatim)
- `benjamin-wilson/public-pool` `src/models/StratumV1ClientStatistics.ts` (read verbatim)

---

## NEW ARCHETYPE — the NOMP "target-share-time" family (share-gated, no idle path)

**Ubiquity.** `node-stratum-pool` (NOMP) and its forks (s-nomp, z-nomp, UNOMP,
energi, the equihash/PHI variants) are the default vardiff for essentially the
entire altcoin / small-BTC open-source pool world outside ckpool and SRI. This is
the single most-copied vardiff in open source.

**Mechanism (distinct from all five we had).** Control variable is share **time**,
not rate or ratio. Estimator is a **RingBuffer of recent inter-share gaps**
(boxcar). Trigger is a **symmetric ±`variancePercent` deadband on time** (typ.
±30%). Retarget is **clock-gated** by `retargetTime`.

**Decline-safety verdict — source-confirmed STRUCTURALLY UNSAFE at the disconnect
boundary.** The *entire* retarget logic lives inside `client.on('submit', …)`.
Verbatim, there is **no timer and no idle path** — difficulty only ever changes
when a share arrives:

```js
client.on('submit', function(){
    var ts = (Date.now() / 1000) | 0;
    ...
    var sinceLast = ts - lastTs;
    timeBuffer.append(sinceLast);
    ...
    if ((ts - lastRtc) < options.retargetTime && timeBuffer.size() > 0) return;
    ...
    if (avg > tMax && client.difficulty > options.minDiff) { ...ease... }
});
```

Two regimes, kept distinct (avoid the value-vs-domain trap):
- **Gradual decline (shares still trickle):** it DOES ease — `avg > tMax` →
  `ddiff = targetTime/avg < 1`. The ±30%-time band is *tighter* than current MARA production's
  ±60% rate band, so it crosses easing readily. Lag comes from (a) the clock-gated
  `retargetTime` and (b) the boxcar `avg()` holding stale pre-decline gaps. So:
  laggy-but-eases, NOT the deadband's can't-register-it failure.
- **Sharp starvation (miner goes quiet):** because retarget is *entirely*
  share-gated with no idle path, a miner whose inter-share time grows past the
  pool's liveness timeout is **dropped before it can ever trigger an ease** — no
  share → no handler call → no rescue. Death-spiral-as-information-loop,
  architecturally guaranteed at the disconnect boundary.

**This maps onto the phase portrait as a THIRD failure locus:** not the sleepy
corner, not the deadband depth-half-plane, but a **disconnect-boundary** failure
(share-gated, no idle rescue). Worth characterizing through the gate.

---

## SAME family, OPPOSITE safety — MiningCore (the config fingerprint does NOT imply the verdict)

MiningCore (C#, high-performance, drives many altcoin pools) carries the
**identical** `targetTime`/`variancePercent`/`retargetTime` config — so by
fingerprint it looks like "NOMP family, just reach." **It is not, on the axis that
matters.** `VarDiffManager` has a second method NOMP lacks — `IdleUpdate` — whose
body eases difficulty WITHOUT a new share (verbatim):

```csharp
public static double? IdleUpdate(...) {
    ...
    if(ctx.LastTs.HasValue) timeDelta = ts - ctx.LastTs.Value;
    ...
    if(timeDelta < options.RetargetTime) return null;   // only after a quiet gap
    ...
    var avg = timeTotal / ((ctx.TimeBuffer?.Size ?? 0) + 1);
    var newDiff = difficulty * options.TargetTime / avg; // growing gap → diff DOWN
    ...
}
```

And it is **live, not dead code** — `PoolBase.cs` wires it to a timer:

```csharp
Observable.Timer(...)                       // background clock
... UpdateVarDiffAsync(connection, bool idle, ...)
    idle ? VarDiffManager.IdleUpdate(...) : VarDiffManager.Update(...);
```

**So two controllers with the IDENTICAL config fingerprint land on OPPOSITE sides
of the decline-safety axis:** NOMP cannot ease while starved; MiningCore is
explicitly built to (timer-driven idle ease). This is the single cleanest evidence
for the survey's discipline point — **config does not determine safety; the
starvation path does, and only the source shows it.** (Comment in MiningCore's own
code: "Always calculate the time until now even there is no share submitted.")

---

## DISTINCT controller — public-pool (Bitaxe / home-miner default)

Not the "same simple custom vardiff" the prior hypothesis guessed — read verbatim,
it's its own mechanism: targets **submissions-per-second**
(`difficultyPerSecond * TARGET_SUBMISSION_PER_SECOND`), quantizes to **powers of
two** (`nearestPowerOfTwo`, like DMND), `2×`/`÷2` hysteresis. **Has an explicit
starvation-ease path** (unlike DMND's deadband):

```ts
public getSuggestedDifficulty(clientDifficulty) {
    if (this.submissionCache.length < 5) {
        if ((Date.now() - submissionCacheStart)/1000 > 60)
            return this.nearestPowerOfTwo(clientDifficulty / 6); // quiet 60s → ease
        else return null;
    }
    ...
}
```

So: a pow2 controller (DMND-like quantization) but WITH idle-ease (DMND-unlike).
A different safety profile again. NOTE: the prior turn's "allows diff below 1 →
flooding" claim is **NOT visible in this file** (there is a `MIN_DIFF = 0.00001`
clamp); do not repeat that claim without the source that shows it.

---

## Coverage statement (characterized, NOT exhaustive)

The deployed open-source field consolidates to roughly:
1. **classic** — naive proportional/boxcar (textbook ancestor, the conceptual baseline)
2. **ratio-deadband** (current MARA production) + **pow2-PID** (DMND) — the deadband lineage
3. **multi-window** (ckpool)
4. **target-share-time** (NOMP family) — NEW here; share-gated, disconnect-boundary unsafe
5. **target-share-time + idle timer** (MiningCore) — same config, idle-ease SAFE
6. **pow2 + sub/sec + idle-ease** (public-pool) — distinct custom
7. **EWMA + asymmetric sequential test** (champion)

No evidence of a further distinct *popular* mechanism was found. The
proportional-continuous and pure-time-scheduled designs hypothesized last turn
show up *inside* these families (NOMP is the time-scheduled one; proportional step
is a component), not as standalone popular controllers.

**Essays/report framing implication.** Use **classic** as the conceptual baseline
(origin of the deadband flaw, nobody's product). Use **NOMP** as the neutral,
named, in-the-wild comparison (ubiquitous, community project, implicates no
operator) — it carries the "this is what's actually deployed" weight without the
us-vs-DMND optics. DMND recedes to an anonymized deployed instance of the deadband
archetype, cited only where a real-SV2-pool existence proof is load-bearing
(proof-tier, naming sanctioned in the report per the earlier call). The archetype
set is presented as **characterized, not exhaustive** — now better supported: we
looked, found the dominant missing family (NOMP) AND a config-identical-but-safer
cousin (MiningCore), and can name the boundary honestly.

**Open / not-yet-done (honest residue):**
- NOMP not yet run through OUR decline gate / phase-portrait instrument — verdict
  above is from source architecture, not from our sim.
- MiningCore's idle ease *exists and is wired*; its decline-safety *quality* (does
  the timer cadence + boxcar lag clear our 5% gate?) is unmeasured.
- public-pool's full retarget path read only via `getSuggestedDifficulty` +
  `addShares`; the caller cadence not traced.

**Build decision — REVISED by the share-driven-family correction above.** The
starvation-drop boundary I'd have built a NOMP arm to locate is a **family
boundary, and ckpool already occupies it — and ckpool is ALREADY in the grid**
(`AlgorithmSpec::ckpool()`). So the question "where does the share-driven family's
starvation-drop boundary fall on the `(depth, rate)` plane — modal cloud or sharp
tail?" is answerable with the **existing ckpool arm**, no new NOMP implementation
needed. A separate NOMP build is therefore **not warranted to locate that
boundary** — it would re-derive a boundary the ckpool arm already gives.

What a NOMP arm would add that ckpool's does NOT is the **boxcar-vs-decaying-EMA
gradual-decline lag** difference (NOMP lags worse): a smaller, separable question,
and still illustrate-not-reveal for the *starvation* failure (source-proven). So:
- The starvation existence+mechanism: **source-proven, no sim** (phase-portrait
  situation — closed-form is the content).
- The starvation boundary *location*: **answerable now with the ckpool arm** — run
  ckpool through the decline gate / phase-portrait and read where its share-driven
  boundary sits relative to the modal cloud, IF that quantitative answer is wanted.
- A NOMP-specific arm: only worth it to quantify the *boxcar lag* on gradual
  declines, which is a refinement, not the headline. Lowest priority.

Build to answer a question the source can't; the source already answers existence,
mechanism, and (via the shared family + existing ckpool arm) the boundary's
character. The genuinely-open quantitative item is "ckpool/share-driven boundary
location on the plane," and it needs no new controller code.

**RESOLVED — confirm, not reveal; no run needed.** Settled the boundary-location
question from geometry already in hand (the phase-portrait disconnect surfaces),
plus one premise-check:

1. *No existing grid arm faithfully models the family's defining property.* The
   trial harness calls `try_vardiff` EVERY tick regardless of share flow
   (`trial.rs`), so the existing `ckpool()` arm effectively has an idle path (the
   tick IS its timer). So "the ckpool arm already locates the *starvation* boundary"
   was itself slightly off — locating it faithfully (an arm that refuses to act on
   zero-share ticks) would be NEW work, not existing code.
2. *But the location conclusion is controller-independent, from the disconnect
   surface.* Already-computed troughs `min_t(r_obs/r*)` over `(depth × rate)`: the
   modal cloud (2–5%/hr, 8–20% depth) sits at **0.92–0.98**, nowhere near θ=0.36;
   disconnect (trough < θ) appears ONLY in the fast-deep corner (rate ≥20). The
   starvation drop needs share flow to crater below liveness BEFORE a retarget —
   a sharp-rate phenomenon. A share-gated controller can only be *worse* at a given
   cell, but cannot manufacture a crater where a gentle modal decline produces none
   (hours-long decline, mild over-difficulty, ample share arrival to ease).

So the share-driven family's starvation boundary sits in the **fast tail, away from
the modal cloud** — confirming the rate-argument prior. A faithful share-gated gate
run would CONFIRM this, not reveal it (phase-portrait situation: the result is
already implied by the geometry). **Verdict: not worth building.** The family's
common-case modal exposure is the *gradual lag* (boxcar worse than decaying-EMA),
NOT the starvation drop (fast-tail only). Survey complete; open quantitative item
closed as confirm-not-reveal.

**GATE RUN — faithful NOMP arm built and run (2026-06-30), on request despite the
confirm-not-reveal verdict.** It required BUILDING a faithful share-gated arm first
(no existing grid arm models "no idle path" — the harness ticks `try_vardiff` every
interval, so `ckpool()` effectively has an idle timer). Added `NompVarDiff` to
`bin/phase-portrait.rs`: verbatim port of NOMP `varDiff.js` (boxcar ring buffer of
inter-share gaps, clock-gated `retargetTime`, symmetric ±`variancePercent` time-band,
full retarget η=1) with the defining property — on a zero-share tick the submit
branch never runs → no retarget, no ease. That zero-share `return` IS the "no idle
path." (Harness adaptation: bulk-add of `n` shares/tick reconstructed as `n` appends
of gap `dt/n`, same per-share reconstruction as the ckpool port.)

- **HEADLINE — CONFIRMED (predicted illustrate-not-reveal):** No disconnect cell
  anywhere at θ=0.36 — every trough ≥0.62. Modal cloud (rate 2–5, depth 8–20%) sits
  ~0.64–0.66, far above the floor. Starvation-drop boundary is **beyond rate 40%/hr
  (sharp tail), NOT over the modal cloud** — the rate-argument prior. Settled-`e` is
  **negative everywhere** (NOMP recovers via shares; passes the over-difficulty gate,
  unlike the deadband). The run confirmed the geometry, as predicted.
- **SURPRISE (confidence-flagged, not over-claimed):** NOMP's troughs are deeper
  EVERYWHERE (0.62–0.90) than sleepy/deadband at modal cells (0.92–0.98), and deep
  even at gentle 1–2%/hr declines (0.64–0.66) where only ~4% is lost — which a 4%
  loss can't produce alone (needs a *tighten*, h_est≈1.5× h_true). Faithful mechanism,
  not a bug: full retarget (η=1) on a symmetric band with **no evidence accumulation**
  → a Poisson-lucky burst trips a tighten and jumps h_est ~30%+ in one step →
  repeated ~50% transient over-difficulty **jitter**, even on gentle declines.
  Consistent with the ckpool doc's "hysteresis fires on noise." So NOMP's real modal
  hazard is **jitter from full-retarget-without-accumulation** — distinct from the
  starvation drop (sharp tail) and from the boxcar lag; the other archetypes lack it
  (champion's CUSUM accumulates; deadband is inert).
- **Confidence:** headline ROBUST (geometry + run agree). The jitter trough is ONE
  seed/run and its low-rate magnitude rides the same 4h-cap flattening as the rest of
  the plane → reported as observed-and-mechanism-plausible, NOT a characterized
  number; jitter-characterization would be a new study, out of this run's scope. Net:
  the run confirmed the starvation-boundary prior AND surfaced one mechanism worth a
  line (full-retarget jitter) — a slightly richer return than pure confirmation, but
  the headline stands: confirm-not-reveal on the starvation boundary.

**Archetype tiling (the shape the survey produced).** The deployed-failure
archetypes tile the failure space by *locus*, not severity: **corner** (sleepy —
deep AND fast), **half-plane in depth** (deadband — rate drops out), and the
**disconnect boundary under starvation** (the share-driven family — info loop
slams shut before ease).

**CORRECTION (premise-check vs source, prompted "NOMP may be like ckpool —
share-driven").** Checked, and the starvation-boundary locus is **NOT NOMP's
alone — it is a property of the whole SHARE-DRIVEN family (NOMP + ckpool).** Read
`ckpool/src/stratifier.c`: the difficulty adjustment is computed **only on the
share-submission path**; idle detection exists but feeds connection-drop, not
vardiff (no idle-timer ease). So under a sharp starvation a miner is dropped
before it can ease under *either* NOMP or ckpool — same locus. My earlier "three
*independent* loci" overclaimed; the honest structure is:

- **Starvation-boundary failure = a FAMILY property** of share-driven retarget
  (no idle path): NOMP, ckpool, classic SMA all share it. NOT a per-controller
  archetype.
- **Within that family, gradual-decline LAG differs, and oppositely to the naive
  guess:** ckpool's EMA is *time-decaying* (`fprop = 1−e^(−elapsed/interval)`), so
  a late share carries a large `elapsed` and eases HARD on arrival; NOMP's boxcar
  is *flat*, so a late gap barely moves the average until stale entries roll out —
  NOMP lags WORSE than ckpool on gradual declines despite both being share-driven.
- **MiningCore escapes the family** via a live idle timer (`IdleUpdate`,
  `PoolBase.cs`) — same config fingerprint, eases without a share. The family is
  defined by the *idle path*, not the config.

So the three *loci* are corner / depth-half-plane / starvation-boundary, but the
third is a **shared family boundary, not a third controller** — a more accurate and
still-strong structure ("not exhaustive" stays honest). This is a correction to
this doc's own prior framing, kept visible per the check-premises-against-source
discipline: the prompt "NOMP may be like ckpool" was right on the safety axis, and
reading both sources is what separated *which* axis they share (starvation locus)
from which they differ on (gradual-decline lag: boxcar vs decaying-EMA).

---

## Landscape, magnitude-provenance, and patch DIRECTION (diagnosis, not deploy advice)

**The separating axis is NOT config or sophistication** — confirmed repeatedly
this survey — it is two things: (a) can the controller ease *without a fresh share*
(the idle path), and (b) is its easing trigger *reachable* on the declines that
actually happen. Four positions:

| Position | Members (source-verified) | Failure | Patch DIRECTION |
|---|---|---|---|
| Structurally-cannot-ease (half-plane) | DMND pow2-PID; current MARA production ±60% deadband | fails on *depth alone*, gentle or sharp | narrow/remove the easing deadband so a modal decline trips it; for pow2 it's structural (finer steps or reachable gain) — **bigger, invasive** |
| Share-driven, no idle path (family) | NOMP + forks; ckpool; classic SMA | drops a *sharply*-starved miner before ease (tail, past ~40%/hr); gradual-lag in common case | **add an idle ease path** — smallest, most universal |
| Escapes via idle timer | MiningCore | none (across the axis) | none |
| Safe-by-construction | champion (EWMA + asym. seq. test) | none (hard minimax constraint) | none |

**Magnitude — measured for ONE position, inferred for the rest (do not flatten).**
- Deadband half-plane: **measured** — variance inflates as `1/(1−d)` to ~5× deep
  (`variance_inflation.svg`), source-grounded.
- Share-driven family: **NOT the same quantity and unmeasured** — common-case harm
  is gradual-*lag* (transient variance excursion sized by the lag window: NOMP
  boxcar / ckpool EMA constant), plus a *rare* tail disconnect (fast tail, so rare
  for modal declines). NOMP's transient-tighten jitter is one-seed, not characterized.
- MiningCore / champion: near-floor.
- **The one magnitude number this corpus owns is the deadband's ~5×.** Anyone who
  states "the harm number for each pool" is inventing three of four.

**Patch DIRECTION per class (mechanism, explicitly NOT "ship this"):**
- **Share-driven family → add an idle ease path.** The whole failure is "retarget
  only runs on submit," so a timer that eases after a miner has been quiet > N
  expected intervals removes it. **It is the mechanism MiningCore already has**, so
  not hypothetical — port it. CAVEAT, source-sharpened: the idle ease must be
  **one-directional (ease on silence, never tighten)**, and reading MiningCore's
  `IdleUpdate` shows *why it is safe to port* — the one-directionality is **emergent,
  not an explicit guard**: an idle gap can only *grow* the elapsed-time average, so
  `newDiff = diff·targetTime/avg` is always ≤ current on an idle call. So the advice
  is "port the **idle-ease-from-elapsed-time** mechanism (inherently ease-only)," NOT
  "add a timer that re-runs the full bidirectional retarget" — the latter would NOT
  inherit the one-directionality and would reintroduce danger upward. The
  missed-interval threshold trades responsiveness vs churn = the pool's tuning call.
  (NOMP-only secondary, lower value: boxcar → decaying-EMA cuts the gradual lag.)
- **Deadband half-plane → narrow/remove the easing deadband.** Cause is NOT a
  missing idle path (they run every share) — it's an *unreachable* trigger, so a
  timer doesn't help. current MARA production: raise the ~0.40 ease threshold toward target. DMND:
  the pow2-in-loop quantization puts the easing crossing below reachable rate →
  structural fix (finer steps or reachable gain), not one line. **This class is
  HARDER to patch than the share-driven one**, though "worse" on the axis.
- **MiningCore / champion → no decline-safety patch.** Note MiningCore's idle-ease
  *exists and is wired live*; its *quality* is unmeasured — read as "has the
  mechanism that escapes the family," NOT "verified-optimal."

**The deploy seam (same as the ideal-rate frontier).** What this survey can assert
with full confidence: the **diagnosis** (which pools fail, how, why) and the
**direction** of the fix. What it canNOT assert without each pool's own
constraints: **"you should ship this"** — decline-safety costs responsiveness /
churn / bandwidth that each pool weighs differently (the variance↔cost exchange
rate again). Mechanism is ours to state; the deploy decision is theirs.

**Honest edges (so none of this over-reads).** Safety verdicts source-grounded for
DMND, current MARA production, NOMP, ckpool, MiningCore; *inferred* (not separately read) for the
NOMP-fork sprawl — they share `varDiff.js`, so the verdict transfers, but only the
canonical body was read. Magnitude measured only for the deadband; share-driven
lag-variance unmeasured; NOMP jitter one-seed. Patch advice is **direction**, not
deployment.

**Neutrality, reinforced (ties to "not a DMND-vs-us thing").** The patch story is
*more* neutral, not less: the family fix is "port the idle path MiningCore already
has" — constructive, universal, nobody singled out, and it happens to apply to the
most-deployed pools. The deadband fix applies to **our own current production controller** as much
as DMND's. So the advice lands symmetrically — a class of fixable flaw, one fix per
class, the same fix regardless of whose pool, with our own controller named among
the patients. Contribution framing, no operator as target.

---

## SUMMARY FIGURE — two-gate decision tree (BUILT 2026-06-30; `figures/safety_tree.svg`, `safety-tree.rs`)

![Two-gate decision tree sorting every surveyed controller by decline-safety. GATE 1
(eases without a fresh share? = idle path?): NO → share-driven family (NOMP+forks,
ckpool, classic SMA), YES → GATE 2 + the escapes-via-idle-timer leaf (MiningCore).
GATE 2 (ease trigger reachable on the modal decline?): NO → deadband (DMND, current
MARA production), YES → safe-by-construction (champion). Harm is shown ONLY where
measured: the deadband leaf carries a SOLID "~5× variance" (measured); the
share-driven leaf carries NO number — a full-weight red hatch labeled "harm
magnitude: UNMEASURED · common case gradual lag (size unknown) · tail: rare but a
DROPPED CONNECTION (severe)" — equally alarming, differently known. Fix-difficulty
rides the gates: gate-1 leaf = "easy: port the idle ease-path (exists in MiningCore)",
gate-2 leaf = "hard: narrow deadband / rework pow2 (structural)". Footer: "easy/hard"
is effort-to-make-safe, NOT a deploy recommendation.](../figures/safety_tree.svg)

**Acceptance test — verdict against the rendered raster (both clauses PASS):**
- *Clause 1 (hatch not read as ~5×):* PASS — the share-driven leaf carries no number;
  ~5× appears only on the deadband leaf, nothing to borrow.
- *Clause 2 (hatch reads UNMEASURED, not SMALL — the hard one):* PASS — the hatch is
  full-weight red (not faint gray), so the leaf is as visually loud as the solid one;
  severity is carried in words ("UNMEASURED" + bold "DROPPED CONNECTION (severe)"), so
  the two leaves read *equally serious, differently known*, not mild-vs-bad.

Spec (retained below, as built to):

### SPEC — two-gate decision tree (report/records-tier)

**Chart type decided: a decision tree, NOT a radar. The radar is rejected on
principle, not taste** — it is the killed `regret_radar` failure mode again. A radar
plots commensurable axes and invites reading enclosed area as "overall goodness";
this survey's finding is the *opposite* — controllers fall into **categories** by two
binary structural properties, and the harm magnitudes are **mixed measured/unmeasured**.
A radar would seat the deadband's *measured* ~5× and the share-driven family's
*unmeasured* lag-variance on the same spoke at equal visual weight — the value-vs-domain
trap rendered as a picture. A taxonomy wants a structure diagram, where there are no
axes to compare areas on and the false-commensurability misread is structurally
impossible.

**The structure (it IS the finding — safety is two structural properties, not
sophistication):**
- **Gate 1 — eases without a fresh share? (idle path?)** NO → share-driven family
  (NOMP+forks, ckpool, classic SMA): has the starvation-drop failure. YES → escaped
  that failure (→ gate 2 for the rest).
- **Gate 2 — ease trigger reachable on the modal decline?** (only for controllers
  that run every share) NO → deadband half-plane (DMND, current MARA production):
  fails on depth alone. YES (+ idle path / evidence-weighing) → safe.
- **Four leaves:** share-driven-no-idle-path / deadband-unreachable-trigger /
  escapes-via-idle-timer (MiningCore) / safe-by-construction (champion).

**Harm encoding — the discipline must SHOW in the visual, not just the text.** Encode
magnitude ONLY where measured, and make measured-vs-unmeasured *visibly different*:
- deadband leaf → **~5× variance, SOLID** (measured, `variance_inflation.svg`). This
  is the EASY leaf to render: one clean measured number.
- share-driven leaf → magnitude unmeasured, but **this leaf carries TWO harms of
  different measured-status and must NOT be flattened into one mood:**
  - *common-case:* **gradual-lag variance — unmeasured in size, but it bites the
    modal decline** (the typical outcome).
  - *tail:* **starvation disconnect — rare for modal declines (past ~40%/hr, per the
    gate run), but a DROPPED CONNECTION when it hits — the severe outcome in the
    whole corpus.**
  So the honest leaf reads "*usually a moderate unmeasured cost, rarely a severe
  one*," which is genuinely two pieces of information. A render showing only the
  mild-sounding "lag" OR only the scary "disconnect" misrepresents the leaf in
  opposite directions. This is the HARD leaf to render, for that reason.
- escapes-via-timer + safe leaves → near-floor.
The measured/unmeasured boundary is itself a survey finding; the figure must render it.

**Fix-difficulty rides the gates (double duty — difficulty DERIVES from which gate
failed, not a pasted-on axis):**
- gate-1 leaf (share-driven) → **EASY: port the ease-from-elapsed-time idle path —
  exists in MiningCore's `IdleUpdate`, inherently ease-only.** Caveat on the figure:
  it must be that mechanism, NOT a timer re-running the full bidirectional retarget.
- gate-2 leaf (deadband) → **HARD: narrow the easing deadband (MARA: raise ease
  threshold toward target) / rework pow2-in-loop quantization (DMND) — structural,
  no drop-in sibling.**
- escapes/safe leaves → "no fix needed" — with the honesty note: MiningCore = "has
  the escaping mechanism," NOT "verified-optimal" (idle-ease *quality* unmeasured).

**Mechanism-not-deploy caveat (governs the whole figure, same seam as the ideal-rate
frontier):** "easy/hard" = *effort to make decline-safe + whether a working example
exists*, NOT *recommended action*. The idle-ease still costs responsiveness/churn
each pool weighs differently. Fix annotations must read "here's the change and how
big it is," never "here's what they should do."

**Build notes:** report/records-tier (carries the measured/unmeasured distinction and
the mechanism-not-deploy caveat a lay reader can't hold — same tier as the phase
portrait, NOT the essays). Needs NO sim — it's a structure diagram over results
already in hand. Build when the queue opens; **render-check it yourself, because the
honesty lives in the leaf-treatment actually reading as differentiated.**

**ACCEPTANCE TEST — the figure must not lie about the unmeasured leaf in EITHER
direction (both clauses required; clause 2 is the one most likely to slip):**
1. *Borrowing-the-number misread:* a reader must not read the share-driven leaf as
   carrying the deadband's measured ~5× — the hatch must not look like the solid
   magnitude on the same scale.
2. *Hatch-reads-as-small misread (the harder, more-likely one):* a reader must not
   infer the hatched leaf is **lower-harm** than the solid one — only that its
   magnitude is **unmeasured**. The natural visual grammar of hatch-vs-solid is
   tentative→less, which the eye maps to less-harm; a faint gray hatch beside an
   alarming solid red WILL read "deadband bad, share-driven mild," which is
   **backwards on the tail** (the share-driven tail is a dropped connection, the
   corpus's worst outcome). Beating this probably means the hatch can't be merely a
   fainter solid — an explicit label ("magnitude unmeasured — lag + rare disconnect")
   likely has to do the work, plus a cue that the *tail* is severe so the leaf doesn't
   collapse to "mild." (Don't prescribe the fix here; just: the hatch must read "we
   don't have the number," NOT "the number is small.")

"Passes" = does not lie in either direction. A figure that merely "looks like a nice
summary," or that only clears clause 1, is NOT done. Same shape as every catch this
arc: literally-passes-the-check, semantically-wrong, caught only by asking what a
reader *infers*, not what the figure technically asserts.
