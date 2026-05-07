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

    let simulation_duration = 16;
    simulate_shares_and_wait(vardiff, 16, simulation_duration);

    let result = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "Hashrate should update due to low share count"
    );
    let new_hashrate = result.unwrap();

    // As estimated shares per minute is 10
    // with current setup realized shares per minute is 60
    // comes under no special case
    assert_eq!(new_hashrate, 6.0 * initial_hashrate);
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
    assert_eq!(new_hashrate, 10.0 * initial_hashrate);

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
    assert_eq!(new_hashrate, 5.0 * initial_hashrate);
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
    assert_eq!(new_hashrate, 3.0 * initial_hashrate);
    let target: Target = hash_rate_to_target(new_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
        .unwrap()
        .into();
    assert!(
        target < initial_target,
        "Target should become harder (larger value)"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

// Verifies that difficulty decreases when no shares are found within a 30 to 60-second window.
//
// Note: at `shares_per_minute = 10`, the parametric noise floor only drops below 100% once
// expected count climbs high enough — around `delta_time = 52s` (λ ≈ 8.7, threshold ≈ 99%).
// We use 55s to leave a comfortable margin while staying inside the dt < 60 branch (÷2.0)
// that this test asserts. Anywhere shorter at this share rate, zero shares is statistically
// indistinguishable from Poisson noise.
fn test_try_vardiff_no_shares_30_to_60s_decrease<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let initial_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();

    let simulation_duration = 55;
    simulate_shares_and_wait(vardiff, 0, simulation_duration);

    let result = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    let new_hashrate = result.expect("Hashrate should have updated");

    // This logic checks the `dt < 60` case, which divides by 2.0
    let expected_hashrate = initial_hashrate / 2.0;
    assert!(
        (new_hashrate - expected_hashrate).abs() < 0.01,
        "Hashrate should be initial / 2. Got: {}, Expected: {}",
        new_hashrate,
        expected_hashrate
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

// Verifies that difficulty decreases when no shares are found over a 60-second window.
fn test_try_vardiff_no_shares_more_than_60s_decrease<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let initial_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();

    let simulation_duration = 60;
    simulate_shares_and_wait(vardiff, 0, simulation_duration);

    let result = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    let new_hashrate = result.expect("Hashrate should have updated");

    // This logic checks the `dt >= 60` case, which divides by 3.0
    let expected_hashrate = initial_hashrate / 3.0;
    assert!(
        (new_hashrate - expected_hashrate).abs() < 0.01,
        "Hashrate should be initial / 3. Got: {}, Expected: {}",
        new_hashrate,
        expected_hashrate
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

// Verifies that the algorithm reduces hashrate when the miner is consistently underperforming,
// once enough samples have accumulated to push the deviation outside the noise floor.
//
// Pre-existing rung-by-rung calibration (which fired at 60%/50%/45%/30%/15% deviations at
// 60s/120s/180s/240s/300s) was retired with the parametric-threshold change: those small-magnitude
// deviations now sit *inside* the Poisson noise band at the configured share rate and should not
// trigger fires. This test instead verifies the surviving guarantee: a sustained 50% deviation
// over 300s produces enough samples (expected ~50, threshold ~42%) for the algorithm to fire and
// correct the hashrate.
fn test_try_vardiff_with_less_spm_than_expected<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let initial_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();
    assert_eq!(initial_hashrate, 1000.0);

    // 25 shares over 300s ⇒ realized = 5 shares/min vs expected 10 shares/min ⇒ 50% deviation.
    let simulation_duration = 300;
    simulate_shares_and_wait(vardiff, 25, simulation_duration);

    let new_hashrate = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed")
        .expect("50% deviation at 300s should fire (threshold ~42%)");

    // hash_rate_from_target produces ~initial × (realized / expected) = 1000 × 0.5 = 500.
    assert!(
        (new_hashrate - 500.0).abs() < 1.0,
        "Hashrate should be ~500 (half of initial); got {new_hashrate}"
    );
    assert!(
        new_hashrate < initial_hashrate,
        "Hashrate should decrease for under-performing miner"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

// Verifies that the algorithm does NOT fire on a deviation that's inside the Poisson
// noise band — this is the core property of the parametric-threshold change.
pub fn test_try_vardiff_no_fire_within_noise<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let initial_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();

    // 8 shares over 60s ⇒ realized = 8/min vs expected 10/min ⇒ 20% deviation.
    // At λ = 10, parametric threshold ≈ 91%. 20% is well within noise.
    let simulation_duration = 60;
    simulate_shares_and_wait(vardiff, 8, simulation_duration);

    let result = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(
        result.is_none(),
        "Algorithm should not fire on noise-level deviation; got {result:?}"
    );
    // Counter is preserved when no fire happens.
    assert_eq!(vardiff.shares_since_last_update(), 8);
}

// Verifies that the algorithm fires once a deviation is comfortably outside the noise floor
// — confirming the threshold drops below 100% as samples accumulate, which the prior
// unconditional ≥100% rung enforced unconditionally (and incorrectly, at small N).
pub fn test_try_vardiff_fires_outside_noise<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = TEST_INITIAL_HASHRATE;
    let initial_target =
        hash_rate_to_target(initial_hashrate.into(), TEST_SHARES_PER_MINUTE.into())
            .unwrap()
            .into();

    // 20 shares over 240s ⇒ realized = 5/min vs expected 10/min ⇒ 50% deviation.
    // At λ = 40, parametric threshold ≈ 47%. 50% is just outside noise.
    let simulation_duration = 240;
    simulate_shares_and_wait(vardiff, 20, simulation_duration);

    let result = vardiff
        .try_vardiff(initial_hashrate, &initial_target, TEST_SHARES_PER_MINUTE)
        .expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "Algorithm should fire when deviation crosses the noise floor"
    );
    let new_hashrate = result.unwrap();
    assert!(
        new_hashrate < initial_hashrate,
        "Hashrate should decrease for under-performing miner; got {new_hashrate}"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}
