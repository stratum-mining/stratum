# Using `nominal_hash_rate`: cold-start seed (RETRACTED) vs. runtime fusion

Written 2026-06-24, retraction added same day after measurement.

> **RETRACTION — Path A (cold-start seed) is withdrawn. Measurement falsified
> its premise.** I led with the seed as the headline "nearly-free win." That was
> wrong, and the error is instructive enough to keep: I took THEORY §8.5's
> 65-min ramp as a *production* phenomenon without checking how the channel
> actually sets the opening difficulty. It doesn't ramp — it opens at the
> nominal. See "Why Path A is wrong" below. The seed was coded, deployed to
> slots 3/4, then rolled back (pool commit `e23df760` reverted by `46d8ec68`).
> The vardiff-internal constructors (`new_seeded` etc.) remain in the library
> as additive dead code — harmless, unused, no caller. **The whole design now
> rests on Path B (runtime fusion of the downward step), which the open-target
> path does NOT already solve.**

The miner declares `nominal_hash_rate` in `OpenStandardMiningChannel` /
`OpenExtendedMiningChannel`, and may revise it mid-run via `UpdateChannel`. The
channel uses it to set the *opening* difficulty and — crucially — this is
already the separate-state principle in production: **the nominal informs the
operating point, never the belief.**

---

## Why Path A is wrong (the falsification)

**The open target already encodes the nominal.** Both channel constructors —
`server/standard.rs:199` and `server/extended.rs:214`, verified identical —
set the opening target via `hash_rate_to_target(nominal_hashrate,
expected_share_per_minute)`, i.e. `D_open = nominal / r*`. So at open the
channel sets the *operating point* from the nominal while leaving the EWMA
*belief* cold (the `EwmaEstimator` `n_ticks==0` fast-path,
`composed/estimator.rs:386`, snaps belief to the first observed count in one
tick). **The nominal→operating-point path is the separate-state design already
implemented.** Path A's error was not "trusting the hint" — it was injecting the
nominal into *belief* on top of the operating point, carving an exception to
"belief stays share-only." The sim closed that exception.

**The mechanism of harm is exactly that violation.** The controller's error
signal is `e = ln(operating_point / belief)`. Seeding `h_estimate` from the
nominal makes belief *agree* with the open operating point, which **zeroes `e`**.
When the open nominal is wrong (the 2.0× over-declared arm), the seeded arm
reads `e ≈ 0` and sits at over-difficulty, while the cold arm's fast-path reads
the real low share rate and corrects *down* immediately. The seed does not so
much *add* over-difficulty as **suppress the correction** of an already-wrong
open. The sign falls straight out of the mechanism — it is not a number taken on
faith.

**The measurement** (`sim/src/bin/seed-rampup.rs`, champion cold vs seeded,
per-tick `e` over the first 65 min, 400 trials/cell):
  - **decl = 1.0× (accurate open):** cold shows `e = 0.0` at *every* head tick.
    Nothing to ramp — the operating point opens at truth. Seed saves +21–36% of a
    tiny base (0.5–4.5 e-min) — only nudging the known steady under-difficulty
    offset, not a ramp.
  - **decl = 2.0× (over-declared):** seed is **−31 to −39%** (worse), in the
    dangerous over-difficulty direction — the suppression above.
  - **decl = 0.5× (under-declared):** seed is −6 to −10% (worse).

**The §8.5 ramp is not a pure artifact — but the seed can't rescue it.** A
5-order open gap (sim `ColdStart`: 1e10 vs 1e15) is what a *placeholder open
nominal* produces — e.g. the SRI translator demo opens with
`nominal_hash_rate = 5_000_000.0` (~5 MH/s, six orders below a real ASIC). So
the ramp is real *where opens are placeholders*. But there the seed **is** the
placeholder, and it additionally suppresses the fast-path correction. Honest
retraction, sharpened: the ramp is real for placeholder opens, the seed can't
fix it there, and it is redundant-or-negative everywhere else.

**Scope note (reference-impl generality):** slots 3/4 are *extended* channels,
so this is settled for the deployed case. The standard-channel open path
(`standard.rs:199`) was confirmed identical, so the generality claim holds for
both channel types — neither ramps on an accurate open.

---

## Path B — runtime fusion (requires an API change)

**What it does.** Treat `nominal_hash_rate` — including mid-run `UpdateChannel`
revisions — as a *second observation channel* feeding the controller
continuously: a fast, unverified hint alongside the slow, honest share stream.
The payoff is **closing the detection gap on a genuine downward step**: when a
miner's real hashrate drops, the share stream takes ~τ to notice, but an
`UpdateChannel` carrying the new (lower) nominal is available immediately.

**Why it needs an API change.** The `Vardiff` trait today receives `hashrate`
as a *per-call argument* to `try_vardiff`; it holds no separate notion of "what
the miner claims" vs. "what I believe from shares." Fusion requires the
controller to carry **two pieces of state** — `controller_belief` (from shares)
and `miner_hint` (from declarations) — and a derived operating point. That
means either:
  - new trait surface (`observe_hint` / `update_nominal`) so the channel can
    push declaration updates into vardiff as they arrive, **and/or**
  - a channel accessor (`get_nominal_hashrate`) plus a vardiff state split so
    the two channels don't collapse into one register.

This is exactly the boundary the cold-start seed avoids. Fusion *cannot* be
done as a per-call arg, because the hint arrives on a different clock than the
`try_vardiff` cadence and must persist between calls.

**Why it is genuinely harder than "just read the field":**
  - **Asymmetry must be enforced per-message, not per-window.** An upward hint
    ("I'm now faster") moves difficulty in the dangerous over-difficulty
    direction on the miner's unverified say-so; it must be near-inert (corroborate
    with shares before acting). A downward hint is safe (eases) and can act
    faster. This is the §6 eager-ease/reluctant-tighten rule applied to the hint
    channel — but now at the granularity of individual `UpdateChannel` messages.
  - **The topology-vs-say-so discriminator is NOT on the wire (corrected).** An
    earlier design pass proposed splitting upward revisions three ways using
    `aggregated_device_count` (count changed → real devices attached → honor the
    tighten; count unchanged + nominal up → a per-device *claim* → defer). Reading
    the actual `UpdateChannel` struct refuted it: its only three fields are
    `channel_id`, `nominal_hash_rate`, `maximum_target` — there is no device
    count, on this message or anywhere in channel state. So the pool CANNOT
    distinguish a legitimate aggregate-attach upward revision from a say-so
    upward claim; they arrive identically. The three-way classifier is
    unbuildable, and the upward leg collapses to **defer** (below). This is the
    same gap as the 0x0002 per-worker-carriage limitation, one layer down:
    `UpdateChannel` carries no provenance marker *at all*, so the attach-vs-say-so
    ambiguity persists even with per-device native-sv2 channels.
  - **Provenance decides coherence.** Fusion is only sound if the nominal is
    *device telemetry* (an independent measurement) — not a re-derivation of the
    share rate (circular) or a static config echo (stale). We have direct
    evidence this varies per deployment: of four observed slots, two carried
    telemetry-shaped nominals and one carried the `nominal = 1` sentinel. No
    protocol field asserts provenance. A fusion that trusts a config-echo as
    telemetry is worse than no fusion.
  - **Runtime plausibility guard.** The same garbage declarations that the
    cold-start gate rejects once at open must be rejected *continuously* for
    fusion, including reconnect/re-open collisions.

**The upward leg: DEFER (decided by worst-case survivability under the missing
discriminator).** Since the pool cannot tell attach-upward from say-so-upward,
whatever rule it picks is applied blind to both. Defer's worst case (a real
attach → the channel runs under-difficulty for ~τ until the share-driven loop
tightens — a bounded burst of cheap shares, self-correcting, safe direction) is
survivable; honor's worst case (a say-so claim → over-difficulty injection, the
self-reinforcing spiral entry) is the failure the whole controller exists to
avoid. Pick the survivable worst case → **defer all nominal-driven tightening to
the share loop.** The loss on the good case is small: on a real attach the share
loop sees the new devices' shares immediately and tightens fast (rising
share-rate is the self-healing direction), so the hint would have tightened only
a few ticks sooner. (NB: a `maximum_target` *shrink* in the same `UpdateChannel`
is still mandatory to honor per spec — defer drops the nominal-driven tighten,
NOT a max_target shrink; they are separable.)

**What it buys that the open-target path does NOT already solve:** the
downward-step detection gap. The open target is set *once* at open and moves
thereafter only via share-driven `SetTarget`. So a mid-run hashrate *drop* is
not reflected until the share statistics reveal it — ~τ of over-difficulty lag,
every genuine decline. An `UpdateChannel` carrying the lower nominal is
available immediately. This is the recurring cost the seed could never touch
(the seed only acted at open), and it is the entire reason Path B exists.

---

## The recommendation to maintainers (post-retraction)

1. **Do not ship the cold-start seed.** Measurement showed the open target
   already encodes the nominal (`D_open = nominal/r*`), so seeding *belief* is
   redundant on accurate opens, cannot fix placeholder opens, and is
   net-negative on over-declared opens (it suppresses the cold fast-path's
   correction). The vardiff-internal `new_seeded` constructors are left as
   additive, uncalled dead code; the pool call-site change was reverted.

2. **Path B (runtime fusion of the downward step) is the whole game — and it
   does NOT need a trait change.** It is where the only payoff the open-target
   path leaves on the table lives, and the downward-hint measurement (below)
   established it as a pool-loop write to the operating-point register the
   fire-path already owns — no `Vardiff` trait change, hard-set (α=1) the ship
   form. What it *does* need is a real per-device `UpdateChannel` data source
   (provenance) and the plausibility guard, both spelled out below. The trait/
   channel API change is only required for the *upward* leg (recovery), which we
   declined on safety grounds — so the API argument is scoped to a payoff we are
   not pursuing.

The honest one-line framing: **the channel already does the safe thing with the
nominal (sets the operating point, not the belief); the only thing left worth
doing is easing the operating point on a plausible downward revision — and that
is a pool-loop write, not a new interface.**

---

## The guard, as it will ship (handler-local, zero library change)

A safety fix to the EXISTING `handle_update_channel` path, which today calls
`update_channel` on every revision — meaning it currently *tightens on the
miner's say-so*, the unguarded-upward injection the design forbids. The guard
replaces that with:

1. **Static plausibility floor** (no rate, no warmth signal): reject if the
   declared nominal is non-finite, below an absolute floor (`nominal=1` sentinel
   dies here), or carries a `max_target` that fails its own sanity check.
2. **Plausible-downward → eager-ease**, hard-set (α=1, the σ-sweep ship form),
   clamped to **both** the miner's `max_target` *and* the pool's floor — pass
   `requested_max = miner_max.min(pool_max_target)` so `update_channel`'s
   existing `target.min(requested_max)` (extended.rs:431) respects whichever
   ceiling is harder. (Target-space: higher target = lower difficulty, so a
   difficulty floor IS a target ceiling and `.min` picks the harder one — the
   clamp is in the right space by construction.) **CONFIG GAP, stated here at
   the clamp so it can't be misread:** `pool_max_target` has no real operand
   today — the only pool floor is vardiff's `DEFAULT_MIN_HASHRATE = 1.0`, which
   never bites. So *as shipped the second clamp is wired but bounded only by the
   non-biting default; the ease is effectively clamped by the miner's `max_target`
   alone until an operator sets a real pool difficulty band.* Do not read
   "clamped to the pool's floor" as a live protection — it is wired-and-ready,
   not active (full rationale below, "The pool-floor operand").
3. **Plausible-upward → defer** to the share-driven loop (no nominal-driven
   tighten). Decided by worst-case survivability under the missing discriminator
   (above): defer's worst case is a bounded self-correcting share-burst; honor's
   is the spiral.
4. **`max_target` shrink honored in ALL branches** (spec-mandatory) — defer/
   reject drop the nominal-driven tighten, NOT a shrink. BUT in the *reject*
   branch the shrink-honor is gated on the `max_target`'s OWN sanity: a message
   judged untrustworthy on its nominal does not get its `max_target` trusted
   blindly (a sentinel-tight shrink is a ceiling-slam DoS, mirror of the
   floor-slam). Sanity-gate the shrink, don't just check `is_shrink`.

**The observable-keyed warm plausibility test was DROPPED — as clamp-redundant,
not as wrong.** An earlier pass specified a warm test (reject a downward nominal
implying `< 5%·r*` share-rate at the current target), keyed to estimator
maturity. It is dropped, and the reasoning must be recorded precisely because it
is *reversible*:
  - On the only **acting** leg (downward), the harm the warm test prevented
    (over-easing into a share-flood) is bounded by the **pool-floor clamp** —
    *once a real floor is configured*: an ease cannot pass the pool's own floor,
    and any ease above it is within the band the pool already sanctioned (worst
    case = benign self-correcting under-difficulty). So the warm test prevents no
    harm the clamp does not — **conditional on the floor existing.** Until the
    operator sets a real band, the clamp is vacuous (config gap, item 2), so the
    over-low-downward case ships bounded only by the *absolute* static floor and
    the miner's `max_target` — an OPEN gap recorded next to the over-declared-open
    hole, not a closed one. The redundancy that justified dropping the warm test
    is therefore *conditional*, which the reversibility condition below makes
    explicit.
  - The warm test's *broader* original job was gating an **acting upward** leg
    (in the three-way classifier). That leg became a flat **defer** when the
    `aggregated_device_count` discriminator turned out to be off the wire — and
    you don't gate what you don't act on. So the warm test lost its upward job to
    a structural decision, not to being wrong.
  - **Reversibility condition (for a future contributor):** the
    "observable-keyed plausibility survives hardware generations" rationale was
    NOT refuted — it was made *currently-unnecessary* by two facts: (a)
    defer-upward and (b) the two-clamp pool floor. But (b) is itself only
    *wired*, not active (the floor has no operand yet), so the over-low-downward
    case is currently un-bounded by the clamp — the warm test would be the
    interim guard for it. Restore the warm test if EITHER: a real pool floor is
    *not* going to be configured soon (so the over-low case needs interim
    cover), OR a future native-sv2 provenance marker resurrects an *acting*
    upward leg (the warm test's original job). It was shelved for the leg
    structure plus an assumed-coming floor, not discarded as unsound.

**Net:** no `n_ticks`, no warmth signal, no cold-state seam (the design no longer
*has* a cold case — there is no rate-keyed test to be cold or warm), no library
or `Vardiff` trait change. Pure handler-local guard. Dormant on the current
translator-aggregate topology (no `UpdateChannel` traffic); it makes the existing
path safe and ready, payoff gated on native-sv2 + per-device carriage.

---

## The two-sensor feature: routing, home, and the OSS path

This section places the guard above into the larger question of *where the
two-sensor fusion should live* — and is careful to keep two registers visibly
apart: **[DECIDED]** items are measured or settled (the guard spec above);
**[PROPOSED — gated on live data]** items are directions for a future round and
the upstream maintainer conversation, deliberately *not* designed at full
resolution. A reader should be able to tell which is which at a glance; do not
promote a [PROPOSED] item to [DECIDED] without the live data that gates it.

### The architecture fork

The fusion can live in one of two homes:

- **Home A — pool-local guard [DECIDED, this is what ships].** The guard above,
  in `handle_update_channel` (sv2-apps `mining_message_handler.rs`), before the
  existing `update_channel` call. Zero `channels_sv2` change; reuses the
  channel's existing `update_channel` / `get_nominal_hashrate` / `get_target`.
  It also *fixes a live bug*: today's handler tightens on the miner's say-so
  unconditionally (the unguarded-upward spiral hazard), so shipping the guard is
  a safety fix regardless of the fusion payoff.

- **Home B — stratum library abstraction [PROPOSED — gated on live data; pointer
  fidelity only].** Make the fusion a first-class, reusable part of
  `channels_sv2` so every SRI consumer inherits the safe behavior instead of
  each pool reimplementing the guard. *Direction* (not a finished API): split
  the conflated `nominal_hashrate` field (`server/extended.rs`, parallel in
  `standard.rs`) into declared-vs-operating, and add a channel method (sketch
  name `apply_hint(declared)`) encapsulating plausibility + direction-asymmetry +
  clamp. The **`Vardiff` trait stays untouched — this part is [DECIDED]**, not
  proposed: the trait is stateless re: belief and takes the operating point as
  an argument, so fusion is an operating-point policy *upstream* of the trait,
  not an estimator concern. The API *shape* is deliberately left open: it should
  be designed with live data in hand for the upstream proposal, because
  committing a sim-only policy to a core library's public surface is the
  premature-abstraction trap (library API churn is the expensive kind).

### Resolution: hybrid, sequenced [DECIDED]

Ship Home A now (validate live, fix the live bug); propose Home B upstream once
live data confirms the payoff. Rationale: Home A's guard logic *is* the literal
spec for Home B's `apply_hint`, so A is not throwaway; and "here is live data"
beats "here is a sim" for SRI reviewers — the data-confirmed contribution is both
more accommodating to the OSS community and more likely accepted than a
speculative one.

### The pool-floor operand (the config gap, full statement) [DECIDED to ship non-biting]

The downward-ease clamp needs a pool difficulty floor, and **none exists in pool
config today** — only vardiff's `DEFAULT_MIN_HASHRATE = 1.0`, which never bites.
This is a **pre-existing whole-pool policy gap, not guard scope**: the vardiff
loop itself already runs against the same non-biting 1.0. Ship decision: wire the
clamp to a non-biting default so it **cannot perturb the slots-3/4 reference
baseline** (the same baseline-protection discipline that drove the seed
rollback). Activating real protection requires an operator-set pool difficulty
band `[min, max]` — flagged as a separate policy decision, **[PROPOSED]** for the
operator, explicitly *not* a free default (a biting band would change live
vardiff behavior on the reference baseline and must be chosen deliberately).

### The OSS-upstreaming path for Home B [PROPOSED — gated on live data]

What Home B buys the ecosystem: every implementation linking `channels_sv2`
(pool, and any proxy/JDC opening server-side channels) inherits safe two-sensor
fusion instead of reimplementing the guard, and the declared-vs-operating field
split fixes a real design smell (one register conflating the miner's declaration
with the controller's belief). The gate is live data first; the maintainer
conversation starts from Home A's confirmed guard logic plus measured payoff, not
from this doc's sim numbers alone.

### Test strategy [route 1 DECIDED as the validation; route 2 PROPOSED enabler]

Two routes to exercise the guard:

1. **marafw 10%-drop reports [DECIDED — the load-bearing validation].** Real
   native-SV2 `UpdateChannel` traffic from firmware reporting genuine hashrate
   drops. This is the validation that gates Home B. **Open dependency:** needs
   per-device channel visibility at the pool — on the current
   sv1→translator→shape-proxy→pool topology the pool sees one aggregate channel,
   so a native miner connecting is necessary-but-not-sufficient; per-device
   carriage is the recorded blocker (the same gap as the 0x0002 discussion).

2. **shape-proxy synthetic injection [PROPOSED — integration-test enabler, not
   designed here].** For *integration* testing before/independent of real
   firmware drops. The shape-proxy is a full SV2 upstream node but does **not**
   currently emit `UpdateChannel` (confirmed: its message handling forwards/
   shapes the share stream only). It *would* need a new endpoint (sketch
   `POST /channels/{id}/send-update`) that builds and sends an `UpdateChannel`
   upstream via its existing upstream writer, driven by its existing
   `RateProfile` / HTTP-API surface, wired through hashrate-gardener's existing
   curl-based control. Named as the next-round enabler; **not designed or built
   here.** Note: synthetic injection tests the *guard's response*, not the
   detection question — telemetry reports physical hashing, which dropping
   share-accepts does not perturb, so the share-shaping the proxy does today
   cannot itself produce a hint; the hint must be *supplied*.

---

## Path B measured: the downward-hint ceiling, and the damped-blend null

**The hint works (perfect-telemetry ceiling).** `downward-hint.rs`, champion
Ewma360/s1.5. On a mid-run drop the pool eases the operating-point register
(Ĥ ← declared) before `try_vardiff` — a pool-loop write to the register the
fire-path already writes, NO `Vardiff` trait change. Decline leg: eliminates
60–100% of the over-difficulty (starvation) area, largest where the share-only
controller is slowest (sparse rates). Recovery leg: share-driven in all arms and
statistically identical — the hint is one-sided, helps the decline, neutral on
recovery. Steady state: converges across arms — transient-only, champion
selection / τ-valley / lever / band do not reopen. Q1 (b-vs-c): the estimator
rescale is a strict precision give-back (discards the EWMA's banked smoothing),
so the zero-trait-change variant (no rescale) is as good or better — no trait
hook needed.

**The damped-blend hypothesis was pre-registered and REFUTED.** Proposed
mechanism (an agent's): hard-set (Ĥ ← declared) over-commits to a possibly-wrong
declared value, so under telemetry noise a damped blend
(Ĥ ← (1−α)·Ĥ + α·declared, α<1) should beat hard-set by averaging out the
misread. Pre-registered as a crossover shape (P1), a noise-vs-lag distinction
(P2), a gate⟷α substitution (P3), and an spm scaling (P4), each with a stated
failure condition, locked on the realistic range σ∈[0,0.30] before any numbers.
Result (`VARDIFF_DH_SWEEP=1`, 400 trials/cell):

- **P1 — FAILED (null).** No interior α CI-separates from α=1 at any σ in the
  locked range; α=1 (hard-set) is strictly best everywhere, monotone in α.
- **P2 — REVERSAL (not merely a null).** Damping monotonically *worsens* the lag
  axis (lower α → higher per-minute bias-rate at every lag/spm). This *disproves*
  the over-commitment mechanism directly, rather than failing to support it.
- **P3 — FAILED (null).** Gate and α are independent, not substitutes. The null
  is meaningful *because* of the floor: observed fire-rate 1.00 against the
  pre-registered 0.80 exclusion floor — zero cells excluded, so this is a real
  null, not the fire-collapse case the floor exists to screen out.
- **P4 — vacuous.** P1(a) never separates in-range, so there is no onset to shift.

**Why hard-set wins — the cost asymmetry (this explains the null, not just
reports it).** The two errors are not symmetric in *consequence* before any
magnitude is measured: under-easing leaves you in the *self-reinforcing*
over-difficulty direction (the blinding, starvation one the whole controller
exists to escape), while over-committing to a noisy-low value puts you in the
*self-correcting* under-difficulty direction. So the signs alone say under-easing
is the worse error — and at realistic noise (σ≤0.30) the magnitudes confirm it:
the over-difficulty area is large and a 30%-wrong eased value simply doesn't hurt
as much as easing only partway does. Damping fails for the *same reason* the
design eases fast and tightens slow — it is the §6 eager-ease/reluctant-tighten
asymmetry again, not a separate empirical fact.

**Where the crossover actually sits (post-hoc envelope, descriptive — NOT a
pre-registered test).** Extending σ past the locked range (0.45, 0.60; added
after the in-range nulls, labeled, changing no verdict): the α=1 advantage
*narrows* with σ, and the α=1 / α=0.75 CIs first overlap at **σ≈0.60**. So a
crossover region exists — but at σ≈0.60 the declared value lands below 0.5×
truth ~20% of the time, which is precisely the plausibility gate's reject
condition. **The damping knob and the plausibility gate address the same failure
from opposite ends, and the gate wins: by the time noise is large enough to
justify damping, it is large enough to reject the hint.** That is the structural
reason α is not a useful knob — not "it didn't help in our range" but "the regime
where it could help is the regime the guard already excludes."

**Scope of the recorded claim (and what is NOT claimed):**
1. Hard-set (α=1) is the ship recommendation in the realistic σ≤0.30 range,
   dominant on both cost axes (over-diff area, under-diff wobble) across all
   tested gates and spm.
2. The α=1 advantage *narrows* with σ; α=1/α=0.75 CIs first overlap at σ≈0.60 —
   a crossover region outside trustable telemetry noise and inside the gate's
   reject band.
3. The universal "damping never helps" claim is explicitly NOT made — the trend
   is toward convergence past σ≈0.60, just past where the hint is usable. A
   finite grid cannot establish the universal claim and we do not reach for it.

**Status (sim-only; what closes the open task is specified, not gestured at):**
The decline trigger is a perfect-telemetry-to-σ0.60 envelope in sim. Hardware
validation is pending native-sv2 `UpdateChannel` traffic carrying a *real
mid-run drop* **with per-device channel visibility at the pool** — NOT via the
translator/shape-proxy aggregate. Native-sv2 miners pointing in is necessary but
not sufficient: if they arrive *through* the proxy as part of the aggregate, the
pool still sees one channel and the per-device drop is blurred (the same
per-worker-carriage gap as the 0x0002 discussion). So the validation is not
unblocked the moment a native miner connects — it needs per-device carriage. The
open-time plausibility guard is a *more distant* horizon: sim-demonstrable but
unfalsifiable on the current topology (one aggregate channel, no per-device open
declarations). The two halves do not share a validation status.
