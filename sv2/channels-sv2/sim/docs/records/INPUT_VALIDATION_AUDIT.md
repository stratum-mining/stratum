# Input-validation audit of `channels_sv2` (the #2221 class)

**Date:** 2026-06-30. **Base audited:** `upstream/main` (stratum-mining/stratum),
the canonical library + PR target. **Method:** five parallel source-reading
passes over the public input surface, each applying one rubric; load-bearing
findings re-verified against source by hand before recording here.

## The bug class

A public library function accepts numeric input it should reject (non-finite
`f32`/`f64` — NaN/±inf — or out-of-range/sentinel), and the bad input flows into
arithmetic that produces a **garbage or dangerous** result instead of an error.
Reference: `hash_rate_to_target` accepted NaN/+inf (only `== 0.0` and
`is_sign_negative()` guards, no `is_finite()`); `value as u128` saturates
silently (NaN→0, +inf→`u128::MAX`), so +inf yielded the **hardest** target — the
over-difficulty/spiral direction. Fixed in **PR #2221** (OPEN, not yet merged) by
an `is_finite()` guard on both operands, ordered first.

## Framing: what #2221 already covers vs. what is independent

#2221 is OPEN, not merged — so `upstream/main` still shows the pre-fix converter.
That means **every site that produces garbage by flowing input *through*
`hash_rate_to_target` is closed the moment #2221 merges**, because all those
callers already `match … Err(_) => return Err(…)` and will propagate the
rejection. Those are the **blast radius of #2221**, not new findings. The audit's
value is separating that blast radius from **genuinely independent** gaps #2221
does not reach.

### Blast radius of #2221 (closed on merge — NOT separate work)

All six pass miner-declared `nominal_hashrate` straight into `hash_rate_to_target`;
none use `.unwrap()`/`.expect()` (all `match`), so no panic concern — the error
arm is simply never reached for NaN/+inf today, and *will* be once #2221 lands:

- `server/extended.rs`: `new_for_pool` (:121), `new_for_job_declaration_client`
  (:159), `update_channel` (:375)
- `server/standard.rs`: `new_for_pool` (:116), `new_for_job_declaration_client`
  (:150), `update_channel` (:322)

These confirm #2221 is **correctly central** (one converter fix covers six
callers), not that there are six new bugs.

### Independent findings (#2221 does NOT close — re-verified by hand)

- **A — `VardiffState::new_with_min(min_allowed_hashrate: f32)` (`vardiff/classic.rs:36`):**
  stores the clamp floor with **zero validation**. A `NaN` floor **silently
  disables the safety clamp** — `new_hashrate < NaN` is always `false` in Rust, so
  the floor at `classic.rs:182` becomes a no-op and a decaying hashrate flows out
  unfloored. SOLID, self-justifying (a NaN that disables a *safety* floor is
  obviously wrong), needs no live data. Return type is already `Result`, so a
  validating guard is non-breaking. **This is the standout independent finding.**

- **B — `try_vardiff` finiteness (`vardiff/classic.rs:96`):** neither the
  `hashrate` input, the `hash_rate_from_target` result, nor the computed
  `new_hashrate` is screened for finiteness. `hashrate` is a **divisor**
  (`hashrate_delta.abs() / hashrate`, `:144`) and `shares_per_minute` is a divisor
  in the fallback (`/ shares_per_minute`, `:139`), so a zero/non-finite input
  manufactures NaN/inf that survives the lower-bound-only clamp and returns into
  the converter. SCOPE NOTE: the real defect is **finiteness**, NOT magnitude —
  the `hashrate * 10.0` multiply branch (`:178`) has no upper bound, but a large
  *finite* jump is legitimate vardiff behaviour; clamping to a `SOME_MAX_HASHRATE`
  would smuggle in a policy number. The fix screens finite-and-positive (divisor
  justification), not a magnitude cap.

- **C — server `set_nominal_hashrate(f32)` (`server/extended.rs:355`,
  `server/standard.rs:275`):** store an **unvalidated f32**; a NaN/inf sits in
  channel state (poisons `get_nominal_hashrate` readers) until the next
  update/vardiff cycle feeds it to the converter — where post-#2221 it IS caught.
  So C is **defense-in-depth**, weaker-justified alone. RECORDS NOTE, not a rushed
  PR; fold into A+B's PR only if cheap.

## Cleared (false suspects — the discipline working)

The pattern flagged these; reading **cleared** each. The cleared set outnumbers
the confirmed — the audit narrowed, it did not rubber-stamp.

- **`hash_rate_from_target` (`target.rs:164`) — CLEARED.** Same missing-`is_finite`
  gap as the reference, but the *inverse* formula saves it: non-finite
  `share_per_min` drives `60.0/share_per_min*100.0` to NaN-or-0, both cast to
  `0u128` and hit the pre-existing `== 0 → DivisionByZero` guard (`:188`) *before*
  any division or output cast. No garbage escapes. (The highest-prior suspect,
  cleared by tracing the arithmetic — the pattern would have falsely convicted it.)
- **All client-side** hashrate setters/getters/constructors
  (`client/extended.rs`, `client/standard.rs`) — **CLEARED**: store/return
  unvalidated f32 but the client modules never call a converter (grep-confirmed);
  no garbage path reachable. (Note: client `set_nominal_hashrate` has the *same
  unvalidated-store shape* as the server's, but no reachable converter — so it is
  cleared where the server's is finding C.)
- **All share-accounting** `update_best_diff`/work-sums (`client` + `server`
  `share_accounting.rs`) — **CLEARED**: their f64 inputs are
  `Target::difficulty_float()` (hash-derived, finite by construction), not raw
  external input. The NaN-poisons-max concern is real in the abstract (no
  `is_finite` guard) but **unreachable** — latent robustness gap, not a current
  bug.
- **Both `set_target`** (`server` + `client`) — **GRAY**: `Target` is a 256-bit
  integer, no non-finite value by construction. (A hostile `0`/max target is a
  separate degenerate-value concern, gated on a pool difficulty ceiling — same
  missing operand as the HOME_A `max_target` deviations, not this class.)
- **`ShareAccounting::new(share_batch_size: usize)` — CLEARED**: zero batch size
  is only ever compared by equality, never a divisor — degenerate batching, no
  div-by-zero/garbage.
- **`ExtranonceAllocator::new` — CLEARED**: already validates sizes/ranges
  (`max_channels == 0`, length bounds), returns `Result`.
- **`JobFactory::new` / job creation — CLEARED**: no numeric difficulty/float input.

## Disposition (the bounded set — the issue, if any, tracks THIS, not "more")

The audit is **done**; the result is a **small, real, bounded** set — not a swarm.
Issue-worthy content is **#2221 + A**, with **B**'s finiteness core a reasonable
companion and **C** a defense-in-depth note. Sequencing (decided):

1. **Let #2221 clear review / merge** — it is the anchor; build structure around it
   only once it is a fact, not a proposal (the enum-variant semver-check was still
   settling at audit time).
2. **Prepare A+B as a ready-but-unopened PR** off `upstream/main` (independent of
   #2221 — different function, no dependency). Verify→implement→describe→run CI's
   *actual* gate (1.89, `--all-features -D warnings`, full test) before opening —
   per [[feedback_pr-ready-means-passes-the-actual-gate]].
3. **Once #2221 lands, surface the class to maintainers** and let *their*
   convention choose the construct: a parent tracking issue (scoped to the
   confirmed bounded set, #2221 linked as the landed first fix, A+B as follow-ons)
   OR standalone cross-referenced PRs. Doing it "as the maintainers would" includes
   letting them say how. Do NOT open a broadly-scoped "audit everything" tracking
   issue — that is a commitment to an open-ended hunt; the audit already happened
   and found a finite set.

The firm bar regardless of construct: each follow-on PR clears what #2221 did —
source-confirmed (done — A/B re-verified by hand), scoped to its evidence (B is
finiteness not magnitude), self-justifying (no live data). The tracking issue links
*confirmed* fixes, not "and we'll probably find more."
