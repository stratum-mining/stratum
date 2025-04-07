//! # Standard Channel Factory
//!
//! A factory for creating and managing [`StandardChannel`]s and [`GroupChannel`]s.

use crate::{
    channel_management::{
        chain_tip::ChainTip,
        group_channel::{GroupChannel, GroupChannelError},
        standard_channel::StandardChannel,
        ShareValidationError, ShareValidationResult,
    },
    job_management::{ExtendedJob, StandardJob},
    mining_sv2::{
        ExtendedExtranonce as ExtendedExtranonceFactory, ExtendedExtranonceError, NewMiningJob,
        OpenStandardMiningChannel, SubmitSharesStandard, Target, UpdateChannel,
    },
    template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTDP},
    utils::{hash_rate_to_target, merkle_root_from_path},
};
use binary_sv2::U256;
use core::ops::Range;
use std::{
    collections::HashMap,
    convert::TryInto,
    sync::{Arc, RwLock},
};

/// A factory for creating and managing [`StandardChannel`]s and [`GroupChannel`]s.
///
/// Allows the user to:
/// - Create Standard and Group channels
/// - Remove Standard and Group channels
/// - Manage channel states upon receipt of:
///   - [`OpenStandardMiningChannel`] (create new channel)
///   - [`UpdateChannel`] (update channel nominal hashrate and target)
///   - [`SetNewPrevHashTDP`] (update channel chain tip)
///   - [`NewTemplate`] (update channel template)
///   - [`SubmitSharesStandard`] (validate shares)
///
/// Only suitable for mining servers, not clients.
#[derive(Clone, Debug)]
pub struct StandardChannelFactory {
    group_channel_factory: Arc<RwLock<HashMap<u32, Arc<RwLock<GroupChannel>>>>>,
    standard_channel_factory: Arc<RwLock<HashMap<u32, Arc<RwLock<StandardChannel>>>>>,
    extended_extranonce_factory: Arc<RwLock<ExtendedExtranonceFactory>>,
    active_chain_tip: Option<ChainTip>,
    active_template: Option<NewTemplate<'static>>,
    future_template: Option<NewTemplate<'static>>,
    share_batch_size: usize,
    expected_share_per_minute_per_channel: f64,
    // todo
    // payout_coinbase_outputs: Vec<TxOut>,
}

// impl block with public methods
impl StandardChannelFactory {
    pub fn new(
        extended_extranonce_range_0: Range<usize>,
        extended_extranonce_range_1: Range<usize>,
        extended_extranonce_range_2: Range<usize>,
        additional_coinbase_script_data: Option<Vec<u8>>,
        share_batch_size: usize,
        expected_share_per_minute_per_channel: f64,
    ) -> Result<Self, StandardChannelFactoryError> {
        let extended_extranonce = ExtendedExtranonceFactory::new(
            extended_extranonce_range_0,
            extended_extranonce_range_1,
            extended_extranonce_range_2,
            additional_coinbase_script_data.clone(),
        )
        .map_err(|e| StandardChannelFactoryError::ExtendedExtranonceError(e))?;

        Ok(Self {
            group_channel_factory: Arc::new(RwLock::new(HashMap::new())),
            standard_channel_factory: Arc::new(RwLock::new(HashMap::new())),
            extended_extranonce_factory: Arc::new(RwLock::new(extended_extranonce)),
            share_batch_size,
            expected_share_per_minute_per_channel,
            active_chain_tip: None,
            active_template: None,
            future_template: None,
        })
    }

    pub fn new_group_channel(
        &self,
        group_channel_id: u32,
    ) -> Result<(), StandardChannelFactoryError> {
        let chain_tip = self
            .active_chain_tip
            .clone()
            .ok_or(StandardChannelFactoryError::NoActiveChainTip)?;

        let mut group_channel = GroupChannel::new(group_channel_id, chain_tip);
        group_channel.set_chain_tip(
            self.active_chain_tip
                .clone()
                .ok_or(StandardChannelFactoryError::NoActiveChainTip)?,
        );
        group_channel.set_active_template(
            self.active_template
                .clone()
                .ok_or(StandardChannelFactoryError::NoActiveTemplate)?,
        );
        group_channel
            .new_active_job()
            .map_err(|e| StandardChannelFactoryError::FailedToCreateActiveJob(e))?;
        group_channel.set_future_template(
            self.future_template
                .clone()
                .ok_or(StandardChannelFactoryError::NoFutureTemplate)?,
        );
        group_channel
            .new_future_job()
            .map_err(|e| StandardChannelFactoryError::FailedToCreateFutureJob(e))?;

        self.group_channel_factory
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?
            .insert(group_channel_id, Arc::new(RwLock::new(group_channel)));

        Ok(())
    }

    pub fn remove_standard_channel(
        &self,
        channel_id: u32,
    ) -> Result<(), StandardChannelFactoryError> {
        // iterate over all group channels, and remove the channel_id from all of them where it is
        // present
        let group_channel_factory_guard = self
            .group_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;
        for (_group_channel_id, group_channel) in group_channel_factory_guard.iter() {
            group_channel
                .write()
                .map_err(|_| StandardChannelFactoryError::PoisonError)?
                .remove_channel(channel_id)
                .map_err(|_| StandardChannelFactoryError::ChannelIdNotFoundOnGroupChannel)?;
        }

        self.standard_channel_factory
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?
            .remove(&channel_id);
        Ok(())
    }

    pub fn get_standard_channel(
        &self,
        channel_id: u32,
    ) -> Result<StandardChannel, StandardChannelFactoryError> {
        let standard_channel_factory_guard = self
            .standard_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

        let standard_channel = standard_channel_factory_guard
            .get(&channel_id)
            .ok_or(StandardChannelFactoryError::StandardChannelNotFound)?;
        let standard_channel_guard = standard_channel
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;
        Ok(standard_channel_guard.clone())
    }

    pub fn has_standard_channel(
        &self,
        channel_id: u32,
    ) -> Result<bool, StandardChannelFactoryError> {
        let res = self
            .standard_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?
            .contains_key(&channel_id);
        Ok(res)
    }

    pub fn has_group_channel(&self, channel_id: u32) -> Result<bool, StandardChannelFactoryError> {
        let res = self
            .group_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?
            .contains_key(&channel_id);
        Ok(res)
    }

    pub fn set_active_chain_tip(&mut self, chain_tip: ChainTip) {
        self.active_chain_tip = Some(chain_tip);
    }

    pub fn get_active_chain_tip(&self) -> Option<ChainTip> {
        self.active_chain_tip.clone()
    }

    pub fn set_active_template(&mut self, template: NewTemplate<'static>) {
        self.active_template = Some(template);
    }

    pub fn set_future_template(&mut self, template: NewTemplate<'static>) {
        self.future_template = Some(template);
    }

    /// Handle an [`OpenStandardMiningChannel`] message.
    ///
    /// We assume the group channel where this new channel is supposed to be added to already
    /// exists, and the caller made the decision to add this new channel to it.
    ///
    /// Therefore, no [`GroupChannel`]s are added or removed to `group_channel_factory` map
    /// here.
    ///
    /// We only create one new [`StandardChannel`] and add its `channel_id` to the [`GroupChannel`]
    /// specified in the function call.
    ///
    /// On successful execution, returns the `channel_id` of the newly created [`StandardChannel`].
    pub fn on_open_standard_mining_channel(
        &self,
        m: OpenStandardMiningChannel<'static>,
        group_channel_id: u32,
        standard_channel_id: u32,
    ) -> Result<u32, StandardChannelFactoryError> {
        let chain_tip = self
            .active_chain_tip
            .clone()
            .ok_or(StandardChannelFactoryError::NoActiveChainTip)?;

        // get read access to group channel factory
        // we dont need write access to the factory
        // as we assume the group already exists (no need to add or remove from hashmap)
        // so we only need to get a reference to the specific
        // group channel
        let group_channel_factory_guard = self
            .group_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;
        let mut group_channel_factory_guard_clone = group_channel_factory_guard.clone();

        // first, we check if the provided group channel id actually exists
        // if not, we terminate execution early by returning GroupChannelNotFound error

        // we get write access to the group channel as we will need to add the new standard
        // channel_id to it
        let mut group_channel_guard = group_channel_factory_guard_clone
            .get_mut(&group_channel_id)
            .ok_or(StandardChannelFactoryError::GroupChannelNotFound)?
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

        // now let's start preparing the new standard channel based on the request message

        // the spec says the following about user_identity string:
        // > It is highly recommended that UTF-8 encoding is used.
        // SRI enforces this here.
        let user_identity_bytes = m.user_identity.inner_as_ref();
        let user_identity_string =
            String::from_utf8(user_identity_bytes.to_vec()).expect("Invalid UTF-8");
        let user_identity: &'static str = Box::leak(user_identity_string.into_boxed_str());

        let nominal_hashrate = m.nominal_hash_rate;

        // for the extranonce_prefix, we need to get write access to the extended_extranonce_factory
        let mut extended_extranonce_guard = self
            .extended_extranonce_factory
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;
        let extranonce_prefix = extended_extranonce_guard
            .next_prefix_standard()
            .map_err(|e| StandardChannelFactoryError::ExtendedExtranonceError(e))?;

        // create new standard channel
        let mut standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            extranonce_prefix.clone().into(),
            nominal_hashrate,
            self.share_batch_size,
            chain_tip,
        );

        // set the first active job
        let active_job_group_channel = group_channel_guard
            .get_active_job()
            .ok_or(StandardChannelFactoryError::NoActiveJob)?;
        let active_job_standard_channel = Self::extended_to_standard_job(
            standard_channel_id,
            active_job_group_channel,
            extranonce_prefix.clone().into(),
        )?;
        standard_channel.push_active_job(active_job_standard_channel);

        // set the first future job
        // let future_job_group_channel =
        // group_channel_guard.get_future_job().ok_or(StandardChannelFactoryError::NoFutureJob)?;
        // let future_job_standard_channel = Self::extended_to_standard_job(standard_channel_id,
        // future_job_group_channel, extranonce_prefix.into())?; standard_channel.
        // set_future_job(future_job_standard_channel);

        // get write access to standard channels map
        let mut standard_channel_factory_guard = self
            .standard_channel_factory
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

        // store new standard channel on channels map
        standard_channel_factory_guard
            .insert(standard_channel_id, Arc::new(RwLock::new(standard_channel)));

        // add new standard channel to group channel
        group_channel_guard
            .add_channel(standard_channel_id)
            .map_err(|_| StandardChannelFactoryError::ChannelIdAlreadyExistsOnGroupChannel)?;

        Ok(standard_channel_id)
    }

    /// Handle an [`UpdateChannel`] message for a Standard Channel.
    ///
    /// Returns the new target that should use to craft
    /// a new `SetTarget` message to be sent to the client, based on
    /// `expected_share_per_minute_per_channel`.
    ///
    /// If the requested maximum target is not enough to reach the expected share per minute,
    /// given the new nominal hashrate, the method returns an error.
    pub fn update_standard_channel(
        &self,
        m: UpdateChannel<'static>,
    ) -> Result<Target, StandardChannelFactoryError> {
        let channel_id = m.channel_id;

        // get read access to standard channels map
        let standard_channel_factory_guard = self
            .standard_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

        let standard_channel = standard_channel_factory_guard
            .get(&channel_id)
            .ok_or(StandardChannelFactoryError::StandardChannelNotFound)?;

        let mut standard_channel_guard = standard_channel
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

        let new_target: U256<'static> = hash_rate_to_target(
            m.nominal_hash_rate.into(),
            self.expected_share_per_minute_per_channel,
        )
        .map_err(|_| StandardChannelFactoryError::HashRateToTargetError)?
        .into();

        // Convert both targets to Target type for comparison
        let new_target = Target::from(new_target.clone());
        let max_target = Target::from(m.maximum_target.clone());

        // todo: double check if this comparison is correct
        if new_target > max_target {
            return Err(StandardChannelFactoryError::RequestedMaxTargetTooLow);
        }

        standard_channel_guard.set_nominal_hashrate(m.nominal_hash_rate);
        standard_channel_guard.set_target(new_target.clone());
        Ok(new_target)
    }

    /// Handle a [`SubmitSharesStandard`] message.
    ///
    /// The return type are [`ShareValidationResult`] / [`ShareValidationError`], which should be
    /// used to decide what to do as an outcome of the share validation process.
    pub fn on_submit_shares_standard(
        &self,
        m: SubmitSharesStandard,
    ) -> Result<ShareValidationResult, StandardChannelFactoryError> {
        let channel_id = m.channel_id;

        // get read access to standard channels map
        let standard_channel_factory_guard = self
            .standard_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

        let standard_channel = standard_channel_factory_guard
            .get(&channel_id)
            .ok_or(StandardChannelFactoryError::StandardChannelNotFound)?;

        let mut standard_channel_guard = standard_channel
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

        standard_channel_guard
            .validate_share(&m)
            .map_err(|e| StandardChannelFactoryError::ShareRejected(e))
    }

    /// Handle a [`SetNewPrevHashTDP`] message.
    ///
    /// Updates the chain tip for all standard and group channels.
    pub fn on_set_new_prev_hash_tdp(
        &self,
        m: SetNewPrevHashTDP<'static>,
    ) -> Result<(), StandardChannelFactoryError> {
        let chain_tip = ChainTip::new(m.prev_hash, m.n_bits, m.header_timestamp);

        let standard_channel_factory_guard = self
            .standard_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

        // iterate over all standard channels and push the new chain tip
        for (_, standard_channel) in standard_channel_factory_guard.iter() {
            let mut standard_channel_guard = standard_channel
                .write()
                .map_err(|_| StandardChannelFactoryError::PoisonError)?;
            standard_channel_guard.set_chain_tip(chain_tip.clone());

            // also flush past jobs into stale jobs
            standard_channel_guard.flush_past_jobs();
        }

        // iterate over all group channels and set the new chain tip
        let group_channel_factory_guard = self
            .group_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;
        for (_, group_channel) in group_channel_factory_guard.iter() {
            let mut group_channel_guard = group_channel
                .write()
                .map_err(|_| StandardChannelFactoryError::PoisonError)?;
            group_channel_guard.set_chain_tip(chain_tip.clone());
        }

        Ok(())
    }

    /// Handle a [`NewTemplate`] message.
    ///
    /// Updates the template (future or active) for all group channels.
    ///
    /// Creates a new (future or active) job and stores it in all group and standard channel states.
    pub fn on_new_template(
        &self,
        m: NewTemplate<'static>,
    ) -> Result<(), StandardChannelFactoryError> {
        let group_channel_factory_guard = self
            .group_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

        // iterate over all group channels
        for (_, group_channel) in group_channel_factory_guard.iter() {
            let mut group_channel_guard = group_channel
                .write()
                .map_err(|_| StandardChannelFactoryError::PoisonError)?;
            match m.future_template {
                true => {
                    // set the new template
                    group_channel_guard.set_future_template(m.clone());

                    // create a new future job
                    group_channel_guard
                        .new_future_job()
                        .map_err(|e| StandardChannelFactoryError::FailedToCreateFutureJob(e))?;

                    // get the future extended job
                    let extended_job = group_channel_guard
                        .get_future_job()
                        .ok_or(StandardChannelFactoryError::NoFutureJob)?;

                    let standard_channel_factory_guard = self
                        .standard_channel_factory
                        .read()
                        .map_err(|_| StandardChannelFactoryError::PoisonError)?;

                    // for each standard channel in the group channel, we need to convert the
                    // extended job to a standard job and store it in the standard channel state
                    let standard_channel_ids = group_channel_guard.get_standard_channel_ids();
                    for standard_channel_id in standard_channel_ids {
                        let standard_channel = standard_channel_factory_guard
                            .get(&standard_channel_id)
                            .ok_or(StandardChannelFactoryError::StandardChannelNotFound)?;
                        let mut standard_channel_guard = standard_channel
                            .write()
                            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

                        let extranonce_prefix = standard_channel_guard.get_extranonce_prefix();

                        let standard_job = Self::extended_to_standard_job(
                            standard_channel_id,
                            extended_job.clone(),
                            extranonce_prefix,
                        )?;
                        standard_channel_guard.set_future_job(standard_job);
                    }
                }
                false => {
                    // set the new template
                    group_channel_guard.set_active_template(m.clone());

                    // create a new active job
                    group_channel_guard
                        .new_active_job()
                        .map_err(|e| StandardChannelFactoryError::FailedToCreateActiveJob(e))?;

                    // get the active extended job
                    let extended_job = group_channel_guard
                        .get_active_job()
                        .ok_or(StandardChannelFactoryError::NoActiveJob)?;

                    let standard_channel_factory_guard = self
                        .standard_channel_factory
                        .read()
                        .map_err(|_| StandardChannelFactoryError::PoisonError)?;

                    // for each standard channel in the group channel, we need to convert the
                    // extended job to a standard job and store it in the standard channel state
                    let standard_channel_ids = group_channel_guard.get_standard_channel_ids();
                    for standard_channel_id in standard_channel_ids {
                        let standard_channel = standard_channel_factory_guard
                            .get(&standard_channel_id)
                            .ok_or(StandardChannelFactoryError::StandardChannelNotFound)?;
                        let mut standard_channel_guard = standard_channel
                            .write()
                            .map_err(|_| StandardChannelFactoryError::PoisonError)?;

                        let extranonce_prefix = standard_channel_guard.get_extranonce_prefix();

                        let standard_job = Self::extended_to_standard_job(
                            standard_channel_id,
                            extended_job.clone(),
                            extranonce_prefix,
                        )?;
                        standard_channel_guard.push_active_job(standard_job);
                    }
                }
            }
        }

        Ok(())
    }
}

// impl block with private methods
impl StandardChannelFactory {
    fn extended_to_standard_job(
        channel_id: u32,
        extended_job: ExtendedJob<'static>,
        extranonce_prefix: Vec<u8>,
    ) -> Result<StandardJob<'static>, StandardChannelFactoryError> {
        let merkle_root = merkle_root_from_path(
            extended_job.get_coinbase_tx_prefix().inner_as_ref(),
            extended_job.get_coinbase_tx_suffix().inner_as_ref(),
            extranonce_prefix.as_ref(),
            &extended_job.get_merkle_path().inner_as_ref(),
        )
        .ok_or(StandardChannelFactoryError::MerkleRootError)?;

        let standard_job_message = NewMiningJob {
            channel_id,
            job_id: extended_job.get_job_id(),
            min_ntime: extended_job.get_min_ntime(),
            version: extended_job.get_version(),
            merkle_root: merkle_root
                .try_into()
                .map_err(|_| StandardChannelFactoryError::MerkleRootError)?,
        };

        let template_id = extended_job
            .get_template_id()
            .ok_or(StandardChannelFactoryError::NoTemplateId)?;
        let mut coinbase = vec![];
        coinbase.extend(extended_job.get_coinbase_tx_prefix().inner_as_ref());
        coinbase.extend(extranonce_prefix);
        coinbase.extend(extended_job.get_coinbase_tx_suffix().inner_as_ref());

        let standard_job = StandardJob::new(template_id, standard_job_message, coinbase);

        Ok(standard_job)
    }
}

#[derive(Debug)]
pub enum StandardChannelFactoryError {
    PoisonError,
    GroupChannelNotFound,
    ChannelIdNotFoundOnGroupChannel,
    ChannelIdAlreadyExistsOnGroupChannel,
    StandardChannelNotFound,
    ExtendedExtranonceError(ExtendedExtranonceError),
    HashRateToTargetError,
    RequestedMaxTargetTooLow,
    ShareRejected(ShareValidationError),
    FailedToCreateFutureJob(GroupChannelError),
    FailedToCreateActiveJob(GroupChannelError),
    MerkleRootError,
    NoFutureJob,
    NoActiveJob,
    NoActiveChainTip,
    NoActiveTemplate,
    NoFutureTemplate,
    NoTemplateId,
}
