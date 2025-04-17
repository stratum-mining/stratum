use crate::{
    channel_management::{
        id::ChannelIdFactory,
        share_accounting::{ShareValidationError, ShareValidationResult},
        standard::{
            channel::{group::GroupChannel, standard::StandardChannel},
            factory::{
                message::InnerStandardChannelFactoryMessage,
                response::InnerStandardChannelFactoryResponse,
            },
        },
    },
    extranonce_prefix_management::standard::ExtranoncePrefixFactoryStandard,
    job_management::{chain_tip::ChainTip, extended_to_standard_job},
    utils::hash_rate_to_target,
};
use binary_sv2::U256;
use mining_sv2::{SubmitSharesStandard, Target};
use std::{collections::HashMap, sync::mpsc};
use stratum_common::bitcoin::transaction::TxOut;
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};
use tracing::{debug, error};

pub struct InnerStandardChannelFactoryIo {
    message: InnerStandardChannelFactoryMessage<'static>,
    response_sender: mpsc::Sender<InnerStandardChannelFactoryResponse<'static>>,
}

impl InnerStandardChannelFactoryIo {
    pub fn new(
        message: InnerStandardChannelFactoryMessage<'static>,
        response_sender: mpsc::Sender<InnerStandardChannelFactoryResponse<'static>>,
    ) -> Self {
        Self {
            message,
            response_sender,
        }
    }
}

pub struct InnerStandardChannelFactory {
    message_receiver: mpsc::Receiver<InnerStandardChannelFactoryIo>,
    channel_id_factory: ChannelIdFactory,
    extranonce_prefix_factory: ExtranoncePrefixFactoryStandard,
    standard_channels: HashMap<u32, StandardChannel<'static>>,
    group_channels: HashMap<u32, GroupChannel<'static>>,
    expected_share_per_minute_per_channel: f32,
    share_batch_size: usize,
    chain_tip: Option<ChainTip>,
    active_template: Option<NewTemplate<'static>>,
    future_templates: HashMap<u64, NewTemplate<'static>>,
    coinbase_reward_outputs: Vec<TxOut>,
}

// impl block with public methods
impl InnerStandardChannelFactory {
    pub fn new(
        message_receiver: mpsc::Receiver<InnerStandardChannelFactoryIo>,
        extranonce_prefix_factory: ExtranoncePrefixFactoryStandard,
        expected_share_per_minute_per_channel: f32,
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
            standard_channels: HashMap::new(),
            group_channels: HashMap::new(),
            expected_share_per_minute_per_channel,
            share_batch_size,
            chain_tip: None,
            active_template: None,
            future_templates: HashMap::new(),
            coinbase_reward_outputs: Vec::new(),
        }
    }

    /// Runner method for the Actor Model implementation.
    ///
    /// Routes incoming messages to the appropriate private handler.
    pub fn run(mut self) {
        loop {
            match self.message_receiver.recv() {
                Ok(io) => {
                    let InnerStandardChannelFactoryIo {
                        message,
                        response_sender,
                    } = io;

                    match message {
                        InnerStandardChannelFactoryMessage::Shutdown => {
                            debug!("Shutting down StandardChannelFactory");
                            // Send acknowledgment before shutting down
                            if response_sender
                                .send(InnerStandardChannelFactoryResponse::Shutdown)
                                .is_err()
                            {
                                error!("Failed to send shutdown response");
                                break;
                            }
                            break;
                        }
                        InnerStandardChannelFactoryMessage::NewStandardChannel(
                            user_identity,
                            nominal_hashrate,
                            max_target,
                            group_channel_id,
                        ) => {
                            let response = self.handle_new_standard_channel(
                                user_identity,
                                nominal_hashrate,
                                max_target,
                                group_channel_id,
                            );
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::UpdateStandardChannel(
                            channel_id,
                            nominal_hashrate,
                            max_target,
                        ) => {
                            let response = self.handle_update_standard_channel(
                                channel_id,
                                nominal_hashrate,
                                max_target,
                            );
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::RemoveStandardChannel(channel_id) => {
                            let response = self.handle_remove_standard_channel(channel_id);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::GetStandardChannel(channel_id) => {
                            let response = self.handle_get_standard_channel(channel_id);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::NewGroupChannel => {
                            let response = self.handle_new_group_channel();
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::RemoveGroupChannel(
                            group_channel_id,
                        ) => {
                            let response = self.handle_remove_group_channel(group_channel_id);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::GetGroupChannel(group_channel_id) => {
                            let response = self.handle_get_group_channel(group_channel_id);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::GetStandardChannelCount => {
                            let response = self.handle_get_standard_channel_count();
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::GetGroupChannelCount => {
                            let response = self.handle_get_group_channel_count();
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::GetAllStandardChannels => {
                            let response = self.handle_get_all_standard_channels();
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::GetAllGroupChannels => {
                            let response = self.handle_get_all_group_channels();
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::GetChainTip => {
                            let response = self.handle_get_chain_tip();
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::ProcessNewTemplate(
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
                        InnerStandardChannelFactoryMessage::ProcessSetNewPrevHash(
                            set_new_prev_hash,
                        ) => {
                            let response = self.handle_process_set_new_prev_hash(set_new_prev_hash);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::ValidateShare(
                            submit_shares_standard,
                        ) => {
                            let response = match self.chain_tip.clone() {
                                Some(chain_tip) => {
                                    self.handle_validate_share(submit_shares_standard, chain_tip)
                                }
                                None => InnerStandardChannelFactoryResponse::ChainTipNotSet,
                            };
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::GetShareAccounting(channel_id) => {
                            let response = self.handle_get_share_accounting(channel_id);
                            if response_sender.send(response).is_err() {
                                error!("Failed to send response back to caller");
                                break;
                            }
                        }
                        InnerStandardChannelFactoryMessage::SetExtranoncePrefix(
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
                    }
                }
                Err(_) => {
                    error!("StandardChannelFactory message receiver closed");
                    break;
                }
            }
        }
    }
}

// impl block with private methods
impl InnerStandardChannelFactory {
    /// Handles a `NewStandardChannel` message.
    fn handle_new_standard_channel(
        &mut self,
        user_identity: String,
        nominal_hashrate: f32,
        max_target: U256<'static>,
        group_channel_id: u32,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        let group_channel = match self.group_channels.get_mut(&group_channel_id) {
            Some(group_channel) => group_channel,
            None => return InnerStandardChannelFactoryResponse::GroupChannelIdNotFound,
        };

        let target_u256 = match hash_rate_to_target(
            nominal_hashrate.into(),
            self.expected_share_per_minute_per_channel.into(),
        ) {
            Ok(target_u256) => target_u256,
            Err(_) => {
                return InnerStandardChannelFactoryResponse::InvalidNominalHashrate;
            }
        };

        let target: Target = target_u256.clone().into();
        let max_target_value: Target = max_target.into();

        if target > max_target_value {
            return InnerStandardChannelFactoryResponse::RequestedMaxTargetOutOfRange;
        }

        let channel_id = match self.channel_id_factory.next() {
            Ok(channel_id) => channel_id,
            Err(e) => {
                return InnerStandardChannelFactoryResponse::FailedToGenerateNextChannelId(e);
            }
        };
        let extranonce_prefix = match self.extranonce_prefix_factory.next_prefix_standard() {
            Ok(extranonce_prefix) => extranonce_prefix,
            Err(e) => {
                return InnerStandardChannelFactoryResponse::FailedToGenerateNextExtranoncePrefixStandard(e);
            }
        };

        let mut channel = StandardChannel::new(
            channel_id,
            user_identity,
            extranonce_prefix.clone(),
            target,
            nominal_hashrate,
            self.share_batch_size,
        );

        if let Some(active_extended_job) = group_channel.get_active_job() {
            let standard_job = extended_to_standard_job(
                channel_id,
                active_extended_job.clone(),
                extranonce_prefix.clone(),
            );
            let template_id = active_extended_job
                .get_template()
                .expect("template must exist")
                .template_id;
            channel.on_new_template(standard_job, template_id);
        }

        self.standard_channels.insert(channel_id, channel);
        self.group_channels
            .get_mut(&group_channel_id)
            .expect("group channel must exist")
            .add_standard_channel_id(channel_id);

        InnerStandardChannelFactoryResponse::NewStandardChannelCreated(
            channel_id,
            target_u256,
            extranonce_prefix,
        )
    }

    /// Handles a `UpdateStandardChannel` message.
    fn handle_update_standard_channel(
        &mut self,
        channel_id: u32,
        nominal_hashrate: f32,
        requested_max_target: U256<'static>,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        if !self.standard_channels.contains_key(&channel_id) {
            return InnerStandardChannelFactoryResponse::StandardChannelIdNotFound;
        }

        let new_target: U256<'static> = match hash_rate_to_target(
            nominal_hashrate.into(),
            self.expected_share_per_minute_per_channel.into(),
        ) {
            Ok(new_target) => new_target,
            Err(_) => {
                return InnerStandardChannelFactoryResponse::InvalidNominalHashrate;
            }
        };

        // convert both targets to Target type for comparison
        let new_target: Target = new_target.into();
        let requested_max_target: Target = requested_max_target.into();

        if new_target > requested_max_target {
            return InnerStandardChannelFactoryResponse::RequestedMaxTargetOutOfRange;
        }

        // finally, actually update the channel state
        let channel = self
            .standard_channels
            .get_mut(&channel_id)
            .expect("standard channel must exist");
        channel.set_nominal_hashrate(nominal_hashrate);
        channel.set_target(new_target.clone().into());

        InnerStandardChannelFactoryResponse::StandardChannelUpdated(new_target.into())
    }

    fn handle_remove_standard_channel(
        &mut self,
        channel_id: u32,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        if !self.standard_channels.contains_key(&channel_id) {
            return InnerStandardChannelFactoryResponse::StandardChannelIdNotFound;
        }

        self.standard_channels.remove(&channel_id);

        for group_channel in self.group_channels.values_mut() {
            group_channel.remove_standard_channel_id(channel_id);
        }

        InnerStandardChannelFactoryResponse::StandardChannelRemoved
    }

    /// Handles a `GetStandardChannel` message.
    fn handle_get_standard_channel(
        &self,
        channel_id: u32,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        if !self.standard_channels.contains_key(&channel_id) {
            return InnerStandardChannelFactoryResponse::StandardChannelIdNotFound;
        }

        let channel = self
            .standard_channels
            .get(&channel_id)
            .expect("standard channel must exist");
        InnerStandardChannelFactoryResponse::StandardChannel(channel.clone())
    }

    /// Handles a `NewGroupChannel` message.
    fn handle_new_group_channel(&mut self) -> InnerStandardChannelFactoryResponse<'static> {
        let group_channel_id = match self.channel_id_factory.next() {
            Ok(group_channel_id) => group_channel_id,
            Err(e) => {
                return InnerStandardChannelFactoryResponse::FailedToGenerateNextChannelId(e);
            }
        };
        let mut group_channel = GroupChannel::new(group_channel_id);

        match &self.active_template {
            Some(template) => {
                match group_channel.on_new_template(template.clone(), self.coinbase_reward_outputs.clone(), self.chain_tip.clone()) {
                    Ok(_) => (),
                    Err(e) => return InnerStandardChannelFactoryResponse::ProcessNewTemplateGroupChannelError(e),
                }
            }
            None => (),
        }

        self.group_channels.insert(group_channel_id, group_channel);

        InnerStandardChannelFactoryResponse::NewGroupChannelCreated(group_channel_id)
    }

    /// Handles a `RemoveGroupChannel` message.
    fn handle_remove_group_channel(
        &mut self,
        group_channel_id: u32,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        if !self.group_channels.contains_key(&group_channel_id) {
            return InnerStandardChannelFactoryResponse::GroupChannelIdNotFound;
        }

        for standard_channel_id in self
            .group_channels
            .get(&group_channel_id)
            .expect("group channel must exist")
            .get_standard_channel_ids()
            .iter()
        {
            self.standard_channels.remove(standard_channel_id);
        }

        self.group_channels.remove(&group_channel_id);
        InnerStandardChannelFactoryResponse::GroupChannelRemoved
    }

    /// Handles a `GetGroupChannel` message.
    fn handle_get_group_channel(
        &self,
        group_channel_id: u32,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        if !self.group_channels.contains_key(&group_channel_id) {
            return InnerStandardChannelFactoryResponse::GroupChannelIdNotFound;
        }

        let group_channel = self
            .group_channels
            .get(&group_channel_id)
            .expect("group channel must exist");
        InnerStandardChannelFactoryResponse::GroupChannel(group_channel.clone())
    }

    fn handle_get_standard_channel_count(&self) -> InnerStandardChannelFactoryResponse<'static> {
        InnerStandardChannelFactoryResponse::StandardChannelCount(
            self.standard_channels.len() as u32
        )
    }

    fn handle_get_group_channel_count(&self) -> InnerStandardChannelFactoryResponse<'static> {
        InnerStandardChannelFactoryResponse::GroupChannelCount(self.group_channels.len() as u32)
    }

    fn handle_get_all_standard_channels(&self) -> InnerStandardChannelFactoryResponse<'static> {
        InnerStandardChannelFactoryResponse::AllStandardChannels(self.standard_channels.clone())
    }

    fn handle_get_all_group_channels(&self) -> InnerStandardChannelFactoryResponse<'static> {
        InnerStandardChannelFactoryResponse::AllGroupChannels(self.group_channels.clone())
    }

    fn handle_get_chain_tip(&self) -> InnerStandardChannelFactoryResponse<'static> {
        match &self.chain_tip {
            Some(chain_tip) => InnerStandardChannelFactoryResponse::ChainTip(chain_tip.clone()),
            None => InnerStandardChannelFactoryResponse::ChainTipNotSet,
        }
    }

    fn handle_process_new_template(
        &mut self,
        template: NewTemplate<'static>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        if !template.future_template && self.chain_tip.is_none() {
            return InnerStandardChannelFactoryResponse::ChainTipNotSet;
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

        let coinbase_tx_value_remaining = template.coinbase_tx_value_remaining;
        let sum_of_coinbase_reward_outputs = coinbase_reward_outputs
            .clone()
            .iter()
            .map(|output| output.value.to_sat())
            .sum::<u64>();
        if coinbase_tx_value_remaining < sum_of_coinbase_reward_outputs {
            return InnerStandardChannelFactoryResponse::InvalidCoinbaseRewardOutputs;
        }

        // iterate over all group channels and update them with the new template
        for group_channel in self.group_channels.values_mut() {
            match group_channel.on_new_template(
                template.clone(),
                coinbase_reward_outputs.clone(),
                self.chain_tip.clone(),
            ) {
                Ok(_) => (),
                Err(e) => {
                    return InnerStandardChannelFactoryResponse::ProcessNewTemplateGroupChannelError(e);
                }
            }

            let extended_job = match template.future_template {
                true => group_channel
                    .get_future_jobs()
                    .get(&template.template_id)
                    .expect("future job must exist"),
                false => group_channel
                    .get_active_job()
                    .expect("active job must exist"),
            };

            // iterate over all standard channels on this group channel and update them with the new
            // template
            for standard_channel_id in group_channel.get_standard_channel_ids() {
                let standard_channel = self
                    .standard_channels
                    .get_mut(standard_channel_id)
                    .expect("standard channel must exist");

                let standard_job = extended_to_standard_job(
                    *standard_channel_id,
                    extended_job.clone(),
                    standard_channel.get_extranonce_prefix().clone(),
                );
                standard_channel.on_new_template(standard_job, template.template_id);
            }
        }

        InnerStandardChannelFactoryResponse::ProcessedNewTemplate
    }

    fn handle_process_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHash<'static>,
    ) -> InnerStandardChannelFactoryResponse<'static> {
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
            None => return InnerStandardChannelFactoryResponse::TemplateNotFound,
        };

        self.future_templates.clear();

        // iterate over all group channels and update them with the new chain tip
        for group_channel in self.group_channels.values_mut() {
            match group_channel.on_set_new_prev_hash(set_new_prev_hash.clone()) {
                Ok(_) => (),
                Err(_) => return InnerStandardChannelFactoryResponse::TemplateNotFound,
            }

            // iterate over all standard channels on this group channel and update them with the new
            // chain tip
            for standard_channel_id in group_channel.get_standard_channel_ids() {
                let standard_channel = self
                    .standard_channels
                    .get_mut(standard_channel_id)
                    .expect("standard channel must exist");

                match standard_channel.on_set_new_prev_hash(set_new_prev_hash.clone()) {
                    Ok(_) => (),
                    Err(_) => return InnerStandardChannelFactoryResponse::TemplateNotFound,
                }
            }
        }

        InnerStandardChannelFactoryResponse::ProcessedSetNewPrevHash
    }

    fn handle_validate_share(
        &mut self,
        submit_shares_standard: SubmitSharesStandard,
        chain_tip: ChainTip,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        let channel_id = submit_shares_standard.channel_id;
        if !self.standard_channels.contains_key(&channel_id) {
            return InnerStandardChannelFactoryResponse::StandardChannelIdNotFound;
        }

        let channel = self
            .standard_channels
            .get_mut(&channel_id)
            .expect("standard channel must exist");

        match channel.validate_share(submit_shares_standard, chain_tip) {
            Ok(result) => match result {
                ShareValidationResult::Valid => InnerStandardChannelFactoryResponse::ValidShare,
                ShareValidationResult::ValidWithAcknowledgement(
                    last_sequence_number,
                    new_submits_accepted_count,
                    new_shares_sum,
                ) => InnerStandardChannelFactoryResponse::ValidShareWithAcknowledgement(
                    last_sequence_number,
                    new_submits_accepted_count,
                    new_shares_sum,
                ),
                ShareValidationResult::BlockFound(template_id, coinbase) => {
                    InnerStandardChannelFactoryResponse::BlockFound(template_id, coinbase)
                }
            },
            Err(e) => match e {
                ShareValidationError::Invalid => InnerStandardChannelFactoryResponse::InvalidShare,
                ShareValidationError::Stale => InnerStandardChannelFactoryResponse::StaleShare,
                ShareValidationError::InvalidJobId => {
                    InnerStandardChannelFactoryResponse::InvalidJobId
                }
                ShareValidationError::DoesNotMeetTarget => {
                    InnerStandardChannelFactoryResponse::ShareDoesNotMeetTarget
                }
                ShareValidationError::VersionRollingNotAllowed => {
                    InnerStandardChannelFactoryResponse::VersionRollingNotAllowed
                }
                ShareValidationError::DuplicateShare => {
                    InnerStandardChannelFactoryResponse::DuplicateShare
                }
                ShareValidationError::InvalidCoinbase => {
                    InnerStandardChannelFactoryResponse::InvalidCoinbase
                }
            },
        }
    }

    fn handle_get_share_accounting(
        &self,
        channel_id: u32,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        if !self.standard_channels.contains_key(&channel_id) {
            return InnerStandardChannelFactoryResponse::StandardChannelIdNotFound;
        }

        let channel = self
            .standard_channels
            .get(&channel_id)
            .expect("standard channel must exist");

        InnerStandardChannelFactoryResponse::ShareAccounting(channel.get_share_accounting().clone())
    }

    fn handle_set_extranonce_prefix(
        &mut self,
        channel_id: u32,
        extranonce_prefix: Vec<u8>,
    ) -> InnerStandardChannelFactoryResponse<'static> {
        if !self.standard_channels.contains_key(&channel_id) {
            return InnerStandardChannelFactoryResponse::StandardChannelIdNotFound;
        }

        let channel = self
            .standard_channels
            .get_mut(&channel_id)
            .expect("standard channel must exist");

        let current_extranonce_prefix = channel.get_extranonce_prefix();
        if current_extranonce_prefix.len() != extranonce_prefix.len() {
            return InnerStandardChannelFactoryResponse::ExtranoncePrefixLengthMismatch;
        }

        channel.set_extranonce_prefix(extranonce_prefix);
        InnerStandardChannelFactoryResponse::SetExtranoncePrefix
    }
}
