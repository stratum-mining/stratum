use crate::{
    channel_management::{
        extended::{
            channel::ExtendedChannel,
            factory::{
                message::InnerExtendedChannelFactoryMessage,
                response::InnerExtendedChannelFactoryResponse,
            },
        },
        id::ChannelIdFactory,
        share_accounting::{ShareValidationError, ShareValidationResult},
    },
    extranonce_prefix_management::extended::ExtranoncePrefixFactoryExtended,
    job_management::chain_tip::ChainTip,
    utils::hash_rate_to_target,
};
use binary_sv2::U256;
use mining_sv2::{SetCustomMiningJob, SubmitSharesExtended, Target};
use std::{collections::HashMap, sync::mpsc};
use stratum_common::bitcoin::transaction::TxOut;
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};
use tracing::{debug, error};

/// Encapsulates the Input/Output interface of the inner factory (Actor Model implementation of the
/// `ExtendedChannelFactory`).
pub struct InnerExtendedChannelFactoryIo {
    message: InnerExtendedChannelFactoryMessage<'static>,
    response_sender: mpsc::Sender<InnerExtendedChannelFactoryResponse<'static>>,
}

impl InnerExtendedChannelFactoryIo {
    /// Creates a new `InnerExtendedChannelFactoryIo` instance.
    ///
    /// Used by the [`ExtendedChannelFactory`] to interact with the inner factory.
    pub fn new(
        message: InnerExtendedChannelFactoryMessage<'static>,
        response_sender: mpsc::Sender<InnerExtendedChannelFactoryResponse<'static>>,
    ) -> Self {
        Self {
            message,
            response_sender,
        }
    }
}

/// Actor Model implementation of the `ExtendedChannelFactory`.
///
/// Encapsulates the Extended Channel Factory state and logic in a concurrency-safe manner.
///
/// The `run` method handles messages incoming from the public methods of
/// [`ExtendedChannelFactory`], which always arrive as instances of
/// [`InnerExtendedChannelFactoryIo`].
pub struct InnerExtendedChannelFactory {
    message_receiver: mpsc::Receiver<InnerExtendedChannelFactoryIo>,
    channel_id_factory: ChannelIdFactory,
    extranonce_prefix_factory: ExtranoncePrefixFactoryExtended,
    extended_channels: HashMap<u32, ExtendedChannel<'static>>,
    expected_share_per_minute_per_channel: f32,
    share_batch_size: usize,
    rollable_extranonce_size: u16,
    version_rolling_allowed: bool,
    chain_tip: Option<ChainTip>,
    active_template: Option<NewTemplate<'static>>,
    future_templates: HashMap<u64, NewTemplate<'static>>,
    coinbase_reward_outputs: Vec<TxOut>,
}

// impl block with public methods
impl InnerExtendedChannelFactory {
    /// Creates a new `InnerExtendedChannelFactory` instance.
    ///
    /// Used by the [`ExtendedChannelFactory`] to create the inner factory.
    pub fn new(
        message_receiver: mpsc::Receiver<InnerExtendedChannelFactoryIo>,
        extranonce_prefix_factory: ExtranoncePrefixFactoryExtended,
        expected_share_per_minute_per_channel: f32,
        rollable_extranonce_size: u16,
        version_rolling_allowed: bool,
        share_batch_size: usize,
        channel_id_factory: ChannelIdFactory,
    ) -> Self {
        if expected_share_per_minute_per_channel <= 0.0 {
            panic!("Expected share per minute per channel must be greater than 0");
        }

        Self {
            message_receiver,
            channel_id_factory,
            extranonce_prefix_factory,
            extended_channels: HashMap::new(),
            expected_share_per_minute_per_channel,
            rollable_extranonce_size,
            version_rolling_allowed,
            chain_tip: None,
            share_batch_size,
            future_templates: HashMap::new(),
            active_template: None,
            coinbase_reward_outputs: Vec::new(),
        }
    }

    /// Runner method for the Actor Model implementation.
    ///
    /// Routes incoming messages to the appropriate private handler.
    pub fn run(mut self) {
        // runs indefinitely until the shutdown message is received
        loop {
            match self.message_receiver.recv() {
                Ok(io) => {
                    let InnerExtendedChannelFactoryIo {
                        message,
                        response_sender,
                    } = io;

                    match message {
                        InnerExtendedChannelFactoryMessage::Shutdown => {
                            debug!("Shutting down ExtendedChannelFactory");
                            // Send acknowledgment before shutting down
                            if response_sender
                                .send(InnerExtendedChannelFactoryResponse::Shutdown)
                                .is_err()
                            {
                                error!("Failed to send shutdown response");
                                break;
                            }
                            break;
                        }
                        InnerExtendedChannelFactoryMessage::NewChannel(
                            user_identity,
                            nominal_hashrate,
                            min_extranonce_size,
                            max_target,
                        ) => {
                            let response = self.handle_new_channel(
                                user_identity,
                                nominal_hashrate,
                                min_extranonce_size,
                                max_target,
                            );
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::GetChannel(channel_id) => {
                            let response = self.handle_get_channel(channel_id);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::GetAllChannels => {
                            let response = self.handle_get_all_channels();
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::GetChannelCount => {
                            let response = self.handle_get_channel_count();
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::RemoveChannel(channel_id) => {
                            let response = self.handle_remove_channel(channel_id);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::UpdateChannel(
                            channel_id,
                            nominal_hashrate,
                            max_target,
                        ) => {
                            let response = self.handle_update_channel(
                                channel_id,
                                nominal_hashrate,
                                max_target,
                            );
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::ProcessNewTemplate(
                            template,
                            coinbase_reward_outputs,
                        ) => {
                            let response =
                                self.handle_process_new_template(template, coinbase_reward_outputs);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::ProcessSetNewPrevHash(
                            set_new_prev_hash,
                        ) => {
                            let response = self.handle_process_set_new_prev_hash(set_new_prev_hash);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::ProcessSetCustomMiningJob(
                            set_custom_mining_job,
                        ) => {
                            let response =
                                self.handle_process_set_custom_mining_job(set_custom_mining_job);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::ValidateShare(
                            submit_shares_extended,
                        ) => {
                            let response = match self.chain_tip.clone() {
                                Some(chain_tip) => {
                                    self.handle_validate_share(submit_shares_extended, chain_tip)
                                }
                                None => InnerExtendedChannelFactoryResponse::ChainTipNotSet,
                            };
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::GetShareAccounting(channel_id) => {
                            let response = self.handle_get_share_accounting(channel_id);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::SetExtranoncePrefix(
                            channel_id,
                            extranonce_prefix,
                        ) => {
                            let response =
                                self.handle_set_extranonce_prefix(channel_id, extranonce_prefix);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerExtendedChannelFactoryMessage::GetChainTip => {
                            let response = self.handle_get_chain_tip();
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                    }
                }
                Err(_) => {
                    error!("ExtendedChannelFactory message receiver closed");
                    break;
                }
            }
        }
    }
}

// impl block with private methods
impl InnerExtendedChannelFactory {
    /// Handles the `NewChannel` Actor Model message variant.
    fn handle_new_channel(
        &mut self,
        user_identity: String,
        nominal_hashrate: f32,
        min_extranonce_size: usize,
        max_target: U256<'static>,
    ) -> InnerExtendedChannelFactoryResponse<'static> {
        let target_u256 = match hash_rate_to_target(
            nominal_hashrate.into(),
            self.expected_share_per_minute_per_channel.into(),
        ) {
            Ok(target_u256) => target_u256,
            Err(_) => {
                return InnerExtendedChannelFactoryResponse::InvalidNominalHashrate;
            }
        };

        let target: Target = target_u256.clone().into();
        let max_target_value: Target = max_target.into();

        if target > max_target_value {
            return InnerExtendedChannelFactoryResponse::RequestedMaxTargetOutOfRange;
        }

        let channel_id = match self.channel_id_factory.next() {
            Ok(channel_id) => channel_id,
            Err(e) => {
                return InnerExtendedChannelFactoryResponse::FailedToGenerateNextChannelId(e);
            }
        };
        let extranonce_prefix = match self
            .extranonce_prefix_factory
            .next_prefix_extended(min_extranonce_size)
        {
            Ok(extranonce_prefix) => extranonce_prefix,
            Err(e) => {
                return InnerExtendedChannelFactoryResponse::FailedToGenerateNextExtranoncePrefixExtended(e);
            }
        };

        let mut channel = ExtendedChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            self.version_rolling_allowed,
            self.rollable_extranonce_size,
            self.share_batch_size,
        );

        match &self.active_template {
            Some(template) => {
                match channel.on_new_template(
                    template.clone(),
                    self.coinbase_reward_outputs.clone(),
                    self.chain_tip.clone(),
                ) {
                    Ok(_) => (),
                    Err(e) => {
                        return InnerExtendedChannelFactoryResponse::ProcessNewTemplateChannelError(
                            e,
                        );
                    }
                };
            }
            None => (),
        }

        self.extended_channels.insert(channel_id, channel);

        InnerExtendedChannelFactoryResponse::NewChannelCreated(
            channel_id,
            target_u256,
            self.rollable_extranonce_size,
            extranonce_prefix,
        )
    }

    /// Handles the `GetChannel` Actor Model message variant.
    fn handle_get_channel(&self, channel_id: u32) -> InnerExtendedChannelFactoryResponse<'static> {
        if !self.extended_channels.contains_key(&channel_id) {
            return InnerExtendedChannelFactoryResponse::ChannelNotFound;
        }

        let channel = self
            .extended_channels
            .get(&channel_id)
            .expect("we already checked that the channel exists");

        InnerExtendedChannelFactoryResponse::Channel(channel.clone())
    }

    /// Handles the `GetAllChannels` message variant.
    fn handle_get_all_channels(&self) -> InnerExtendedChannelFactoryResponse<'static> {
        let channels = self.extended_channels.clone();
        InnerExtendedChannelFactoryResponse::AllChannels(channels)
    }

    /// Handles the `GetChannelCount` message variant.
    fn handle_get_channel_count(&self) -> InnerExtendedChannelFactoryResponse<'static> {
        let channel_count = self.extended_channels.len() as u32;
        InnerExtendedChannelFactoryResponse::ChannelCount(channel_count)
    }

    /// Handles the `RemoveChannel` message variant.
    fn handle_remove_channel(
        &mut self,
        channel_id: u32,
    ) -> InnerExtendedChannelFactoryResponse<'static> {
        if !self.extended_channels.contains_key(&channel_id) {
            return InnerExtendedChannelFactoryResponse::ChannelNotFound;
        }

        self.extended_channels
            .remove(&channel_id)
            .expect("we already checked that the channel exists");

        InnerExtendedChannelFactoryResponse::ChannelRemoved
    }

    /// Handles the `UpdateChannel` Actor Model message variant.
    fn handle_update_channel(
        &mut self,
        channel_id: u32,
        nominal_hashrate: f32,
        requested_max_target: U256<'static>,
    ) -> InnerExtendedChannelFactoryResponse<'static> {
        if !self.extended_channels.contains_key(&channel_id) {
            return InnerExtendedChannelFactoryResponse::ChannelNotFound;
        }

        let new_target: U256<'static> = match hash_rate_to_target(
            nominal_hashrate.into(),
            self.expected_share_per_minute_per_channel.into(),
        ) {
            Ok(new_target) => new_target,
            Err(_) => {
                return InnerExtendedChannelFactoryResponse::InvalidNominalHashrate;
            }
        };

        // Convert both targets to mining_sv2::Target type for comparison
        let new_target: Target = new_target.into();
        let requested_max_target: Target = requested_max_target.into();

        if new_target > requested_max_target {
            return InnerExtendedChannelFactoryResponse::RequestedMaxTargetOutOfRange;
        }

        // Finally, actually update the channel state
        let channel = self
            .extended_channels
            .get_mut(&channel_id)
            .expect("we already checked that the channel exists");
        channel.set_nominal_hashrate(nominal_hashrate);
        channel.set_target(new_target.clone().into());

        InnerExtendedChannelFactoryResponse::ChannelUpdated(new_target.into())
    }

    /// Handles the `ProcessNewTemplate` Actor Model message variant.
    ///
    /// Iterates over all channels and updates them with the new template.
    fn handle_process_new_template(
        &mut self,
        template: NewTemplate<'static>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> InnerExtendedChannelFactoryResponse<'static> {
        if !template.future_template && self.chain_tip.is_none() {
            return InnerExtendedChannelFactoryResponse::ChainTipNotSet;
        }

        // update the active or future template at factory level
        // so that newly created channels can already start with a job
        if template.future_template {
            self.future_templates
                .insert(template.template_id, template.clone());
        } else {
            self.active_template = Some(template.clone());
        }
        self.coinbase_reward_outputs = coinbase_reward_outputs.clone();

        // check that the coinbase reward outputs are not overspending
        // template.coinbase_tx_value_remaining
        let coinbase_tx_value_remaining = template.coinbase_tx_value_remaining;
        let sum_of_coinbase_reward_outputs = coinbase_reward_outputs
            .clone()
            .iter()
            .map(|output| output.value.to_sat())
            .sum::<u64>();
        if coinbase_tx_value_remaining < sum_of_coinbase_reward_outputs {
            return InnerExtendedChannelFactoryResponse::InvalidCoinbaseRewardOutputs;
        }

        // iterate over all channels and update them with the new template
        for channel in self.extended_channels.values_mut() {
            match channel.on_new_template(
                template.clone(),
                coinbase_reward_outputs.clone(),
                self.chain_tip.clone(),
            ) {
                Ok(_) => (),
                Err(e) => {
                    return InnerExtendedChannelFactoryResponse::ProcessNewTemplateChannelError(e);
                }
            }
        }

        InnerExtendedChannelFactoryResponse::ProcessedNewTemplate
    }

    /// Handles the `ProcessSetNewPrevHash` Actor Model message variant.
    ///
    /// Iterates over all channels and updates them.
    fn handle_process_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHash<'static>,
    ) -> InnerExtendedChannelFactoryResponse<'static> {
        let prev_hash = set_new_prev_hash.clone().prev_hash;
        let nbits = set_new_prev_hash.clone().n_bits;
        let min_ntime = set_new_prev_hash.clone().header_timestamp;

        // update the chain tip at factory level
        self.chain_tip = Some(ChainTip::new(prev_hash.into(), nbits, min_ntime));

        // activate future template at factory level
        // so that newly created channels can already start with a job
        match self.future_templates.get(&set_new_prev_hash.template_id) {
            Some(template) => {
                let mut activated_template = template.clone();
                activated_template.future_template = false;
                self.active_template = Some(activated_template);
            }
            None => return InnerExtendedChannelFactoryResponse::TemplateNotFound,
        };

        self.future_templates.clear();

        for channel in self.extended_channels.values_mut() {
            match channel.on_set_new_prev_hash(set_new_prev_hash.clone()) {
                Ok(_) => (),
                Err(_) => return InnerExtendedChannelFactoryResponse::TemplateNotFound,
            }
        }
        InnerExtendedChannelFactoryResponse::ProcessedSetNewPrevHash
    }

    fn handle_process_set_custom_mining_job(
        &mut self,
        set_custom_mining_job: SetCustomMiningJob<'static>,
    ) -> InnerExtendedChannelFactoryResponse<'static> {
        if !self
            .extended_channels
            .contains_key(&set_custom_mining_job.channel_id)
        {
            return InnerExtendedChannelFactoryResponse::CustomMiningJobBadChannelId;
        }

        match &self.chain_tip {
            Some(chain_tip) => {
                if set_custom_mining_job.prev_hash != chain_tip.prev_hash() {
                    // not able to deal with potential forks
                    // custom jobs must be matched with our chain tip
                    return InnerExtendedChannelFactoryResponse::CustomMiningJobBadPrevHash;
                }
                if set_custom_mining_job.nbits != chain_tip.nbits() {
                    // not able to deal with potential forks
                    // custom jobs must be matched with our chain tip
                    return InnerExtendedChannelFactoryResponse::CustomMiningJobBadNbits;
                }
                if set_custom_mining_job.min_ntime < chain_tip.min_ntime() {
                    // not able to deal with potential forks
                    // custom jobs must be matched with our chain tip
                    return InnerExtendedChannelFactoryResponse::CustomMiningJobBadNtime;
                }
            }
            None => {
                return InnerExtendedChannelFactoryResponse::ChainTipNotSet;
            }
        }

        let channel = self
            .extended_channels
            .get_mut(&set_custom_mining_job.channel_id)
            .expect("we already checked that the channel exists");

        match channel.on_set_custom_mining_job(set_custom_mining_job) {
            Ok(job_id) => InnerExtendedChannelFactoryResponse::ProcessedSetCustomMiningJob(job_id),
            Err(e) => InnerExtendedChannelFactoryResponse::ProcessSetCustomMiningJobChannelError(e),
        }
    }

    fn handle_validate_share(
        &mut self,
        submit_shares_extended: SubmitSharesExtended<'static>,
        chain_tip: ChainTip,
    ) -> InnerExtendedChannelFactoryResponse<'static> {
        let channel_id = submit_shares_extended.channel_id;
        if !self.extended_channels.contains_key(&channel_id) {
            return InnerExtendedChannelFactoryResponse::ChannelNotFound;
        }

        let channel = self
            .extended_channels
            .get_mut(&channel_id)
            .expect("we already checked that the channel exists");

        match channel.validate_share(submit_shares_extended, chain_tip) {
            Ok(result) => match result {
                ShareValidationResult::Valid => InnerExtendedChannelFactoryResponse::ValidShare,
                ShareValidationResult::ValidWithAcknowledgement(
                    last_sequence_number,
                    new_submits_accepted_count,
                    new_shares_sum,
                ) => InnerExtendedChannelFactoryResponse::ValidShareWithAcknowledgement(
                    last_sequence_number,
                    new_submits_accepted_count,
                    new_shares_sum,
                ),
                ShareValidationResult::BlockFound(template_id, coinbase) => {
                    InnerExtendedChannelFactoryResponse::BlockFound(template_id, coinbase)
                }
            },
            Err(e) => match e {
                ShareValidationError::Invalid => InnerExtendedChannelFactoryResponse::InvalidShare,
                ShareValidationError::Stale => InnerExtendedChannelFactoryResponse::StaleShare,
                ShareValidationError::InvalidJobId => {
                    InnerExtendedChannelFactoryResponse::InvalidJobId
                }
                ShareValidationError::DoesNotMeetTarget => {
                    InnerExtendedChannelFactoryResponse::ShareDoesNotMeetTarget
                }
                ShareValidationError::VersionRollingNotAllowed => {
                    InnerExtendedChannelFactoryResponse::VersionRollingNotAllowed
                }
                ShareValidationError::DuplicateShare => {
                    InnerExtendedChannelFactoryResponse::DuplicateShare
                }
                ShareValidationError::InvalidCoinbase => {
                    InnerExtendedChannelFactoryResponse::InvalidCoinbase
                }
            },
        }
    }

    fn handle_get_share_accounting(
        &self,
        channel_id: u32,
    ) -> InnerExtendedChannelFactoryResponse<'static> {
        if !self.extended_channels.contains_key(&channel_id) {
            return InnerExtendedChannelFactoryResponse::ChannelNotFound;
        }

        let channel = self
            .extended_channels
            .get(&channel_id)
            .expect("we already checked that the channel exists");

        InnerExtendedChannelFactoryResponse::ShareAccounting(channel.get_share_accounting().clone())
    }

    fn handle_set_extranonce_prefix(
        &mut self,
        channel_id: u32,
        extranonce_prefix: Vec<u8>,
    ) -> InnerExtendedChannelFactoryResponse<'static> {
        if !self.extended_channels.contains_key(&channel_id) {
            return InnerExtendedChannelFactoryResponse::ChannelNotFound;
        }

        let channel = self
            .extended_channels
            .get_mut(&channel_id)
            .expect("we already checked that the channel exists");

        let current_extranonce_prefix = channel.get_extranonce_prefix();
        if current_extranonce_prefix.len() != extranonce_prefix.len() {
            return InnerExtendedChannelFactoryResponse::ExtranoncePrefixLengthMismatch;
        }

        channel.set_extranonce_prefix(extranonce_prefix);

        InnerExtendedChannelFactoryResponse::SetExtranoncePrefix
    }

    fn handle_get_chain_tip(&self) -> InnerExtendedChannelFactoryResponse<'static> {
        match &self.chain_tip {
            Some(chain_tip) => InnerExtendedChannelFactoryResponse::ChainTip(chain_tip.clone()),
            None => InnerExtendedChannelFactoryResponse::ChainTipNotSet,
        }
    }
}
