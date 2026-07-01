# Records — index

The supporting record behind the paper (`../information-floor.md`) and its
derivation notebook (`../THEORY.md`). This index is a **map, not a second home**:
it gives each file's *purpose* and *where to start*, both stable fields. It
deliberately does **not** restate each doc's status (current / superseded /
pending / built) — that lives in each doc's own header banner, stated once and
authoritatively, so there is no second copy to drift out of sync. For *what is
left to run* across all of these, see [`OPEN_ITEMS.md`](./OPEN_ITEMS.md).

Files are grouped by **where a reader starts**, not by importance — the groups
are a reading order, not a ranking.

## Start here — architecture & findings

| file | purpose |
|---|---|
| [`DESIGN.md`](./DESIGN.md) | Architectural reference: the three-stage pipeline (Estimator / Boundary / UpdateRule), inter-stage handoff contracts, the `Composed` adapter, the algorithm registry, scenario DSL, Grid, and the Metric trait. |
| [`FINDINGS.md`](./FINDINGS.md) | The stage-by-stage iteration record (Classic → … ) — what the framework measured at each fix. A dated derivation log; its `FullRemedy` waypoint is mid-arc, not the shipped champion (see its banner). |

## Investigations — per-controller / per-question deep dives

| file | purpose |
|---|---|
| [`VARDIFF_SURVEY.md`](./VARDIFF_SURVEY.md) | What's actually deployed in open source: the archetype survey (classic / deadband / share-driven family / champion), the two-gate decline-safety taxonomy, and the source-verified NOMP·ckpool·MiningCore reads. |
| [`CKPOOL_INVESTIGATION.md`](./CKPOOL_INVESTIGATION.md) | Porting ckpool's multi-window EMA + hysteresis to the tick framework: what transfers, what doesn't, and the EWMA(120) gate-test addendum. |
| [`PID_INVESTIGATION.md`](./PID_INVESTIGATION.md) | The pow2-in-loop PID (DMND-style) deadband analysis: the structural dead-zone that puts the easing crossing below reachable rate. |
| [`NOMINAL_HASHRATE_COLDSTART.md`](./NOMINAL_HASHRATE_COLDSTART.md) | Using firmware-reported `nominal_hash_rate`: cold-start seed (retracted after measurement) vs. runtime fusion as an inert hint. |

## Verification — test specs & status registers

| file | purpose |
|---|---|
| [`SLOW_DECLINE_TEST.md`](./SLOW_DECLINE_TEST.md) | Spec + result for the sustained-decline death-spiral gate (the champion's selection criterion). |
| [`DECLINE_COLLAPSE_TEST.md`](./DECLINE_COLLAPSE_TEST.md) | Spec + result for the dimensionless-group / slope-aware matched-detector question (the floor-limited result). |
| [`LEVER_TEST_PREREG.md`](./LEVER_TEST_PREREG.md) | The pre-registered `r*`-lever F-statistic protocol (committed before the data) and its discharge. |
| [`PHASE_PORTRAIT_SPEC.md`](./PHASE_PORTRAIT_SPEC.md) | Instrumentation + figure spec for the `(depth × rate)` phase portrait, the depth-axis run, and the variance-inflation twin. |
| [`FIGURES-STATUS.md`](./FIGURES-STATUS.md) | Per-figure status against the closed theory — which structural figures are current vs. superseded-framing, so a clean-looking stale figure isn't reused. |
| [`OPEN_ITEMS.md`](./OPEN_ITEMS.md) | The cross-record status register: what is left to run, marked by status-type (runnable-open / sim-done-HW-pending / pending-on-data / closed). |

---

*Maintenance: this index lists purpose and group only — both stable. If a file's
purpose or group changes, update its line here; status is **not** carried here on
purpose (it would be a second source that goes stale — see the `OPEN_ITEMS.md`
"thin index, not a home" discipline). New record → add a row to its group.*
