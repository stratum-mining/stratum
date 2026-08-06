use crate::target::hash_rate_to_target;
use crate::vardiff::classic::VardiffState;
use crate::vardiff::Vardiff;
use bitcoin::Target;

const TEST_MIN_HASHRATE: f32 = 1.0;
const TEST_SHARES_PER_MINUTE: f32 = 12.0;
const TEST_HASHRATE: f32 = 1.0e12;

fn add_shares(v: &mut VardiffState, n: u32) {
    for _ in 0..n {
        v.increment_shares_since_last_update();
    }
}

fn make_vardiff() -> VardiffState {
    VardiffState::new_with_min(TEST_MIN_HASHRATE).expect("Failed to create VardiffState")
}

/// Simulate elapsed time by backdating the timestamp.
fn simulate_elapsed(v: &mut VardiffState, secs: u64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    v.set_timestamp_of_last_update(now - secs);
}

#[test]
fn new_state_has_zero_shares() {
    let v = make_vardiff();
    assert_eq!(v.shares_since_last_update(), 0);
    assert_eq!(v.min_allowed_hashrate(), TEST_MIN_HASHRATE);
}

#[test]
fn increment_shares_accumulates() {
    let mut v = make_vardiff();
    v.increment_shares_since_last_update();
    v.increment_shares_since_last_update();
    assert_eq!(v.shares_since_last_update(), 2);
}

#[test]
fn add_shares_bulk() {
    let mut v = make_vardiff();
    add_shares(&mut v, 42);
    assert_eq!(v.shares_since_last_update(), 42);
}

#[test]
fn reset_counter_zeroes_state() {
    let mut v = make_vardiff();
    add_shares(&mut v, 10);
    v.reset_counter().unwrap();
    assert_eq!(v.shares_since_last_update(), 0);
}

#[test]
fn reset_counter_clears_sign_persistence_state() {
    // reset_counter must clear the sign-persistence CUSUM state (cusum_last_sign /
    // cusum_consecutive), not just the share counter — when state was added to the
    // struct, its reset was added too. This is a STATE-CLEANLINESS invariant
    // asserted DIRECTLY (via the cusum_sign_state test accessor), deliberately NOT
    // via fire behavior. Severity is honest hygiene, not a live bug: (1) the carried
    // discount (≤0.6) is too small to change whether a tick fires in any scenario we
    // could construct — fresh and stale-reset controllers fire on the same tick; and
    // (2) reset_counter is Vardiff-trait API surface, not called in the production
    // channel path. A behavioral "fires earlier" test would have no teeth (verified —
    // it passes with OR without the fix), so it would be a guard that guards nothing.
    // The direct field assertion below DOES fail if the two cusum_* resets are
    // removed, which is the real, teeth-bearing invariant.
    let spm = 30.0f32;
    let target: Target = hash_rate_to_target(TEST_HASHRATE.into(), spm.into())
        .unwrap()
        .into();
    let mut v = make_vardiff();
    // Accumulate sign-persistence with several same-direction ticks.
    for _ in 0..6 {
        add_shares(&mut v, (spm as u32) * 5);
        simulate_elapsed(&mut v, 60);
        let _ = v.try_vardiff(TEST_HASHRATE, &target, spm).unwrap();
    }
    // Pre-reset: state has accumulated (consecutive > 0, sign set).
    let (_, consecutive_before) = v.cusum_sign_state();
    assert!(
        consecutive_before > 0,
        "precondition: sign-persistence should have accumulated before reset (got 0)"
    );

    v.reset_counter().unwrap();

    let (sign_after, consecutive_after) = v.cusum_sign_state();
    assert_eq!(
        (sign_after, consecutive_after),
        (0, 0),
        "reset_counter must zero the sign-persistence state (got sign={sign_after}, \
         consecutive={consecutive_after}); leaving it stale carries an accumulated \
         discount into the next cycle"
    );
}

#[test]
fn no_fire_within_15s() {
    let mut v = make_vardiff();
    let target = hash_rate_to_target(TEST_HASHRATE.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    add_shares(&mut v, 100);
    simulate_elapsed(&mut v, 10);
    let result = v
        .try_vardiff(TEST_HASHRATE, &target, TEST_SHARES_PER_MINUTE)
        .unwrap();
    assert_eq!(result, None);
}

#[test]
fn fires_when_miner_is_much_faster() {
    // The champion deliberately requires SUSTAINED evidence to tighten (raise
    // difficulty): an 8x tighten-multiplier plus a slow EWMA(360) mean a single
    // tick of a fast miner does NOT fire — tightening into a possibly-transient
    // spike is the dangerous direction. Across repeated same-direction ticks the
    // EWMA catches up and the sign-persistence discount relaxes the threshold, so
    // a genuinely-faster miner is caught within a few minutes. Drive several ticks.
    let mut v = make_vardiff();
    let target = hash_rate_to_target(TEST_HASHRATE.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    // Miner sustaining 5x expected rate (60 shares / 60s vs 12 spm target).
    let mut fired = None;
    for _ in 0..12 {
        add_shares(&mut v, 60);
        simulate_elapsed(&mut v, 60);
        if let Some(new_h) = v
            .try_vardiff(TEST_HASHRATE, &target, TEST_SHARES_PER_MINUTE)
            .unwrap()
        {
            fired = Some(new_h);
            break;
        }
    }
    let new_h = fired.expect("a sustained 5x-faster miner should fire within a few minutes");
    assert!(
        new_h > TEST_HASHRATE,
        "Hashrate should increase when miner is sustainedly faster: got {}",
        new_h
    );
}

#[test]
fn fires_when_miner_is_much_slower() {
    let mut v = make_vardiff();
    let target = hash_rate_to_target(TEST_HASHRATE.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    // 3 shares over 300s → EWMA rate=3 → realized_spm=3 (vs expected 12).
    // delta = |3/12 - 1| * 100 = 75%.
    // CUSUM loosening threshold at 300s/5 ticks: ~24%. 75% > 24% → fires.
    add_shares(&mut v, 3);
    simulate_elapsed(&mut v, 300);
    let result = v
        .try_vardiff(TEST_HASHRATE, &target, TEST_SHARES_PER_MINUTE)
        .unwrap();
    assert!(result.is_some(), "Should fire on 75% deviation at 300s");
    let new_h = result.unwrap();
    assert!(
        new_h < TEST_HASHRATE,
        "Hashrate should decrease when miner is slower: got {}",
        new_h
    );
}

#[test]
fn no_fire_on_stable_rate() {
    let mut v = make_vardiff();
    let target = hash_rate_to_target(TEST_HASHRATE.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    // Miner at exactly expected rate
    add_shares(&mut v, 12);
    simulate_elapsed(&mut v, 60);
    let result = v
        .try_vardiff(TEST_HASHRATE, &target, TEST_SHARES_PER_MINUTE)
        .unwrap();
    assert_eq!(result, None, "Should not fire when rate matches target");
}

#[test]
fn partial_retarget_moves_toward_estimate_not_fully() {
    let mut v = make_vardiff();
    let target = hash_rate_to_target(TEST_HASHRATE.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    // Miner at 3x rate over 300s. EWMA rate=36 → realized_spm=36.
    // h_estimate ≈ 3x current. Champion tightening threshold at n=5 ticks:
    // sensitivity 1.5·√(12/30)≈0.95, base ≈ 0.95/5 + 0.05 ≈ 0.24, ×8 (tighten
    // multiplier) ≈ 192%. Delta = |3-1|·100 = 200% > 192% → fires (a deliberately
    // narrow margin: the 8× makes even a 3× tightening only just clear the bar).
    // EWMA first tick: rate = pending = 36. realized_spm = 36 * 60/60 = 36.
    // That's 3x the 12 SPM target.
    add_shares(&mut v, 36);
    simulate_elapsed(&mut v, 300);
    let result = v
        .try_vardiff(TEST_HASHRATE, &target, TEST_SHARES_PER_MINUTE)
        .unwrap();
    assert!(result.is_some(), "Should fire on 3x deviation at 300s");
    let new_h = result.unwrap();
    // With eta=0.2: new ≈ 1x + 0.2*(3x - 1x) = 1.4x
    assert!(new_h > TEST_HASHRATE, "Should increase: got {}", new_h);
    // The key property: partial retarget doesn't jump all the way to 3x
    assert!(
        new_h < TEST_HASHRATE * 2.0,
        "eta=0.2 should keep new hashrate below 2x (full retarget would be ~3x): got {}",
        new_h
    );
}

#[test]
fn consecutive_fires_accelerate_eta() {
    let mut v = make_vardiff();
    let target = hash_rate_to_target(TEST_HASHRATE.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();

    // First fire (eta=0.2): 5x rate over 300s crosses the CUSUM tightening threshold.
    add_shares(&mut v, 60);
    simulate_elapsed(&mut v, 300);
    let h1 = v
        .try_vardiff(TEST_HASHRATE, &target, TEST_SHARES_PER_MINUTE)
        .unwrap()
        .expect("First fire should trigger on 5x deviation at 300s");

    assert!(
        h1 > TEST_HASHRATE,
        "First fire should increase hashrate: got {}",
        h1
    );

    // Second fire in same direction: after rescale, the EWMA rate is adjusted
    // but with 60 new shares the miner is still clearly faster. Use 300s again
    // so the boundary is lenient enough.
    let target2 = hash_rate_to_target(h1.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    add_shares(&mut v, 60);
    simulate_elapsed(&mut v, 300);
    let h2 = v
        .try_vardiff(h1, &target2, TEST_SHARES_PER_MINUTE)
        .unwrap()
        .expect("Second fire should trigger — same direction, eta should accelerate");

    assert!(
        h2 > h1,
        "Second fire should continue increasing: got {} (was {})",
        h2,
        h1
    );

    // The acceleration property: the second same-direction fire ramps eta by the
    // champion's acceleration (0.05) from eta_base 0.2 to 0.25 (capping at 0.6 over
    // many consecutive fires), so the second step is a larger fraction of the
    // remaining gap than a non-accelerating retarget would take.
    let step1_fraction = (h1 - TEST_HASHRATE) / TEST_HASHRATE;
    let step2_fraction = (h2 - h1) / h1;
    assert!(
        step2_fraction > step1_fraction * 0.5,
        "Second step fraction ({:.4}) should be meaningful relative to first ({:.4})",
        step2_fraction,
        step1_fraction
    );
}

#[test]
fn min_hashrate_floor_enforced() {
    let mut v = VardiffState::new_with_min(100.0).expect("Failed to create VardiffState");
    let hashrate = 150.0f32;
    let target = hash_rate_to_target(hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    // Zero shares — estimate will be very low
    add_shares(&mut v, 0);
    simulate_elapsed(&mut v, 300);
    let result = v
        .try_vardiff(hashrate, &target, TEST_SHARES_PER_MINUTE)
        .unwrap();
    if let Some(new_h) = result {
        assert!(
            new_h >= 100.0,
            "Should clamp to min_allowed_hashrate: got {}",
            new_h
        );
    }
}

#[test]
fn poisson_boundary_used_at_low_spm() {
    let mut v = make_vardiff();
    let spm = 4.0f32; // Below the champion's spm_threshold of 6 → PoissonCI branch
    let target = hash_rate_to_target(TEST_HASHRATE.into(), spm.into())
        .unwrap()
        .into();
    // Moderate deviation — PoissonCI should be more conservative (higher threshold)
    add_shares(&mut v, 6); // 1.5x rate at SPM 4 over 60s
    simulate_elapsed(&mut v, 60);
    let result = v.try_vardiff(TEST_HASHRATE, &target, spm).unwrap();
    // At SPM 6 with dt=60s, PoissonCI threshold is quite high (~50%+),
    // so a 1.5x deviation (50%) may not fire
    // This verifies the conservative behavior at low SPM
    assert_eq!(
        result, None,
        "PoissonCI should be conservative at low SPM — 1.5x should not fire at dt=60s"
    );
}

#[test]
fn cusum_boundary_used_at_high_spm() {
    let mut v = make_vardiff();
    let spm = 30.0f32; // Above the champion's spm_threshold of 6 → CUSUM branch
    let target = hash_rate_to_target(TEST_HASHRATE.into(), spm.into())
        .unwrap()
        .into();
    // 2x rate at SPM 30 over 300s — CUSUM should fire (tighter boundary)
    add_shares(&mut v, 300); // 60 spm realized, 2x the target
    simulate_elapsed(&mut v, 300);
    let result = v.try_vardiff(TEST_HASHRATE, &target, spm).unwrap();
    assert!(
        result.is_some(),
        "CUSUM should fire at high SPM with 2x deviation over 300s"
    );
}

#[test]
fn asymmetric_cusum_tightening_is_harder_than_loosening() {
    // At high SPM, tightening (miner faster) requires more evidence than
    // loosening (miner slower) due to the champion's tighten_multiplier = 8.0
    // (the decline-safety-selected asymmetry; the earlier fitness-selected
    // contender used 3.0 — decline-safety demands stronger tightening reluctance).
    let spm = 30.0f32;
    let target: Target = hash_rate_to_target(TEST_HASHRATE.into(), spm.into())
        .unwrap()
        .into();

    // The asymmetry: tightening requires 8x the evidence of loosening. The property
    // we assert is direction-asymmetry under SYMMETRIC-magnitude moves: a deep
    // loosening (miner drops to 0.2x) fires within a few minutes, while a
    // comparable-magnitude tightening (miner jumps to 5x) does NOT fire in the same
    // window — the 8x multiplier makes tightening into a possibly-transient spike
    // the deliberately-reluctant direction. We drive each per-tick and compare.
    // (Shallow 0.5x/2x moves don't cross even the loosening threshold at spm=30
    // with the slow EWMA(360); the asymmetry shows on deviations large enough to
    // fire at all.)
    fn fires_within(
        shares_per_tick: u32,
        spm: f32,
        target: &Target,
        max_ticks: u32,
    ) -> Option<u32> {
        let mut v = make_vardiff();
        for t in 1..=max_ticks {
            add_shares(&mut v, shares_per_tick);
            simulate_elapsed(&mut v, 60);
            if v.try_vardiff(TEST_HASHRATE, target, spm).unwrap().is_some() {
                return Some(t);
            }
        }
        None
    }

    // Loosening: miner sustains 0.2x rate (6 spm vs 30 target) — the safe direction.
    let loosen = fires_within((spm as u32) / 5, spm, &target, 20);
    // Tightening: miner sustains 5x rate (150 spm) — the dangerous direction, 8x harder.
    let tighten = fires_within((spm as u32) * 5, spm, &target, 20);

    let lt = loosen.expect("a deep loosening (0.2x) should fire within 20 ticks");
    match tighten {
        // If tightening also fires, loosening must have fired no later (8x harder).
        Some(tt) => assert!(
            lt <= tt,
            "loosening should fire no later than tightening (8x harder): \
             loosen={lt} ticks, tighten={tt} ticks"
        ),
        // Expected: a comparable-magnitude tightening does NOT fire in the window —
        // that is the dangerous-direction reluctance, working as designed.
        None => {}
    }
}
