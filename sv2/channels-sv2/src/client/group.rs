//! Sv2 Group Channel - Mining Client Abstraction.
//!
//! This module provides the [`GroupChannel`] struct, which acts as a mining client's
//! abstraction over the state of a Sv2 group channel. It tracks group-level job state
//! and associated standard and extended channels, but delegates share validation and job lifecycle
//! to the channels themselves.

extern crate alloc;
use super::{HashMap, HashSet, MAX_FUTURE_JOBS};
use crate::client::error::GroupChannelError;
use alloc::collections::VecDeque;
use mining_sv2::{NewExtendedMiningJobOwned, SetNewPrevHashOwned as SetNewPrevHashMp};

/// Mining Client abstraction over the state of an Sv2 Group Channel.
///
/// Tracks:
/// - the group channel's unique `group_channel_id`
/// - associated `channel_ids` (indexed by `channel_id`)
/// - future jobs (indexed by `job_id`, to be activated upon receipt of a
///   [`SetNewPrevHash`](SetNewPrevHashMp) message, capped at [`MAX_FUTURE_JOBS`])
/// - active job
///
/// Does **not** track:
/// - past or stale jobs
/// - share validation state (handled per-channel)
#[derive(Debug, Clone)]
pub struct GroupChannel {
    /// Unique identifier for the group channel
    group_channel_id: u32,
    /// Set of channel IDs associated with this group channel
    channel_ids: HashSet<u32>,
    /// Future jobs, indexed by job_id, waiting to be activated
    future_jobs: HashMap<u32, NewExtendedMiningJobOwned>,
    /// Future job IDs ordered by receipt, oldest at the front and newest at the back.
    /// Replaced IDs move to the back; overflow evicts from the front.
    future_job_order: VecDeque<u32>,
    /// Currently active mining job for the group channel
    active_job: Option<NewExtendedMiningJobOwned>,
    /// Full extranonce size for jobs associated with this group channel.
    /// The constructor initializes this as None, but as new channels are added, we keep this updated.
    /// At no point in time, two channels can belong to the same group while having different full extranonce sizes.
    full_extranonce_size: Option<usize>,
}

impl GroupChannel {
    /// Creates a new [`GroupChannel`] with the given group_channel_id.
    pub fn new(group_channel_id: u32) -> Self {
        Self {
            group_channel_id,
            channel_ids: HashSet::new(),
            future_jobs: HashMap::new(),
            future_job_order: VecDeque::new(),
            active_job: None,
            full_extranonce_size: None,
        }
    }

    /// Adds a channel to the group by its `channel_id` with the specified `full_extranonce_size`.
    /// For extended channels, the `full_extranonce_size` is the sum of its `extranonce_prefix` size and its `rollable_extranonce_size`.
    /// For standard channels, the `full_extranonce_size` is the size of its `extranonce_prefix`.
    ///
    /// If this is the first channel ever added to the group, sets the group's `full_extranonce_size`.
    /// If other channels already exist, validates that the `full_extranonce_size` matches.
    ///
    /// Returns an error if the provided `full_extranonce_size` doesn't match the existing value.
    pub fn add_channel_id(
        &mut self,
        channel_id: u32,
        full_extranonce_size: usize,
    ) -> Result<(), GroupChannelError> {
        match self.full_extranonce_size {
            // if the full extranonce size is already set, check if it matches the new full extranonce size
            Some(existing_size) => {
                if existing_size != full_extranonce_size {
                    return Err(GroupChannelError::FullExtranonceSizeMismatch);
                }
            }
            // if the full extranonce size is not yet set, set it
            None => {
                self.full_extranonce_size = Some(full_extranonce_size);
            }
        }

        self.channel_ids.insert(channel_id);
        Ok(())
    }

    /// Removes a channel from the group channel
    /// channel by its `channel_id`.
    pub fn remove_channel_id(&mut self, channel_id: u32) {
        self.channel_ids.remove(&channel_id);
    }

    /// Returns the group channel ID.
    pub fn get_group_channel_id(&self) -> u32 {
        self.group_channel_id
    }

    /// Returns an iterator over all channel IDs associated with this group channel.
    pub fn get_channel_ids(&self) -> impl Iterator<Item = &u32> + '_ {
        self.channel_ids.iter()
    }

    /// Returns the number of channel IDs associated with this group channel.
    pub fn get_channel_ids_count(&self) -> usize {
        self.channel_ids.len()
    }

    /// Returns `true` if this group channel has no channel IDs associated with it.
    pub fn is_empty(&self) -> bool {
        self.channel_ids.is_empty()
    }

    /// Returns `true` if this group channel contains `channel_id`.
    pub fn has_channel_id(&self, channel_id: u32) -> bool {
        self.channel_ids.contains(&channel_id)
    }

    /// Returns a reference to the current active job, if any.
    pub fn get_active_job(&self) -> Option<&NewExtendedMiningJobOwned> {
        self.active_job.as_ref()
    }

    /// Returns an iterator over all future jobs, keyed by `job_id`.
    ///
    /// At most [`MAX_FUTURE_JOBS`] jobs are kept (oldest evicted first).
    pub fn get_future_jobs(&self) -> impl Iterator<Item = (&u32, &NewExtendedMiningJobOwned)> + '_ {
        self.future_jobs.iter()
    }

    /// Returns a reference to a future job by `job_id`, if present.
    pub fn get_future_job(&self, job_id: u32) -> Option<&NewExtendedMiningJobOwned> {
        self.future_jobs.get(&job_id)
    }

    /// Returns the number of future jobs.
    pub fn get_future_jobs_count(&self) -> usize {
        self.future_jobs.len()
    }

    /// Returns the full extranonce size for jobs associated with this group channel.
    pub fn get_full_extranonce_size(&self) -> Option<usize> {
        self.full_extranonce_size
    }

    /// Handles a newly received [`NewExtendedMiningJob`](mining_sv2::NewExtendedMiningJob) message from upstream.
    ///
    /// - If `min_ntime` is present, sets this job as active.
    /// - If `min_ntime` is empty, stores it as a future job. At most [`MAX_FUTURE_JOBS`] future
    ///   jobs are kept: storing a new one beyond that limit evicts the oldest.
    pub fn on_new_extended_mining_job(
        &mut self,
        new_extended_mining_job: NewExtendedMiningJobOwned,
    ) {
        match new_extended_mining_job.min_ntime.clone().into_inner() {
            Some(_min_ntime) => {
                self.active_job = Some(new_extended_mining_job);
            }
            None => {
                let job_id = new_extended_mining_job.job_id;
                self.future_jobs.insert(job_id, new_extended_mining_job);

                // a replaced job_id moves to the back of the eviction order
                self.future_job_order.retain(|id| *id != job_id);
                self.future_job_order.push_back(job_id);

                if self.future_jobs.len() > MAX_FUTURE_JOBS {
                    if let Some(evicted_job_id) = self.future_job_order.pop_front() {
                        self.future_jobs.remove(&evicted_job_id);
                    }
                }
            }
        }
    }

    /// Handles an upstream [`SetNewPrevHash`](SetNewPrevHashMp) message.
    ///
    /// Activates the future job matching `job_id` from the message, making it the active job.
    /// Clears all other future jobs.
    ///
    /// Returns `Err(GroupChannelError::JobIdNotFound)` if no matching job found.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashMp,
    ) -> Result<(), GroupChannelError> {
        match self.future_jobs.remove(&set_new_prev_hash.job_id) {
            Some(job) => {
                self.active_job = Some(job);
            }
            None => return Err(GroupChannelError::JobIdNotFound),
        }

        // all other future jobs are now useless
        self.future_jobs.clear();
        self.future_job_order.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binary_sv2::Sv2OptionOwned as Sv2Option;

    #[test]
    fn test_future_jobs_are_bounded() {
        let mut group_channel = GroupChannel::new(1);

        let future_job = NewExtendedMiningJobOwned {
            channel_id: 1,
            job_id: 0,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        let flood_size = 10_000u32;
        for job_id in 0..flood_size {
            let mut job = future_job.clone();
            job.job_id = job_id;
            group_channel.on_new_extended_mining_job(job);
        }

        assert_eq!(group_channel.get_future_jobs_count(), MAX_FUTURE_JOBS);

        for job_id in 0..flood_size - MAX_FUTURE_JOBS as u32 {
            assert!(group_channel.get_future_job(job_id).is_none());
        }
        for job_id in flood_size - MAX_FUTURE_JOBS as u32..flood_size {
            assert!(group_channel.get_future_job(job_id).is_some());
        }
    }

    #[test]
    fn test_replaced_future_job_moves_to_back_of_eviction_order() {
        let mut group_channel = GroupChannel::new(1);

        let future_job = NewExtendedMiningJobOwned {
            channel_id: 1,
            job_id: 0,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        // fill the store with MAX_FUTURE_JOBS distinct job_ids
        for job_id in 0..MAX_FUTURE_JOBS as u32 {
            let mut job = future_job.clone();
            job.job_id = job_id;
            group_channel.on_new_extended_mining_job(job);
        }

        // re-send job_id 0: it should move to the back of the eviction order
        group_channel.on_new_extended_mining_job(future_job.clone());

        // one more distinct job_id: job_id 1 is now the oldest and gets evicted
        let mut job = future_job.clone();
        job.job_id = MAX_FUTURE_JOBS as u32;
        group_channel.on_new_extended_mining_job(job);

        assert_eq!(group_channel.get_future_jobs_count(), MAX_FUTURE_JOBS);
        assert!(group_channel.get_future_job(1).is_none());
        assert!(group_channel.get_future_job(0).is_some());

        // the replaced job_id can still be activated
        let set_new_prev_hash = SetNewPrevHashMp {
            channel_id: 1,
            job_id: 0,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            nbits: 503543726,
            min_ntime: 1746839905,
        };
        group_channel
            .on_set_new_prev_hash(set_new_prev_hash)
            .unwrap();
    }

    #[test]
    fn test_add_channel_id() {
        let mut group_channel = GroupChannel::new(1);
        group_channel.add_channel_id(1, 10).unwrap();
        assert_eq!(group_channel.get_full_extranonce_size(), Some(10));

        // add a second channel with the same full extranonce size
        group_channel.add_channel_id(2, 10).unwrap();
        assert_eq!(group_channel.get_full_extranonce_size(), Some(10));

        // add a third channel with a different full extranonce size
        // this should return an error
        assert!(group_channel.add_channel_id(3, 12).is_err());
        assert_eq!(group_channel.get_channel_ids_count(), 2);
        assert!(!group_channel.has_channel_id(3));
        assert_eq!(group_channel.get_full_extranonce_size(), Some(10));
    }
}
