use crate::utils::{hash_rate_from_target, hash_rate_to_target};
use mining_sv2::Target;

use super::Vardiff;

#[derive(Debug)]
pub struct VardiffState {
    pub estimated_channel_hashrate: f32,
    pub shares_per_minute: f32,
    pub shares_since_last_update: u32,
    pub timestamp_of_last_update: u64,
    pub current_miner_target: Target,
}

impl VardiffState {
    pub fn new(shares_per_minute: f32, estimated_channel_hashrate: f32) -> Self {
        let current_miner_target =
            hash_rate_to_target(estimated_channel_hashrate as f64, shares_per_minute as f64)
                .expect("Cannot convert hash rate to target")
                .into();
        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();
        VardiffState {
            estimated_channel_hashrate,
            shares_per_minute,
            shares_since_last_update: 0,
            timestamp_of_last_update: timestamp_secs,
            current_miner_target,
        }
    }

    pub fn set_shares_per_minute(&mut self, shares_per_minute: f32) {
        self.shares_per_minute = shares_per_minute;
    }

    pub fn set_timestamp_of_last_update(&mut self, timestamp_of_last_update: u64) {
        self.timestamp_of_last_update = timestamp_of_last_update;
    }

    pub fn set_shares_since_last_update(&mut self, shares_since_last_update: u32) {
        self.shares_since_last_update = shares_since_last_update;
    }

    pub fn set_current_miner_target(&mut self, current_miner_target: Target) {
        self.current_miner_target = current_miner_target;
    }
}

impl Vardiff for VardiffState {
    fn hashrate(&self) -> f32 {
        self.estimated_channel_hashrate
    }

    fn shares_per_minute(&self) -> f32 {
        self.shares_per_minute
    }

    fn last_update_timestamp(&self) -> u64 {
        self.timestamp_of_last_update
    }

    fn shares_since_last_update(&self) -> u32 {
        self.shares_since_last_update
    }

    fn target(&self) -> Target {
        self.current_miner_target.clone()
    }

    fn set_hashrate(&mut self, estimated_channel_hashrate: f32) {
        self.estimated_channel_hashrate = estimated_channel_hashrate;
        let current_miner_target = hash_rate_to_target(
            estimated_channel_hashrate as f64,
            self.shares_per_minute as f64,
        )
        .expect("Cannot convert hash rate to target")
        .into();
        self.set_current_miner_target(current_miner_target);
    }

    fn increment_shares_since_last_update(&mut self) {
        self.shares_since_last_update += 1;
    }

    fn reset_counter(&mut self) {
        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();
        self.set_timestamp_of_last_update(timestamp_secs);
        self.set_shares_since_last_update(0);
    }

    fn update_hashrate(&mut self) {
        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();

        let delta_time = timestamp_secs - self.timestamp_of_last_update;
        #[cfg(test)]
        if delta_time == 0 {
            return;
        }
        #[cfg(not(test))]
        if delta_time <= 15 {
            return;
        }
        tracing::debug!("DELTA TIME: {:?}", delta_time);
        let realized_share_per_min =
            self.shares_since_last_update as f64 / (delta_time as f64 / 60.0);
        tracing::debug!("REALIZED SHARES PER MINUTE: {:?}", realized_share_per_min);
        tracing::debug!("CURRENT MINER TARGET: {:?}", self.current_miner_target);
        let mut new_miner_hashrate = match hash_rate_from_target(
            self.current_miner_target.clone().into(),
            realized_share_per_min,
        ) {
            Ok(hashrate) => hashrate as f32,
            Err(e) => {
                tracing::debug!("{:?} -> Probably min_individual_miner_hashrate parameter was not set properly in config file. New hashrate will be automatically adjusted to match the real one.", e);
                self.estimated_channel_hashrate * realized_share_per_min as f32
                    / self.shares_per_minute
            }
        };

        let hashrate_delta = new_miner_hashrate - self.estimated_channel_hashrate;
        let hashrate_delta_percentage =
            (hashrate_delta.abs() / self.estimated_channel_hashrate) * 100.0;
        tracing::debug!("\nMINER HASHRATE: {:?}", new_miner_hashrate);

        if (hashrate_delta_percentage >= 100.0)
            || (hashrate_delta_percentage >= 60.0) && (delta_time >= 60)
            || (hashrate_delta_percentage >= 50.0) && (delta_time >= 120)
            || (hashrate_delta_percentage >= 45.0) && (delta_time >= 180)
            || (hashrate_delta_percentage >= 30.0) && (delta_time >= 240)
            || (hashrate_delta_percentage >= 15.0) && (delta_time >= 300)
        {
            // realized_share_per_min is 0.0 when d.difficulty_mgmt.shares_since_last_update is 0
            // so it's safe to compare realized_share_per_min with == 0.0
            if realized_share_per_min == 0.0 {
                new_miner_hashrate = match delta_time {
                    dt if dt <= 30 => self.estimated_channel_hashrate / 1.5,
                    dt if dt < 60 => self.estimated_channel_hashrate / 2.0,
                    _ => self.estimated_channel_hashrate / 3.0,
                };
            }
            if (realized_share_per_min > 0.0) && (hashrate_delta_percentage > 1000.0) {
                new_miner_hashrate = match delta_time {
                    dt if dt <= 30 => self.estimated_channel_hashrate * 10.0,
                    dt if dt < 60 => self.estimated_channel_hashrate * 5.0,
                    _ => self.estimated_channel_hashrate * 3.0,
                };
            }
            self.set_hashrate(new_miner_hashrate);
            self.reset_counter();
        }
    }
}
