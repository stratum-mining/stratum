//! Sv2 Group Channel - Mining Client Abstraction.
//!
//! This module provides the [`GroupChannel`] struct, which acts as a mining client's
//! abstraction over the state of a Sv2 group channel. It tracks group-level job state
//! and associated standard channels, but delegates share validation and job lifecycle
//! to standard channels.

use super::{HashMap, HashSet};
use crate::client::error::GroupChannelError;
use mining_sv2::{NewExtendedMiningJob, SetNewPrevHash as SetNewPrevHashMp};

/// Mining Client abstraction over the state of an Sv2 Group Channel.
///
/// Tracks:
/// - the group channel's unique `group_channel_id`
/// - associated `standard_channel_ids` (indexed by `channel_id`)
/// - future jobs (indexed by `job_id`, to be activated upon receipt of a
///   [`SetNewPrevHash`](SetNewPrevHashMp) message)
/// - active job
///
/// Does **not** track:
/// - past or stale jobs
/// - share validation state (handled per-standard channel)
#[derive(Debug, Clone)]
pub struct GroupChannel<'a> {
    /// Unique identifier for the group channel
    group_channel_id: u32,
    /// Set of channel IDs associated with this group channel
    standard_channel_ids: HashSet<u32>,
    /// Future jobs, indexed by job_id, waiting to be activated
    future_jobs: HashMap<u32, NewExtendedMiningJob<'a>>,
    /// Currently active mining job for the group channel
    active_job: Option<NewExtendedMiningJob<'a>>,
}

impl<'a> GroupChannel<'a> {
    /// Creates a new [`GroupChannel`] with the given group_channel_id.
    pub fn new(group_channel_id: u32) -> Self {
        Self {
            group_channel_id,
            standard_channel_ids: HashSet::new(),
            future_jobs: HashMap::new(),
            active_job: None,
        }
    }

    /// Adds a [`StandardChannel`](crate::client::standard::StandardChannel) to the group channel
    /// by referencing its `channel_id`.
    pub fn add_standard_channel_id(&mut self, standard_channel_id: u32) {
        self.standard_channel_ids.insert(standard_channel_id);
    }

    /// Removes a [`StandardChannel`](crate::client::standard::StandardChannel) from the group
    /// channel by its `channel_id`.
    pub fn remove_standard_channel_id(&mut self, standard_channel_id: u32) {
        self.standard_channel_ids.remove(&standard_channel_id);
    }

    /// Returns the group channel ID.
    pub fn get_group_channel_id(&self) -> u32 {
        self.group_channel_id
    }

    /// Returns a reference to all standard channel IDs associated with this group channel.
    pub fn get_standard_channel_ids(&self) -> &HashSet<u32> {
        &self.standard_channel_ids
    }

    /// Returns a reference to the current active job, if any.
    pub fn get_active_job(&self) -> Option<&NewExtendedMiningJob<'a>> {
        self.active_job.as_ref()
    }

    /// Returns a reference to all future jobs indexed by job_id.
    pub fn get_future_jobs(&self) -> &HashMap<u32, NewExtendedMiningJob<'a>> {
        &self.future_jobs
    }

    /// Handles a newly received [`NewExtendedMiningJob`] message from upstream.
    ///
    /// - If `min_ntime` is present, sets this job as active.
    /// - If `min_ntime` is empty, stores it as a future job.
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

    /// Handles an upstream [`SetNewPrevHash`](SetNewPrevHashMp) message.
    ///
    /// Activates the future job matching `job_id` from the message, making it the active job.
    /// Clears all other future jobs.
    ///
    /// Returns `Err(GroupChannelError::JobIdNotFound)` if no matching job found.
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
