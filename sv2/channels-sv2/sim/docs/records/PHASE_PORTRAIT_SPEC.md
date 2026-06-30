# Phase-portrait figure — instrumentation + figure spec (BUILD-READY, NOT BUILT)

**Status:** queued, proof-tier, behind the Fi3 push (with the two share-rate
studies). Spec written 2026-06-29; **not run** — both because of the push hold
and because the premise needed source-checking first (it was false; see §0).

## 0. Why this is instrumentation, not a contour-read of the existing grid

The natural instinct — "extract the admissibility contour from the
`slow-decline` grid we've already run" — is **false against source**, and the
check that established that is the point:

- The decline grid records **settled-`e` vs the 5% over-difficulty gate**
  (`DECLINE_SAFETY_GATE_PCT`) — the *information* failure (the controller talking
  itself blind). Its "trough" field (`end_e`) is over-difficulty at decline end,
  **not** min-share-rate.
- The figure needs **min-share-rate vs a liveness θ** — the *disconnect* failure
  (the miner dropped). That margin **is not computed anywhere in the Rust sim.**
- A liveness θ exists **only in the essay HTML widget** (`livenessZone`,
  `better-vardiff.html`) — the one source that was explicitly *not* allowed to
  define the geometry (a widget traces sampled trajectories, not the region).

So reading the contour off the grid would characterize the over-difficulty gate,
and reading it off the widget would be the trajectory-vs-region trap. The figure
requires a new (cheap) recording pass. **What it buys is bigger than a picture:**
the sim instruments only the over-difficulty failure and the essays visualize
only the disconnect failure; this figure puts **both on one set of `(d, ṙ)`
axes**, making the corpus's central causal claim geometric — over-difficulty is
the *interior*, disconnect is the *boundary it eventually reaches*.

## 1. The object

Phase space: declines parameterized by **depth `d`** (fraction of hashrate lost)
× **rate `ṙ`** (%/hr) — the plane the cost model and archetype split already live
on. Each controller has, on this plane, two nested regions:

- **over-difficulty region** — where settled-`e` exceeds the gate (what the grid
  already measures; the *interior*).
- **disconnect region** — where `min_t(share_rate) < θ` (new; the *boundary* the
  over-difficulty region eventually reaches as θ tightens).

The **admissibility cut** is the zero-margin contour `min_t(share_rate) = θ`, and
admissibility-by-design = "the disconnect region stays clear of the modal-decline
cloud for every θ in the deployment range."

Archetype shapes (the payload — corner vs half-plane):
- **sleepy-symmetric** → failure region is a **corner** (high `d` AND high `ṙ`);
  survives anything it can catch up to. Grid-traced (no closed form).
- **structural-deadband** (DMND / current MARA production) → failure region is a **half-plane in
  `d` alone** (rate drops out — fails the gentle slow slide identically to the
  cliff). Closed-form depth threshold from the deadband margin — see §4.

## 2. Instrumentation (the cheap part — recording, not re-architecture)

The per-tick realized share rate is **already derivable** from `TickRecord` +
the schedule, so this is a fold over the existing decline loop, NOT a trial-driver
change:

```
r_obs(t) = shares_per_minute · schedule.at(t) / current_hashrate_before
         = r* · e^{−e(t)}          (e already computed in the loop)
```

Add, in the decline-window pass (`slow-decline.rs` / `decline_safety.rs`):
- a **liveness threshold `θ`** parameter — share-rate fraction below which the
  miner is dropped (analog of the widget's `livenessZone`). A *swept* parameter,
  not a constant (§3).
- per cell, record **`min_t(r_obs / r*)`** over the decline window — the
  share-rate trough (NOT `end_e`; that is over-difficulty, a different failure).
- disconnect indicator: `min_t(r_obs/r*) < θ`. The zero-margin contour over the
  `(d, ṙ)` grid is the admissibility cut.

Run over the **same `(d, ṙ)` grid the decline rig already walks**, per archetype
(champion sleepy-symmetric arm; the deadband arm). The grid the rig has IS the
sampling; only the recorded quantity changes.

## 3. The θ-family (the genuinely novel part)

θ is **not a constant** — it's the pool's liveness policy (current MARA production's is ~10 min,
generous). Sweep θ over a deployment-plausible range; each θ gives a contour
`A(θ)`, monotone in θ, sweeping across the plane. The figure shows the
**θ-family** of cuts, not one cut. This is what makes the "safety-by-config vs
safety-by-design" point geometric: a generous θ *shrinks* the visible disconnect
region (hiding a flaw), a tighter θ *expands* it back over the modal cloud —
so admissibility-by-design = failure region clear *across the θ range*, not just
at the configured θ.

**GUARD (socket, not a measured constant):** `⟨θ value + swept range =
deployment-plausible, sourced from real pool liveness policies; the widget's
livenessZone is ONE illustrative point, not the range⟩`. The figure is "the cut
as a function of an assumed θ-family," never "the cut."

## 4. Internal validation (claim↔execution check)

The **structural-deadband half-plane is closed-form**: the deadband margin gives
the depth `d*` at which a falling miner finally clears the easing crossing,
rate-independent (`Kp=−0.01`, deadband ±41.4 SPM at setpoint 10 — `d*` is the
fall that drives the lower crossing reachable). **But** the closed form gives the
*over-difficulty* half-plane (where it can't ease); the *disconnect* half-plane
is that, shifted by the liveness grace (θ + window lag). The instrumented contour
must agree with the closed form **up to the θ grace** — that agreement is the
figure's internal validation, the same claim↔execution discipline as the rest of
the corpus. (The sleepy corner has no closed form; it stays grid-traced.)

## 5. Guards (bake in so the built figure can't overclaim)

1. **θ is a policy socket** (§3) — figure is θ-family-conditional, never absolute.
2. **`(d, ṙ)` is a 2D SLICE, not the whole space** — real declines have *shape*
   (drift vs step), and `min_t(share_rate)` depends on the estimator **window**
   (a 15-min boxcar troughs differently than an EWMA), so the contour is
   **window-specific** and the plane is a projection. Caption: "the illuminating
   slice, window-specific," not the whole object. `⟨window-specific contour⟩`.
3. **Grid-resolution** — a contour from a coarse grid is an interpolation; the
   sleepy-corner boundary is "traced from the sim grid at resolution X," not an
   analytic curve. Only the deadband half-plane is closed-form.

## 6. Essays placement — DECIDE AFTER THE REPORT FIGURE EXISTS, not before

Build for the **technical report** first (proof-tier; the θ-socket and
slice-projection hedges live comfortably there). The figure *might* then carry
"the corner" in plain English better than prose — the modal cloud sitting safely
outside the sleepy corner but inside the deadband half-plane is a one-look
argument. But two bars must clear first, decidable only once it renders:
- **honestly schematic** — a hedged phase portrait can read more technical than
  the essays' register; a `(d,ṙ)` plane with density + θ-contours is a step up
  from the essays' trajectory-level register and could break the "you don't need
  the technical report" framing.
- **non-duplicating** — the essays already carry decline-shapes and the spiral; a
  *regions* view (not trajectories) may complement rather than duplicate, but
  that's a judgment on the rendered object.

If it clears both: lift a **simplified, explicitly-schematic** version (abstract
regions + modal cloud + corner-vs-half-plane) into the essays, leaving the
θ-family and projection machinery in the report — the same lift-the-clean-version,
leave-the-machinery pattern as Fig 5. **This is a post-render decision; do not
commit an essay slot to a figure that doesn't exist yet.**

## 7. Build order when the queue clears

1. add θ param + `min_t(r_obs/r*)` recording to the decline pass (§2)
2. run over the existing `(d, ṙ)` grid, per archetype, sweeping θ (§3)
3. trace zero-margin contours; overlay the §6 modal-decline density
4. check the deadband contour against its closed form up to the θ grace (§4)
5. render the report figure with all three guards captioned (§5)
6. THEN decide essays (§6) on the rendered object

## 8. FIRST-CUT RESULT (`bin/phase-portrait`, run 2026-06-29) — premise checked, two corrections

A minimal instrumented cut was built and run BEFORE the full figure, to check
the geometry emerges from real cells (the premise-check-before-build discipline).
The instrument (`min_t(r_obs/r*) = min_t(e^{−e})` folded over the decline loop)
works, and the two archetypes provably differ in the data — BUT the run refuted
two of this spec's earlier assumptions, both load-bearing:

- **The archetype difference is REAL in the data.** Sleepy's trough is a 2D
  function of (rate, spm) — worsens down-and-left (0.86→0.60 as rate climbs,
  deeper at low spm). Deadband's trough is rate-dependent but **spm-INVARIANT**
  (every spm column identical at each rate: 0.96/0.92/0.80/0.60/0.50/0.50) — the
  "evidence regime drops out" signature. The shapes are distinguishable; the
  figure's core claim holds.
- **CORRECTION 1 — θ-sweep is REQUIRED, not optional polish.** At the widget's
  θ=0.36, on the 50%-depth scenario, **zero cells disconnect** for EITHER
  archetype (all troughs ≥0.50). The failure regions are EMPTY at this θ. So the
  interesting θ (where regions appear and separate) is higher; §3's θ-family is
  not "the novel part to add" — without it there is no region to draw at all.
  The figure must sweep θ up from 0.36 to find where each archetype's region
  first appears.
- **CORRECTION 2 — DEPTH must be a real axis; the rate-only proxy is wrong.** The
  deadband's defining claim is failure-as-half-plane-IN-DEPTH. The first cut
  fixed depth at the scenario's 50% and swept rate, so it could NOT see the
  depth half-plane — only the trough deepening with rate at fixed depth. The
  spm-invariance is suggestive but is not the depth-half-plane the claim is
  about. The real figure needs `decline_scenario` parameterized by depth (cap
  the ramp at variable `to`), not just rate.

Net: the instrument is validated and committed (`bin/phase-portrait`), the
archetypes provably separate, but the FIGURE needs the θ-sweep (Correction 1)
and a depth axis (Correction 2) before it shows anything — both were in this
spec's ambition (§3, the `(d,ṙ)` plane) but were deprioritized in the first cut
and the run proved them load-bearing. No figure rendered; building one on the
θ=0.36/fixed-depth cut would have shown two empty regions and miscalled it.

## 9. THE EMPTY REGION IS A RESULT (report this whether or not the figure renders)

The θ=0.36 emptiness is not a build obstacle — it is the geometric confirmation
of the cost model's central claim, arrived at by a second road. The cost reframe
concluded the first-order cost is interior (riding over-difficulty: variance +
diagnostic blindness), NOT the disconnect tail, which a generous timeout mutes.
The run shows this directly: **at realistic θ the disconnect region is empty over
the modal declines, so the action is all in the over-difficulty interior, not at
the liveness boundary.** State this in the report regardless of the figure.

**The deciding question — answered from the instrument, no new run.** The trough
IS the over-difficulty surface in other units (`e = −ln(trough)`), so this turn's
bin already produced BOTH failure surfaces. Deadband, at the MODAL (low-rate)
cells a real decline visits:

| rate %/hr | trough | over-diff `e=−ln(trough)` | over-diff > 5% gate? | disconnect < θ=0.36? |
|---|---|---|---|---|
| 2 (modal) | 0.92 | +8.3% | **YES (interior fail)** | no (boundary clear) |
| 5 (modal) | 0.80 | +22% | **YES (interior fail)** | no (boundary clear) |
| 10 | 0.60 | +51% | YES | no |
| 20–40 | 0.50 | +69% | YES | no |

**So the figure HAS its punchline, confirmed in data:** the deadband rides deep
over-difficulty on the modal declines (interior failure squarely in the cloud)
while its disconnect boundary stays clear — the failure is real and **invisible
at the liveness layer a pool would notice**. Safety-by-config (generous θ hides
it), not by-design. That interior/boundary relationship — not two filled regions —
IS the figure.

**CORRECTION 3 to the build target (supersedes "sweep θ up until regions
appear").** Do NOT crank θ tighter than realistic to populate the disconnect
region — that is the fabricated-precision trap with a θ-knob (a region that only
exists at a timeout no pool uses). The honest figure draws BOTH regions
(over-difficulty interior + disconnect boundary) across the REALISTIC θ-range,
and the emptiness/narrowness of the disconnect region there is the content. The
build still needs the depth axis (Correction 2); it does NOT need an
unrealistic-θ sweep (Correction 1 is satisfied by "report the empty region as the
result," not by tightening θ to fill it).

**Build-worth verdict:** the figure is worth building (punchline confirmed
present), needs depth-as-axis + realistic-θ-range + the over-difficulty surface
overlaid (already computable from trough), and renders the interior-vs-boundary
relationship. Still proof-tier, still queued — but no longer at risk of being a
null render.

## 10. DEPTH-AXIS RESULT (`bin/phase-portrait` rewritten, run 2026-06-29)

Depth is now a real axis: a `(rate × depth)` grid sliced at spm=6, recording
BOTH region quantities directly against their own thresholds (NOT one derived
from the other) — settled-`e` vs the 5% gate (over-difficulty/interior) and
`min_t(r_obs/r*)` vs θ=0.36 (disconnect/boundary). The result is cleaner than the
spec anticipated: the deadband's failure geometry is **fully analytic**.

**The deadband is a no-op during the decline; both its failure surfaces follow.**
At the rates where depth is genuinely exercised (20, 40 %/hr — see the cap caveat
below), the disconnect troughs are *identical across rate* and equal **1 − depth**
exactly (0.90/0.80/0.70/0.60/0.50 for depth 0.10–0.50). That is the signature of a
controller that does nothing during the decline: observed rate just tracks the raw
hashrate loss, `r_obs/r* = H/Ĥ = 1 − d`. Likewise settled-`e` = `−ln(1 − d)`
(8.3% at d=0.08, 4.1% at d=0.04 — both reproduced from the data). So ONE no-op
identity generates both regions, with two depth thresholds:

- **over-difficulty** crosses the 5% gate at actual depth **d\* ≈ 5%**
  (`−ln(1−d) > 0.05`),
- **disconnect** crosses θ=0.36 at actual depth **d\* = 1 − θ ≈ 64%**.

This is §9.2a's "one surface, two level sets" made exact and depth-resolved: the
modal-decline cloud (8–20% actual loss) lands squarely in the gap `(5%, 64%)` —
**interior fails, boundary clear** — and the disconnect boundary bites only past
~64% depth, the cliff corner where any controller crashing that hard trips
liveness regardless. The over-difficulty crossing is a **vertical line in depth,
rate-independent** = the half-plane-in-depth the spec predicted (§1, §4), now
measured and given its closed form (the no-op identity IS the closed form §4
asked for; d\*_disconnect = 1−θ is the trivial no-op case).

**Why the closed form is trustworthy where a fit would not be.** `e = −ln(1−d)`
is not a curve fit to the deadband's points — it is *derived from the controller
doing nothing*. Because it never retargets, observed rate IS the raw loss and the
error follows analytically; the points line up because the mechanism forces them
to, not the reverse. The same fact explains the verticality: a no-op has no
rate-dependence to give the boundary any shape but vertical, so **rate drops out
of the failure boundary precisely because nothing acts on it**. Geometry and
mechanism are one fact — the half-plane is vertical *because* the controller is
inert. This is also why the eventual SVG is sound: it will *illustrate a derived
fact*, not *be the evidence for it*, which retires the trajectory-vs-region worry
that opened this sub-thread (§0) — you cannot misread a closed form off a plotted
curve.

**Sleepy is a corner, by reacting.** Its troughs sit far ABOVE 1−depth (0.81 at
depth 0.80/rate 20, vs the deadband's would-be 0.20) — it eases as shares thin, so
observed rate stays up. The trough depends on BOTH rate and depth (deeper AND
faster to trough), the corner signature. On this spm=6 slice sleepy never crosses
either threshold (settled-`e` negative everywhere — slightly easy, the safe
direction; trough ≥0.73). Its only failure (the +2.7% at sparse 2–4 spm, §9.2) is
OFF this slice — the plane is evidence-rate-specific (§5 guard 2).

**CAVEAT — the 4h decline cap flattens depth at low rates (state it, don't hide
it).** `DECLINE_MAX_MIN=240` means a slow rate cannot reach a deep target in
bounded time: at 2%/hr only 8% is lost in 4h regardless of the depth-target
column. So the depth axis is genuinely swept only at **rate ≥ 20%/hr**; the
low-rate columns are depth-flattened (every depth row reaches the same
cap-limited actual loss). The slow+deep corner is **unreachable in bounded time,
NOT "safe"** — do not read the low-rate columns as a depth sweep. The half-plane
claim is read from the cap-free columns (20, 40). Raising the cap to exercise
depth at modal rates would test 40h declines (2%/hr → 80%) — a scenario past
"sustained decline," so the cap stays and the limit is reported.

**Net:** the figure is now not just punchline-confirmed but analytically backed —
the deadband surface is `e = −ln(1 − min(d, rate·4%))`, two thresholds cut it, the
modal cloud is in the gap. Worth rendering; the honest render uses the cap-free
columns for the depth claim and labels the cap-flattened region as unreachable,
not empty-because-safe. Essays decision still post-render (§6).

## 11. RENDER (`bin/phase-portrait` → `figures/phase_portrait.svg`, 2026-06-29) — BUILT

The figure is rendered by the SAME bin that produces the surface (one
data→figure path, nothing copied; `VARDIFF_PP_OUT` overrides the target). Two
panels share the depth axis, and the panel TREATMENTS encode the methodological
contrast directly:

- **Deadband (left) — drawn from the closed form.** Three horizontal zones
  (safe / interior-fails-boundary-clear / both-fail) split by the two derived
  thresholds (gate at d≈5%, disconnect at d=1−θ≈64%). Horizontal because the
  failure is depth-determined and rate drops out — the half-plane drawn straight
  because the controller is inert (§10). This panel is NOT measured points; it is
  the derived fact, which is why it is sound to draw it as clean lines.
- **Champion (right) — a measured trough heatmap.** It reacts, so the surface
  must be traced; blue opacity ∝ `1−trough`, deepening down-and-right into the
  corner, crossing neither (dashed) threshold on this slice. The contrast in
  treatment IS the point: analytic vs traced, because no-op vs reacts.
- **Both panels:** the cap-unreachable region (`depth > 4%·rate`) is a
  transparent hatch OVERLAY labeled "unreachable in a bounded decline — NOT
  safe" (the colored zone shows through: you can't *read* here, the controller
  isn't *safe* here); the modal-decline cloud is boxed, sitting in the amber gap.

Embedded in `information-floor.md` §9.2a. Classed as a **data figure**
(sim-generated, measured/derived output), NOT a structural-framing figure — so it
is not governed by the item-1/rate-aware supersession that `FIGURES-STATUS.md`
tracks; it is listed there only for provenance.

**Render-quality checks done (rasterized + inspected, not just well-formed XML):**
hatch made a transparent overlay so it does not bury the amber punchline band;
modal-label moved below its box to clear the gate-line label; champion heatmap
verified to stay clear of both threshold lines. **Still genuinely ahead:** the
essays-placement call (§6) — decided against the duplication-and-register bar now
that the object exists; the figure illustrates a derived fact, so the
trajectory-vs-region worry (§0) does not apply, but the register question
(abstract `(d,ṙ)` plane vs the essays' concrete trajectories) is the live one.

## 12. THE "HOW BAD" TWIN — variance-inflation curve (`figures/variance_inflation.svg`, 2026-06-29) — BUILT

The portrait shows *where* the failure is; this curve shows *how bad* it is there,
from the SAME identity — no new run. Relative payout variance over a window goes
as `1/(r_obs·τ)` (Poisson share count, §1/§3); on-target that is the floor
`1/(r*·τ)`, and over-difficulty pushes `r_obs → r*·trough`, so the inflation over
the floor is exactly `1/trough`. Deadband = closed form `1/(1−d)` (the no-op
identity again — derived, not measured); champion = `1/trough` measured at the
cap-free rates (20, 40 %/hr). Result: deadband climbs to ~5× at 80% loss (~1.25×
at 20%, ~2× at 50%); champion holds ≤~1.4× even deep/fast. Inflation is OVER the
floor, so it stacks on the irreducible `1/(r*·τ)` the bandwidth lever sets (§6) —
the floor is where everyone starts, this is how far the controller pushes you
above it. Embedded in `information-floor.md` §9.2a beside the portrait; this is the
variance currency of the four-currency harm account made concrete.

**A real bug the RENDER caught (kept as a record — this is why rasterize-and-look,
not well-formed-XML, is the check).** First render of the variance curve showed
the champion (blue) tracking the deadband (red) to ~5× — flatly contradicting the
captured sleepy trough surface (≥0.73 → ≤1.4×). Cause: `arms = [sleepy, deadband]`
so arm **0** is the champion, but both the variance champion curve AND the
ALREADY-COMMITTED phase-portrait champion heatmap read `get(1, …)` = the deadband.
The portrait heatmap hid it (a darker-than-true heatmap reads as plausible; no zone
contradicted it); the line chart exposed it (a curve in the wrong place is
unmissable). Fixed both to `get(0, …)`; re-rendered and re-verified the champion
panel is now sleepy's shallow troughs. The lesson is the checker-vs-artifact one
again: well-formed XML passed both times; only the rasterized read adjudicated, and
the figure type that made the defect VISIBLE (line vs heatmap) is what caught it.

## 13. TARGETED AUDIT — "what else is single-source, single-figure?" (2026-06-29)

The §12 bug proved a CLASS exists: a figure that selects its data by an
index/label decoupled from the labeled identity, where a wrong pick still renders
coherently and no second view disagrees. Since a wrong figure of this class had
ALREADY SHIPPED (and passed a rasterize-and-look at the time), a sweep of every
other figure generator for the same class is a check on committed work, not
build-ahead — run immediately. Read-only audit of all 9 SVG-emitting bins
(4 parallel readers).

**Two filters applied before treating any flag as exposure:**
1. **Is it embedded in the report?** `floor_ribbon`, `trajectory_plot`,
   `regret_radar` are NOT embedded in `information-floor.md` (regret_radar is the
   *rejected* six-axis radar, referenced only in THEORY.md's retraction). Flags on
   non-embedded figures are not live report exposure. *(Later update: `floor_ribbon`
   was dropped as an unembedded byproduct; `trajectory_plot` was subsequently
   embedded at §8.5 as a demoted supporting figure. The audit's reasoning stands as
   recorded; this notes the figures' current status.)*
2. **Is the selection CURRENTLY wrong, or merely brittle-if-refactored?** The §12
   bug was *present-tense wrong* (label "champion" / data = deadband, from
   DIFFERENT sources). The readers (primed to find the pattern) flagged several
   embedded figures, but adjudicating each against source (the direct read decides,
   not the reflex):
   - `tau_valley.rs:258` — picks the s=1.5 column, but the figure's VERIFIED premise
     (caption + table) is that columns are identical to 0.1% across sensitivities.
     A "wrong" column renders the SAME curve — choice is immaterial. **Not exposure.**
   - `steady_transient.rs:254` — `named` is computed from the SAME `tau`/`sens` that
     build the plotted cell (`cfg(tau,sens)`) on the next line. Label is mechanically
     tied to data, not decoupled. **Not exposure.**
   - `tau_tradeoff.rs` `.2` tuple-field — `.2`=settled is cross-checked by the
     named destructuring `let (area,wob,_)=rows[ti]` used for the curves; correct now,
     and a reorder breaks both views. Brittle, not wrong. **Not exposure.**
   - `tau_family.rs`, `excess_lever.rs` — auditor verdict SAFE, confirmed.

**Verdict: no live present-tense exposure of the §12 class in any embedded figure.**
The structural reason: in the §12 bug the selector and the labeled identity came
from DIFFERENT sources (the literal "champion" label vs `get(1,…)`); in every
embedded figure here the label and the data derive from the SAME source (same
`tau`/`sens`, same enum, same verified-identical column). That sameness is the line
between *wrong* and *brittle*. The audit found brittleness (correct now, silent if
later reordered), not a second shipped-wrong figure — and the honest record is that
the auditors over-flagged the mechanical pattern and the source read narrowed it.

**Residual (logged, not blocking):** the brittle spots want the cheap structural
defense — bind label-to-data so a reorder can't silently decouple them (e.g.
`(label, extractor)` structs instead of parallel arrays; named destructuring over
`.N`). Worth a pass when the queue next opens; not exposure today.

## 14. LAY-TWIN of the variance figure (`figures/variance_inflation_lay.svg`, 2026-06-30) — BUILT (figure only; essay edit HELD)

The essays-placement analysis (the why-essay, `better-vardiff.html`) concluded the
variance number is the ONE finding from this arc that crosses into the plain-English
layer as a *figure* — it fills a real hole (the essay says over-difficulty "doesn't
lose work but is still bad" then is vague about *how* bad) and it is the most
lay-accessible result produced: *a bad controller makes payouts ~5× noisier exactly
when the miner is already struggling; the good one stays near normal.* The SV2-
cheapens-the-lever point and the sweet-spot-band are sentence-tier (no figure, SV2
stated qualitatively — the ~2× rides the noise-framing socket); everything abstract
(phase portrait, frontier, τ-figures, constraint_space) is held report-only because
its honest version needs machinery a lay reader can't carry.

**Scope is frozen at three things** (figure + two sentences) to protect the two-part
reframe's promise ("you don't need the technical report"); lifting more would drift
the essay proof-tier. Per the timing rule, the published voice is the user's to time,
so: the lay FIGURE is built now (self-contained, commits nothing to the essay, the
cheap reviewable half that de-risks the eventual drop-in); the essay EDIT (insert +
two sentences) is HELD for the user's go. Build-vs-edit, not a blanket hold — the
figure has a clean stopping point (it renders), the edit is where scope-creep lives.

The lay-twin is rendered by the SAME bin (`phase-portrait.rs`) from the SAME `get`
source, stripped to register: depth on x ("% hashrate lost"), y in "Nx noisier /
normal", "a bad controller" vs "a good controller", typical-decline band shaded, and
the load-bearing caveat in the caption — "noisier, not smaller — you still earn what
you're owed." NO `1/(1−d)`, NO τ, NO cap language.

**Cross-check WITH TEETH (the check this build alone could run).** The lay-twin
shares the exact `1/trough` quantity with `variance_inflation.svg` and the portrait
champion panel — the cross-derivation tie that caught the §12 index bug. So the lay
function RETURNS the champion series it actually drew, and `main()` compares it
against an INDEPENDENT `1/get(0,·)` read (a tautology `1/t==1/t` would have no teeth;
comparing drawn-vs-independent does). If the lay render used the wrong arm (the §12
bug) its drawn series would be `1/get(1,·)`=deadband and the check fires. Result:
`cross-check OK — all three views agree (§12 fix holds)`. Third view of the same
number, free, with teeth.
