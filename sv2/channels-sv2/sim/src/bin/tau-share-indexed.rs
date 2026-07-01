//! SHARE-INDEXED estimator — the rate-aware window, measured as a TRADEOFF.
//!
//! ===========================================================================
//! PRE-REGISTRATION (written BEFORE any number; the discipline that carried the
//! whole vardiff arc — lock the prediction, name it a tradeoff not a payoff).
//! ===========================================================================
//!
//! THE QUESTION. The fixed champion (Ewma360, TIME-indexed: α=exp(−tick/τ)) is
//! the gentlest decline-safe FIXED window under minimax-over-rate. tau-family.rs
//! measured that the per-rate over-difficulty optimum SLIDES (τ*∝1/r): a fixed
//! window is off-valley at the band edges. A SHARE-INDEXED window — decay per
//! share-batch, span counted in SHARES not seconds — is rate-aware BY
//! CONSTRUCTION and should track the per-rate valley floor. This binary measures
//! what that tracking costs and whether it stays admissible. It does NOT, and
//! cannot, decide whether share-indexing is "better" — see SCOPE.
//!
//! WHY SHARE-INDEXED, NOT τ-SCHEDULED (the build choice, and it is not taste).
//! The lazy alternative — make τ a function of measured rate, `τ_eff = k/r*` —
//! reintroduces the operand problem the arc kept hitting: `r*` there is the
//! NOISY share-derived estimate, so the time constant depends on the very
//! quantity it is smoothing (a rate-estimate → τ → rate-estimate FEEDBACK path
//! with its own stability question), and it is noisiest at LOW rate — exactly
//! the regime the feature targets. Share-indexing has no such loop: counting
//! shares instead of seconds makes the window rate-aware without ever computing
//! an `r*` to schedule on. So we build the share-indexed estimator (a new
//! `Estimator` impl alongside EwmaEstimator — NO trait change, NO channel
//! change), not the τ-schedule. The confound-avoidance is the reason, not
//! elegance.
//!
//! SCOPE — WHAT THIS SETTLES AND WHAT IT STRUCTURALLY CANNOT.
//!   (a) OVER-DIFFICULTY TRACKING — a CONSTRUCTION CHECK, not a finding. A
//!       constant-shares window tracks the per-rate optimum BY DEFINITION
//!       (that is what share-indexing IS), and tau-family already measured the
//!       optimum slides τ*∝1/r. So "does share-indexed track the sliding
//!       optimum" is near-vacuous as an empirical test — it passes unless the
//!       estimator is BUGGY. Relabel honestly: this verifies the code realizes
//!       the intended behavior (build-verification); it does NOT establish a
//!       result with a real chance of failing. If it does NOT track, that is a
//!       BUG, not a finding. The sim's real empirical content is NOT here.
//!   (b) ADMISSIBILITY of the JUMPY regime — the real test, and clean ONLY if the
//!       envelope stresses the RIGHT failure mode. The decline-safety gate is
//!       binary/one-sided (catches over-difficulty, not wobble), so a jumpy
//!       window passes it while being wobble-worse — fine, that is the tradeoff.
//!       BUT the fixed champion's risk is SLEEPY LAG; share-indexing's risk is
//!       JUMPY OVERSHOOT — a DIFFERENT failure mode. CONFIRMED AGAINST SOURCE:
//!       `slow-decline.rs` builds only `Phase::Hold` segments (mature → stepped
//!       declines → floor) and gates on SETTLED-e after recovery — it stresses
//!       sleepy lag and AVERAGES OUT the transient trough where a jumpy window is
//!       fragile. So passing THAT envelope is passing the cells that stress the
//!       OLD window's weakness, SILENT on the new one's. REQUIREMENT (locked):
//!       the sweep MUST add a scenario that stresses jumpy overshoot — a rate
//!       SPIKE / rapid increase or a recovery TRANSIENT — gated on the transient
//!       TROUGH (or peak over-difficulty during the transient), NOT only settled-e.
//!       Without that, P3's "admissible at every cell" is scoped to the wrong
//!       failure mode and is not a real pass. This is the load-bearing addition.
//!
//!       TROUGH-GATE DEFINITION — FULLY GROUNDED, zero chosen parameters (pinned
//!       BEFORE numbers; reached by reading the clock expression recursively
//!       until no controller parameter remained — see "the recursion" below).
//!       The gate has TWO δ's that measure DIFFERENT things and never touch:
//!
//!         BREACH:  over-difficulty `e > δ_threshold`, δ_threshold = 0.05.
//!           INHERITED from the settled-e gate — commensurate on WHAT COUNTS as
//!           over-difficulty (a LEVEL; 5% is the admissibility line). The breach
//!           test NEVER uses δ_clock. δ_threshold is the ONLY fixed parameter, and
//!           it is correctly fixed (it is the inherited admissibility level, not a
//!           soft mechanism crossover).
//!
//!         WINDOW:  a breach must be SUSTAINED ≥ W(r*) to count. W is the
//!           floor-limited spiral-onset time — how long ANY floor-limited
//!           estimator would take to detect an excursion of spiral-risk depth at
//!           the COLLAPSED rate (the best anyone could do → arm-independent):
//!             W(r*) = k · z² · e^(δ_clock) / (r* · δ_clock²)
//!           term by term, every one channel/floor/mechanism/swept:
//!             · 1/δ_clock²  — the Fisher/information floor (Var ≥ 1/(r*·τ_eff) ⇒
//!               resolving δ needs τ_eff ≥ z²/(r*·δ²) shares-worth-of-time);
//!             · e^(δ_clock) — the `e^(−e)` collapse factor at the spiral-risk
//!               depth (the mechanism that drives the spiral), so r_obs = r*·e^(−δ);
//!             · r*          — the rate (independent variable);
//!             · z²          — the detection-CONFIDENCE constant (how many σ of
//!               separation counts as "detected"; "resolve δ" means δ ≥ z·SE).
//!               This is the FLOOR-ANALOGUE OF THE α we dropped — a confidence
//!               knob hiding inside "detection time." It is O(1) and a pure
//!               MULTIPLIER on W, so it FOLDS INTO the swept k (k·z² is one
//!               relabeled swept constant; a 1σ-vs-2σ choice is within the
//!               k-sweep's span). NOT a separate fixed knob — verified, not assumed.
//!             · α, τ        — ABSENT. The SPRT 2·log(1/α) (controller confidence
//!               overhead, back-door τ) was dropped; no controller window appears.
//!
//!         THE TWO δ's, why different and why both right:
//!           δ_threshold (5%) is a LEVEL — correct for "is this over-difficulty
//!             unacceptable," inherited so both gates agree on what counts.
//!           δ_clock is a TIMESCALE-SETTING DEPTH — must be where `e^(−e)`
//!             MATERIALLY collapses the stream (~0.5: e^(−0.5)≈0.6, a 40% collapse,
//!             genuine spiral territory), NOT 5% (e^(−0.05)≈0.95, no collapse —
//!             grounding the CLOCK there gave the implausibly-long ~17–133min W,
//!             the gate reporting it measured the wrong quantity). At δ_clock=0.5
//!             W ≈ k·6.6/r* — a genuine TRANSIENT window, ~60× shorter, cleanly
//!             separated from the 120-min settle window, which is what lets the
//!             gate catch a short deep jumpy spike as transient.
//!
//!         TWO SWEPT SENSITIVITY AXES (neither chosen; both reported as curves):
//!           k       ∈ {0.5, 1, 2, 4}  — soft crossover multiplier (spiral-onset
//!             is a soft threshold, not sharp; absorbs z²).
//!           δ_clock — swept WIDE, [0.1, 0.9] (or until the boundaries appear),
//!             NOT pre-narrowed to [0.3,0.7]. The band edges are NOT waste: at low
//!             δ_clock the clock is so slow both arms PASS (confirms "too shallow
//!             ⇒ vacuous gate"); at high δ_clock so fast both arms FAIL (confirms
//!             "too deep ⇒ impossibly strict"); the differential, if real, lives
//!             BETWEEN those boundaries and is LOCATED by them — seeing all three
//!             (low-both-pass, middle-differential, high-both-fail) is what proves
//!             the differential is not a windowing artifact. Pre-narrowing to
//!             [0.3,0.7] would define the band by the answer expected (excluding
//!             the regions where the result is predicted) — the last smuggled
//!             choice; sweeping wide turns the boundaries into the calibration.
//!
//!       So: breach at δ_threshold (level, fixed, inherited), window at δ_clock
//!       (depth, swept wide), k swept (absorbs the confidence constant), r* the
//!       independent variable, α/τ absent. ZERO chosen parameters; two reported
//!       sensitivity axes; the differential located against its own boundaries.
//!
//!       THE RECURSION (why this is now actually closed). "Read the rig before
//!       trusting it" applied recursively, four levels, each the same shape — a
//!       parameter of the thing-being-judged hiding in the judge, caught by
//!       reading the actual expression not the clean-looking form: (1) slow-decline
//!       probed sleepy-lag not jumpy-overshoot (wrong failure mode); (2) W grounded
//!       in τ (the controller's window — circular); (3) T_d carried α (controller
//!       confidence — back-door τ); (4) "resolve δ" carried z² (the floor-analogue
//!       confidence constant — folds into k). The recursion TERMINATES here: every
//!       term is provably channel/floor/mechanism/swept, nothing controller-specific
//!       remains. That termination IS what "fully grounded" means.
//!
//!       CHAMPION AS BASELINE ON THE NEW SCENARIO (the control — non-optional).
//!       The fixed champion cleared the SETTLED-e gate; it was NEVER tested for
//!       transient overshoot. So run the CHAMPION through the new jumpy-overshoot
//!       stressor too, as the baseline arm. Three outcomes, only interpretable
//!       WITH the control: (champ PASS, SI FAIL) = real finding, SI introduces a
//!       transient breach the champion doesn't; (both PASS) = jumpy risk bounded,
//!       the tradeoff is purely the wobble-magnitude one; (both FAIL) = the
//!       trough-gate is TOO STRICT (it fails a config already shipped) → recalibrate
//!       W/threshold, do not report SI as inadmissible. Share-indexing's pass/fail
//!       on the new gate is UNINTERPRETABLE without the champion's pass/fail on
//!       the SAME gate — that is the bar-calibration control, same "verify against
//!       the right baseline" move as the rest of the arc.
//!   REAL EMPIRICAL CONTENT of the sim, stated precisely: (i) the MAGNITUDE of the
//!   wobble cost across the band (the tradeoff curve's y-axis), and (ii) whether
//!   the jumpy regime clears a gate whose envelope ACTUALLY stresses jumpy
//!   overshoot. Not (a) (construction), and not (c) below.
//!   NOT settleable here — and not for want of a better rig:
//!     (c) NET SUPERIORITY ("is it the better controller"). Share-indexing tracks
//!         the per-rate OVER-DIFFICULTY optimum, which at high rate IS the short,
//!         jumpy window tau-family-safety.rs already flagged at 2–3× the
//!         champion's WOBBLE. So the predicted result is the SAME two-primitive
//!         tradeoff, now spread across the band: share-indexing rebalances toward
//!         over-difficulty at every rate, PAYING wobble for it. Whether that net
//!         trade is "better" bottoms out in the over-diff/wobble scalar weighting
//!         the project DELIBERATELY refuses to fix (constraint-not-cost, §9.3) —
//!         the same place the champion's selection bottomed out in a tie-break,
//!         not an optimum. So the honest OUTPUT is a measured TRADEOFF CURVE
//!         (over-difficulty recovered, wobble paid, both inside the gate), NOT a
//!         payoff and NOT a verdict. Calling it a "payoff" is the one-axis
//!         overclaim the tau_tradeoff caption took four passes to scrub out;
//!         do not reintroduce it here.
//!
//! PRE-REGISTERED PREDICTIONS (each falsifiable; locked before numbers):
//!   P1 (CONSTRUCTION CHECK, not a finding — see SCOPE(a)): across the band,
//!      share-indexed's worst over-difficulty area is ≤ fixed-360's at the band
//!      EDGES. This passes unless the estimator is buggy; a FAIL here is a BUG
//!      (the optimum-tracker fails to track), not an empirical result. Kept as a
//!      build-verification, NOT counted as a settled finding.
//!   P2 (the cost — a MEASURED MAGNITUDE, the real output): share-indexed's
//!      wobble is HIGHER than fixed-360's where it tracks a shorter effective
//!      window (high spm), buying over-difficulty with wobble — the same trade as
//!      τ=30 in tau-family-safety. The DELIVERABLE is the magnitude of that
//!      wobble cost per rate (the tradeoff curve's y-axis), not a pass/fail.
//!      FAILS IF: it lowers over-difficulty WITHOUT raising wobble — a free lunch
//!      the two-primitive structure says doesn't exist (would need explaining,
//!      not celebrating).
//!   P3 (admissibility — the binary gate, AGAINST A JUMPY-OVERSHOOT ENVELOPE):
//!      share-indexed clears the decline-safety gate at EVERY rate×spm cell, ON A
//!      SWEEP THAT INCLUDES the jumpy-overshoot stressor (rate spike / recovery
//!      transient, gated on the transient trough) — NOT only the sleepy-lag
//!      decline `slow-decline.rs` currently sweeps. FAILS IF: any cell breaches —
//!      the rate-aware window is INADMISSIBLE somewhere fixed-360 was safe, and
//!      the feature is gated out regardless of over-difficulty tracking
//!      (admissibility is necessary, §9.2). A pass on the SLEEPY-LAG-ONLY
//!      envelope does NOT satisfy P3 — that would be scoped to the wrong failure
//!      mode (SCOPE(b)). The gate's trough WINDOW+THRESHOLD must be pre-pinned and
//!      the CHAMPION run through the SAME stressor as the baseline (SCOPE(b)); a
//!      breach is only a finding if the champion CLEARS it (else the gate is too
//!      strict, recalibrate). This is the load-bearing test.
//!   P4 (the EXPECTED SHAPE — a reporting commitment, NOT a weighting): the
//!      result is a RATE-STRUCTURED tradeoff — FAVORABLE at low rate (over-diff
//!      stakes high, wobble already low) and UNFAVORABLE at high rate (over-diff
//!      already small, wobble cost 2–3×), net value UNASSERTED throughout. This
//!      is the inverted-attractiveness point from §8.3 (the trade is worst where
//!      the over-diff RATIO is largest). Pre-committing this shape locks against
//!      reading a clean LOW-rate number post-hoc as a GLOBAL win. It does NOT fix
//!      the weighting (still refused); it fixes that the CONCLUSION reports the
//!      tradeoff's rate-structure, which the prior already predicts. FAILS IF:
//!      the tradeoff is NOT rate-structured (e.g. uniformly favorable) — which
//!      would contradict the §8.3 inverted-ratio finding and need reconciling.
//!   P4b (the NOT-WORTH-BUILDING-ANYWHERE outcome — pre-registered as a REAL
//!      possible result, not a failure to explain away). The favorable regime is
//!      LOW rate — but the per-rate slide there is only 360→~240 (spm 2),
//!      MODEST; the DRAMATIC slide (360→30) is at HIGH rate, exactly the
//!      UNFAVORABLE regime (jumpy, wobble-costly, small absolute stakes). So the
//!      live possibility, pre-registered: share-indexing's BIG over-difficulty
//!      recoveries are all where the trade is bad, and its favorable-regime
//!      (low-rate) recovery is MARGINAL — in which case the honest conclusion is
//!      "not worth the wobble cost ANYWHERE: big wins in the unfavorable regime,
//!      marginal wins in the favorable one." The question P4 must FORCE is not
//!      just "is the low-rate trade favorable in DIRECTION" (yes, by the
//!      asymmetry) but "is the low-rate over-difficulty recovery LARGE ENOUGH to
//!      be worth ANY wobble at all" — and 240-vs-360 modesty says that is
//!      genuinely open, possibly NEGATIVE. A clean "tracks the optimum + stays
//!      admissible" result must NOT be read as "worth building" if the
//!      recovery-where-favorable is marginal. Pre-registering P4b stops that read.
//!   NOTE: there is NO "share-indexed wins" prediction, by construction — net
//!   value is the unfixed weighting. P1 is a build-check; P2's magnitude + P4's
//!   rate-shape ARE the tradeoff; P3 is whether the tradeoff is even on the table
//!   (admissible against the RIGHT failure mode) at all.
//!
//! HARDWARE CAVEAT (record with the result, do not soften). Even a clean sim
//! here is the CEILING of what is knowable without a different deployment — and
//! plausibly the ceiling FULL STOP. The share-indexing payoff is a SECOND-ORDER
//! effect: the DIFFERENCE between two windows that BOTH sit on the 1/√(r*τ) floor.
//! The fixed champion's FIRST-order hardware claims (settled offset, lever
//! √-scaling) already came back sim-only — below significance at the converged
//! sample counts this topology produced. A second-order effect needs MORE power
//! to resolve a SMALLER difference, on a deployment that couldn't resolve the
//! larger ones, AND needs per-device multi-rate channels the translator-aggregate
//! topology does not provide (the per-device-carriage gap, cf. the telemetry-hint
//! live-validation block). So the honest status of any result here is
//! "sim-validated; hardware-pending AND possibly below hardware resolution in
//! principle" — not a waystation to hardware truth, but likely its ceiling.
//!
//! Usage: cargo run --release --bin tau-share-indexed
//! Env: VARDIFF_SI_TRIALS (default 80 base, CI-scaled), VARDIFF_SI_OUT.
//!
//! THE HEADLINE (the answer to "how hard to build and settle"): the build is
//! HOURS (estimator) plus a DAY-PLUS (the jumpy-overshoot scenario + its
//! trough-gate calibrated against the champion baseline — that calibration is
//! where the last mis-scope would hide, not a separable extra). But "SETTLE" in
//! the sense of "is rate-aware the BETTER vardiff" is NOT REACHABLE — not for
//! want of effort, but because the question bottoms out in (i) the over-diff/
//! wobble weighting the project refuses to fix, and (ii) a SECOND-ORDER effect
//! the deployment may be unable to resolve. The feature can be promoted to
//! "sim-validated, with a measured rate-structured TRADEOFF, admissibility
//! cleared against the right failure mode" — i.e. CHARACTERIZED — and no
//! further. That ceiling is a property of the QUESTION and the DEPLOYMENT, not
//! the effort: it is a day from CHARACTERIZED, never a day from SETTLED.
//!
//! ===========================================================================
//! RESULT — the study ran its PREMISE-CHECK stage and the chain TERMINATED there,
//! at the right place. The ShareIndexedEstimator is BUILT (estimator.rs, P1
//! construction-check passes: first-obs-sets-rate, converges-to-steady-input,
//! coincides-with-time-EWMA at the reference rate to <1e-9, holds-on-empty-tick).
//! Before building the graded sweep (P2/P3/P4), the single premise the whole
//! feature rests on — τ*∝1/r, "the per-rate optimum is a constant share count" —
//! was measured. It does NOT hold, and that collapses the reason to build further.
//! ===========================================================================
//!
//! WHAT WAS MEASURED (three probes; each refuted a premise this study assumed):
//!  1. τ*∝1/r is REFUTED AS AN EXACT FORM — the optimum still SLIDES, just gentler
//!     than 1/r (NOT "the optimum is rate-invariant"; the slide is real and
//!     confirmed). `tau-optimum-fit.rs` (fine short-τ-weighted grid, per-rate argmin
//!     WITH valley-sharpness error bars): the per-rate optimum expressed IN SHARES is
//!     NOT constant — spm6 band [2.4,5.6] vs spm30 band [12,12] are DISJOINT (the
//!     slope exceeds the bars). τ* slides GENTLER than 1/r. The earlier coarse-grid
//!     "4.5→15" was confirmed REAL curvature (not one-grid-step quantization) by the
//!     error-bar refinement. CAVEAT recorded by the binary itself: spm12/20/30
//!     argmins RAILED at the grid edge τ=24 — off-grid lower bounds.
//!     RECONCILES WITH §8.3 (required — both artifacts describe this optimum and must
//!     not appear to disagree): §8.3's DURABLE content is the DIRECTIONAL slide (the
//!     minimax hides a per-rate optimum that MOVES with rate, sleepier-when-sparse) —
//!     CONFIRMED, stands. What this REFUTES is §8.3's EXACT-PROPORTIONALITY LABEL,
//!     "τ*∝1/r made literal," which §8.3 attached on a grid IT ITSELF flagged "railed
//!     at the grid edge / coarse." The refutation is the REFINEMENT'S OWN clean
//!     evidence — the WITHIN-BOUNDARY-REGIME disjoint bands (spm6 [2.4,5.6] vs spm30
//!     [12,12], one controller, no guard switch) — NOT a re-reading of §8.3's
//!     argmins. (Do NOT cite the spm2→spm30 shares-opt "8→15 rising" as corroboration:
//!     that pair CROSSES the spm<6/≥6 guard switch — two stitched regimes, not one
//!     curve — exactly the cross-switch artifact flagged as unreadable-as-a-slope.
//!     §8.3's coarse, guard-fractured argmins are too crude to have "hinted" the
//!     within-regime slope; the refinement ESTABLISHED it.) So: §8.3's directional
//!     slide stands; the ∝1/r label was the coarse-grid overreach, refuted by the
//!     refinement on its own within-regime evidence. "REFUTED" scopes to the EXACT
//!     proportionality, NOT to the slide.
//!  2. The high-rate optimum is REAL and SUB-TICK, not estimator degeneracy.
//!     `tick-floor-probe.rs`: the over-difficulty curve is SMOOTH-monotone toward
//!     short τ in BOTH arms straddling the 60s tick (no erraticism below the tick)
//!     — so the railing at τ=24 is a true optimum BELOW the tick floor, not the
//!     scan landing on noise past a resolution edge. The high-rate optimum wants a
//!     window the 60s sim tick cannot represent.
//!  3. "share-indexed worse on declines" is REFUTED as a matched-window ARTIFACT.
//!     `decline-window-trace.rs` at the GROUNDED n_span=72: α_share≈0.66 < champion
//!     0.846 throughout the decline — share-indexed runs a SHORTER window (jumpier),
//!     and carries LESS over-difficulty (area 899 < 1156), not more. The probe's
//!     "worse" was matched-to-LONG-τ (a longer window lags more — a property of the
//!     window length, not of share-indexing). The "holds-on-empty-ticks stretches
//!     the window on a decline" mechanism I had hypothesized does NOT occur at the
//!     grounded config. No decline liability.
//!
//! THE STRUCTURAL FINDING (what the surviving measurement supports — a DIRECTION,
//! never a value, and SCOPED TO ONE AXIS): the OVER-DIFFICULTY-optimal rate-coupling
//! is INTERMEDIATE — window ∝ 1/r^p with p<1. Relative to the OVER-DIFFICULTY
//! optimum, the fixed champion (p=0, window rate-independent) is too sleepy at high
//! rate and constant-share indexing (p=1, window ∝ 1/r exactly) OVER-couples (slides
//! the window harder than the over-difficulty optimum slides, hence too jumpy at
//! high rate). On THAT axis the over-difficulty-optimal form couples window-to-rate
//! LESS than proportionally — both endpoints are off it, for opposite reasons.
//!
//! THE DEPLOYMENT CLAUSE (do NOT read the above as a ship-this verdict — it is a
//! one-axis result, and the project's whole discipline is not letting one axis
//! stand in for the two-axis balance). The τ*-slope is the rate-dependence of the
//! OVER-DIFFICULTY optimum — where the GATE's preferred coupling sits. But the
//! DEPLOYED coupling (the champion's analogue: gentlest-admissible, ORDERED by
//! wobble within the gate-admissible set) has a DIFFERENT rate-dependence — a slope
//! this study did NOT measure. So:
//!   · "p<1 is over-difficulty-optimal" is ESTABLISHED (the surviving slope).
//!   · "p<1 is the DEPLOYMENT-optimal coupling" is UNMEASURED. Measuring it needs
//!     the WOBBLE axis swept ACROSS RATE — the Stage-3 study deliberately NOT run.
//!     Wobble pulls the deployment target away from the over-difficulty optimum
//!     (§8.3); whether it pulls toward LESS coupling or MORE is exactly what the
//!     unrun sweep would say. p=1-over-couples-on-over-difficulty does NOT establish
//!     p=1-is-the-wrong-deployment-coupling.
//!   · RECONCILIATION WITH THE COMMITTED §8.3 FLAG (required — both this record and
//!     the flag describe the champion's coupling and must not appear to disagree):
//!     §8.3 says the champion (p=0, fixed-τ) sits SLEEPER than the over-difficulty
//!     optimum DELIBERATELY, because wobble pulls it there — "360 is the balance,
//!     not a mistake." THAT VERDICT STANDS and this finding does NOT contradict it:
//!     they concern DIFFERENT optima. p=0 being over-difficulty-suboptimal-toward-
//!     sleepy is EXACTLY what trading toward the safe wobble axis looks like; calling
//!     p=0 "wrong" would be wrong, because it is correctly placed on the BALANCE
//!     while being off the over-difficulty optimum. This finding is about the
//!     over-difficulty optimum; §8.3 is about the balance. No conflict.
//!
//! HONEST SCOPE — this rests on ONE measurement, not two, and supports a DIRECTION
//! not a VALUE:
//!  - It rests on ONE measurement: the τ*-slope (#1). The grounded-trace finding
//!    (#3-corrected: constant-share is short/jumpy at high rate) is that slope's
//!    CONSEQUENCE, not independent corroboration. "Constant-share is too jumpy" =
//!    "its window is shorter than the optimum" = "the optimum slid gentler than
//!    1/r" = the slope, restated. One finding in two presentations — NOT two
//!    convergent confirmations. p<1 is exactly as well-supported as the slope is,
//!    and no better. (Independence check, demanded before this was made durable:
//!    confirmed NOT independent — same measurement twice.)
//!  - p is a DIRECTION, not a fittable VALUE on this rig. Three contaminants, each
//!    of the very slope you would fit p from: (a) the high-rate optima are RAILED
//!    (off-grid lower bounds, #1 caveat) — biasing a fitted p TOO LOW (too-long
//!    railed optima make the slope look gentler than it is); (b) the spm<6/≥6 guard
//!    switch FRACTURES the slope (two different controllers — a single p across them
//!    is meaningless); (c) the high-rate optimum is SUB-TICK (#2), so p would be fit
//!    partly where the optimal window is UNREPRESENTABLE at 60s. Clean p-fittable
//!    data is a NARROW mid-rate slice (~spm6–8: two located points plus three
//!    railed) — too little to pin p, biased where it isn't. Fitting p would extract
//!    a NUMBER from data that supports only a DIRECTION — the exact error this arc
//!    kept catching, one level deeper.
//!
//! WHY NO FURTHER BUILD (the chain terminated correctly, not for fatigue):
//!  - DON'T fit p and build the 1/r^p estimator: p is biased (railing) and
//!    unsupported where it matters (sub-tick) — building on it repeats the
//!    coarse-grid τ*∝1/r mistake one level down.
//!  - DON'T build the plain p=1 (constant-share) graded sweep: the analysis just
//!    identified p=1 as a WRONG ENDPOINT (over-coupled); characterizing it in
//!    isolation characterizes a known-suboptimal config.
//!  - The design finding (intermediate coupling; both endpoints wrong for opposite
//!    reasons) IS the deliverable, reached analytically from the refined τ*(r) plus
//!    the traces. P2/P3/P4/P4b stand as the spec they always were — NOT run,
//!    because the premise that motivated them did not survive its own check.
//!
//! THE COMPLETE CEILING — FIVE WALLS, distinguished by WHY each is a wall (three
//! resolution walls, one not-run, one policy — the honest end-state):
//!   • SHAPE of the over-difficulty answer: KNOWN — over-difficulty-optimal coupling
//!     is intermediate, p<1, both extremes off it for opposite reasons.
//!   • VALUE of p: UNRESOLVABLE on this rig — railed + guard-fractured + sub-tick
//!     (sim-tick resolution wall, for the parameter).
//!   • the high-rate OPTIMUM itself: SUB-TICK — below the 60s clock (sim-tick
//!     resolution wall, for the optimum).
//!   • DEPLOYMENT coupling (is the gate-plus-wobble BALANCE also p<1, and what is
//!     its p): UNMEASURED — NOT RUN, distinct from sub-resolution. It needs the
//!     WOBBLE axis swept across rate (the Stage-3 study deliberately not built); the
//!     over-difficulty slope measured here does not determine it. This is precisely
//!     what stopping at the design finding leaves open: the over-difficulty axis's
//!     coupling shape is in hand; the deployment axis's coupling is the thing not
//!     built. (Consistent with §8.3: the champion is balanced on this axis, p=0.)
//!   • NET VALUE ("is it better"): OPEN — the over-diff/wobble weighting the
//!     project refuses to fix (policy, not measurement).
//!   • HARDWARE validation: UNREACHABLE — a second-order effect below sample
//!     resolution; even the first-order champion claims came back sim-only.
//!   We know the STRUCTURE of the over-difficulty answer and the PRECISE reason each
//!   deeper level is unreachable — three resolution walls, one not-run, one policy.
//!   That is the chain terminating where it should.
//!
//! STATUS: estimator BUILT (estimator.rs, P1 passes); premise MEASURED and REFUTED
//! (tau-optimum-fit, tick-floor-probe, decline-window-trace); structural finding
//! ESTABLISHED (over-difficulty-optimal coupling is p<1 — a direction, ONE axis);
//! deployment coupling UNMEASURED (needs the unrun wobble-across-rate sweep); graded
//! sweep DELIBERATELY NOT BUILT (p sub-resolution; building it fits a biased number
//! or characterizes a known-wrong-on-over-difficulty endpoint). Characterized,
//! never settled — by the QUESTION, not the effort.

fn main() {
    eprintln!("tau-share-indexed: premise MEASURED, chain TERMINATED at the structural finding.");
    eprintln!("Estimator BUILT (estimator.rs P1 passes); graded sweep deliberately NOT built.");
    eprintln!();
    eprintln!("FINDING (ONE axis — over-difficulty): the OVER-DIFFICULTY-optimal rate-coupling is");
    eprintln!("  INTERMEDIATE — window ∝ 1/r^p, p<1. Relative to the over-difficulty optimum, fixed");
    eprintln!("  champion (p=0) is too sleepy at high rate; constant-share (p=1) over-couples (too jumpy).");
    eprintln!();
    eprintln!("DEPLOYMENT CLAUSE: this is NOT a ship-this verdict. The DEPLOYED coupling is the");
    eprintln!("  gate-plus-WOBBLE balance (the champion's criterion), a DIFFERENT slope NOT measured here.");
    eprintln!("  §8.3 stands: champion (p=0) is correctly BALANCED, not a mistake — different optimum.");
    eprintln!();
    eprintln!("EVIDENCE (one measurement + its consequence, NOT two independent confirmations):");
    eprintln!("  - tau-optimum-fit:      τ*∝1/r REFUTED — shares-opt spm6 [2.4,5.6] vs spm30 [12,12] DISJOINT.");
    eprintln!("  - tick-floor-probe:     high-rate optimum is REAL + SUB-TICK (smooth curves both arms).");
    eprintln!("  - decline-window-trace: 'share-indexed worse on declines' REFUTED — matched-window artifact.");
    eprintln!();
    eprintln!("CEILING (5 walls): over-diff shape KNOWN; p UNRESOLVABLE (railed+guard-fractured+sub-tick);");
    eprintln!("  DEPLOYMENT coupling UNMEASURED (unrun wobble-across-rate); net value OPEN (refused");
    eprintln!("  weighting); hardware UNREACHABLE (second-order). See the module header.");
}
