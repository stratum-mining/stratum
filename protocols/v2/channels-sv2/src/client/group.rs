//! Abstraction over the state of a Sv2 Group Channel, as seen by a Mining Client

use crate::client::error::GroupChannelError;

use std::collections::{HashMap, HashSet};

use mining_sv2::{NewExtendedMiningJob, SetNewPrevHash as SetNewPrevHashMp};

/// Mining Client abstraction over the state of a Sv2 Group Channel.
///
/// It keeps track of:
/// - the group channel's unique `group_channel_id`
/// - the group channel's `standard_channel_ids` (indexed by `channel_id`)
/// - the group channel's future jobs (indexed by `job_id`, to be activated upon receipt of a
///   `SetNewPrevHash` message)
/// - the group channel's active job
///
/// Since share validation happens at the Standard Channel level, we don't really keep track of:
/// - the group channel's past jobs
/// - the group channel's stale jobs
/// - the group channel's share validation state
#[derive(Debug, Clone)]
pub struct GroupChannel<'a> {
    group_channel_id: u32,
    standard_channel_ids: HashSet<u32>,
    // future jobs are indexed with job_id (u32)
    future_jobs: HashMap<u32, NewExtendedMiningJob<'a>>,
    active_job: Option<NewExtendedMiningJob<'a>>,
}

impl<'a> GroupChannel<'a> {
    pub fn new(group_channel_id: u32) -> Self {
        Self {
            group_channel_id,
            standard_channel_ids: HashSet::new(),
            future_jobs: HashMap::new(),
            active_job: None,
        }
    }

    pub fn add_standard_channel_id(&mut self, standard_channel_id: u32) {
        self.standard_channel_ids.insert(standard_channel_id);
    }

    pub fn remove_standard_channel_id(&mut self, standard_channel_id: u32) {
        self.standard_channel_ids.remove(&standard_channel_id);
    }

    pub fn get_group_channel_id(&self) -> u32 {
        self.group_channel_id
    }

    pub fn get_standard_channel_ids(&self) -> &HashSet<u32> {
        &self.standard_channel_ids
    }

    pub fn get_active_job(&self) -> Option<&NewExtendedMiningJob<'a>> {
        self.active_job.as_ref()
    }

    pub fn get_future_jobs(&self) -> &HashMap<u32, NewExtendedMiningJob<'a>> {
        &self.future_jobs
    }

    /// Called when a `NewExtendedMiningJob` message is received from upstream.
    ///
    /// If the job is a future job, it is added to the `future_jobs` map.
    /// If the job is an active job, it is set as the active job.
    pub fn on_new_extended_mining_job(
        &mut self,
        new_extended_mining_job: NewExtendedMiningJob<'a>,
    ) {
        match new_extended_mining_job.min_ntime.clone().into_inner() {
            Some(_min_ntime) => {
                self.active_job = Some(new_extended_mining_job);
            }
            None => {
                self.future_jobs
                    .insert(new_extended_mining_job.job_id, new_extended_mining_job);
            }
        }
    }

    /// Called when a `SetNewPrevHash` message is received from upstream.
    ///
    /// If there is some future job matching the `job_id` that `SetNewPrevHash` points to,
    /// this future job is "activated" and set as the active job.
    ///
    /// If there is not future job matching the `job_id` that `SetNewPrevHash` points to,
    /// returns an error.
    ///
    /// All other future jobs are cleared.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashMp<'a>,
    ) -> Result<(), GroupChannelError> {
        match self.future_jobs.remove(&set_new_prev_hash.job_id) {
            Some(job) => {
                self.active_job = Some(job);
            }
            None => return Err(GroupChannelError::JobIdNotFound),
        }

        // all other future jobs are now useless
        self.future_jobs.clear();
        Ok(())
    }
}
