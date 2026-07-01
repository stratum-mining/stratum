# Open items — status register (thin index; homes are the linked docs)

A single-glance, **status-typed** view of what is left across the records, and why.
This is an INDEX, not a home: it states each item's status and points at the doc
that owns the protocol/detail; it does not duplicate them (one source of truth per
item). Built 2026-06-30 from a scan that verified each flag against actual bin
content + run-records — which caught several STALE flags (e.g. "NOMP not yet run
through our gate" and the slope-aware race were already run; see Closed below).

Status types (the distinction is the point — these are NOT all "TODO"):
- **RUNNABLE-OPEN** — a sim test that could be run now and wasn't. (Currently: none.)
- **SIM-DONE / HW-PENDING** — done in simulation; correctly awaiting hardware we
  don't have. NOT a loose end.
- **PENDING-ON-DATA** — needs real traffic/data that doesn't exist yet.
- **RESOLVED-CONDITIONAL** — investigated and resolved, but the resolution rests on
  named checks (not yet cleared); closeable by those specific gates, not reopenable
  from scratch.
- **CLOSED-THIS-TURN / CLOSED-BY-DESIGN** — discharged or deliberately not-run;
  listed so they are not re-opened or mistaken for gaps.

---

## Currently open

### SIM-DONE / HW-PENDING

- **Decline floor-collapse, two-point same-`ρ/r*` HW overlay** — `DECLINE_COLLAPSE_TEST.md`
  §5. Experiment B (the slope-aware matched-detector race) is **built and run in
  sim** (`bin/matched-detector`, oracle ×0.95 → floor-limited). What remains is the
  *hardware* confirmation: two real-miner declines at matched `ρ/r*` overlaid to
  confirm the floor-collapse shape on iron. Status: sim-complete, HW-pending — not a
  skipped test.

### RESOLVED-CONDITIONAL (closeable by named checks, not reopenable from scratch)

- **Connected-silence** — `CONNECTED_SILENCE_TEST.md`. Investigated on ops-manager
  field testimony (pinging-but-not-hashing is the endemic case, not gradual decline);
  traced to source across six checks. **Resolved favorably:** MARA's pool ticks vardiff
  on a 60s timer and does NOT evict silent-but-connected channels, so it eases (not
  freezes) a silent miner — NOT in the NOMP/ckpool freeze set; no degraded-resume
  spiral; champion ≥ classic across the board and better at resume for short/medium
  silence (bounded — protection erodes as both floor at long silence). **No essay
  change** (resolved worry = recorded reassurance, not a new spine). **Conditional on
  two named checks** (the path to unconditional): (1) deployed-commit confirmation of
  the 60s-ticker + no-idle-eviction architecture; (2) JDC/translator topology trace
  IF MARA routes miners through a JDC (that component's vardiff would govern). Probes
  are LOCAL working bins, not reproduction surface.

### PENDING-ON-DATA

- **Telemetry-hint cold-start (Home B) validation** — `NOMINAL_HASHRATE_COLDSTART.md`
  (open dependency, ~line 334/428). Gated on real native-SV2 `UpdateChannel` traffic
  carrying genuine firmware hashrate drops (the `marafw` 10%-drop reports). Status:
  awaiting real traffic; not runnable in sim.

---

## Closed

### CLOSED-THIS-TURN

- **Lever F-test (pre-registered)** — `LEVER_TEST_PREREG.md` (discharge section).
  The committed per-window Poisson-dispersion test (`F = mean(N−μ_pred)²/mean(μ_pred)`)
  was **run exactly as pre-registered** (`bin/lever-ftest`). Result: **UNMEASURABLE**
  at both 6 and 30 spm — the prereg's pre-registered LIKELY outcome, vindicated (the
  champion's continuously-moving EWMA belief is never flat, so the per-window
  baseline the test needs doesn't exist; the large F is the unmet-precondition
  artifact, NOT a Poisson-floor refutation). NOT engineered around. A pre-registered
  commitment, now discharged. The floor claim itself rests on Theorem 2 + the
  EXCESS-lever figure (`information-floor.md` §8.4), unaffected.

### CLOSED-BY-DESIGN (do NOT re-open or add "for completeness")

- **Decline-collapse Experiment A** — `DECLINE_COLLAPSE_TEST.md` §7.2. Explicitly not
  built; its one number (unconfounded `L*` depth) came as a by-product of Experiment
  B. Adding it reopens nothing.
- **`AsymmetricPoissonCI` sub-guard** — `SLOW_DECLINE_TEST.md` (~line 174). Deferred
  by design: taking it reopens champion selection at the 2-spm margin and owes an
  spm≥6 re-confirm — a bad trade absent real connection-rate data showing a 2–4 spm
  tail.
- **NOMP decline-gate run** — `VARDIFF_SURVEY.md` (gate-run section). Already run
  (faithful share-gated arm built, `bin/phase-portrait`); the "not yet run" line
  elsewhere in that doc is stale relative to the gate-run record. Headline confirmed
  (no disconnect at θ=0.36; starvation boundary fast-tail). MiningCore idle-ease
  *quality* and NOMP *jitter* characterization are explicitly out-of-scope refinements
  (one-seed / mechanism-not-magnitude), not committed tests.
- **EWMA(120) decline gate** — `CKPOOL_INVESTIGATION.md` gate-test addendum. Already
  run; EWMA(120) is gate-unsafe at sub-guard (+5.9%), ship Ewma360. Closed.

---

*Maintenance: this register is a status view. When an item's status changes, update
its line here AND its home doc; never let the two disagree (keep the protocol in the
home, only the status here).*
