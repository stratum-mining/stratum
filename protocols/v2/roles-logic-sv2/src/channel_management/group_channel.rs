//! # Group Channel
//!
//! A collection of `channel_id`s representing Standard Channels.
//!
//! Receives Extended Jobs and keeps them in state for conversion to Standard Jobs.

use crate::{
    channel_management::chain_tip::ChainTip,
    job_management::{job_factory::JobFactory, ExtendedJob},
    template_distribution_sv2::NewTemplate,
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
/// - 1 Chain Tip (`prev_hash`, `nbits`, `min_ntime`)
/// - 1 Active Job
/// - 1 Future Job
/// - No list of Past or Stale Jobs, as all share validation is done in the Standard Channels
///
/// The Group Channel is responsible for creating the active and future Extended Jobs.
/// The Extended Job is then converted to a Standard Job and set in each Standard Channel by the
/// [`StandardChannelFactory`].
#[derive(Clone, Debug)]
pub struct GroupChannel {
    channel_id: u32,
    channels: HashSet<u32>,
    job_factory: JobFactory,
    chain_tip: ChainTip,
    current_job: Option<ExtendedJob<'static>>,
    future_job: Option<ExtendedJob<'static>>,
}

impl GroupChannel {
    /// Create a new [`GroupChannel`].
    pub fn new(channel_id: u32, chain_tip: ChainTip) -> Self {
        Self {
            channel_id,
            channels: HashSet::new(),
            chain_tip,
            job_factory: JobFactory::new(None, None),
            current_job: None,
            future_job: None,
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
    pub fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = chain_tip;
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
        let chain_tip = self.chain_tip.clone();

        let job = self
            .job_factory
            .new_extended_job(self.channel_id, Some(chain_tip), 0)
            .map_err(|_| GroupChannelError::JobFactoryError)?;

        self.current_job = Some(job.clone());
        Ok(())
    }

    /// Get the active job.
    pub fn get_active_job(&self) -> Option<ExtendedJob<'static>> {
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

    pub fn get_future_job(&self) -> Option<ExtendedJob<'static>> {
        self.future_job.clone()
    }
}

#[derive(Debug)]
pub enum GroupChannelError {
    ChannelIdAlreadyExists,
    ChannelIdNotFound,
    JobFactoryError,
}
