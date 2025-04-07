//! # Standard Channel
//!
//! A channel for receiving and submitting shares for Standard Jobs.

use crate::{
    channel_management::{chain_tip::ChainTip, ShareValidationError, ShareValidationResult},
    mining_sv2::{NewMiningJob, SubmitSharesStandard, SubmitSharesSuccess, Target},
    utils::{target_to_difficulty, u256_to_block_hash},
};
use std::convert::TryInto;
use stratum_common::bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    CompactTarget, Target as BitcoinTarget,
};

/// A Standard Channel creates and tracks PoW submissions via Standard Jobs.
///
/// Channel state consists of:
/// - Channel Id
/// - User Identity String
/// - Extranonce Prefix
/// - Target
/// - Nominal Hashrate
/// - 1 Active + 1 Past Chain Tips
/// - 1 Active + 1 Future + 1 Past Jobs
/// - Share validation state
///     - Last Share Sequence Number
///     - Count of Accepted Shares
///     - Sum of Work (discrete integral over different share difficulties)
///     - Share Batch Size (to know when to send `SubmitShares.Success` messages)
///
/// Differently from the Extended Channel, the Standard Channel does not have a [`JobFactory`].
/// Since every Standard Channel belongs to a Group Channel, the active and future jobs are actually
/// created by the [`GroupChannel`].
///
/// The active and future Extended Jobs are then converted to Standard Jobs at the
/// [`StandardChannelFactory`] and set here so we can track share submission.
#[derive(Clone, Debug)]
pub struct StandardChannel {
    channel_id: u32,
    user_identity: &'static str,
    extranonce_prefix: Vec<u8>,
    target: Target,
    nominal_hashrate: f32,
    active_chain_tip: Option<ChainTip>,
    past_chain_tip: Option<ChainTip>,
    active_job: Option<NewMiningJob<'static>>,
    future_job: Option<NewMiningJob<'static>>,
    past_job: Option<NewMiningJob<'static>>,
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
        share_batch_size: usize,
    ) -> Self {
        Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            target: Target::new(0, 0),
            nominal_hashrate,
            active_chain_tip: None,
            past_chain_tip: None,
            active_job: None,
            future_job: None,
            past_job: None,
            last_share_sequence_number: 0,
            shares_accepted: 0,
            share_work_sum: 0.0,
            share_batch_size,
        }
    }

    /// Set the extranonce prefix.
    pub fn set_extranonce_prefix(&mut self, extranonce_prefix: Vec<u8>) {
        self.extranonce_prefix = extranonce_prefix;
    }

    /// Get the extranonce prefix.
    pub fn get_extranonce_prefix(&self) -> Vec<u8> {
        self.extranonce_prefix.clone()
    }

    /// Set the target.
    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    /// Set the nominal hashrate.   
    pub fn set_nominal_hashrate(&mut self, nominal_hashrate: f32) {
        self.nominal_hashrate = nominal_hashrate;
    }

    /// Set the active job.
    ///
    /// As every Standard Channel belongs to a Group Channel, the active job is actually created by
    /// [`GroupChannel::new_active_job`].
    ///
    /// It is then converted to a Standard Job and set here so we can track share submission.
    pub fn set_active_job(&mut self, job: NewMiningJob<'static>) {
        self.active_job = Some(job);
    }

    /// Get the active job.
    pub fn get_active_job(&self) -> Option<NewMiningJob<'static>> {
        self.active_job.clone()
    }

    /// Set the future job.
    ///
    /// As every Standard Channel belongs to a Group Channel, the future job is actually created by
    /// [`GroupChannel::new_future_job`].
    ///
    /// It is then converted to a Standard Job and set here so we can track share submission.
    pub fn set_future_job(&mut self, job: NewMiningJob<'static>) {
        self.future_job = Some(job);
    }

    /// Get the future job.
    pub fn get_future_job(&self) -> Option<NewMiningJob<'static>> {
        self.future_job.clone()
    }

    /// Push a new chain tip to the channel.
    ///
    /// The currently active chain tip is moved to the past chain tip.
    pub fn push_chain_tip(&mut self, chain_tip: ChainTip) {
        if self.active_chain_tip.is_some() {
            self.past_chain_tip = Some(self.active_chain_tip.clone().unwrap());
        }
        self.active_chain_tip = Some(chain_tip);
    }

    /// Validates a share based on the channel state.
    ///
    /// The return type are [`ShareValidationResult`] / [`ShareValidationError`], which should be
    /// used to decide what to do as an outcome of the share validation process.
    pub fn validate_share(
        &mut self,
        share: &SubmitSharesStandard,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is either the active job or past job
        let is_active_job = self
            .active_job
            .as_ref()
            .map_or(false, |job| job.job_id == job_id);
        let is_past_job = self
            .past_job
            .as_ref()
            .map_or(false, |job| job.job_id == job_id);

        // if job_id is not either the active job or past job, return an error
        if !is_active_job && !is_past_job {
            return Err(ShareValidationError::InvalidJobId);
        }

        // get the actual job + chain tip this share belongs to
        let (job, chain_tip) = if is_active_job {
            (
                self.active_job.as_ref().expect("active job must exist"),
                self.active_chain_tip
                    .as_ref()
                    .expect("active chain tip must exist"),
            )
        } else {
            (
                self.past_job.as_ref().expect("past job must exist"),
                self.past_chain_tip
                    .as_ref()
                    .expect("past chain tip must exist"),
            )
        };

        let merkle_root: [u8; 32] = job
            .merkle_root
            .inner_as_ref()
            .try_into()
            .expect("merkle root must be 32 bytes");

        let prev_hash = chain_tip.prev_hash();
        let nbits = CompactTarget::from_consensus(chain_tip.nbits());

        // create the header for validation
        let header = Header {
            version: Version::from_consensus(share.version as i32),
            prev_blockhash: u256_to_block_hash(prev_hash.clone()),
            merkle_root: (*Hash::from_bytes_ref(&merkle_root)).into(),
            time: share.ntime,
            bits: nbits,
            nonce: share.nonce,
        };

        // convert the header hash to a target type for easy comparison
        let hash = header.block_hash();
        let raw_hash: [u8; 32] = *hash.to_raw_hash().as_ref();
        let target: Target = raw_hash.into();

        // check if the share hash is a valid block
        let network_target = BitcoinTarget::from_compact(nbits);
        if network_target.is_met_by(hash) {
            self.last_share_sequence_number = share.sequence_number;
            self.shares_accepted += 1;
            self.share_work_sum += target_to_difficulty(target);

            return Ok(ShareValidationResult::BlockFound);
        }

        if is_past_job {
            return Err(ShareValidationError::Stale);
        }

        // check if the share hash meets the channel target
        if target <= self.target {
            self.last_share_sequence_number = share.sequence_number;
            self.shares_accepted += 1;
            self.share_work_sum += target_to_difficulty(target);

            // if sequence number is a multiple of share_batch_size
            // it's time to send a SubmitShares.Success
            if share.sequence_number % self.share_batch_size as u32 == 0 {
                return Ok(ShareValidationResult::ValidWithAcknowledgement);
            } else {
                return Ok(ShareValidationResult::Valid);
            }
        } else {
            Err(ShareValidationError::DoesNotMeetTarget)
        }
    }

    /// Returns a [`SubmitSharesSuccess`] message based on the current channel state.
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
    NoActiveChainTip,
}
