# A conservation-law theory of vardiff quality

> **Status: this is the derivation-and-falsification NOTEBOOK, dated to its
> mid-arc state** (≈ the §5.8 validation pass and the Q4–Q9 refinements of §8–§9).
> Read this for **how the theory was derived and what was tried and killed**; read
> `information-floor.md` for **what is true and decided**. Four things this banner
> fixes, honestly:
>
> 1. **Current truth is in information-floor.** Where this notebook frames something
>    pre-closure — the asymmetry (§5.1) and the death-spiral mechanism (§5.2/§5.2a),
>    the rate-aware question, the regime mechanism — the authoritative *closed*
>    version is `information-floor.md` §6.1 (the asymmetry-as-mechanism / regime-split), §8.3 (the
>    rate-aware closure: rate-coupling is policy, no admissibility content), and
>    §10(d) (the sub-guard reconciliation). This notebook is **not** updated to those
>    closures, by design — see (2).
> 2. **The derivation and the falsification trail are authoritative HERE, and are
>    NOT superseded.** This doc is where the dead ends are recorded — §3a
>    (magnitude-cancellation, REFUTED), §8.4 (synthesis attempt, REFUTED), the Q6/Q7
>    surprises (§9.1/§9.2), **and the in-flight-work rationale (§5.1, §10.2),
>    RETRACTED — a retarget rejects no in-flight work (`job_id_to_target`); the
>    asymmetry survives on §5.2 self-starvation, not lost work** — and that record
>    *is the doc's purpose*: it is what `information-floor.md` compresses to "premises drawn and
>    killed" and points back to. The conclusions are dated; the kill-record is live.
>    **Do not read "dated" as "wholly superseded"** — half of this doc (the
>    derivation and the trail) exists nowhere else. (Banner kill-list extended to
>    cover the in-flight-work retraction, 2026-06-28 — until claim-ids carry this in
>    the planned registry, this banner IS the reconciliation record, so it must
>    enumerate the retraction, not omit it.)
> 3. **The opening proposal was decided.** This notebook was written in the future
>    tense of an open proposal ("decide whether this framing should replace the
>    radar"). It *was* decided: the linear sign-split reframe **replaced** the
>    six-axis radar, and the champion ships (current truth: `information-floor.md`). That tense is
>    resolved — the question is not open.
> 4. **…but not every proposal here was adopted.** The *core reframe* was adopted;
>    the specific premises this notebook explores **include ones tried and killed**
>    (§3a, §8.4 above). "The proposal was adopted" is true of the core reframe, **not**
>    of every claim drawn here — the killed ones are preserved as the method record,
>    not as live proposals.

This note (1) derives what vardiff is mathematically trying to do from
the plant model, (2) states the conservation law that the model
implies, (3) proposes a single principled objective and the secondary
stats needed to make it legible, and — most importantly — (4)
**tries to break its own theory**. Read §5 (Holes) before believing §3.

---

## 1. The plant is one identity

Strip `run_trial_with_observer` (`src/trial.rs:294`) to its mechanics.
The algorithm commits to a hashrate estimate `H_est`, which sets the
difficulty `D = H_est / r*` (with `r*` the target shares/min). The
simulator then draws the share count for a tick from

```
λ = (H_true / H_est) · r* · (Δt / 60),     N ~ Poisson(λ)
```

so the *observed* share rate relative to setpoint is

```
        r_obs / r*  =  H_true / H_est.                       (★)
```

This single identity is the whole problem. **The quantity the
algorithm can observe (its share rate vs. setpoint) is exactly the
reciprocal of the quantity it is trying to control (its difficulty
error).** There is no separate "plant output" and "error signal" — they
are the same signal. Every vardiff is a feedback controller whose
*measurement is its tracking error*.

Work in the natural error coordinate — logarithmic, because difficulty
is multiplicative and (per `PID_INVESTIGATION.md`) quantized in powers
of two:

```
e(t) = log( H_est(t) / H_true(t) ) = − log( r_obs / r* ).
```

A 2× over-difficulty and a 2× under-difficulty are symmetric in `e`.
Everything vardiff wants is `e(t) → 0`. Note for later that (★)
rewrites as

```
        r_obs = r* · e^(−e).                                 (★★)
```

## 2. The conservation law: information rate is pinned to r*

For a Poisson stream, the Fisher information an interval carries about
`log H_true` equals its expected count, `λ`. When the loop is on
target (`e ≈ 0`, so `λ ≈ r*·Δt/60`), information arrives at a **constant
rate of `r*` nats per minute — independent of `H`, of difficulty, of
the miner's size.** That is not an accident; pinning `r_obs` to `r*` is
*precisely what setting `D = H_est/r*` does*. The algorithm fixes its
own sensory bandwidth by policy.

This is the conserved quantity, and it has a hard consequence. The
Cramér–Rao bound on any unbiased estimate of `e` over an effective
window `τ_eff` is

```
        Var(ê)  ≥  1 / (r* · τ_eff).                          (1)
```

The crate already encodes the special case of (1) — see
`SettledAccuracy::poisson_floor` (`src/metrics.rs:863`), which is
exactly `1/√(r*·τ)` dressed as a half-normal percentile. **Quality is
not free: it is bought with observation time, at a fixed exchange rate
`r*` set by policy, not by algorithmic cleverness.** No estimator beats
`1/√(r*·τ)`.

## 3. The algebraic consequence: every "axis" is one trade-off

The window `τ` that buys steady-state precision in (1) is the *same* `τ`
that delays the response to a real change. Push this through the
crate's four-axis decomposition and the six radar axes collapse into
three faces of one law:

**Estimator — bias vs. variance.** Steady error `~ 1/(r*·τ)` shrinks
with `τ`; post-step lag `~ τ` grows with `τ`. One knob.

**Boundary — Type I vs. Type II (the sequential-detection bound).** A
likelihood-ratio detector firing at evidence `log(1/α)` against a step
of size `δ` waits

```
        T_d(δ)  ≈  2·log(1/α) / (r* · δ²).                    (2)
```

Jitter (false fires under stable load) *is* `α` per unit time; reaction
time *is* `T_d`. They are not two axes — they are one ROC operating
point chosen by `α`.

**Update rule — overshoot vs. convergence.** Partial retarget `η` gives
`e_k ≈ e_0·(1−η)^k`: convergence time `∝ 1/η`, per-fire overshoot
exposure `∝ η`. One knob. (`FINDINGS.md` iteration 3 is this exact
trade: `η: 1.0 → 0.2` moved overshoot 106% → 10% at the cost of slower
convergence.)

All three are **speed vs. stability, bounded by the conserved rate
`r*`.** The four-axis split is engineering-real and worth keeping for
*attribution*; informationally there is ~one knob per axis and they all
spend the same currency.

**Empirical confirmation already in the tree.** Commit `31a9dbc1`
found a "~0.55 maximin ceiling for the three-stage architecture …
small-drop reaction and convergence trade against each other on a
shared agility budget, confirmed from four independent directions."
That "shared agility budget" *is* the conserved `r*` of (1)–(2). Four
unrelated parameterizations hitting the same wall is what a
conservation law looks like from the outside.

### 3a. The magnitude-cancellation result *(REFUTED by data — see §5.8)*

> **This subsection's conclusion is empirically false.** The validation
> pass (§5.8) shows post-step regret rises ~`∝ δ²`, not flat. It is kept
> here because the *derivation* is instructive about what assumption
> fails (near-SPRT detection, §5.3). Do not act on it.

Watch the cost of a step. Pre-detection regret = (error)² × (dwell
time) = `δ² · T_d`. Substitute (2):

```
        δ² · T_d  ≈  δ² · 2log(1/α)/(r*·δ²)  =  2log(1/α)/r*.
```

**The `δ²` cancels.** To leading order every step change costs the same
integrated tracking error, because larger changes are detected
proportionally faster. If true, this is decisive for the shootout: it
means splitting reactivity into **React −10%** and **React −50%** axes
*double-counts a single conserved quantity*, and equal-weighting them
with jitter rewards the midpoint of a trade-off curve ("balanced")
rather than distance to the frontier. (§5 shows where this result
breaks.)

## 4. The proposed objective

Because regret is additive over time, the six axes are all projections
of one functional — the integrated squared log-error, i.e. an LQ
tracking cost with a control-effort penalty:

```
                 1   T                          ┌                ┐²
   J  =  ───  ∫   ( log H_est/H_true )² dt  +  ρ·Σ │ Δ log D       │
                 T   0                       fires └  (per fire)   ┘
        └──────── tracking regret ────────┘     └── control effort ──┘
```

- The regret term absorbs **convergence + reaction + settled-accuracy +
  overshoot** with relative weights *derived from the model*, not
  hand-chosen ceilings. A run that never converges simply keeps
  accruing `e²` — so the survivor bias and trial-length dependence of
  the current time-to-event metrics vanish (a non-finisher is the
  worst score, not a dropped sample).
- The effort term absorbs **jitter** (many small fires) and
  **step-safety / overshoot** (few big fires) in one quadratic.
  Retargets cost a message plus a miner re-tune, so `ρ` is the system's
  one real trade-off knob.
- `ρ` replaces all six arbitrary ceilings (`/0.30`, `/0.5`, `/600s`, …)
  with a single interpretable Lagrange multiplier: *how many units of
  difficulty-error-variance is one retarget worth?*

**Normalization via the conservation law.** The minimum achievable `J`
over a scenario is set by `r*` and the process-noise statistics (a
Wiener/Kalman bound), not by the algorithm. So the headline can be a
unit-free efficiency

```
   E  =  J_optimal / J_algorithm  ∈ (0, 1],
```

with `E = 1` meaning information-theoretically optimal. This is
automatically SPM-aware (the floor scales with `r*`) and has no
arbitrary ceilings — the root fix for the "skewed scales" complaint.
(§5.4 disputes whether `J_optimal` is actually computable.)

## 5. Holes in the theory (read this part)

The framing above is clean, which is suspicious. Here is where it is
wrong or incomplete.

### 5.1 The cost is asymmetric — symmetric `e²` is the biggest error

> **Framing refined (not rewritten) — see `information-floor.md` §6.1.** This
> section (and §6 below) treats the eager-ease/reluctant-tighten asymmetry as *the*
> load-bearing safety property. A later mechanism study refines this: the validity
> invariant is *dangerous-direction protection*, and the asymmetry is *one mechanism*
> for it — the *dense-rate* one (the boundary's reluctant-tighten), switched OFF at
> sparse rate where the *estimator* carries the protection (PoissonCI, the sparse
> boundary, is a symmetric trigger). This higher-altitude section is left as-is; the
> mechanism-level correction lives in information-floor §6.1. Not stale-and-clean —
> flagged.
>
> **Second, separate flag — the in-flight-work rationale below is RETRACTED**
> (`information-floor.md` §6, PR #2188 / `job_id_to_target`). The bullet's claim that
> tightening "invalidates shares miners already have in flight" is **false against the
> implementation**: each job snapshots its target at creation and shares validate against
> *that* snapshot until the next block, so a retarget rejects no in-flight work and churn
> is value-neutral. This is a REFUTED premise in the sense of §3a/§8.4 — derivation
> retained for the record, conclusion **not** rewritten — and the "left as-is" above does
> NOT extend to it. What survives is the *asymmetry's direction*, which stands on §5.2's
> self-starvation (over-difficulty starves the share stream), not on lost work.

The single most important hole. Commit `a1d3fa7b` (AsymmetricCusum) and
`5d871ed3` ("never surprise the miner") establish that the two
directions of error are *not* equally costly:

- **Tightening** (raising difficulty, `e` driven up): invalidates
  shares miners already have in flight `[RETRACTED — see flag above: a retarget
  rejects no in-flight work; the asymmetry survives via §5.2 self-starvation, not this]`,
  and at the extreme triggers the
  "timeout death spiral" seen with physical miners — the miner submits
  nothing, looks dead, and the loop can't recover.
- **Easing** (lowering difficulty, `e` driven down): old, harder work
  is still valid; the only cost is slightly more share traffic and pool
  variance. Nearly free.

A symmetric quadratic `e²` averages this away entirely. The honest loss
is asymmetric, and probably has a *barrier* on the over-difficulty side
rather than a quadratic — being 3× over-difficulty is not 9× as bad as
1×, it is potentially catastrophic (disconnect). **Minimum viable fix:**
split the regret into over- and under-difficulty halves with
`w_over ≫ w_under`, and split the effort term into up- and down-fires
with `ρ_up > ρ_down`. The LQ elegance is real but it is the *linearized,
symmetric* shadow of an asymmetric, possibly constrained problem.

### 5.2 The conservation law is only local — and that *is* the death spiral

§2 claims information arrives at constant rate `r*`. By (★★) the actual
rate is `r* · e^(−e)`:

- Over-difficulty (`e > 0`): `r_obs` collapses → **information starves
  exactly when you most need it to detect the error.** Positive
  feedback: too-high difficulty → fewer shares → slower detection →
  stays too-high. This *is* the death spiral, and it is endogenous to
  the Poisson observation model.
- Under-difficulty (`e < 0`): `r_obs` floods → more information → fast
  self-correction.

So "constant `r*`" is the conservation law *at the setpoint only* — the
basis for the CRLB (1) and for steady-state accuracy. It does **not**
hold during the transients that the reaction/convergence metrics
measure. This both *limits* the clean theory and *explains* the
asymmetry of §5.1 from first principles: the same `e^(−e)` factor that
makes over-difficulty harmful also makes it harder to observe. A good
shootout objective must reward escaping the starved region fast, which
plain symmetric regret under-weights (it integrates `e²`, but the
*observability* penalty for `e>0` is extra and not captured by `e²`).

### 5.2a A second sensor sidesteps the starvation — in one regime only

§5.2 says the share stream starves exactly when `e>0`. There is a *second*
sensor that does not: the miner's on-device telemetry, carried in the SV2
`nominal_hash_rate` field (at open in `OpenChannel`, mid-run in
`UpdateChannel`). The two sensors are complementary:

- **Sensor 1 — the share stream.** Slow, consistent, unbiased, and
  precision-floored at `1/√(r*·τ)` (§2). Always present once mining, but
  its rate `r_obs = r*·e^(−e)` *collapses* in the over-difficulty regime —
  the §5.2 death loop.
- **Sensor 2 — device telemetry (`nominal_hash_rate`).** A *direct* report
  of what the chips are doing, not an inference from share arrivals. Fast,
  but intermittent, unverified, and frequently missing or misconfigured (one
  observed deployment declared the sentinel `nominal = 1`).

**This does not contradict §2, and the distinction is load-bearing.** §2's
floor `Var(ê) ≥ 1/(r*·τ)` is a bound on estimating `H` *from the share
stream* — it is a property of the **Poisson observation channel**, and "no
estimator beats `1/√(r*·τ)`" quantifies over estimators *of that channel*.
Sensor 2 is a **different channel the floor was never about**; it does not
*beat* the share-channel floor, it *sidesteps* it — it carries its own error
(device-reporting noise, staleness, the sentinel/missing failure modes),
bounded by a *different* budget, not by `1/√(r*·τ)`. The conservation law
stands untouched in both directions: Sensor 2 neither beats §2's floor nor is
free of error, and because its error is *unverified* it must be guarded, not
trusted as an oracle (the asymmetry below, and the runtime guard the feature
design specifies). We have simply added an observation the law does not
govern.

The payoff is **asymmetric across the two starved regimes**, and the
asymmetry is the whole point:

- **Hashrate degradation (the rescue).** This is exactly the §5.2 death
  loop: a real drop drives `e>0`, share rate collapses, detection starves.
  Sensor 2 does *not* pass through the `e^(−e)` collapse, so a downward
  `UpdateChannel` is available ~immediately — before the share stream
  reveals the drop — and easing the operating point on it breaks the
  positive feedback. Measured: this eliminates up to 60–100% of the
  over-difficulty area *in the perfect-telemetry ceiling* (the realistic
  noisy/laggy envelope is the pending follow-up)
  (`sim/docs/NOMINAL_HASHRATE_COLDSTART.md`, Path B; binary
  `sim/src/bin/downward-hint.rs`).
- **Cold-start (*not* a rescue — there was nothing to rescue).** The naive
  expectation is that Sensor 2 should also fix cold-start. It does not,
  because cold-start is **not starved at the operating-point level**: the
  channel's *opening difficulty is already derived from the nominal*
  (`D_open = nominal/r*`), so the operating point begins informed by Sensor 2
  the moment the channel opens. The remaining cold belief is the EWMA's, and
  the attempt to also seed *that* from the nominal was tried and **refuted**
  — it double-counts the nominal the open target already encoded and is
  net-negative on inaccurate opens (`NOMINAL_HASHRATE_COLDSTART.md`,
  retraction; binary `seed-rampup.rs`). So Sensor 2 helps in *one* of the two
  weak regimes; the other was never starved where a second sensor could
  reach it.

The §6 eager-ease/reluctant-tighten asymmetry carries over directly, now as
an *information* asymmetry on the hint: act eagerly on a downward revision
(the safe, self-correcting direction), but **defer an upward revision to the
share loop** — tightening on the miner's unverified say-so is the spiral
entry of §5.1, and the protocol carries no field to distinguish a legitimate
capacity increase from an unbacked claim. That the ease is a *hard set* (not
a damped blend toward the declared value) and that the upward leg is deferred
are *measured/decided*, not assumed (`NOMINAL_HASHRATE_COLDSTART.md`: the
damped-blend hypothesis was pre-registered and refuted; the defer-upward leg
decided by worst-case survivability).

Scope: this is sim-validated to a telemetry-noise envelope (σ ≤ 0.30) and a
perfect-telemetry ceiling. Live confirmation awaits native-SV2 per-device
`UpdateChannel` traffic at the pool.

### 5.3 The `δ²` cancellation is fragile

§3a assumes (a) an essentially optimal sequential detector, (b) that
the correction-phase regret (which *does* scale with `δ²`) is dominated
by the detection-phase regret, and (c) that `δ` is in the regime where
(2) holds — not so small that the estimator window saturates and `T_d`
exceeds the trial. Real boundaries (PoissonCI, CUSUM) are not SPRT, and
at small `δ` detection time is capped by the window, not by `2/δ²`.
So "every step costs the same" is a leading-order idealization for
moderate `δ`. It still kills the *equal-weighting* of React-10/50, but
it does not justify dropping small-step sensitivity entirely — the
catching of slow `−10%` declines (failing ASICs) is a real, separately
valuable capability (`FINDINGS.md` iteration 4; the whole point of
AdaCUSUM).

### 5.4 `J_optimal` is probably not computable

The efficiency `E = J_optimal/J` needs the Bayes-optimal
filter+controller for each scenario, under Poisson observation,
*asymmetric* cost (§5.1), *and* power-of-two quantization. That is not
closed-form. The CRLB floor (1) gives a *lower bound* on the regret
term (hence an *upper* bound on `E`), and pow2 quantization adds a hard
regret floor of `(½ log 2)² ≈ 0.12` that no algorithm can beat — but
the true optimum sits somewhere above the CRLB and is unknown. **`E`
should be reported as "fraction of a known lower bound," explicitly a
bound, not "fraction of optimal."** Otherwise it overstates how much
headroom remains.

### 5.5 Quantization breaks continuous LQ

Difficulty moves in powers of two; `e` cannot be driven to 0. There is
a floor `e_min ≈ ½ log 2` and the system limit-cycles within one
quantum rather than settling. Continuous LQ has no notion of this. In
practice it means: (a) a regret floor as in §5.4, and (b) jitter has an
irreducible component (toggling across a quantum boundary) that is not
a tuning failure. The effort metric must not punish this.

### 5.6 The score is only as good as the scenario ensemble

Regret is additive, so summing across scenarios is legitimate — but the
*weights* are the empirical frequency of cold-starts vs. steps vs.
stable operation vs. aged-counter steps (`SettledStep`, commit
`70fcb260`, which showed reaction time is dominated by counter age, not
SPM). A single mean regret hides which regime an algorithm fails in.
This is no worse than the current radar (which also averages over a
chosen grid), but it is not better either: **the objective does not
free us from choosing a representative workload.** It only makes the
within-workload aggregation principled.

### 5.7 Non-stationary truth

The step/cold-start scenarios are idealized. Real hashrate drifts
(diurnal, thermal) and fails (ASIC dropout, on/off). Integrated regret
handles arbitrary `H_true(t)` fine in principle, but the *test*
trajectories must include drift and partial failure for the number to
mean anything. The current grid is mostly steps; that is a workload
gap, orthogonal to the metric choice.

## 5.8 What the validation pass actually found

`src/bin/regret-effort.rs` computes the regret/effort decomposition
over 1000-trial trajectories for the three contenders (production
monolith `Cumul/Step/FullClamp*`, `balanced`, `react_priority`) and
tests the three decisive predictions. Results (full table in the
binary's output; `cargo run --release --bin regret-effort`):

**Q1 — Is the conservation law binding? NO (it is true but slack).**
Settled steady-state mean-`e²` runs at **0.02–0.20× the single-tick
Poisson floor `1/r*`** across every algorithm and share rate — i.e.
*far below* the one-tick floor, because the estimators average over
`τ_eff ≫ 1` tick, exactly as (1) permits. The CRLB is a real lower
bound but it is **not where the cost lives**: steady-state tracking is
nowhere near the information limit, it is comfortably inside it. The
"~0.55 maximin ceiling" of `31a9dbc1` is therefore **not** the CRLB
wall I claimed in §3 — it is a property of the *control architecture
and the old metric's normalization*, not of the observation channel.
That plank of the theory is **withdrawn**.

**Q2 — Does the `δ²` cancellation hold? NO — refuted.** Post-step
integrated regret rises **monotonically and steeply with `|δ|`**: for
the production algorithm at SPM=6 it goes 0.21 (−10%) → 0.98 (−25%) →
4.58 (−50%) nats²·min — roughly `∝ δ²`, the *opposite* of flat. Larger
steps are not detected fast enough to cancel their larger error. So
React−10 and React−50 measure **genuinely different-magnitude costs**;
they do *not* double-count one conserved quantity. §3a is **wrong as
stated** (it assumed near-SPRT detection, which §5.3 flagged; the data
confirms real boundaries are far from that ideal). The practical
consequence flips: keeping a small-step and a large-step term is
**justified**, though they should be weighted by *cost* (regret), not
counted as equal rates.

**Q2′ — The directional asymmetry is real and large.** For equal
`|δ|`, **drops cost ~3–4× more than rises**: production at SPM=6, −50%
= 4.58 vs +50% = 1.20 nats²·min. And the sign split confirms the
mechanism precisely — a drop's regret is **>99% over-difficulty**
(`reg_over`), a rise's is **>97% under-difficulty**. Over-difficulty
(the death-spiral side, §5.1–5.2) is both the costlier *and* the
self-reinforcing direction, exactly as `r_obs = r*·e^(−e)` predicts.
This is the **strongest-confirmed** part of the theory: the model's
directional structure shows up cleanly in the data.

**Q3 — Is there a real trade-off wall? Inconclusive from this pass,
and the aggregate is workload-dominated.** Cold-start regret
(~4.5 nats², ~100 effort) **dwarfs every other scenario by ~50–1000×**,
so any all-scenarios mean is just a cold-start measurement in disguise
(confirms §5.6 with a vengeance). Broken out by class, the three
contenders are nearly tied on cold-start and stable, and differ only
modestly on drop/rise — the production monolith actually shows the
*lowest* drop-regret (0.080) but it pays elsewhere (it is the algorithm
the whole `FINDINGS.md` derivation was built to replace, so a single
post-hoc regret number under-rates its known low-SPM detection
failures, which live in the `SettledStep`/counter-age regime this pass
didn't include). **No clean frontier emerged at three points** — and
the cold-start confound means the `(regret, effort)` plane must be
reported *per scenario class*, never pooled.

**Net effect on the theory.** The *foundation* survives and the
*directional/asymmetric* claims are confirmed with hard numbers. The
two claims that would have justified the most aggressive
simplification — "the conservation law is the binding constraint" and
"magnitude cancels, so collapse the reactivity axes" — are **both
refuted by the data**. The theory is real physics but it is *not*
tight: algorithms operate well inside the information bound, so quality
differences are dominated by control-structure choices and by the
asymmetric, magnitude-dependent transient costs — not by a single
conserved budget. This is the honest, less elegant picture, and it is
the one the metric redesign must be built on.

## 6. Does this give us everything to redo the shootout?

**Partly — and after the validation pass the answer is more
constrained than the elegant version promised.**

What survives and *is* worth building the shootout on:

- A principled **primary objective** — but it must be the **asymmetric,
  magnitude-weighted** regret, not a symmetric `e²`. The data (§5.8)
  forces both the over/under split (drops cost 3–4× rises) and a real
  dependence on `|δ|` (regret `∝ δ²`). One quadratic with two direction
  weights, scored separately by scenario class.
- A **direction-split effort** term (`effort_up` for costly tightening,
  `effort_down` for near-free easing) — confirmed meaningful: the
  monolith's stable effort is ~50/50 up/down while the EWMA contenders
  fire more, and the up/down ratio is exactly the tightening-cost knob
  `a1d3fa7b` cared about.
- A **survivor-bias-free, trial-length-robust** treatment of
  convergence and reaction (a non-finisher accrues maximal regret
  instead of being dropped). This needs no theory to be true — it
  follows from integrating `e²` — and it is the single most defensible
  improvement over the status quo.
- The right **picture**, with one mandatory caveat: algorithms as points
  in the `(regret, effort)` plane **per scenario class**, never pooled.
  Cold-start regret dwarfs everything ~50–1000× (§5.8 Q3), so a pooled
  plane is a cold-start plane in disguise.

What the data **took away** from the elegant version:

- **The CRLB floor is not a useful normalizer.** Algorithms run at
  0.02–0.20× the floor (§5.8 Q1) — it is slack, so `E = J_opt/J` would
  be a ratio of two small numbers far from the bound and would not
  discriminate. SPM-awareness has to come from per-SPM reporting, not
  from dividing by `1/(r*τ)`.
- **You cannot collapse React-10 and React-50.** §3a is refuted; small-
  and large-step costs are different and both real. Keep both, weight
  by regret (cost), not by equal rate.
- **There is no single magic number** — for the original reasons (the
  asymmetry §5.1, the observability trap §5.2, the un-knowable optimum
  §5.4) *and* the new one: the cold-start confound means any scalar
  that pools scenarios is dominated by one regime.
- Regret and effort are **not intuitive** to a human reviewer the way
  "converges in 4 minutes" and "fires 0.03×/min" are. They are correct
  but illegible.

So the proposal for the shootout is: **rank on the per-scenario-class
`(drop-regret, rise-regret, effort_up, effort_down)` profile, and
report these secondary stats as context.** Every one is a projection or
derivative of the *same* trajectory `e(t)`, so they cannot contradict
the primary objective — they only make it legible:

| Secondary stat | What it is, in `e(t)` terms | Why a human wants it |
|---|---|---|
| **Reaction half-life** | time for transient `e²` to decay 50% after a step | the intuitive "responsiveness," in seconds |
| **Convergence time** | duration of the cold-start transient-regret tail | intuitive, unchanged from today |
| **Jitter (fire rate, stable)** | count rate of the effort term at `e ≈ 0` | intuitive; also the irreducible-quantum check (§5.5) |
| **Peak over-difficulty & dwell** | `max(e>0)` and time spent there | the death-spiral / disconnect proxy (§5.1–5.2) |
| **Up/down effort split** | effort term partitioned by fire direction | exposes the tightening cost (§5.1) |

These five are the legible shadows of the two-or-three primary numbers.
The radar, if kept at all, should plot *these* — each already
higher-is-better and each tied to a real cost — rather than six
arbitrarily-normalized fitness components.

## 7. Status and next steps

**Done:** the regret/effort decomposition is implemented as
`src/bin/regret-effort.rs` (a pure post-process of existing
trajectories — no new simulation, works for the opaque monolith too)
and run at 1000 trials over the three contenders. It produced the §5.8
findings that refuted two of the theory's claims and confirmed the
asymmetric/directional core.

**Next, if we proceed to rebuild the shootout on this basis:**

1. Promote `regret_over` / `regret_under` / `effort_up` / `effort_down`
   from the standalone bin into a `DerivedMetric` so they sit alongside
   the existing registry and feed the regression harness.
2. Report them **per scenario class** (cold / stable / drop / rise),
   never pooled — the cold-start confound (§5.8 Q3) is not optional to
   handle.
3. Add the `SettledStep` / counter-age scenarios (`70fcb260`) to the
   regret pass before drawing any "which algorithm wins" conclusion —
   this pass omitted them, and they are where the monolith's known
   detection failures live (§5.8 Q3 caveat).
4. Replace the radar/maximin with the per-class `(regret, effort)`
   profile plus the five legible secondary stats; drop `E = J_opt/J`
   (the floor is slack, §5.8 Q1).
5. Decide the direction-weight ratios (`w_over/w_under`, `ρ_up/ρ_down`)
   from the share-rejection cost model in `a1d3fa7b` — this is a
   product/values call, not something the math settles.

**Honest bottom line.** The theory's *foundation* (the plant identity,
the log-error coordinate, the directional asymmetry, the death-spiral
observability trap) held up and is confirmed in data. Its *elegant
simplifications* (binding conservation law, magnitude cancellation,
efficiency-vs-optimal) did **not** survive contact with the
trajectories. What we are left with is still a real improvement over
the six-axis radar — it deletes the arbitrary ceilings, fixes survivor
bias, and grounds the weights in measurable cost — but it is a
multi-number, per-scenario profile, not a single conserved budget.
The shootout can be redone on this basis; it cannot be reduced to one
elegant scalar.

## 8. Reformulating the law to fit the observations

§5.8 refuted the law *as stated*. But a refuted prediction usually
means the law was **applied wrong**, not absent. Both refutations point
at the same omitted variable — the effective window `τ`, which the
algorithm *chooses* and *spends* — so this section reformulates the law
around `τ` and tests two specific tweaks (Q4, Q5 in the binary). Both
candidate reformulations are *also* quantitatively wrong, but the
**pattern** of their failure is what finally locates the real
structure.

### 8.1 Why the original failed: `τ` is endogenous

- **Q1 looked slack only because I used the wrong `τ`.** I compared
  settled `e²` to the *one-tick* floor `1/r*`. But a stable algorithm
  lets its window grow (cumulative counter; EWMA integration), so its
  *actual* `τ_eff ≫ 1` tick and `1/(r*·τ_eff)` is 5–50× smaller — right
  where the algorithms sit. The law isn't beaten; **the algorithm
  spends `τ` to buy precision.**
- **Q2's `δ²` scaling is the signature of window-limited detection.**
  SPRT (evidence-limited) gives `T_d ∝ 1/δ²` and regret flat in `δ`.
  The data's regret `∝ δ²` is exactly `regret = δ²·T_d` with
  **`T_d ≈ const`** — detection waits ~`τ` regardless of step size.
  Real boundaries are window-limited, not evidence-limited.

Both roads lead to `τ`. So the conserved quantity, if any, is a
relationship *between* the two things `τ` trades.

### 8.2 Tweak 1 — a bandwidth–variance *product* (tested as Q5)

Reformulate from "precision floor" to "product law":

```
   V_ss  ≈  c₁/(r*·τ)        steady error, shrinks with τ
   K     ≡  regret/δ²  ≈ c₂·τ   transient cost coeff, grows with τ
   ⇒  V_ss · K  ≈  c₁c₂/r*      τ cancels  →  V_ss·K·r* ~ const
```

A Bode/uncertainty-principle form: *steady precision × transient
agility is bounded, and `τ` only slides you along the curve.*

**Result (Q5): refuted as a conservation law — `V_ss·K·r*` does NOT
cluster.** The monolith sits at 0.14–0.34, the EWMA contenders at
0.49–0.90 — a systematic ~2–3× split, stable across SPM. **But this is
more useful than conservation would have been:** a non-constant product
means algorithms achieve *genuinely different trade-off efficiency*, so
`V_ss·K·r*` is a **scalar figure of merit** (lower = closer to the
frontier), not a law. The frontier is its minimum achievable value.

**Critical caveat (same trap as §5.8 Q3).** The monolith's *low*
product is a **false positive**. Its `V_ss` is tiny because it barely
fires when stable (high threshold → near-zero jitter), and `K` is
measured on a *fresh* counter — but that same sluggishness is exactly
why it fails *aged-counter* detection (`70fcb260`). Both `V_ss` and `K`
are measured on young counters, so the product **flatters the monolith
for the very defect the whole derivation set out to fix.** The figure
of merit is only valid once `SettledStep`/counter-age cells are folded
in. Until then it is actively misleading.

### 8.3 Tweak 2 — state-dependent information rate (tested as Q4)

Put (★★) *into* the law instead of leaving it as a footnote:

```
   dI/dt = r* · e^(−e(t))
```

Information starves at over-difficulty (`e>0`), floods at
under-difficulty (`e<0`). For a log-symmetric perturbation `|e_step|=a`,
a drop lands at `+a` and a matched rise at `−a`, giving the falsifiable
prediction

```
   regret_drop / regret_rise  ≈  e^(2a).     (matched |e_step|)
```

**Result (Q4): refuted as stated, but the *failure mode* is the real
finding.** Predicted 4.0 at `a=ln2`; measured **1.78 at SPM=6 and it
INVERTS to ~0.8 at SPM≥12** (rises cost *more* there). So:

- At **low SPM** detection is the bottleneck → the starvation asymmetry
  shows (drops cost more, qualitatively as predicted, just smaller than
  `e^(2a)`).
- At **high SPM** detection is cheap for both → what's left is
  *correction dynamics*, and a large rise (+100%) **overshoots into
  over-difficulty**, so the rise transient now spends time at `e>0` too.

### 8.4 A synthesis attempt — *REFUTED by Q6, see §9.1*

> **The `⟨e⟩⁺` mechanism below is empirically false.** Q6 (§9.1) shows
> upward steps sit at ~0% over-difficulty at *every* SPM — overshoot
> does **not** manufacture `e>0` — yet they still cost more at high SPM.
> So over-difficulty fraction does not predict cost. The reasoning is
> kept because its *refutation* (§9) is what finally separates "cost
> driver" from "directional label." Do not act on this subsection.

Q4's inversion is the punchline: the penalty attaches to
**over-difficulty as a *state*, not to a drop as an *event*.** You reach
`e>0` either by a downward step *or* by overshooting an upward
correction. So the corrected law is:

```
   regret  ∝  e_step²  ·  exp( 2·⟨e⟩⁺_transient )
```

where `⟨e⟩⁺_transient` is the time-averaged *signed, over-difficulty-
weighted* error during the transient — not a function of step
direction. This unifies three observations under one mechanism:
the confirmed drop>rise asymmetry (§5.8 Q2′), Q4's high-SPM inversion
(overshoot manufactures over-difficulty), and the death-spiral
(over-difficulty is self-reinforcing because `dI/dt` falls there).

**Directly testable next:** the `reg_over` fraction of the **+100% rise
transient at high SPM** should be elevated (overshoot signature). If it
is, the state-dependent-`⟨e⟩⁺` law is confirmed and replaces both the
naive `e²` and the directional fudge with one physically-grounded term.

### 8.5 Net: what the law becomes

| Original claim | Verdict | Replacement |
|---|---|---|
| `Var ≥ 1/(r*τ)` is the *binding* floor | slack (wrong τ) | `τ` is endogenous; precision is *bought* with it |
| Quality = closeness to that floor | refuted | `V_ss·K·r*` figure of merit (not conserved) — *valid only with aged-counter cells* |
| Every step costs the same (`δ²` cancels) | refuted | window-limited detection ⇒ `regret ∝ δ²·T_d`, keep both step sizes |
| Drops cost `e^(2a)`× rises | refuted (inverts at high SPM) | penalty tracks **over-difficulty state** `⟨e⟩⁺`, not step direction |

The conservation *instinct* was right — there is a real, `r*`-anchored
trade-off between steady precision and transient agility — but it is a
**product/efficiency relationship, not a budget**, and the dominant cost
term is **state-dependent (over-difficulty), not magnitude- or
direction-dependent**. That is a tighter, falsifiable theory than §1–4,
and it is the one to build the shootout metric on — *after* the
counter-age cells are added, without which every steady-state-anchored
number (including the new product) systematically flatters the
sluggish production monolith.

## 9. Two more refinements tested — both surprised

§8 ended with two falsifiable predictions and a known gap. Q6 tests the
`⟨e⟩⁺` overshoot mechanism; Q7 adds the counter-age regime. **Both
returned the opposite of what the theory predicted**, and the second
appears to *contradict a committed empirical finding* — which makes the
metric itself the next thing to interrogate.

### 9.1 Q6 — the overshoot mechanism is false; the split is still a clean label

§8.4 predicted the over-difficulty fraction of a **+100% upward** step
would *climb with SPM* as correction overshoot manufactures `e>0`.
Measured fraction:

```
  −50% drop:  1.00 / 1.00 / 1.00 / 1.00   (SPM 4 / 8 / 15 / 30)
  +100% rise: 0.04 / 0.01 / 0.00 / 0.00
```

The rise over-fraction is **~0 and *falling*, not climbing.** Upward
steps stay under-difficulty for essentially the whole transient — the
damped update rules (`PartialRetarget`, `AcceleratingPartialRetarget`,
`FullRetargetWithClamp`) approach from below and do **not** overshoot
into `e>0`. So:

- **§8.4's unifying law `regret ∝ e²·exp(2⟨e⟩⁺)` is refuted.** Rises
  cost *more* at high SPM (Q4) while sitting at ~0% over-difficulty;
  over-difficulty fraction therefore cannot be the cost driver. The
  attempt to make over-difficulty *state* the single explanatory
  variable fails.
- **But the over/under split is an excellent *directional classifier*** —
  perfectly clean (1.00 vs 0.00) and SPM-stable. It reliably says *which
  way* an algorithm erred; it just does not, by itself, say *how much it
  cost*. Keep it as a label, not as the cost model.

What actually drives the high-SPM "rises cost more" inversion, then?
Not overshoot. The remaining candidate is the **under-difficulty
correction tail**: a +100% step needs a large multiplicative climb
(`ln2` of headroom), and the damped, quantized update takes several
fires to get there, accruing under-difficulty regret the whole way —
*more* total than a −50% drop whose correction is a single bounded
clamp. I.e. the asymmetry at high SPM is an **update-rule/convergence**
effect, not an observation/information effect. That is a different axis
of the §3 decomposition than §8.3 assumed, and it means the drop/rise
asymmetry has **two distinct causes** that swap dominance with SPM:
information-starvation (low SPM, favors drop-cost) and
correction-tail-length (high SPM, favors rise-cost). No single
exponential captures both.

### 9.2 Q7 — counter-age regret is flat (and §8.5 mis-named the culprit)

§8.5 asserted that steady-state-anchored metrics flatter the monolith
because they miss aged-counter detection failure. Prediction: monolith
post-step regret should **explode** with counter age. Measured
integrated regret (nats²·min, −50% drop):

```
  monolith SPM6:  6.04 / 5.77 / 5.76 / 5.79   (settle 5 / 30 / 60 / 120 min)
  monolith SPM30: 4.64 / 4.03 / 3.63 / 3.59
  EWMA     SPM6:  4.96 / 4.80 / 4.75 / 4.66
```

**Regret is flat-to-slightly-*decreasing* with counter age, for every
algorithm — the opposite of the prediction.** Two readings, and they
have very different consequences:

1. **The regret integral saturates (metric limitation, most likely).**
   The post-step window is fixed at 60 min and the drop is to half-rate
   (`e_step = ln2 ≈ 0.69`, `e² ≈ 0.48`). If the algorithm has *not*
   reacted, regret accrues at ≈0.48/min → ~29 over 60 min. The observed
   ~3–6 means it reacts within roughly the first ~10 min in the *mean*,
   and a tail of slow trials is **diluted by the mean and capped by the
   window**. A 51-min reaction and a 12-min reaction both just read
   "high regret for this trial," and averaged with fast trials the
   age signal washes out. This is the **same survivor/window artifact
   the regret metric was supposed to *fix* — but it only fixes it for
   *non-detection*, not for *slow* detection within a bounded window.**
   If so, integrated regret needs either an unbounded (or much longer)
   window, or a per-trial *reaction-latency* companion stat to expose
   the slow tail. **This is a real limitation of the proposed metric and
   must be resolved before it can rank algorithms on detection.**

2. **The monolith genuinely reacts fine here and `70fcb260`'s 51-min
   figure was jitter-capped / counter-age-specific** in a way these
   cells don't reproduce (different SPM, the actual-counter-age vs
   intended-settle gap that commit itself flagged). Less likely given
   the commit's care, but not excludable from this pass alone.

**Q8 adjudicates it: reading (2).** A direct reaction-latency
companion (mean minutes to first fire after the step, non-reactors
counted at the 60-min cap, no survivor drop) is **flat across counter
age** for every algorithm:

```
  monolith SPM6:  3.9 / 3.5 / 3.6 / 3.6 min   (settle 5 / 30 / 60 / 120)
  monolith SPM12: 1.8 / 2.0 / 2.1 / 2.1
  EWMA     SPM6:  3.4 / 3.3 / 3.4 / 3.3
```

Latency does **not** grow with age — so the flat regret in Q7 is *not*
a metric-saturation artifact (that would have shown growing latency
under flat regret). In these cells **the monolith genuinely reacts in
2–4 min regardless of counter age.** Two consequences, one reassuring
and one that demands follow-up:

- *Reassuring for the metric:* bounded-window regret and the latency
  companion **agree**, so the regret profile is not blind here. The
  §9.5 worry about saturation is real in principle but does not bite in
  this regime.
- *Resolved by Q9 — there was never a contradiction; I tested the wrong
  algorithm.* Re-reading `70fcb260`: the age-blindness it reports is for
  **ClassicComposed** (3–30%, "functionally blind at high SPM") and
  **Parametric** (<1%) — *not* the monolith, which the commit itself
  lists as "100% age-independent." Q7/Q8 tested the monolith, so flat
  regret/latency *agrees* with the commit, it doesn't contradict it.
  §8.5's "flatters the monolith" framing named the wrong culprit. Q9
  (§9.4) tests the algorithms the commit actually flagged.

### 9.3 Updated scorecard

| Refinement | Predicted | Measured | Status |
|---|---|---|---|
| §8.4 `⟨e⟩⁺` overshoot drives cost | rise over-fraction climbs with SPM | ~0 at all SPM, falling | **refuted**; split is a label, not a cost model |
| high-SPM rise-cost cause | overshoot → over-difficulty | under-difficulty correction tail | **reattributed** to update-rule/convergence |
| §8.5 counter-age regret blow-up | monolith regret explodes with age | flat regret AND flat latency (Q8) | **mis-aimed** — monolith isn't the blind one; §8.5 named the wrong algorithm |
| `70fcb260` reproduces? (Q9) | ClassicComposed/Parametric go age-blind | CC 30→1%, Param 1→0% across SPM | **confirmed** — no contradiction; but regret *under-weights* it (§9.4) |

### 9.4 Q9 — the cross-check reproduces `70fcb260`, and exposes the regret metric's worst flaw

Testing the algorithms the commit actually flagged, with its
methodology (detection rate = P[fire within 60min], plus actual counter
age at the step), on the `settle=60min, −10%` headline cell:

```
  algo                       SPM   det%   actual_age(min)   regret(nats²·min)
  ClassicComposed  Cumul/Step  6    30%       34.6              1.328
       "                      12     9%       50.7              0.801
       "                      30     1%       58.6              0.685
  Parametric  Cumul/Poisson    6     1%       59.3              0.676
       "                      12     1%       59.7              0.665
       "                      30     0%       59.9              0.666
  monolith  Cumul/Step/*       6   100%        9.8              0.902
       "                      12   100%        8.7              0.558
       "                      30   100%       19.6              0.204
```

**`70fcb260` reproduces exactly.** ClassicComposed's detection falls
30%→9%→1% across SPM (the commit's "3–30%, functionally blind at high
SPM"); Parametric is ~0% (its "<1%"). The mechanism is visible in the
`actual_age` column: the blind algorithms let the counter mature to
35–60 min (they don't jitter), so the cumulative window dilutes the
−10% signal below threshold — exactly the FINDINGS.md iteration-0
mechanism. The monolith jitters its counter young (10–20 min) and so
detects 100% — the commit was right to call it age-independent. **There
was no contradiction; my §8.5/§9.2 alarm was a misreading of which
algorithm the commit indicted.**

**But the `regret` column is the real discovery — and it is bad news
for the proposed metric.** ClassicComposed at SPM30 detects a −10%
degradation **1% of the time** yet has the *lowest* regret of its row
(0.685). As it goes more blind (30%→1%), its regret *falls*
(1.328→0.685). The reason is structural and damning:

```
  a −10% drop is  e_step = ln(0.9) = −0.105,  e_step² ≈ 0.011
```

A 10% degradation is a **tiny** error in `e²`. Missing it entirely
costs ~`0.011/min` of regret — nearly nothing. So **the `e²` regret
functional is almost blind to small-magnitude degradation by
construction** — which is precisely the failure mode (slow ASIC death,
thermal throttle, partial dropout) that `70fcb260` built
`CounterAgeSensitivity` to catch and that operators care about most. A
quadratic loss says "a 10% revenue leak is 1% as bad as a 50% one";
operationally a *persistent undetected* 10% leak is arguably *worse*
than a transient 50% one, because it runs forever.

This is the **sharpest limitation found in the whole investigation.**
It is not a measurement artifact (Q8 ruled that out) — it is the
**choice of `e²` as the loss**. Quadratic regret correctly ranks
transient tracking but **systematically under-weights small persistent
biases**, and "detect slow degradation" cannot be recovered from it at
any window length. The detection objective is a genuinely separate
dimension, not a projection of the regret trajectory — contradicting
the §6 hope that all secondary stats are "shadows of the same `e(t)`."

**Consequence for the metric.** Detection of small persistent drops
must be a **first-class term**, not derived from regret:

- either weight the loss by a **revenue/persistence** factor — e.g.
  integrate `|e|` (linear, `∝0.105/min`) or a *time-since-divergence*
  penalty rather than `e²`, so a small error that persists accrues
  unboundedly; or
- carry the commit's `CounterAgeSensitivity` detection-rate as an
  explicit, co-equal axis alongside regret/effort.

The linear-vs-quadratic choice is itself a values call (how does
disruption scale with error size?), and it interacts with the
asymmetry of §5.1. This is the key open design decision for the
shootout metric.

### 9.5 Where the theory now stands

The plant identity, the log-error coordinate, the `r*`-anchored
precision↔agility trade-off (as a *product/efficiency* relation, §8.2),
and the low-SPM information-starvation asymmetry **survive**. What has
fallen away with each test is every attempt to compress *all* cost into
one closed-form term. The transient cost is genuinely **two mechanisms**
(information-limited detection at low SPM; convergence-tail length at
high SPM, §9.1) that trade dominance. And §9.4 found the deeper limit:
**small-magnitude persistent degradation is invisible to an `e²` loss**,
so detection is a separate dimension, not a projection of the regret
trajectory. The bounded-window *measurement* worry (Q7) turned out not
to bite — Q7 and Q8 agree — but that was the lesser problem; the
*loss-shape* problem (§9.4) is the real one.

The metric for the shootout therefore needs, at minimum:

1. per-scenario-class `(regret_over, regret_under, effort_up,
   effort_down)` — the directional labels are clean and SPM-stable
   (§9.1);
2. a **first-class small-degradation detection term** — either a
   persistence-weighted loss (integrate `|e|`, or a
   time-since-divergence penalty) or the commit's
   `CounterAgeSensitivity` detection-rate carried as a co-equal axis.
   `e²` regret **cannot** supply this (§9.4);
3. a **companion reaction-latency / non-detection stat** as a cheap
   guardrail — it is what adjudicated Q7 (§9.2).

The `70fcb260` "discrepancy" is **resolved**: Q9 reproduces the commit
exactly (ClassicComposed 30→1%, Parametric ~0%), and there was no
contradiction — my earlier alarm tested the monolith, which the commit
never indicted. That is now closed, not open.

The conservation law is real but **diagnostic, not sufficient**: it
explains the *shape* of the transient trade-offs and kills the arbitrary
six-axis ceilings, but (a) the transient cost is a small vector of
mechanism-specific terms, not one scalar, and (b) the operationally
critical "catch slow degradation" objective lives **outside** the
quadratic-regret picture entirely. The single elegant scalar of §1–4 is
not recoverable; the honest metric is regret/effort **plus** an explicit
detection axis. That is the theory's resting point, and the
linear-vs-quadratic loss-shape choice (§9.4) is the key remaining design
decision — a values call, not a math one.

## 10. Synopsis and the loss-shape decision

This section is the standalone summary: the current best model, the
metric sketch it implies, and a cheap empirical comparison of the three
loss-shape options — including how each changes the algorithm ranking
and what the comparison *looks like* (radar, plane, scalar).

### 10.1 Current best model (one paragraph)

A vardiff algorithm is a feedback controller whose **measurement is its
own tracking error**: the observed share rate relative to setpoint is
exactly `H_true/H_est` (★). Work in `e = ln(H_est/H_true)`; the goal is
`e → 0`. Quality lives in two ledgers — **tracking error** `e(t)` and
**control effort** (retargets). The conservation instinct (a fixed
information rate `r*` bounding precision vs. agility) is *real but
diagnostic*: it explains the shape of the trade-offs and kills the
arbitrary six-axis ceilings, but it is **not** a binding budget
(§5.8 Q1: algorithms run far inside it) and the cost does **not**
collapse to one scalar. Three things refused to be unified: transient
cost is two SPM-dependent mechanisms (detection-limited low-SPM,
convergence-tail high-SPM, §9.1); the error is **asymmetric** (over-
difficulty is costlier and self-starving, §5.8 Q2′); and — the sharpest
finding — **a quadratic loss is structurally blind to small persistent
degradation** (§9.4), the exact failing-ASIC case operators care about.

### 10.2 Metric sketch

Per trial, from the universal trajectory `(H_est, H_true, fires)` — no
introspection, works for every algorithm:

```
  regret_over   = ⟨e²⟩  over ticks with e>0   (over-difficulty; death-spiral side)
  regret_under  = ⟨e²⟩  over ticks with e<0   (under-difficulty; cheap side)
  effort_up     = Σ(Δln D)²  on tightening fires   (costly: rejects in-flight shares)
  effort_down   = Σ(Δln D)²  on easing fires       (≈free)
  detection     = P[fire within window | small persistent drop]   (SEPARATE axis, §9.4)
```

> **Flag (in-flight-work retraction, same as §5.1):** the `effort_up` annotation
> "rejects in-flight shares" is RETRACTED — a retarget rejects no in-flight work
> (`information-floor.md` §6, `job_id_to_target`). `effort_up` is still costed above
> `effort_down`, but the surviving basis is the §6(i) variance/over-difficulty concern
> and churn/usability, **not** lost work. Annotation kept as written for the record;
> the cost direction stands on the corrected basis.

Reported **per scenario class** (cold / stable / drop / rise / aged-
counter), never pooled (cold-start dominance, §5.8 Q3). The four
regret/effort terms are clean, model-derived, ceiling-free. `detection`
is the one term that is *not* a projection of `e(t)` and must be carried
explicitly.

### 10.3 The decision: how does disruption scale with error size?

The open choice is the **loss shape** on `e`. It is a values call (how
bad is a small persistent bias vs. a large transient one?), and it
determines whether slow degradation is visible. Three options, scored on
the same data (`regret-effort` §10, 600 trials, SPM {6,12,20,30}, mix =
stable + ±50%/±10% steps + aged-counter −10%):

| algo | Rq (e²) | Rl (\|e\|) | Det% | Eff |
| --- | --- | --- | --- | --- |
| monolith(prod) | 0.0748 | 0.1952 | 100% | 0.027 |
| balanced | 0.0564 | 0.1674 | 100% | 0.059 |
| react_priority | 0.0540 | 0.1609 | 100% | 0.060 |
| ClassicComposed | 0.0818 | 0.1935 | **10%** | 0.438 |
| Parametric | 0.0999 | 0.2181 | **1%** | 0.087 |

**Option A — Quadratic only (`∫e²`, the LQ-pure metric).**
*Implies:* disruption scales with the square of error; big transient
excursions dominate, small biases are negligible. *Ranking:*
`react_priority > balanced > monolith > ClassicComposed > Parametric`.
*Problem:* ClassicComposed detects the −10% drop **10% of the time** yet
scores *better than the monolith* on Rq (0.082 vs 0.075 — within noise),
because a 10% miss costs `(ln0.9)² ≈ 0.011`/min — nearly nothing.
Detection failure is invisible. **Visualization:** a single
`(regret, effort)` Pareto plane; one dot per algo; lower-left wins. Clean
and legible — but it would green-light a functionally-blind algorithm.

**Option B — Linear (`∫|e|`).**
*Implies:* disruption scales with error magnitude; a 10% bias is 1/5 as
bad as a 50% one (not 1/25). Small persistent errors get ~2.3× more
relative weight than under quadratic. *Ranking:* reorders the mid-tier —
`… ClassicComposed > monolith > Parametric` (ClassicComposed *rises*
above the monolith on Rl, the opposite of Rq). *Problem:* still does not
*solve* detection — a 10%-blind algo only accrues `0.105`/min, so a fast
algo with occasional larger errors can still out-score it. Linear
narrows the blindness gap but does not close it. **Visualization:** same
plane, different axis scaling; the dots reshuffle but the picture is the
same shape.

**Option C — Regret + explicit detection axis (recommended).**
*Implies:* small-persistent-degradation detection is its **own
objective**, not derivable from any `∫f(e)` (the §9.4 result). Carry
`detection` as a co-equal term. *Ranking (detection-first):*
`balanced > react_priority > monolith > ClassicComposed > Parametric` —
and now the 10%/1% detectors are correctly pinned to the bottom *by the
metric itself*, not by luck. This is what `70fcb260`'s
`CounterAgeSensitivity` already encodes; the regret work doesn't replace
it, it **joins** it. **Visualization:** a small **radar is justified
again here** — but a *principled* one, 3–5 axes that are real
independent costs, each higher-is-better and ceiling-free:
`{tracking (−Rq or −Rl), gentleness (−Eff), detection (Det), over-
difficulty safety (−regret_over)}`. Unlike the current six-axis radar
(correlated projections + arbitrary `/0.30`-style ceilings), these axes
are genuinely orthogonal — §9.1 showed the over/under and detection
terms move independently.

### 10.4 What the comparison shows

The rankings **do diverge**, and exactly where the theory predicts:

- The **top tier is robust** — `react_priority`/`balanced` beat the
  monolith under every philosophy. That ordering is trustworthy.
- The **mid-tier flips on loss shape** — ClassicComposed vs monolith
  swaps between Rq and Rl (the small-error reweighting bites).
- Only the **detection axis** (Option C) correctly identifies
  ClassicComposed (10%) and Parametric (1%) as operationally broken;
  pure regret (A or B) rates them mid-pack because their *steady-state*
  tracking is fine and they fire rarely (low effort). **A metric without
  an explicit detection term will recommend a blind algorithm.**

**Bottom line for the shootout.** Use **Option C**: per-class
`(regret_over, regret_under, effort_up, effort_down)` + an explicit
`detection` axis, shown as a 4–5-axis principled radar *and* a
`(regret, effort)` plane per class. Pick **linear** over quadratic for
the regret term itself (it is the more defensible disruption model and
narrows — though does not close — the small-error gap that detection
then finishes). Drop the single-scalar ambition and `E = J_opt/J`
entirely. This is cheap to build: every number above came from one
post-processing binary over existing trajectories.

### 10.5 What was implemented (and decided)

Built and committed:

- **`LogErrorRegret`** (`metrics.rs`, in `registry()`, `ReportOnly`) —
  per-cell `regret_over` / `regret_under` (time-averaged **linear**
  `|e|`), `regret_lin`, `effort_up` / `effort_down` / `effort`, from the
  universal trajectory (works for the opaque monolith). Renders per
  scenario class. Unit-tested on the sign split.
- **`bin/regret-radar`** — the five-axis principled radar (tracking,
  gentleness, detection, over-diff safety, tighten-care).

**Normalization decision: against the production incumbent, not a
candidate-set hull.** Each cost axis is scored `ref/(ref+cost)`, so the
monolith lands at exactly the **0.5 mid-ring** (drawn bold); beating it
pushes outward, losing pulls inward, bounded in (0,1). Detection is an
absolute probability and is plotted **raw** (normalizing a probability
by the set max would falsely promote the best candidate to 1.0).

Why incumbent-reference beats hull-normalization (the original sketch):

- **Stable run-to-run.** Hull = best-in-set makes every vertex depend on
  *who else you plotted*; a new contender silently rescales everyone.
  The incumbent is fixed.
- **No strawman needed.** Hull-normalization has no spread unless a
  bad candidate is present (among only-good algos it amplifies noise and
  the all-100% detection axis goes flat-uninformative). Incumbent-
  reference reads directly as "beats / loses to prod" per axis with just
  the real contenders. (A flat detection axis is then *correct
  information* — "these all detect fine" — not a defect; a blind algo
  added to the panel simply plots near center.)
- **Still ceiling-free and principled.** The reference is the thing we
  are trying to beat, not a hand-picked `/0.30`.

At 1000 trials the three contenders vs the monolith reference: tracking
0.50/0.51/0.52, over-diff safety 0.50/0.56/0.56 (contenders beat prod),
gentleness 0.50/0.30/0.30 and tighten-care 0.50/0.27/0.26 (contenders
**worse** — they fire more), detection all 1.00. The honest tradeoff:
the contenders track better and are safer on over-difficulty, paid for
in retarget frequency.

**Retired:** the old `bin/radar-chart` and its four generated SVGs
(`radar_chart`, `radar_contenders`, `radar_pareto`,
`radar_symmetric_comparison`). `EqualWeightFitness` is marked
**deprecated** but kept until the maximin sweep bins
(`sweep-balanced` / `-estimators` / `-signpersist*` / `-voladapt`)
migrate to regret/effort scoring — that migration is part of the
retune, not done here.
