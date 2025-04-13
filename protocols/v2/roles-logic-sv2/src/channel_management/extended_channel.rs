//! # Extended Channel
//!
//! A channel for receiving and submitting shares for Extended Jobs.
use crate::{
    channel_management::{chain_tip::ChainTip, ShareValidationError, ShareValidationResult},
    job_management::JobFactory,
    mining_sv2::{NewExtendedMiningJob, SubmitSharesExtended, SubmitSharesSuccess, Target},
    template_distribution_sv2::NewTemplate,
    utils::{merkle_root_from_path, target_to_difficulty, u256_to_block_hash},
};
use std::convert::TryInto;
use stratum_common::bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    transaction::TxOut,
    CompactTarget, Target as BitcoinTarget,
};

/// An Extended Channel creates and tracks PoW submissions via Extended Jobs.
///
/// Channel state consists of:
/// - Channel Id
/// - User Identity String
/// - Extranonce Prefix
/// - Rollable Extranonce Size
/// - Target
/// - Nominal Hashrate
/// - [`JobFactory`] internal state
///     - Job ID Factory
///     - 1 Future Template
///     - 1 Active Template
///     - Multiple Coinbase Outputs for Reward distribution
/// - 1 Active + 1 Past Chain Tips
/// - 1 Active + 1 Future + 1 Past Jobs
/// - Share Validation State
///     - Last Share Sequence Number
///     - Count of Accepted Shares
///     - Sum of Work (discrete integral over different share difficulties)
///     - Share Batch Size (to know when to send `SubmitShares.Success` messages)

#[derive(Clone, Debug)]
pub struct ExtendedChannel {
    channel_id: u32,
    user_identity: &'static str,
    extranonce_prefix: Vec<u8>,
    rollable_extranonce_size: u16,
    target: Target,
    nominal_hashrate: f32,
    job_factory: JobFactory,
    active_chain_tip: Option<ChainTip>,
    past_chain_tip: Option<ChainTip>,
    active_job: Option<NewExtendedMiningJob<'static>>,
    future_job: Option<NewExtendedMiningJob<'static>>,
    past_job: Option<NewExtendedMiningJob<'static>>,
    last_share_sequence_number: u32,
    shares_accepted: u32,
    share_work_sum: f64,
    share_batch_size: usize,
}

impl ExtendedChannel {
    pub fn new(
        channel_id: u32,
        user_identity: &'static str,
        extranonce_prefix: Vec<u8>,
        rollable_extranonce_size: u16,
        nominal_hashrate: f32,
        share_batch_size: usize,
        version_rolling_allowed: bool,
    ) -> Self {
        Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size,
            target: Target::new(0, 0), // default target
            nominal_hashrate,
            job_factory: JobFactory::new(
                Some(rollable_extranonce_size as u8),
                Some(version_rolling_allowed),
            ),
            active_chain_tip: None,
            past_chain_tip: None,
            active_job: None,
            future_job: None,
            past_job: None,
            last_share_sequence_number: 0,
            shares_accepted: 0,
            share_work_sum: 0.0,
            share_batch_size: share_batch_size,
        }
    }

    /// Set the extranonce prefix.
    pub fn set_extranonce_prefix(&mut self, extranonce_prefix: Vec<u8>) {
        self.extranonce_prefix = extranonce_prefix;
    }

    /// Set the full extranonce size.
    pub fn set_full_extranonce_size(&mut self, extranonce_size: u16) {
        self.rollable_extranonce_size = extranonce_size;
    }

    /// Set the target.
    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    /// Set the nominal hashrate.
    pub fn set_nominal_hashrate(&mut self, nominal_hashrate: f32) {
        self.nominal_hashrate = nominal_hashrate;
    }

    /// Set the future template of the JobFactory.
    pub fn set_future_template(&mut self, template: NewTemplate<'static>) {
        self.job_factory.set_future_template(template);
    }

    /// Set the currently active template of the JobFactory.
    pub fn set_active_template(&mut self, template: NewTemplate<'static>) {
        self.job_factory.set_active_template(template);
    }

    /// Push a new active chain tip.
    ///
    /// If there is already an active chain tip, it is moved to the past chain tip.
    pub fn push_chain_tip(&mut self, chain_tip: ChainTip) {
        if self.active_chain_tip.is_some() {
            self.past_chain_tip = Some(self.active_chain_tip.clone().unwrap());
        }
        self.active_chain_tip = Some(chain_tip);
    }

    /// Set the coinbase outputs.
    pub fn set_coinbase_outputs(&mut self, coinbase_outputs: Vec<TxOut>) {
        self.job_factory
            .set_additional_coinbase_outputs(coinbase_outputs);
    }

    /// Leverage the JobFactory to create a new active job from the current template.
    ///
    /// The Channel State is updated with the new active job, while the previous active job is moved
    /// to the past job.
    pub fn new_active_job(&mut self) -> Result<(), ExtendedChannelError> {
        let active_chain_tip = self
            .active_chain_tip
            .as_ref()
            .ok_or(ExtendedChannelError::NoActiveChainTip)?;

        let job = self
            .job_factory
            .new_extended_job(
                self.channel_id,
                Some(active_chain_tip.clone()),
                self.extranonce_prefix.len(),
            )
            .map_err(|_| ExtendedChannelError::JobFactoryError)?;

        if let Some(past_job) = self.active_job.take() {
            self.past_job = Some(past_job);
        }

        self.active_job = Some(job.clone());
        Ok(())
    }

    /// Get the active job.
    pub fn get_active_job(&self) -> Option<NewExtendedMiningJob<'static>> {
        self.active_job.clone()
    }

    /// Leverage the internal [`JobFactory`] to create a new future job from the future template.
    ///
    /// The Channel State is updated with the new future job.
    pub fn new_future_job(&mut self) -> Result<(), ExtendedChannelError> {
        let job = self
            .job_factory
            .new_extended_job(self.channel_id, None, self.extranonce_prefix.len())
            .map_err(|_| ExtendedChannelError::JobFactoryError)?;

        self.future_job = Some(job.clone());
        Ok(())
    }

    /// Get the future job.
    pub fn get_future_job(&self) -> Option<NewExtendedMiningJob<'static>> {
        self.future_job.clone()
    }

    /// Validates a share based on the channel state.
    ///
    /// The return type are [`ShareValidationResult`] / [`ShareValidationError`], which should be
    /// used to decide what to do as an outcome of the share validation process.
    pub fn validate_share(
        &mut self,
        share: &SubmitSharesExtended,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is either the active job or some past job
        let is_active_job = self
            .active_job
            .as_ref()
            .map_or(false, |job| job.job_id == job_id);
        let is_past_job = self
            .past_job
            .as_ref()
            .map_or(false, |job| job.job_id == job_id);

        // if job_id is not either the active job or some past job, return an error
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

        // calculate the merkle root from the coinbase tx prefix, suffix, extranonce, and merkle
        // path
        let merkle_root: [u8; 32] = merkle_root_from_path(
            job.coinbase_tx_prefix.inner_as_ref(),
            job.coinbase_tx_suffix.inner_as_ref(),
            share.extranonce.inner_as_ref(),
            &job.merkle_path.inner_as_ref(),
        )
        .ok_or(ShareValidationError::Invalid)?
        .try_into()
        .expect("merkle root must be 32 bytes");

        let prev_hash = chain_tip.prev_hash();
        let nbits = CompactTarget::from_consensus(chain_tip.nbits());

        // Validate when version rolling is not allowed
        if !job.version_rolling_allowed {
            // If version rolling is not allowed, ensure bits 13-28 are 0
            // This is done by checking if the version & 0x1fffe000 == 0
            if (share.version & 0x1fffe000) != 0 {
                return Err(ShareValidationError::VersionRollingNotAllowed);
            }
        }

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

    /// Returns a [`SubmitSharesSuccess message based on the current channel state.
    pub fn get_share_acknowledgement(&self) -> SubmitSharesSuccess {
        SubmitSharesSuccess {
            channel_id: self.channel_id,
            last_sequence_number: self.last_share_sequence_number,
            new_submits_accepted_count: self.shares_accepted,
            new_shares_sum: self.share_work_sum as u64,
        }
    }
}

#[derive(Debug)]
pub enum ExtendedChannelError {
    JobFactoryError,
    ShareHasInvalidJobId,
    InvalidShare,
    NoPrevHash,
    NoNbits,
    NoActiveChainTip,
    ShareDoesNotMeetTarget,
}
