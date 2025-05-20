use crate::utils::{hash_rate_from_target, hash_rate_to_target};
use std::convert::TryInto;

#[allow(warnings)]
#[derive(Debug)]
pub struct DownstreamVardiffState {
    pub estimated_downstream_hash_rate: f32,
    pub shares_per_minute: f32,
    pub shares_since_last_update: u32,
    pub timestamp_of_last_update: u64,
    pub current_miner_target: Vec<u8>,
}

impl DownstreamVardiffState {
    pub fn new(shares_per_minute: f32, estimated_downstream_hash_rate: f32) -> Self {
        let current_miner_target = hash_rate_to_target(
            estimated_downstream_hash_rate as f64,
            shares_per_minute as f64,
        )
        .expect("Cannot convert hash rate to target")
        .to_vec();
        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();
        DownstreamVardiffState {
            estimated_downstream_hash_rate,
            shares_per_minute,
            shares_since_last_update: 0,
            timestamp_of_last_update: timestamp_secs,
            current_miner_target,
        }
    }

    pub fn get_hash_rate(&self) -> f32 {
        self.estimated_downstream_hash_rate
    }

    pub fn get_shares_per_minute(&self) -> f32 {
        self.shares_per_minute
    }

    pub fn get_timestamp_of_last_update(&self) -> u64 {
        self.timestamp_of_last_update
    }

    pub fn get_shares_since_last_update(&self) -> u32 {
        self.shares_since_last_update
    }

    pub fn get_current_miner_target(&self) -> Vec<u8> {
        self.current_miner_target.clone()
    }

    pub fn set_hash_rate(&mut self, estimated_downstream_hash_rate: f32) {
        self.estimated_downstream_hash_rate = estimated_downstream_hash_rate;
        let current_miner_target = hash_rate_to_target(
            estimated_downstream_hash_rate as f64,
            self.shares_per_minute as f64,
        )
        .expect("")
        .to_vec();
        self.set_current_miner_target(current_miner_target);
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

    pub fn set_current_miner_target(&mut self, current_miner_target: Vec<u8>) {
        self.current_miner_target = current_miner_target;
    }

    pub fn update_shares_since_last_update(&mut self) {
        self.shares_since_last_update += 1;
    }

    pub fn reset(&mut self) {
        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();
        self.set_timestamp_of_last_update(timestamp_secs);
        self.set_shares_since_last_update(0);
    }

    pub fn update_downstream_hashrate(&mut self) -> Option<(f32, f32)> {
        let timestamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();

        let delta_time = timestamp_secs - self.timestamp_of_last_update;
        #[cfg(test)]
        if delta_time == 0 {
            return None;
        }
        #[cfg(not(test))]
        if delta_time <= 15 {
            return None;
        }
        tracing::debug!("DELTA TIME: {:?}", delta_time);
        let realized_share_per_min =
            self.shares_since_last_update as f64 / (delta_time as f64 / 60.0);
        tracing::debug!("REALIZED SHARES PER MINUTE: {:?}", realized_share_per_min);
        tracing::debug!("CURRENT MINER TARGET: {:?}", self.current_miner_target);
        let mut new_miner_hashrate = match hash_rate_from_target(
            self.current_miner_target.clone().try_into().unwrap(),
            realized_share_per_min,
        ) {
            Ok(hashrate) => hashrate as f32,
            Err(e) => {
                tracing::debug!("{:?} -> Probably min_individual_miner_hashrate parameter was not set properly in config file. New hashrate will be automatically adjusted to match the real one.", e);
                self.estimated_downstream_hash_rate * realized_share_per_min as f32
                    / self.shares_per_minute
            }
        };

        let mut hashrate_delta = new_miner_hashrate - self.estimated_downstream_hash_rate;
        let hashrate_delta_percentage =
            (hashrate_delta.abs() / self.estimated_downstream_hash_rate) * 100.0;
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
                    dt if dt <= 30 => self.estimated_downstream_hash_rate / 1.5,
                    dt if dt < 60 => self.estimated_downstream_hash_rate / 2.0,
                    _ => self.estimated_downstream_hash_rate / 3.0,
                };
                hashrate_delta = new_miner_hashrate - self.estimated_downstream_hash_rate;
            }
            if (realized_share_per_min > 0.0) && (hashrate_delta_percentage > 1000.0) {
                new_miner_hashrate = match delta_time {
                    dt if dt <= 30 => self.estimated_downstream_hash_rate * 10.0,
                    dt if dt < 60 => self.estimated_downstream_hash_rate * 5.0,
                    _ => self.estimated_downstream_hash_rate * 3.0,
                };
                hashrate_delta = new_miner_hashrate - self.estimated_downstream_hash_rate;
            }
            self.set_hash_rate(new_miner_hashrate);
            self.reset();
        }
        Some((self.estimated_downstream_hash_rate, hashrate_delta))
    }
}
