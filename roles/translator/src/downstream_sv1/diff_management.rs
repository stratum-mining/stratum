use super::Downstream;

use crate::{error::Error, ProxyResult};
use roles_logic_sv2::{bitcoin::util::uint::Uint256, utils::Mutex};
use std::{ops::Div, sync::Arc};
use v1::json_rpc;

impl Downstream {
    /// initializes the timestamp and resets the number of submits for a connection.
    /// Should only be called once for the lifetime of a connection since `try_update_difficulty_settings()`
    /// also does this during this update
    pub fn init_difficulty_management(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        self_
            .safe_lock(|d| {
                let timestamp_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("time went backwards")
                    .as_secs();
                d.difficulty_mgmt.timestamp_of_last_update = timestamp_secs;
                d.difficulty_mgmt.submits_since_last_update = 0;
            })
            .map_err(|_e| Error::PoisonLock)?;
        Ok(())
    }

    /// if enough shares have been submitted according to the config, this function updates the difficulty for the connection and sends the new
    /// difficulty to the miner
    pub async fn try_update_difficulty_settings(
        self_: Arc<Mutex<Self>>,
    ) -> ProxyResult<'static, ()> {
        let diff_mgmt = self_
            .clone()
            .safe_lock(|d| d.difficulty_mgmt.clone())
            .map_err(|_e| Error::PoisonLock)?;
        tracing::debug!(
            "\nTIME OF LAST DIFFICULTY UPDATE: {:?}",
            diff_mgmt.timestamp_of_last_update
        );
        tracing::debug!(
            "NUMBER SHARES SUBMITTED: {:?}\n",
            diff_mgmt.submits_since_last_update
        );
        if diff_mgmt.submits_since_last_update >= diff_mgmt.miner_num_submits_before_update {
            let prev_target = roles_logic_sv2::utils::hash_rate_to_target(
                diff_mgmt.min_individual_miner_hashrate,
                diff_mgmt.shares_per_minute,
            )
            .to_vec();
            tracing::debug!("TARGET FROM HASH RATE: {:?}", &prev_target);
            if let Some(new_hash_rate) =
                Self::update_miner_hashrate(self_.clone(), prev_target.clone())?
            {
                #[cfg(not(test))]
                let new_target = roles_logic_sv2::utils::hash_rate_to_target(
                    new_hash_rate,
                    diff_mgmt.shares_per_minute,
                )
                .to_vec();
                #[cfg(not(test))]
                let message = Self::get_set_difficulty(new_target)?;
                #[cfg(not(test))]
                Downstream::send_message_downstream(self_.clone(), message).await?;
            }
        }
        Ok(())
    }

    /// calculates the target according to the current stored hashrate of the miner
    pub fn hash_rate_to_target(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, Vec<u8>> {
        let target = self_
            .safe_lock(|d| {
                roles_logic_sv2::utils::hash_rate_to_target(
                    d.difficulty_mgmt.min_individual_miner_hashrate,
                    d.difficulty_mgmt.shares_per_minute,
                )
                .to_vec()
            })
            .map_err(|_e| Error::PoisonLock)?;
        Ok(target)
    }

    /// increments the number of shares since the last difficulty update
    pub(super) fn save_share(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        tracing::info!("SAVED SHARE");
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
    pub(super) fn get_set_difficulty(target: Vec<u8>) -> ProxyResult<'static, json_rpc::Message> {
        let value = Downstream::difficulty_from_target(target)?;
        let set_target = v1::methods::server_to_client::SetDifficulty { value };
        let message: json_rpc::Message = set_target.into();
        Ok(message)
    }

    /// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
    /// difficulty for the Downstream role sent via the SV1 `mining.set_difficulty` message.
    pub(super) fn difficulty_from_target(target: Vec<u8>) -> ProxyResult<'static, f64> {
        let target = target.as_slice();

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
                tracing::debug!("\nDELTA TIME: {:?}", delta_time);
                let realized_share_per_min =
                    d.difficulty_mgmt.submits_since_last_update as f32 / (delta_time as f32 / 60.0);
                let new_miner_hashrate = roles_logic_sv2::utils::hash_rate_from_target(
                    miner_target.clone().try_into()?,
                    realized_share_per_min,
                );
                let hashrate_delta =
                    new_miner_hashrate as f32 - d.difficulty_mgmt.min_individual_miner_hashrate;
                tracing::debug!("\nMINER HASHRATE: {:?}", new_miner_hashrate);
                d.difficulty_mgmt.min_individual_miner_hashrate = new_miner_hashrate as f32;
                d.difficulty_mgmt.timestamp_of_last_update = timestamp_secs;
                d.difficulty_mgmt.submits_since_last_update = 0;
                // update channel hashrate (read by upstream)
                d.upstream_difficulty_config
                    .safe_lock(|c| {
                        c.actual_nominal_hashrate += hashrate_delta;
                        Some(new_miner_hashrate)
                    })
                    .map_err(|_e| Error::PoisonLock)
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

    // test ability to approach target share per minute. Currently set to test against 20% error
    #[tokio::test]
    async fn test_diff_management() {
        let downstream_conf = DownstreamDifficultyConfig {
            min_individual_miner_hashrate: 0.0,  // updated below
            miner_num_submits_before_update: 10, // update after 5 submits
            shares_per_minute: 10.0,             // 10 shares per minute
            submits_since_last_update: 0,
            timestamp_of_last_update: 0, // updated below
        };
        let upstream_config = UpstreamDifficultyConfig {
            channel_diff_update_interval: 60,
            channel_nominal_hashrate: 0.0,
            actual_nominal_hashrate: 0.0,
            timestamp_of_last_update: 0,
            should_aggregate: false,
        };
        let (tx_sv1_submit, _rx_sv1_submit) = unbounded();
        let (tx_outgoing, _rx_outgoing) = unbounded();
        // create Downstream instance
        let mut downstream = Downstream::new(
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

        let total_run_time = std::time::Duration::from_secs(300);
        let config_shares_per_minute = downstream_conf.shares_per_minute;
        // get initial hashrate
        let initial_nominal_hashrate = measure_hashrate(10);
        // get target from hashrate and shares_per_sec
        let initial_target = roles_logic_sv2::utils::hash_rate_to_target(
            initial_nominal_hashrate as f32,
            config_shares_per_minute,
        );
        println!("INITIAL TARGET: {:?}", initial_target);

        downstream.difficulty_mgmt.min_individual_miner_hashrate = initial_nominal_hashrate as f32;

        // run for run time
        let timer = std::time::Instant::now();
        let mut elapsed = std::time::Duration::from_secs(0);
        let downstream = Arc::new(Mutex::new(downstream));
        Downstream::init_difficulty_management(downstream.clone()).unwrap();
        let mut target = initial_target;
        let mut count = 0;
        while elapsed <= total_run_time {
            // start hashing util a target is met and submit to
            mock_mine(target.clone().into());
            Downstream::save_share(downstream.clone()).unwrap();
            Downstream::try_update_difficulty_settings(downstream.clone())
                .await
                .unwrap();
            target = downstream
                .safe_lock(|d| {
                    roles_logic_sv2::utils::hash_rate_to_target(
                        d.difficulty_mgmt.min_individual_miner_hashrate,
                        config_shares_per_minute,
                    )
                })
                .unwrap();
            elapsed = timer.elapsed();
            count += 1;
            println!("Submitted {:?} share in {:?} seconds", count, elapsed);
            let calculated_share_per_min = count as f32 / (elapsed.as_secs_f32() / 60.0);
            println!("Actual Share/Min {:?}", calculated_share_per_min);
        }

        let calculated_share_per_min = count as f32 / (elapsed.as_secs_f32() / 60.0);

        println!(
            "CALCULATED HASHRATE: {:?}",
            downstream
                .clone()
                .safe_lock(|d| d.difficulty_mgmt.min_individual_miner_hashrate)
                .unwrap()
        );
        println!("Actual Share/Min {:?}", calculated_share_per_min);
        let calculated_share_per_min = count as f32 / (elapsed.as_secs_f32() / 60.0);
        let err = 1.0
            - (config_shares_per_minute - calculated_share_per_min / config_shares_per_minute)
                .abs();
        assert!(
            err < 0.2,
            "Calculated share_per_min does not meet 20% error"
        );
    }

    fn mock_mine(target: Target) -> U256<'static> {
        let mut share: Target = [255_u8; 32].into();
        while shares_is_gt(share.clone().into(), target.clone().into()) {
            let mut rng = thread_rng();
            let number = rng.gen::<u64>();
            let digest: U256 = Sha256::digest(&number.to_le_bytes())
                .to_vec()
                .try_into()
                .unwrap();
            share = u256_to_target(digest);
        }
        let test_share: U256 = share.clone().into();
        let test_target: U256 = target.into();
        println!("\nSHARE: {:?}", test_share);
        println!("TARGET: {:?}\n", test_target);
        share.into()
    }

    // returns hashrate based on how fast the device hashes over the given duration
    fn measure_hashrate(duration_secs: u64) -> f64 {
        let start_time = Instant::now();
        let mut hashes: u64 = 0;
        let duration = Duration::from_secs(duration_secs);

        while start_time.elapsed() < duration {
            let mut rng = thread_rng();
            let number = rng.gen::<u64>();
            let _hash: U256 = Sha256::digest(&number.to_le_bytes())
                .to_vec()
                .try_into()
                .unwrap();
            // ops to make hashrate closer to mock mine
            let _share: Target = u256_to_target(_hash);
            hashes += 1;
        }

        let elapsed_secs = start_time.elapsed().as_secs_f64();
        let hashrate = hashes as f64 / elapsed_secs;
        let nominal_hash_rate = hashrate;
        println!("Hashrate: {:.2} H/s", nominal_hash_rate);
        nominal_hash_rate
    }

    fn u256_to_target(u: U256) -> Target {
        let v = u.to_vec();
        // below unwraps never panics
        let head = u128::from_be_bytes(v[0..16].try_into().unwrap());
        let tail = u128::from_be_bytes(v[16..32].try_into().unwrap());
        Target::new(head, tail)
    }

    fn shares_is_gt(share: U256<'static>, target: U256<'static>) -> bool {
        let a = share.inner_as_ref();
        let b = target.inner_as_ref();
        u128::from_be_bytes(a[0..16].try_into().unwrap())
            > u128::from_be_bytes(b[0..16].try_into().unwrap())
    }
}
