//! # Standard Channel Factory
//!
//! A factory for creating and managing [`StandardChannel`]s and [`GroupChannel`]s.

use crate::{
    channel_management::{group_channel::GroupChannel, standard_channel::StandardChannel},
    mining_sv2::{
        ExtendedExtranonce as ExtendedExtranonceFactory, ExtendedExtranonceError,
        NewExtendedMiningJob, NewMiningJob, OpenStandardMiningChannel, SubmitSharesStandard,
        Target, UpdateChannel,
    },
    template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTDP},
    utils::{hash_rate_to_target, Id as IdFactory},
};
use binary_sv2::U256;
use core::ops::Range;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

/// todo
///
/// past_jobs_capacity is the capacity of the past jobs queue for each channel.
pub struct StandardChannelFactory {
    channel_id_factory: Arc<RwLock<IdFactory>>,
    group_channel_factory: Arc<RwLock<HashMap<u32, Arc<RwLock<GroupChannel>>>>>,
    standard_channel_factory: Arc<RwLock<HashMap<u32, Arc<RwLock<StandardChannel>>>>>,
    extended_extranonce_factory: Arc<RwLock<ExtendedExtranonceFactory>>,
    past_jobs_capacity: usize,
    share_batch_size: usize,
    expected_share_per_minute_per_channel: f64,
}

impl StandardChannelFactory {
    pub fn new(
        extended_extranonce_range_0: Range<usize>,
        extended_extranonce_range_1: Range<usize>,
        extended_extranonce_range_2: Range<usize>,
        additional_coinbase_script_data: Option<Vec<u8>>,
        past_jobs_capacity: usize,
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
            past_jobs_capacity,
            share_batch_size,
            expected_share_per_minute_per_channel,
        })
    }

    pub async fn new_group_channel(
        &mut self,
        past_jobs_size: usize,
    ) -> Result<u32, StandardChannelFactoryError> {
        let group_channel_id = self
            .channel_id_factory
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?
            .next();

        let group_channel = GroupChannel::new(group_channel_id, past_jobs_size);

        self.group_channel_factory
            .write()
            .map_err(|_| StandardChannelFactoryError::PoisonError)?
            .insert(group_channel_id, Arc::new(RwLock::new(group_channel)));

        Ok(group_channel_id)
    }

    pub async fn remove_channel(
        &mut self,
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
                .await;
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
    pub async fn on_open_standard_mining_channel(
        &mut self,
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
            self.past_jobs_capacity,
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
        group_channel_guard.add_channel(standard_channel_id).await;

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
    pub async fn on_update_channel(
        &mut self,
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

    pub async fn on_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<(), StandardChannelFactoryError> {
        todo!()
    }

    pub async fn on_set_new_prev_hash_tdp(
        &mut self,
        m: SetNewPrevHashTDP<'static>,
    ) -> Result<(), StandardChannelFactoryError> {
        todo!()
    }

    pub async fn on_new_template(
        &mut self,
        m: NewTemplate<'static>,
    ) -> Result<(), StandardChannelFactoryError> {
        todo!()
    }
}

#[derive(Debug)]
pub enum StandardChannelFactoryError {
    PoisonError,
    GroupChannelNotFound,
    StandardChannelNotFound,
    ExtendedExtranonceError(ExtendedExtranonceError),
    HashRateToTargetError,
    RequestedMaxTargetTooLow,
}
