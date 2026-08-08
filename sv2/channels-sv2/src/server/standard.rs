//! Sv2 Standard Channel - Mining Server Abstraction.
//!
//! This module provides the [`StandardChannel`] struct, which models and manages the state of a
//! Stratum V2 (SV2) standard channel as maintained on a mining server.
//!
//! ## Responsibilities
//!
//! `StandardChannel` is responsible for managing all the state associated with an SV2 standard
//! channel, including:
//!
//! - **Channel Parameters**: Unique `channel_id`, `user_identity`, `extranonce_prefix`, maximum
//!   target, nominal hashrate, and other properties negotiated at channel opening.
//! - **Target Difficulty**: Maintains both the requested maximum target and the current working
//!   target for the channel, recalculated as hashrate or share rate changes.
//! - **Job Lifecycle Management**: Manages active, future, past, and stale jobs, including
//!   activation on new chain tips and template updates.
//! - **Share Validation and Accounting**: Validates submitted shares, updates share accounting
//!   state, detects duplicates, and manages batch acknowledgements for SV2 `SubmitShares.Success`
//!   responses.
//! - **Chain Tip Management**: Tracks the latest known chain tip (block height, previous hash,
//!   timestamp, and target) for constructing headers and validating shares.
//!
//! ## Usage
//!
//! Intended for use by pool servers or SV2-compliant job declaration clients (JDC), not by mining
//! devices or proxies. Encapsulates logic for handling SV2 messages such as `NewTemplate`,
//! `SetNewPrevHash`, and `SubmitSharesStandard`.
//!
//! ## Notes
//!
//! - Only one active job is allowed at a time. When a chain tip updates, jobs from the previous tip
//!   become stale and are tracked accordingly.
//! - Share batch acknowledgment logic is tied to the configured batch size.
//! - Extranonce prefix updates must be consistent with SV2 protocol constraints.
//! - Job lifecycle and share accounting are managed on a per-channel basis.
use crate::{
    chain_tip::ChainTip,
    extranonce_manager::{AllocatedExtranoncePrefix, ExtranoncePrefix},
    server::{
        error::StandardChannelError,
        jobs::{
            extended::ExtendedJob, factory::JobFactory, job_store::JobStore, standard::StandardJob,
        },
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    target::{bytes_to_hex, hash_rate_to_target, u256_to_block_hash},
    MAX_EXTRANONCE_LEN, VERSION_ROLLING_MASK,
};
use bitcoin::{
    absolute::LockTime,
    blockdata::{
        block::{Header, Version},
        witness::Witness,
    },
    consensus::Encodable,
    hashes::sha256d::Hash,
    transaction::{OutPoint, Transaction, TxIn, TxOut, Version as TxVersion},
    CompactTarget, Sequence, Target,
};
use mining_sv2::{
    SubmitSharesStandardOwned, ERROR_CODE_OPEN_MINING_CHANNEL_INVALID_NOMINAL_HASHRATE,
    ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW, ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE,
    ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID,
    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
    ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE, ERROR_CODE_SUBMIT_SHARES_STALE_SHARE,
    ERROR_CODE_UPDATE_CHANNEL_INVALID_NOMINAL_HASHRATE,
};
use std::collections::HashMap;
use template_distribution_sv2::{NewTemplateOwned, SetNewPrevHashOwned as SetNewPrevHash};
use tracing::debug;

/// Abstraction of a Sv2 Standard Channel.
///
/// It keeps track of:
/// - the channel's unique `channel_id`
/// - the channel's `user_identity`
/// - the channel's unique `extranonce_prefix`
/// - the channel's requested max target (limit established by the client)
/// - the channel's current target
/// - the channel's mapping between `job_id` and target
/// - the channel's nominal hashrate
/// - whether the channel's nominal hashrate is treated as stable
/// - the channel's internal job store
/// - the channel's share accounting state
/// - the channel's expected share per minute
/// - the channel's job factory
/// - the channel's chain tip
#[derive(Debug)]
pub struct StandardChannel {
    pub channel_id: u32,
    user_identity: String,
    extranonce_prefix: ExtranoncePrefix,
    requested_max_target: Target,
    target: Target,
    job_id_to_target: HashMap<u32, Target>,
    nominal_hashrate: f32,
    stable_hashrate: bool,
    share_accounting: ShareAccounting,
    expected_share_per_minute: f32,
    job_store: JobStore<StandardJob>,
    job_factory: JobFactory,
    chain_tip: Option<ChainTip>,
}

impl StandardChannel {
    /// Constructor of `StandardChannel` for a Sv2 Pool Server.
    /// Not meant for usage on a Sv2 Job Declaration Client.
    ///
    /// Initializes the standard channel state with the provided parameters, including channel
    /// identifiers, difficulty targets, share accounting, and job management.
    /// Returns an error if target/difficulty parameters are invalid or extranonce prefix
    /// requirements are not met.
    ///
    /// For non-JD jobs, `pool_tag_string` is added to the coinbase scriptSig as
    /// `Sv2/pool_tag_string//`.
    ///
    /// Returns [`StandardChannelError::ScriptSigSizeTooLarge`] if the tags, the delimiters, the
    /// extranonce prefix and a worst-case coinbase prefix do not fit within the coinbase
    /// `scriptSig` budget, see [`JobFactory::fits_script_sig_budget`].
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_pool(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: AllocatedExtranoncePrefix,
        requested_max_target: Target,
        nominal_hashrate: f32,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        pool_tag_string: String,
    ) -> Result<Self, StandardChannelError> {
        Self::new(
            channel_id,
            user_identity,
            extranonce_prefix.into(),
            requested_max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            Some(pool_tag_string),
            None,
        )
    }

    /// Constructor of `StandardChannel` for a Sv2 Job Declaration Client.
    /// Not meant for usage on a Sv2 Pool Server.
    ///
    /// Initializes the extended channel state with the provided parameters, including channel
    /// identifiers, difficulty targets, share accounting, and job management.
    /// Returns an error if target/difficulty parameters are invalid or extranonce prefix
    /// requirements are not met.
    ///
    /// The `pool_tag_string` and `miner_tag_string` are added to the coinbase scriptSig as
    /// `Sv2/pool_tag_string/miner_tag_string/`.
    ///
    /// Returns [`StandardChannelError::ScriptSigSizeTooLarge`] if the tags, the delimiters, the
    /// extranonce prefix and a worst-case coinbase prefix do not fit within the coinbase
    /// `scriptSig` budget, see [`JobFactory::fits_script_sig_budget`].
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_job_declaration_client(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: AllocatedExtranoncePrefix,
        requested_max_target: Target,
        nominal_hashrate: f32,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        pool_tag_string: Option<String>,
        miner_tag_string: String,
    ) -> Result<Self, StandardChannelError> {
        Self::new(
            channel_id,
            user_identity,
            extranonce_prefix.into(),
            requested_max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            pool_tag_string,
            Some(miner_tag_string),
        )
    }

    // private constructor
    #[allow(clippy::too_many_arguments)]
    fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: ExtranoncePrefix,
        requested_max_target: Target,
        nominal_hashrate: f32,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        pool_tag_string: Option<String>,
        miner_tag_string: Option<String>,
    ) -> Result<Self, StandardChannelError> {
        let calculated_target =
            match hash_rate_to_target(nominal_hashrate.into(), expected_share_per_minute.into()) {
                Ok(target_u256) => target_u256,
                Err(_) => {
                    return Err(StandardChannelError::OpenChannelInvalidNominalHashrate(
                        ERROR_CODE_OPEN_MINING_CHANNEL_INVALID_NOMINAL_HASHRATE,
                    ));
                }
            };

        // Clamp to max_target rather than error. The client declared max_target as
        // an acceptable difficulty floor, so using it when the initial target would
        // otherwise exceed it is always valid.
        let target = calculated_target.min(requested_max_target);

        if extranonce_prefix.len() > MAX_EXTRANONCE_LEN as usize {
            return Err(StandardChannelError::ExtranoncePrefixTooLarge);
        }

        let job_factory = JobFactory::new(true, pool_tag_string, miner_tag_string);

        // conservative check against the spec's worst-case `NewTemplate::coinbase_prefix`.
        // the exact size is re-checked against each actual template in `JobFactory::coinbase`.
        // standard channels have no rollable extranonce, so the prefix is the full extranonce
        if !job_factory.fits_script_sig_budget(extranonce_prefix.len()) {
            return Err(StandardChannelError::ScriptSigSizeTooLarge);
        }

        Ok(Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            requested_max_target,
            target,
            job_id_to_target: HashMap::new(),
            nominal_hashrate,
            stable_hashrate: false,
            share_accounting: ShareAccounting::new(share_batch_size),
            expected_share_per_minute,
            job_store: JobStore::new(),
            job_factory,
            chain_tip: None,
        })
    }

    /// Returns the unique channel ID for this channel.
    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns the user identity string for this channel.
    pub fn get_user_identity(&self) -> &str {
        &self.user_identity
    }

    /// Returns the extranonce prefix bytes.
    pub fn get_extranonce_prefix(&self) -> &[u8] {
        self.extranonce_prefix.as_bytes()
    }

    /// Sets a new extranonce prefix for this channel.
    ///
    /// Jobs created before the update keep the prefix they were created under, and share
    /// validation is performed accordingly.
    ///
    /// Because of that, the previous prefix (and therefore its slot in the
    /// [`ExtranonceAllocator`](crate::extranonce_manager::ExtranonceAllocator) that minted it) is
    /// not released here: it is handed over to the channel's job store, which only drops it once
    /// every job created under it has become stale. This prevents the allocator from handing the
    /// same extranonce space to another live channel while those jobs still validate shares.
    ///
    /// Returns an error if the new prefix is too large, or if it would push the assembled coinbase
    /// `scriptSig` past its budget (see [`JobFactory::fits_script_sig_budget`]). The channel is
    /// left unchanged in both error cases.
    pub fn set_extranonce_prefix(
        &mut self,
        extranonce_prefix: AllocatedExtranoncePrefix,
    ) -> Result<(), StandardChannelError> {
        if extranonce_prefix.len() > MAX_EXTRANONCE_LEN as usize {
            return Err(StandardChannelError::ExtranoncePrefixTooLarge);
        }

        // re-run the constructor's invariant: a prefix that is individually valid can still push
        // the assembled scriptSig past the consensus cap
        if !self
            .job_factory
            .fits_script_sig_budget(extranonce_prefix.len())
        {
            return Err(StandardChannelError::ScriptSigSizeTooLarge);
        }

        let retired_extranonce_prefix =
            std::mem::replace(&mut self.extranonce_prefix, extranonce_prefix.into());
        self.job_store
            .retire_extranonce_prefix(retired_extranonce_prefix);

        Ok(())
    }

    /// Updates the current target for this channel.
    ///
    /// Please note that this will NOT update the target associated with jobs that were already created.
    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    /// Updates the nominal hashrate for this channel.
    pub fn set_nominal_hashrate(&mut self, nominal_hashrate: f32) {
        self.nominal_hashrate = nominal_hashrate;
    }

    /// Sets whether this channel's nominal hashrate should be treated as stable.
    pub fn set_stable_hashrate(&mut self, stable_hashrate: bool) {
        self.stable_hashrate = stable_hashrate;
    }

    /// Returns whether this channel's nominal hashrate is treated as stable.
    pub fn get_stable_hashrate(&self) -> bool {
        self.stable_hashrate
    }

    /// Returns the requested maximum target for this channel.
    pub fn get_requested_max_target(&self) -> &Target {
        &self.requested_max_target
    }

    /// Returns the current target for this channel.
    ///
    /// Please note that this is the current target for the channel. Jobs created before the current target are associated with previously set targets, for which shares will be validated against.
    pub fn get_target(&self) -> &Target {
        &self.target
    }

    /// Returns the nominal hashrate for this channel.
    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    /// Updates channel configuration with a new nominal hashrate.
    ///
    /// Recomputes target difficulty and updates channel state.
    ///
    /// If the recomputed target is easier than the effective `requested_max_target`,
    /// the target is clamped to `requested_max_target`.
    ///
    /// Returns [`StandardChannelError::UpdateChannelInvalidNominalHashrate`] when
    /// `nominal_hashrate` cannot be converted into a valid target.
    ///
    /// This can be used in two scenarios:
    /// - Client sent `UpdateChannel` message, which contains a `requested_max_target` parameter
    ///   that's also used as input.
    /// - vardiff algorithm estimated a new nominal hashrate, in which case `requested_max_target`
    ///   is `None` and we use the value from the channel state (that was set either during channel
    ///   opening or some previous `UpdateChannel` message).
    pub fn update_channel(
        &mut self,
        nominal_hashrate: f32,
        requested_max_target: Option<Target>,
    ) -> Result<(), StandardChannelError> {
        let target = match hash_rate_to_target(
            nominal_hashrate.into(),
            self.expected_share_per_minute.into(),
        ) {
            Ok(target) => target,
            Err(_) => {
                return Err(StandardChannelError::UpdateChannelInvalidNominalHashrate(
                    ERROR_CODE_UPDATE_CHANNEL_INVALID_NOMINAL_HASHRATE,
                ));
            }
        };

        let requested_max_target = match requested_max_target {
            Some(ref requested_max_target) => *requested_max_target,
            None => self.requested_max_target,
        };

        // debug hex of target_u256 and max_target
        // just like in share validation
        // to big-endian for display
        let target_bytes = target.to_be_bytes();
        let max_target_bytes = requested_max_target.to_be_bytes();

        // Get the old target for comparison on the debug log
        // Not really needed for the actual method functionality
        // But it's useful to have for debugging purposes
        let old_target = self.target;
        let old_target_bytes = old_target.to_be_bytes();

        debug!(
            "updating channel target \nold target:\t{}\nnew target:\t{}\nmax_target:\t{}",
            bytes_to_hex(&old_target_bytes),
            bytes_to_hex(&target_bytes),
            bytes_to_hex(&max_target_bytes)
        );

        // Clamp to max_target rather than error. The client declared max_target as
        // an acceptable difficulty floor, so using it when vardiff would otherwise
        // exceed it is always valid.
        let new_target = target.min(requested_max_target);

        self.target = new_target;
        self.nominal_hashrate = nominal_hashrate;
        self.requested_max_target = requested_max_target;
        Ok(())
    }

    /// Returns the currently active job, if any.
    pub fn get_active_job(&self) -> Option<&StandardJob> {
        self.job_store.get_active_job()
    }
    /// Returns the job ID for a future job from a template ID, if any.
    pub fn get_future_job_id_from_template_id(&self, template_id: u64) -> Option<u32> {
        self.job_store
            .get_future_job_id_from_template_id(template_id)
    }

    /// Returns a reference to a future job from its job ID, if any.
    pub fn get_future_job(&self, job_id: u32) -> Option<&StandardJob> {
        self.job_store.get_future_job(job_id)
    }

    /// Returns a reference to a past job from its job ID, if any.
    pub fn get_past_job(&self, job_id: u32) -> Option<&StandardJob> {
        self.job_store.get_past_job(job_id)
    }

    /// Returns a reference to a stale job from its job ID, if any.
    pub fn get_stale_job(&self, job_id: u32) -> Option<&StandardJob> {
        self.job_store.get_stale_job(job_id)
    }

    /// Returns the expected number of shares per minute for this channel.
    pub fn get_shares_per_minute(&self) -> f32 {
        self.expected_share_per_minute
    }

    /// Returns the current chain tip, if set.
    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Only for testing purposes, not meant to be used in real apps.
    #[cfg(test)]
    fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    /// Returns a reference to the share accounting state for this channel.
    pub fn get_share_accounting(&self) -> &ShareAccounting {
        &self.share_accounting
    }

    /// Updates the channel state with a new job.
    ///
    /// If the template is a future template, the chain tip is not used.
    /// If the template is not a future template, the chain tip must be set.
    ///
    /// Only meant for usage on a Sv2 Pool Server or a Sv2 Job Declaration Client,
    /// but not on mining clients such as Mining Devices or Proxies.
    ///
    /// Only meant to be used in case we want to broadcast standard jobs.
    /// In case we want to broadcast extended jobs via group channel, use `on_group_channel_job`
    /// instead.
    ///
    /// Returns [`StandardChannelError::JobFactoryError`] wrapping
    /// [`JobFactoryError::ScriptSigSizeTooLarge`](crate::server::jobs::error::JobFactoryError::ScriptSigSizeTooLarge)
    /// if the template's `coinbase_prefix` pushes the assembled coinbase `scriptSig` past
    /// its budget. The constructor can only check against the spec's worst-case prefix (see
    /// [`JobFactory::fits_script_sig_budget`]), so this is where an out-of-spec Template Provider
    /// is caught.
    pub fn on_new_template(
        &mut self,
        template: NewTemplateOwned,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<(), StandardChannelError> {
        match template.future_template {
            true => {
                let new_job = self
                    .job_factory
                    .new_standard_job(
                        self.channel_id,
                        None,
                        self.extranonce_prefix.as_bytes().to_vec(),
                        template.clone(),
                        coinbase_reward_outputs,
                    )
                    .map_err(StandardChannelError::JobFactoryError)?;
                self.job_store.add_future_job(template.template_id, new_job);
            }
            false => {
                match self.chain_tip.clone() {
                    // we can only create non-future jobs if we have a chain tip
                    None => return Err(StandardChannelError::ChainTipNotSet),
                    Some(chain_tip) => {
                        let new_job = self
                            .job_factory
                            .new_standard_job(
                                self.channel_id,
                                Some(chain_tip),
                                self.extranonce_prefix.as_bytes().to_vec(),
                                template.clone(),
                                coinbase_reward_outputs,
                            )
                            .map_err(StandardChannelError::JobFactoryError)?;

                        // associate the new active job with the current target
                        self.job_id_to_target
                            .insert(new_job.get_job_id(), self.target);

                        // add the new active job to the job store
                        self.job_store.add_active_job(new_job);
                    }
                }
            }
        }

        Ok(())
    }

    /// Used as an alternative to `on_new_template` when an extended job is meant to be broadcast
    /// to the group channel, instead of multiple standard jobs to diffferent standard channels.
    ///
    /// We use this method to update the channel state, so it can validate share from the job that
    /// was broadcasted to the group channel.
    pub fn on_group_channel_job(
        &mut self,
        extended_job: ExtendedJob,
    ) -> Result<(), StandardChannelError> {
        let standard_job = extended_job
            .into_standard_job(self.channel_id, self.extranonce_prefix.as_bytes().to_vec())
            .map_err(|_| StandardChannelError::FailedToConvertToStandardJob)?;

        match standard_job.is_future() {
            true => {
                self.job_store
                    .add_future_job(standard_job.get_template().template_id, standard_job);
            }
            false => {
                // associate the new active job with the current target
                self.job_id_to_target
                    .insert(standard_job.get_job_id(), self.target);

                // add the new active job to the job store
                self.job_store.add_active_job(standard_job);
            }
        }

        Ok(())
    }

    /// Updates the channel state with a new `SetNewPrevHash` message.
    ///
    /// If there are no future jobs, returns an error.
    /// If there are future jobs, the active job is set to the job with the given `template_id`.
    ///
    /// All past jobs are cleared.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHash,
    ) -> Result<(), StandardChannelError> {
        match self.job_store.has_future_jobs() {
            false => {
                return Err(StandardChannelError::TemplateIdNotFound);
            }
            // try to activate the future job, and also mark past jobs as stale
            true => {
                if !self.job_store.activate_future_job(
                    set_new_prev_hash.template_id,
                    set_new_prev_hash.header_timestamp,
                ) {
                    return Err(StandardChannelError::TemplateIdNotFound);
                }

                // clear the job id to target mapping only after a successful activation,
                // so that an early-return error path does not corrupt channel state.
                self.job_id_to_target.clear();

                // associate the new active job with the current target
                let job_id = self
                    .job_store
                    .get_active_job()
                    .expect("active job must exist")
                    .get_job_id();
                self.job_id_to_target.insert(job_id, self.target);
            }
        }

        // clear seen shares, as shares for past chain tip will be rejected as stale
        self.share_accounting.flush_seen_shares();

        // update the chain tip
        self.chain_tip = Some(set_new_prev_hash.into());

        Ok(())
    }

    /// Validates a submitted share and updates accounting state.
    ///
    /// Returns the result of share validation, including block found, valid share, duplicate, or
    /// error if the share is stale, does not meet target, or has ntime below the chain tip's
    /// `min_ntime`.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesStandardOwned,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        let job_id = share.job_id;

        // check if job_id is active job
        let is_active_job = self
            .job_store
            .get_active_job()
            .is_some_and(|job| job.get_job_id() == job_id);

        // check if job_id is past job
        let is_past_job = self.job_store.get_past_job(job_id).is_some();

        // check if job_id is stale job
        let is_stale_job = self.job_store.get_stale_job(job_id).is_some();

        if is_stale_job {
            self.share_accounting
                .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_STALE_SHARE);
            return Err(ShareValidationError::Stale(
                ERROR_CODE_SUBMIT_SHARES_STALE_SHARE,
            ));
        }

        // if job_id is not active, past or stale, return error
        if !is_active_job && !is_past_job && !is_stale_job {
            self.share_accounting
                .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID);
            return Err(ShareValidationError::InvalidJobId(
                ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID,
            ));
        }

        let job = if is_active_job {
            self.job_store
                .get_active_job()
                .expect("active job must exist")
        } else if is_past_job {
            self.job_store
                .get_past_job(job_id)
                .expect("past job must exist")
        } else {
            self.job_store
                .get_stale_job(job_id)
                .expect("stale job must exist")
        };

        let Some(job_target) = self.job_id_to_target.get(&job_id) else {
            self.share_accounting
                .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID);
            return Err(ShareValidationError::InvalidJobId(
                ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID,
            ));
        };

        let merkle_root = job.get_merkle_root().to_array();

        let chain_tip = self
            .chain_tip
            .as_ref()
            .ok_or(ShareValidationError::NoChainTip)?;

        let prev_hash = chain_tip.prev_hash();
        let nbits = CompactTarget::from_consensus(chain_tip.nbits());

        if share.ntime < chain_tip.min_ntime() {
            self.share_accounting
                .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE);
            return Err(ShareValidationError::Invalid(
                ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE,
            ));
        }

        // Only the non-rollable version bits are compared: `!VERSION_ROLLING_MASK` zeroes
        // the BIP323 general-purpose bits the miner may change, so any remaining difference
        // from the job's advertised version means an unauthorized change. Standard channels
        // always allow version rolling within the mask.
        if (share.version & !VERSION_ROLLING_MASK) != (job.get_version() & !VERSION_ROLLING_MASK) {
            self.share_accounting.increment_rejected_shares(
                ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
            );
            return Err(ShareValidationError::Invalid(
                ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
            ));
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
        let share_hash = header.block_hash();
        let share_raw_hash: [u8; 32] = *share_hash.to_raw_hash().as_ref();
        let share_hash_target = Target::from_le_bytes(share_raw_hash);
        let share_hash_as_diff = share_hash_target.difficulty_float();
        let network_target = Target::from_compact(nbits);

        // print hash_as_target and self.target as human readable hex
        let share_hash_target_bytes = share_hash_target.to_be_bytes();
        let job_target_bytes = job_target.to_be_bytes();

        debug!(
            "share validation \nshare:\t\t{}\njob target:\t{}\nnetwork target:\t{}",
            bytes_to_hex(&share_hash_target_bytes),
            bytes_to_hex(&job_target_bytes),
            format!("{:x}", network_target)
        );

        // check if a block was found
        if network_target.is_met_by(share_hash) {
            if self
                .share_accounting
                .is_share_seen(share_hash.to_raw_hash())
            {
                self.share_accounting
                    .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE);
                return Err(ShareValidationError::DuplicateShare(
                    ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE,
                ));
            }
            self.share_accounting.update_share_accounting(
                job_target.difficulty_float(),
                share.sequence_number,
                share_hash.to_raw_hash(),
            );
            self.share_accounting.increment_blocks_found();
            self.share_accounting.mark_batch_acknowledged();

            let op_pushbytes_pool_miner_tag = self
                .job_factory
                .op_pushbytes_pool_miner_tag()
                .map_err(|_| ShareValidationError::InvalidCoinbase)?;

            let mut script_sig = job.get_template().coinbase_prefix.to_owned_bytes();
            script_sig.extend(op_pushbytes_pool_miner_tag);
            // the opcode must describe the job's extranonce prefix, not the channel's current one:
            // `set_extranonce_prefix` may have rotated to a different length since this job was
            // created, and the job's `merkle_root` is committed to the job's prefix
            script_sig.push(job.get_extranonce_prefix().len() as u8); // OP_PUSHBYTES_X (for the extranonce)
            script_sig.extend(job.get_extranonce_prefix());

            let tx_in = TxIn {
                previous_output: OutPoint::null(),
                script_sig: script_sig.into(),
                sequence: Sequence(job.get_template().coinbase_tx_input_sequence),
                witness: Witness::from(vec![vec![0; 32]]),
            };

            let coinbase = Transaction {
                version: TxVersion::non_standard(job.get_template().coinbase_tx_version as i32),
                lock_time: LockTime::from_consensus(job.get_template().coinbase_tx_locktime),
                input: vec![tx_in],
                output: job.get_coinbase_outputs().to_vec(),
            };
            let mut serialized_coinbase = Vec::new();
            coinbase
                .consensus_encode(&mut serialized_coinbase)
                .map_err(|_| ShareValidationError::InvalidCoinbase)?;

            return Ok(ShareValidationResult::BlockFound(
                share_hash.to_raw_hash(),
                Some(job.get_template().template_id),
                serialized_coinbase,
            ));
        }

        // check if the share hash meets the job target
        if share_hash_target <= *job_target {
            if self
                .share_accounting
                .is_share_seen(share_hash.to_raw_hash())
            {
                self.share_accounting
                    .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE);
                return Err(ShareValidationError::DuplicateShare(
                    ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE,
                ));
            }

            self.share_accounting.update_share_accounting(
                job_target.difficulty_float(),
                share.sequence_number,
                share_hash.to_raw_hash(),
            );

            // update the best diff
            self.share_accounting.update_best_diff(share_hash_as_diff);

            Ok(ShareValidationResult::Valid(share_hash.to_raw_hash()))
        } else {
            self.share_accounting
                .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW);
            Err(ShareValidationError::DoesNotMeetTarget(
                ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        chain_tip::ChainTip,
        extranonce_manager::{
            AllocatedExtranoncePrefix, ExtranonceAllocator, ExtranonceAllocatorError,
            ExtranoncePrefix, ExtranoncePrefixError,
        },
        server::{
            error::StandardChannelError,
            jobs::factory::{MAX_COINBASE_PREFIX_SIZE, MAX_SCRIPT_SIG_SIZE},
            share_accounting::{ShareValidationError, ShareValidationResult},
            standard::StandardChannel,
        },
        MAX_EXTRANONCE_LEN,
    };
    use binary_sv2::Sv2OptionOwned as Sv2Option;
    use bitcoin::{
        consensus::deserialize, transaction::TxOut, Amount, ScriptBuf, Target, Transaction,
    };
    use mining_sv2::{
        NewMiningJobOwned as NewMiningJob, SubmitSharesStandardOwned,
        ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW,
        ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
    };
    use std::convert::TryInto;
    use template_distribution_sv2::{
        NewTemplateOwned as NewTemplate, SetNewPrevHashOwned as SetNewPrevHashTdp,
    };

    const SATS_AVAILABLE_IN_TEMPLATE: u64 = 5000000000;

    #[test]
    fn test_future_job_activation_flow() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation
        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 10.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        assert!(!standard_channel.job_store.has_future_jobs());

        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let expected_future_standard_job = NewMiningJob {
            channel_id: standard_channel_id,
            job_id: 1,
            merkle_root: [
                244, 175, 251, 103, 170, 111, 89, 111, 245, 253, 167, 99, 126, 231, 170, 153, 174,
                69, 251, 243, 5, 119, 145, 76, 19, 107, 215, 155, 166, 36, 228, 210,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(None),
        };

        let future_standard_job_from_channel = standard_channel.get_future_job(1).unwrap().clone();
        assert_eq!(
            future_standard_job_from_channel.get_job_message(),
            &expected_future_standard_job
        );

        let ntime = 1747092633;
        let set_new_prev_hash = SetNewPrevHashTdp {
            template_id: template.template_id,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            header_timestamp: ntime,
            n_bits: 503543726,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                174, 119, 3, 0, 0,
            ]
            .into(),
        };

        standard_channel
            .on_set_new_prev_hash(set_new_prev_hash)
            .unwrap();
        let mut previously_future_job = future_standard_job_from_channel.clone();
        previously_future_job.activate(ntime);

        let activated_job = standard_channel.get_active_job().unwrap();

        // assert that the activated job is the same as the previously future job
        assert_eq!(
            activated_job.get_job_message(),
            previously_future_job.get_job_message()
        );
    }

    #[test]
    fn test_non_future_job_creation_flow() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 10.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        let ntime = 1747092633;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        let nbits = 503543726;

        let chain_tip = ChainTip::new(prev_hash, nbits, ntime);
        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let expected_active_standard_job = NewMiningJob {
            channel_id: standard_channel_id,
            job_id: 1,
            merkle_root: [
                244, 175, 251, 103, 170, 111, 89, 111, 245, 253, 167, 99, 126, 231, 170, 153, 174,
                69, 251, 243, 5, 119, 145, 76, 19, 107, 215, 155, 166, 36, 228, 210,
            ]
            .into(),
            version: 536870912,
            min_ntime: Sv2Option::new(Some(ntime)),
        };

        let active_standard_job_from_channel = standard_channel.get_active_job().unwrap().clone();

        assert_eq!(
            active_standard_job_from_channel.get_job_message(),
            &expected_active_standard_job
        );
    }

    #[test]
    fn test_share_validation_block_found() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        // channel target: 04325c53ef368eb04325c53ef368eb04325c53ef368eb04325c53ef368eb0431
        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let ntime = 1745596910;
        let prev_hash = [
            251, 175, 106, 40, 35, 87, 122, 90, 58, 51, 78, 32, 202, 236, 228, 36, 154, 174, 206,
            144, 147, 195, 21, 224, 195, 103, 214, 189, 51, 190, 24, 98,
        ]
        .into();
        let n_bits = 545259519;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);

        // prepare standard channel with non-future job
        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let active_standard_job = standard_channel.get_active_job().unwrap();

        // this share has hash 3c34f63de61283c907b68e3127146d7d11f1fb14e50020a8317a292d11e2dab6
        // which satisfied the network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        let share_valid_block = SubmitSharesStandardOwned {
            channel_id: standard_channel_id,
            sequence_number: 0,
            job_id: active_standard_job.get_job_id(),
            nonce: 0,
            ntime: 1745596932,
            version: 536870912,
        };

        let res = standard_channel.validate_share(share_valid_block.clone());

        assert!(matches!(
            res,
            Ok(ShareValidationResult::BlockFound(_, _, _))
        ));
        assert_eq!(
            standard_channel.get_share_accounting().get_blocks_found(),
            1
        );

        // re-submitting the same valid block must be rejected as duplicate
        let res = standard_channel.validate_share(share_valid_block);
        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DuplicateShare(_)
        ));
        assert_eq!(
            standard_channel.get_share_accounting().get_blocks_found(),
            1
        );
    }

    #[test]
    fn test_share_validation_ntime_below_min_ntime() {
        // Regression test: a share with ntime < min_ntime must be rejected.
        // Reuses the block-found test vectors but sets min_ntime one second
        // above the share's ntime.
        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0];
        script_bytes.push(20);
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        let prev_hash = [
            251, 175, 106, 40, 35, 87, 122, 90, 58, 51, 78, 32, 202, 236, 228, 36, 154, 174, 206,
            144, 147, 195, 21, 224, 195, 103, 214, 189, 51, 190, 24, 98,
        ]
        .into();
        let n_bits = 545259519;
        // set min_ntime one second above the share's ntime (1745596932 + 1)
        let chain_tip = ChainTip::new(prev_hash, n_bits, 1745596933);

        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let active_standard_job = standard_channel.get_active_job().unwrap();

        let share_below_min_ntime = SubmitSharesStandardOwned {
            channel_id: standard_channel_id,
            sequence_number: 0,
            job_id: active_standard_job.get_job_id(),
            nonce: 0,
            ntime: 1745596932,
            version: 536870912,
        };

        let res = standard_channel.validate_share(share_below_min_ntime);

        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));
        assert_eq!(
            standard_channel.get_share_accounting().get_blocks_found(),
            0
        );
    }

    #[test]
    fn test_share_validation_block_found_after_extranonce_prefix_rotation() {
        // note:
        // same test vectors as `test_share_validation_block_found`, plus an extranonce prefix
        // rotation (to a prefix of a different length) in between job creation and share
        // submission.
        //
        // the job (and therefore its merkle root, and therefore the winning nonce) is committed to
        // the prefix that was current at `on_new_template` time, so the coinbase reconstructed on
        // BlockFound must also be committed to that same prefix.

        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        // channel target: 04325c53ef368eb04325c53ef368eb04325c53ef368eb04325c53ef368eb0431
        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
        let ntime = 1745596910;
        let prev_hash = [
            251, 175, 106, 40, 35, 87, 122, 90, 58, 51, 78, 32, 202, 236, 228, 36, 154, 174, 206,
            144, 147, 195, 21, 224, 195, 103, 214, 189, 51, 190, 24, 98,
        ]
        .into();
        let n_bits = 545259519;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);

        // prepare standard channel with non-future job
        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // capture what the job is committed to, before rotating the channel's prefix
        let active_standard_job = standard_channel.get_active_job().unwrap();
        let job_id = active_standard_job.get_job_id();
        let job_merkle_root = active_standard_job.get_merkle_root().to_array();
        let job_extranonce_prefix = active_standard_job.get_extranonce_prefix().to_vec();
        assert_eq!(job_extranonce_prefix, extranonce_prefix);

        // rotate the channel to a prefix of a different length
        let rotated_extranonce_prefix = vec![0xab; 16];
        standard_channel
            .set_extranonce_prefix(
                AllocatedExtranoncePrefix::for_test(rotated_extranonce_prefix).unwrap(),
            )
            .unwrap();

        // this share has hash 3c34f63de61283c907b68e3127146d7d11f1fb14e50020a8317a292d11e2dab6
        // which satisfied the network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        let share_valid_block = SubmitSharesStandardOwned {
            channel_id: standard_channel_id,
            sequence_number: 0,
            job_id,
            nonce: 0,
            ntime: 1745596932,
            version: 536870912,
        };

        let Ok(ShareValidationResult::BlockFound(_, _, serialized_coinbase)) =
            standard_channel.validate_share(share_valid_block)
        else {
            panic!("expected BlockFound");
        };

        let coinbase: Transaction = deserialize(&serialized_coinbase).unwrap();

        // merkle_path is empty in this template, so the merkle root IS the coinbase txid
        let coinbase_txid: [u8; 32] = *coinbase.compute_txid().as_ref();
        assert_eq!(coinbase_txid, job_merkle_root);

        // the scriptSig must push the job's extranonce prefix, with a matching OP_PUSHBYTES opcode
        let script_sig = coinbase.input[0].script_sig.as_bytes();
        let extranonce_start = script_sig.len() - job_extranonce_prefix.len();
        assert_eq!(
            script_sig[extranonce_start - 1],
            job_extranonce_prefix.len() as u8
        );
        assert_eq!(&script_sig[extranonce_start..], &job_extranonce_prefix[..]);
    }

    #[test]
    fn test_share_validation_does_not_meet_target() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 100.0; // bigger hashrate to get higher difficulty
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        // channel target: 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435
        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let ntime = 1745596910;
        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let n_bits = 453040064;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);

        // prepare standard channel with non-future job
        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let active_standard_job = standard_channel.get_active_job().unwrap();

        // this share has hash a5b65006d89dab9de2b23ececd3b0435f163607f7da1ba2f0bcde62b29e8cd44
        // which does not meet the channel target
        // 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435
        let share_low_diff = SubmitSharesStandardOwned {
            channel_id: standard_channel_id,
            sequence_number: 0,
            job_id: active_standard_job.get_job_id(),
            nonce: 3,
            ntime: 1745596932,
            version: 536870912,
        };

        let res = standard_channel.validate_share(share_low_diff);

        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DoesNotMeetTarget(_)
        ));
        assert_eq!(
            standard_channel
                .get_share_accounting()
                .get_rejected_shares_error_count(ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW),
            1
        );
    }

    #[test]
    fn test_share_validation_valid_share() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1_000.0; // bigger hashrate to get higher difficulty
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        // channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        // network target: 000000000000d7c0000000000000000000000000000000000000000000000000
        let ntime = 1745596910;
        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let n_bits = 453040064;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);

        // prepare standard channel with non-future job
        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // this share has hash 0000545efd44aa70bc49d037df93ba3e51ec5937f72a28f9d3c15d8f17c1194e
        // which does meet the channel target
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205
        // but does not meet network target
        // 000000000000d7c0000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesStandardOwned {
            channel_id: standard_channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 92092,
            ntime: 1745611105,
            version: 536870912,
        };
        let res = standard_channel.validate_share(valid_share);

        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));
    }

    #[test]
    fn test_share_validation_invalid_non_rollable_version_bit() {
        // on standard channels, version rolling is always allowed within the BIP323
        // general-purpose bits mask (0x1FFFFFE0)
        // only the bits inside the mask may differ from the job version
        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        // force an easy target so that a share would be valid if not for the version check
        standard_channel.set_target(max_target);

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        let ntime = 1745596910;
        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let n_bits = 453040064;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);

        // prepare standard channel with non-future job
        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // the job version is 536870912 (0x20000000)
        // this share has version 0, which clears bit 29, outside the BIP323
        // general-purpose bits mask (0x1FFFFFE0)
        // no nonce should be accepted
        for nonce in 0..1024u32 {
            let share = SubmitSharesStandardOwned {
                channel_id: standard_channel_id,
                sequence_number: nonce,
                job_id: 1,
                nonce,
                ntime: 1745611105,
                version: 0,
            };
            let res = standard_channel.validate_share(share);
            let err = res.expect_err("share with non-rollable version bits must be rejected");
            match err {
                ShareValidationError::Invalid(code) => {
                    assert_eq!(
                        code,
                        ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT
                    );
                }
                other => panic!("expected ShareValidationError::Invalid, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_share_validation_rollable_version_bits() {
        // on standard channels, version rolling is always allowed within the BIP323
        // general-purpose bits mask (0x1FFFFFE0)
        // shares that only differ in the bits inside the mask are accepted
        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        // force an easy target so that finding a valid share is trivial
        standard_channel.set_target(max_target);

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        let ntime = 1745596910;
        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let n_bits = 453040064;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);

        // prepare standard channel with non-future job
        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // the job version is 536870912 (0x20000000)
        // this share version only sets bits 5-20, which are inside the BIP323
        // general-purpose bits mask (0x1FFFFFE0)
        let rolled_version = 0x20000000 | 0x1fffe0;

        // this share has hash
        // d4b5578385be26ff3aa10be48dfc907072bc30def0c1a295cf6113b1980c3427
        // which does meet the channel target
        let share = SubmitSharesStandardOwned {
            channel_id: standard_channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745611105,
            version: rolled_version,
        };

        assert!(standard_channel.validate_share(share).is_ok());
    }

    #[test]
    fn test_new_clamps_target_to_max_target() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [0, 0, 0, 1].to_vec();
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let very_small_hashrate = 0.1;
        // less permissive max_target to exercise constructor clamp path
        let not_so_permissive_max_target = Target::from_le_bytes([
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0x00,
        ]);

        let channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            not_so_permissive_max_target,
            very_small_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        assert_eq!(
            channel.get_requested_max_target(),
            &not_so_permissive_max_target
        );
        assert_eq!(channel.get_target(), &not_so_permissive_max_target);
    }

    // standard channels have no rollable extranonce, so a 52 char pool tag places the worst-case
    // scriptSig exactly on the budget:
    // 8 (MAX_COINBASE_PREFIX_SIZE) + 1 + 3 ("Sv2") + 3 + 52 (tag) + 1 + 32 (extranonce prefix)
    // = 100
    const POOL_TAG_AT_SCRIPT_SIG_BUDGET: usize = 52;
    const EXTRANONCE_PREFIX_LEN_AT_SCRIPT_SIG_BUDGET: usize = 32;

    fn new_standard_channel_with_pool_tag(
        pool_tag_len: usize,
        extranonce_prefix_len: usize,
    ) -> Result<StandardChannel, StandardChannelError> {
        StandardChannel::new(
            1,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![0xab; extranonce_prefix_len]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            100,
            1.0,
            Some("x".repeat(pool_tag_len)),
            None,
        )
    }

    #[test]
    fn test_new_rejects_oversized_script_sig() {
        // exactly on the budget
        let channel = new_standard_channel_with_pool_tag(
            POOL_TAG_AT_SCRIPT_SIG_BUDGET,
            EXTRANONCE_PREFIX_LEN_AT_SCRIPT_SIG_BUDGET,
        )
        .unwrap();
        assert_eq!(
            channel.job_factory.script_sig_size(
                MAX_COINBASE_PREFIX_SIZE,
                channel.get_extranonce_prefix().len()
            ),
            MAX_SCRIPT_SIG_SIZE
        );

        // one byte over the budget, via a longer tag
        let channel = new_standard_channel_with_pool_tag(
            POOL_TAG_AT_SCRIPT_SIG_BUDGET + 1,
            EXTRANONCE_PREFIX_LEN_AT_SCRIPT_SIG_BUDGET,
        );
        assert!(matches!(
            channel.unwrap_err(),
            StandardChannelError::ScriptSigSizeTooLarge
        ));
    }

    // a 60 char pool tag makes the budget bind below MAX_EXTRANONCE_LEN, so the setter's scriptSig
    // check is exercised on a prefix that is still individually valid:
    // 8 (MAX_COINBASE_PREFIX_SIZE) + 1 + 3 ("Sv2") + 3 + 60 (tag) + 1 + 24 (extranonce prefix)
    // = 100
    const POOL_TAG_BINDING_BELOW_MAX_EXTRANONCE_LEN: usize = 60;
    const EXTRANONCE_PREFIX_LEN_AT_BINDING_BUDGET: usize = 24;

    #[test]
    fn test_set_extranonce_prefix_rejects_oversized_script_sig() {
        // start well within the budget
        let original_prefix_len = 4;
        let mut channel = new_standard_channel_with_pool_tag(
            POOL_TAG_BINDING_BELOW_MAX_EXTRANONCE_LEN,
            original_prefix_len,
        )
        .unwrap();
        let original_prefix = channel.get_extranonce_prefix().to_vec();

        // growing up to the budget is allowed
        channel
            .set_extranonce_prefix(
                AllocatedExtranoncePrefix::for_test(vec![
                    0xcd;
                    EXTRANONCE_PREFIX_LEN_AT_BINDING_BUDGET
                ])
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            channel.get_extranonce_prefix().len(),
            EXTRANONCE_PREFIX_LEN_AT_BINDING_BUDGET
        );

        // go back to the original prefix, so we can assert the channel is untouched on error
        channel
            .set_extranonce_prefix(
                AllocatedExtranoncePrefix::for_test(original_prefix.clone()).unwrap(),
            )
            .unwrap();

        // a prefix that is individually valid (<= MAX_EXTRANONCE_LEN) but pushes the assembled
        // scriptSig one byte past the budget must be rejected
        let oversized_prefix = vec![0xcd; EXTRANONCE_PREFIX_LEN_AT_BINDING_BUDGET + 1];
        assert!(oversized_prefix.len() <= MAX_EXTRANONCE_LEN as usize);
        let res = channel
            .set_extranonce_prefix(AllocatedExtranoncePrefix::for_test(oversized_prefix).unwrap());
        assert!(matches!(
            res.unwrap_err(),
            StandardChannelError::ScriptSigSizeTooLarge
        ));
        assert_eq!(channel.get_extranonce_prefix(), &original_prefix[..]);
    }

    #[test]
    fn test_update_channel() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let expected_share_per_minute = 1.0;
        let initial_hashrate = 10.0;
        let share_batch_size = 100; // this is the most permissive possible max_target
        let max_target = Target::from_le_bytes([0xff; 32]);

        // Create a channel with initial hashrate
        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            initial_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        // Get the initial target
        let initial_target = channel.get_target().clone();

        // Update the channel with a new hashrate (higher)
        let new_hashrate = 100.0;
        channel
            .update_channel(new_hashrate, Some(max_target))
            .unwrap();

        // Get the new target after update
        let new_target = channel.get_target().clone();

        // The target should be different after updating with a different hashrate
        // old target: 006d0b803685c01b42e00da17006d0b803685c01b42e00da17006d0b803685bf
        // new target: 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435
        assert_ne!(initial_target, new_target);

        // The nominal hashrate should be updated
        assert_eq!(channel.get_nominal_hashrate(), new_hashrate);

        // Test invalid hashrate (negative)
        let result = channel.update_channel(-1.0, Some(max_target));
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(StandardChannelError::UpdateChannelInvalidNominalHashrate(_))
        ));

        // Create a not so permissive max_target so we can test a target that exceeds it
        let not_so_permissive_max_target = Target::from_le_bytes([
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0x00,
        ]);

        // Update with a hashrate that would compute a target exceeding max_target.
        // The channel should clamp to not_so_permissive_max_target instead of erroring.
        // calculated target: 2492492492492492492492492492492492492492492492492492492492492491
        // max target:        00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
        let very_small_hashrate = 0.1;
        let result =
            channel.update_channel(very_small_hashrate, Some(not_so_permissive_max_target));
        assert!(result.is_ok());
        assert_eq!(channel.get_target(), &not_so_permissive_max_target);

        // Test successful update with not_so_permissive_max_target
        // new target: 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205
        // max target: 00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
        let sufficiently_big_hashrate = 1000.0;
        let result = channel.update_channel(
            sufficiently_big_hashrate,
            Some(not_so_permissive_max_target),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_extranonce_prefix() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1_000.0;
        let share_batch_size = 100;
        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, extranonce_prefix.as_slice());

        let new_extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
        ]
        .to_vec();

        channel
            .set_extranonce_prefix(
                AllocatedExtranoncePrefix::for_test(new_extranonce_prefix.clone()).unwrap(),
            )
            .unwrap();
        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, new_extranonce_prefix.as_slice());

        let new_extranonce_prefix_too_long = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 1,
        ]
        .to_vec();
        assert!(matches!(
            ExtranoncePrefix::from_wire(new_extranonce_prefix_too_long),
            Err(ExtranoncePrefixError::ExceedsMaxLength)
        ));
    }

    #[test]
    fn test_set_new_prev_hash_without_future_jobs_preserves_state() {
        // Regression test: when on_set_new_prev_hash is called with no future jobs to
        // activate, it must return an error WITHOUT corrupting channel state. Previously
        // the function cleared job_id_to_target before checking for future jobs, so a
        // caller that treated the error as recoverable would crash on the next share at
        // the `expect("job target must exist")` site.
        let standard_channel_id = 1;
        let user_identity = "user_identity".to_string();

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 100.0;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0];
        script_bytes.push(20);
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        let ntime = 1745596910;
        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let n_bits = 453040064;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);

        standard_channel.set_chain_tip(chain_tip);
        standard_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let active_standard_job = standard_channel.get_active_job().unwrap();
        let active_job_id = active_standard_job.get_job_id();
        assert!(!standard_channel.job_store.has_future_jobs());

        // No future jobs available -> on_set_new_prev_hash must return Err.
        let snph = SetNewPrevHashTdp {
            template_id: 999,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            header_timestamp: ntime + 600,
            n_bits,
            target: [0xff; 32].into(),
        };
        let res = standard_channel.on_set_new_prev_hash(snph);
        assert!(matches!(res, Err(StandardChannelError::TemplateIdNotFound)));

        // Channel state must be preserved: active job still active, target entry intact.
        // A subsequent share submission for the still-active job must NOT panic on a
        // missing job_id_to_target entry. The share itself does not meet target, so we
        // expect DoesNotMeetTarget — the load-bearing assertion is that it returns
        // without panicking.
        let share_low_diff = SubmitSharesStandardOwned {
            channel_id: standard_channel_id,
            sequence_number: 0,
            job_id: active_job_id,
            nonce: 3,
            ntime: 1745596932,
            version: 536870912,
        };
        let res = standard_channel.validate_share(share_low_diff);
        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DoesNotMeetTarget(_)
        ));
    }

    #[test]
    fn test_rotated_extranonce_prefix_slot_not_reused_while_job_live() {
        // Regression test: rotating the channel's extranonce prefix must not return the old
        // prefix's allocator slot to the free pool while jobs created under it can still accept
        // shares. Otherwise the allocator could hand the very same extranonce space to a second
        // live channel, making the same work replayable across both.
        let mut allocator = ExtranonceAllocator::new(vec![], 32, 2).unwrap();

        let prefix_1 = allocator.allocate_standard().unwrap();
        let prefix_2 = allocator.allocate_standard().unwrap();
        assert_ne!(prefix_1.as_bytes(), prefix_2.as_bytes());
        assert_eq!(allocator.allocated_count(), 2);

        let prefix_1_bytes = prefix_1.as_bytes().to_vec();

        let mut standard_channel = StandardChannel::new_for_pool(
            1,
            "user_identity".to_string(),
            prefix_1,
            Target::from_le_bytes([0xff; 32]),
            100.0,
            100,
            1.0,
            String::new(),
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 159, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967294,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 158,
            merkle_path: vec![].try_into().unwrap(),
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0];
        script_bytes.push(20);
        script_bytes.extend_from_slice(&pubkey_hash);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::from(script_bytes),
        }];

        let ntime = 1745596910;
        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let n_bits = 453040064;
        standard_channel.set_chain_tip(ChainTip::new(prev_hash, n_bits, ntime));
        standard_channel
            .on_new_template(template, coinbase_reward_outputs)
            .unwrap();

        // rotate the channel onto the second prefix, while the job above is still live
        standard_channel.set_extranonce_prefix(prefix_2).unwrap();

        // the rotated-out slot is still reserved, so the allocator is still full
        assert_eq!(allocator.allocated_count(), 2);
        assert!(matches!(
            allocator.allocate_standard(),
            Err(ExtranonceAllocatorError::CapacityExhausted)
        ));

        // and the pre-rotation job is still live under the old prefix bytes
        assert_eq!(
            standard_channel
                .get_active_job()
                .unwrap()
                .get_extranonce_prefix(),
            prefix_1_bytes.as_slice()
        );
    }
}
