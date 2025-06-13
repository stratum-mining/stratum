//! ## Downstream SV1 Difficulty Management Module
//!
//! This module contains the logic and helper functions
//! for managing difficulty and hashrate adjustments for downstream mining clients
//! communicating via the SV1 protocol.
//!
//! It handles tasks such as:
//! - Converting SV2 targets received from upstream into SV1 difficulty values.
//! - Calculating and updating individual miner hashrates based on submitted shares.
//! - Preparing SV1 `mining.set_difficulty` messages.
//! - Potentially managing difficulty thresholds and adjustment logic for downstream miners.

use super::{Downstream, DownstreamMessages, SetDownstreamTarget};

use super::super::error::{Error, ProxyResult};
use primitive_types::U256;
use roles_logic_sv2::{mining_sv2::Target, utils::Mutex};
use std::{ops::Div, sync::Arc};
use tracing::debug;
use v1::json_rpc;

impl Downstream {
    /// Initializes the difficulty management parameters for a downstream connection.
    ///
    /// This function sets the initial timestamp for the last difficulty update and
    /// resets the count of submitted shares. It also adds the miner's configured
    /// minimum hashrate to the aggregated channel nominal hashrate stored in the
    /// upstream difficulty configuration.Finally, it sends a `SetDownstreamTarget` message upstream
    /// to the Bridge to inform it of the initial target for this new connection, derived from
    /// the provided `init_target`.This should typically be called once when a downstream connection
    /// is established.
    pub async fn init_difficulty_management(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let (connection_id, upstream_difficulty_config, miner_hashrate, init_target) = self_
            .safe_lock(|d| {
                _ = d.difficulty_mgmt.reset_counter();
                (
                    d.connection_id,
                    d.upstream_difficulty_config.clone(),
                    d.hashrate,
                    d.target.clone(),
                )
            })?;
        // add new connection hashrate to channel hashrate
        upstream_difficulty_config.safe_lock(|u| {
            u.channel_nominal_hashrate += miner_hashrate;
        })?;
        // update downstream target with bridge
        let init_target = binary_sv2::U256::from(init_target);
        Self::send_message_upstream(
            self_,
            DownstreamMessages::SetDownstreamTarget(SetDownstreamTarget {
                channel_id: connection_id,
                new_target: init_target.into(),
            }),
        )
        .await?;

        Ok(())
    }

    /// Removes the disconnecting miner's hashrate from the aggregated channel nominal hashrate.
    ///
    /// This function is called when a downstream miner disconnects to ensure that their
    /// individual hashrate is subtracted from the total nominal hashrate reported for
    /// the channel to the upstream server.
    #[allow(clippy::result_large_err)]
    pub fn remove_miner_hashrate_from_channel(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        self_.safe_lock(|d| {
            d.upstream_difficulty_config
                .safe_lock(|u| {
                    let hashrate_to_subtract = d.hashrate;
                    if u.channel_nominal_hashrate >= hashrate_to_subtract {
                        u.channel_nominal_hashrate -= hashrate_to_subtract;
                    } else {
                        u.channel_nominal_hashrate = 0.0;
                    }
                })
                .map_err(|_e| Error::PoisonLock)
        })??;
        Ok(())
    }

    /// Attempts to update the difficulty settings for a downstream miner based on their
    /// performance.
    ///
    /// This function is triggered periodically or based on share submissions. It calculates
    /// the miner's estimated hashrate based on the number of shares submitted and the elapsed
    /// time since the last update. If the estimated hashrate has changed significantly according to
    /// predefined thresholds, a new target is calculated, a `mining.set_difficulty` message is
    /// sent to the miner, and a `SetDownstreamTarget` message is sent upstream to the Bridge to
    /// notify it of the target change for this channel. The difficulty management parameters
    /// (timestamp and share count) are then reset.
    pub async fn try_update_difficulty_settings(
        self_: Arc<Mutex<Self>>,
    ) -> ProxyResult<'static, ()> {
        let (timestamp_of_last_update, shares_since_last_update, channel_id) =
            self_.clone().safe_lock(|d| {
                (
                    d.difficulty_mgmt.last_update_timestamp(),
                    d.difficulty_mgmt.shares_since_last_update(),
                    d.connection_id,
                )
            })?;
        debug!("Time of last diff update: {:?}", timestamp_of_last_update);
        debug!("Number of shares submitted: {:?}", shares_since_last_update);

        if let Some((_, new_target)) = Self::update_miner_hashrate(self_.clone())? {
            debug!("New target from hashrate: {:?}", new_target);
            let message = Self::get_set_difficulty(new_target.clone())?;
            let target = binary_sv2::U256::from(new_target);
            Downstream::send_message_downstream(self_.clone(), message).await?;
            let update_target_msg = SetDownstreamTarget {
                channel_id,
                new_target: target.into(),
            };
            // notify bridge of target update
            Downstream::send_message_upstream(
                self_.clone(),
                DownstreamMessages::SetDownstreamTarget(update_target_msg),
            )
            .await?;
        }
        Ok(())
    }

    /// Increments the counter for shares submitted by this downstream miner.
    ///
    /// This function is called each time a valid share is received from the miner.
    /// The count is used in the difficulty adjustment logic to estimate the miner's
    /// performance over a period.
    #[allow(clippy::result_large_err)]
    pub(super) fn save_share(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        self_.safe_lock(|d| {
            d.difficulty_mgmt.increment_shares_since_last_update();
        })?;
        Ok(())
    }

    /// Converts an SV2 target received from upstream into an SV1 difficulty value
    /// and formats it as a `mining.set_difficulty` JSON-RPC message.
    #[allow(clippy::result_large_err)]
    pub(super) fn get_set_difficulty(target: Target) -> ProxyResult<'static, json_rpc::Message> {
        let value = Downstream::difficulty_from_target(target)?;
        debug!("Difficulty from target: {:?}", value);
        let set_target = v1::methods::server_to_client::SetDifficulty { value };
        let message: json_rpc::Message = set_target.into();
        Ok(message)
    }

    /// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
    /// difficulty for the Downstream role sent via the SV1 `mining.set_difficulty` message.
    #[allow(clippy::result_large_err)]
    pub(super) fn difficulty_from_target(target: Target) -> ProxyResult<'static, f64> {
        // reverse because target is LE and this function relies on BE
        let mut target = binary_sv2::U256::from(target).to_vec();

        target.reverse();

        let target = target.as_slice();
        debug!("Target: {:?}", target);

        // If received target is 0, return 0
        if Downstream::is_zero(target) {
            return Ok(0.0);
        }
        let target = U256::from_big_endian(target);
        let pdiff: [u8; 32] = [
            0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        ];
        let pdiff = U256::from_big_endian(pdiff.as_ref());

        if pdiff > target {
            let diff = pdiff.div(target);
            Ok(diff.low_u64() as f64)
        } else {
            let diff = target.div(pdiff);
            let diff = diff.low_u64() as f64;
            // TODO still results in a difficulty that is too low
            Ok(1.0 / diff)
        }
    }

    /// Updates the miner's estimated hashrate and adjusts the aggregated channel nominal hashrate.
    ///
    /// This function calculates the miner's realized shares per minute over the period
    /// since the last update and uses it, along with the current target, to estimate
    /// their hashrate. It then compares this new estimate to the previous one and
    /// updates the miner's stored hashrate and the channel's aggregated hashrate
    /// if the change is significant based on time-dependent thresholds.
    #[allow(clippy::result_large_err)]
    pub fn update_miner_hashrate(
        self_: Arc<Mutex<Self>>,
    ) -> ProxyResult<'static, Option<(f32, Target)>> {
        let update = self_.super_safe_lock(|d| {
            let previous_hashrate = d.hashrate;
            let previous_target = d.target.clone();
            let update = d
                .difficulty_mgmt
                .try_vardiff(previous_hashrate, &previous_target);
            if let Ok(Some((new_hashrate, ref new_target))) = update {
                // update channel hashrate and target
                d.hashrate = new_hashrate;
                d.target = new_target.clone();
                let hashrate_delta = new_hashrate - previous_hashrate;
                d.upstream_difficulty_config.super_safe_lock(|c| {
                    if c.channel_nominal_hashrate + hashrate_delta > 0.0 {
                        c.channel_nominal_hashrate += hashrate_delta;
                    } else {
                        c.channel_nominal_hashrate = 0.0;
                    }
                });
            }
            update
        })?;
        Ok(update)
    }

    /// Helper function to check if target is set to zero for some reason (typically happens when
    /// Downstream role first connects).
    /// https://stackoverflow.com/questions/65367552/checking-a-vecu8-to-see-if-its-all-zero
    fn is_zero(buf: &[u8]) -> bool {
        let (prefix, aligned, suffix) = unsafe { buf.align_to::<u128>() };

        prefix.iter().all(|&x| x == 0)
            && suffix.iter().all(|&x| x == 0)
            && aligned.iter().all(|&x| x == 0)
    }
}

#[cfg(test)]
mod test {

    use crate::config::{DownstreamDifficultyConfig, UpstreamDifficultyConfig};
    use async_channel::unbounded;
    use binary_sv2::U256;
    use rand::{thread_rng, Rng};
    use roles_logic_sv2::{mining_sv2::Target, utils::Mutex};
    use sha2::{Digest, Sha256};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    use crate::downstream_sv1::Downstream;

    #[ignore] // as described in issue #988
    #[test]
    fn test_diff_management() {
        let expected_shares_per_minute = 1000.0;
        let total_run_time = std::time::Duration::from_secs(60);
        let initial_nominal_hashrate = measure_hashrate(5);
        let target = match roles_logic_sv2::utils::hash_rate_to_target(
            initial_nominal_hashrate,
            expected_shares_per_minute,
        ) {
            Ok(target) => target,
            Err(_) => panic!(),
        };

        let mut share = generate_random_80_byte_array();
        let timer = std::time::Instant::now();
        let mut elapsed = std::time::Duration::from_secs(0);
        let mut count = 0;
        while elapsed <= total_run_time {
            // start hashing util a target is met and submit to
            mock_mine(target.clone().into(), &mut share);
            elapsed = timer.elapsed();
            count += 1;
        }

        let calculated_share_per_min = count as f32 / (elapsed.as_secs_f32() / 60.0);
        // This is the error margin for a confidence of 99.99...% given the expect number of shares
        // per minute TODO the review the math under it
        let error_margin = get_error(expected_shares_per_minute);
        let error = (calculated_share_per_min - expected_shares_per_minute as f32).abs();
        assert!(
            error <= error_margin as f32,
            "Calculated shares per minute are outside the 99.99...% confidence interval. Error: {:?}, Error margin: {:?}, {:?}", error, error_margin,calculated_share_per_min
        );
    }

    fn get_error(lambda: f64) -> f64 {
        let z_score_99 = 6.0;
        z_score_99 * lambda.sqrt()
    }

    fn mock_mine(target: Target, share: &mut [u8; 80]) {
        let mut hashed: Target = [255_u8; 32].into();
        while hashed > target {
            hashed = hash(share);
        }
    }

    // returns hashrate based on how fast the device hashes over the given duration
    fn measure_hashrate(duration_secs: u64) -> f64 {
        let mut share = generate_random_80_byte_array();
        let start_time = Instant::now();
        let mut hashes: u64 = 0;
        let duration = Duration::from_secs(duration_secs);

        while start_time.elapsed() < duration {
            for _ in 0..10000 {
                hash(&mut share);
                hashes += 1;
            }
        }

        let elapsed_secs = start_time.elapsed().as_secs_f64();

        hashes as f64 / elapsed_secs
    }

    fn hash(share: &mut [u8; 80]) -> Target {
        let nonce: [u8; 8] = share[0..8].try_into().unwrap();
        let mut nonce = u64::from_le_bytes(nonce);
        nonce += 1;
        share[0..8].copy_from_slice(&nonce.to_le_bytes());
        let hash = Sha256::digest(&share).to_vec();
        let hash: U256<'static> = hash.try_into().unwrap();
        hash.into()
    }

    fn generate_random_80_byte_array() -> [u8; 80] {
        let mut rng = thread_rng();
        let mut arr = [0u8; 80];
        rng.fill(&mut arr[..]);
        arr
    }

    #[tokio::test]
    async fn test_converge_to_spm_from_low() {
        test_converge_to_spm(1.0).await
    }
    //TODO
    //#[tokio::test]
    //async fn test_converge_to_spm_from_high() {
    //    test_converge_to_spm(1_000_000_000_000).await
    //}

    async fn test_converge_to_spm(start_hashrate: f64) {
        let downstream_conf = DownstreamDifficultyConfig {
            min_individual_miner_hashrate: start_hashrate as f32, // updated below
            shares_per_minute: 1000.0,                            // 1000 shares per minute
            submits_since_last_update: 0,
            timestamp_of_last_update: 0, // updated below
        };
        let upstream_config = UpstreamDifficultyConfig {
            channel_diff_update_interval: 60,
            channel_nominal_hashrate: 0.0,
            timestamp_of_last_update: 0,
            should_aggregate: false,
        };
        let (tx_sv1_submit, _rx_sv1_submit) = unbounded();
        let (tx_outgoing, _rx_outgoing) = unbounded();
        let downstream = Downstream::new(
            1,
            vec![],
            vec![],
            None,
            None,
            tx_sv1_submit,
            tx_outgoing,
            false,
            0,
            downstream_conf.clone(),
            Arc::new(Mutex::new(upstream_config)),
        );

        let total_run_time = std::time::Duration::from_secs(75);
        let config_shares_per_minute = downstream_conf.shares_per_minute;
        let timer = std::time::Instant::now();
        let mut elapsed = std::time::Duration::from_secs(0);

        let expected_nominal_hashrate = measure_hashrate(5);
        let expected_target = match roles_logic_sv2::utils::hash_rate_to_target(
            expected_nominal_hashrate,
            config_shares_per_minute.into(),
        ) {
            Ok(target) => target,
            Err(_) => panic!(),
        };

        let mut initial_target = downstream.target.clone();
        let downstream = Arc::new(Mutex::new(downstream));
        Downstream::init_difficulty_management(downstream.clone())
            .await
            .unwrap();
        let mut share = generate_random_80_byte_array();
        while elapsed <= total_run_time {
            mock_mine(initial_target.clone().into(), &mut share);
            Downstream::save_share(downstream.clone()).unwrap();
            Downstream::try_update_difficulty_settings(downstream.clone())
                .await
                .unwrap();
            initial_target = downstream.safe_lock(|d| d.target.clone()).unwrap();
            elapsed = timer.elapsed();
        }
        let expected_0s = trailing_0s(expected_target.inner_as_ref().to_vec());
        let actual_0s = trailing_0s(binary_sv2::U256::from(initial_target.clone()).to_vec());
        assert!(expected_0s.abs_diff(actual_0s) <= 1);
    }

    fn trailing_0s(mut v: Vec<u8>) -> usize {
        let mut ret = 0;
        while v.pop() == Some(0) {
            ret += 1;
        }
        ret
    }
}
