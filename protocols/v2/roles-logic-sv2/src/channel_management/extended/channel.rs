//! Abstraction over the state of a Sv2 Extended Channel

pub mod error;

use crate::{
    channel_management::{
        extended::channel::error::ExtendedChannelError,
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    job_management::{
        chain_tip::ChainTip,
        extended::{ExtendedJob, ExtendedJobFactory},
    },
    utils::{merkle_root_from_path, target_to_difficulty, u256_to_block_hash},
};
use mining_sv2::{SetCustomMiningJob, SubmitSharesExtended, Target};
use std::{collections::HashMap, convert::TryInto};
use stratum_common::bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    transaction::TxOut,
    CompactTarget, Target as BitcoinTarget,
};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};

/// Abstraction of a Sv2 Extended Channel.
///
/// It keeps track of:
/// - the channel's unique `channel_id`
/// - the channel's `user_identity`
/// - the channel's unique `extranonce_prefix`
/// - the channel's rollable extranonce size
/// - the channel's target
/// - the channel's nominal hashrate
/// - the channel's future jobs (indexed by `template_id`, to be activated upon receipt of a
///   `SetNewPrevHash` message)
/// - the channel's active job
/// - the channel's past jobs (which were active jobs under the current chain tip, indexed by
///   `job_id`)
/// - the channel's stale jobs (which were past and active jobs under the previous chain tip,
///   indexed by `job_id`)
/// - the channel's share validation state
/// - the channel's job factory
#[derive(Clone, Debug)]
pub struct ExtendedChannel<'a> {
    channel_id: u32,
    user_identity: String,
    extranonce_prefix: Vec<u8>,
    rollable_extranonce_size: u16,
    target: Target, // todo: try to use Target from rust-bitcoin
    nominal_hashrate: f32,
    // future jobs are indexed with template_id (u64)
    future_jobs: HashMap<u64, ExtendedJob<'a>>,
    active_job: Option<ExtendedJob<'a>>,
    // past jobs are indexed with job_id (u32)
    past_jobs: HashMap<u32, ExtendedJob<'a>>,
    // stale jobs are indexed with job_id (u32)
    stale_jobs: HashMap<u32, ExtendedJob<'a>>,
    job_factory: ExtendedJobFactory,
    share_accounting: ShareAccounting,
}

impl<'a> ExtendedChannel<'a> {
    pub fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        target: Target,
        nominal_hashrate: f32,
        version_rolling_allowed: bool,
        rollable_extranonce_size: u16,
        share_batch_size: usize,
    ) -> Self {
        Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size,
            target,
            nominal_hashrate,
            future_jobs: HashMap::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            stale_jobs: HashMap::new(),
            job_factory: ExtendedJobFactory::new(rollable_extranonce_size, version_rolling_allowed),
            share_accounting: ShareAccounting::new(share_batch_size),
        }
    }

    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    pub fn get_user_identity(&self) -> &String {
        &self.user_identity
    }

    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }

    pub fn set_extranonce_prefix(&mut self, extranonce_prefix: Vec<u8>) {
        self.extranonce_prefix = extranonce_prefix;
    }

    pub fn get_rollable_extranonce_size(&self) -> u16 {
        self.rollable_extranonce_size
    }

    pub fn get_target(&self) -> &Target {
        &self.target
    }

    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    pub fn set_nominal_hashrate(&mut self, nominal_hashrate: f32) {
        self.nominal_hashrate = nominal_hashrate;
    }

    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    pub fn get_active_job(&self) -> Option<&ExtendedJob<'a>> {
        self.active_job.as_ref()
    }

    pub fn get_future_jobs(&self) -> &HashMap<u64, ExtendedJob<'a>> {
        &self.future_jobs
    }

    pub fn get_past_jobs(&self) -> &HashMap<u32, ExtendedJob<'a>> {
        &self.past_jobs
    }

    pub fn get_share_accounting(&self) -> &ShareAccounting {
        &self.share_accounting
    }

    /// Updates the channel state with a new template.
    ///
    /// If the template is a future template, the chain tip is not used.
    /// If the template is not a future template, the chain tip must be set.
    pub fn on_new_template(
        &mut self,
        template: NewTemplate<'a>,
        coinbase_reward_outputs: Vec<TxOut>,
        chain_tip: Option<ChainTip>,
    ) -> Result<(), ExtendedChannelError> {
        let new_job = self
            .job_factory
            .new_job(
                self.channel_id,
                chain_tip.clone(), /* chain_tip is checked above for non-future templates on the
                                    * factory */
                self.extranonce_prefix.clone().len(),
                template.clone(),
                coinbase_reward_outputs,
            )
            .map_err(|e| ExtendedChannelError::JobFactoryError(e))?;

        match template.future_template {
            // if the template is a future template, add it to the future jobs
            true => {
                self.future_jobs.insert(template.template_id, new_job);
            }
            false => {
                // if there's already some active job, move it to the past jobs
                // and set the new job as the active job
                if let Some(active_job) = self.active_job.take() {
                    self.past_jobs.insert(active_job.get_job_id(), active_job);
                    self.active_job = Some(new_job);
                } else {
                    // if there's no active job, simply set the new job as the active job
                    self.active_job = Some(new_job);
                }
            }
        }

        Ok(())
    }

    /// Updates the channel state with a new `ChainTip`.
    ///
    /// If there are no future jobs, the active job is set to `None`.
    /// If there are future jobs, the active job is set to the job with the given `template_id`.
    ///
    /// All past jobs are cleared.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHash<'a>,
    ) -> Result<(), ExtendedChannelError> {
        match self.future_jobs.is_empty() {
            true => {
                // if there are no future jobs, SetNewPrevHash.template_id is ignored
                // we simply set the active job to None so that whenever
                // a NewTemplate message arrives (with future_template = false),
                // the corresponding job will be set as active
                self.active_job = None;
            }
            false => {
                // the SetNewPrevHash message was addressed to a specific future template
                if !self
                    .future_jobs
                    .contains_key(&set_new_prev_hash.template_id)
                {
                    return Err(ExtendedChannelError::TemplateIdNotFound);
                }

                // move currently active job to past jobs (so it can be marked as stale)
                let currently_active_job = self.active_job.take();
                if let Some(active_job) = currently_active_job {
                    self.past_jobs.insert(active_job.get_job_id(), active_job);
                }

                // activate the future job
                let mut activated_job = self
                    .future_jobs
                    .remove(&set_new_prev_hash.template_id)
                    .expect("future job must exist");

                activated_job.activate(set_new_prev_hash.header_timestamp);

                self.active_job = Some(activated_job);
            }
        }

        // mark all past jobs as stale, so that shares can be rejected with the appropriate error
        // code
        self.stale_jobs = self.past_jobs.clone();

        // clear past jobs, as we're no longer going to validate shares for them
        self.past_jobs.clear();

        Ok(())
    }

    /// Updates the channel state with a new custom mining job.
    ///
    /// If there is an active job, it is moved to the past jobs.
    /// The new custom mining job is then set as the active job.
    pub fn on_set_custom_mining_job(
        &mut self,
        set_custom_mining_job: SetCustomMiningJob<'a>,
    ) -> Result<u32, ExtendedChannelError> {
        let new_job = self
            .job_factory
            .new_custom_job(set_custom_mining_job, self.extranonce_prefix.len())
            .map_err(|e| ExtendedChannelError::JobFactoryError(e))?;

        let job_id = new_job.get_job_id();

        if let Some(active_job) = self.active_job.take() {
            self.past_jobs.insert(active_job.get_job_id(), active_job);
        }

        self.active_job = Some(new_job);

        Ok(job_id)
    }

    /// Validates a share.
    ///
    /// Updates the channel state with the result of the share validation.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesExtended,
        chain_tip: ChainTip,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is active job
        let is_active_job = self
            .active_job
            .as_ref()
            .map_or(false, |job| job.get_job_id() == job_id);

        // check if job_id is past job
        let is_past_job = self.past_jobs.contains_key(&job_id);

        // check if job_id is stale job
        let is_stale_job = self.stale_jobs.contains_key(&job_id);

        // if job_id is not active, past or stale, return error
        if !is_active_job && !is_past_job && !is_stale_job {
            return Err(ShareValidationError::InvalidJobId);
        }

        let job = if is_active_job {
            self.active_job.as_ref().expect("active job must exist")
        } else if is_past_job {
            self.past_jobs.get(&job_id).expect("past job must exist")
        } else {
            self.stale_jobs.get(&job_id).expect("stale job must exist")
        };

        if is_stale_job {
            return Err(ShareValidationError::Stale);
        }

        let mut full_extranonce = vec![];
        full_extranonce.extend(self.extranonce_prefix.clone());
        full_extranonce.extend(share.extranonce.inner_as_ref());

        // calculate the merkle root from:
        // - job coinbase_tx_prefix
        // - full extranonce
        // - job coinbase_tx_suffix
        // - job merkle_path
        let merkle_root: [u8; 32] = merkle_root_from_path(
            job.get_coinbase_tx_prefix().inner_as_ref(),
            job.get_coinbase_tx_suffix().inner_as_ref(),
            full_extranonce.as_ref(),
            &job.get_merkle_path().inner_as_ref(),
        )
        .ok_or(ShareValidationError::Invalid)?
        .try_into()
        .expect("merkle root must be 32 bytes");

        let prev_hash = chain_tip.prev_hash();
        let nbits = CompactTarget::from_consensus(chain_tip.nbits());

        // validate when version rolling is not allowed
        if !job.version_rolling_allowed() {
            // If version rolling is not allowed, ensure bits 13-28 are 0
            // This is done by checking if the version & 0x1fffe000 == 0
            // ref: https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki
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
        let hash_as_target: Target = raw_hash.into();

        // check if the share hash is a valid block
        let network_target = BitcoinTarget::from_compact(nbits);
        if network_target.is_met_by(hash) {
            // last_share_sequence_number = share.sequence_number;
            // shares_accepted += 1;
            // share_work_sum += target_to_difficulty(target);
            self.share_accounting.update_share_accounting(
                target_to_difficulty(hash_as_target),
                share.sequence_number,
                hash.to_raw_hash(),
            );

            let template_id = match job.get_template() {
                Some(template) => Some(template.template_id),
                None => None,
            };
            let mut coinbase = vec![];
            coinbase.extend(job.get_coinbase_tx_prefix().inner_as_ref());
            coinbase.extend(full_extranonce);
            coinbase.extend(job.get_coinbase_tx_suffix().inner_as_ref());

            return Ok(ShareValidationResult::BlockFound(template_id, coinbase));
        }

        // check if the share hash meets the channel target
        if hash_as_target <= self.target {
            if self.share_accounting.is_share_seen(hash.to_raw_hash()) {
                return Err(ShareValidationError::DuplicateShare);
            }

            self.share_accounting.update_share_accounting(
                target_to_difficulty(hash_as_target.clone()),
                share.sequence_number,
                hash.to_raw_hash(),
            );

            // update the best target
            self.share_accounting.update_best_target(hash_as_target);

            let last_sequence_number = self.share_accounting.get_last_share_sequence_number();
            let new_submits_accepted_count = self.share_accounting.get_shares_accepted();
            let new_shares_sum = self.share_accounting.get_share_work_sum() as u64;

            // if sequence number is a multiple of share_batch_size
            // it's time to send a SubmitShares.Success
            if self.share_accounting.should_acknowledge() {
                return Ok(ShareValidationResult::ValidWithAcknowledgement(
                    last_sequence_number,
                    new_submits_accepted_count,
                    new_shares_sum,
                ));
            } else {
                return Ok(ShareValidationResult::Valid);
            }
        } else {
            Err(ShareValidationError::DoesNotMeetTarget)
        }
    }
}
