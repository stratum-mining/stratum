/// Contains a generic test implementation that is agnostic to the Vardiff implementation,
/// providing methods to verify the correctness of any specific implementation.
use std::{thread, time::Duration};

use crate::VardiffState;
mod classic;

use super::Vardiff;

pub const TEST_INITIAL_HASHRATE: f32 = 1000.0;
pub const TEST_SHARES_PER_MINUTE: f32 = 10.0;
pub const TEST_MIN_ALLOWED_HASHRATE: f32 = 10.0;

pub fn simulate_shares_and_wait<V: Vardiff>(
    vardiff: &mut V,
    num_shares: u32,
    wait_duration_secs: u64,
) {
    for _ in 0..num_shares {
        vardiff.increment_shares_since_last_update();
    }
    if wait_duration_secs > 0 {
        thread::sleep(Duration::from_secs(wait_duration_secs));
    }
}

pub fn test_set_hashrate_updates_target<V: Vardiff>(vardiff: &mut V) {
    let original_target = vardiff.target();
    let new_hashrate = TEST_INITIAL_HASHRATE * 2.0;

    vardiff
        .set_hashrate(new_hashrate)
        .expect("Failed to set hashrate");

    assert_eq!(vardiff.hashrate(), new_hashrate);
    assert_ne!(vardiff.target(), original_target, "Target should change");

    assert!(vardiff.target() < original_target);

    let very_low_hashrate = TEST_MIN_ALLOWED_HASHRATE / 2.0;
    vardiff
        .set_hashrate(very_low_hashrate.clone())
        .expect("Failed to set hashrate");
    assert_eq!(vardiff.hashrate(), very_low_hashrate);

    assert!(vardiff.target() > original_target);
}

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

pub fn test_try_vardiff_no_change_short_interval<V: Vardiff>(vardiff: &mut V) {
    simulate_shares_and_wait(vardiff, 5, 10);
    let result = vardiff.try_vardiff().expect("try_vardiff failed");

    assert!(
        result.is_none(),
        "Should not update within 15s; elapsed 10s"
    );
}

pub fn test_try_vardiff_stable_hashrate_minimal_change_or_no_change<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = vardiff.hashrate();

    let simulation_duration_secs = 70;
    let expected_shares_for_duration =
        (TEST_SHARES_PER_MINUTE * simulation_duration_secs as f32 / 60.0).round() as u32;

    simulate_shares_and_wait(
        vardiff,
        expected_shares_for_duration,
        simulation_duration_secs,
    );

    let result = vardiff.try_vardiff().expect("try_vardiff failed");

    if let Some(new_hashrate) = result {
        let diff_percentage = ((new_hashrate - initial_hashrate).abs() / initial_hashrate) * 100.0;
        println!(
            "Stable hashrate test: new hashrate {}, initial {}, diff_pct {}",
            new_hashrate, initial_hashrate, diff_percentage
        );
        assert!(
            diff_percentage < 20.0,
            "Change should be minimal for stable rate if any"
        );
        assert_eq!(vardiff.shares_since_last_update(), 0)
    } else {
        assert!(
            true,
            "No update for stable hashrate is acceptable if deviation is low"
        );
    }
}

pub fn test_try_vardiff_low_hashrate_increase_target<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = vardiff.hashrate();
    let initial_target = vardiff.target();

    let simulation_duration = 65;
    simulate_shares_and_wait(vardiff, 2, simulation_duration);

    let result = vardiff.try_vardiff().expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "Hashrate should update due to low share count"
    );
    let new_hashrate = result.unwrap();

    assert!(
        new_hashrate < initial_hashrate,
        "Estimated hashrate should decrease"
    );
    assert!(
        vardiff.target() > initial_target,
        "Target should become easier (larger value)"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

pub fn test_try_vardiff_high_hashrate_decrease_target<V: Vardiff>(vardiff: &mut V) {
    let initial_hashrate = vardiff.hashrate();
    let initial_target = vardiff.target();

    let simulation_duration_secs = 65;
    simulate_shares_and_wait(vardiff, 30, simulation_duration_secs);

    let result = vardiff.try_vardiff().expect("try_vardiff failed");
    assert!(
        result.is_some(),
        "Hashrate should update due to high share count"
    );
    let new_hashrate = result.unwrap();

    assert!(
        new_hashrate > initial_hashrate,
        "Estimated hashratem should increase"
    );
    assert!(
        vardiff.target() < initial_target,
        "Target should become harder (smaller value)"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}

#[test]
fn test_try_vardiff_hashrate_clamps_to_minimum() {
    let mut vardiff = VardiffState::new_with_min(
        TEST_SHARES_PER_MINUTE,
        TEST_MIN_ALLOWED_HASHRATE * 1.5,
        TEST_MIN_ALLOWED_HASHRATE,
    )
    .expect("Failed to create VardiffState");

    let simulation_duration_secs = 70;
    simulate_shares_and_wait(&mut vardiff, 0, simulation_duration_secs);

    let result = vardiff.try_vardiff().expect("try_vardiff failed");
    assert!(result.is_some(), "Hashrate should update");
    let new_hashrate = result.unwrap();

    assert_eq!(
        new_hashrate, TEST_MIN_ALLOWED_HASHRATE,
        "Hashrate should be clamped to minimum"
    );
    assert_eq!(
        vardiff.hashrate(),
        TEST_MIN_ALLOWED_HASHRATE,
        "Stored hashrate should be clamped"
    );
    assert_eq!(vardiff.shares_since_last_update(), 0);
}
