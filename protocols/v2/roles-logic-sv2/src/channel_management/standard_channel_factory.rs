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
    mining_sv2::{
        ExtendedExtranonce as ExtendedExtranonceFactory, ExtendedExtranonceError,
        NewExtendedMiningJob, NewMiningJob, OpenStandardMiningChannel, SubmitSharesStandard,
        Target, UpdateChannel,
    },
    template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTDP},
    utils::{hash_rate_to_target, merkle_root_from_path, Id as IdFactory},
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
    channel_id_factory: Arc<RwLock<IdFactory>>,
    group_channel_factory: Arc<RwLock<HashMap<u32, Arc<RwLock<GroupChannel>>>>>,
    standard_channel_factory: Arc<RwLock<HashMap<u32, Arc<RwLock<StandardChannel>>>>>,
    extended_extranonce_factory: Arc<RwLock<ExtendedExtranonceFactory>>,
    share_batch_size: usize,
    expected_share_per_minute_per_channel: f64,
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
            channel_id_factory: Arc::new(RwLock::new(IdFactory::new())),
            group_channel_factory: Arc::new(RwLock::new(HashMap::new())),
            standard_channel_factory: Arc::new(RwLock::new(HashMap::new())),
            extended_extranonce_factory: Arc::new(RwLock::new(extended_extranonce)),
            share_batch_size,
            expected_share_per_minute_per_channel,
        })
    }

    pub fn new_group_channel(&self) -> Result<u32, StandardChannelFactoryError> {
        let group_channel_id = self
            .channel_id_factory
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?
            .next();

        let group_channel = GroupChannel::new(group_channel_id);

        self.group_channel_factory
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?
            .insert(group_channel_id, Arc::new(RwLock::new(group_channel)));

        Ok(group_channel_id)
    }

    pub fn remove_channel(&self, channel_id: u32) -> Result<(), StandardChannelFactoryError> {
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
    ) -> Result<u32, StandardChannelFactoryError> {
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

        // create new channel_id
        let standard_channel_id = self
            .channel_id_factory
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?
            .next();

        // create new standard channel
        let standard_channel = StandardChannel::new(
            standard_channel_id,
            user_identity,
            extranonce_prefix.into(),
            nominal_hashrate,
            self.share_batch_size,
        );

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

    /// Handle an [`UpdateChannel`] message.
    ///
    /// Returns the new target that should use to craft
    /// a new `SetTarget` message to be sent to the client, based on
    /// `expected_share_per_minute_per_channel`.
    ///
    /// If the requested maximum target is not enough to reach the expected share per minute,
    /// given the new nominal hashrate, the method returns an error.
    pub fn on_update_channel(
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
            standard_channel_guard.push_chain_tip(chain_tip.clone());
        }

        // iterate over all group channels and push the new chain tip
        let group_channel_factory_guard = self
            .group_channel_factory
            .read()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?;
        for (_, group_channel) in group_channel_factory_guard.iter() {
            let mut group_channel_guard = group_channel
                .write()
                .map_err(|_| StandardChannelFactoryError::PoisonError)?;
            group_channel_guard.push_chain_tip(chain_tip.clone());
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
                        standard_channel_guard.set_active_job(standard_job);
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
        extended_job: NewExtendedMiningJob<'static>,
        extranonce_prefix: Vec<u8>,
    ) -> Result<NewMiningJob<'static>, StandardChannelFactoryError> {
        let merkle_root = merkle_root_from_path(
            extended_job.coinbase_tx_prefix.inner_as_ref(),
            extended_job.coinbase_tx_suffix.inner_as_ref(),
            extranonce_prefix.as_ref(),
            &extended_job.merkle_path.inner_as_ref(),
        )
        .ok_or(StandardChannelFactoryError::MerkleRootError)?;

        let standard_job = NewMiningJob {
            channel_id,
            job_id: extended_job.job_id,
            min_ntime: extended_job.min_ntime,
            version: extended_job.version,
            merkle_root: merkle_root
                .try_into()
                .map_err(|_| StandardChannelFactoryError::MerkleRootError)?,
        };
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
}
