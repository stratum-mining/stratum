//! Sv2 Group Channel - Mining Server Abstraction.
//!
//! This module defines the [`GroupChannel`] struct, which provides an abstraction of a Stratum V2
//! (SV2) group channel as maintained by a mining server.
//!
//! A group channel represents a logical grouping of standard and extended channels, allowing multiple mining
//! entities to share jobs. It manages job distribution and activation for all
//! associated channels, but delegates share validation and accounting to those channels.
//!
//! ## Responsibilities
//!
//! `GroupChannel` is responsible for managing the state associated with an SV2 group channel,
//! including:
//!
//! - **Group Channel ID**: Holds the unique `group_channel_id`.
//! - **Channel Management**: Tracks the set of associated channel IDs, allowing
//!   for dynamic addition and removal.
//! - **Job Factory and Store**: Manages creation and storage of jobs (future and active) using the
//!   job factory and job store abstractions.
//! - **Job Lifecycle Management**: Stores jobs built from new templates or handed over
//!   prebuilt (see [`PrebuiltJob`]), including:
//!   - Future jobs (indexed by `job_id`, and by `template_id` when built from a template)
//!   - Active job (currently being mined)
//! - **Chain Tip Management**: Tracks the latest known chain tip (block height, previous hash,
//!   timestamp, and target) for constructing headers and activating jobs.
//!
//! ## Notes
//!
//! - Share validation and accounting is handled at the channel level, not in the group
//!   channel.
//! - Past and stale jobs are not tracked in this abstraction.
//! - Extranonce prefix management is deferred to channels; group jobs use an empty prefix.

use crate::{
    chain_tip::ChainTip,
    server::{
        error::GroupChannelError,
        jobs::{
            extended::ExtendedJob,
            factory::JobFactory,
            job_store::{JobStore, MAX_PAST_JOBS},
            PrebuiltJob,
        },
    },
};
use bitcoin::transaction::TxOut;
use std::collections::HashSet;
use template_distribution_sv2::{NewTemplateOwned, SetNewPrevHashOwned as SetNewPrevHashTdp};
use tracing::warn;

/// Abstraction of a Group Channel.
///
/// It keeps track of:
/// - the group channel's unique `group_channel_id`
/// - the group channel's `channels` (indexed by `channel_id`)
/// - the group channel's job factory
/// - the group channel's future jobs (indexed by `job_id`, and by `template_id` when built from
///   a template; activated by `on_set_new_prev_hash` or `activate_future_job`)
/// - the group channel's active job
/// - the group channel's chain tip
/// - the group channel's full extranonce size
///
/// Since share validation happens at the Channel level, we don't really keep track of:
/// - the group channel's past jobs
/// - the group channel's stale jobs
/// - the group channel's share validation state
#[derive(Debug)]
pub struct GroupChannel {
    group_channel_id: u32,
    channel_ids: HashSet<u32>,
    job_factory: JobFactory,
    job_store: JobStore<ExtendedJob>,
    chain_tip: Option<ChainTip>,
    full_extranonce_size: usize,
}

impl GroupChannel {
    /// Constructor of `GroupChannel` for a Sv2 Pool Server.
    /// Not meant for usage on a Sv2 Job Declaration Client.
    ///
    /// Initializes the group channel state with the provided group channel ID.
    /// `version_rolling_allowed` is the version-rolling policy the group's jobs advertise; it must
    /// be the policy of the extended channels the jobs are imported into, which refuse a looser
    /// one (see [`ExtendedChannel::on_group_channel_job`](super::extended::ExtendedChannel::on_group_channel_job)).
    ///
    /// For non-JD jobs, `pool_tag_string` is added to the coinbase scriptSig as
    /// `Sv2/pool_tag_string//`.
    ///
    /// Returns [`GroupChannelError::ScriptSigSizeTooLarge`] if the tags, the delimiters, the
    /// extranonce and a worst-case coinbase prefix do not fit within the coinbase `scriptSig`
    /// budget, see [`JobFactory::fits_script_sig_budget`].
    pub fn new_for_pool(
        group_channel_id: u32,
        full_extranonce_size: usize,
        version_rolling_allowed: bool,
        pool_tag_string: String,
    ) -> Result<Self, GroupChannelError> {
        let group_channel = Self::new(
            group_channel_id,
            full_extranonce_size,
            version_rolling_allowed,
            Some(pool_tag_string),
            None,
        )?;
        Ok(group_channel)
    }

    /// Constructor of `GroupChannel` for a Sv2 Job Declaration Client.
    /// Not meant for usage on a Sv2 Pool Server.
    ///
    /// Initializes the extended channel state with the provided parameters, including channel
    /// identifiers, difficulty targets, share accounting, and job management.
    /// Returns an error if target/difficulty parameters are invalid or extranonce prefix
    /// requirements are not met.
    ///
    /// `version_rolling_allowed` is the version-rolling policy the group's jobs advertise; it must
    /// be the policy of the extended channels the jobs are imported into, which refuse a looser
    /// one (see [`ExtendedChannel::on_group_channel_job`](super::extended::ExtendedChannel::on_group_channel_job)).
    ///
    /// The `pool_tag_string` and `miner_tag_string` are added to the coinbase scriptSig as
    /// `Sv2/pool_tag_string/miner_tag_string/`.
    ///
    /// Returns [`GroupChannelError::ScriptSigSizeTooLarge`] if the tags, the delimiters, the
    /// extranonce and a worst-case coinbase prefix do not fit within the coinbase `scriptSig`
    /// budget, see [`JobFactory::fits_script_sig_budget`].
    pub fn new_for_job_declaration_client(
        group_channel_id: u32,
        full_extranonce_size: usize,
        version_rolling_allowed: bool,
        pool_tag_string: Option<String>,
        miner_tag_string: String,
    ) -> Result<Self, GroupChannelError> {
        let group_channel = Self::new(
            group_channel_id,
            full_extranonce_size,
            version_rolling_allowed,
            pool_tag_string,
            Some(miner_tag_string),
        )?;
        Ok(group_channel)
    }

    // private constructor
    fn new(
        group_channel_id: u32,
        full_extranonce_size: usize,
        version_rolling_allowed: bool,
        pool_tag: Option<String>,
        miner_tag: Option<String>,
    ) -> Result<Self, GroupChannelError> {
        let job_factory = JobFactory::new(version_rolling_allowed, pool_tag, miner_tag);

        // conservative check against the spec's worst-case `NewTemplate::coinbase_prefix`.
        // the exact size is re-checked against each actual template in `JobFactory::coinbase`
        if !job_factory.fits_script_sig_budget(full_extranonce_size) {
            return Err(GroupChannelError::ScriptSigSizeTooLarge);
        }

        Ok(Self {
            group_channel_id,
            channel_ids: HashSet::new(),
            job_factory,
            // group channels never validate shares, so they replace the active job rather than
            // retiring it into past jobs (see `JobStore::replace_active_job`). The cap is
            // therefore inert here and needs no constructor parameter.
            job_store: JobStore::new(MAX_PAST_JOBS),
            chain_tip: None,
            full_extranonce_size,
        })
    }

    /// Adds a channel ID to this group channel. Also takes the `full_extranonce_size` of the channel to be added.
    ///
    /// Returns an error if the provided `full_extranonce_size` doesn't match the group channel's `full_extranonce_size`.
    pub fn add_channel_id(
        &mut self,
        channel_id: u32,
        full_extranonce_size: usize,
    ) -> Result<(), GroupChannelError> {
        if self.full_extranonce_size != full_extranonce_size {
            return Err(GroupChannelError::FullExtranonceSizeMismatch);
        }

        self.channel_ids.insert(channel_id);
        Ok(())
    }

    /// Removes a channel ID from this group channel.
    pub fn remove_channel_id(&mut self, channel_id: u32) {
        self.channel_ids.remove(&channel_id);
    }

    /// Returns the unique group channel ID for this group channel.
    pub fn get_group_channel_id(&self) -> u32 {
        self.group_channel_id
    }

    /// Set the full extranonce size for this group channel.
    /// Also clears all channel IDs, as no channels can belong to the same group while having different `full_extranonce_size`s.
    ///
    /// Returns [`GroupChannelError::ScriptSigSizeTooLarge`] if the new size would push the
    /// assembled coinbase `scriptSig` past its budget (see
    /// [`JobFactory::fits_script_sig_budget`]), leaving the group channel unchanged.
    pub fn set_full_extranonce_size(
        &mut self,
        full_extranonce_size: usize,
    ) -> Result<(), GroupChannelError> {
        // re-run the constructor's invariant, before touching any state
        if !self
            .job_factory
            .fits_script_sig_budget(full_extranonce_size)
        {
            return Err(GroupChannelError::ScriptSigSizeTooLarge);
        }

        if self.full_extranonce_size != full_extranonce_size {
            self.channel_ids.clear();
        }

        self.full_extranonce_size = full_extranonce_size;

        Ok(())
    }

    pub fn get_full_extranonce_size(&self) -> usize {
        self.full_extranonce_size
    }

    /// Returns an iterator over channel IDs associated with this group channel.
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

    /// Returns the current chain tip, if set.
    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Only for testing purposes, not meant to be used in real apps.
    #[cfg(test)]
    pub fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    /// Returns a reference to the currently active job, if any.
    pub fn get_active_job(&self) -> Option<&ExtendedJob> {
        self.job_store.get_active_job()
    }

    /// Returns the job ID for a future job from a template ID, if any.
    pub fn get_future_job_id_from_template_id(&self, template_id: u64) -> Option<u32> {
        self.job_store
            .get_future_job_id_from_template_id(template_id)
    }

    /// Returns a reference to a future job from its job ID, if any.
    pub fn get_future_job(&self, job_id: u32) -> Option<&ExtendedJob> {
        self.job_store.get_future_job(job_id)
    }

    /// Updates the group channel state with a new template.
    ///
    /// If the template is a future template, the chain tip is not used. At most
    /// `MAX_FUTURE_JOBS` (16) future jobs are kept: storing a new one beyond that limit evicts
    /// the oldest.
    /// If the template is not a future template, the chain tip must be set, and the new job
    /// replaces the active job. The replaced job is dropped: group channels never validate
    /// shares, so no past-job history is kept.
    /// Returns an error if a non-future job cannot be created due to missing chain tip.
    ///
    /// Returns [`GroupChannelError::JobFactoryError`] wrapping
    /// [`JobFactoryError::ScriptSigSizeTooLarge`](crate::server::jobs::error::JobFactoryError::ScriptSigSizeTooLarge)
    /// if the template's `coinbase_prefix` pushes the assembled coinbase `scriptSig` past its
    /// budget. The constructor can only check against the spec's worst-case prefix (see
    /// [`JobFactory::fits_script_sig_budget`]), so this is where an out-of-spec Template Provider
    /// is caught.
    pub fn on_new_template(
        &mut self,
        template: NewTemplateOwned,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<(), GroupChannelError> {
        match template.future_template {
            true => {
                let new_job = self
                    .job_factory
                    .new_extended_job(
                        self.group_channel_id,
                        None,
                        vec![], /* empty extranonce prefix, as it will be replaced by the
                                 * channel's extranonce prefix */
                        template.clone(),
                        coinbase_reward_outputs,
                        self.full_extranonce_size,
                    )
                    .map_err(GroupChannelError::JobFactoryError)?;
                self.job_store
                    .add_future_job(Some(template.template_id), new_job);
            }
            false => {
                match self.chain_tip.clone() {
                    // we can only create non-future jobs if we have a chain tip
                    None => return Err(GroupChannelError::ChainTipNotSet),
                    Some(chain_tip) => {
                        let new_job = self
                            .job_factory
                            .new_extended_job(
                                self.group_channel_id,
                                Some(chain_tip),
                                vec![], /* empty extranonce prefix, as it will be replaced by
                                         * the channel's extranonce prefix */
                                template.clone(),
                                coinbase_reward_outputs,
                                self.full_extranonce_size,
                            )
                            .map_err(GroupChannelError::JobFactoryError)?;
                        // group channels never validate shares, so the replaced active job is
                        // dropped instead of being retained as a past job
                        self.job_store.replace_active_job(new_job);
                    }
                }
            }
        }
        Ok(())
    }

    /// Stores a [`PrebuiltJob`].
    ///
    /// The job is broadcast to the group as the `NewExtendedMiningJob` (§5.3.16) carrying the
    /// given bytes, with the group's version-rolling policy and no extranonce prefix (the member
    /// channels set theirs when they import it through their `on_group_channel_job`). With
    /// `future` set, the job is kept for activation by
    /// [`activate_future_job`](Self::activate_future_job); otherwise it requires the chain tip,
    /// is active under it and replaces the active job, which is dropped since group channels
    /// never validate shares.
    ///
    /// Returns the job ID, which identifies the job to the miners and to
    /// [`activate_future_job`](Self::activate_future_job);
    /// [`GroupChannelError::ChainTipNotSet`] for an active job without a chain tip; and
    /// [`GroupChannelError::JobFactoryError`] wrapping
    /// [`JobFactoryError::InvalidPrebuiltCoinbase`](crate::server::jobs::error::JobFactoryError::InvalidPrebuiltCoinbase)
    /// if the bytes around the group's extranonce slot do not deserialize as a coinbase
    /// transaction. The group channel is left unchanged in both error cases.
    pub fn on_prebuilt_job(
        &mut self,
        job: PrebuiltJob,
        future: bool,
    ) -> Result<u32, GroupChannelError> {
        let chain_tip = match future {
            true => None,
            false => Some(
                self.chain_tip
                    .clone()
                    .ok_or(GroupChannelError::ChainTipNotSet)?,
            ),
        };
        let new_job = self
            .job_factory
            .new_extended_job_from_prebuilt_job(
                self.group_channel_id,
                chain_tip,
                vec![], /* empty extranonce prefix, as it will be replaced by the channel's
                         * extranonce prefix */
                job,
                self.full_extranonce_size,
            )
            .map_err(GroupChannelError::JobFactoryError)?;
        let job_id = new_job.get_job_id();
        match new_job.is_future() {
            true => {
                self.job_store.add_future_job(None, new_job);
            }
            // group channels never validate shares, so the replaced active job is dropped
            // instead of being retained as a past job
            false => self.job_store.replace_active_job(new_job),
        }
        Ok(job_id)
    }

    /// Activates the future job `job_id` under `chain_tip`, the way
    /// [`on_set_new_prev_hash`](Self::on_set_new_prev_hash) activates the job of a template: the
    /// group's `SetNewPrevHash` (§5.3.17) names the job by `job_id` and carries the tip. For a
    /// [`PrebuiltJob`] this is the only way to activate
    /// it, as no Template Distribution message refers to it.
    ///
    /// The previously active job and the other future jobs are dropped, since group channels
    /// never validate shares, and the chain tip is updated. Returns
    /// [`GroupChannelError::JobIdNotFound`] if no future job carries `job_id`, leaving the group
    /// channel unchanged.
    pub fn activate_future_job(
        &mut self,
        job_id: u32,
        chain_tip: ChainTip,
    ) -> Result<(), GroupChannelError> {
        if !self
            .job_store
            .activate_future_job_replacing_active(job_id, chain_tip.ntime_start())
        {
            return Err(GroupChannelError::JobIdNotFound);
        }
        self.chain_tip = Some(chain_tip);
        Ok(())
    }

    /// Updates the group channel state with a new [`SetNewPrevHash`](SetNewPrevHashTdp) message
    /// (Template Distribution Protocol variant).
    ///
    /// If there is a future job matching the `template_id` specified in `SetNewPrevHash`,
    /// this future job is "activated" and set as the active job, see
    /// [`activate_future_job`](Self::activate_future_job). The previously active job is
    /// dropped: group channels never validate shares, so no past or stale job history is kept.
    ///
    /// If no future jobs are queued, the peer is not conforming to the Template Distribution
    /// Protocol, which requires at least one future `NewTemplate` before every `SetNewPrevHash`.
    /// The message is still applied rather than rejected: its chain-tip fields are self-contained
    /// and remain usable, so discarding them would only leave this channel unable to recover. The
    /// active job (if any) is dropped, since it commits to the previous chain tip.
    ///
    /// Updates the chain tip for the group channel.
    /// Returns an error if future jobs are queued but none matches the `template_id`, leaving
    /// the chain tip untouched.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashTdp,
    ) -> Result<(), GroupChannelError> {
        if !self.job_store.has_future_jobs() {
            // a chain-tip update with no queued future template means the peer broke
            // the protocol, but the tip itself is still usable, so recover instead of
            // wedging the channel. the active job committed to the previous chain tip,
            // and group channels never validate shares against stored jobs, so it is
            // dropped outright rather than retired to stale.
            warn!(
                "SetNewPrevHash with no queued future template: non-conforming Template \
                 Distribution peer, recovering the chain tip"
            );
            self.job_store.clear_active_job();
            self.chain_tip = Some(set_new_prev_hash.into());
            return Ok(());
        }

        // the chain tip must only advance once we know a job was actually activated
        let job_id = self
            .job_store
            .get_future_job_id_from_template_id(set_new_prev_hash.template_id)
            .ok_or(GroupChannelError::TemplateIdNotFound)?;
        self.activate_future_job(job_id, set_new_prev_hash.into())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        chain_tip::ChainTip,
        server::{
            error::GroupChannelError,
            group::GroupChannel,
            jobs::{
                error::JobFactoryError,
                factory::{MAX_COINBASE_PREFIX_SIZE, MAX_SCRIPT_SIG_SIZE},
                job_store::MAX_FUTURE_JOBS,
                JobOrigin, PrebuiltJob,
            },
        },
    };
    use binary_sv2::Sv2OptionOwned as Sv2Option;
    use bitcoin::{transaction::TxOut, Amount, ScriptBuf};
    use mining_sv2::NewExtendedMiningJobOwned as NewExtendedMiningJob;
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
        let group_channel_id = 1;
        let full_extranonce_size = 32;
        let mut group_channel =
            GroupChannel::new(group_channel_id, full_extranonce_size, true, None, None).unwrap();

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

        assert!(!group_channel.job_store.has_future_jobs());
        group_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();
        assert!(group_channel.get_active_job().is_none());

        let future_job_id = group_channel
            .get_future_job_id_from_template_id(template.template_id)
            .unwrap();

        let future_job = group_channel.get_future_job(future_job_id).unwrap().clone();

        // we know that the provided template + coinbase_reward_outputs should generate this future
        // job
        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            ntime_start: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 42, 82, 0, 6, 83, 118, 50, 47, 47,
                47, 32,
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
            ntime_start: ntime,
            n_bits: 503543726,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                174, 119, 3, 0, 0,
            ]
            .into(),
        };

        group_channel
            .on_set_new_prev_hash(set_new_prev_hash)
            .unwrap();

        // we just activated the only future job
        assert!(group_channel.get_active_job().is_some());

        let mut previously_future_job = future_job.clone();
        previously_future_job.activate(ntime);

        let activated_job = group_channel.get_active_job().unwrap();

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
        let group_channel_id = 1;
        let full_extranonce_size = 32;
        let mut group_channel =
            GroupChannel::new(group_channel_id, full_extranonce_size, true, None, None).unwrap();

        let ntime = 1746839905;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        let n_bits = 503543726;

        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
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

        group_channel.set_chain_tip(chain_tip);
        group_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let active_job = group_channel.get_active_job().unwrap();

        // we know that the provided template + coinbase_reward_outputs should generate this
        // non-future job
        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            ntime_start: Sv2Option::new(Some(ntime)),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 42, 82, 0, 6, 83, 118, 50, 47, 47,
                47, 32,
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
        let group_channel_id = 1;
        let full_extranonce_size = 32;
        let mut group_channel =
            GroupChannel::new(group_channel_id, full_extranonce_size, true, None, None).unwrap();

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

        assert!(group_channel
            .on_new_template(template.clone(), invalid_coinbase_reward_outputs)
            .is_err());

        assert!(!group_channel.job_store.has_future_jobs());
    }

    #[test]
    fn test_add_channel_id() {
        let group_channel_id = 1;
        let full_extranonce_size = 32;
        let mut group_channel =
            GroupChannel::new(group_channel_id, full_extranonce_size, true, None, None).unwrap();

        // add a first channel with the correct full extranonce size
        group_channel
            .add_channel_id(1, full_extranonce_size)
            .unwrap();
        assert_eq!(group_channel.get_channel_ids_count(), 1);
        assert!(group_channel.has_channel_id(1));
        assert_eq!(
            group_channel.get_full_extranonce_size(),
            full_extranonce_size
        );

        // add a second channel with the correct full extranonce size
        group_channel
            .add_channel_id(2, full_extranonce_size)
            .unwrap();
        assert_eq!(group_channel.get_channel_ids_count(), 2);
        assert!(group_channel.has_channel_id(1));
        assert!(group_channel.has_channel_id(2));
        assert_eq!(
            group_channel.get_full_extranonce_size(),
            full_extranonce_size
        );

        // add a third channel with a different full extranonce size
        // this should return an error
        let new_full_extranonce_size = 24;
        assert!(group_channel
            .add_channel_id(3, new_full_extranonce_size)
            .is_err());
        assert_eq!(group_channel.get_channel_ids_count(), 2);
        assert!(!group_channel.has_channel_id(3));

        // set the full extranonce size to a new value
        group_channel
            .set_full_extranonce_size(new_full_extranonce_size)
            .unwrap();
        assert_eq!(
            group_channel.get_full_extranonce_size(),
            new_full_extranonce_size
        );
        // all channel IDs should be cleared
        assert_eq!(group_channel.get_channel_ids_count(), 0);

        // add a fourth channel with the correct full extranonce size
        group_channel
            .add_channel_id(4, new_full_extranonce_size)
            .unwrap();
        assert_eq!(group_channel.get_channel_ids_count(), 1);
        assert!(group_channel.has_channel_id(4));

        // add a fifth channel with the old full extranonce size
        // this should return an error because the full extranonce size is now set to 24
        assert!(group_channel.add_channel_id(5, 32).is_err());
        assert_eq!(group_channel.get_channel_ids_count(), 1);
        assert!(!group_channel.has_channel_id(5));
    }

    #[test]
    fn test_set_new_prev_hash_with_unknown_template_id() {
        // Regression test: when on_set_new_prev_hash carries a template_id that matches no
        // queued future job, activation is a no-op and the group channel must report the
        // failure instead of silently advancing the chain tip.
        let group_channel_id = 1;
        let full_extranonce_size = 32;
        let mut group_channel =
            GroupChannel::new(group_channel_id, full_extranonce_size, true, None, None).unwrap();

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
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        group_channel
            .on_new_template(template, coinbase_reward_outputs)
            .unwrap();
        assert!(group_channel.job_store.has_future_jobs());

        // the only queued future job came from template_id 1, so template_id 2 cannot activate
        let set_new_prev_hash = SetNewPrevHash {
            template_id: 2,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            ntime_start: 1746839905,
            n_bits: 503543726,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                174, 119, 3, 0, 0,
            ]
            .into(),
        };

        assert!(matches!(
            group_channel.on_set_new_prev_hash(set_new_prev_hash),
            Err(GroupChannelError::TemplateIdNotFound)
        ));

        // the failed activation must not have corrupted channel state
        assert!(group_channel.get_chain_tip().is_none());
        assert!(group_channel.get_active_job().is_none());
    }

    #[test]
    fn test_set_new_prev_hash_without_future_jobs_updates_chain_tip() {
        // Regression test: a SetNewPrevHash with no queued future job means the peer broke the
        // Template Distribution Protocol, which requires at least one future NewTemplate
        // beforehand. The channel must still recover from it — record the new chain tip rather
        // than reject the message and wedge the group-job pipeline in a persistent error path,
        // since the tip carried by the message is self-contained and usable.
        let mut group_channel = GroupChannel::new(1, 32, true, None, None).unwrap();
        assert!(!group_channel.job_store.has_future_jobs());
        assert!(group_channel.get_chain_tip().is_none());

        let prev_hash: binary_sv2::U256Owned = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        let set_new_prev_hash = SetNewPrevHash {
            template_id: 0,
            prev_hash: prev_hash.clone(),
            ntime_start: 1746839905,
            n_bits: 503543726,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                174, 119, 3, 0, 0,
            ]
            .into(),
        };

        group_channel
            .on_set_new_prev_hash(set_new_prev_hash)
            .unwrap();

        let chain_tip = group_channel.get_chain_tip().unwrap();
        assert_eq!(chain_tip.prev_hash(), prev_hash);
        assert_eq!(chain_tip.ntime_start(), 1746839905);
        assert_eq!(chain_tip.nbits(), 503543726);
        // no job could have been activated
        assert!(group_channel.get_active_job().is_none());
    }

    // a 52 char pool tag places the worst-case scriptSig exactly on the budget:
    // 8 (MAX_COINBASE_PREFIX_SIZE) + 1 + 3 ("Sv2") + 3 + 52 (tag) + 1 + 32 (extranonce) = 100
    const POOL_TAG_AT_SCRIPT_SIG_BUDGET: usize = 52;

    #[test]
    fn test_new_rejects_oversized_script_sig() {
        let full_extranonce_size = 32;

        // exactly on the budget
        let group_channel = GroupChannel::new(
            1,
            full_extranonce_size,
            true,
            Some("x".repeat(POOL_TAG_AT_SCRIPT_SIG_BUDGET)),
            None,
        )
        .unwrap();
        assert_eq!(
            group_channel
                .job_factory
                .script_sig_size(MAX_COINBASE_PREFIX_SIZE, full_extranonce_size),
            MAX_SCRIPT_SIG_SIZE
        );

        // one byte over the budget
        let group_channel = GroupChannel::new(
            1,
            full_extranonce_size,
            true,
            Some("x".repeat(POOL_TAG_AT_SCRIPT_SIG_BUDGET + 1)),
            None,
        );
        assert!(matches!(
            group_channel.unwrap_err(),
            GroupChannelError::ScriptSigSizeTooLarge
        ));
    }

    #[test]
    fn test_set_full_extranonce_size_rejects_oversized_script_sig() {
        let group_channel_id = 1;
        let full_extranonce_size = 16;
        let mut group_channel = GroupChannel::new(
            group_channel_id,
            full_extranonce_size,
            true,
            Some("x".repeat(POOL_TAG_AT_SCRIPT_SIG_BUDGET)),
            None,
        )
        .unwrap();
        group_channel
            .add_channel_id(1, full_extranonce_size)
            .unwrap();

        // growing up to the budget is allowed
        group_channel.set_full_extranonce_size(32).unwrap();
        assert_eq!(group_channel.get_full_extranonce_size(), 32);
        // channel ids are cleared, since the full extranonce size changed
        assert_eq!(group_channel.get_channel_ids_count(), 0);

        group_channel.add_channel_id(1, 32).unwrap();

        // one byte past the budget must be rejected, leaving the channel untouched
        let res = group_channel.set_full_extranonce_size(33);
        assert!(matches!(
            res.unwrap_err(),
            GroupChannelError::ScriptSigSizeTooLarge
        ));
        assert_eq!(group_channel.get_full_extranonce_size(), 32);
        assert_eq!(group_channel.get_channel_ids_count(), 1);
    }

    #[test]
    fn test_on_new_template_rejects_oversized_script_sig() {
        let group_channel_id = 1;
        let full_extranonce_size = 32;
        let mut group_channel = GroupChannel::new(
            group_channel_id,
            full_extranonce_size,
            true,
            Some("x".repeat(POOL_TAG_AT_SCRIPT_SIG_BUDGET)),
            None,
        )
        .unwrap();

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

        let template = |coinbase_prefix: Vec<u8>| NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: coinbase_prefix.try_into().unwrap(),
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

        // a spec-compliant 8 byte coinbase_prefix fits exactly
        group_channel
            .on_new_template(
                template(vec![0xab; MAX_COINBASE_PREFIX_SIZE]),
                coinbase_reward_outputs.clone(),
            )
            .unwrap();

        // an out-of-spec Template Provider sending 9 bytes overflows the budget. without this
        // check the group channel would distribute unmineable work to every channel in the group
        let res = group_channel.on_new_template(
            template(vec![0xab; MAX_COINBASE_PREFIX_SIZE + 1]),
            coinbase_reward_outputs,
        );
        assert!(matches!(
            res.unwrap_err(),
            GroupChannelError::JobFactoryError(JobFactoryError::ScriptSigSizeTooLarge)
        ));
    }

    #[test]
    fn test_future_template_storage_is_bounded() {
        let mut group_channel = GroupChannel::new(1, 32, true, None, None).unwrap();

        let flood_size = 10_000u64;
        for template_id in 0..flood_size {
            let template = NewTemplate {
                template_id,
                future_template: true,
                version: 536870912,
                coinbase_tx_version: 2,
                coinbase_prefix: vec![].try_into().unwrap(),
                coinbase_tx_input_sequence: u32::MAX,
                coinbase_tx_value_remaining: 0,
                coinbase_tx_outputs_count: 0,
                coinbase_tx_outputs: vec![].try_into().unwrap(),
                coinbase_tx_locktime: 0,
                merkle_path: vec![].try_into().unwrap(),
            };

            group_channel.on_new_template(template, vec![]).unwrap();
        }

        // only the newest MAX_FUTURE_JOBS templates survive; the oldest were evicted
        for template_id in 0..flood_size - MAX_FUTURE_JOBS as u64 {
            assert!(group_channel
                .get_future_job_id_from_template_id(template_id)
                .is_none());
        }
        for template_id in flood_size - MAX_FUTURE_JOBS as u64..flood_size {
            assert!(group_channel
                .get_future_job_id_from_template_id(template_id)
                .is_some());
        }
    }

    #[test]
    fn test_replaced_active_job_is_dropped() {
        let mut group_channel = GroupChannel::new(1, 32, true, None, None).unwrap();
        group_channel.set_chain_tip(ChainTip::new([0; 32].into(), 0x1d00ffff, 1));

        let flood_size = 10_000u64;
        for template_id in 0..flood_size {
            let template = NewTemplate {
                template_id,
                future_template: false,
                version: 536870912,
                coinbase_tx_version: 2,
                coinbase_prefix: vec![].try_into().unwrap(),
                coinbase_tx_input_sequence: u32::MAX,
                coinbase_tx_value_remaining: 0,
                coinbase_tx_outputs_count: 0,
                coinbase_tx_outputs: vec![].try_into().unwrap(),
                coinbase_tx_locktime: 0,
                merkle_path: vec![].try_into().unwrap(),
            };

            group_channel.on_new_template(template, vec![]).unwrap();
        }

        // group channels never validate shares, so replaced active jobs must be dropped
        // instead of retained as past jobs
        for job_id in 0..=flood_size as u32 {
            assert!(group_channel.job_store.get_past_job(job_id).is_none());
            assert!(group_channel.job_store.get_stale_job(job_id).is_none());
        }
        assert!(group_channel.get_active_job().is_some());

        // future job activation must also drop the displaced active job, rather than retiring
        // it into past/stale history
        let future_template = NewTemplate {
            template_id: flood_size,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![].try_into().unwrap(),
            coinbase_tx_input_sequence: u32::MAX,
            coinbase_tx_value_remaining: 0,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: vec![].try_into().unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };
        group_channel
            .on_new_template(future_template, vec![])
            .unwrap();

        let set_new_prev_hash = SetNewPrevHash {
            template_id: flood_size,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            ntime_start: 1746839905,
            n_bits: 503543726,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                174, 119, 3, 0, 0,
            ]
            .into(),
        };
        group_channel
            .on_set_new_prev_hash(set_new_prev_hash)
            .unwrap();

        for job_id in 0..=flood_size as u32 + 1 {
            assert!(group_channel.job_store.get_past_job(job_id).is_none());
            assert!(group_channel.job_store.get_stale_job(job_id).is_none());
        }
        assert!(group_channel.get_active_job().is_some());
    }

    #[test]
    fn test_group_jobs_advertise_the_configured_version_rolling_policy() {
        // a group job's flag is what every extended channel importing it tells its miner and
        // enforces on shares, so it is the policy the application configured, not a constant
        let mut group_channel = GroupChannel::new(1, 32, false, None, None).unwrap();

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
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::from(script_bytes),
        }];

        group_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();
        let job_id = group_channel
            .get_future_job_id_from_template_id(template.template_id)
            .unwrap();
        assert!(
            !group_channel
                .get_future_job(job_id)
                .unwrap()
                .get_job_message()
                .version_rolling_allowed
        );
    }

    /// The bytes of a prebuilt job: here the `NewExtendedMiningJob` a group channel
    /// built from the template of `test_future_job_activation_flow` with a 32-byte extranonce
    /// slot (`Sv2///` tag included), so that the pre-mined shares of the template tests apply.
    fn prebuilt_job() -> PrebuiltJob {
        PrebuiltJob {
            version: 536870912,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 42, 82, 0, 6, 83, 118, 50, 47, 47,
                47, 32,
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
        }
    }

    // network target: 7fffff0000000000000000000000000000000000000000000000000000000000
    fn prebuilt_job_chain_tip() -> ChainTip {
        let prev_hash = [
            251, 175, 106, 40, 35, 87, 122, 90, 58, 51, 78, 32, 202, 236, 228, 36, 154, 174, 206,
            144, 147, 195, 21, 224, 195, 103, 214, 189, 51, 190, 24, 98,
        ]
        .into();
        ChainTip::new(prev_hash, 545259519, 1745596910)
    }

    #[test]
    fn test_prebuilt_job_future_activation_flow() {
        let mut group_channel = GroupChannel::new(1, 32, true, None, None).unwrap();

        // an active job needs a chain tip the group does not have yet
        assert!(matches!(
            group_channel.on_prebuilt_job(prebuilt_job(), false),
            Err(GroupChannelError::ChainTipNotSet)
        ));

        let job_id = group_channel.on_prebuilt_job(prebuilt_job(), true).unwrap();
        assert!(group_channel.get_active_job().is_none());

        // the bytes are broadcast as given, with the group's version-rolling policy and no
        // extranonce prefix, and nothing is parsed out of them
        let future_job = group_channel.get_future_job(job_id).unwrap();
        let job = prebuilt_job();
        assert_eq!(
            future_job.get_job_message(),
            &NewExtendedMiningJob {
                channel_id: 1,
                job_id,
                ntime_start: Sv2Option::new(None),
                version: job.version,
                version_rolling_allowed: true,
                coinbase_tx_prefix: job.coinbase_tx_prefix,
                coinbase_tx_suffix: job.coinbase_tx_suffix,
                merkle_path: job.merkle_path,
            }
        );
        assert!(future_job.get_extranonce_prefix().is_empty());
        assert!(future_job.get_coinbase_outputs().is_empty());
        assert_eq!(future_job.get_origin(), &JobOrigin::Prebuilt);

        // an unknown job id activates nothing and leaves the group untouched
        let chain_tip = prebuilt_job_chain_tip();
        assert!(matches!(
            group_channel.activate_future_job(job_id + 1, chain_tip.clone()),
            Err(GroupChannelError::JobIdNotFound)
        ));
        assert!(group_channel.get_chain_tip().is_none());
        assert!(group_channel.get_future_job(job_id).is_some());

        group_channel
            .activate_future_job(job_id, chain_tip.clone())
            .unwrap();
        assert_eq!(group_channel.get_chain_tip(), Some(&chain_tip));
        assert!(group_channel.get_future_job(job_id).is_none());
        let active_job = group_channel.get_active_job().unwrap();
        assert_eq!(active_job.get_job_id(), job_id);
        assert_eq!(active_job.get_ntime_start(), Some(chain_tip.ntime_start()));

        // the same bytes under the current tip (a target change) replace the active job
        let next_job_id = group_channel
            .on_prebuilt_job(prebuilt_job(), false)
            .unwrap();
        assert_ne!(next_job_id, job_id);
        let active_job = group_channel.get_active_job().unwrap();
        assert_eq!(active_job.get_job_id(), next_job_id);
        assert_eq!(active_job.get_ntime_start(), Some(chain_tip.ntime_start()));
    }

    #[test]
    fn test_prebuilt_job_that_is_not_a_coinbase_is_refused() {
        // the member channels compute merkle roots from the bytes around the group's extranonce
        // slot, so bytes that do not deserialize as a coinbase transaction are refused before
        // they are broadcast
        let mut group_channel = GroupChannel::new(1, 32, true, None, None).unwrap();
        let job = PrebuiltJob {
            coinbase_tx_prefix: vec![1, 2, 3].try_into().unwrap(),
            coinbase_tx_suffix: vec![4, 5].try_into().unwrap(),
            ..prebuilt_job()
        };
        assert!(matches!(
            group_channel.on_prebuilt_job(job, true),
            Err(GroupChannelError::JobFactoryError(
                JobFactoryError::InvalidPrebuiltCoinbase
            ))
        ));
        assert!(group_channel.get_future_job(1).is_none());
    }
}
