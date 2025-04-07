//! # Group Channel
//!
//! A collection of `channel_id`s representing Standard Channels.
//!
//! Receives Extended Jobs and keeps them in state for conversion to Standard Jobs.

use crate::mining_sv2::NewExtendedMiningJob;
use std::collections::{HashSet, VecDeque};
/// Every Standard Channel belongs to some Group Channel.
///
/// A Group Channel receives Extended Jobs.
///
/// A Group Channel keeps track of one current, one future, and few multiple past jobs.
pub struct GroupChannel {
    channel_id: u32,
    channels: HashSet<u32>,
    current_job: Option<NewExtendedMiningJob<'static>>,
    future_job: Option<NewExtendedMiningJob<'static>>,
    past_jobs: VecDeque<NewExtendedMiningJob<'static>>,
}

impl GroupChannel {
    /// Create a new [`GroupChannel`].
    ///
    /// * `channel_id` - The ID of the channel.
    /// * `past_jobs_size` - The number of past jobs to keep. Once full, pushing new jobs will
    ///   remove the oldest job.
    pub fn new(channel_id: u32, past_jobs_capacity: usize) -> Self {
        Self {
            channel_id,
            channels: HashSet::new(),
            current_job: None,
            future_job: None,
            past_jobs: VecDeque::with_capacity(past_jobs_capacity),
        }
    }

    /// Get the IDs of all channels in the group channel.
    pub async fn get_channel_ids(&self) -> Vec<u32> {
        todo!()
    }

    /// Add a channel to the group channel.
    pub async fn add_channel(&mut self, channel_id: u32) {
        todo!()
    }

    /// Remove a channel from the group channel.
    pub async fn remove_channel(&mut self, channel_id: u32) {
        todo!()
    }

    /// Set the current job for the group channel.
    /// Automatically pushes the previous current job to the `past_jobs` array.
    /// If the `past_jobs` array is full, the oldest job is removed.
    pub async fn set_current_job(&mut self, job: NewExtendedMiningJob<'static>) {
        todo!()
    }

    /// Set the future job for the group channel.
    pub async fn set_future_job(&mut self, job: NewExtendedMiningJob<'static>) {
        todo!()
    }
}
