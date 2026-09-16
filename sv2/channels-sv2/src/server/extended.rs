//! # SV2 Extended Channel - Mining Server Abstraction.
//!
//! This module defines the [`ExtendedChannel`] struct, which provides an abstraction of a SV2
//! extended channel as maintained by a mining pool server.
//!
//! ## Responsibilities
//!
//! `ExtendedChannel` is responsible for managing all the state associated with an SV2 extended
//! channel, including:
//!
//! - **Channel Parameters**: Holds the unique `channel_id`, `user_identity`, `extranonce_prefix`,
//!   and other parameters negotiated during channel opening.
//! - **Target Difficulty**: Manages the target difficulty (`target`) and maximum allowed target
//!   (`requested_max_target`), based on client requests and nominal hashrate.
//! - **Job Lifecycle Management**: Stores jobs received from new templates or custom job messages,
//!   including:
//!   - Future jobs (indexed by `template_id`)
//!   - Active job (currently being mined)
//!   - Past and stale jobs (for share validation over time)
//! - **Share Validation and Accounting**: Validates shares submitted by the miner, updating
//!   internal accounting and detecting duplicates or stale submissions. Determines if a share meets
//!   the channel or network target and responds accordingly.
//! - **Chain Tip Management**: Tracks the latest known chain tip (previous hash, timestamp, and
//!   target) for constructing headers and validating shares.
//! - **Version Rolling**: Honors server configuration on whether version rolling is permitted,
//!   ensuring submitted share versions only differ from the job's advertised version in the
//!   BIP323 general-purpose bits (and match it exactly when version rolling is not permitted).
//!
//! ## Usage
//!
//! This struct is intended for use on the **pool server side** or by SV2-compliant job declaration
//! clients. It encapsulates logic for responding to SV2 messages such as `NewTemplate`,
//! `SetNewPrevHash`, `SetCustomMiningJob`, and `SubmitSharesExtended`.
//!
//! ## Notes
//!
//! - Only one active job is allowed at a time. Jobs from a previous chain tip become stale when a
//!   new chain tip is set.
//! - Share acknowledgment logic is tied to a configured batch size (e.g., every `N` valid shares).
//! - Extranonce validation supports dynamic updates of `extranonce_prefix` but enforces consistency
//!   with previously agreed parameters.

use crate::{
    chain_tip::ChainTip,
    extranonce_manager::{AllocatedExtranoncePrefix, ExtranoncePrefix},
    merkle_root::merkle_root_from_path,
    server::{
        error::ExtendedChannelError,
        jobs::{
            extended::ExtendedJob,
            factory::JobFactory,
            job_store::{JobStore, MAX_PAST_JOBS},
            JobOrigin,
        },
        share_accounting::{ShareAccounting, ShareValidationError, ShareValidationResult},
    },
    target::{bytes_to_hex, hash_rate_to_target, u256_to_block_hash},
    MAX_EXTRANONCE_LEN, MAX_FUTURE_BLOCK_TIME, VERSION_ROLLING_MASK,
};
use bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    transaction::TxOut,
    CompactTarget, Target,
};
use mining_sv2::{
    SetCustomMiningJobOwned, SubmitSharesExtendedOwned,
    ERROR_CODE_OPEN_MINING_CHANNEL_INVALID_NOMINAL_HASHRATE,
    ERROR_CODE_OPEN_MINING_CHANNEL_MAX_TARGET_OUT_OF_RANGE,
    ERROR_CODE_SUBMIT_SHARES_BAD_EXTRANONCE_SIZE, ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW,
    ERROR_CODE_SUBMIT_SHARES_DUPLICATE_SHARE, ERROR_CODE_SUBMIT_SHARES_INVALID_JOB_ID,
    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
    ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE, ERROR_CODE_SUBMIT_SHARES_STALE_SHARE,
    ERROR_CODE_UPDATE_CHANNEL_INVALID_NOMINAL_HASHRATE,
    ERROR_CODE_UPDATE_CHANNEL_MAX_TARGET_OUT_OF_RANGE, ERROR_CODE_VERSION_ROLLING_NOT_ALLOWED,
};
use std::collections::HashMap;
use template_distribution_sv2::{NewTemplateOwned, SetNewPrevHashOwned as SetNewPrevHashTdp};
use tracing::debug;

/// Mining Server abstraction of a Sv2 Extended Channel.
///
/// It keeps track of:
/// - the channel's unique `channel_id`
/// - the channel's `user_identity`
/// - the channel's unique `extranonce_prefix`
/// - the channel's rollable extranonce size
/// - whether the channel allows version rolling; a group job allowing it on a channel that does
///   not is refused (see [`on_group_channel_job`](Self::on_group_channel_job))
/// - the channel's requested max target (limit established by the client)
/// - the channel's current target
/// - the channel's mapping between `job_id` and target
/// - the channel's nominal hashrate
/// - whether the channel's nominal hashrate is treated as stable
/// - the channel's internal job store
/// - the channel's [`JobFactory`]
/// - the channel's [`ShareAccounting`]
/// - the channel's expected share per minute
/// - the channel's [`JobFactory`]
/// - the channel's [`ChainTip`]
#[derive(Debug)]
pub struct ExtendedChannel {
    channel_id: u32,
    user_identity: String,
    extranonce_prefix: ExtranoncePrefix,
    rollable_extranonce_size: u16,
    version_rolling_allowed: bool,
    requested_max_target: Target,
    target: Target,
    job_id_to_target: HashMap<u32, Target>,
    nominal_hashrate: f32,
    stable_hashrate: bool,
    job_store: JobStore<ExtendedJob>,
    job_factory: JobFactory,
    share_accounting: ShareAccounting,
    expected_share_per_minute: f32,
    chain_tip: Option<ChainTip>,
}

impl ExtendedChannel {
    /// Constructor of `ExtendedChannel` for a Sv2 Pool Server.
    /// Not meant for usage on a Sv2 Job Declaration Client.
    ///
    /// Initializes the extended channel state with the provided parameters, including channel
    /// identifiers, difficulty targets, share accounting, and job management.
    /// Returns an error if target/difficulty parameters are invalid or extranonce prefix
    /// requirements are not met. In particular, a zero `max_target` is refused with
    /// [`ExtendedChannelError::OpenChannelInvalidMaxTarget`].
    ///
    /// For non-JD jobs, `pool_tag_string` is added to the coinbase scriptSig as
    /// `Sv2/pool_tag_string//`.
    ///
    /// Returns [`ExtendedChannelError::ScriptSigSizeTooLarge`] if the tags, the delimiters, the
    /// full extranonce and a worst-case coinbase prefix do not fit within the coinbase `scriptSig`
    /// budget, see [`JobFactory::fits_script_sig_budget`].
    ///
    /// `max_past_jobs` caps the past jobs retained under the current chain tip. `None` and
    /// `Some(0)` both select the crate default.
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_pool(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: AllocatedExtranoncePrefix,
        max_target: Target,
        nominal_hashrate: f32,
        version_rolling_allowed: bool,
        rollable_extranonce_size: u16,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        pool_tag_string: String,
        max_past_jobs: Option<usize>,
    ) -> Result<Self, ExtendedChannelError> {
        Self::new(
            channel_id,
            user_identity,
            extranonce_prefix.into(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            Some(pool_tag_string),
            None,
            max_past_jobs,
        )
    }

    /// Constructor of `ExtendedChannel` for a Sv2 Job Declaration Client.
    /// Not meant for usage on a Sv2 Pool Server.
    ///
    /// Initializes the extended channel state with the provided parameters, including channel
    /// identifiers, difficulty targets, share accounting, and job management.
    /// Returns an error if target/difficulty parameters are invalid or extranonce prefix
    /// requirements are not met. In particular, a zero `max_target` is refused with
    /// [`ExtendedChannelError::OpenChannelInvalidMaxTarget`].
    ///
    /// The `pool_tag_string` and `miner_tag_string` are added to the coinbase scriptSig as
    /// `Sv2/pool_tag_string/miner_tag_string/`.
    ///
    /// Returns [`ExtendedChannelError::ScriptSigSizeTooLarge`] if the tags, the delimiters, the
    /// full extranonce and a worst-case coinbase prefix do not fit within the coinbase `scriptSig`
    /// budget, see [`JobFactory::fits_script_sig_budget`].
    ///
    /// `max_past_jobs` caps the past jobs retained under the current chain tip. `None` and
    /// `Some(0)` both select the crate default.
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_job_declaration_client(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: AllocatedExtranoncePrefix,
        max_target: Target,
        nominal_hashrate: f32,
        version_rolling_allowed: bool,
        rollable_extranonce_size: u16,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        pool_tag_string: Option<String>,
        miner_tag_string: String,
        max_past_jobs: Option<usize>,
    ) -> Result<Self, ExtendedChannelError> {
        Self::new(
            channel_id,
            user_identity,
            extranonce_prefix.into(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            pool_tag_string,
            Some(miner_tag_string),
            max_past_jobs,
        )
    }

    // private constructor
    #[allow(clippy::too_many_arguments)]
    fn new(
        channel_id: u32,
        user_identity: String,
        extranonce_prefix: ExtranoncePrefix,
        max_target: Target,
        nominal_hashrate: f32,
        version_rolling_allowed: bool,
        rollable_extranonce_size: u16,
        share_batch_size: usize,
        expected_share_per_minute: f32,
        pool_tag: Option<String>,
        miner_tag: Option<String>,
        max_past_jobs: Option<usize>,
    ) -> Result<Self, ExtendedChannelError> {
        // see OpenChannelInvalidMaxTarget
        if max_target == Target::ZERO {
            return Err(ExtendedChannelError::OpenChannelInvalidMaxTarget(
                ERROR_CODE_OPEN_MINING_CHANNEL_MAX_TARGET_OUT_OF_RANGE,
            ));
        }

        let target =
            match hash_rate_to_target(nominal_hashrate.into(), expected_share_per_minute.into()) {
                Ok(target) => target,
                Err(_) => {
                    return Err(ExtendedChannelError::OpenChannelInvalidNominalHashrate(
                        ERROR_CODE_OPEN_MINING_CHANNEL_INVALID_NOMINAL_HASHRATE,
                    ));
                }
            };

        // Clamp to max_target rather than error. The client declared max_target as
        // an acceptable difficulty floor, so using it when the initial target would
        // otherwise exceed it is always valid.
        let target = target.min(max_target);

        if extranonce_prefix.len() > MAX_EXTRANONCE_LEN as usize {
            return Err(ExtendedChannelError::ExtranoncePrefixTooLarge);
        }

        let job_factory = JobFactory::new(version_rolling_allowed, pool_tag, miner_tag);

        // conservative check against the spec's worst-case `NewTemplate::coinbase_prefix`.
        // the exact size is re-checked against each actual template in `JobFactory::coinbase`
        if !job_factory
            .fits_script_sig_budget(extranonce_prefix.len() + rollable_extranonce_size as usize)
        {
            return Err(ExtendedChannelError::ScriptSigSizeTooLarge);
        }

        // fall back to the default when the caller has no opinion: `None`, or `Some(0)`, which
        // would otherwise evict the just-retired job and reject the most common late share
        let max_past_jobs = match max_past_jobs {
            Some(cap) if cap > 0 => cap,
            _ => MAX_PAST_JOBS,
        };

        Ok(Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size,
            version_rolling_allowed,
            requested_max_target: max_target,
            target,
            job_id_to_target: HashMap::new(),
            nominal_hashrate,
            stable_hashrate: false,
            job_store: JobStore::new(max_past_jobs),
            job_factory,
            share_accounting: ShareAccounting::new(
                share_batch_size,
                crate::seen_shares_budget(expected_share_per_minute as f64),
            ),
            expected_share_per_minute,
            chain_tip: None,
        })
    }

    /// Returns the unique channel ID for this channel.
    pub fn get_channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Returns the user identity string associated with this channel.
    pub fn get_user_identity(&self) -> &str {
        &self.user_identity
    }

    /// Returns the extranonce prefix bytes for this channel.
    pub fn get_extranonce_prefix(&self) -> &[u8] {
        self.extranonce_prefix.as_bytes()
    }

    /// Returns the current chain tip, if set.
    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Returns the expected number of shares per minute configured for this channel.
    pub fn get_shares_per_minute(&self) -> f32 {
        self.expected_share_per_minute
    }

    /// Only for testing purposes, not meant to be used in real apps.
    #[cfg(test)]
    fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    /// Updates the extranonce prefix for this channel.
    ///
    /// After this call, all newly created jobs will reference the new prefix.
    /// Jobs created before the update will continue to use the previous prefix,
    /// and share validation will be performed accordingly.
    ///
    /// Because of that, the previous prefix (and therefore its slot in the
    /// [`ExtranonceAllocator`](crate::extranonce_manager::ExtranonceAllocator) that minted it) is
    /// not released here: it is handed over to the channel's job store, which only drops it once
    /// every job created under it has become stale. This prevents the allocator from handing the
    /// same extranonce space to another live channel while those jobs still validate shares.
    ///
    /// Returns an error if the new extranonce prefix and the channel's rollable extranonce would
    /// exceed [`MAX_EXTRANONCE_LEN`], or if they would push the assembled coinbase `scriptSig`
    /// past its budget (see
    /// [`JobFactory::fits_script_sig_budget`]). The channel is left unchanged in both error
    /// cases.
    pub fn set_extranonce_prefix(
        &mut self,
        extranonce_prefix: AllocatedExtranoncePrefix,
    ) -> Result<(), ExtendedChannelError> {
        let full_extranonce_size = extranonce_prefix.len() + self.rollable_extranonce_size as usize;
        if full_extranonce_size > MAX_EXTRANONCE_LEN as usize {
            return Err(ExtendedChannelError::ExtranoncePrefixTooLarge);
        }

        // re-run the constructor's invariant: a prefix that is individually valid can still push
        // the assembled scriptSig past the consensus cap
        if !self
            .job_factory
            .fits_script_sig_budget(full_extranonce_size)
        {
            return Err(ExtendedChannelError::ScriptSigSizeTooLarge);
        }

        let retired_extranonce_prefix =
            std::mem::replace(&mut self.extranonce_prefix, extranonce_prefix.into());
        self.job_store
            .retire_extranonce_prefix(retired_extranonce_prefix);

        Ok(())
    }

    /// Replaces the upstream-assigned region of this channel's extranonce prefix.
    ///
    /// The locally allocated suffix and its bitmap lease remain attached to the channel, so this
    /// does not consume a second allocation while jobs created with the previous upstream prefix
    /// remain valid. Existing jobs retain their captured prefix bytes; new jobs use the updated
    /// prefix.
    ///
    /// The channel is left unchanged if the resulting full extranonce exceeds
    /// [`MAX_EXTRANONCE_LEN`] or the assembled coinbase `scriptSig` budget.
    ///
    /// Old prefix bytes share ownership of the same allocator slot while any future, active or
    /// past job uses them. A later `set_extranonce_prefix` rotation cannot release that slot
    /// prematurely. Once those jobs are stale or evicted, only the current prefix (if it still
    /// uses this allocation) keeps the slot reserved. This update consumes no additional slot.
    ///
    /// If this channel belongs to a [`GroupChannel`](super::group::GroupChannel) and the update
    /// changes its full extranonce size, the application must update the group with
    /// [`GroupChannel::set_full_extranonce_size`](super::group::GroupChannel::set_full_extranonce_size)
    /// and register the compatible channel IDs again. Changing the group size clears its current
    /// channel membership.
    pub fn set_upstream_extranonce_prefix(
        &mut self,
        upstream_prefix: &[u8],
    ) -> Result<(), ExtendedChannelError> {
        let full_extranonce_size = upstream_prefix.len()
            + self.extranonce_prefix.preserved_len()
            + self.rollable_extranonce_size as usize;
        if full_extranonce_size > MAX_EXTRANONCE_LEN as usize {
            return Err(ExtendedChannelError::ExtranoncePrefixTooLarge);
        }
        if !self
            .job_factory
            .fits_script_sig_budget(full_extranonce_size)
        {
            return Err(ExtendedChannelError::ScriptSigSizeTooLarge);
        }

        let snapshot = self
            .extranonce_prefix
            .snapshot_for_upstream_update(upstream_prefix);
        self.extranonce_prefix
            .set_upstream_prefix(upstream_prefix)
            .map_err(|_| ExtendedChannelError::ExtranoncePrefixTooLarge)?;
        if let Some(snapshot) = snapshot {
            self.job_store.retire_extranonce_prefix(snapshot);
        }
        Ok(())
    }

    /// Returns the number of bytes available for the rollable portion of the extranonce.
    pub fn get_rollable_extranonce_size(&self) -> u16 {
        self.rollable_extranonce_size
    }

    /// Returns the full extranonce size in bytes.
    pub fn get_full_extranonce_size(&self) -> usize {
        self.extranonce_prefix.len() + self.rollable_extranonce_size as usize
    }

    /// Returns the requested maximum target for this channel.
    pub fn get_requested_max_target(&self) -> &Target {
        &self.requested_max_target
    }

    /// Returns the current target for this channel.
    ///
    /// Please note that this is the current target for the channel. Jobs created before the current target was set are associated with previously set targets, for which shares will be validated against.
    pub fn get_target(&self) -> &Target {
        &self.target
    }

    /// Updates the current target for this channel.
    ///
    /// Please note that this will NOT update the target associated with jobs that were already created.
    ///
    /// Returns [`ExtendedChannelError::InvalidTarget`] if `target` is zero, leaving the channel
    /// unchanged.
    pub fn set_target(&mut self, target: Target) -> Result<(), ExtendedChannelError> {
        if target == Target::ZERO {
            return Err(ExtendedChannelError::InvalidTarget);
        }

        self.target = target;

        Ok(())
    }

    /// Returns the job ID for a future job from a template ID, if any.
    pub fn get_future_job_id_from_template_id(&self, template_id: u64) -> Option<u32> {
        self.job_store
            .get_future_job_id_from_template_id(template_id)
    }

    /// Returns the nominal hashrate for this channel.
    pub fn get_nominal_hashrate(&self) -> f32 {
        self.nominal_hashrate
    }

    /// Sets whether this channel's nominal hashrate should be treated as stable.
    pub fn set_stable_hashrate(&mut self, stable_hashrate: bool) {
        self.stable_hashrate = stable_hashrate;
    }

    /// Returns whether this channel's nominal hashrate is treated as stable.
    pub fn get_stable_hashrate(&self) -> bool {
        self.stable_hashrate
    }

    /// Updates the nominal hashrate for this channel.
    pub fn set_nominal_hashrate(&mut self, hashrate: f32) {
        self.nominal_hashrate = hashrate;
    }

    /// Updates channel configuration with a new nominal hashrate.
    ///
    /// Recomputes target difficulty and updates channel state.
    ///
    /// If the recomputed target is easier than the effective `requested_max_target`,
    /// the target is clamped to `requested_max_target`.
    ///
    /// Returns [`ExtendedChannelError::UpdateChannelInvalidNominalHashrate`] when
    /// `new_nominal_hashrate` cannot be converted into a valid target, and
    /// [`ExtendedChannelError::UpdateChannelInvalidMaxTarget`] when `requested_max_target` is
    /// zero (see the constructor). The channel is left unchanged in both error cases.
    ///
    /// This can be used in two scenarios:
    /// - Client sent `UpdateChannel` message, which contains a `requested_max_target` parameter
    ///   that's also used as input.
    /// - vardiff algorithm estimated a new nominal hashrate, in which case `requested_max_target`
    ///   is `None` and we use the value from the channel state (that was set either during channel
    ///   opening or some previous `UpdateChannel` message).
    pub fn update_channel(
        &mut self,
        new_nominal_hashrate: f32,
        requested_max_target: Option<Target>,
    ) -> Result<(), ExtendedChannelError> {
        let target = match hash_rate_to_target(
            new_nominal_hashrate.into(),
            self.expected_share_per_minute.into(),
        ) {
            Ok(target) => target,
            Err(_) => {
                return Err(ExtendedChannelError::UpdateChannelInvalidNominalHashrate(
                    ERROR_CODE_UPDATE_CHANNEL_INVALID_NOMINAL_HASHRATE,
                ));
            }
        };

        let requested_max_target = match requested_max_target {
            Some(ref requested_max_target) => requested_max_target,
            None => &self.requested_max_target,
        };

        // see OpenChannelInvalidMaxTarget
        if *requested_max_target == Target::ZERO {
            return Err(ExtendedChannelError::UpdateChannelInvalidMaxTarget(
                ERROR_CODE_UPDATE_CHANNEL_MAX_TARGET_OUT_OF_RANGE,
            ));
        }

        // debug hex of target_u256 and max_Target
        // just like in share validation
        // big-endian for display
        let target_bytes = target.to_be_bytes();
        let max_target = requested_max_target;
        let max_target_bytes = max_target.to_be_bytes();

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
        let new_target = target.min(*requested_max_target);

        self.nominal_hashrate = new_nominal_hashrate;
        self.target = new_target;
        self.requested_max_target = *requested_max_target;

        Ok(())
    }

    /// Returns a reference to the currently active job, if any.
    pub fn get_active_job(&self) -> Option<&ExtendedJob> {
        self.job_store.get_active_job()
    }

    /// Returns a reference to a future job from its job ID, if any.
    pub fn get_future_job(&self, job_id: u32) -> Option<&ExtendedJob> {
        self.job_store.get_future_job(job_id)
    }

    /// Returns a reference to a past job from its job ID, if any.
    ///
    /// At most `MAX_PAST_JOBS` past jobs are kept under the current chain tip (oldest
    /// evicted first).
    pub fn get_past_job(&self, job_id: u32) -> Option<&ExtendedJob> {
        self.job_store.get_past_job(job_id)
    }
    /// Returns a reference to the share accounting state for this channel.
    pub fn get_share_accounting(&self) -> &ShareAccounting {
        &self.share_accounting
    }

    /// Updates the channel state with a new template.
    ///
    /// If the template is a future template, the chain tip is not used. At most
    /// `MAX_FUTURE_JOBS` (16) future jobs are kept: storing a new one beyond that limit evicts
    /// the oldest.
    /// If the template is not a future template, the chain tip must be set, and the previous
    /// active job (if any) moves to past jobs, of which at most `MAX_PAST_JOBS` are kept
    /// (oldest evicted first).
    ///
    /// Only meant for usage on a Sv2 Pool Server or a Sv2 Job Declaration Client,
    /// but not on mining clients such as Mining Devices or Proxies.
    ///
    /// Only meant to be used if REQUIRES_CUSTOM_WORK is NOT set on the connection this channel exists on.
    /// If this flag is set, on_set_custom_mining_job should be used instead.
    ///
    /// Returns [`ExtendedChannelError::JobFactoryError`] wrapping
    /// [`JobFactoryError::ScriptSigSizeTooLarge`](crate::server::jobs::error::JobFactoryError::ScriptSigSizeTooLarge)
    /// if the template's `coinbase_prefix` pushes the assembled coinbase `scriptSig` past
    /// its budget. The constructor can only check against the spec's worst-case prefix (see
    /// [`JobFactory::fits_script_sig_budget`]), so this is where an out-of-spec Template Provider
    /// is caught.
    pub fn on_new_template(
        &mut self,
        template: NewTemplateOwned,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<(), ExtendedChannelError> {
        match template.future_template {
            true => {
                let new_job = self
                    .job_factory
                    .new_extended_job(
                        self.channel_id,
                        None,
                        self.extranonce_prefix.as_bytes().to_vec(),
                        template.clone(),
                        coinbase_reward_outputs,
                        self.get_full_extranonce_size(),
                    )
                    .map_err(ExtendedChannelError::JobFactoryError)?;
                self.job_store.add_future_job(template.template_id, new_job);
            }
            false => {
                match self.chain_tip.clone() {
                    // we can only create non-future jobs if we have a chain tip
                    None => return Err(ExtendedChannelError::ChainTipNotSet),
                    Some(chain_tip) => {
                        let new_job = self
                            .job_factory
                            .new_extended_job(
                                self.channel_id,
                                Some(chain_tip),
                                self.extranonce_prefix.as_bytes().to_vec(),
                                template.clone(),
                                coinbase_reward_outputs,
                                self.get_full_extranonce_size(),
                            )
                            .map_err(ExtendedChannelError::JobFactoryError)?;

                        let job_id = new_job.get_job_id();
                        // add the new active job to the job store, dropping the evicted past
                        // job's target mapping (its shares degrade to InvalidJobId) before the
                        // new job's is recorded, as the evicted job may carry the same ID
                        if let Some(evicted_job_id) = self.job_store.add_active_job(new_job) {
                            self.job_id_to_target.remove(&evicted_job_id);
                        }
                        self.job_id_to_target.insert(job_id, self.target);
                    }
                }
            }
        }

        Ok(())
    }

    /// Used as an alternative to `on_new_template` when an extended job is meant to be broadcast to the group channel,
    /// instead of multiple extended jobs to different extended channels.
    ///
    /// We use this method to update the channel state, so it can validate shares from the job that was broadcast to the group channel.
    ///
    /// Only meant to be used if REQUIRES_CUSTOM_WORK is NOT set on the connection this channel exists on.
    /// If this flag is set, on_set_custom_mining_job should be used instead.
    ///
    /// A non-future job is mined against this channel's chain tip, so a `min_ntime` below the
    /// tip's (the group channel built it under an older tip than this channel's) is refused with
    /// [`ExtendedChannelError::JobMinNtimeBelowChainTip`], leaving the channel unchanged.
    ///
    /// A job that advertises version rolling while this channel's policy forbids it is refused
    /// with [`ExtendedChannelError::GroupJobVersionRollingNotAllowed`], leaving the channel
    /// unchanged: the job's flag is what the miner is told and what share validation enforces. A
    /// job stricter than the channel is imported as is.
    pub fn on_group_channel_job(
        &mut self,
        mut extended_job: ExtendedJob,
    ) -> Result<(), ExtendedChannelError> {
        // make sure the extranonce prefix is associated to the channel's extranonce prefix
        extended_job.set_extranonce_prefix(self.extranonce_prefix.as_bytes().to_vec());

        let template_id = match extended_job.get_origin() {
            JobOrigin::NewTemplate(template) => template.template_id,
            JobOrigin::SetCustomMiningJob(_) => {
                return Err(ExtendedChannelError::InvalidJobOrigin);
            }
        };

        // the job's flag is what the miner is told and what validate_share enforces, so a job
        // permitting rolling on a channel that forbids it would put the policy aside; a stricter
        // job is fine, as its own flag is enforced
        if extended_job.version_rolling_allowed() && !self.version_rolling_allowed {
            return Err(ExtendedChannelError::GroupJobVersionRollingNotAllowed);
        }

        match extended_job.is_future() {
            true => {
                self.job_store.add_future_job(template_id, extended_job);
            }
            false => {
                // the job is mined against this channel's chain tip, whose min_ntime is the
                // smallest nTime available for it; a job allowing earlier shares would have them
                // carry a timestamp the tip declared unavailable
                if let Some(min_ntime) = extended_job.get_min_ntime() {
                    if self
                        .chain_tip
                        .as_ref()
                        .is_some_and(|chain_tip| min_ntime < chain_tip.min_ntime())
                    {
                        return Err(ExtendedChannelError::JobMinNtimeBelowChainTip);
                    }
                }

                let job_id = extended_job.get_job_id();
                // the evicted past job's target mapping is dropped (its shares degrade to
                // InvalidJobId) before the new job's is recorded, as the evicted job may carry
                // the same ID
                if let Some(evicted_job_id) = self.job_store.add_active_job(extended_job) {
                    self.job_id_to_target.remove(&evicted_job_id);
                }
                self.job_id_to_target.insert(job_id, self.target);
            }
        }

        Ok(())
    }

    /// Updates the channel state with a new `SetNewPrevHash` message (Template Distribution
    /// Protocol variant).
    ///
    /// If there are future jobs in the Job Store, it activates the future job matching the
    /// `template_id` and sets it as the active job.
    ///
    /// If there are future jobs in the Job Store, but the template id is not found, returns an
    /// error.
    ///
    /// All past jobs are cleared.
    ///
    /// Accepted-share hashes are flushed only if `prev_hash` actually changed: a repeated tip
    /// commits to the same header space, see [`ShareAccounting::flush_seen_shares`].
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashTdp,
    ) -> Result<(), ExtendedChannelError> {
        // extended channels dedicated to custom work don't need to keep track of future jobs
        match self.job_store.has_future_jobs() {
            false => {
                // demote the previously-active job to past so that the subsequent
                // mark_past_jobs_as_stale call moves it into the stale set. without this,
                // a late share for the still-active old job would skip the stale check in
                // validate_share and panic on the missing job_id_to_target entry that we
                // just cleared.
                self.job_store.deactivate_job();
                // explicitly mark past jobs as stale, because we're not going to
                // do it implicitly via activate_future_job in case this extended channel is doing custom work
                self.job_store.mark_past_jobs_as_stale();
                // there is no active job in this branch, so any previous job target
                // mappings are obsolete after the chain tip update.
                self.job_id_to_target.clear();
            }
            true => {
                // try to activate the future job, and also mark past jobs as stale
                if !self.job_store.activate_future_job(
                    set_new_prev_hash.template_id,
                    set_new_prev_hash.header_timestamp,
                ) {
                    return Err(ExtendedChannelError::TemplateIdNotFound);
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

        // hashes are retained while prev_hash is unchanged, see ShareAccounting::flush_seen_shares
        if self
            .chain_tip
            .as_ref()
            .is_some_and(|chain_tip| chain_tip.prev_hash() != set_new_prev_hash.prev_hash)
        {
            self.share_accounting.flush_seen_shares();
        }

        // update the chain tip
        self.chain_tip = Some(set_new_prev_hash.into());

        Ok(())
    }

    /// Updates the channel state with a new custom mining job.
    ///
    /// Under the same chain tip, the previously active job is moved to the past jobs; at most
    /// `MAX_PAST_JOBS` past jobs are kept, retiring one beyond that limit evicts the
    /// oldest. On a chain tip change, the previously active job and all past jobs go stale
    /// instead. The new custom mining job is then set as the active job.
    ///
    /// Assumes SetCustomMiningJob.{prev_hash, nbits, min_ntime} have already been validated.
    /// Updates the channel's `ChainTip`. Accepted-share hashes are flushed only if `prev_hash`
    /// changed: a custom job that keeps it and only advances `min_ntime` (or `nbits`) commits to
    /// the same header space as its predecessor, see [`ShareAccounting::flush_seen_shares`].
    ///
    /// Returns the job id of the new custom mining job.
    ///
    /// To be used by a Sv2 Pool Server upon receiving a `SetCustomMiningJob` message.
    pub fn on_set_custom_mining_job(
        &mut self,
        set_custom_mining_job: SetCustomMiningJobOwned,
    ) -> Result<u32, ExtendedChannelError> {
        let new_job = self
            .job_factory
            .new_extended_job_from_custom_job(
                set_custom_mining_job.clone(),
                self.extranonce_prefix.as_bytes().to_vec(),
                self.get_full_extranonce_size(),
            )
            .map_err(ExtendedChannelError::JobFactoryError)?;

        let set_custom_mining_job_static = set_custom_mining_job;
        let prev_hash = set_custom_mining_job_static.prev_hash;
        let nbits = set_custom_mining_job_static.nbits;
        let min_ntime = set_custom_mining_job_static.min_ntime;
        let new_chain_tip = ChainTip::new(prev_hash, nbits, min_ntime);

        let is_new_prev_hash = self
            .chain_tip
            .as_ref()
            .is_some_and(|chain_tip| chain_tip.prev_hash() != new_chain_tip.prev_hash());
        let is_new_chain_tip = is_new_prev_hash
            || self.chain_tip.as_ref().is_some_and(|chain_tip| {
                chain_tip.nbits() != new_chain_tip.nbits()
                    || chain_tip.min_ntime() != new_chain_tip.min_ntime()
            });

        let job_id = new_job.get_job_id();

        if is_new_chain_tip {
            // tip transition: the displaced active job goes stale together with the past set,
            // bypassing the MAX_PAST_JOBS cap — retiring it through the capped path would push
            // the oldest past job out of the stale set, misclassifying its late shares as
            // InvalidJobId instead of Stale
            self.job_store.deactivate_job();
            self.job_store.mark_past_jobs_as_stale();
            self.job_id_to_target.clear();
        }

        // only a prev_hash change opens a new header space; nbits or min_ntime alone do not,
        // see ShareAccounting::flush_seen_shares
        if is_new_prev_hash {
            self.share_accounting.flush_seen_shares();
        }

        // dropping the evicted past job's target mapping (its shares degrade to InvalidJobId)
        if let Some(evicted_job_id) = self.job_store.add_active_job(new_job) {
            self.job_id_to_target.remove(&evicted_job_id);
        }

        // update the chain tip
        self.chain_tip = Some(new_chain_tip);

        // associate the new active job with the current target
        self.job_id_to_target.insert(job_id, self.target);

        Ok(job_id)
    }

    /// Validates a share.
    ///
    /// Updates the channel state with the result of the share validation.
    /// Rejects shares whose `ntime` is outside `[min_ntime, min_ntime + MAX_FUTURE_BLOCK_TIME]`,
    /// where `min_ntime` is the referenced job's: the chain tip's for jobs built or activated
    /// under it, and the group job's own for jobs installed via
    /// [`on_group_channel_job`](Self::on_group_channel_job) (see [`MAX_FUTURE_BLOCK_TIME`] for
    /// how this clockless upper bound relates to the spec's elapsed-time window).
    ///
    /// Version rolling is enforced per the job's own `version_rolling_allowed`, which
    /// [`on_group_channel_job`](Self::on_group_channel_job) keeps no looser than the channel's
    /// policy.
    ///
    /// A block is reported when the share hash meets the network target the tip's `nbits`
    /// encodes; a stricter Template Distribution `SetNewPrevHash.target` is not consulted, as
    /// [`ChainTip`] does not carry it.
    pub fn validate_share(
        &mut self,
        share: SubmitSharesExtendedOwned,
    ) -> Result<ShareValidationResult, ShareValidationError> {
        // the accepted-share dedup cache is a hard budget on servers: forgetting a
        // still-valid hash would re-enable duplicate-share replay, so once the budget is hit
        // the channel must be closed by the embedding application
        if self.share_accounting.is_seen_shares_budget_exhausted() {
            return Err(ShareValidationError::SeenSharesBudgetExhausted);
        }

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
        };

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

        let extranonce_size = share.extranonce.len();
        if extranonce_size != self.rollable_extranonce_size as usize {
            self.share_accounting
                .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_BAD_EXTRANONCE_SIZE);
            return Err(ShareValidationError::BadExtranonceSize(
                ERROR_CODE_SUBMIT_SHARES_BAD_EXTRANONCE_SIZE,
            ));
        }

        let extranonce_prefix = job.get_extranonce_prefix();
        let mut full_extranonce = vec![];
        full_extranonce.extend_from_slice(extranonce_prefix);
        full_extranonce.extend(share.extranonce.as_bytes());

        // calculate the merkle root from:
        // - job coinbase_tx_prefix
        // - full extranonce
        // - job coinbase_tx_suffix
        // - job merkle_path
        let merkle_root: [u8; 32] = merkle_root_from_path(
            &job.get_coinbase_tx_prefix_without_bip141(),
            &job.get_coinbase_tx_suffix_without_bip141(),
            full_extranonce.as_ref(),
            job.get_merkle_path().as_slice(),
        )
        .ok_or(ShareValidationError::Invalid(
            ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE,
        ))?;

        let chain_tip = self
            .chain_tip
            .as_ref()
            .ok_or(ShareValidationError::NoChainTip)?;

        let prev_hash = chain_tip.prev_hash();
        let nbits = CompactTarget::from_consensus(chain_tip.nbits());

        // the share's ntime is bounded by the min_ntime of the job it references: a job built
        // by this channel takes it from the chain tip at creation, a future job receives it from
        // the SetNewPrevHash that activates it, and a job installed via on_group_channel_job
        // carries the group job's own, which differs from this channel's tip if the application
        // fans the group job out before updating the tip. Every active or past job carries one:
        // a job without it is a future job, which is never mined on.
        let job_min_ntime = job
            .get_min_ntime()
            .expect("active and past jobs carry a min_ntime");

        if share.ntime < job_min_ntime {
            self.share_accounting
                .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE);
            return Err(ShareValidationError::Invalid(
                ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE,
            ));
        }

        // consensus caps block timestamps at ~2h in the future; the allowance is anchored at the
        // receipt of the message that supplied min_ntime, since this crate has no clock (see
        // MAX_FUTURE_BLOCK_TIME)
        if share.ntime > job_min_ntime.saturating_add(MAX_FUTURE_BLOCK_TIME) {
            self.share_accounting
                .increment_rejected_shares(ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE);
            return Err(ShareValidationError::Invalid(
                ERROR_CODE_SUBMIT_SHARES_INVALID_SHARE,
            ));
        }

        // Only BIP323 general-purpose bits may differ from the job's advertised version.
        // When version rolling is not allowed, the share version must match the job version exactly.
        let version_rolling_mask = if job.version_rolling_allowed() {
            VERSION_ROLLING_MASK
        } else {
            0
        };

        // Only the non-rollable version bits are compared: `!version_rolling_mask` zeroes
        // the BIP323 general-purpose bits the miner may change, so any remaining difference
        // from the job's advertised version means an unauthorized change. When version
        // rolling is not allowed, the mask is 0 and this degenerates to strict equality
        // with the job version.
        if (share.version & !version_rolling_mask) != (job.get_version() & !version_rolling_mask) {
            if job.version_rolling_allowed() {
                self.share_accounting.increment_rejected_shares(
                    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
                );
                return Err(ShareValidationError::Invalid(
                    ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
                ));
            }
            self.share_accounting
                .increment_rejected_shares(ERROR_CODE_VERSION_ROLLING_NOT_ALLOWED);
            return Err(ShareValidationError::VersionRollingNotAllowed(
                ERROR_CODE_VERSION_ROLLING_NOT_ALLOWED,
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
        let raw_share_hash: [u8; 32] = *share_hash.to_raw_hash().as_ref();
        let share_hash_target = Target::from_le_bytes(raw_share_hash);
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

            let mut coinbase = vec![];
            coinbase.extend(job.get_coinbase_tx_prefix_with_bip141());
            coinbase.extend(full_extranonce.clone());
            coinbase.extend(job.get_coinbase_tx_suffix_with_bip141());

            match job.get_origin() {
                JobOrigin::NewTemplate(template) => {
                    let template_id = template.template_id;
                    return Ok(ShareValidationResult::BlockFound(
                        share_hash.to_raw_hash(),
                        Some(template_id),
                        coinbase,
                    ));
                }
                JobOrigin::SetCustomMiningJob(_set_custom_mining_job) => {
                    return Ok(ShareValidationResult::BlockFound(
                        share_hash.to_raw_hash(),
                        None,
                        coinbase,
                    ));
                }
            }
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
            error::ExtendedChannelError,
            extended::ExtendedChannel,
            jobs::{
                extended::ExtendedJob,
                factory::{MAX_COINBASE_PREFIX_SIZE, MAX_SCRIPT_SIG_SIZE},
                job_store::{MAX_FUTURE_JOBS, MAX_PAST_JOBS},
            },
            share_accounting::{ShareValidationError, ShareValidationResult},
        },
        MAX_EXTRANONCE_LEN,
    };
    use binary_sv2::{Sv2OptionOwned as Sv2Option, U256Owned as U256};
    use bitcoin::{transaction::TxOut, Amount, ScriptBuf, Target};
    use mining_sv2::{
        NewExtendedMiningJobOwned as NewExtendedMiningJob,
        SetCustomMiningJobOwned as SetCustomMiningJob,
        SubmitSharesExtendedOwned as SubmitSharesExtended,
        ERROR_CODE_OPEN_MINING_CHANNEL_MAX_TARGET_OUT_OF_RANGE,
        ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW,
        ERROR_CODE_SUBMIT_SHARES_INVALID_NON_ROLLABLE_VERSION_BIT,
        ERROR_CODE_UPDATE_CHANNEL_MAX_TARGET_OUT_OF_RANGE,
    };
    use std::convert::TryInto;
    use template_distribution_sv2::{
        NewTemplateOwned as NewTemplate, SetNewPrevHashOwned as SetNewPrevHash,
    };

    const SATS_AVAILABLE_IN_TEMPLATE: u64 = 5000000000;

    #[test]
    fn test_future_job_activation_flow() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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

        assert!(!channel.job_store.has_future_jobs());
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();
        assert!(channel.get_active_job().is_none());

        let future_job_id = channel
            .get_future_job_id_from_template_id(template.template_id)
            .unwrap();

        let future_job = channel.get_future_job(future_job_id).unwrap().clone();

        // we know that the provided template + coinbase_reward_outputs should generate this future
        // job
        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 41, 82, 0, 6, 83, 118, 50, 47, 47,
                47, 31,
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

        assert_eq!(future_job.get_job_message(), &expected_job);

        let ntime = 1746839905;
        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
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

        channel.on_set_new_prev_hash(set_new_prev_hash).unwrap();

        // we just activated the only future job
        assert!(!channel.job_store.has_future_jobs());

        let mut previously_future_job = future_job.clone();
        previously_future_job.activate(ntime);

        let activated_job = channel.get_active_job().unwrap();

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

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let ntime = 1746839905;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        let n_bits = 503543726;

        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        channel.set_chain_tip(chain_tip);

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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

        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        assert!(!channel.job_store.has_future_jobs());

        let active_job = channel.get_active_job().unwrap().clone();

        // we know that the provided template + coinbase_reward_outputs should generate this
        // non-future job
        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(ntime)),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 41, 82, 0, 6, 83, 118, 50, 47, 47,
                47, 31,
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

        assert_eq!(active_job.get_job_message(), &expected_job);
    }

    #[test]
    fn test_coinbase_reward_outputs_sum_above_template_value() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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

        let invalid_coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE + 1), /* oops: one too many extra
                                                                      * sats */
            script_pubkey: script,
        }];

        let res = channel.on_new_template(template.clone(), invalid_coinbase_reward_outputs);

        assert!(res.is_err());
        assert!(!channel.job_store.has_future_jobs());
    }

    #[test]
    fn test_share_validation_block_found() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation and share
        // validation

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        // channel target: 04325c53ef368eb04325c53ef368eb04325c53ef368eb04325c53ef368eb0431

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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
        channel.set_chain_tip(chain_tip);

        // prepare channel with non-future job
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // this share has hash 6b356f9f445f4cdfab140f69ff66803f8f98a0d8bcd089dc7d2bdeeee74a5f83
        // which satisfies network target
        // 7fffff0000000000000000000000000000000000000000000000000000000000
        let share_valid_block = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_valid_block.clone());

        assert!(matches!(
            res,
            Ok(ShareValidationResult::BlockFound(_, _, _))
        ));
        assert_eq!(channel.get_share_accounting().get_blocks_found(), 1);

        // re-submitting the same valid block must be rejected as duplicate
        let res = channel.validate_share(share_valid_block);
        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DuplicateShare(_)
        ));
        assert_eq!(channel.get_share_accounting().get_blocks_found(), 1);
    }

    #[test]
    fn test_share_validation_ntime_below_min_ntime() {
        // Regression test: a share with ntime < min_ntime must be rejected.
        // Reuses the block-found test vectors but sets min_ntime one second
        // above the share's ntime.
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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
        // set min_ntime one second above the share's ntime (1745596971 + 1)
        let chain_tip = ChainTip::new(prev_hash, n_bits, 1745596972);
        channel.set_chain_tip(chain_tip);

        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let share_below_min_ntime = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 8,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_below_min_ntime);

        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));
        assert_eq!(channel.get_share_accounting().get_blocks_found(), 0);
    }

    #[test]
    fn test_share_validation_does_not_meet_target() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation and share
        // validation

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 100.0; // bigger hashrate to get higher difficulty
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        // channel target: 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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
        channel.set_chain_tip(chain_tip);

        // prepare channel with non-future job
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // this share has hash d5767872f3a26e7f9f21cd968f27cfdb8b4061bb9ce0959852594ee8620f4efb
        // which does not meet the channel target
        // 000aebbc990fff5144366f000aebbc990fff5144366f000aebbc990fff514435
        let share_low_diff = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_low_diff);

        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::DoesNotMeetTarget(_)
        ));
        assert_eq!(
            channel
                .get_share_accounting()
                .get_rejected_shares_error_count(ERROR_CODE_SUBMIT_SHARES_DIFFICULTY_TOO_LOW),
            1
        );
    }

    #[test]
    fn test_share_validation_valid_share() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation and share
        // validation

        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1_000.0; // bigger hashrate to get higher difficulty
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        // channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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

        // network tarkget is: 000000000000d7c0000000000000000000000000000000000000000000000000
        let n_bits = 453040064;
        let ntime = 1745611105;
        let prev_hash = [
            23, 205, 72, 134, 153, 86, 220, 153, 224, 28, 216, 146, 228, 120, 227, 157, 213, 99,
            160, 163, 128, 59, 139, 190, 158, 62, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        channel.set_chain_tip(chain_tip);

        // prepare channel with non-future job
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // this share has hash 000054cc966c174fc21b056b994c0aa265c40126222040984f571230b9adde1f
        // which does meet the channel target
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205
        // but does not meet network target
        // 000000000000d7c0000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 16647,
            ntime: 1745611105,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(valid_share);
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));

        // try to cheat by re-submitting the same share
        // with a different sequence number
        let repeated_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 2,
            job_id: 1,
            nonce: 16647,
            ntime: 1745611105,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(repeated_share);

        // assert duplicate share is rejected
        assert!(matches!(res, Err(ShareValidationError::DuplicateShare(_))));
    }

    #[test]
    fn test_new_clamps_target_to_max_target() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [0, 0, 0, 1].to_vec();
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;
        let expected_share_per_minute = 1.0;
        let very_small_hashrate = 0.1;

        // less permissive max_target to exercise constructor clamp path
        let not_so_permissive_max_target = Target::from_le_bytes([
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0x00,
        ]);

        let channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            not_so_permissive_max_target,
            very_small_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
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

    // a 52 char pool tag places the worst-case scriptSig exactly on the budget:
    // 8 (MAX_COINBASE_PREFIX_SIZE) + 1 + 3 ("Sv2") + 3 + 52 (tag) + 1 + 24 + 8 (full extranonce)
    // = 100
    const POOL_TAG_AT_SCRIPT_SIG_BUDGET: usize = 52;
    const EXTRANONCE_PREFIX_LEN_AT_SCRIPT_SIG_BUDGET: usize = 24;
    const ROLLABLE_EXTRANONCE_SIZE_AT_SCRIPT_SIG_BUDGET: u16 = 8;

    fn new_extended_channel_with_pool_tag(
        pool_tag_len: usize,
        extranonce_prefix_len: usize,
    ) -> Result<ExtendedChannel, ExtendedChannelError> {
        ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![0xab; extranonce_prefix_len]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            ROLLABLE_EXTRANONCE_SIZE_AT_SCRIPT_SIG_BUDGET,
            100,
            1.0,
            Some("x".repeat(pool_tag_len)),
            None,
            None,
        )
    }

    #[test]
    fn test_new_rejects_oversized_script_sig() {
        // exactly on the budget
        let channel = new_extended_channel_with_pool_tag(
            POOL_TAG_AT_SCRIPT_SIG_BUDGET,
            EXTRANONCE_PREFIX_LEN_AT_SCRIPT_SIG_BUDGET,
        )
        .unwrap();
        assert_eq!(
            channel
                .job_factory
                .script_sig_size(MAX_COINBASE_PREFIX_SIZE, channel.get_full_extranonce_size()),
            MAX_SCRIPT_SIG_SIZE
        );

        // one byte over the budget, via a longer tag
        let channel = new_extended_channel_with_pool_tag(
            POOL_TAG_AT_SCRIPT_SIG_BUDGET + 1,
            EXTRANONCE_PREFIX_LEN_AT_SCRIPT_SIG_BUDGET,
        );
        assert!(matches!(
            channel.unwrap_err(),
            ExtendedChannelError::ScriptSigSizeTooLarge
        ));

        // one byte over the budget, via a longer extranonce prefix
        let channel = new_extended_channel_with_pool_tag(
            POOL_TAG_AT_SCRIPT_SIG_BUDGET,
            EXTRANONCE_PREFIX_LEN_AT_SCRIPT_SIG_BUDGET + 1,
        );
        assert!(matches!(
            channel.unwrap_err(),
            ExtendedChannelError::ScriptSigSizeTooLarge
        ));
    }

    #[test]
    fn test_set_extranonce_prefix_rejects_oversized_script_sig() {
        // start well within the budget
        let original_prefix_len = 4;
        // One extra tag byte makes the scriptSig budget bind at a 31-byte full extranonce, below
        // MAX_EXTRANONCE_LEN. This isolates the scriptSig check from the full-extranonce check.
        let pool_tag_len = POOL_TAG_AT_SCRIPT_SIG_BUDGET + 1;
        let largest_valid_prefix_len = EXTRANONCE_PREFIX_LEN_AT_SCRIPT_SIG_BUDGET - 1;
        let mut channel =
            new_extended_channel_with_pool_tag(pool_tag_len, original_prefix_len).unwrap();
        let original_prefix = channel.get_extranonce_prefix().to_vec();

        // growing up to the budget is allowed
        channel
            .set_extranonce_prefix(
                AllocatedExtranoncePrefix::for_test(vec![0xcd; largest_valid_prefix_len]).unwrap(),
            )
            .unwrap();
        assert_eq!(
            channel.get_extranonce_prefix().len(),
            largest_valid_prefix_len
        );

        // go back to the original prefix, so we can assert the channel is untouched on error
        channel
            .set_extranonce_prefix(
                AllocatedExtranoncePrefix::for_test(original_prefix.clone()).unwrap(),
            )
            .unwrap();

        // a prefix that is individually valid (<= MAX_EXTRANONCE_LEN) but pushes the assembled
        // scriptSig one byte past the budget must be rejected
        let oversized_prefix = vec![0xcd; largest_valid_prefix_len + 1];
        assert!(oversized_prefix.len() <= MAX_EXTRANONCE_LEN as usize);
        let res = channel.set_extranonce_prefix(
            AllocatedExtranoncePrefix::for_test(oversized_prefix.clone()).unwrap(),
        );
        assert!(matches!(
            res.unwrap_err(),
            ExtendedChannelError::ScriptSigSizeTooLarge
        ));
        assert_eq!(channel.get_extranonce_prefix(), &original_prefix[..]);

        let res = channel.set_upstream_extranonce_prefix(&oversized_prefix);
        assert!(matches!(
            res.unwrap_err(),
            ExtendedChannelError::ScriptSigSizeTooLarge
        ));
        assert_eq!(channel.get_extranonce_prefix(), &original_prefix[..]);
    }

    #[test]
    fn test_set_extranonce_prefix_enforces_full_extranonce_size_transactionally() {
        let original_prefix_len = 4;
        let mut channel = new_extended_channel_with_pool_tag(0, original_prefix_len).unwrap();

        let largest_valid_prefix_len =
            MAX_EXTRANONCE_LEN as usize - ROLLABLE_EXTRANONCE_SIZE_AT_SCRIPT_SIG_BUDGET as usize;
        let largest_valid_prefix = vec![0xcd; largest_valid_prefix_len];
        channel
            .set_extranonce_prefix(
                AllocatedExtranoncePrefix::for_test(largest_valid_prefix.clone()).unwrap(),
            )
            .unwrap();

        let result = channel.set_extranonce_prefix(
            AllocatedExtranoncePrefix::for_test(vec![0xef; largest_valid_prefix_len + 1]).unwrap(),
        );
        assert!(matches!(
            result,
            Err(ExtendedChannelError::ExtranoncePrefixTooLarge)
        ));
        assert_eq!(channel.get_extranonce_prefix(), &largest_valid_prefix);
    }

    #[test]
    fn test_set_upstream_extranonce_prefix_preserves_allocation_transactionally() {
        let mut allocator =
            ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 5, 256).unwrap();
        let allocated_prefix = allocator.allocate_extended(2).unwrap();
        let old_suffix = allocated_prefix.as_bytes()[1..].to_vec();
        let mut channel = ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            allocated_prefix.into(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            2,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();

        allocator.set_upstream_prefix(vec![0xcc, 0xdd]).unwrap();
        channel
            .set_upstream_extranonce_prefix(allocator.upstream_prefix())
            .unwrap();
        let mut expected = vec![0xcc, 0xdd];
        expected.extend_from_slice(&old_suffix);
        assert_eq!(channel.get_extranonce_prefix(), &expected);
        assert_eq!(allocator.allocated_count(), 1);

        let prefix_before_error = channel.get_extranonce_prefix().to_vec();
        let result = channel.set_upstream_extranonce_prefix(&[0xee; 29]);
        assert!(matches!(
            result,
            Err(ExtendedChannelError::ExtranoncePrefixTooLarge)
        ));
        assert_eq!(channel.get_extranonce_prefix(), &prefix_before_error);
        assert_eq!(allocator.allocated_count(), 1);
    }

    #[test]
    fn test_update_channel() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let expected_share_per_minute = 1.0;
        let initial_hashrate = 10.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;

        // this is the most permissive possible max_target
        let max_target = Target::from_le_bytes([0xff; 32]);

        // Create a channel with initial hashrate
        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            initial_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
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
            Err(ExtendedChannelError::UpdateChannelInvalidNominalHashrate(_))
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
        let extranonce_prefix = [0, 0, 0, 0, 0, 0, 0, 1].to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1_000.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, extranonce_prefix.as_slice());

        let new_extranonce_prefix = [0, 0, 0, 0, 0, 0, 0, 0, 0, 2].to_vec();

        channel
            .set_extranonce_prefix(
                AllocatedExtranoncePrefix::for_test(new_extranonce_prefix.clone()).unwrap(),
            )
            .unwrap();
        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix, new_extranonce_prefix.as_slice());

        let new_extranonce_prefix_too_large = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0,
        ]
        .to_vec();

        // too-large extranonce prefixes are rejected at the wire boundary
        assert!(matches!(
            ExtranoncePrefix::from_wire(new_extranonce_prefix_too_large.clone()),
            Err(ExtranoncePrefixError::ExceedsMaxLength)
        ));
    }

    // Builds an extended channel from a real allocator that only has room for two channels,
    // creates an active job under the first allocated prefix, then rotates the channel onto the
    // second one.
    //
    // Returns the allocator (now with both slots handed out), the channel, the bytes of the
    // rotated-out prefix, and the id of the job created under it.
    fn extended_channel_with_rotated_extranonce_prefix(
    ) -> (ExtranonceAllocator, ExtendedChannel, Vec<u8>, u32) {
        let total_extranonce_len = 32;
        let max_channels = 2;
        let min_rollable_size = 8;

        let mut allocator =
            ExtranonceAllocator::new(vec![], total_extranonce_len, max_channels).unwrap();

        let prefix_1 = allocator.allocate_extended(min_rollable_size).unwrap();
        let prefix_2 = allocator.allocate_extended(min_rollable_size).unwrap();
        assert_ne!(prefix_1.as_bytes(), prefix_2.as_bytes());
        assert_eq!(allocator.allocated_count(), 2);

        let (mut channel, prefix_1_bytes, job_id) = extended_channel_with_live_job(prefix_1, false);
        channel.set_extranonce_prefix(prefix_2).unwrap();
        (allocator, channel, prefix_1_bytes, job_id)
    }

    fn extended_channel_with_live_job(
        prefix_1: AllocatedExtranoncePrefix,
        future: bool,
    ) -> (ExtendedChannel, Vec<u8>, u32) {
        let prefix_1_bytes = prefix_1.as_bytes().to_vec();
        let rollable_extranonce_size = (32 - prefix_1_bytes.len()) as u16;

        let mut channel = ExtendedChannel::new_for_pool(
            1,
            "user_identity".to_string(),
            prefix_1,
            Target::from_le_bytes([0xff; 32]),
            100.0,
            true,
            rollable_extranonce_size,
            100,
            1.0,
            String::new(),
            None,
        )
        .unwrap();

        let ntime = 1745596910;
        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let n_bits = 453040064;
        channel.set_chain_tip(ChainTip::new(prev_hash, n_bits, ntime));

        let template = NewTemplate {
            template_id: 1,
            future_template: future,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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

        channel
            .on_new_template(template, coinbase_reward_outputs)
            .unwrap();
        let job_id = if future {
            channel.get_future_job_id_from_template_id(1).unwrap()
        } else {
            channel.get_active_job().unwrap().get_job_id()
        };

        (channel, prefix_1_bytes, job_id)
    }

    #[test]
    fn test_mixed_prefix_updates_reserve_slot_until_jobs_go_stale() {
        for future in [false, true] {
            let mut allocator =
                ExtranonceAllocator::from_upstream_prefix(vec![0xaa], vec![0xbb], 32, 2).unwrap();
            let prefix = allocator.allocate_extended(8).unwrap();
            let (mut channel, old_bytes, job_id) = extended_channel_with_live_job(prefix, future);

            allocator.set_upstream_prefix(vec![0xcc]).unwrap();
            channel.set_upstream_extranonce_prefix(&[0xcc]).unwrap();
            assert_eq!(allocator.allocated_count(), 1);
            assert_eq!(&channel.get_extranonce_prefix()[1..], &old_bytes[1..]);

            let replacement = allocator.allocate_extended(8).unwrap();
            channel.set_extranonce_prefix(replacement).unwrap();
            let job = if future {
                channel.get_future_job(job_id).unwrap()
            } else {
                channel.get_active_job().unwrap()
            };
            assert_eq!(job.get_extranonce_prefix(), old_bytes);
            assert_eq!(allocator.allocated_count(), 2);
            allocator.set_upstream_prefix(vec![0xaa]).unwrap();
            assert!(matches!(
                allocator.allocate_extended(8),
                Err(ExtranonceAllocatorError::CapacityExhausted)
            ));

            // Activating the future job must preserve its old prefix and allocation.
            if future {
                channel
                    .on_set_new_prev_hash(SetNewPrevHash {
                        template_id: 1,
                        prev_hash: [2; 32].into(),
                        header_timestamp: 1745596970,
                        n_bits: 453040064,
                        target: [0xff; 32].into(),
                    })
                    .unwrap();
                assert_eq!(
                    channel.get_active_job().unwrap().get_extranonce_prefix(),
                    old_bytes
                );
                assert_eq!(allocator.allocated_count(), 2);
            }

            // A new tip with no matching future job retires the old work.
            channel
                .on_set_new_prev_hash(SetNewPrevHash {
                    template_id: 999,
                    prev_hash: [1; 32].into(),
                    header_timestamp: 1745597510,
                    n_bits: 453040064,
                    target: [0xff; 32].into(),
                })
                .unwrap();
            assert!(channel.get_active_job().is_none());
            assert_eq!(allocator.allocated_count(), 1);
            let reused = allocator.allocate_extended(8).unwrap();
            assert_eq!(reused.as_bytes(), old_bytes);
            drop(reused);
            drop(channel);
            assert_eq!(allocator.allocated_count(), 0);
        }
    }

    #[test]
    fn test_rotated_extranonce_prefix_slot_not_reused_while_job_live() {
        // Regression test: rotating the channel's extranonce prefix must not return the old
        // prefix's allocator slot to the free pool while jobs created under it can still accept
        // shares. Otherwise the allocator could hand the very same extranonce space to a second
        // live channel, making the same work replayable across both.
        let (mut allocator, channel, prefix_1_bytes, _job_id) =
            extended_channel_with_rotated_extranonce_prefix();

        // the rotated-out slot is still reserved, so the allocator is still full
        assert_eq!(allocator.allocated_count(), 2);
        assert!(matches!(
            allocator.allocate_extended(8),
            Err(ExtranonceAllocatorError::CapacityExhausted)
        ));

        // and the pre-rotation job is still live under the old prefix bytes
        assert_eq!(
            channel.get_active_job().unwrap().get_extranonce_prefix(),
            prefix_1_bytes.as_slice()
        );
    }

    #[test]
    fn test_retired_extranonce_prefix_released_after_jobs_go_stale() {
        // Counterpart of the test above: the deferred release must actually happen once the jobs
        // created under the old prefix become stale, otherwise slots would leak.
        let (mut allocator, mut channel, _prefix_1_bytes, job_id) =
            extended_channel_with_rotated_extranonce_prefix();

        let new_prev_hash = SetNewPrevHash {
            template_id: 999,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            header_timestamp: 1745596910 + 600,
            n_bits: 453040064,
            target: [0xff; 32].into(),
        };
        channel.on_set_new_prev_hash(new_prev_hash).unwrap();

        // the pre-rotation job can no longer accept shares
        let late_share = SubmitSharesExtended {
            channel_id: 1,
            sequence_number: 0,
            job_id,
            nonce: 0,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![0; 31].try_into().unwrap(),
        };
        assert!(matches!(
            channel.validate_share(late_share),
            Err(ShareValidationError::Stale(_))
        ));

        // so its prefix was released back to the allocator
        assert_eq!(allocator.allocated_count(), 1);
        assert!(allocator.allocate_extended(8).is_ok());
    }

    #[test]
    fn test_retired_extranonce_prefix_released_after_job_eviction() {
        // Eviction counterpart of the test above: when the last job created under a
        // rotated-out prefix is evicted from past jobs, the prefix's slot must be released
        // right away — a peer withholding the next chain transition must not be able to pin
        // allocator slots.
        let (mut allocator, mut channel, _prefix_1_bytes, job_id) =
            extended_channel_with_rotated_extranonce_prefix();

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

        // flood enough non-future templates to push the pre-rotation job out of past jobs
        for template_id in 2..2 + MAX_PAST_JOBS as u64 + 2 {
            let template = NewTemplate {
                template_id,
                future_template: false,
                version: 536870912,
                coinbase_tx_version: 2,
                coinbase_prefix: vec![82, 0].try_into().unwrap(),
                coinbase_tx_input_sequence: 4294967295,
                coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
                coinbase_tx_outputs_count: 1,
                coinbase_tx_outputs: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113,
                    209, 222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153,
                    98, 180, 139, 235, 216, 54, 151, 78, 140, 249,
                ]
                .try_into()
                .unwrap(),
                coinbase_tx_locktime: 0,
                merkle_path: vec![].try_into().unwrap(),
            };
            channel
                .on_new_template(template, coinbase_reward_outputs.clone())
                .unwrap();
        }
        assert!(channel.get_past_job(job_id).is_none());

        // the evicted job was the last reference to the rotated-out prefix, so its slot is
        // free again
        assert_eq!(allocator.allocated_count(), 1);
        assert!(allocator.allocate_extended(8).is_ok());
    }

    #[test]
    fn test_on_group_channel_job_assigns_extranonce_prefix_to_future_job() {
        // Test that on_group_channel_job assigns the channel's extranonce prefix
        // to a future job that came from a group channel (with empty prefix)
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let channel_extranonce_prefix = vec![1, 2, 3, 4, 5, 6, 7];
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(channel_extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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

        // Create a job with empty extranonce prefix (as group channels do)
        let group_job = ExtendedJob::from_template(
            template.clone(),
            vec![], // empty extranonce prefix from group channel
            coinbase_reward_outputs,
            vec![],
            vec![],
            NewExtendedMiningJob {
                channel_id,
                job_id: 1,
                min_ntime: Sv2Option::new(None),
                version: template.version,
                version_rolling_allowed,
                coinbase_tx_prefix: vec![].try_into().unwrap(),
                coinbase_tx_suffix: vec![].try_into().unwrap(),
                merkle_path: vec![].try_into().unwrap(),
            },
        )
        .unwrap();

        // Verify the job has empty extranonce prefix initially
        assert_eq!(group_job.get_extranonce_prefix(), &vec![]);

        assert!(!channel.job_store.has_future_jobs());

        // Call on_group_channel_job to assign this channel's extranonce prefix
        channel.on_group_channel_job(group_job).unwrap();

        // Verify the job was added to future jobs
        assert!(channel.job_store.has_future_jobs());

        // Verify the job now has the channel's extranonce prefix assigned
        let future_job_id = channel
            .get_future_job_id_from_template_id(template.template_id)
            .unwrap();
        let stored_job = channel.get_future_job(future_job_id).unwrap();
        assert_eq!(
            stored_job.get_extranonce_prefix(),
            &channel_extranonce_prefix
        );
    }

    #[test]
    fn test_on_group_channel_job_assigns_extranonce_prefix_to_active_job() {
        // Test that on_group_channel_job assigns the channel's extranonce prefix
        // to an active (non-future) job from a group channel
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let channel_extranonce_prefix = vec![10, 20, 30, 40, 50];
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(channel_extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: false, // non-future/active job
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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

        let ntime = 1746839905;
        // Create a job with empty extranonce prefix (as group channels do)
        let group_job = ExtendedJob::from_template(
            template.clone(),
            vec![], // empty extranonce prefix from group channel
            coinbase_reward_outputs,
            vec![],
            vec![],
            NewExtendedMiningJob {
                channel_id,
                job_id: 1,
                min_ntime: Sv2Option::new(Some(ntime)),
                version: template.version,
                version_rolling_allowed,
                coinbase_tx_prefix: vec![].try_into().unwrap(),
                coinbase_tx_suffix: vec![].try_into().unwrap(),
                merkle_path: vec![].try_into().unwrap(),
            },
        )
        .unwrap();

        // Set chain tip to enable active job storage
        channel.set_chain_tip(ChainTip::new(U256::from([0; 32]), 0, ntime));

        // Verify the job has empty extranonce prefix initially
        assert_eq!(group_job.get_extranonce_prefix(), &vec![]);
        assert!(channel.get_active_job().is_none());

        // Call on_group_channel_job
        channel.on_group_channel_job(group_job).unwrap();

        // Verify the job was added as active job with the channel's extranonce prefix
        let active_job = channel.get_active_job().unwrap();
        assert_eq!(
            active_job.get_extranonce_prefix(),
            &channel_extranonce_prefix
        );
    }

    #[test]
    fn test_on_group_channel_job_rejects_custom_mining_job() {
        // Test that on_group_channel_job returns InvalidJobOrigin error for SetCustomMiningJob
        // because custom jobs don't come from group channels
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = vec![1, 2, 3, 4];
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 4u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix.clone()).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        // Create a job from SetCustomMiningJob (not from group channel)
        let custom_job = SetCustomMiningJob {
            channel_id,
            request_id: 0,
            token: vec![].try_into().unwrap(),
            version: 536870912,
            prev_hash: [0; 32].into(),
            min_ntime: 1746839905,
            nbits: 503543726,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![].try_into().unwrap(),
            coinbase_tx_input_n_sequence: 4294967295,
            coinbase_tx_outputs: vec![].try_into().unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        let extended_job = ExtendedJob::from_custom_job(
            custom_job,
            vec![], // empty extranonce prefix
            vec![],
            vec![],
            vec![],
            NewExtendedMiningJob {
                channel_id,
                job_id: 1,
                min_ntime: Sv2Option::new(None),
                version: 536870912,
                version_rolling_allowed,
                coinbase_tx_prefix: vec![].try_into().unwrap(),
                coinbase_tx_suffix: vec![].try_into().unwrap(),
                merkle_path: vec![].try_into().unwrap(),
            },
        );

        // Call on_group_channel_job and expect InvalidJobOrigin error
        // because custom jobs don't come from group channels
        let result = channel.on_group_channel_job(extended_job);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelError::InvalidJobOrigin
        ));
    }

    #[test]
    fn test_set_new_prev_hash_without_future_jobs_marks_active_as_stale() {
        // Regression test: when on_set_new_prev_hash takes the no-future-jobs branch,
        // the previously-active job must be moved to stale so that late shares are
        // rejected as Stale instead of panicking on a missing job_id_to_target entry.
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 100.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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
        channel.set_chain_tip(chain_tip);

        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let active_job_id = channel.get_active_job().unwrap().get_job_id();
        assert!(!channel.job_store.has_future_jobs());

        // Chain tip flip with no matching future job (typical of custom-work / JD flow
        // where the channel never queues future jobs from a TDP NewTemplate).
        let new_prev_hash = SetNewPrevHash {
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

        channel.on_set_new_prev_hash(new_prev_hash).unwrap();

        // A miner's in-flight share for the now-old job arrives after the tip flip.
        let late_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: active_job_id,
            nonce: 0,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(late_share);
        assert!(matches!(res, Err(ShareValidationError::Stale(_))));
    }

    #[test]
    fn test_set_custom_mining_job_chain_tip_change_marks_past_job_stale() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = vec![1, 2, 3, 4];
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 100.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let first_prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ];
        let second_prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];

        let first_job_id = channel
            .on_set_custom_mining_job(custom_mining_job(
                channel_id,
                1,
                first_prev_hash,
                1745596910,
            ))
            .unwrap();
        let second_job_id = channel
            .on_set_custom_mining_job(custom_mining_job(
                channel_id,
                2,
                second_prev_hash,
                1745596970,
            ))
            .unwrap();

        assert_ne!(first_job_id, second_job_id);
        assert!(channel.job_store.get_stale_job(first_job_id).is_some());

        let late_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: first_job_id,
            nonce: 0,
            ntime: 1745596930,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(late_share);
        assert!(matches!(res, Err(ShareValidationError::Stale(_))));
    }

    #[test]
    fn test_set_custom_mining_job_same_chain_tip_keeps_past_job() {
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = vec![1, 2, 3, 4];
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 100.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ];
        let min_ntime = 1745596910;

        let first_job_id = channel
            .on_set_custom_mining_job(custom_mining_job(channel_id, 1, prev_hash, min_ntime))
            .unwrap();
        let second_job_id = channel
            .on_set_custom_mining_job(custom_mining_job(channel_id, 2, prev_hash, min_ntime))
            .unwrap();

        assert_ne!(first_job_id, second_job_id);
        assert!(channel.job_store.get_past_job(first_job_id).is_some());
        assert!(channel.job_store.get_stale_job(first_job_id).is_none());
    }

    #[test]
    fn test_set_custom_mining_job_chain_tip_change_keeps_all_past_jobs_in_stale_set() {
        // At a tip transition the displaced active job and every retained past job must land
        // in the stale set: with past jobs at the MAX_PAST_JOBS cap, a capped retirement of
        // the displaced job would push the oldest past job out of the stale set,
        // misclassifying its late shares as InvalidJobId instead of Stale.
        let channel_id = 1;

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![1, 2, 3, 4]).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            100.0,
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();

        let first_prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ];
        let second_prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ];

        // fill past jobs to the cap under the first tip, plus the active job
        let mut same_tip_job_ids = Vec::new();
        for request_id in 0..MAX_PAST_JOBS as u32 + 2 {
            let job_id = channel
                .on_set_custom_mining_job(custom_mining_job(
                    channel_id,
                    request_id,
                    first_prev_hash,
                    1745596910,
                ))
                .unwrap();
            same_tip_job_ids.push(job_id);
        }

        // tip transition
        channel
            .on_set_custom_mining_job(custom_mining_job(
                channel_id,
                MAX_PAST_JOBS as u32 + 2,
                second_prev_hash,
                1745596970,
            ))
            .unwrap();

        // the newest MAX_PAST_JOBS past jobs and the displaced active job are all stale
        for job_id in same_tip_job_ids.iter().rev().take(MAX_PAST_JOBS + 1) {
            assert!(channel.job_store.get_stale_job(*job_id).is_some());
        }
    }

    #[test]
    fn test_retired_extranonce_prefix_kept_through_future_job_activation() {
        // Activating a future job created under a rotated-out prefix must not release that
        // prefix's allocator slot, even when the activation's retirement of the displaced
        // active job coincides with past jobs sitting at the MAX_PAST_JOBS cap. The activated
        // job keeps validating shares under the old prefix bytes, so releasing the slot could
        // hand the same extranonce space to a second live channel.
        let total_extranonce_len = 32;
        let max_channels = 2;
        let min_rollable_size = 8;

        let mut allocator =
            ExtranonceAllocator::new(vec![], total_extranonce_len, max_channels).unwrap();

        let prefix_1 = allocator.allocate_extended(min_rollable_size).unwrap();
        let prefix_2 = allocator.allocate_extended(min_rollable_size).unwrap();
        let prefix_1_bytes = prefix_1.as_bytes().to_vec();
        let rollable_extranonce_size =
            (total_extranonce_len as usize - prefix_1_bytes.len()) as u16;

        let mut channel = ExtendedChannel::new_for_pool(
            1,
            "user_identity".to_string(),
            prefix_1,
            Target::from_le_bytes([0xff; 32]),
            100.0,
            true,
            rollable_extranonce_size,
            100,
            1.0,
            String::new(),
            None,
        )
        .unwrap();

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

        let template = |template_id: u64, future_template: bool| NewTemplate {
            template_id,
            future_template,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        // a future job created under the first prefix, which is then rotated out
        channel
            .on_new_template(template(100, true), coinbase_reward_outputs.clone())
            .unwrap();
        channel.set_extranonce_prefix(prefix_2).unwrap();

        // fill past jobs to the cap with non-future jobs under the second prefix
        let ntime = 1745596910;
        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        channel.set_chain_tip(ChainTip::new(prev_hash, 453040064, ntime));
        for template_id in 0..MAX_PAST_JOBS as u64 + 2 {
            channel
                .on_new_template(
                    template(template_id, false),
                    coinbase_reward_outputs.clone(),
                )
                .unwrap();
        }

        // activate the future job created under the rotated-out prefix
        let new_prev_hash = SetNewPrevHash {
            template_id: 100,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            header_timestamp: ntime + 600,
            n_bits: 453040064,
            target: [0xff; 32].into(),
        };
        channel.on_set_new_prev_hash(new_prev_hash).unwrap();

        // the active job validates shares under the rotated-out prefix bytes, so its
        // allocator slot must still be reserved
        assert_eq!(
            channel.get_active_job().unwrap().get_extranonce_prefix(),
            prefix_1_bytes.as_slice()
        );
        assert_eq!(allocator.allocated_count(), 2);
        assert!(matches!(
            allocator.allocate_extended(min_rollable_size),
            Err(ExtranonceAllocatorError::CapacityExhausted)
        ));
    }

    #[test]
    fn test_share_validation_version_rolling_not_allowed() {
        // when version rolling is not allowed on the channel,
        // the share version must match the job version exactly
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = false;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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
            251, 175, 106, 40, 35, 87, 122, 90, 58, 51, 78, 32, 202, 236, 228, 36, 154, 174, 206,
            144, 147, 195, 21, 224, 195, 103, 214, 189, 51, 190, 24, 98,
        ]
        .into();
        let n_bits = 545259519;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        channel.set_chain_tip(chain_tip);

        // prepare channel with non-future job
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // the job version is 536870912 (0x20000000)
        // any share version that differs from the job version must be rejected

        // this share flips bit 0, which is outside the BIP323 general-purpose bits mask
        let share_non_rollable_bit = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: 536870913,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_non_rollable_bit);
        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::VersionRollingNotAllowed(_)
        ));

        // this share sets bit 5, which is inside the BIP323 general-purpose bits mask:
        // rolling it is still forbidden when version rolling is not allowed
        let share_rolled_bit = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: 0x20000020,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share_rolled_bit);
        assert!(matches!(
            res.unwrap_err(),
            ShareValidationError::VersionRollingNotAllowed(_)
        ));
    }

    #[test]
    fn test_share_validation_invalid_non_rollable_version_bit() {
        // when version rolling is allowed on the channel,
        // only the BIP323 general-purpose bits may differ from the job version
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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
            251, 175, 106, 40, 35, 87, 122, 90, 58, 51, 78, 32, 202, 236, 228, 36, 154, 174, 206,
            144, 147, 195, 21, 224, 195, 103, 214, 189, 51, 190, 24, 98,
        ]
        .into();
        let n_bits = 545259519;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        channel.set_chain_tip(chain_tip);

        // prepare channel with non-future job
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // the job version is 536870912 (0x20000000)
        // this share flips bit 0, which is outside the BIP323 general-purpose bits
        let share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: 536870913,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        let res = channel.validate_share(share);
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

    #[test]
    fn test_share_validation_rollable_version_bits() {
        // when version rolling is allowed on the channel,
        // shares that only differ in the BIP323 general-purpose bits are accepted
        let channel_id = 1;
        let user_identity = "user_identity".to_string();
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let expected_share_per_minute = 1.0;
        let nominal_hashrate = 1.0;
        let version_rolling_allowed = true;
        let rollable_extranonce_size = 8u16;
        let share_batch_size = 100;

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            nominal_hashrate,
            version_rolling_allowed,
            rollable_extranonce_size,
            share_batch_size,
            expected_share_per_minute,
            None,
            None,
            None,
        )
        .unwrap();

        // force an easy target so that finding a valid share is trivial
        channel.set_target(max_target).unwrap();

        let template_id = 1;
        let template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
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
            251, 175, 106, 40, 35, 87, 122, 90, 58, 51, 78, 32, 202, 236, 228, 36, 154, 174, 206,
            144, 147, 195, 21, 224, 195, 103, 214, 189, 51, 190, 24, 98,
        ]
        .into();
        let n_bits = 545259519;
        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        channel.set_chain_tip(chain_tip);

        // prepare channel with non-future job
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        // the job version is 536870912 (0x20000000)
        // this share version only sets bits 5-20, which are inside the BIP323
        // general-purpose bits mask (0x1FFFFFE0)
        let rolled_version = 0x20000000 | 0x1fffe0;

        // this share has hash
        // 1a489bab0bb696a5833a47cf8b2487c1c3863d01f53d9911fe063250c9f4e282
        // which does meet the channel target
        let share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: 1745596971,
            version: rolled_version,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        assert!(channel.validate_share(share).is_ok());
    }

    fn custom_mining_job(
        channel_id: u32,
        request_id: u32,
        prev_hash: [u8; 32],
        min_ntime: u32,
    ) -> SetCustomMiningJob {
        SetCustomMiningJob {
            channel_id,
            request_id,
            token: vec![request_id as u8].try_into().unwrap(),
            version: 536870912,
            prev_hash: prev_hash.into(),
            min_ntime,
            nbits: 453040064,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_n_sequence: 4294967295,
            coinbase_tx_outputs: vec![
                1u8, 0, 0xf2, 0x05, 0x2a, 0x01, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220, 194, 147,
                204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        }
    }

    #[test]
    fn test_future_template_storage_is_bounded() {
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            4u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();

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

        let flood_size = 10_000u64;
        for template_id in 0..flood_size {
            let template = NewTemplate {
                template_id,
                future_template: true,
                version: 536870912,
                coinbase_tx_version: 2,
                coinbase_prefix: vec![82, 0].try_into().unwrap(),
                coinbase_tx_input_sequence: 4294967295,
                coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
                coinbase_tx_outputs_count: 1,
                coinbase_tx_outputs: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113,
                    209, 222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153,
                    98, 180, 139, 235, 216, 54, 151, 78, 140, 249,
                ]
                .try_into()
                .unwrap(),
                coinbase_tx_locktime: 0,
                merkle_path: vec![].try_into().unwrap(),
            };

            channel
                .on_new_template(template, coinbase_reward_outputs.clone())
                .unwrap();
        }

        // only the newest MAX_FUTURE_JOBS templates survive; the oldest were evicted
        for template_id in 0..flood_size - MAX_FUTURE_JOBS as u64 {
            assert!(channel
                .get_future_job_id_from_template_id(template_id)
                .is_none());
        }
        for template_id in flood_size - MAX_FUTURE_JOBS as u64..flood_size {
            assert!(channel
                .get_future_job_id_from_template_id(template_id)
                .is_some());
        }
    }

    // Builds an extended channel with the given past-jobs cap, feeds it `templates`
    // non-future templates (each retiring the previous active job), and returns how many past
    // jobs survived.
    fn retained_past_jobs(max_past_jobs: Option<usize>, templates: u64) -> usize {
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            4u16,
            100,
            1.0,
            None,
            None,
            max_past_jobs,
        )
        .unwrap();

        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        channel.set_chain_tip(ChainTip::new(prev_hash, 503543726, 1747092633));

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

        for template_id in 0..templates {
            let template = NewTemplate {
                template_id,
                future_template: false,
                version: 536870912,
                coinbase_tx_version: 2,
                coinbase_prefix: vec![82, 0].try_into().unwrap(),
                coinbase_tx_input_sequence: 4294967295,
                coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
                coinbase_tx_outputs_count: 1,
                coinbase_tx_outputs: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113,
                    209, 222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153,
                    98, 180, 139, 235, 216, 54, 151, 78, 140, 249,
                ]
                .try_into()
                .unwrap(),
                coinbase_tx_locktime: 0,
                merkle_path: vec![].try_into().unwrap(),
            };
            channel
                .on_new_template(template, coinbase_reward_outputs.clone())
                .unwrap();
        }

        (0..=templates as u32)
            .filter(|job_id| channel.get_past_job(*job_id).is_some())
            .count()
    }

    #[test]
    fn test_max_past_jobs_override_and_zero_fallback() {
        let templates = MAX_PAST_JOBS as u64 + 2;

        // `None` and `Some(0)` both mean "no opinion" and select the default
        assert_eq!(retained_past_jobs(None, templates), MAX_PAST_JOBS);
        assert_eq!(retained_past_jobs(Some(0), templates), MAX_PAST_JOBS);

        // a real override bounds below the default
        assert_eq!(retained_past_jobs(Some(2), templates), 2);
    }

    #[test]
    fn test_past_job_storage_is_bounded() {
        let channel_id = 1;
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            4u16,
            100,
            1.0,
            None,
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
        channel.set_chain_tip(ChainTip::new(prev_hash, nbits, ntime));

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

        let flood_size = 10_000u64;
        for template_id in 0..flood_size {
            let template = NewTemplate {
                template_id,
                future_template: false,
                version: 536870912,
                coinbase_tx_version: 2,
                coinbase_prefix: vec![82, 0].try_into().unwrap(),
                coinbase_tx_input_sequence: 4294967295,
                coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
                coinbase_tx_outputs_count: 1,
                coinbase_tx_outputs: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113,
                    209, 222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153,
                    98, 180, 139, 235, 216, 54, 151, 78, 140, 249,
                ]
                .try_into()
                .unwrap(),
                coinbase_tx_locktime: 0,
                merkle_path: vec![].try_into().unwrap(),
            };

            channel
                .on_new_template(template, coinbase_reward_outputs.clone())
                .unwrap();
        }

        // each non-future template retires the previous active job; only the newest
        // MAX_PAST_JOBS retired jobs survive
        let retained = (0..=flood_size as u32)
            .filter(|job_id| channel.get_past_job(*job_id).is_some())
            .count();
        assert_eq!(retained, MAX_PAST_JOBS);
        assert!(channel.get_active_job().is_some());

        // target metadata must not outlive the jobs it belongs to: one entry for the active
        // job plus one per retained past job
        assert_eq!(channel.job_id_to_target.len(), MAX_PAST_JOBS + 1);
    }

    #[test]
    fn test_share_validation_ntime_above_max_future_block_time() {
        // Regression test: a share ntime beyond the chain tip's min_ntime +
        // MAX_FUTURE_BLOCK_TIME would put a consensus-invalid timestamp in the block header,
        // so it must be rejected; ntime exactly on the bound is still accepted.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1_000.0, // bigger hashrate to get higher difficulty
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::from(script_bytes),
        }];

        // anchor the tip so the pre-mined share (ntime 1745611105, nonce 16647, from
        // test_share_validation_valid_share) sits exactly on the upper bound
        let share_ntime: u32 = 1745611105;
        let ntime = share_ntime - crate::MAX_FUTURE_BLOCK_TIME;
        let n_bits = 453040064;
        let prev_hash = [
            23, 205, 72, 134, 153, 86, 220, 153, 224, 28, 216, 146, 228, 120, 227, 157, 213, 99,
            160, 163, 128, 59, 139, 190, 158, 62, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        channel.set_chain_tip(ChainTip::new(prev_hash, n_bits, ntime));
        channel
            .on_new_template(template, coinbase_reward_outputs)
            .unwrap();

        let share = |sequence_number: u32, ntime: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id: 1,
            nonce: 16647,
            ntime,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        // one second above the bound: rejected before any PoW evaluation
        let res = channel.validate_share(share(0, share_ntime + 1));
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));

        // u32::MAX is likewise rejected (the bound saturates instead of wrapping)
        let res = channel.validate_share(share(1, u32::MAX));
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));

        // exactly on the bound: the pre-mined share is accepted
        let res = channel.validate_share(share(2, share_ntime));
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));
    }

    #[test]
    fn test_share_validation_ntime_below_group_job_min_ntime() {
        // A job installed via on_group_channel_job carries the group job's own min_ntime, which
        // is later than this channel's tip if the application fans the group job out before
        // updating the channel's chain tip. A share in the gap
        // (chain_tip.min_ntime <= ntime < job.min_ntime) must be rejected.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            1.0,
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();
        // permissive channel target, so that acceptance only hinges on the nTime bounds
        channel.set_target(max_target).unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::from(script_bytes),
        }];

        // network target: 000000000000d7c0... (hard, so no accidental BlockFound)
        let n_bits = 453040064;
        let prev_hash: U256 = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let tip_ntime = 1745596910;
        channel.set_chain_tip(ChainTip::new(prev_hash.clone(), n_bits, tip_ntime));

        // the group channel had already advanced to a later tip when it built this job
        let job_min_ntime = tip_ntime + 3;
        let mut group_job_factory = crate::server::jobs::factory::JobFactory::new(true, None, None);
        let group_job = group_job_factory
            .new_extended_job(
                99,
                Some(ChainTip::new(prev_hash, n_bits, job_min_ntime)),
                vec![],
                template,
                coinbase_reward_outputs,
                channel.get_full_extranonce_size(),
            )
            .unwrap();
        assert_eq!(group_job.get_min_ntime(), Some(job_min_ntime));
        channel.on_group_channel_job(group_job).unwrap();
        let job_id = channel.get_active_job().unwrap().get_job_id();

        let share = |sequence_number: u32, ntime: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id,
            nonce: 0,
            ntime,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };

        // a share in the gap (at or above the tip's min_ntime, below the job's) is rejected
        let res = channel.validate_share(share(0, job_min_ntime - 1));
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));
        assert_eq!(channel.get_share_accounting().get_shares_accepted(), 0);

        // the upper bound is anchored at the job's min_ntime as well: one second past it is
        // rejected, exactly on it is accepted
        let res =
            channel.validate_share(share(2, job_min_ntime + crate::MAX_FUTURE_BLOCK_TIME + 1));
        assert!(matches!(res.unwrap_err(), ShareValidationError::Invalid(_)));
        let res = channel.validate_share(share(3, job_min_ntime + crate::MAX_FUTURE_BLOCK_TIME));
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));

        // at the job's min_ntime the share is accepted (channel target is permissive)
        let res = channel.validate_share(share(1, job_min_ntime));
        assert!(matches!(res, Ok(ShareValidationResult::Valid(_))));
    }

    #[test]
    fn test_min_ntime_only_custom_job_keeps_seen_shares() {
        // min_ntime is only a lower bound on a share's ntime and job IDs are not committed into
        // the block header, so a custom job that keeps prev_hash (and nbits) and only advances
        // min_ntime commits to the same header space as its predecessor. The accepted-share
        // hashes must survive it, or the same proof becomes creditable again under the new job
        // ID.
        let channel_id = 1;
        let max_target = Target::from_le_bytes([0xff; 32]);
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![1, 2, 3, 4]).unwrap(),
            max_target,
            100.0,
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();
        // permissive channel target, so that the share is accepted under both jobs
        channel.set_target(max_target).unwrap();

        let prev_hash = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ];
        let min_ntime = 1745596910;
        let first_job_id = channel
            .on_set_custom_mining_job(custom_mining_job(channel_id, 1, prev_hash, min_ntime))
            .unwrap();

        let share = |sequence_number: u32, job_id: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id,
            nonce: 0,
            ntime: min_ntime + 1,
            version: 536870912,
            extranonce: vec![0; 8].try_into().unwrap(),
        };
        assert!(matches!(
            channel.validate_share(share(0, first_job_id)),
            Ok(ShareValidationResult::Valid(_))
        ));

        // same prev_hash, nbits and coinbase: only min_ntime advances, and the share's ntime
        // still meets it
        let second_job_id = channel
            .on_set_custom_mining_job(custom_mining_job(channel_id, 2, prev_hash, min_ntime + 1))
            .unwrap();
        assert_ne!(first_job_id, second_job_id);
        // a tip-field change still stales the previous job; only the dedup flush is keyed to
        // prev_hash
        assert!(channel.job_store.get_stale_job(first_job_id).is_some());

        // the identical proof under the new job ID is a duplicate, not a second credit
        assert!(matches!(
            channel.validate_share(share(1, second_job_id)),
            Err(ShareValidationError::DuplicateShare(_))
        ));
        assert_eq!(channel.get_share_accounting().get_shares_accepted(), 1);
    }

    #[test]
    fn test_zero_max_target_is_rejected() {
        // a zero max_target is refused at open, on update and on set_target, leaving the
        // channel unchanged (see OpenChannelInvalidMaxTarget)
        let res = ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![1, 2, 3, 4]).unwrap(),
            Target::ZERO,
            1.0,
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        );
        match res {
            Err(ExtendedChannelError::OpenChannelInvalidMaxTarget(code)) => {
                assert_eq!(code, ERROR_CODE_OPEN_MINING_CHANNEL_MAX_TARGET_OUT_OF_RANGE);
            }
            other => panic!("expected OpenChannelInvalidMaxTarget, got {other:?}"),
        }

        let max_target = Target::from_le_bytes([0xff; 32]);
        let nominal_hashrate = 1.0;
        let mut channel = ExtendedChannel::new(
            1,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![1, 2, 3, 4]).unwrap(),
            max_target,
            nominal_hashrate,
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();
        let target_before = *channel.get_target();

        let res = channel.update_channel(100.0, Some(Target::ZERO));
        match res {
            Err(ExtendedChannelError::UpdateChannelInvalidMaxTarget(code)) => {
                assert_eq!(code, ERROR_CODE_UPDATE_CHANNEL_MAX_TARGET_OUT_OF_RANGE);
            }
            other => panic!("expected UpdateChannelInvalidMaxTarget, got {other:?}"),
        }
        assert!(matches!(
            channel.set_target(Target::ZERO),
            Err(ExtendedChannelError::InvalidTarget)
        ));

        // the channel is left unchanged
        assert_eq!(channel.get_target(), &target_before);
        assert_eq!(channel.get_requested_max_target(), &max_target);
        assert_eq!(channel.get_nominal_hashrate(), nominal_hashrate);
    }

    #[test]
    fn test_on_group_channel_job_rejects_min_ntime_below_chain_tip() {
        // A non-future job installed via on_group_channel_job is mined against this channel's
        // chain tip, whose min_ntime is the smallest nTime available for it, so a group job built
        // under an older tip (a lower min_ntime) must be refused and leave the channel unchanged.
        // A min_ntime equal to the tip's is the lowest allowed.
        let channel_id = 1;
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
            ])
            .unwrap(),
            Target::from_le_bytes([0xff; 32]),
            1.0,
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::from(script_bytes),
        }];

        let n_bits = 453040064;
        let prev_hash: U256 = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let tip_ntime = 1745596910;
        channel.set_chain_tip(ChainTip::new(prev_hash.clone(), n_bits, tip_ntime));

        let full_extranonce_size = channel.get_full_extranonce_size();
        let mut group_job_factory = crate::server::jobs::factory::JobFactory::new(true, None, None);
        let mut group_job = |min_ntime: u32| {
            group_job_factory
                .new_extended_job(
                    99,
                    Some(ChainTip::new(prev_hash.clone(), n_bits, min_ntime)),
                    vec![],
                    template.clone(),
                    coinbase_reward_outputs.clone(),
                    full_extranonce_size,
                )
                .unwrap()
        };

        assert!(matches!(
            channel.on_group_channel_job(group_job(tip_ntime - 1)),
            Err(ExtendedChannelError::JobMinNtimeBelowChainTip)
        ));
        assert!(channel.get_active_job().is_none());

        channel.on_group_channel_job(group_job(tip_ntime)).unwrap();
        assert!(channel.get_active_job().is_some());
    }

    #[test]
    fn test_group_job_reusing_a_stale_job_id_replaces_the_stale_job() {
        // Job IDs come from whichever factory built the job: the channel's own factory at open,
        // the group channel's afterwards. A group job can thus arrive under the ID of a job that
        // went stale on the last tip transition; the stale namesake is dropped and shares for
        // the ID validate against the live job.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            1.0,
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();
        // permissive channel target, so that acceptance only hinges on job resolution
        channel.set_target(max_target).unwrap();

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::from(script_bytes),
        }];

        // network target: 000000000000d7c0... (hard, so no accidental BlockFound)
        let n_bits = 453040064;
        let tip_ntime = 1745596910;
        let prev_hash: U256 = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        channel.set_chain_tip(ChainTip::new(prev_hash, n_bits, tip_ntime));

        // the channel's own factory mints job 1
        channel
            .on_new_template(template.clone(), coinbase_reward_outputs.clone())
            .unwrap();
        assert_eq!(channel.get_active_job().unwrap().get_job_id(), 1);

        // a tip transition with no future job queued retires it as stale
        let next_prev_hash: U256 = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        let next_tip_ntime = tip_ntime + 600;
        channel
            .on_set_new_prev_hash(SetNewPrevHash {
                template_id: 999,
                prev_hash: next_prev_hash.clone(),
                header_timestamp: next_tip_ntime,
                n_bits,
                target: [0xff; 32].into(),
            })
            .unwrap();
        let share = |sequence_number: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id: 1,
            nonce: 0,
            ntime: next_tip_ntime,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };
        assert!(matches!(
            channel.validate_share(share(0)),
            Err(ShareValidationError::Stale(_))
        ));

        // a fresh group factory mints its job 1 under the new tip
        let mut group_job_factory = crate::server::jobs::factory::JobFactory::new(true, None, None);
        let group_job = group_job_factory
            .new_extended_job(
                99,
                Some(ChainTip::new(next_prev_hash, n_bits, next_tip_ntime)),
                vec![],
                template,
                coinbase_reward_outputs,
                channel.get_full_extranonce_size(),
            )
            .unwrap();
        assert_eq!(group_job.get_job_id(), 1);
        channel.on_group_channel_job(group_job).unwrap();

        // ID 1 names the group job only: shares for it are validated, not rejected as stale
        assert_eq!(channel.get_active_job().unwrap().get_job_id(), 1);
        assert!(matches!(
            channel.validate_share(share(1)),
            Ok(ShareValidationResult::Valid(_))
        ));
    }

    #[test]
    fn test_group_job_looser_than_the_channel_version_rolling_policy_is_rejected() {
        // The job's flag is what the miner is told and what validate_share enforces, so a group
        // job permitting version rolling must not enter a channel that forbids it, future or not;
        // a job as strict as the channel is imported.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            1.0,
            false,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();

        let template = |template_id: u64, future_template: bool| NewTemplate {
            template_id,
            future_template,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::from(script_bytes),
        }];

        let n_bits = 453040064;
        let prev_hash: U256 = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let tip_ntime = 1745596910;
        channel.set_chain_tip(ChainTip::new(prev_hash.clone(), n_bits, tip_ntime));

        // a group built with rolling allowed feeds a channel that forbids it
        let mut looser_group_factory =
            crate::server::jobs::factory::JobFactory::new(true, None, None);
        let active_job = looser_group_factory
            .new_extended_job(
                99,
                Some(ChainTip::new(prev_hash.clone(), n_bits, tip_ntime)),
                vec![],
                template(1, false),
                coinbase_reward_outputs.clone(),
                channel.get_full_extranonce_size(),
            )
            .unwrap();
        assert!(active_job.version_rolling_allowed());
        assert!(matches!(
            channel.on_group_channel_job(active_job),
            Err(ExtendedChannelError::GroupJobVersionRollingNotAllowed)
        ));
        assert!(channel.get_active_job().is_none());

        let future_job = looser_group_factory
            .new_extended_job(
                99,
                None,
                vec![],
                template(2, true),
                coinbase_reward_outputs.clone(),
                channel.get_full_extranonce_size(),
            )
            .unwrap();
        assert!(matches!(
            channel.on_group_channel_job(future_job),
            Err(ExtendedChannelError::GroupJobVersionRollingNotAllowed)
        ));
        assert!(channel.get_future_job_id_from_template_id(2).is_none());

        // a group with the channel's own policy is imported
        let mut group_factory = crate::server::jobs::factory::JobFactory::new(false, None, None);
        let active_job = group_factory
            .new_extended_job(
                99,
                Some(ChainTip::new(prev_hash, n_bits, tip_ntime)),
                vec![],
                template(3, false),
                coinbase_reward_outputs,
                channel.get_full_extranonce_size(),
            )
            .unwrap();
        channel.on_group_channel_job(active_job).unwrap();
        assert!(!channel.get_active_job().unwrap().version_rolling_allowed());
    }

    #[test]
    fn test_repeated_prev_hash_keeps_seen_shares() {
        // Template and job IDs are not committed into the block header, so a non-conforming
        // Template Provider can queue an identical future template under a new template_id and
        // repeat the same SetNewPrevHash. The replacement job commits to the same headers as
        // the previous one, so the accepted-share hashes must survive the repeated tip or the
        // same proof becomes creditable again under the new job_id.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            1.0,
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            None,
        )
        .unwrap();
        // permissive channel target, so that the share is accepted under both jobs
        channel.set_target(max_target).unwrap();

        let template = |template_id: u64| NewTemplate {
            template_id,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::from(script_bytes),
        }];

        // network target: 000000000000d7c0... (hard, so no accidental BlockFound)
        let prev_hash: U256 = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        let set_new_prev_hash = |template_id: u64| SetNewPrevHash {
            template_id,
            prev_hash: prev_hash.clone(),
            header_timestamp: 1745596910,
            n_bits: 453040064,
            target: [0xff; 32].into(),
        };

        channel
            .on_new_template(template(1), coinbase_reward_outputs.clone())
            .unwrap();
        channel.on_set_new_prev_hash(set_new_prev_hash(1)).unwrap();
        let first_job_id = channel.get_active_job().unwrap().get_job_id();

        let share = |sequence_number: u32, job_id: u32| SubmitSharesExtended {
            channel_id,
            sequence_number,
            job_id,
            nonce: 0,
            ntime: 1745596910,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };
        assert!(matches!(
            channel.validate_share(share(0, first_job_id)),
            Ok(ShareValidationResult::Valid(_))
        ));

        // the peer repeats the tip under a new template_id: same header space, new job_id
        channel
            .on_new_template(template(2), coinbase_reward_outputs)
            .unwrap();
        channel.on_set_new_prev_hash(set_new_prev_hash(2)).unwrap();
        let second_job_id = channel.get_active_job().unwrap().get_job_id();
        assert_ne!(first_job_id, second_job_id);

        // the identical proof under the new job_id is a duplicate, not a second credit
        assert!(matches!(
            channel.validate_share(share(1, second_job_id)),
            Err(ShareValidationError::DuplicateShare(_))
        ));
        assert_eq!(channel.get_share_accounting().get_shares_accepted(), 1);
    }

    #[test]
    fn test_reused_job_id_evicted_from_past_jobs_keeps_the_active_job_target() {
        // The channel's own factory and its group channel's both count from 1, so the job the
        // channel's factory mints can share its ID with the past job that its installation
        // evicts. The target mapping of the new job must survive that eviction, or every share
        // for the current job is rejected as InvalidJobId until the next job arrives.
        let channel_id = 1;
        let extranonce_prefix = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();
        let max_target = Target::from_le_bytes([0xff; 32]);
        let mut channel = ExtendedChannel::new(
            channel_id,
            "user_identity".to_string(),
            ExtranoncePrefix::from_wire(extranonce_prefix).unwrap(),
            max_target,
            1.0,
            true,
            8u16,
            100,
            1.0,
            None,
            None,
            Some(2),
        )
        .unwrap();
        // permissive channel target, so that acceptance only hinges on job resolution
        channel.set_target(max_target).unwrap();

        let template = |template_id: u64| NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::from(script_bytes),
        }];

        // network target: 000000000000d7c0... (hard, so no accidental BlockFound)
        let n_bits = 453040064;
        let tip_ntime = 1745596910;
        let prev_hash: U256 = [
            154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73, 34, 0,
            162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
        ]
        .into();
        channel.set_chain_tip(ChainTip::new(prev_hash.clone(), n_bits, tip_ntime));

        // group jobs 1, 2 and 3 fill the channel: 3 is active, 1 and 2 are past
        let mut group_job_factory = crate::server::jobs::factory::JobFactory::new(true, None, None);
        for template_id in 1..=3 {
            let group_job = group_job_factory
                .new_extended_job(
                    99,
                    Some(ChainTip::new(prev_hash.clone(), n_bits, tip_ntime)),
                    vec![],
                    template(template_id),
                    coinbase_reward_outputs.clone(),
                    channel.get_full_extranonce_size(),
                )
                .unwrap();
            channel.on_group_channel_job(group_job).unwrap();
        }
        assert_eq!(channel.get_active_job().unwrap().get_job_id(), 3);

        // the channel's own factory mints job 1; installing it evicts group job 1
        channel
            .on_new_template(template(4), coinbase_reward_outputs)
            .unwrap();
        assert_eq!(channel.get_active_job().unwrap().get_job_id(), 1);

        let share = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 0,
            ntime: tip_ntime,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
        };
        assert!(matches!(
            channel.validate_share(share),
            Ok(ShareValidationResult::Valid(_))
        ));
    }
}
