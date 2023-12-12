use super::{Downstream, DownstreamMessages, SetDownstreamTarget};

use crate::{error::Error, ProxyResult};
use roles_logic_sv2::utils::Mutex;
use std::{ops::Div, sync::Arc};
use tracing::error;
use v1::json_rpc;

use stratum_common::bitcoin::util::uint::Uint256;

impl Downstream {
    /// initializes the timestamp and resets the number of submits for a connection.
    /// Should only be called once for the lifetime of a connection since `try_update_difficulty_settings()`
    /// also does this during this update
    pub async fn init_difficulty_management(
        self_: Arc<Mutex<Self>>,
        init_target: &[u8],
    ) -> ProxyResult<'static, ()> {
        let (connection_id, upstream_difficulty_config, miner_hashrate) = self_
            .safe_lock(|d| {
                let timestamp_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("time went backwards")
                    .as_secs();
                d.difficulty_mgmt.timestamp_of_last_update = timestamp_secs;
                d.difficulty_mgmt.submits_since_last_update = 0;
                (
                    d.connection_id,
                    d.upstream_difficulty_config.clone(),
                    d.difficulty_mgmt.min_individual_miner_hashrate,
                )
            })
            .map_err(|_e| Error::PoisonLock)?;
        // add new connection hashrate to channel hashrate
        upstream_difficulty_config
            .safe_lock(|u| {
                u.channel_nominal_hashrate += miner_hashrate;
            })
            .map_err(|_e| Error::PoisonLock)?;
        // update downstream target with bridge
        let init_target = binary_sv2::U256::try_from(init_target.to_vec())?;
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

    /// Called before a miner disconnects so we can remove the miner's hashrate from the aggregated channel hashrate
    #[allow(clippy::result_large_err)]
    pub fn remove_miner_hashrate_from_channel(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        self_
            .safe_lock(|d| {
                d.upstream_difficulty_config
                    .safe_lock(|u| {
                        u.channel_nominal_hashrate -=
                            d.difficulty_mgmt.min_individual_miner_hashrate
                    })
                    .map_err(|_e| Error::PoisonLock)
            })
            .map_err(|_e| Error::PoisonLock)??;
        Ok(())
    }

    /// if enough shares have been submitted according to the config, this function updates the difficulty for the connection and sends the new
    /// difficulty to the miner
    pub async fn try_update_difficulty_settings(
        self_: Arc<Mutex<Self>>,
    ) -> ProxyResult<'static, ()> {
        let (diff_mgmt, channel_id) = self_
            .clone()
            .safe_lock(|d| (d.difficulty_mgmt.clone(), d.connection_id))
            .map_err(|_e| Error::PoisonLock)?;
        tracing::debug!(
            "Time of last diff update: {:?}",
            diff_mgmt.timestamp_of_last_update
        );
        tracing::debug!(
            "Number of shares submitted: {:?}",
            diff_mgmt.submits_since_last_update
        );
        let prev_target = match roles_logic_sv2::utils::hash_rate_to_target(
            diff_mgmt.min_individual_miner_hashrate.into(),
            diff_mgmt.shares_per_minute.into(),
        ) {
            Ok(target) => target.to_vec(),
            Err(v) => return Err(Error::TargetError(v)),
        };
        if let Some(new_hash_rate) =
            Self::update_miner_hashrate(self_.clone(), prev_target.clone())?
        {
            let new_target = match roles_logic_sv2::utils::hash_rate_to_target(
                new_hash_rate.into(),
                diff_mgmt.shares_per_minute.into(),
            ) {
                Ok(target) => target,
                Err(v) => return Err(Error::TargetError(v)),
            };
            tracing::debug!("New target from hashrate: {:?}", new_target.inner_as_ref());
            let message = Self::get_set_difficulty(new_target.to_vec())?;
            // send mining.set_difficulty to miner
            Downstream::send_message_downstream(self_.clone(), message).await?;
            let update_target_msg = SetDownstreamTarget {
                channel_id,
                new_target: new_target.into(),
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

    /// calculates the target according to the current stored hashrate of the miner
    #[allow(clippy::result_large_err)]
    pub fn hash_rate_to_target(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, Vec<u8>> {
        self_
            .safe_lock(|d| {
                match roles_logic_sv2::utils::hash_rate_to_target(
                    d.difficulty_mgmt.min_individual_miner_hashrate.into(),
                    d.difficulty_mgmt.shares_per_minute.into(),
                ) {
                    Ok(target) => Ok(target.to_vec()),
                    Err(e) => Err(Error::TargetError(e)),
                }
            })
            .map_err(|_e| Error::PoisonLock)?
    }

    /// increments the number of shares since the last difficulty update
    #[allow(clippy::result_large_err)]
    pub(super) fn save_share(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        self_
            .safe_lock(|d| {
                d.difficulty_mgmt.submits_since_last_update += 1;
            })
            .map_err(|_e| Error::PoisonLock)?;
        Ok(())
    }

    /// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
    /// difficulty for the Downstream role and creates the SV1 `mining.set_difficulty` message to
    /// be sent to the Downstream role.
    #[allow(clippy::result_large_err)]
    pub(super) fn get_set_difficulty(target: Vec<u8>) -> ProxyResult<'static, json_rpc::Message> {
        let value = Downstream::difficulty_from_target(target)?;
        tracing::debug!("Difficulty from target: {:?}", value);
        let set_target = v1::methods::server_to_client::SetDifficulty { value };
        let message: json_rpc::Message = set_target.into();
        Ok(message)
    }

    /// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
    /// difficulty for the Downstream role sent via the SV1 `mining.set_difficulty` message.
    #[allow(clippy::result_large_err)]
    pub(super) fn difficulty_from_target(mut target: Vec<u8>) -> ProxyResult<'static, f64> {
        // reverse because target is LE and this function relies on BE
        target.reverse();
        let target = target.as_slice();
        tracing::debug!("Target: {:?}", target);

        // If received target is 0, return 0
        if Downstream::is_zero(target) {
            return Ok(0.0);
        }
        let target = Uint256::from_be_slice(target)?;
        let pdiff: [u8; 32] = [
            0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        ];
        let pdiff = Uint256::from_be_bytes(pdiff);

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

    /// This function updates the miner hashrate and resets difficulty management params. To calculate hashrate it calculates the realized shares per minute from the number of shares submitted
    /// and the delta time since last update. It then uses the realized shares per minute and the target those shares where mined on to calculate an estimated hashrate during that period with the
    /// function [`roles_logic_sv2::utils::hash_rate_from_target`]. Lastly, it adjusts the `channel_nominal_hashrate` according to the change in estimated miner hashrate
    #[allow(clippy::result_large_err)]
    pub fn update_miner_hashrate(
        self_: Arc<Mutex<Self>>,
        miner_target: Vec<u8>,
    ) -> ProxyResult<'static, Option<f32>> {
        self_
            .safe_lock(|d| {
                let timestamp_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("time went backwards")
                    .as_secs();

                // reset if timestamp is at 0
                if d.difficulty_mgmt.timestamp_of_last_update == 0 {
                    d.difficulty_mgmt.timestamp_of_last_update = timestamp_secs;
                    d.difficulty_mgmt.submits_since_last_update = 0;
                    return Ok(None);
                }

                let delta_time = timestamp_secs - d.difficulty_mgmt.timestamp_of_last_update;
                if delta_time == 0 {
                    return Ok(None);
                }
                tracing::debug!("\nDELTA TIME: {:?}", delta_time);
                let realized_share_per_min =
                    d.difficulty_mgmt.submits_since_last_update as f64 / (delta_time as f64 / 60.0);
                tracing::debug!("\nREALIZED SHARES PER MINUTE {:?}", realized_share_per_min);
                let mut new_miner_hashrate = match roles_logic_sv2::utils::hash_rate_from_target(
                    miner_target.clone().try_into()?,
                    realized_share_per_min,
                ) {
                    Ok(hashrate) => hashrate as f32,
                    Err(e) => {
                        error!("{:?} -> Probably min_individual_miner_hashrate parameter was not set properly in config file. New hashrate will be automatically adjusted to match the real one.", e);
                        d.difficulty_mgmt.min_individual_miner_hashrate * realized_share_per_min as f32 / d.difficulty_mgmt.shares_per_minute
                    }
                };

                let mut hashrate_delta =
                    new_miner_hashrate - d.difficulty_mgmt.min_individual_miner_hashrate;
                let hashrate_delta_percentage = (hashrate_delta.abs()
                    / d.difficulty_mgmt.min_individual_miner_hashrate)
                    * 100.0;
                tracing::debug!("\nMINER HASHRATE: {:?}", new_miner_hashrate);

                if (hashrate_delta_percentage >= 100.0)
                    || (hashrate_delta_percentage >= 60.0) && (delta_time >= 60)
                    || (hashrate_delta_percentage >= 50.0) && (delta_time >= 120)
                    || (hashrate_delta_percentage >= 45.0) && (delta_time >= 180)
                    || (hashrate_delta_percentage >= 30.0) && (delta_time >= 240)
                    || (hashrate_delta_percentage >= 15.0) && (delta_time >= 300)
                {
                if realized_share_per_min < 0.01 {
                    new_miner_hashrate = match delta_time {
                        dt if dt < 30 => d.difficulty_mgmt.min_individual_miner_hashrate / 2.0,
                        dt if dt < 60 => d.difficulty_mgmt.min_individual_miner_hashrate / 3.0,
                        _ => d.difficulty_mgmt.min_individual_miner_hashrate / 5.0,
                    };
                    hashrate_delta =
                        new_miner_hashrate - d.difficulty_mgmt.min_individual_miner_hashrate;
                }
                d.difficulty_mgmt.min_individual_miner_hashrate = new_miner_hashrate;
                d.difficulty_mgmt.timestamp_of_last_update = timestamp_secs;
                d.difficulty_mgmt.submits_since_last_update = 0;
                // update channel hashrate (read by upstream)
                d.upstream_difficulty_config.super_safe_lock(|c| {
                    c.channel_nominal_hashrate += hashrate_delta;
                });
                Ok(Some(new_miner_hashrate))
                } else {
                    Ok(None)
                }
            })
            .map_err(|_e| Error::PoisonLock)?
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
    use crate::proxy_config::{DownstreamDifficultyConfig, UpstreamDifficultyConfig};
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

    #[test]
    fn test_diff_management() {
        let expected_shares_per_minute = 1000.0;
        let total_run_time = std::time::Duration::from_secs(11);
        let initial_nominal_hashrate = measure_hashrate(5);
        let target = match roles_logic_sv2::utils::hash_rate_to_target(
            dbg!(initial_nominal_hashrate),
            expected_shares_per_minute.into(),
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
        // This is the error margin for a confidence of 99% given the expect number of shares per
        // minute TODO the review the math under it
        let error_margin = dbg!(get_error(expected_shares_per_minute.into()));
        let error = (calculated_share_per_min - expected_shares_per_minute as f32).abs();
        assert!(
            error <= error_margin as f32,
            "Calculated shares per minute are outside the 99% confidence interval. Error: {:?}, Error margin: {:?}, {:?}", error, error_margin,calculated_share_per_min
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
        let hashrate = hashes as f64 / elapsed_secs;
        let nominal_hash_rate = hashrate;
        nominal_hash_rate
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
            min_individual_miner_hashrate: 0.0, // updated below
            shares_per_minute: 1000.0,          // 1000 shares per minute
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
        let mut downstream = Downstream::new(
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
        downstream.difficulty_mgmt.min_individual_miner_hashrate = start_hashrate as f32;

        let total_run_time = std::time::Duration::from_secs(10);
        let config_shares_per_minute = downstream_conf.shares_per_minute;
        let timer = std::time::Instant::now();
        let mut elapsed = std::time::Duration::from_secs(0);

        let expected_nominal_hashrate = measure_hashrate(5);
        let expected_target = match roles_logic_sv2::utils::hash_rate_to_target(
            dbg!(expected_nominal_hashrate),
            config_shares_per_minute.into(),
        ) {
            Ok(target) => target,
            Err(_) => panic!(),
        };

        let initial_nominal_hashrate = start_hashrate;
        let mut initial_target = match roles_logic_sv2::utils::hash_rate_to_target(
            initial_nominal_hashrate,
            config_shares_per_minute.into(),
        ) {
            Ok(target) => target,
            Err(_) => panic!(),
        };
        let downstream = Arc::new(Mutex::new(downstream));
        Downstream::init_difficulty_management(downstream.clone(), initial_target.inner_as_ref())
            .await
            .unwrap();
        let mut share = generate_random_80_byte_array();
        while elapsed <= total_run_time {
            mock_mine(initial_target.clone().into(), &mut share);
            Downstream::save_share(downstream.clone()).unwrap();
            Downstream::try_update_difficulty_settings(downstream.clone())
                .await
                .unwrap();
            initial_target = downstream
                .safe_lock(|d| {
                    match roles_logic_sv2::utils::hash_rate_to_target(
                        d.difficulty_mgmt.min_individual_miner_hashrate.into(),
                        config_shares_per_minute.into(),
                    ) {
                        Ok(target) => target,
                        Err(_) => panic!(),
                    }
                })
                .unwrap();
            elapsed = timer.elapsed();
        }
        let expected_0s = trailing_0s(expected_target.inner_as_ref().to_vec());
        let actual_0s = trailing_0s(initial_target.inner_as_ref().to_vec());
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
