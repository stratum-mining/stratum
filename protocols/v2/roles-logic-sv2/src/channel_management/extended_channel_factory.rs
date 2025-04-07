//! # Extended Channel Factory
//!
//! A factory for creating and managing [`ExtendedChannel`]s.

use crate::{
    channel_management::{
        chain_tip::ChainTip,
        extended_channel::{ExtendedChannel, ExtendedChannelError},
        ShareValidationError, ShareValidationResult,
    },
    mining_sv2::{
        ExtendedExtranonce as ExtendedExtranonceFactory, ExtendedExtranonceError,
        OpenExtendedMiningChannel, SubmitSharesExtended, Target, UpdateChannel,
    },
    template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTDP},
    utils::hash_rate_to_target,
};
use binary_sv2::U256;
use core::ops::Range;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

/// A factory for creating and managing [`ExtendedChannel`]s.
///
/// Allows the user to:
/// - Create channels
/// - Remove channels
/// - Manage channel states upon receipt of:
///   - [`OpenExtendedMiningChannel`] (create new channel)
///   - [`UpdateChannel`] (update channel nominal hashrate and target)
///   - [`NewTemplate`] (update channel template)
///   - [`SetNewPrevHashTDP`] (update channel chain tip)
///   - [`SetCustomMiningJob`] (add new custom mining job)
///   - [`SubmitSharesExtended`] (validate shares)
///
/// Only suitable for mining servers, not clients.
#[derive(Clone, Debug)]
pub struct ExtendedChannelFactory {
    extended_channel_factory: Arc<RwLock<HashMap<u32, Arc<RwLock<ExtendedChannel>>>>>,
    extended_extranonce_factory: Arc<RwLock<ExtendedExtranonceFactory>>,
    chain_tip: Option<ChainTip>,
    active_template: Option<NewTemplate<'static>>,
    future_template: Option<NewTemplate<'static>>,
    share_batch_size: usize,
    expected_share_per_minute_per_channel: f64,
    version_rolling_allowed: bool,
    // todo
    // payout_coinbase_outputs: Vec<TxOut>,
}

impl ExtendedChannelFactory {
    pub fn new(
        extended_extranonce_range_0: Range<usize>,
        extended_extranonce_range_1: Range<usize>,
        extended_extranonce_range_2: Range<usize>,
        additional_coinbase_script_data: Option<Vec<u8>>,
        share_batch_size: usize,
        expected_share_per_minute_per_channel: f64,
        version_rolling_allowed: bool,
    ) -> Result<Self, ExtendedChannelFactoryError> {
        let extended_extranonce = ExtendedExtranonceFactory::new(
            extended_extranonce_range_0,
            extended_extranonce_range_1,
            extended_extranonce_range_2,
            additional_coinbase_script_data.clone(),
        )
        .map_err(|e| ExtendedChannelFactoryError::ExtendedExtranonceError(e))?;

        Ok(Self {
            extended_channel_factory: Arc::new(RwLock::new(HashMap::new())),
            extended_extranonce_factory: Arc::new(RwLock::new(extended_extranonce)),
            chain_tip: None,
            active_template: None,
            future_template: None,
            share_batch_size,
            expected_share_per_minute_per_channel,
            version_rolling_allowed,
        })
    }

    pub fn remove_channel(&self, channel_id: u32) -> Result<(), ExtendedChannelFactoryError> {
        self.extended_channel_factory
            .write()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?
            .remove(&channel_id);
        Ok(())
    }

    pub fn get_extended_channel(
        &self,
        channel_id: u32,
    ) -> Result<ExtendedChannel, ExtendedChannelFactoryError> {
        let extended_channel_factory_guard = self
            .extended_channel_factory
            .read()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        let extended_channel = extended_channel_factory_guard
            .get(&channel_id)
            .ok_or(ExtendedChannelFactoryError::ExtendedChannelNotFound)?;
        let extended_channel_guard = extended_channel
            .read()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;
        Ok(extended_channel_guard.clone())
    }

    pub fn has_extended_channel(
        &self,
        channel_id: u32,
    ) -> Result<bool, ExtendedChannelFactoryError> {
        let res = self
            .extended_channel_factory
            .read()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?
            .contains_key(&channel_id);
        Ok(res)
    }

    pub fn set_active_template(&mut self, template: NewTemplate<'static>) {
        self.active_template = Some(template);
    }

    pub fn set_future_template(&mut self, template: NewTemplate<'static>) {
        self.future_template = Some(template);
    }

    pub fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    pub fn get_chain_tip(&self) -> Option<ChainTip> {
        self.chain_tip.clone()
    }

    /// Handle an [`OpenExtendedMiningChannel`] message.
    ///
    /// Returns the `channel_id` of the newly created [`ExtendedChannel`].
    pub fn on_open_extended_mining_channel(
        &self,
        m: OpenExtendedMiningChannel<'static>,
        extended_channel_id: u32,
    ) -> Result<(), ExtendedChannelFactoryError> {
        let active_chain_tip = self
            .chain_tip
            .clone()
            .ok_or(ExtendedChannelFactoryError::NoActiveChainTip)?;

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

        // create new extended channel
        let mut extended_channel = ExtendedChannel::new(
            extended_channel_id,
            user_identity,
            extranonce_prefix.into(),
            extranonce_size,
            nominal_hashrate,
            self.share_batch_size,
            self.version_rolling_allowed,
            active_chain_tip,
        );

        let active_template = self
            .active_template
            .clone()
            .ok_or(ExtendedChannelFactoryError::NoActiveTemplate)?;
        // let future_template =
        // self.future_template.clone().ok_or(ExtendedChannelFactoryError::NoFutureTemplate)?;
        extended_channel.set_active_template(active_template);
        // extended_channel.set_future_template(future_template);

        // set the first active job
        extended_channel
            .new_active_job()
            .map_err(|e| ExtendedChannelFactoryError::FailedToCreateActiveJob(e))?;

        // set the first future job
        extended_channel
            .new_future_job()
            .map_err(|e| ExtendedChannelFactoryError::FailedToCreateFutureJob(e))?;

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
    pub fn update_extended_channel(
        &self,
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
    /// Returns a `ShareValidationResult` or `ShareValidationError`.
    pub fn on_submit_shares_extended(
        &self,
        m: SubmitSharesExtended<'static>,
    ) -> Result<ShareValidationResult, ExtendedChannelFactoryError> {
        let channel_id = m.channel_id;

        // get read access to extended channels map
        let extended_channel_factory_guard = self
            .extended_channel_factory
            .read()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        let extended_channel = extended_channel_factory_guard.get(&channel_id).ok_or(
            ExtendedChannelFactoryError::ShareRejected(ShareValidationError::InvalidChannelId),
        )?;

        // get write access to extended channel
        let mut extended_channel_guard = extended_channel
            .write()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        extended_channel_guard
            .validate_share(&m)
            .map_err(|e| ExtendedChannelFactoryError::ShareRejected(e))
    }

    pub fn on_set_new_prev_hash_tdp(
        &self,
        m: SetNewPrevHashTDP<'static>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        let chain_tip = ChainTip::new(m.prev_hash, m.n_bits, m.header_timestamp);

        let extended_channel_factory_guard = self
            .extended_channel_factory
            .read()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        // iterate over all extended channels and set the new chain tip
        for (_, extended_channel) in extended_channel_factory_guard.iter() {
            let mut extended_channel_guard = extended_channel
                .write()
                .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;
            extended_channel_guard.set_chain_tip(chain_tip.clone());

            // also flush past jobs into stale jobs
            extended_channel_guard.flush_past_jobs();
        }

        Ok(())
    }

    pub fn on_new_template(
        &self,
        m: NewTemplate<'static>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        let extended_channel_factory_guard = self
            .extended_channel_factory
            .read()
            .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;

        // iterate over all extended channels and set the new template
        for (_, extended_channel) in extended_channel_factory_guard.iter() {
            let mut extended_channel_guard = extended_channel
                .write()
                .map_err(|_| ExtendedChannelFactoryError::PoisonError)?;
            match m.future_template {
                true => {
                    extended_channel_guard.set_future_template(m.clone());
                    extended_channel_guard
                        .new_future_job()
                        .map_err(|e| ExtendedChannelFactoryError::FailedToCreateFutureJob(e))?;
                }
                false => {
                    extended_channel_guard.set_active_template(m.clone());
                    extended_channel_guard
                        .new_active_job()
                        .map_err(|e| ExtendedChannelFactoryError::FailedToCreateActiveJob(e))?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum ExtendedChannelFactoryError {
    ExtendedExtranonceError(ExtendedExtranonceError),
    PoisonError,
    ExtendedChannelNotFound,
    HashRateToTargetError,
    RequestedMaxTargetTooLow,
    ShareRejected(ShareValidationError),
    FailedToCreateFutureJob(ExtendedChannelError),
    FailedToCreateActiveJob(ExtendedChannelError),
    NoActiveChainTip,
    NoActiveTemplate,
    NoFutureTemplate,
}
