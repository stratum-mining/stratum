//! # Standard Channel
//!
//! A channel for receiving and submitting shares for Standard Jobs.

use crate::{
    job_management::JobFactory,
    mining_sv2::{NewMiningJob, SubmitSharesStandard, SubmitSharesSuccess, Target},
    template_distribution_sv2::NewTemplate,
    utils::{target_to_difficulty, u256_to_block_hash},
};
use binary_sv2::U256;
use std::{collections::VecDeque, convert::TryInto};
use stratum_common::bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    BlockHash, CompactTarget,
};

/// A Standard Channel receives Standard Jobs.
///
/// A Standard Channel keeps track of one current, one future, and few multiple past jobs.
///
/// Job Management can be done in two ways:
/// 1. Using the JobFactory to actively create jobs and send them to the network. Useful for mining
///    servers.
/// 2. Receiving `NewMiningJob` messages and pushing them to the Channel State. Useful for mining
///    clients.
///
/// A Standard Channel keeps track of the share submission process for current and past jobs.
///
/// Not thread safe, we expect concurrency protection to be managed by the caller.
pub struct StandardChannel {
    channel_id: u32,
    user_identity: &'static str,
    extranonce_prefix: Vec<u8>,
    target: Target,
    nominal_hashrate: f32,
    job_factory: JobFactory,
    active_prev_hash: Option<U256<'static>>,
    active_nbits: Option<u32>,
    active_job: Option<NewMiningJob<'static>>,
    future_job: Option<NewMiningJob<'static>>,
    past_jobs: VecDeque<NewMiningJob<'static>>,
    last_share_sequence_number: u32,
    shares_accepted: u32,
    share_work_sum: f64,
    share_batch_size: usize,
}

impl StandardChannel {
    pub fn new(
        channel_id: u32,
        user_identity: &'static str,
        extranonce_prefix: Vec<u8>,
        nominal_hashrate: f32,
        past_jobs_capacity: usize,
        share_batch_size: usize,
    ) -> Self {
        let job_creator = JobFactory::new(0); // standard jobs have extranonce_len = 0
        Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            target: Target::new(0, 0),
            nominal_hashrate,
            job_factory: job_creator,
            active_prev_hash: None,
            active_nbits: None,
            active_job: None,
            future_job: None,
            past_jobs: VecDeque::with_capacity(past_jobs_capacity),
            last_share_sequence_number: 0,
            shares_accepted: 0,
            share_work_sum: 0.0,
            share_batch_size,
        }
    }

    // push a past job to the past jobs queue
    // if the queue is full, the oldest job is removed
    fn push_past_job(&mut self, job: NewMiningJob<'static>) {
        if self.past_jobs.len() == self.past_jobs.capacity() {
            self.past_jobs.pop_back();
        }
        self.past_jobs.push_front(job);
    }

    pub fn set_extranonce_prefix(&mut self, extranonce_prefix: Vec<u8>) {
        self.extranonce_prefix = extranonce_prefix;
    }

    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    pub fn set_nominal_hashrate(&mut self, nominal_hashrate: f32) {
        self.nominal_hashrate = nominal_hashrate;
    }

    /// Set the future template of the JobFactory.
    ///
    /// This method meant for users that are going to use the JobFactory to create jobs.
    pub fn set_future_template(&mut self, template: NewTemplate<'static>) {
        self.job_factory.set_future_template(template);
    }

    /// Set the currently active template of the JobFactory.
    ///
    /// This method meant for users that are going to use the JobFactory to create jobs.
    pub fn set_active_template(&mut self, template: NewTemplate<'static>) {
        self.job_factory.set_current_template(template);
    }

    /// Set the active prev_hash
    pub fn set_active_prev_hash(&mut self, prev_hash: U256<'static>) {
        self.active_prev_hash = Some(prev_hash);
    }

    /// Set the active nbits
    pub fn set_active_nbits(&mut self, nbits: u32) {
        self.active_nbits = Some(nbits);
    }

    /// Push the currently active job to the past jobs and set the new active job.
    ///
    /// This method meant for users that are not using the JobFactory to create jobs,
    /// but instead are receiving `NewMiningJob` messages and need to push them to the Channel
    /// State.
    pub fn push_active_job(&mut self, job: NewMiningJob<'static>) {
        if let Some(past_job) = self.active_job.take() {
            self.push_past_job(past_job);
        }
        self.active_job = Some(job);
    }

    /// Set the future job.
    ///
    /// This method meant for users that are not using the JobFactory to create jobs,
    /// but instead are receiving `NewMiningJob` messages and need to push them to the Channel
    /// State.
    pub fn set_future_job(&mut self, job: NewMiningJob<'static>) {
        self.future_job = Some(job);
    }

    /// Leverage the JobFactory to create a new active job from the current template.
    ///
    /// This method is meant for users that are using the JobFactory to create jobs.
    ///
    /// The Channel State is updated with the new active job.
    ///
    /// And we return the `NewMiningJob` message that can be sent to the network.
    pub fn new_active_job(&mut self) -> Result<NewMiningJob<'static>, StandardChannelError> {
        let job = self
            .job_factory
            .standard_job_from_current_template()
            .map_err(|_| StandardChannelError::JobFactoryError)?;
        if let Some(past_job) = self.active_job.take() {
            self.push_past_job(past_job);
        }
        self.active_job = Some(job.clone());
        Ok(job)
    }

    /// Leverage the JobFactory to create a new future job from the future template.
    ///
    /// This method is meant for users that are using the JobFactory to create jobs.
    ///
    /// The Channel State is updated with the new future job.
    ///
    /// And we return the `NewMiningJob` message that can be sent to the network.
    pub fn new_future_job(&mut self) -> Result<NewMiningJob<'static>, StandardChannelError> {
        let job = self
            .job_factory
            .standard_job_from_future_template()
            .map_err(|_| StandardChannelError::JobFactoryError)?;
        self.future_job = Some(job.clone());
        Ok(job)
    }

    /// Validates a share based on the channel state.
    ///
    /// Returns a boolean indicating whether it's time to send a SubmitShares.Success
    pub fn validate_share(
        &mut self,
        share: &SubmitSharesStandard,
    ) -> Result<bool, StandardChannelError> {
        let job_id = share.job_id;

        // check if job_id is either the active job or some past job
        let is_active_job = self
            .active_job
            .as_ref()
            .map_or(false, |job| job.job_id == job_id);
        let is_past_job = self.past_jobs.iter().any(|job| job.job_id == job_id);

        // if job_id is not either the active job or some past job, return an error
        if !is_active_job && !is_past_job {
            return Err(StandardChannelError::ShareHasInvalidJobId);
        }

        // get the actual job this share belongs to
        let job = if is_active_job {
            self.active_job.as_ref().expect("active job must exist")
        } else {
            self.past_jobs
                .iter()
                .find(|job| job.job_id == job_id)
                .expect("job must exist")
        };

        let merkle_root: [u8; 32] = job
            .merkle_root
            .inner_as_ref()
            .try_into()
            .expect("merkle root must be 32 bytes");
        let prev_hash = self
            .active_prev_hash
            .as_ref()
            .ok_or(StandardChannelError::NoPrevHash)?;
        let nbits = self.active_nbits.ok_or(StandardChannelError::NoNbits)?;

        // create the header for validation
        let header = Header {
            version: Version::from_consensus(job.version as i32),
            prev_blockhash: u256_to_block_hash(prev_hash.clone()),
            merkle_root: (*Hash::from_bytes_ref(&merkle_root)).into(),
            time: 0,
            bits: CompactTarget::from_consensus(nbits),
            nonce: 0,
        };

        // convert the header hash to a target type for easy comparison
        let hash = header.block_hash();
        let hash: [u8; 32] = *hash.to_raw_hash().as_ref();
        let hash: Target = hash.into();

        // check if the share hash meets the target
        if hash <= self.target {
            self.last_share_sequence_number = share.sequence_number;
            self.shares_accepted += 1;
            self.share_work_sum += target_to_difficulty(hash);

            // if sequence number is a multiple of share_batch_size, return true
            // because it's time to send a SubmitShares.Success
            if share.sequence_number % self.share_batch_size as u32 == 0 {
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Err(StandardChannelError::ShareDoesNotMeetTarget)
        }
    }

    /// Returns a SubmitSharesSuccess message based on the channel state.
    pub fn get_shares_acknowledgement(&self) -> SubmitSharesSuccess {
        SubmitSharesSuccess {
            channel_id: self.channel_id,
            last_sequence_number: self.last_share_sequence_number,
            new_submits_accepted_count: self.shares_accepted,
            new_shares_sum: self.share_work_sum as u64,
        }
    }
}

pub enum StandardChannelError {
    JobFactoryError,
    ShareHasInvalidJobId,
    NoPrevHash,
    NoNbits,
    ShareDoesNotMeetTarget,
}
