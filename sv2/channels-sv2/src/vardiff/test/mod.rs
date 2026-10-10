/// Contains a generic test implementation that is agnostic to the Vardiff implementation,
/// providing methods to verify the correctness of any specific implementation.
use std::{thread, time::Duration};

mod classic;

use super::Vardiff;
use crate::target::hash_rate_to_target;
use bitcoin::Target;

pub const TEST_INITIAL_HASHRATE: f32 = 1000.0;
pub const TEST_SHARES_PER_MINUTE: f32 = 10.0;
pub const TEST_MIN_ALLOWED_HASHRATE: f32 = 10.0;

// Helper function to simulate a number of shares being found over a given duration.
pub fn simulate_shares_and_wait<V: Vardiff>(
    vardiff: &mut V,
    num_shares: u32,
    wait_duration_secs: u64,
) {
    for _ in 0..num_shares {
        vardiff.increment_shares_since_last_update();
    }

    // Rather than waiting for wait_duration,
    // we are performing time magic and going
    // back in time.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - wait_duration_secs;

    vardiff.set_timestamp_of_last_update(now);
}

// Verifies that the share counter can be incremented and reset correctly.
pub fn test_increment_and_reset_shares<V: Vardiff>(vardiff: &mut V) {
    let initial_timestamp = vardiff.last_update_timestamp();

    vardiff.increment_shares_since_last_update();
    assert_eq!(vardiff.shares_since_last_update(), 1);

    vardiff.increment_shares_since_last_update();
    assert_eq!(vardiff.shares_since_last_update(), 2);

    thread::sleep(Duration::from_secs(1));

    vardiff.reset_counter().expect("Failed to reset counter");
    assert_eq!(vardiff.shares_since_last_update(), 0);

    assert!(
        vardiff.last_update_timestamp() > initial_timestamp,
        "Timestamp should update on reset"
    );
}

// A backwards clock step (e.g. an NTP correction) leaves the recorded timestamp in the
// future. `try_vardiff` must decline to adjust rather than panic on the elapsed-time
// subtraction, and it must re-anchor the window: the `delta_time <= 15` early return runs
// before `reset_counter()`, so leaving the future timestamp in place would stall vardiff
// for the entire length of the clock step, not just one round.
pub fn test_backwards_clock_step_reanchors_window<V: Vardiff>(vardiff: &mut V) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // Pretend the last update happened an hour in the future.
    vardiff.set_timestamp_of_last_update(now + 3600);

    let target =
        hash_rate_to_target(TEST_INITIAL_HASHRATE.into(), TEST_SHARES_PER_MINUTE.into()).unwrap();

    assert!(matches!(
        vardiff.try_vardiff(TEST_INITIAL_HASHRATE, &target, TEST_SHARES_PER_MINUTE),
        Ok(None)
    ));

    assert!(
        vardiff.last_update_timestamp() <= now + 1,
        "window not re-anchored (timestamp still {}, now {}): vardiff would stall for the \
         full length of the clock step",
        vardiff.last_update_timestamp(),
        now
    );
}

// Ensures that `try_vardiff` results in a minimal or no change when the hashrate is stable.
pub fn test_try_vardiff_stable_hashrate_minimal_change_or_no_change<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let iniital_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();

    let simulation_duration_secs = 5;
    let expected_shares_for_duration = 1;

    simulate_shares_and_wait(
        vardiff,
        expected_shares_for_duration,
        simulation_duration_secs,
    );

    let result = vardiff
        .try_vardiff(initial_hashrate, &iniital_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");

    if let Some(new_hashrate) = result {
        let diff_percentage = ((new_hashrate - initial_hashrate).abs() / initial_hashrate) * 100.0;
        println!(
            "Stable hashrate test: new hashrate {new_hashrate}, initial {initial_hashrate}, diff_pct {diff_percentage}"
        );
        assert!(
            diff_percentage < 20.0,
            "Change should be minimal for stable rate if any"
        );
        assert_eq!(vardiff.shares_since_last_update(), 0)
    } else {
        assert_eq!(None, result);
    }
}

// Tests if a high share submission rate correctly increases the difficulty (lowers the target).
pub fn test_try_vardiff_low_hashrate_decrease_target<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let initial_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();

    // 60 shares a minute against an expected 10, so this is an over-delivering miner and the
    // response is to tighten. The window is 120 seconds rather than 16 because tightening now
    // requires more evidence than loosening; 16 seconds of a 6x excess is 16 shares, which is not
    // enough to act on. Same rate, longer observation.
    let simulation_duration = 120;
    simulate_shares_and_wait(vardiff, 120, simulation_duration);

    let result = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "Hashrate should update due to high share count"
    );
    let new_hashrate = result.unwrap();

    // As estimated shares per minute is 10
    // with current setup realized shares per minute is 60
    // comes under no special case
    // A retarget now closes part of the gap, so the belief moves toward the
    // estimate of 6.0x without reaching it in one step.
    assert!(
        new_hashrate > initial_hashrate && new_hashrate < 6.0 * initial_hashrate,
        "belief should move toward 6.0x without arriving: got {new_hashrate}"
    );
    let target: Target = hash_rate_to_target(new_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    println!("target: {target:?}");
    assert!(
        target < initial_target,
        "Target should become harder (larger value)"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

// Checks the difficulty adjustment logic for a high share rate within a 30-second window.
pub fn test_try_vardiff_with_shares_less_than_30<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let initial_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();

    let simulation_duration = 16;
    simulate_shares_and_wait(vardiff, 500, simulation_duration);

    let result = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "Hashrate should update due to low share count"
    );
    let new_hashrate = result.unwrap();

    // This logic checks the `dt <= 30` case, which multiple by 10
    // A retarget now closes part of the gap, so the belief moves toward the
    // estimate of 10.0x without reaching it in one step.
    assert!(
        new_hashrate > initial_hashrate && new_hashrate < 10.0 * initial_hashrate,
        "belief should move toward 10.0x without arriving: got {new_hashrate}"
    );

    let target: Target = hash_rate_to_target(new_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    assert!(
        target < initial_target,
        "Target should become harder (larger value)"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

// Checks the difficulty adjustment logic for a high share rate within a 30 to 60-second window.
pub fn test_try_vardiff_with_shares_30_to_60s<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let initial_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();

    let simulation_duration = 31;
    simulate_shares_and_wait(vardiff, 5000, simulation_duration);

    let result = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "Hashrate should update due to low share count"
    );
    let new_hashrate = result.unwrap();

    // This logic checks the `dt < 60` case, which multiple by 5
    // A retarget now closes part of the gap, so the belief moves toward the
    // estimate of 5.0x without reaching it in one step.
    assert!(
        new_hashrate > initial_hashrate && new_hashrate < 5.0 * initial_hashrate,
        "belief should move toward 5.0x without arriving: got {new_hashrate}"
    );
    let target: Target = hash_rate_to_target(new_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    assert!(
        target < initial_target,
        "Target should become harder (larger value)"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

// Checks the difficulty adjustment logic for a high share rate over a 60-second window.
pub fn test_try_vardiff_with_shares_more_than_60s<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let initial_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();

    let simulation_duration = 60;
    simulate_shares_and_wait(vardiff, 1000, simulation_duration);

    let result = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "Hashrate should update due to low share count"
    );
    let new_hashrate = result.unwrap();

    // This logic checks the `dt >= 60` case, which multiple by 3
    // A retarget now closes part of the gap, so the belief moves toward the
    // estimate of 3.0x without reaching it in one step.
    assert!(
        new_hashrate > initial_hashrate && new_hashrate < 3.0 * initial_hashrate,
        "belief should move toward 3.0x without arriving: got {new_hashrate}"
    );
    let target: Target = hash_rate_to_target(new_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    assert!(
        target < initial_target,
        "Target should become harder (larger value)"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

/// An under-delivering channel should have its belief revised downward.
///
/// Asserts the property rather than a chain of exact values. The previous version pinned five
/// literals — 400.0, 200.0, 106.0, 74.2, 62.327995 — which were the cumulative mean's arithmetic
/// to full float precision. That encoded one estimator rather than the behaviour under test, so
/// any change to how the rate is estimated failed the test without telling the reader whether
/// the behaviour had actually regressed.
///
/// Each step delivers below the target rate, at a ratio that rises across the run
/// (0.4, 0.5, 0.55, 0.7, 0.85), so the deviation shrinks and a controller with a decision
/// boundary may legitimately decline to act on the later ones. Firing is therefore required only
/// once; what is required throughout is that the belief never moves *up* while the channel is
/// under-delivering.
fn test_try_vardiff_with_less_spm_than_expected<V: Vardiff>(vardiff: &mut V) {
    let mut hashrate = TEST_INITIAL_HASHRATE;
    assert_eq!(hashrate, 1000.0);
    let mut target: Target = hash_rate_to_target(hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();

    // (elapsed seconds, shares delivered) — every pair is below TEST_SHARES_PER_MINUTE
    let deliveries = [(60u64, 4u32), (120, 10), (180, 16), (240, 28), (300, 42)];
    let mut fires = 0;

    for (duration, shares) in deliveries {
        simulate_shares_and_wait(vardiff, shares, duration);
        let outcome = vardiff
            .try_vardiff(hashrate, &target, TEST_SHARES_PER_MINUTE)
            .expect("try_vardiff failed");
        if let Some(new_hashrate) = outcome {
            assert!(
                new_hashrate < hashrate,
                "an under-delivering channel must not be revised upward: {} -> {}",
                hashrate,
                new_hashrate
            );
            assert!(
                new_hashrate > 0.0,
                "belief must stay positive, got {}",
                new_hashrate
            );
            hashrate = new_hashrate;
            target = hash_rate_to_target(hashrate.into(), TEST_SHARES_PER_MINUTE.into())
                .unwrap()
                .into();
            fires += 1;
        }
    }

    assert!(
        fires > 0,
        "no evaluation acted on a channel delivering 40% of its target rate"
    );
    assert!(
        hashrate < TEST_INITIAL_HASHRATE,
        "belief should have fallen from {} but is {}",
        TEST_INITIAL_HASHRATE,
        hashrate
    );
}
