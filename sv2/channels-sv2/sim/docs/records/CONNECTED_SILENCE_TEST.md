# Connected-silence — investigation & resolution

**Status: RESOLVED FAVORABLY, conditional on two named checks.** Investigated in
response to ops-manager field testimony, traced to source across six checks,
resolved that MARA's stack handles connected-silence soundly. NOT an essay
change (a resolved reassurance, not a new problem); recorded here as the audit
trail + the path to making it unconditional.

Banner-scope: sim-validated + source-traced on `sv2-apps@test/vardiff-simulation-framework`;
**deployed-commit-pending** and **pool-role-only** (see §4). Provenance is two
**local working probes** — `connected-silence` and `silence-resume` — **held
off-branch** (NOT in this branch's `src/bin/`), pending the two checks in §4.
They are named here so the finding is traceable to what produced it, but are
deliberately not committed: their headline is conditional on §4, so they are not
reproduction surface. When §4 clears they can be committed and these references
upgraded to on-branch paths (see §4 closing note).

## 1. What prompted it

Ops-manager review of the essays contradicted the modal-decline premise: he does
**not** typically see gradual fade; he sees **sharp drop-offs**, and reports
"pinging but not hashing" miners as common (500–1000 per large site at any
moment) — i.e. **connected-but-silent** is the endemic field condition, not the
gradual decline the essays organize around. This raised the worry: the champion
was selected for gradual-decline-safety; is it *worse* on the connected-silence
case the field says is actually common?

The survey pins a "share-driven, no idle path" failure on the NOMP/ckpool family
(retarget runs only on share submission → no shares, no retarget → difficulty
frozen). The worry was that MARA's stack shares that failure.

## 2. The traced chain (six checks, none assumed)

Each step was a switch (flips the conclusion) or a measurement, traced to source
before the next was taken — see [[feedback_verify-before-re-anchor]].

1. **"But SRI has an idle path"** (source, upstream classic tests).
   `try_vardiff` has `no_shares_*_decrease` branches — SRI is on the
   *has-idle-path* side of survey Gate 1, **not** the NOMP/ckpool no-idle-path
   side. The flat "connected-silence failure applies to MARA" was a family
   property asserted of a member that doesn't have it.
2. **The idle path works, but the champion lags** (`connected-silence` probe).
   Both controllers ease a zero-share miner from tick 1 down to floor; neither
   freezes. The champion eases *slower* (EWMA-360 decays ~15%/tick; the
   tightening-only asymmetry brake doesn't apply when realized < target, so
   easing is unbraked) — it holds 31–59% of H0 at 10 min where classic is ~0.
   A **transient lag**, closed by ~30 min, both floored by ~60.
3. **The 60s timer drives it unconditionally** (`sv2-apps` pool role,
   `channel_manager/mod.rs:618` `run_vardiff_loop`). The role ticks
   `try_vardiff` over **all** channels on a 60s wall-clock interval, regardless
   of share arrival (shares come on a separate `increment_shares_since_last_update`
   path). So the idle-ease branch is **live in deployment**, not dead code —
   this resolves the dispositive precondition (b): if the role were
   share-triggered-only, the branch would never fire during silence and the
   controller *would* freeze, inverting everything.
4. **Silent channels persist — not evicted** (`sv2-apps` pool role,
   `downstream/mod.rs` recv loop + `mining_message_handler.rs:247` CloseChannel).
   The downstream message loop is a bare `recv().await` with **no idle/read
   timeout** (exhaustive sweep: only connect-timeout, graceful-shutdown-timeout,
   the 60s ticker). A channel leaves the vardiff map only on explicit
   `CloseChannel` or connection-break — **never on silence**. So a
   "pinging but not hashing" miner (TCP-alive, zero shares) stays in the map and
   keeps being ticked → the idle ease actually fires. This is the
   liveness-keyed-on-connection branch: channels persist, the easing happens,
   the "disconnect handles it / difficulty moot" branch does NOT occur.
5. **At resume, the champion is *better* — bounded** (`silence-resume` probe,
   symmetric metric). The champion's silence-lag is **latent** (inert during
   silence — no shares, nothing validated). It converts to realized cost only at
   resume, and only if the miner resumes degraded. Measured: **no degraded-resume
   over-difficulty spiral for either controller** (champion over-integral ≡ 0 on
   a symmetric metric — trustworthy, not a sign artifact). The real resume cost
   is a **share-flood**, and it is overwhelmingly *classic's*: classic floor-dives
   during silence, so a resuming miner faces near-zero difficulty and
   over-submits for ~6 ticks until retighten. The champion's gentle ease leaves
   it near-correct at resume — no flood.
6. **The protection is bounded, "near-zero" was an overclaim.** The champion's
   flood is **nonzero and grows with silence** (179 → 1,306 → 49,801 excess
   shares at 10/30/60 min silence) because it too reaches the floor eventually,
   just slower. Resume-protection is a **short-and-medium-silence property**, not
   universal; the advantage compresses as silence lengthens toward
   floor-saturation. (Caught as a favorable-direction overclaim; see
   [[feedback_favorable-result-broken-metric]].)

## 3. Resolution

- **No freeze.** MARA's pool ticks vardiff on a 60s timer and does not evict
  silent-but-connected channels, so it eases a pinging-but-not-hashing miner
  rather than freezing it. MARA is **not** in the NOMP/ckpool structural-freeze
  set — the scary hypothesis, refuted at the call site.
- **No degraded-resume spiral**, either controller (symmetric metric).
- **Champion ≥ classic across the board, strictly better at resume for
  short-and-medium silence**, for the same slow-window reason it is
  gradual-decline-safe (reluctance to over-react, in both directions — it does
  not over-ease into the floor, so there is nothing to recover from and no
  flood). Coherence: gradual-safety + transient-silence-lag + clean-resume are
  one property seen in three scenarios, which is why it generalizes rather than
  coincides. **Bounded** to short/medium silence.
- **Essays unchanged.** The detour started as a worry (champion worse on the
  common case) and resolved to no worry (it handles it fine, better at resume).
  A resolved worry is a recorded reassurance, not a new spine; the
  gradual-decline framing was never wrong — the field relocated the common case
  and the controller handles the relocated case soundly.

### Measurement notes (why the numbers are trustworthy in the way they claim)
- The resume harm metric is a **symmetric pair** — `over` (held too-hard,
  spiral sign) + `flood` (held too-easy, excess shares). A one-sided metric
  inverts this comparison because the two controllers fail on **opposite signs**
  (champion too-hard-from-above, classic too-easy-from-below); two sign-bugs were
  hit and caught en route. See [[feedback_sign-opposite-comparisons-are-sign-fragile]].
- **Flood magnitudes are MODEL-INFLATED.** Direction and ordering are
  trustworthy (classic ≫ champion, scales with silence); absolute counts are not
  (the sim's per-tick Poisson λ = (true/est)·spm is uncapped, so a floored
  controller meeting a resume shows a one-tick spike real submission caps +
  sub-tick retightening would bound — the million-share cells are not real share
  counts). Real magnitudes are telemetry-dependent — consistent with map-not-verdict.

## 4. The two checks owed (path to UNCONDITIONAL)

The resolution is conditional on two named checks; clearing both promotes it
from resolved-favorably-conditional to confirmed-favorably. They are specific
gates, not vague future work:

1. **Deployed-commit confirmation.** §2.3 (60s ticker) and §2.4 (no-idle-eviction)
   are verified on `sv2-apps@test/vardiff-simulation-framework`, not confirmed as
   the running deployed commit. The architecture is fundamental (a read loop
   without an idle timeout; a timer sweeping all channels) so deployed-divergence
   is unlikely — but that is an inference, marked-not-relied-on. One-line check
   against the running commit.
2. **JDC/translator topology trace.** The whole chain is verified for the **pool**
   role. The `jd-client` has its own vardiff map (`miner-apps/jd-client/...`, same
   pattern) with separately-untraced eviction behavior. If MARA's deployment
   routes miners through a JDC/translator, *that* component's vardiff governs the
   silent miner, and the §2.4 eviction analysis must be re-run for it. This is
   partly a code trace (the JDC eviction path) and partly an ops fact (which
   topology MARA runs).

**Promotion note.** Clearing both checks is the gate for *two* coupled upgrades,
kept in sync so commit-status tracks confidence: (a) the two probes
(`connected-silence`, `silence-resume`) get committed to `src/bin/` — they are no
longer conditional, so they become citable provenance like the other committed
bins; and (b) the references to them in this doc (currently named-as-held-off-branch,
§ banner / §2.2 / §2.5) upgrade to on-branch `src/bin/*.rs` paths. Until then the
probes stay local and this doc names them without a committable path — provenance
traceable, status honest, no dangling reference.
