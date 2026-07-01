# Vardiff at the Information Floor: What a Share-Rate Controller Can and Cannot Do

**Abstract.** A variable-difficulty (vardiff) controller is unusual: the only
thing it can measure is its own tracking error. We take that one fact to its
conclusion and arrive at a result that is structural, not a horse race. Two
theorems — a *measurement identity* (the observable depends on hashrate only
through the controller's error) and an *information floor* (the error estimate's
variance is Cramér–Rao-bounded by `1/(r*τ)`) — together imply that across the
operating band (`r* ≈ 4–30` spm) **every reasonable controller is pinned to the
same floor**: the spread between best and worst is a narrow band in composite
cost — about 12% — the residual differences are *gentleness and safety* rather
than *agility*, and the one lever that moves the floor is the share rate `r*`
itself. The metric we use
to score algorithms — **linear sign-split tracking regret + direction-split
effort, per scenario class, with detection reported separately as a
rate-dependent diagnostic rather than folded into the production score** — is
derived from the same two theorems; an earlier version that scored detection
inside the production scalar was corrected after measurement showed the term is
floor-saturated (carries no ranking signal) below ~10 spm.

The champion we ship, `Ewma360/s1.5`, is **not the hero of this story**: it is
the *existence proof* that the safe corner of the achievable frontier is
occupiable. It is deliberately the **gentlest decline-safe configuration**, and
it pays for that with slow transients — ~65-min cold-start ramp and ~28-min
detection latency at 12 spm — which read as a regression only if one expects "our
best algorithm" and read as the thesis once one expects "the gentlest config
that is provably safe on a sustained decline." The discipline that produced this
is itself part of the result: three candidate hero claims were drawn and killed
by measurement before they could be published (§8), and the surviving claims are
trustworthy precisely because the casualties are named rather than buried.

Scope, stated once and honestly: the *behavioral* layer (counter-age blindness,
champion-beats-incumbent on a live drop) is confirmed on real hardware. For the
present champion specifically, the decline response is hardware-confirmed *in
direction* — it eases the safe way on a sustained 50% drop, with no rejection
spike — while its settled over-difficulty figure and its behavior on a slow,
moderate decline (the regime the safety gate binds on) remain simulation results
(§9.4). The cost-model weights remain a calibrated judgment.

Convention: *Theorem* and *Lemma* are reserved for results genuinely proved from
the model (Theorems 1–2, the §4 Lemma); everything else load-bearing is
an *Argument*, *Rationale*, or *Choice* — our reasoning or our decisions — and
*Observation* is a fact about particular algorithms established in simulation
(named inline). Mathematical claims are never "validated numerically": they are
proved or they are not.

Note on terminology: *regret* here means the time-integrated tracking loss
`∫|e|`, **not** online-learning regret against a comparator; read it as "tracking
loss."

---

## 1. The problem, and the model

Variable difficulty exists because the miners on one pool span many orders of
magnitude in hashrate, while the pool needs every connection to deliver shares at
roughly the same rate. A share is a proof-of-work that clears a per-connection
difficulty `D`; a miner of hashrate `H` clears it and submits valid shares at a
rate proportional to `H/D`. A single global `D` would bury the pool under shares
from its biggest miners and starve its smallest of shares entirely — and starving
a connection of shares means no timely hashrate estimate for it and a
high-variance, lumpy reward. Per-connection vardiff fixes this by moving each `D`
so that connection's share rate stays near a target `r*`.

That makes `r*` the one design constant doing three jobs at once: it caps the
bandwidth and CPU each connection costs, it fixes the variance of the pool's
per-miner hashrate estimate, and it bounds the miner's reward variance. The
controller's whole job is to hold the realized rate at `r*` as `H` drifts. **`r*`
is also, as the rest of this document shows, the only quantity that moves the
fundamental limit on how well any controller can do that job** — which is why it
recurs from the model here (§1) to the floor (§3) to the lever (§8). A connection
delivering only a few shares per minute has a hashrate estimate so noisy (its
band is `1/√(r*τ)`, §3) that reward variance and detection both suffer regardless
of vardiff — which is *why* `r*` is set above that floor, and why the
controller's worst regime (§9, the sub-guard analysis) is one the system is
provisioned to avoid.

Two modeling choices fall out of the physics; neither is a free knob. **Share
arrivals are Poisson** because each hash is an independent trial with a tiny
success probability against an enormous number of trials per window, so over any
window short enough that `H` is steady, the count is Poisson with rate equal to
hashrate over work-per-share. Under `D = Ĥ/r*` that rate is `r_obs = r*·H/Ĥ` —
equation (1) — with the proportionality constant folded into the units of `D`,
which is why nothing downstream depends on it. And **the natural coordinate is `e
= ln(Ĥ/H)`** because the controller only ever acts on `D` multiplicatively, the
only thing it can observe depends on `Ĥ` solely through the ratio `H/Ĥ`, and only
in log units is a 2× over-difficulty the same distance from target as a 2×
under-difficulty. Working in `e` turns a retarget into an additive step `s` and
makes the two signs honest, symmetric coordinates instead of artifacts of where
you sit on the difficulty axis.

This is an *evaluation* model, not a control-design model. To rank algorithms we
need only the path each produces — the sequence of `(error, did-it-fire,
step-size)` — and the Poisson-in-log model produces exactly that: cheaply,
repeatably, and identically for an algorithm whose internals we can read and one
we cannot, because the only signal any controller has is a function of `e` alone
(Theorem 1). That is what lets one metric rank the whole field without privileged
knowledge of anyone's implementation.

What the model leaves out — because the metric covers exactly the situations the
model can express. It assumes `H` is constant within a window (true over one
retarget, false across a real ramp, which is why scenarios drive `H(t)`
explicitly); one worker per connection (real connections pool many workers,
raising the effective rate and reshaping the noise); retargets that are instant
and lossless — which, against this implementation, is exact, not an idealization:
a retarget rejects no in-flight work (shares validate against a per-job target
snapshot, §6), so there is no transition cost to model; and a continuous
difficulty with no floor and no rate limit on how often it can change (real pools
clamp `D` from below and cap fire cadence). The cadence cap and the churn/usability
cost land on the effort term and are taken up in §7.
Anything beyond this list is a coverage gap to declare, not a hole in the
derivation.

### Setup

A **miner** has true hashrate `H > 0`. The **controller** holds a belief `Ĥ > 0`
and sets difficulty `D = Ĥ/r*`, where `r*` is the target share rate
(shares/min). Shares then arrive as a Poisson process of rate

```
  r_obs = r* · H/Ĥ.                                                  (1)
```

Over a window of `τ` minutes the controller sees a single number: the share count
`N ~ Poisson(λ)`, `λ = r_obs·τ`. It periodically **fires** — rescales `Ĥ` by a
factor, i.e. adds a **log-step** `s = ln(Ĥ⁺/Ĥ⁻)` to its belief; `s > 0`
*tightens*, `s < 0` *eases*.

**Definition 1 (error coordinate).** The controller acts multiplicatively (it
scales `Ĥ`) and, by (1), the observable depends on `Ĥ` only through the ratio
`H/Ĥ`. A multiplicative quantity is linearized by its logarithm, and only in log
coordinates are an over- and an under-shoot by the same *factor* equidistant from
the target. So work in

```
  e = ln(Ĥ/H),     so that     r_obs/r* = e^{−e}.                    (2)
```

`e = 0` is exact; `e > 0` is over-difficulty, `e < 0` under-difficulty. The
control goal is `e → 0`.

A **scenario** specifies `H(t)` and the initial belief; the named classes
(`baseline.rs`) are *cold-start*, *stable*, *step ±p%*, *settled-aged drop*
(truth holds long enough to mature the share counter, then drops), and *sustained
decline* (a continuous `−ρ %/hr` ramp — the safety scenario of §9). A **metric**
ranks algorithms by averaging over a fixed ensemble of classes, **per class,
never pooled**, and — as §3 forces — **per share rate `r*`**, because `r*` moves
the floor that everything else is measured against.

---

## 2. The controller measures only its own error

**Theorem 1.** *The sole observable `N` has a distribution depending on `H` and
`Ĥ` only through the error `e`.*

*Proof.* By (1)–(2), `N ~ Poisson(r*τ·e^{−e})`. The parameter is a function of
`e` alone. ∎

There is no measurement of the miner separate from the error being controlled.
Every familiar quality statistic — accuracy, jitter, reaction, overshoot — is
therefore a functional of the one scalar process `e(t)` together with the fire
sequence the controller derives from it. This is why a small set of `e`-based
terms can suffice, and why the metric is written entirely in `e` and `s`.

---

## 3. Precision costs time, and time costs agility — the result everything rests on

**Theorem 2 (information floor).** *Any unbiased estimate of `e` from a window of
`τ` minutes has `Var(ê) ≥ 1/(r*τ)`.*

*Proof.* The window's count is `N ~ Poisson(λ)`, `λ = r*τ·e^{−e}`. For a Poisson
observation the log-likelihood in `e` is `N ln λ − λ`, with score `∂_e(N ln λ −
λ) = (N/λ − 1)·λ' = λ − N` since `λ' = −λ`. The Fisher information is `E[(λ−N)²] =
Var(N) = λ`; at the operating point `e≈0`, `λ = r*τ`. Cramér–Rao gives `Var(ê) ≥
1/(r*τ)`. ∎

(The crate encodes this as `SettledAccuracy::poisson_floor`, `metrics.rs:864` —
`1/√(r*τ)` as a percentile.)

This is the load-bearing result of the entire document. Read structurally, it
says the controller's information per unit time is fixed by `r*` alone; a
controller can spend that information on precision (long window) or on agility
(short window), but it cannot manufacture more of it. Three consequences follow,
and they are the spine of everything below.

**Corollary (the central trade-off).** Steady-state precision improves only by
enlarging the averaging window `τ`; but the same `τ` is the lag in following a
real change. **Accuracy and agility are bought from one budget at a fixed rate
`r*`.**

- *The apparent quality axes are one trade-off.* The estimator's window trades
  accuracy against lag; the fire threshold trades false alarms against detection
  delay; the retarget gain trades convergence against overshoot. Each is one knob
  on the same accuracy-vs-agility line. Scoring six such projections with equal
  weight (the deprecated `EqualWeightFitness`) rewards the *middle* of the
  trade-off curve, not the *frontier*. **Observation (commit `31a9dbc1`):** four
  independent parameterizations of the pipeline all saturate at the same quality
  ≈0.55 — one wall seen four ways. So score the frontier's own coordinates:
  **tracking error** and **control effort**, nothing derived from them.
- *A steady offset has a price, not a defect.* An algorithm sitting short of the
  floor is paying for it elsewhere (agility, effort); §10 uses this.
- *The field is flat across the operating band; detection is floor-limited at
  production rates.* This is the structural headline, and it is a direct reading
  of the bound: when `r*τ` is small (production rates, monitoring-length windows),
  the noise band
  `1/√(r*τ)` is wide enough that controllers differing in window or threshold
  produce nearly the same achievable tracking error and nearly the same
  (in)ability to see a small change. §8 measures this directly — a ~12% best-to-
  worst spread in composite cost across the 4–30 spm operating band, and a
  detection signal that is statistically zero below ~10 spm and opens up only as
  `r*` rises (Figure, §8.4: the lever).

The floor `1/√(r*τ)` is a **noise band** — one standard deviation of an
*unbiased* estimator — not a systematic offset. When §8/§10 quote an accuracy
ceiling, that is the *width* of this band at the operating `r*τ`, the scatter a
perfect tracker still shows, against which a real algorithm's *systematic* offset
(§10) is measured.

**The achievable frontier, in one line each.** Two floors follow from Theorem 2
and bound what *any* algorithm can do; the §9 sub-guard analysis places the
champion against them.

- *Static (stable load).* On steady `H`, the cost-minimizing settled offset under
  the §6 asymmetry is not zero but a quantile of the noise band: `e* ≈ −0.67·σ`,
  `σ = 1/√(r*τ)`. At 60 spm, `σ≈8%`; at 2 spm with `τ≈2.5 min`, `r*τ≈5` so
  `σ≈45%` — the band swamps any few-percent offset, i.e. at low share rate the
  asymmetry-optimal target is itself lost in the noise.
- *Dynamic (declining load).* On a ramp of `ρ` per minute, an estimator of
  effective averaging time `τ_eff` lags truth by `≈ ρ·τ_eff`, while its noise is
  `1/√(r*τ_eff)`. Shrinking `τ_eff` to cut the lag widens the noise; their sum
  `L(τ_eff) ≈ ρ·τ_eff + z/√(r*τ_eff)` has a floor at `τ_eff ∝
  (z/ρ)^{2/3}·r*^{-1/3}` — a minimum tracking error no algorithm beats on a
  decline of that rate and share count. This is the bound the §9 sub-guard cells
  are read against, and it is the physics behind the τ-safety-valley of §8: too
  long a window lags a sustained decline into the dangerous direction, too short
  a window is too noisy to act on, and the safe window is the minimum of `L`.

---

## 4. The error norm: squared is blind, linear is not

Tracking cost is `∫ f(e) dt` for some norm `f`. The choice of `f` is a judgment
about how harm scales with error — but one judgment is inadmissible.

**Lemma (blindness of the square).** *A persistent, undetected fractional
hashrate drop of size `g` produces a steady error `e = −ln(1−g) = g + O(g²)`.
Under `f(e)=e²` it costs `e² = g² + O(g³)`; under `f(e)=|e|` it costs `|e| = g +
O(g²)`.*

*Proof.* The drop sends `H → (1−g)H` with `Ĥ` fixed, so `e = ln(Ĥ/((1−g)H)) =
−ln(1−g)`; expand. ∎

Operational harm from a difficulty error (lost or excess work) scales with its
**magnitude** `g`, i.e. linearly. The squared norm undervalues it by a factor
`1/g`, which diverges as `g → 0`: a small *persistent* leak — a failing or
throttling ASIC, the case operators care about most — is essentially free under
`e²`. **Observation (`regret-effort`):** an algorithm that detects a −10% drop
~1% of the time scores *better* on `e²` than one that detects it always, because
the miss costs only `(ln0.9)² ≈ 0.01`/min. The linear norm removes this blind
spot (the same miss costs `≈0.10`/min) and only reorders the middle of the
ranking, never the top.

**Choice 1.** Use `f(e) = |e|`. Report it split by sign, since the two signs
carry different harm (§6):

```
  regret_over  = ⟨|e|⟩ over time with e > 0       (over-difficulty)
  regret_under = ⟨|e|⟩ over time with e < 0       (under-difficulty)
```

---

## 5. Detection is a separate axis — and at production rates the floor flattens it

Linear regret narrows the blind spot but does not close it: a fast algorithm with
occasional large errors can still outscore a chronically blind one on `∫|e|`. The
deeper reason is structural.

**Argument (detection is not derivable from the scored error paths).** *There is
no functional `F` with `detection = F(e on stable, e on step)` holding across all
algorithms.*

*Why.* Catching a small drop requires the share counter to be *young* when the
drop lands: a matured counter averages the weak post-drop signal against a long
pre-drop baseline and never crosses threshold. Counter age at the drop is
determined by the fire history of a *matured, on-target* loop — a state the
stable scenario (never perturbed) and the step scenarios (perturbed while young
or with a large signal) never enter. Two controllers can therefore produce
*identical* stable and step error paths yet differ on detection, because they
differ only in that matured-counter regime. So detection must be carried
explicitly:

```
  detection = P[ fire within W min | counter matured, then −g drop ].
```

**But detection must be measured against its own false-alarm rate, and at
production rates that correction sends it to zero.** The honest quantity is not
the raw fire probability — which a twitchy controller inflates by firing often
regardless of any drop — but the **excess**:

```
  EXCESS = P[fire within W | −g drop] − P[fire within W | no drop].
```

**Observation (`detection-control`, `excess-lever`):** at production rates the
information floor (Theorem 2) is so coarse that a −10% drop is statistically
invisible within a monitoring window — `EXCESS = 0.00` at a 60-min window and
`+0.05` at a 15-min window at 4–6 spm, *for the whole field*. The raw detection
number is then an artifact of fire cadence (the window straddles scheduled
settling fires whether or not a drop occurred), which is exactly the trap an
earlier version of this metric fell into. The correction has two parts, and both
matter:

**Choice 2 (corrected).** *Detection is removed from the production score.* Below
~10 spm it is floor-saturated (Theorem 2) and carries no ranking signal; folding
it into the scalar there merely rewards twitchiness. It is instead reported as a
**rate-dependent diagnostic** — `EXCESS` versus `r*` — where it does discriminate
(§8, the lever): the same `EXCESS` climbs monotonically to `+0.75` at 60 spm as
the floor recedes. Detection thus stops being a scoring axis and becomes the
visible measure of what raising `r*` buys.

---

## 6. The two directions are not symmetric

**Argument.** *Over-difficulty is worse than under-difficulty, and a controller
should be reluctant to tighten and eager to ease.*

*Why — the operating-point asymmetry (§6(i)), which is the whole basis.* `e < 0`
(difficulty low): shares run a little fast; all work stays valid; cost is mild
inefficiency, and it is the *safe* side. `e > 0` (difficulty high): the connection
is starved of valid shares, which inflates both the miner's reward variance and
the pool's hashrate-estimate variance for it — risking an offline misread — and
compounds when `H` is genuinely falling (the death-spiral, §9). This grounds *both*
halves of the asymmetry directly, without any appeal to transition cost:
- **Eager to ease** (`s<0` fires readily) is death-spiral avoidance — the exact
  mechanism the §9 safety story rests on: when `H` falls, ease fast to follow it
  down rather than starve the miner.
- **Reluctant to tighten** (`s>0` requires more evidence) suppresses false
  excursions to the dangerous over-difficulty side, and when it lags a genuine
  hashrate *increase*, it lags into *under*-difficulty — the benign, safe-but-
  slightly-wasteful side. So tighten-reluctance is also a safety bias.

**Killed premise — the lost-in-flight-work argument (was §6(ii)).** Earlier
versions justified the tightening penalty with a *proof* that a tightening fire
invalidates in-flight shares aimed at the old target (fraction `1−e^{−s}` lost).
**That is false against the implementation and is retracted.** A retarget mutates
only the channel's current target (`extended.rs:341`); each job snapshots the
target *at its creation* into `job_id_to_target` (`extended.rs:510/549/668`), and
shares are validated against that per-job snapshot (`extended.rs:724`), not the
current target, until the map is cleared on a new prev-hash (`extended.rs:586`).
So a difficulty change rejects no in-flight work — old jobs stay valid until the
next block — and it does not even force a job switch (it rides the normal job
cadence), so there is no pipeline flush to charge for. Moreover, **share value is
proportional to difficulty**: a share clearing a higher threshold is worth
proportionally more, and a miner's credited work-rate equals its hashrate
regardless of the difficulty the pool sets, so even a genuinely rejected
low-difficulty share is not lost *earnings*. In expectation, churn costs the miner
**no value at all**. Its only real costs are *variance* (overshoot into transient
over-difficulty — which is §6(i), the safe-vs-dangerous-side concern, not a
transition cost) and *usability* (rejections read as errors and muddy monitoring).

**Choice 3 (re-homed and split).** The asymmetry's *existence and direction*
stand on §6(i) safety, as above — not on lost work and not on simulation fitness.
But the three "3:1" weights the paper carried were never one thing, and the killed
premise lands surgically on exactly one of them:
- **`regret_over:regret_under` (operating-point regret weighting): survives.**
  Pure §6(i) — over-difficulty is the dangerous side. Unaffected.
- **`tighten_multiplier` (boundary evidence asymmetry): survives, re-homed.**
  Directional reactivity for safety (eager-ease / reluctant-tighten, above). The
  carve-out does not touch it.
- **`effort_up:effort_down` (the effort-*direction* asymmetry): loses its basis.**
  This one was justified by lost in-flight work; with churn value-neutral, there
  is no transition-cost reason to charge an upward step more than a downward one
  of equal size. Its directional split is retired.

*Magnitude is a tuning judgment, and the finding argues it down.* The `3.0`
multiplier leaned on a fitness function weighting "stability" — which bundles
`step_magnitude_safety` (overshoot → over-difficulty; pure §6(i), fully
justified) with `jitter` (fire frequency). Value-neutrality specifically guts the
*jitter* half: if firing costs no value, penalizing it heavily rests only on
usability and minor pool overhead — much softer than a value cost. **Observation
(`champion-weights`):** the best algorithm is the same for every ratio in `[1:1,
4:1]`, and under balanced weights `1.5–2.0` ranks at least as well; only an
ungrounded `5:1` changes the ranking. So the ranking does not hinge on the exact
value, and the deflated jitter cost argues for the lower end of that range. The
price of a smaller multiplier is being slower to track a genuine hashrate
*increase* — but that lag lands on the benign under-difficulty side, so it is a
price worth paying. The asymmetry's *direction* is safety-load-bearing; its
*magnitude* is a soft tuning choice, no longer propped up by a transition cost
that does not exist.

**The retraction was stress-tested by a full champion-hunt reopen, and the
champion held — but the headline is *why*.** `sweep-recalibrated.rs` re-ran
selection under the corrected cost (effort-direction forced to `1:1`; `λ` swept
*and* dropped), with the decline gate as the selector. **The load-bearing
result: selection never rested on the cost weighting in the first place.** With
`λ`'s anchor gone the *cost*-optimal config is now `λ`-sensitive across the whole
window range (it swings from `Ewma150/s0.3` at `λ=0` to `Ewma720/s2` at `λ=2`) —
so a champion defined by the cost would be arbitrary now. But the champion was
defined by the *binding decline-safety gate* (§9), which is pure §6(i)
operating-point safety and untouched by the retraction. The cost-leaders the
recalibration favors at the long-window end fail that gate **decisively**:
`Ewma720/s2` (the `λ=1–2` cost-winner) settles at **+9.6%** over-difficulty at
the worst sub-guard cell — multiple cells well over the 5% gate — while the
champion `Ewma360/s1.5` passes comfortably at **+2.7%**. (Honest margin note:
the short-window `λ=0` cost-winner `Ewma150/s0.3` lands *at* the gate line —
exactly +5.0% in a single worst cell, indistinguishable from the threshold — so
it is gate-*indeterminate*, not a decisive failure; the vindication rests on the
unambiguous +9.6% rejection, not on the boundary configs.) So the soft `λ` was
always free to wander; it simply never mattered, because the *gate*, not the
cost, was doing the selecting all along — a clean vindication of constraint-over-
cost. One nuance kept visible: a shorter window, `Ewma240/s1.5`, also passes the
gate (+3.5%), so the champion's specific `τ=360` is now justified as the *safest*
gate-passer (2.7% vs 3.5%) and by the §8.3 τ-valley floor — not by any cost
consideration, which `λ` no longer anchors.

*The under-difficulty side has its own cost, and the metric only partly prices
it.* Under-difficulty (`e<0`) is not free: at the realized rate `r = r*·e^{−e}`,
an `e=−0.07` offset runs the connection ~7% over its target share rate,
permanently — and bounding exactly that per-connection load is *why* `r*` is set
where it is (§1). The resource cost (extra bandwidth, CPU, share-accounting) is
linear in excess volume, and since `r − r* ≈ −r*·e` near the operating point,
**linear in excess volume is to first order linear in `|e|`** — so it adds no new
functional form, only a one-sided increase to the `regret_under` coefficient. The
clean way to settle it is one measured number — marginal cost `c` per extra share
from share-accounting telemetry — added as `c·r*·max(0,−e)`; this is left as the
one external-economics input the simulation cannot supply.

*The bandwidth component of that cost is small, and it cuts in the lever's favor.*
The §8.4 lever (raise `r*` to lower the floor) has a cost the model names but does
not size: the bandwidth and CPU each connection costs scale with its share rate.
Upstream share traffic is **linear in `r*`** (`B_up = r*·s_share`); downstream
(jobs, `SetTarget`) is essentially independent of `r*`. The per-share wire size is
small in SV2: `SubmitSharesExtended` is six `u32` fields (24 B) + a length-prefixed
extranonce (`B0_32`, ≤ 32 B) under a 6-byte frame header — call it **~31–47 B on
the wire** (the upper end with a full extranonce and the noise-transport AEAD tag;
`SV2_FRAME_HEADER_SIZE = 6`, `submit_shares.rs`, `framing-sv2`). At 6 spm that is
tens of bits/sec per connection — negligible against the gain from a lower floor —
and ~5 Mbit/s upstream for 100k connections; the binding pool-side cost is message
*processing*, also linear in `r*·N`. The load-bearing point for the lever:
**an SV2 share is ~4× smaller than its SV1 equivalent (~180–220 B), so SV2
structurally affords a higher `r*` — a lower floor — at the same bandwidth.** The
lever is cheaper to pull on SV2 than on the protocol it replaces. ⟨The wire sizes
are read from the message definitions and the framing constant; the exact bytes
under encryption + TCP framing should be confirmed against the SV2 noise-framing
spec before any figure is published as measured rather than structural.⟩

*What this cost actually is — and what it is not.* Bandwidth is **not** a harm
from a bad controller; it is the **price of the remedy**. The harm a controller
does is payout variance (§4) — being *wrong*, holding difficulty too high through
a decline; that is the curve that is steep for the deadband and flat for the
champion. Bandwidth measures something orthogonal: the cost of the *one fix* the
whole analysis points to — raise `r*` to lower the floor for everyone (§8.4). The
two must not be summed: one says how badly the floor hurts you, the other says
what it costs to move the floor. But they connect, in a chain that is the
quantitative spine of the argument:

> bandwidth (and pool-side processing) sets the **maximum affordable `r*`** →
> which sets the **minimum achievable floor `1/√(r*·τ)`** → which is the
> **irreducible variance harm** no controller can get under.

The last arrow lands on the *variance* term **only** — not on total harm. The lever
is `r*`; the other two harm terms do not depend on `r*`. Disconnect is governed by
the liveness floor θ (§9.2a), diagnostic-blindness by operator behavior — both are
`r*`-invariant, so raising `r*` to the bandwidth-max buys down the statistical noise
and *nothing else*. A deadband controller sitting at the lowest achievable variance
floor is **still a deadband controller**: still failing the modal decline
structurally (§9.2a), still diagnostically blind. The lever lowers the unavoidable
noise; it is not a fix. (Reading "the floor on the harm" as "a high enough `r*`
fixes the controllers" inverts the §4 point — the chain bounds *one* of three
currencies, not the total.)

So bandwidth is what determines *how much of the variance harm is fixable versus
structural*. And because it is pure arithmetic on message size — `B_up = r*·s_share`,
no behavioral assumption, no operator-dependence, no Poisson statistics — it is the
**most exactly quantifiable number in this entire analysis**, where variance needs
the suppressed-rate trajectory and diagnostic-blindness needs operator behavior.

This is where SV2's ~4× smaller share pays off as a *number on the harm metric*,
not just "smaller messages." At equal bandwidth budget SV2 affords ~4× the share
rate; since the floor goes as `1/√(r*·τ)`, **4× the rate is `√4 = 2×` a tighter
floor.** The protocol change fixes no controller — it **lowers the floor every
controller is stuck at, by about a factor of two relative to SV1.** That is a
concrete, defensible claim tying the bandwidth study directly to the metric: the
variance curve (§4) says how far each controller sits above the floor; this says
where the floor itself sits and what it costs to move.

Two caveats, same discipline as the rest. (1) **It is a pool-scale claim, not a
per-miner one.** Per connection the cost is tens of bits/sec — the lever is
effectively free for any single miner; the constraint binds only on aggregate
`r*·N` message processing at the pool. Stated per-miner it would read as a far
larger cost than it is. (2) **The `~4×` and the `~2×` are structural, not
wire-measured** — they ride the same `⟨noise-framing spec⟩` socket as the byte
counts above; sound as structural claims, but not to be published as hard figures
until the on-the-wire bytes under encryption + TCP are confirmed.

### 6.1 The asymmetry is one *mechanism* for the real invariant, and it is regime-limited

§6 frames eager-ease/reluctant-tighten as *the* safety asymmetry. A later study (the
eager-ease mechanism work; commits `9518f3bf`→`a6ccec7f`, rigs `eager-ease-*`,
`cellA-mechanism`, `which-boundary`, `belief-vs-op`) refines this — and the refinement
matters because §6's framing makes the asymmetry the *validity axis* when it is really
*one mechanism on it*. §6's argument is **correct for its regime** (dense rate); what
follows is the mechanism-level completion, not a contradiction.

**The validity axis is dangerous-direction protection, not asymmetry.** What an
admissible controller must do is suppress *self-deepening* fires during an
over-difficulty escape — fires that raise difficulty (raise the belief) while already
over-difficult, which the `e^{−e}` collapse turns into a spiral. Eager-ease/
reluctant-tighten is one *implementation* of that suppression, not the thing itself; a
controller can provide the protection by other means, and at low rate it must.

**Fire *direction* is set by the estimator, not the boundary** (verified from source,
`composed.rs`). The deviation statistic is a magnitude (`delta = |h_estimate/hashrate −
1|`, direction-blind); the boundary returns a magnitude threshold; on a fire the update
moves the operating point *toward* `h_estimate`. So every fire's direction is
`sign(h_estimate − OP)` — set entirely by where the estimator's belief sits relative to
the operating point. The threshold decides *whether* a fire happens (magnitude); the
belief-position decides *which way* (direction). These are two separate things — keep
them separate.

**The asymmetry mechanism is the DENSE-rate protector, switched OFF at sparse rate**
(verified, `which-boundary.rs`, exact-equality). The reluctant-tighten bias lives in the
boundary's `tighten_multiplier`, which scales only the `would_tighten` branch
(`SignPersistenceCusumBoundary`). But the production boundary is `AdaptiveSignPersist`,
which switches on the configured rate (`spm_threshold = 6`): **below spm 6 it runs
PoissonCI**, which is *symmetric* — no `tighten_multiplier`, no directional refusal at
all (this is why the low-SPM guard exists — SignPersistenceCusum collapses at low SPM).
So at sparse rate the boundary gives *no* dangerous-direction protection; it is a
symmetric magnitude *trigger* whose threshold falls as inter-fire time accumulates.

**So the protection is regime-split — and the split is ASYMMETRIC, not two
symmetric complements** (verified by the strength sweep, `eager-ease-strength.rs`):
- **Dense rate:** the boundary's reluctant-tighten is the *active/default* protector
  — it refuses weak-evidence tighten-fires by direction (the §6 mechanism, correctly
  described *for this regime*). But it is **substitutable**: a slow-enough estimator
  covers dense too (sweeping the estimator window τ 360→2880 at dense drives the
  self-deepening fires 5→0). So the champion uses reluctant-tighten at dense because
  it is the *wobble-cheap* way to protect there, **not because it is the only way**.
- **Sparse rate:** the *estimator* is the **SOLE NECESSARY** protector — and here the
  asymmetry bites: boundary reluctance at *any* strength cannot substitute (sweeping
  the tighten-multiplier tm 8→1024× at sparse leaves the self-deepening fires flat at
  4 — regime-locked, the boundary categorically cannot cover sparse). The estimator
  carries the protection. It sets the fire direction
  (during over-difficulty the belief is *predominantly* below the stale-high operating
  point, so fires ease) and, being slow (Ewma360), has **low belief volatility** — ~8×
  lower per-tick than a fast Ewma30 (`belief-vs-op.rs`) — so it produces no large
  belief-*jumps* that clear the magnitude threshold (so even when the belief is above
  the operating point — the tighten configuration — the margin is too small and too
  slowly-changing to fire). Verified controlled comparison: at sparse rate the champion
  and a fast-estimator arm run the *same* symmetric boundary (PoissonCI, below the spm6
  switch) at *comparable* operating-point levels (median OP/true ≈ 0.31 vs 0.33), yet
  the fast arm produces self-deepening fires (4) and the slow one none (0) — the
  difference is belief volatility, not the boundary and not the operating-point level.
  *(Mechanism stated precisely, against a plausible-but-wrong version: the slow belief
  is still ABOVE the operating point ~36% of escape ticks — as often as the fast one —
  so the protection is NOT "the belief stays below the operating point" (false); it is
  "the low-volatility belief produces no threshold-clearing spike, even when above." The
  flip-above-OP enables the tighten direction but is not sufficient; the spike that
  clears the magnitude threshold is the second condition, and the slow estimator denies
  it. Recorded so this is not re-derived as the wrong "stays below" version.)*

This is the mechanism-level completion of §6: the asymmetry's *direction and existence*
stand (§6(i)), but it is **the dense-regime mechanism for a more general invariant —
dangerous-direction protection — with the estimator carrying that invariant at sparse
rate where the boundary asymmetry is structurally absent.** It also explains,
mechanistically, why rate-awareness has no admissibility content (it is a window-*size*
dial; the protection lives in the suppression mechanisms — direction-setting and
belief-volatility — not in the window length), the result §9 and the rate-aware closure
establish empirically.

![The §6.1 structural map: which mechanism carries the dangerous-direction protection
in each rate regime, and the **asymmetry** between them. Two regimes split at the spm6
boundary switch. **SPARSE (spm<6):** the boundary is a symmetric PoissonCI — a magnitude
*trigger* with no directional content — so the *estimator* (slow Ewma360) is the **sole
necessary** protector (sets fire direction + low belief-volatility); it is regime-locked,
boundary reluctance at *any* strength (tm 8→1024×) cannot substitute. **DENSE (spm≥6):**
the boundary's reluctant-tighten is the *active/default* protector, but it is
**substitutable** — a slow-enough estimator (τ 360→2880) covers dense too (fires 5→0), so
the boundary is the wobble-cheap way, not the only way. The asymmetry is the point: sparse
has a sole-necessary protector, dense has a default-with-a-substitute — NOT a symmetric
"each covers a regime the other cannot." This is a **structural map** (what-protects-where),
deliberately distinct from `tau_tradeoff.svg`'s selection plane (where the champion sits in
the admissible set). It makes no per-mechanism-curve claim — the mechanism detail (how each
protects, the categorical-vs-weak and volatility-not-OP-position precision) stays in the
prose above, where three corrections pinned it. Verified by `eager-ease-strength.rs`;
schematic, hand-authored.](figures/dangerous_direction_protection.svg)

*Three superseded mechanism stories, documented (the carry-forward).* The sparse-rate
mechanism was mis-stated three times, each refuted by tracing the actual dynamics, each
corrected to the verified version before going durable: (1) "tm absent via the ease
branch of SignPersistenceCusum" — wrong, that boundary is switched OFF at sparse
(`which-boundary.rs`); (2) "persistence-discount anti-spiral" (the guard regime) —
wrong, the guard runs PoissonCI which has no persistence-discount, the falling threshold
is PoissonCI's dt-accumulation; (3) "the slow belief stays below the operating point" —
wrong, it is above ~36% of the time, the protection is low belief-volatility not
OP-position (`belief-vs-op.rs`). Each was *plausible* and read clean; each was wrong in
its specifics. The lesson, thoroughly earned on the hardest part of the arc (the
sparse-rate mechanism, where both instrument resolution and mechanism subtlety are
worst): a mechanism that reads clean is not verified until you trace the dynamics it
claims, and the durable record should carry the verified mechanism with the
plausible-but-wrong ones explicitly marked — so the correction is not made a fourth time.

---

## 7. The metric

Per scenario class **and per share rate `r*`**, from the trajectory `{(e, fired,
s)}` alone — hence computable for every algorithm, transparent or opaque
(`LogErrorRegret`, `metrics.rs`):

```
  regret_over, regret_under  =  ⟨|e|⟩, split by sign of e            (§4)
  effort_up,  effort_down    =  Σs² and Σ|s|, split by sign of s     (below)
  [diagnostic] EXCESS(r*)    =  P[fire|drop] − P[fire|no drop]       (§5, NOT scored below ~10 spm)
```

The **vector is the primary object**; for ranking, the *production* scalar is

```
  cost = 3·regret_over + 1·regret_under
       + ρ·( (3·effort_up + 1·effort_down)_quadratic
             + λ·(3·effort_up + 1·effort_down)_linear ),     ρ = ½.
```

Two things changed from the earlier version, both forced by measurement:
**detection is no longer in the scalar** (§5 — floor-saturated at production
rates), and **effort now carries a linear `Σ|s|` term alongside `Σs²`**. The
linear term closes the dual of §4's blind spot: hold `Σs²` fixed, shrink each
step's amplitude, and raise the fire frequency, and `Σ|s| → ∞` while `Σs² → 0` —
so high-frequency, low-amplitude churn would otherwise score as "gentle," which
the quadratic term cannot see. That blind spot is real and the linear term still
closes it. What the term does **not** rest on — and an earlier version wrongly
claimed it did — is *lost work*: §6 retracts the lost-in-flight-work premise (a
retarget rejects no in-flight shares, and churn is value-neutral), so `Σ|s|` is
**not** "cumulative lost work in regret's currency." It is a **churn/usability
penalty** (frequent retargets read as errors, muddy monitoring, cost minor pool
overhead) — a real but *soft* cost, not a value cost. Consequently `λ` is **not**
anchored at `λ=1` "at face value"; it is a soft tuning weight. The quadratic term
still does its own job (penalizing overshoot and concentrated actuation — one
large retarget costs more than several small ones, `S² > k(S/k)²`, which is the
§6(i) variance/over-difficulty concern). The champion is stable across `λ ∈
{0,½,1,2}` (§9), so the deflation of `λ`'s justification does not move the result.

*Assumption (fire cadence is capped).* Real pools forbid the churn corner with a
**minimum inter-fire interval**, which the model adopts as an explicit
assumption; with the linear effort term *and* the cadence cap, the gentleness
reading of effort is honest from both sides.

*The tick interval is orthogonal to the floor — but only because the EWMA decays
per unit time.* The simulator and the shipped champion run on a fixed tick (the
`60 s` evaluation cadence); a natural question is whether that cadence is itself
a lever on accuracy. It is not: the floor is `1/√(r*·τ)` where `τ` is the
estimator's *memory* (the champion's `tau_secs = 360`), not the tick, so changing
the tick neither buys nor costs precision. What the tick *does* trade is
(i) **actuation latency** — a faster tick shaves up to one tick-period off the
worst-case detect→act delay, second-order on the modal gentle decline (60 s is
small against its timescale) but more relevant on fast declines; (ii) **churn** —
a faster tick re-opens the corner the cadence cap (above) closes; and
(iii) **downstream `SetTarget` traffic** — faster retargets cost more messages
(the bandwidth term below). The orthogonality is **conditional on the EWMA being
parameterized per unit time**, and that is verified for the shipped controller:
`α = exp(−tick_secs / tau_secs)` (`composed/estimator.rs`), so a change to the
tick recomputes `α` to hold the same continuous-time `τ` — the memory window, and
thus the floor, is unchanged. The condition is load-bearing and worth stating
because it is implementation-specific: a controller that hard-codes a fixed `α`
*per tick* would silently move `τ` (hence the floor) when the tick changes, so
the tick cannot be retuned on such an implementation without re-deriving the
floor. (We verified it for the controller this paper ships; it is a per-codebase
check, not a protocol guarantee.)

**Per class and per rate, never pooled.** Cold-start cost dwarfs steady-state
cost, and the floor moves with `r*`, so a pooled average erases every distinction
that matters. This is not bookkeeping: §8 shows the champion is selected by a
*minimax over `r*`* — best worst-case across the band — precisely because no
single rate's ranking is the answer.

---

## 8. What the figures show — and what we tried to show and could not

The metric is a vector per class per rate. The hard part is not drawing it; it is
knowing *what claim each picture is allowed to make*. This investigation killed
three candidate principal figures by measurement before any of them was
published, and that record is not an embarrassment to hide — it is the strongest
evidence that the harness was permitted to invalidate its own conclusions. We
state the casualties first, then the survivors.

### 8.1 Three premises drawn and killed

- **The floor as an estimator bound under the field.** The natural hero figure
  was "performance versus `r*`, every controller hugging the Poisson floor from
  above." It is false as drawn: the cost-blind maximum-likelihood estimator (the
  policy-free controller that fires every tick and emits its raw belief) sits
  *above* the field, not below it — a real controller *holds* difficulty between
  fires, low-pass-filtering the very noise the every-tick MLE emits. The floor is
  not a line beneath emitted-difficulty error, so the figure cannot be drawn that
  way.
- **A thin ribbon in steady-state error.** The flatness is real but does not live
  on the steady-RMS axis: there the field spreads +89–161%, and worse, that axis
  crowns *classic* (which holds an effectively enormous window and so tracks a
  stable stream tightly while failing every transient and the safety gate). The
  ~12% flatness lives in the *composite* cost across rates, not in steady error.
- **The champion as the Pareto frontier.** On a steady-vs-transient scatter the
  champion is *inside* the field's Pareto front — five configurations are cheaper
  on steady cost *and* faster on transient lag. It becomes the frontier only once
  the points are colored by safety (§8.2): all five dominators fail the
  cross-rate decline gate. The champion is the *safe* frontier, not the cost
  frontier — a weaker and more honest claim.

These three are the **figure-level** premises this paper killed in drawing its own
figures. The full **theory-level** derivation and falsification trail — the
conservation-law derivation (plant identity → information floor), and the
model-level dead ends this paper compresses out, e.g. the magnitude-cancellation
result and the refuted synthesis attempt — live in `THEORY.md`, the
derivation-and-falsification notebook. This paper states the surviving conclusions;
that notebook records how they were reached and what was tried and killed. (The two
kill-lists are *distinct*: these three are figure premises; THEORY's are model-level
premises — not the same set.)

Each premise died to the same discipline: render the measured points before
writing the caption. The figures that survived did so because they were checked
the same way.

### 8.2 The companion: the champion is the safe frontier (steady-vs-transient scatter)

`steady-transient.rs` plots every configuration at a fixed rate as a point: x =
steady-state cost (tracking + effort on a pure stable stream), y = transient lag
(cold-start ramp + aged-drop detection latency), each point colored by
decline-safety — the worst settled over-difficulty after a sustained decline,
measured over the *authoritative* rate×magnitude grid (identical to the safety
gate of §9; an earlier partial grid mis-colored the long-window family and was
corrected). Lower-left is better on both axes.

*What it shows.* A clean convex frontier exists, parametrized by the estimator
window. The champion is the **lower-left-most safe point**: the five
configurations that beat it on both axes all fail the decline gate (they are red)
— so no *safe* configuration dominates it. The safe configurations themselves
trace a convex envelope, and the champion is its gentle-steady corner. This is
the minimax-over-`r*`-plus-safety selection made visual: the champion is chosen
not because it wins the field but because it is the gentlest configuration that
is *safe everywhere*. Confirmed at 12 and 30 spm; at 4 spm the field is
window-degenerate (configurations of the same window collapse to one point, so
the neighborhood cannot be resolved there) — stated rather than implied.

![Steady cost vs transient lag at 12 spm. Each point is a configuration; green =
decline-safe (worst settled over-difficulty ≤ 5% over the cross-rate grid), hollow
red = fails the gate. The new champion (Ewma360/s1.5) is the lower-left-most green
point; the five configurations below-left of it — cheaper on steady cost *and*
faster on transient lag — are all red, so no *safe* configuration dominates it.
The dashed line is the envelope of the safe configurations.](figures/steady_transient.svg)

### 8.3 The mechanism: why the champion's window is what it is (the τ-safety-valley)

The scatter shows cheaper-and-faster points colored red with no in-panel reason.
`tau-valley.rs` supplies the reason: worst settled over-difficulty after a
sustained decline is a **U-shaped function of the estimator window τ**, with its
minimum at the champion's window. Too long a window (sleepy) lags a sustained
decline into the dangerous over-difficulty direction; too short a window
(twitchy) overshoots it; the safe band is the minimum of the dynamic floor
`L(τ_eff)` from §3. **Observation:** the curve is *sensitivity-invariant* — worst
settled error is identical to 0.1% across boundary sensitivities `s∈{0.3…2}`,
because settled error after recovery is an estimator-window property, not a
firing-threshold one (a per-fixed-sensitivity sweep controls the window×threshold
confound: the floor sits at the champion's window at *every* fixed sensitivity).
This converts "we picked this window" from a selection outcome into a visible
physical reason.

> **Scope correction (measured — `slow-decline.rs` ckpool gate test; see
> CKPOOL_INVESTIGATION.md gate-test addendum).** This invariance is across
> *sensitivities of one boundary family* (SignPersistenceCusum, s∈{0.3..2}); it does
> **NOT** extrapolate across boundary *type*. At τ=120, spm6–8, settled-e is **−13%
> under SignPersistenceCusum** but **+1.8% under AdaptivePoissonCusum** — a ~15pp
> boundary-type effect. So "worst-settled is an estimator-window property" holds
> *within a boundary family* (the claim above stands as stated, scoped to it), but
> the property is **sensitivity-general, not boundary-type-general**. Consequence: a
> τ-prediction may NOT be extrapolated "to any boundary" — only within the family it
> was measured in. (The settled-e SIGN even flips across boundary type at fixed τ,
> which a pure-window-property reading would forbid.)
>
> **And the consequence for THIS section's other content — the valley numbers are
> boundary-specific (do not read them as boundary-general), but they ARE the
> champion's-exact-boundary numbers.** Everything in §8.3 — the U floored at 360, the
> +2.7/+3.5/+5.9 settled gradient across 360/240/120, the τ*∝1/r slide — was measured
> (verified: `tau-valley.rs`/`tau-family.rs`/`tau-family-safety.rs` cfg) under the
> **rate-split `AdaptiveSignPersist(…,6)`** — SignPersist above spm6, **PoissonCI below**
> — i.e. the *champion's production boundary*, PoissonCI included at exactly the sparse
> cells where the gate binds. Finding 2 establishes the valley's *quantitative shape is
> boundary-type-specific*: a *different* boundary gives a different gradient (the same
> τ=120 that sits at +5.9% here sits at +1.8% under AdaptivePoissonCusum at spm6–8 — the
> flank shifts, the crossing relocates). So the valley's numbers characterize *the
> champion's boundary*, not a universal τ-cost law. This does NOT weaken — it
> *strengthens* — the production selection: the 360-floor and +2.7%-passes were measured
> under the boundary the champion actually ships (sparse cells = PoissonCI included), so
> the selection and the characterization are mutually consistent. Only the *generality*
> across boundary TYPES narrows (a hypothetical PoissonCusum-everywhere controller would
> have a different valley — but that is not what ships). The dual-boundary τ-sweep
> (eligible since the ckpool crossing) would map the PoissonCusum valley for
> completeness; it is NOT a consistency gap in the champion's 360-selection, which is
> already boundary-consistent.

**Flagged — the safe window is a minimax over rate; the per-rate optimum moves
(τ\*∝1/r, MEASURED), and the fixed window is the two-primitive balance, not a
rate-invariant optimum.** Keep the *result* and the *interpretation* separate:
the first is permanent, the second is the constraint-not-cost call.

*The minimax vs the per-rate question.* The U above is `worst-settled-e` over the
*worst cell of the whole rate×spm grid* (`tau-valley.rs`: minimax over rate), so
its minimum is the single window safe across the *whole band* — the right object
for an **admissibility** claim, and that claim is solid: fixed `tau_secs=360`
stays decline-safe everywhere (worst settled **+2.7%** vs the 5% gate). What the
minimax collapses, by construction, is whether the *per-rate* optimum moves.
THEORY eq. (2) predicts it does (`T_d ∝ 1/(r*·δ²)` ⇒ `τ* ∝ 1/r`), and the
champion is fixed `tau_secs=360`, **time-indexed and unscheduled** (source:
`α = exp(−tick_secs/tau_secs)`).

*RESULT (durable, measured — `tau-family.rs`).* Keeping the rate axis the minimax
discards, the per-rate over-difficulty-area valley minimum **slides monotonically
with rate**: argmin `τ*` = 240 → 150 → 45 → 30 across spm 2 → 30 (bracketed after
extending τ down to the 30 s tick floor; a first pass railed at the grid edge).
This is `τ*∝1/r` made literal. It is robust to both confounds: to the
clamp-magnitude confound because it reads argmin *positions*, not depths; and to
the `spm_threshold=6` guard switch because the slide holds *within* each control
regime and *across* it. This is the result a future reader should treat as
permanent — it survives every narrowing below.

*INTERPRETATION (what the slide licenses about the champion — a two-primitive
trade, not a one-axis verdict).* `τ*` minimizes *one* primitive (over-difficulty
area, the escape-speed cost). The champion was selected on the gate *plus both
primitives*. The admissibility check (`tau-family-safety.rs`) is decisive: at the
per-rate optimum `τ*=30`, every high-spm cell **passes** the decline-safety gate
(worst settled-e negative) — but carries **2–3× the under-difficulty wobble** of
the champion (−14…−23 % vs −6…−8 %). So the fixed 360 is **over-damped on the
over-difficulty axis and correctly-damped on the wobble axis** — it is the
two-primitive balance, not a mistake. (`tau_family.svg`, two panels: short τ wins
the left/over-difficulty axis, loses the right/wobble axis.)

*The asymmetry the trade runs along (a structural fact, not a weighting).* The
two axes are **not symmetric**, and this is the spine of the whole paper (§6,
§9.2): over-difficulty is the self-reinforcing, spiral-direction cost the
controller exists to bound; wobble is self-healing under-difficulty (more shares
→ fast correction). So moving to shorter τ at high rate trades the **dangerous**
axis *down* for the **safe** axis *up* — the same direction the
eager-ease/reluctant-tighten asymmetry already commits the design to, with a
bounded, self-correcting downside (the wobble is safe-side, so its tolerability is
the kind of thing an operator can judge per-deployment, whereas the over-difficulty
it relieves is not something to leave on the table). Whether that **favorable
direction** is net-positive is the *weighting call* — deliberately not asserted, and
resolved to policy-only by the rate-aware closure (see Net status).

*But where on the band is the trade worth taking? — opposite to the raw ratio.*
The over-difficulty magnitude is ≈10× at spm30 (1060 vs 81 e-min at τ=360 vs
τ=30) — real, though partially clamp-confounded, so *indicative of scale*, not a
pinned number. Critically, that ratio is **largest where the absolute stakes are
smallest**: at high spm the absolute over-difficulty is already tiny (area 0–36
at τ=30), so the 10× is 10× of not-much, bought with a *large* (2–3×) wobble
increase — the *worst* place to take the trade. Where shaving over-difficulty
actually matters is the sparse, low-rate, spiral-prone regime, where the absolute
over-difficulty is large — but that is below the `spm_threshold=6` guard switch, a
different controller. So the trade's *attractiveness* is itself rate-dependent and
runs **opposite to the raw ratio**: a reader who sees "10× at spm30" must not
conclude high rate is where share-indexing pays — it is the regime where the trade
is least worth its wobble.

**Net status:** §8.3's "safe window floored at the champion's window" is correct
as an **admissibility/minimax** statement and stands. But the *gentlest* window is
**rate-dependent (τ\*∝1/r, measured)**, and the fixed 360 is the balance of a real
two-primitive trade — over-damped for over-difficulty, correctly-damped for
wobble. A **share-indexed** span (a window in shares, not seconds) is the natural
rate-aware **rebalancing** (not a *fix* — 360 is the balance, not a defect) that
makes `τ*` rate-invariant in the unit that matters; adopting it would move toward
the favorable (dangerous-axis-down) direction at a safe-side wobble cost whose net
value is weighting-dependent and therefore deliberately not asserted here. This is
no longer "weakly observed" — the slide and the trade are both measured
(`tau-family.rs`, `tau-family-safety.rs`). What remains open is only the *weighting
call* — and the **rate-aware closure has since settled that this weighting call is
the *entirety* of share-indexing's question**: rate-coupling has **no admissibility
content in either regime** (§6.1; the deployment-coupling and guard-spiral sweeps
established both regimes self-correct — dense via escape-sub-spiral so no gate binds,
sparse via the estimator-carried recovery — neither a safety axis). So a
share-indexed window is a **policy/wobble dial with no safety content**, not a live
open question; the constraint-not-cost discipline leaves that policy call to the
operator, and there is **nothing further to *measure***.

![Worst settled over-difficulty (over the cross-rate decline grid) versus
estimator window τ. The curve is a U floored at the champion's window (τ=360,
ringed): both flanks — sleepy long windows that lag a sustained decline, twitchy
short windows that overshoot it — rise above the +5% runaway gate (dashed). The
green band is the safe region. The valley is sensitivity-invariant (identical to
0.1% across boundary sensitivities s0.3–s2), so it is a genuine window effect, not
a window×threshold confound.](figures/tau_valley.svg)

The minimax-U above carries the *admissibility* half (one window safe across the
whole band). The per-rate *family* below carries the other half the flag
distinguishes — the slide and its trade.

![Per-rate τ-family, two panels. **LEFT (over-difficulty area, the
dangerous/spiral axis):** one U-curve per share rate r\*; the ringed minimum
slides left (`argmin τ*` 240→150→45→30) as r\* rises — `τ*∝1/r` measured. **RIGHT
(wobble, the safe/self-healing axis):** the *same* short τ\* that wins the left
panel *loses* here (the champion's 360 sits at 2–3× lower wobble). The two axes
are **not symmetric** — over-difficulty self-reinforces (the spiral the controller
exists to bound), wobble self-heals — so short τ trades the dangerous axis down
for the safe one: a favorable *direction*, net value left to policy. Log-τ axis;
spm<6 vs ≥6 differ by the guard switch. Generated by `tau-family.rs` /
`tau-family-safety.rs`.](figures/tau_family.svg)

### 8.4 The lever: raising `r*` buys agility (EXCESS vs `r*`)

The structural claim's other half — *the one lever that moves the floor is `r*`*
— is carried by `excess-lever.rs`: false-alarm-corrected detection `EXCESS` of a
−10% drop versus share rate. The honest object is detection-excess, **not** the
composite cost (which is non-monotone in `r*` and so cannot show a clean lever)
and **not** the stable-safe-window result (which is a *robustness* claim about
the champion, not a claim that `r*` buys anything). `EXCESS` climbs monotonically
from `+0.05` at 4 spm to `+0.75` at 60 spm, and the whole field is bunched near
the floor at the low end — the floor binds *everyone* at production rates and
recedes for *everyone* as `r*` rises.

*Caption obligation, because "floor-limited" is window-dependent.* At production
rates a −10% drop is at-or-near the detection floor: `EXCESS = 0.00` at a 60-min
monitoring window (the saturation finding — the drop is perfectly invisible) and
`+0.05` at a 15-min window (near-floor). Both numbers must appear or the +0.05
reads as contradicting the ≈0 saturation result, when in fact they are the same
finding at two window lengths.

![False-alarm-corrected detection EXCESS of a −10% drop (15-min window, false-alarm
control held at the same window) versus share rate r* (log axis, window fixed). The
champion's EXCESS climbs monotonically from +0.05 at 4 spm — inside the shaded
production band, where the drop is at the information floor — to +0.75 at 60 spm as
the floor recedes; the field (faint) is bunched near the floor at low r*, so the
limit binds *everyone* at production rates. At a 60-min window the production EXCESS
is 0.00 (the drop is perfectly invisible); the +0.05 here is the same finding at the
tighter window.](figures/excess_lever.svg)

### 8.5 The trajectory, demoted

The single-timeline trajectory plot (`trajectory-plot`) — estimate chasing truth
through cold-start, settle, and an aged drop, with a fire-raster showing
"gentle versus violent" — is kept only as a §8 supporting detail. It is *not* a
principal figure: it shows absolute behavior at one rate, where "slow" is a
property of the rate, not the controller, and it makes the champion's deliberate
gentleness look like a regression. Its one genuine virtue is the fire-raster
(many short marks for the champion versus a handful of tall ones for classic);
that virtue does not earn it the opening of the paper.

![Supporting detail (NOT a principal figure — see the demotion above): a single
timeline of one decline at one share rate. The estimate (line) chases truth through
cold-start, settle, and an aged drop; the fire-raster beneath shows *when* each
controller retargets — the champion's many short marks (frequent small partial
steps) versus classic's handful of tall ones (rare large jumps). Read it for the
fire-raster's gentle-vs-violent contrast only; the absolute "slowness" here is a
property of the single rate shown, not of the controller. Generated by
`trajectory-plot.rs`.](figures/trajectory_plot.svg)

### 8.6 Where to set `r*`: the optimization frame (a band, not a number)

The lever (§8.4) and the bandwidth cost (§6) together pose an obvious question —
*what is the ideal `r*`?* — and the honest answer has the same shape as the harm
account: a structured frame, not a scalar. The trade is well-posed. Raising `r*`
**monotonically lowers the variance floor** as `1/√(r*·τ)` — pure benefit, but
with diminishing *returns* (the √ means the 10th doubling buys far less floor than
the 1st). Against it, bandwidth and pool-side message processing rise **linearly**
as `r*·N` (§6). A `√r*` benefit against an `r*` cost is a classic knee: a region
where you still buy meaningful floor cheaply, and a region past which you pay
linearly for √-shrinking gains.

But the knee is **not a universal number**, for exactly the reason the harm was
four currencies and not one: the benefit is in variance (statistical), the cost is
in bandwidth (a pool-scale resource), and whether a given floor-reduction is
*worth* a given bandwidth depends on the fourth currency the analysis refused to
collapse — how much a deployment's miners and pool actually care about payout
variance (payout scheme, miner size, bandwidth headroom). A large pool with fat
pipes and small-miner PPLNS has its knee at high `r*`; a bandwidth-constrained pool
with industrial miners on FPPS has it at low `r*`. Same curve, different knee,
because the variance↔bandwidth *exchange rate* is a deployment fact. So "ideal
`r*`" is well-defined *for a given pool*, and genuinely undefined in the abstract.

What *is* universal is the **bracket**, and it has four regions, each with a
different kind of cost — not one threshold but a structured axis:

- **below ~2–4 spm — the control floor.** Decline-safety itself degrades here (the
  sub-guard cells, §9.2): the genuine "don't go here," control-limited and
  near-universal. This is the band's true bottom.
- **~4–10 spm — safe and tracking, but detection-saturated.** The controller still
  tracks and stays decline-safe, but detection is floor-saturated (§5) — blind to
  *small* drops. This is not broken; it is a *knowing* trade-off, and it is exactly
  where **production runs (4–6 spm, §9.4)**: above the hard floor, below the
  detection threshold, the diagnostic blindness accepted for the bandwidth saved.
- **~10–30 spm — full diagnostic visibility.** The floor falls with `√r*`,
  bandwidth rises linearly; **the knee lives in here**, pool-chosen.
- **above ~30 spm — √-returns spent.** Each bandwidth doubling buys a
  barely-perceptible floor improvement; only an extraordinary variance premium
  justifies it.

![The r* optimization frame (closed form, no sim run): benefit = variance-floor
width 1/√(r*·τ), falling fast then flattening (the √); cost = bandwidth ∝ r*,
linear and never flattening. Four regions bracket the answer: control floor
(~2–4 spm, decline-safety degrades), safe-but-detection-blind (~4–10, where
production runs at 4–6 per §9.4), full-visibility knee region (~10–30, the knee is
the pool's call), and √-returns-spent (>~30). The y-axis is relative (shape, not
magnitude) — this is a frame, not a number. CRITICAL: the curve crossing near ~10
is NOT the knee; the knee is where marginal floor-reduction stops being worth
marginal bandwidth, which is the pool's exchange rate, not a geometric coincidence
of two normalized curves. Generated by `rstar-frontier.rs`.](figures/rstar_frontier.svg)

So the deliverable is not "the ideal rate is X spm" — it is the **method**: the
floor-vs-`r*` curve, the bandwidth-vs-`r*` curve, the 10–30 knee bracket as the
universal part, and the one input the pool supplies (its bandwidth budget *or* its
variance target) that picks the point inside. A single recommended number would be
wrong for most pools; the frame is right for all of them.

One trap, the same one in a new hat: the clean move would be to set the variance
target equal to "where controller harm stops mattering" and back out a single
ideal `r*`. Resist it — "where variance stops mattering" is the
operational/payout-dependent judgment the analysis explicitly *could not price*
(§6), so feeding it back to manufacture a single rate would launder a
deployment-specific value through the math and present it as universal. The seam
stays visible: the math gives the curve and the bracket; the pool supplies the one
number that picks the point; the answer is always "ideal *for you*, given *that*,"
never "ideal, full stop."

---

## 9. Selection, safety, and validation

### 9.1 How the champion was selected (minimax over `r*`)

The target share rate is a static deployment parameter whose value is not known
in advance, so the champion is chosen by a **minimax over `r*`**: the
configuration whose *worst* gap to the per-rate best-in-field, across `r* ∈
{4,6,12,30}` (60 spm as a high-rate anchor, outside the minimax), is smallest.
`sweep-minimax.rs` scores the corrected metric (§7) at each rate independently —
so each configuration's own per-rate false-alarm behavior enters through the
stable-scenario effort term, with no false-alarm convention reused across rates.

Three findings, all consistent with §3. The field is **flat** (~12% best-to-worst
at every rate — Theorem 2 again; the flatness is confirmed at high trial count by
`confirm-champions`, which re-ran the big-sweep top-100 region and found rank 1 to
rank 100 within ~3%, i.e. statistically indistinguishable). The cost-optimal configuration **walks with the
effort weight `λ`**, drifting toward the long-window "sleepy" corner as firing is
penalized more — so a configuration crowned by cost alone would be free-tuned by a
weight grounded only to within a factor. And the band-optimal cost lands in the
same sleepy corner the single-rate search did, so **the decline-safety gate is
the actual selector**, not the cost.

### 9.2 The decline-safety gate (the death-spiral test)

A sustained decline drives `e` positive (over-difficulty), the costly direction;
the death-spiral risk is self-reinforcing starvation. `slow-decline.rs` runs rate
∈ {1–40} %/hr × spm ∈ {2–30}, gating on the *settled* error after a 120-min
recovery window (not the transient trough). The gate is the selector among the
near-tied band-optimal configurations, and it is **uninherited**: a sleepier
easer is a different animal on a decline, so safety is re-cleared from scratch for
each candidate.

**Result.** Among the λ-robust band cluster, **only the champion `Ewma360/s1.5`
has zero runaway cells** (worst settled +2.7%); every sleepier configuration that
beats it on band-cost fails at the sub-guard 2–4 spm cells (settled +5.6% to
+9.6%) — the rates a single-rate or 12-spm view cannot see. The classic incumbent
fails hardest (settled +22%, transiently +109% — a starved miner). The
death-spiral risk is the *incumbent's*, not the candidate's, and the gate
confirms rather than re-selects the champion: the configurations that beat it on
cost are exactly the ones that are unsafe.

§9.2 above establishes the champion is *admissible* — zero runaway cells. The
figure extends that into the full picture: the admissible *island* that result
defines, where in it the champion sits, and why — the gate walls the island,
wobble places the champion within it.

![MINIMAX over rate×spm — the selection in one plane. The estimator window τ is a
path (jumpy←→sleepy). Over-difficulty area (dangerous, U-shaped) GATES — it walls
BOTH ends of the admissible island: sleepy windows lag the decline; jumpy windows
are too noisy at their worst-case sparse rate (jumpy mechanism inferred from the
minimax structure, not proven by this figure). The island is the set of
zero-runaway configs §9.2's gate result defines. Wobble (safe) falls sleepy-ward
across the band and only ORDERS within the island — it does not wall. So the gate
gives the island; wobble gives the champion's place in it: τ=360 is the gentlest
of the admissible configs (the sleepy edge), INTERIOR not extremal — admissible
by the gate, selected within by wobble. Generated by
`tau-tradeoff.rs`.](figures/tau_tradeoff.svg)

Two cross-references the figure carries but the caption leaves to here: (i) the
over-difficulty minimum sits *left* of the champion (at τ≈240) — a jumpier window
would lower over-difficulty but climb the wobble gradient, which is the trade
§8.3's flag scopes; (ii) τ=30 reads HIGH in this minimax = worst-case across
rates, NOT "τ=30 is bad" — per-rate it is *optimal* at high spm, the slide §8.3's
`tau_family.svg` shows. This figure is the fixed-window (minimax) half; that one
is the per-rate half.

`tau_tradeoff.svg` (this figure) is **consistent** with the closed theory — the
gate-walls-the-island / wobble-orders-within / champion-as-gentlest-admissible
geometry survived the arc (see `FIGURES-STATUS.md`). The *other* structural figure,
`constraint_space.svg` (the validity plane), carries **superseded framing** — it
makes asymmetry the validity axis, which §6.1 refines (validity is dangerous-direction
protection; asymmetry is the dense-regime mechanism for it). Flagged in the SVG source
and `FIGURES-STATUS.md`; not reused as-is.

### 9.2a The cost is interior, not at the boundary — the disconnect region is empty at realistic liveness

The gate above measures the *over-difficulty* failure (settled `e` vs the 5%
gate) — the controller talking itself blind. There is a second, distinct failure
a pool could care about: *disconnect* — the share rate falling so far that a
liveness timeout drops the miner, `min_t(r_obs/r*) < θ` for a liveness floor `θ`.
The two live on the same `(depth, rate)` plane; the cost model's central claim is
that the first-order cost is the **interior** over-difficulty ride (variance +
diagnostic blindness), *not* the **boundary** disconnect tail, which a generous
operator timeout mutes.

We can check this directly, because the disconnect trough *is* the over-difficulty
surface in other units: `e = −ln(r_obs/r*)`, so a trough of 0.80 is `e = +22%`
over-difficulty. Instrumenting `min_t(r_obs/r*)` over the decline window
(`bin/phase-portrait`) for the structural-deadband archetype, at the modal
declines a real miner experiences (gentle, 2–5 %/hr):

| rate %/hr | trough | over-difficulty `−ln(trough)` | past 5% gate? | below θ=0.36 (drops)? |
|---|---|---|---|---|
| 2 (modal) | 0.92 | +8.3% | **yes — interior fail** | no — boundary clear |
| 5 (modal) | 0.80 | +22% | **yes — interior fail** | no — boundary clear |
| 10 | 0.60 | +51% | yes | no |
| 20–40 | 0.50 | +69% | yes | no |

At a deployment-plausible liveness floor (the essays' θ=0.36), **the disconnect
region is empty over the entire modal-decline cloud** — no cell drops — while the
over-difficulty region is squarely *inside* it. This is the cost model's interior-
not-boundary claim arrived at by a second road: the danger lives in the interior
the modal declines actually visit, and is **invisible at the liveness boundary a
pool would notice**.

The two regions are not two findings — they are **two level sets of one surface**.
The over-difficulty contour (where `e` crosses the 5% gate) and the disconnect
contour (where the trough crosses θ) are cuts of the *same* `min_t(e^{−e})`
surface at different heights: the gate sits at `e=+5%` (trough ≈ 0.95), θ sits far
below at 0.36. "Interior failure, boundary clear" is then just the statement that
**the modal-decline cloud lives in the gap between those two heights** — high
enough to cross the gate, not low enough to reach θ. Stated that way the result is
inevitable rather than coincidental: of course the disconnect region is empty
where the over-difficulty region is not — it is the same measurement read through
two thresholds, and the modal cloud sits between them.

A depth-axis sweep (`bin/phase-portrait`, depth = fraction of hashrate lost, run
independently of rate) makes this **exact** for the deadband, because during a
decline it is a *no-op*: it does not retarget, so observed rate just tracks the
raw loss, `r_obs/r* = H/Ĥ = 1 − d`. One identity then generates both surfaces —
`settled-e = −ln(1 − d)` — and the two thresholds become two depths:

- over-difficulty crosses the 5% gate at depth **d ≈ 5%**;
- disconnect crosses θ=0.36 at depth **d = 1 − θ ≈ 64%**.

The modal cloud (≈8–20% loss) sits in the gap `(5%, 64%)`: past the gate, far from
the liveness floor. The over-difficulty crossing is a **vertical line in depth,
rate-independent** — and the verticality is not an observation that happened to
come out straight, it is a *consequence of the controller being inert*: a no-op
has no rate-dependence to give the boundary any other shape, so rate drops out of
the failure boundary precisely because nothing acts on it. The geometry and the
mechanism are one fact. This is the failure-as-half-plane-in-depth that
distinguishes the structural-deadband archetype from the champion's corner (the
champion *reacts*, so its trough stays well above `1 − d` and it crosses neither
threshold on this slice; its only failure is the sparse-rate +2.7% of §9.2, off
this evidence-rate slice).

A bounded-decline caveat, stated because it is real and then bounded because it
is orthogonal: a sustained decline reaches deep loss in bounded time only above
~20%/hr (a 4h cap; at 2%/hr you lose 8% in 4h regardless of the target depth), so
the slow-and-deep corner is *unreachable in a bounded decline* — not separately
safe, and not to be read as an empty "safe region." But this costs the result
nothing, because the half-plane claim is *about depth alone* and is read where
depth is genuinely free — the cap-free fast columns. The cap obscures only the
slow-deep corner, the one region a depth-determined failure boundary does not rest
on. The limitation is real and orthogonal: it would bite a claim about the
slow-deep corner, which this is not. Spec and surfaces:
`records/PHASE_PORTRAIT_SPEC.md` §10. A generous timeout doesn't fix the flaw; it hides it
(safety-by-config, not safety-by-design). The empty disconnect region at realistic
θ is therefore not an absence of a result — it *is* the result.

(Reaching the disconnect boundary requires tightening θ well past any timeout a
pool actually uses; doing so to populate the picture would be fabricated
precision — a region that exists only at an unrealistic config. The honest
statement is the one above: at realistic θ the boundary is clear and the interior
is not. Spec and the geometry of both archetypes: `records/PHASE_PORTRAIT_SPEC.md`.)

![The phase portrait on the (rate × depth) plane, sliced at spm=6. LEFT
(structural-deadband): because it is a no-op during the decline, its two
boundaries are the CLOSED FORM e=−ln(1−d) — the over-difficulty gate at depth
≈5% and the disconnect floor at depth=1−θ≈64% are horizontal (vertical-in-rate)
lines, the half-plane-in-depth drawn straight because nothing acts to bend it.
The modal declines (≈8–20% loss) sit in the amber gap between them: over-difficulty
past the gate, miner not dropped. RIGHT (champion Ewma360): a MEASURED trough
heatmap — it reacts, so the surface must be traced, not derived; the blue deepens
down-and-right into a corner but crosses neither threshold on this slice (its only
failure is off-slice, the sparse-rate +2.7% of §9.2). Hatched: the slow-and-deep
region unreachable in a bounded (≤4h) decline — NOT a safe region; the half-plane
is read where depth is genuinely free. The figure ILLUSTRATES the closed form, it
is not the evidence for it — generated by `phase-portrait.rs`.](figures/phase_portrait.svg)

The portrait shows *where* the failure is; its twin shows *how bad* it is there,
from the same identity. Relative payout variance over a window goes as
`1/(r_obs·τ)` (Poisson share count, §1/§3); on-target that is the floor
`1/(r*·τ)`, and over-difficulty pushes `r_obs → r*·trough`, so the variance
inflates over the floor by exactly `1/trough`. For the deadband that is the
closed form `1/(1−d)` (the no-op identity once more — it never retargets, so
observed rate is the raw loss); for the champion it is `1/trough`, measured.

![Payout-variance inflation (over the on-target floor) versus decline depth, at
spm=6. The deadband (red) blows up as the closed form 1/(1−d) because it does
nothing during the decline — ~1.25× at 20% loss, ~2× at 50%, ~5× at 80%. The
champion (blue; solid 20%/hr, dashed 40%/hr, the cap-free columns) stays near 1×
— ≤~1.4× even at deep/fast declines — because it reacts. The inflation is over
the floor itself, so this is the harm *on top of* the irreducible `1/(r*·τ)` the
bandwidth lever sets (§6): the floor is where everyone starts, this is how far the
controller pushes you above it. Generated by `phase-portrait.rs`.](figures/variance_inflation.svg)

This is the variance currency of the harm account (§6's four-currency picture)
made concrete: it prices being *wrong* (holding difficulty too high), the harm the
controller actually controls — steep for the deadband, flat for the champion —
and it is over the floor, so it stacks on the irreducible noise rather than
replacing it (the lever lowers the floor; the controller decides how far above it
you sit).

### 9.3 The gate is a constraint, not a weighted objective

Decline-safety enters as a **hard constraint satisfied across all plausible
failure magnitudes**, not as a weighted term calibrated to a failure-magnitude
distribution — and this is forced, not stylistic. The magnitude at which the
responsiveness gate would bind is **firmware/config-mix determined and
operator-movable**: an operator can classify miners into similar-profile proxies
and *normalize* the per-proxy distribution, flattening the very magnitude a tuned
controller would target. The pool operators, asked, **do not know** the current
distribution. A controller tuned to a distribution that is both unknown *and*
actively homogenizable would be optimizing against a target that moves out from
under it. So the gate stays a footnote: the champion satisfies it everywhere
(which is how it was selected), and "the distribution is unknown and
operator-movable" is a stronger reason not to tune to it than any measured weight
would have been.

### 9.4 What real hardware validates — and what it does not

Everything above is derived from a model and scored in simulation. The model's
*behavioral* predictions were checked against real miners — an Antminer S21 (~200
TH/s) on testnet4, driven through the **shape-proxy** tool against side-by-side
SRI pool instances.

**The classic algorithm's mechanics reproduced quantitatively:** steady-state
jitter zero over 30+ min; deterministic −16.7% per fire; exact 300 s cadence;
~60% post-staircase overshoot; ±50% symmetry. Most important, the
**counter-age dependence** the §5 mechanism rests on: a 5-min counter reacted in
4.4 min, a 51-min counter in 51.8 min — the matured-counter blindness, seen in
hardware. And a **previous** champion's win reproduced live: deployed beside
classic, both matured overnight, both miners halved at once — classic took hours
and first moved in the *wrong* direction; the champion responded in minutes and
settled correctly (the §5 detection and §6 wrong-direction claims, outside
simulation).

**The hardware re-confirmation — what now holds, and what is still owed.** That
first hardware test was run on the *previous* (`s0.3`) champion. The present
champion `Ewma360/s1.5` is a simulation re-selection under the corrected metric
(detection removed from the scalar, linear effort added, minimax over `r*`,
decline-gate-as-constraint). What transfers by construction is the **mechanism** —
counter-age blindness, the runaway direction, the gentleness/safety trade — all
architectural and rate-driven, not parameter-specific. The specific
parameterization's live behavior does not transfer, and was tested directly.

On a pool-only deployment (so the champion's `VardiffState` governs the miner with
no translator vardiff in the path), a sustained 50% hashrate drop at 6 and 30 spm
produced the §9.2 signature on iron: the difficulty-implied hashrate **eased
downward** to follow the decline — the safe direction, not the death-spiral — with
shares flowing throughout and **no rejection spike** during the drop (the cleanest
tell: a runaway would show rejections climbing as difficulty stayed too high for
the reduced hashrate; there were none). This reproduces for `Ewma360/s1.5` what
PR #2154 showed for the previous champion: a direction-correct, no-starvation
response to a sustained loss.

Two parts of the safety claim remain simulation-only, and the distinction matters:

- *The settled figure, not just the direction.* The dashboard shows the implied
  hashrate easing down and zero rejections — qualitatively safe — but not the
  settled implied-H/true-H ratio the sim reports as `+2.7%`. The run was also
  ~48 min, short of the sim's 120-min settle window: what is confirmed is *easing
  correctly*, not the final settle point. The quantitative match needs
  per-decision logging (pool `vardiff=debug`) over a longer drop.
- *The gate-stress decline, not just the easy one.* A 50% drop is the *large, fast*
  signal — the one every configuration catches quickly (§9.1, the −50% gate was
  non-binding). The death-spiral risk the gate actually binds on is the *slow,
  moderate* decline (the 1–40 %/hr sweep, board-shedding magnitudes ≈25–33%), where
  a sleepy controller lags into over-difficulty. That regime is confirmed in
  simulation only; the hardware test exercised the easy direction, not the gate's
  binding corner.

So the scope is precise: *the present champion's decline response is
hardware-confirmed in direction and starvation-avoidance on a sustained 50% drop;
its settled over-difficulty and its behavior on a slow moderate decline remain
simulation results.* The remaining open hardware tests are a slow moderate decline
on iron with `vardiff=debug` for the settled number, multi-connection operation
(the model assumes one worker per connection), and measuring `c` to close the §6
share-volume term. Production runs at `r* ≈ 4–6` spm with headroom, and the model
supports running faster: a higher `r*` tightens both the detection floor and the
estimate, at a share-volume cost the headroom absorbs.

---

## 10. One consistency check, and how to break the result

**The offset is optimal, not a defect (`confirm-debias`).** The champion sits at a
steady under-difficulty offset, short of the noise-band floor a policy-free
estimator reaches by firing every tick. Multiplying its belief by `b ≥ 1` closes
the offset smoothly, but the cost rises monotonically from `b = 1`:
`regret_under` falls while `regret_over` rises faster under the `3:1` weight. The
unbiased belief is the cost minimum, so the offset is not an error — exactly as §3
predicts. What it *is*, precisely: under the `3:1` asymmetry the cost-minimizing
center of the noise band is not its mean but a quantile below it, `≈ −0.67·σ_eff`;
the `3:1` weight fixes the coefficient and sign, `σ_eff` is set by the window
choice. `confirm-debias` verifies the *quantile condition* (`b=1` minimizes
cost), not the band *width* — so an independent knob that *could* push accuracy
toward the floor is correctly scored *worse*. The metric is self-consistent.

**Falsifiers.** The result should be revised if: (a) some `∫f(e)` on the scored
scenarios reproduces the detection ranking, refuting the §5 Argument; (b) an
*unbiased* estimator beats `1/(r*τ)`, refuting Theorem 2 (biased estimators
routinely beat the CRB on variance, so the qualifier is essential); (c) the
champion changes within `w_over:w_under ∈ [1:1,4:1]` or across `λ ∈ {0,½,1,2}`,
breaking the §6/§7 robustness; or (d) a real failure mode falls outside the
scored ensemble — a coverage gap to declare, not a soundness error. Three further
premises *did* fail and were retired (§8.1); that they were drawn, measured, and
killed is the mechanism by which the survivors earn trust.

**Declared coverage gap (d): the sub-guard boundary is asymmetry-blind — by
design, and the estimator covers it.** Below spm6 the guard is a symmetric
PoissonCI, so the *boundary* provides no directional (reluctant-tighten)
protection where data is sparsest. This is **not** an abandonment of safety: §6.1
established that at sparse rate the dangerous-direction protection is
**estimator-carried** (the slow Ewma360 sets fire direction and, via low
belief-volatility, produces no threshold-clearing self-deepening spike —
`belief-vs-op.rs`), and §9.2's gate confirms the champion has **zero runaway cells
including the sub-guard 2–4 spm cells (+2.7%)**. The symmetric boundary
self-deepens at sparse rate only with a *fast* estimator (`eager-ease-mechanism`:
4 fires at Ewma30 vs 0 at the champion's Ewma360) — the champion's slow estimator
is exactly why the sub-guard is safe. What *is* genuinely degraded at sparse rate
is **accuracy** — the steady-state offset is lost inside the §3 noise band (σ≈45%
at 2 spm) — but that is an accuracy point, not a safety one, and it stands on a
2–4 spm tail alone. The named option (`AsymmetricPoissonCI`, in the codebase) is
therefore **optional hardening / defense-in-depth** — boundary-level directional
protection, redundant with the estimator's *for the champion's slow estimator*
(load-bearing only with a faster estimator, the 4-fires case) — **not a safety
fix**, and it is *deferred*: taking it reopens champion selection at the margin and
owes a spm≥6 re-confirmation, a bad trade for cells the estimator already covers
safely. The trigger that would reopen **AsymmetricPoissonCI specifically** (not the
sub-guard concern generally — the accuracy degradation surfaces on the tail alone):
real connection-rate data showing a non-trivial tail at 2–4 spm *and* a reason to
run a faster estimator there, the only regime where boundary-level sparse asymmetry
would add protection the slow estimator isn't already providing.

---

## 11. Status of each claim

| Claim | Kind | Source |
| --- | --- | --- |
| Observable depends only on `e` | Theorem 1 | §2 |
| Precision floor `1/(r*τ)` | Theorem 2 | §3 |
| Quality axes are one trade-off | Corollary + obs. | §3, `31a9dbc1` |
| Field is flat across `r*` (~12% spread in composite cost) | Observation | §8, `sweep-minimax` |
| Squared norm blind to small drops | Lemma | §4 |
| Detection not derivable from `e(t)` | Argument | §5 |
| Detection floor-saturated at production rates → out of scalar | Observation | §5, `detection-control` |
| Detection EXCESS rises with `r*` (the lever) | Observation | §8, `excess-lever` |
| Over>under, eager-ease/reluctant-tighten — direction safety-justified (§6(i)) | Argument | §6 |
| Validity axis is DANGEROUS-DIRECTION PROTECTION, not asymmetry; asymmetry is one mechanism for it | Reframe (item 1) | §6.1, `eager-ease-*` |
| Fire DIRECTION = sign(h_estimate − OP) (estimator's), threshold is direction-blind magnitude | Source-verified | §6.1, `composed.rs` |
| Reluctant-tighten is the DENSE-rate protector; switched OFF at sparse (PoissonCI, symmetric, via spm6 guard) | Source-verified | §6.1, `which-boundary.rs` |
| Sparse dangerous-direction protection is the ESTIMATOR's: direction-setting + low belief-volatility (no threshold-clearing spike); NOT "belief stays below OP" (false, above ~36%) | Verified (controlled) | §6.1, `belief-vs-op.rs` |
| The regime split is ASYMMETRIC: estimator is SOLE-NECESSARY at sparse (reluctance any-strength can't substitute, tm 8→1024× flat); boundary is ACTIVE-but-SUBSTITUTABLE at dense (slow estimator covers it, τ 360→2880 drives fires 5→0) — NOT symmetric "each covers a regime the other can't" | Verified (strength sweep) | §6.1, `eager-ease-strength.rs` |
| Three superseded sparse-mechanism stories (ease-branch / persistence-discount / stays-below-OP) — each plausible, each refuted by tracing dynamics, corrected to verified | Method | §6.1, `which-boundary` + `belief-vs-op` |
| Lost-in-flight-work premise (old §6(ii)) — RETRACTED: retarget rejects no in-flight shares (per-job target snapshot), churn value-neutral | Killed premise | §6, `extended.rs` |
| `effort_up:effort_down` direction asymmetry retired (rested on lost work) | Killed premise | §6 |
| `regret_over:regret_under` + `tighten_multiplier` survive on §6(i) safety; magnitude a soft tuning judgment, deflated toward `1.5–2.0` | Choice + obs. | `a1d3fa7b`, `champion-weights` |
| Linear `Σ\|s\|` effort term closes the churn blind spot — re-homed as churn/usability cost, not lost work | Argument | §7 |
| Champion = the *safe* frontier (not the cost frontier) | Observation | §8, `steady-transient` |
| Decline-safety is a τ-valley, floored at the champion's window | Observation | §8, `tau-valley` |
| Champion selected by minimax over `r*`, safety as constraint | Choice + obs. | §9, `sweep-minimax`, `slow-decline` |
| Steady under-difficulty offset (`≈−0.67·σ_eff`) is cost-optimal | Observation | §10, `confirm-debias` |
| Counter-age mechanism on real hardware | Observation (HW) | §9, PR #2154 |
| *Previous* champion beats classic on a live drop | Observation (HW) | §9, PR #2154 |
| *Present* champion: decline *direction* HW-confirmed (50% drop, no rejection spike) | Observation (HW) | §9.4 |
| *Present* champion: settled-e and slow-moderate-decline gate still sim-only | Scope note | §9.4 |
| Three hero premises drawn and killed by measurement | Method | §8.1 |

The structural finding — across the operating band the field is flat, the
residual axis is gentleness-and-safety not agility, detection is floor-limited at
production rates, and the one lever is `r*` — is proved from Theorems 1–2 and
confirmed by direct measurement.
The champion is the existence proof that the safe corner of the frontier can be
occupied, selected by minimax over `r*` with decline-safety as a hard constraint.
The behavioral layer — counter-age blindness, the direction-correct response to a
sustained loss — is externally confirmed on real hardware; for the present
champion specifically, that decline response is hardware-confirmed in direction
(a sustained 50% drop, eased the safe way, no rejection spike), while its settled
over-difficulty figure and its behavior on a slow moderate decline remain
simulation results, and the cost-model weights remain a calibrated judgment —
robust over the grounded range and open to those measurements and an economic
backtest.