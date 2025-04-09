//! # Extended Channel Factory
//!
//! A factory for creating and managing [`ExtendedChannel`]s.

use crate::{
    channel_management::extended_channel::ExtendedChannel,
    mining_sv2::{
        ExtendedExtranonce as ExtendedExtranonceFactory, ExtendedExtranonceError,
        NewExtendedMiningJob, OpenExtendedMiningChannel, SubmitSharesExtended, Target,
        UpdateChannel,
    },
    template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTDP},
    utils::{hash_rate_to_target, Id as IdFactory},
};
use binary_sv2::U256;
use core::ops::Range;
use std::{
    collections::HashMap,
    f32::consts::E,
    sync::{Arc, RwLock},
};

// todo
pub struct ExtendedChannelFactory {
    channel_id_factory: Arc<RwLock<IdFactory>>,
    extended_channel_factory: Arc<RwLock<HashMap<u32, Arc<RwLock<ExtendedChannel>>>>>,
    extended_extranonce_factory: Arc<RwLock<ExtendedExtranonceFactory>>,
    past_jobs_capacity: usize,
    share_batch_size: usize,
    expected_share_per_minute_per_channel: f64,
}

impl ExtendedChannelFactory {
    pub fn new(
        extended_extranonce_range_0: Range<usize>,
        extended_extranonce_range_1: Range<usize>,
        extended_extranonce_range_2: Range<usize>,
        additional_coinbase_script_data: Option<Vec<u8>>,
        past_jobs_capacity: usize,
        share_batch_size: usize,
        expected_share_per_minute_per_channel: f64,
    ) -> Result<Self, ExtendedChannelFactoryError> {
        let extended_extranonce = ExtendedExtranonceFactory::new(
            extended_extranonce_range_0,
            extended_extranonce_range_1,
            extended_extranonce_range_2,
            additional_coinbase_script_data.clone(),
        )
        .map_err(|e| ExtendedChannelFactoryError::ExtendedExtranonceError(e))?;

        Ok(Self {
            channel_id_factory: Arc::new(RwLock::new(IdFactory::new())),
            extended_channel_factory: Arc::new(RwLock::new(HashMap::new())),
            extended_extranonce_factory: Arc::new(RwLock::new(extended_extranonce)),
            past_jobs_capacity,
            share_batch_size,
            expected_share_per_minute_per_channel,
        })
    }

    pub async fn remove_channel(
        &mut self,
        channel_id: u32,
    ) -> Result<(), ExtendedChannelFactoryError> {
        self.extended_channel_factory
            .write()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?
            .remove(&channel_id);
        Ok(())
    }

    /// Handle an [`OpenExtendedMiningChannel`] message.
    ///
    /// Returns the `channel_id` of the newly created [`ExtendedChannel`].
    pub async fn on_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel<'static>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        // now let's start preparing the new standard channel based on the request message

        // the spec says the following about user_identity string:
        // > It is highly recommended that UTF-8 encoding is used.
        // SRI enforces this here.
        let user_identity_bytes = m.user_identity.inner_as_ref();
        let user_identity_string =
            String::from_utf8(user_identity_bytes.to_vec()).expect("Invalid UTF-8");
        let user_identity: &'static str = Box::leak(user_identity_string.into_boxed_str());

        let nominal_hashrate = m.nominal_hash_rate;
        let extranonce_size = m.min_extranonce_size;

        // for the extranonce_prefix, we need to get write access to the extended_extranonce_factory
        let mut extended_extranonce_guard = self
            .extended_extranonce_factory
            .write()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;
        let extranonce_prefix = extended_extranonce_guard
            .next_prefix_extended(extranonce_size as usize)
            .map_err(|e| ExtendedChannelFactoryError::ExtendedExtranonceError(e))?;

        // create new channel_id
        let extended_channel_id = self
            .channel_id_factory
            .write()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?
            .next();

        // create new extended channel
        let extended_channel = ExtendedChannel::new(
            extended_channel_id,
            user_identity,
            extranonce_prefix.into(),
            extranonce_size,
            nominal_hashrate,
            self.past_jobs_capacity,
            self.share_batch_size,
        );

        // get write access to extended channels map
        let mut extended_channel_factory_guard = self
            .extended_channel_factory
            .write()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        // store new extended channel on channels map
        extended_channel_factory_guard
            .insert(extended_channel_id, Arc::new(RwLock::new(extended_channel)));

        Ok(())
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
    ) -> Result<Target, ExtendedChannelFactoryError> {
        let channel_id = m.channel_id;

        // get read access to extended channels map
        let extended_channel_factory_guard = self
            .extended_channel_factory
            .read()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        let extended_channel = extended_channel_factory_guard
            .get(&channel_id)
            .ok_or(ExtendedChannelFactoryError::ExtendedChannelNotFound)?;

        // get write access to extended channel
        let mut extended_channel_guard = extended_channel
            .write()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        let new_target: U256<'static> = hash_rate_to_target(
            m.nominal_hash_rate.into(),
            self.expected_share_per_minute_per_channel,
        )
        .map_err(|_| ExtendedChannelFactoryError::HashRateToTargetError)?
        .into();

        // Convert both targets to Target type for comparison
        let new_target = Target::from(new_target.clone());
        let max_target = Target::from(m.maximum_target.clone());

        // todo: double check if this comparison is correct
        if new_target > max_target {
            return Err(ExtendedChannelFactoryError::RequestedMaxTargetTooLow);
        }

        extended_channel_guard.set_nominal_hashrate(m.nominal_hash_rate);
        extended_channel_guard.set_target(new_target.clone());
        Ok(new_target)
    }

    /// Handle an [`SubmitSharesExtended`] message.
    ///
    /// Returns a boolean indicating whether it's time to send a SubmitShares.Success
    pub async fn on_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended<'static>,
    ) -> Result<bool, ExtendedChannelFactoryError> {
        let channel_id = m.channel_id;

        // get read access to extended channels map
        let extended_channel_factory_guard = self
            .extended_channel_factory
            .read()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        let extended_channel = extended_channel_factory_guard
            .get(&channel_id)
            .ok_or(ExtendedChannelFactoryError::ExtendedChannelNotFound)?;

        // get write access to extended channel
        let mut extended_channel_guard = extended_channel
            .write()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        let share_validation = extended_channel_guard.validate_share(&m);

        todo!()
    }

    pub async fn on_set_new_prev_hash_tdp(
        &mut self,
        m: SetNewPrevHashTDP<'static>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        todo!()
    }

    pub async fn on_new_template(
        &mut self,
        m: NewTemplate<'static>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        todo!()
    }
}

pub enum ExtendedChannelFactoryError {
    ExtendedExtranonceError(ExtendedExtranonceError),
    PoisonError,
    ExtendedChannelNotFound,
    HashRateToTargetError,
    RequestedMaxTargetTooLow,
    ShareRejected(),
}
