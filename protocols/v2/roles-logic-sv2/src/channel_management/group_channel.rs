//! # Group Channel
//!
//! A collection of `channel_id`s representing Standard Channels.
//!
//! Receives Extended Jobs and keeps them in state for conversion to Standard Jobs.

use crate::{
    channel_management::chain_tip::ChainTip, job_management::JobFactory,
    mining_sv2::NewExtendedMiningJob, template_distribution_sv2::NewTemplate,
};
use std::collections::HashSet;
use stratum_common::bitcoin::transaction::TxOut;

/// A Group Channel is a collection of Standard Channels.
///
/// Group Channel state consists of:
/// - Channel Id
/// - Set of Standard Channel Ids
/// - [`JobFactory`] internal state
///     - Job ID Factory
///     - 1 Future Template
///     - 1 Active Template
///     - Multiple Coinbase Outputs for Reward distribution
/// - 1 Active + 1 Past Chain Tips
/// - 1 Active + 1 Future + 1 Past Jobs
///
/// The Group Channel is responsible for creating the active and future Extended Jobs.
/// The Extended Job is then converted to a Standard Job and set in each Standard Channel by the
/// [`StandardChannelFactory`].
#[derive(Clone, Debug)]
pub struct GroupChannel {
    channel_id: u32,
    channels: HashSet<u32>,
    job_factory: JobFactory,
    active_chain_tip: Option<ChainTip>,
    past_chain_tip: Option<ChainTip>,
    current_job: Option<NewExtendedMiningJob<'static>>,
    future_job: Option<NewExtendedMiningJob<'static>>,
    past_job: Option<NewExtendedMiningJob<'static>>,
}

impl GroupChannel {
    /// Create a new [`GroupChannel`].
    pub fn new(channel_id: u32) -> Self {
        Self {
            channel_id,
            channels: HashSet::new(),
            active_chain_tip: None,
            past_chain_tip: None,
            job_factory: JobFactory::new(None, None),
            current_job: None,
            future_job: None,
            past_job: None,
        }
    }

    /// Get the channel ids of the Standard Channels in the Group Channel.
    pub fn get_standard_channel_ids(&self) -> Vec<u32> {
        self.channels.iter().map(|id| *id).collect()
    }

    /// Add a channel to the Group Channel.
    pub fn add_channel(&mut self, channel_id: u32) -> Result<(), GroupChannelError> {
        if self.channels.contains(&channel_id) {
            return Err(GroupChannelError::ChannelIdAlreadyExists);
        }
        self.channels.insert(channel_id);
        Ok(())
    }

    /// Remove a channel from the Group Channel.
    pub fn remove_channel(&mut self, channel_id: u32) -> Result<(), GroupChannelError> {
        if !self.channels.contains(&channel_id) {
            return Err(GroupChannelError::ChannelIdNotFound);
        }
        self.channels.remove(&channel_id);
        Ok(())
    }

    /// Set the future template.
    pub fn set_future_template(&mut self, template: NewTemplate<'static>) {
        self.job_factory.set_future_template(template);
    }

    /// Set the active template.
    pub fn set_active_template(&mut self, template: NewTemplate<'static>) {
        self.job_factory.set_active_template(template);
    }

    /// Push a new chain tip to the Group Channel.
    ///
    /// The currently active chain tip is moved to the past chain tip.
    pub fn push_chain_tip(&mut self, chain_tip: ChainTip) {
        if self.active_chain_tip.is_some() {
            self.past_chain_tip = Some(self.active_chain_tip.clone().unwrap());
        }
        self.active_chain_tip = Some(chain_tip);
    }

    /// Set the coinbase outputs for the [`JobFactory`].
    pub fn set_coinbase_outputs(&mut self, coinbase_outputs: Vec<TxOut>) {
        self.job_factory
            .set_additional_coinbase_outputs(coinbase_outputs);
    }

    /// Leverage the internal [`JobFactory`] to create a new active job from the active chain tip.
    ///
    /// The Channel State is updated with the new active job.
    pub fn new_active_job(&mut self) -> Result<(), GroupChannelError> {
        let active_chain_tip = self
            .active_chain_tip
            .as_ref()
            .ok_or(GroupChannelError::NoActiveChainTip)?;

        let job = self
            .job_factory
            .new_extended_job(self.channel_id, Some(active_chain_tip.clone()), 0)
            .map_err(|_| GroupChannelError::JobFactoryError)?;

        if let Some(past_job) = self.current_job.take() {
            self.past_job = Some(past_job);
        }

        self.current_job = Some(job.clone());
        Ok(())
    }

    /// Get the active job.
    pub fn get_active_job(&self) -> Option<NewExtendedMiningJob<'static>> {
        self.current_job.clone()
    }

    /// Leverage the internal [`JobFactory`] to create a new future job from the future template.
    ///
    /// The Channel State is updated with the new future job.
    pub fn new_future_job(&mut self) -> Result<(), GroupChannelError> {
        let job = self
            .job_factory
            .new_extended_job(self.channel_id, None, 0)
            .map_err(|_| GroupChannelError::JobFactoryError)?;

        self.future_job = Some(job.clone());
        Ok(())
    }

    pub fn get_future_job(&self) -> Option<NewExtendedMiningJob<'static>> {
        self.future_job.clone()
    }
}

#[derive(Debug)]
pub enum GroupChannelError {
    ChannelIdAlreadyExists,
    ChannelIdNotFound,
    NoActiveChainTip,
    JobFactoryError,
}
