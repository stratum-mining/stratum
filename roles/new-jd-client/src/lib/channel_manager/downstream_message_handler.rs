use std::sync::{Arc, RwLock};

use stratum_common::roles_logic_sv2::{
    bitcoin::{Amount, TxOut},
    channels_sv2::server::{
        error::{ExtendedChannelError, StandardChannelError},
        extended::ExtendedChannel,
        group::GroupChannel,
        jobs::job_store::DefaultJobStore,
        share_accounting::{ShareValidationError, ShareValidationResult},
        standard::StandardChannel,
    },
    codec_sv2::binary_sv2::Str0255,
    handlers_sv2::{
        HandleMiningMessagesFromClientAsync, HandlerError as Error, SupportedChannelTypes,
    },
    job_declaration_sv2::PushSolution,
    mining_sv2::*,
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::SubmitSolution,
    VardiffState,
};
use tracing::{error, info, warn};

use crate::{channel_manager::ChannelManager, error::JDCError, utils::StdFrame};

impl HandleMiningMessagesFromClientAsync for ChannelManager {
    fn get_channel_type_for_client(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }
    fn is_work_selection_enabled_for_client(&self) -> bool {
        false
    }
    fn is_client_authorized(&self, user_identity: &Str0255) -> Result<bool, Error> {
        Ok(true)
    }
    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Error> {
        info!("Received handle_close_channel from Downstream");
        Ok(())
    }

    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel<'_>,
    ) -> Result<(), Error> {
        let user_string = msg.user_identity.as_utf8_or_hex();
        let mut split = user_string.split("#").collect::<Vec<&str>>();
        let downstream_id = split.pop().unwrap().parse::<u32>().unwrap();
        info!("Received handle_open_standard_mining_channel from Downstream");
        let request_id = msg.get_request_id_as_u32();
        let user_identity = std::str::from_utf8(msg.user_identity.as_ref())
            .map(|s| s.to_string())
            .map_err(|e| Error::External(JDCError::InvalidUserIdentity(e.to_string()).into()))?;

        let messages = self
            .channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                let mut messages: Vec<(u32, AnyMessage)> = Vec::new();
                let Some(ref last_future_template) = channel_manager_data.last_future_template
                else {
                    let error = OpenMiningChannelError {
                        request_id,
                        error_code: "unknown-user"
                            .to_string()
                            .try_into()
                            .expect("error code must be valid string"),
                    };
                    return vec![(
                        downstream_id,
                        AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                    )];
                };

                let Some(ref last_new_prev_hash) = channel_manager_data.last_new_prev_hash else {
                    let error = OpenMiningChannelError {
                        request_id,
                        error_code: "unknown-user"
                            .to_string()
                            .try_into()
                            .expect("error code must be valid string"),
                    };
                    return vec![(
                        downstream_id,
                        AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                    )];
                };

                // it should exist
                let downstream = channel_manager_data.downstream.get(&downstream_id).unwrap();
                let pool_coinbase_output = TxOut {
                    value: Amount::from_sat(last_future_template.coinbase_tx_value_remaining),
                    script_pubkey: self.coinbase_reward_script.script_pubkey(),
                };

                let downstream_messages =
                    downstream.downstream_data.super_safe_lock(|data| {
                        let mut messages: Vec<(u32, AnyMessage)> = vec![];
                        if !data.require_std_job && data.group_channels.is_none() {
                            let group_channel_id = channel_manager_data.channel_id_factory.next();
                            let job_store = Box::new(DefaultJobStore::new());

                            let mut group_channel = GroupChannel::new_for_job_declaration_client(
                                group_channel_id,
                                job_store,
                                self.pool_tag_string.clone(),
                                self.miner_tag_string.clone(),
                            );

                            if let Err(e) = group_channel.on_new_template(
                                last_future_template.clone(),
                                vec![pool_coinbase_output.clone()],
                            ) {
                                let error = OpenMiningChannelError {
                                    request_id,
                                    error_code: "unknown-user"
                                        .to_string()
                                        .try_into()
                                        .expect("error code must be valid string"),
                                };
                                return vec![(
                                    downstream_id,
                                    AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                                )];
                            }

                            if let Err(e) =
                                group_channel.on_set_new_prev_hash(last_new_prev_hash.clone())
                            {
                                let error = OpenMiningChannelError {
                                    request_id,
                                    error_code: "unknown-user"
                                        .to_string()
                                        .try_into()
                                        .expect("error code must be valid string"),
                                };
                                return vec![(
                                    downstream_id,
                                    AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                                )];
                            };

                            data.group_channels = Some(group_channel);
                        }

                        let nominal_hash_rate = msg.nominal_hash_rate;
                        let requested_max_target = msg.max_target.into_static();

                        let group_channel_id = match data.group_channels.as_ref() {
                            Some(group_channel) => group_channel.get_group_channel_id(),
                            None => 0,
                        };
                        let channel_id = channel_manager_data.channel_id_factory.next();
                        let Ok(extranonce_prefix) = channel_manager_data
                            .extranonce_prefix_factory_standard
                            .next_prefix_standard()
                        else {
                            let error = OpenMiningChannelError {
                                request_id,
                                error_code: "unknown-user"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return vec![(
                                downstream_id,
                                AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                            )];
                        };

                        let job_store = Box::new(DefaultJobStore::new());

                        let mut standard_channel =
                            match StandardChannel::new_for_job_declaration_client(
                                channel_id,
                                user_identity,
                                extranonce_prefix.to_vec(),
                                requested_max_target.into(),
                                nominal_hash_rate,
                                self.share_batch_size,
                                self.shares_per_minute,
                                job_store,
                                self.pool_tag_string.clone(),
                                self.miner_tag_string.clone(),
                            ) {
                                Ok(channel) => channel,
                                Err(e) => match e {
                                    StandardChannelError::InvalidNominalHashrate => {
                                        error!("OpenMiningChannelError: invalid-nominal-hashrate");
                                        let error = OpenMiningChannelError {
                                            request_id,
                                            error_code: "invalid-nominal-hashrate"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        return vec![(
                                            downstream_id,
                                            AnyMessage::Mining(Mining::OpenMiningChannelError(
                                                error,
                                            )),
                                        )];
                                    }
                                    StandardChannelError::RequestedMaxTargetOutOfRange => {
                                        error!("OpenMiningChannelError: max-target-out-of-range");
                                        let error = OpenMiningChannelError {
                                            request_id,
                                            error_code: "max-target-out-of-range"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        return vec![(
                                            downstream_id,
                                            AnyMessage::Mining(Mining::OpenMiningChannelError(
                                                error,
                                            )),
                                        )];
                                    }
                                    _ => {
                                        error!(
                                            "error in handle_open_standard_mining_channel: {e:?}"
                                        );
                                        let error = OpenMiningChannelError {
                                            request_id,
                                            error_code: "something-went-wrong"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        return vec![(
                                            downstream_id,
                                            AnyMessage::Mining(Mining::OpenMiningChannelError(
                                                error,
                                            )),
                                        )];
                                    }
                                },
                            };

                        let open_standard_mining_channel_success =
                            OpenStandardMiningChannelSuccess {
                                request_id: msg.request_id.clone(),
                                channel_id,
                                target: standard_channel.get_target().clone().into(),
                                extranonce_prefix: standard_channel
                                    .get_extranonce_prefix()
                                    .clone()
                                    .try_into()
                                    .expect("extranonce_prefix must be valid"),
                                group_channel_id,
                            }
                            .into_static();

                        messages.push((
                            downstream_id,
                            AnyMessage::Mining(Mining::OpenStandardMiningChannelSuccess(
                                open_standard_mining_channel_success,
                            )),
                        ));

                        if let Err(e) = standard_channel.on_new_template(
                            last_future_template.clone(),
                            vec![pool_coinbase_output.clone()],
                        ) {
                            let error = OpenMiningChannelError {
                                request_id: msg.request_id.as_u32(),
                                error_code: "unknown-user"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return vec![(
                                downstream_id,
                                AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                            )];
                        }

                        let future_standard_job_id = standard_channel
                            .get_future_template_to_job_id()
                            .get(&last_future_template.template_id)
                            .expect("future job id must exist");
                        let future_standard_job = standard_channel
                            .get_future_jobs()
                            .get(future_standard_job_id)
                            .expect("future job must exist");
                        let future_standard_job_message =
                            future_standard_job.get_job_message().clone().into_static();

                        messages.push((
                            downstream_id,
                            AnyMessage::Mining(Mining::NewMiningJob(future_standard_job_message)),
                        ));

                        let prev_hash = last_new_prev_hash.prev_hash.clone();
                        let header_timestamp = last_new_prev_hash.header_timestamp;
                        let n_bits = last_new_prev_hash.n_bits;
                        let set_new_prev_hash_mining = SetNewPrevHash {
                            channel_id,
                            job_id: *future_standard_job_id,
                            prev_hash,
                            min_ntime: header_timestamp,
                            nbits: n_bits,
                        };

                        if let Err(e) =
                            standard_channel.on_set_new_prev_hash(last_new_prev_hash.clone())
                        {
                            let error = OpenMiningChannelError {
                                request_id: msg.request_id.clone().as_u32(),
                                error_code: "unknown-user"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return vec![(
                                downstream_id,
                                AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                            )];
                        }

                        let vardiff = VardiffState::new().unwrap();

                        data.vardiff.insert(channel_id, Box::new(vardiff));
                        data.standard_channels.insert(channel_id, standard_channel);
                        channel_manager_data
                            .channel_id_to_downstream_id
                            .insert(channel_id, downstream_id);

                        if let Some(group_channel) = data.group_channels.as_mut() {
                            group_channel.add_standard_channel_id(channel_id);
                        }

                        messages
                    });

                messages
            });

        for (downstream_id, message) in messages {
            self.channel_manager_channel
                .downstream_sender
                .send((downstream_id, message));
        }

        Ok(())
    }

    async fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel<'_>,
    ) -> Result<(), Error> {
        error!("------------------------------------------------");
        error!("OEMC: {msg:?}");
        error!("------------------------------------------------");
        let user_string = msg.user_identity.as_utf8_or_hex();
        let mut split = user_string.split("#").collect::<Vec<&str>>();
        let downstream_id = split.pop().unwrap().parse::<u32>().unwrap();
        info!("Received handle_open_extended_mining_channel from Downstream {downstream_id}");
        let request_id = msg.get_request_id_as_u32();
        let user_identity = std::str::from_utf8(msg.user_identity.as_ref())
            .map(|s| s.to_string())
            .unwrap();

        let nominal_hash_rate = msg.nominal_hash_rate;
        let requested_max_target = msg.max_target.into_static();
        let requested_min_rollable_extranonce_size = msg.min_extranonce_size;

        let messages =
            self.channel_manager_data
                .super_safe_lock(|channel_manager_data| {
                    let mut messages: Vec<(u32, AnyMessage)> = vec![];

                    let downstream = channel_manager_data
                        .downstream
                        .get_mut(&downstream_id)
                        .unwrap();

                    messages = downstream.downstream_data.super_safe_lock(|data| {
                        let mut messages: Vec<(u32, AnyMessage)> = vec![];
                        let channel_id = channel_manager_data.channel_id_factory.next();
                        let Ok(extranonce_prefix) = channel_manager_data
                            .extranonce_prefix_factory_extended
                            .next_prefix_extended(requested_min_rollable_extranonce_size.into())
                        else {
                            let error = OpenMiningChannelError {
                                request_id,
                                error_code: "max-target-out-of-range"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return vec![(
                                downstream_id,
                                AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                            )];
                        };

                        let Some(last_future_template) =
                            channel_manager_data.last_future_template.clone()
                        else {
                            let error = OpenMiningChannelError {
                                request_id,
                                error_code: "no-template-to-share"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return vec![(
                                downstream_id,
                                AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                            )];
                        };

                        let Some(last_new_prev_hash) =
                            channel_manager_data.last_new_prev_hash.clone()
                        else {
                            let error = OpenMiningChannelError {
                                request_id,
                                error_code: "no-prev-hash-in-the-system"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return vec![(
                                downstream_id,
                                AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                            )];
                        };

                        let job_store = Box::new(DefaultJobStore::new());

                        let mut extended_channel =
                            match ExtendedChannel::new_for_job_declaration_client(
                                channel_id,
                                user_identity,
                                extranonce_prefix.into(),
                                requested_max_target.into(),
                                nominal_hash_rate,
                                true,
                                requested_min_rollable_extranonce_size,
                                self.share_batch_size,
                                self.shares_per_minute,
                                job_store,
                                self.pool_tag_string.clone(),
                                self.miner_tag_string.clone(),
                            ) {
                                Ok(channel) => channel,
                                Err(e) => match e {
                                    ExtendedChannelError::InvalidNominalHashrate => {
                                        error!("OpenMiningChannelError: invalid-nominal-hashrate");
                                        let error = OpenMiningChannelError {
                                            request_id,
                                            error_code: "invalid-nominal-hashrate"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        return return vec![(
                                            downstream_id,
                                            AnyMessage::Mining(Mining::OpenMiningChannelError(
                                                error,
                                            )),
                                        )];
                                    }
                                    ExtendedChannelError::RequestedMaxTargetOutOfRange => {
                                        error!("OpenMiningChannelError: max-target-out-of-range");
                                        let error = OpenMiningChannelError {
                                            request_id,
                                            error_code: "max-target-out-of-range"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        return vec![(
                                            downstream_id,
                                            AnyMessage::Mining(Mining::OpenMiningChannelError(
                                                error,
                                            )),
                                        )];
                                    }
                                    ExtendedChannelError::RequestedMinExtranonceSizeTooLarge => {
                                        error!(
                                            "OpenMiningChannelError: min-extranonce-size-too-large"
                                        );
                                        let error = OpenMiningChannelError {
                                            request_id,
                                            error_code: "min-extranonce-size-too-large"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        return vec![(
                                            downstream_id,
                                            AnyMessage::Mining(Mining::OpenMiningChannelError(
                                                error,
                                            )),
                                        )];
                                    }
                                    _ => {
                                        error!(
                                            "error in handle_open_extended_mining_channel: {:?}",
                                            e
                                        );
                                        let error = OpenMiningChannelError {
                                            request_id,
                                            error_code: "max-target-out-of-range"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        return vec![(
                                            downstream_id,
                                            AnyMessage::Mining(Mining::OpenMiningChannelError(
                                                error,
                                            )),
                                        )];
                                    }
                                },
                            };

                        let open_extended_mining_channel_success =
                            OpenExtendedMiningChannelSuccess {
                                request_id,
                                channel_id,
                                target: extended_channel.get_target().clone().into(),
                                extranonce_prefix: extended_channel
                                    .get_extranonce_prefix()
                                    .clone()
                                    .try_into()
                                    .unwrap(),
                                extranonce_size: extended_channel.get_rollable_extranonce_size(),
                            }
                            .into_static();

                        messages.push((
                            downstream_id,
                            AnyMessage::Mining(Mining::OpenExtendedMiningChannelSuccess(
                                open_extended_mining_channel_success,
                            )),
                        ));

                        let pool_coinbase_output = TxOut {
                            value: Amount::from_sat(
                                last_future_template.coinbase_tx_value_remaining,
                            ),
                            script_pubkey: self.coinbase_reward_script.script_pubkey(),
                        };

                        // create a future extended job based on the last future template
                        if let Err(e) = extended_channel.on_new_template(
                            last_future_template.clone(),
                            vec![pool_coinbase_output],
                        ) {
                            let error = OpenMiningChannelError {
                                request_id,
                                error_code: "max-target-out-of-range"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return vec![(
                                downstream_id,
                                AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                            )];
                        }

                        let future_extended_job_id = extended_channel
                            .get_future_template_to_job_id()
                            .get(&last_future_template.template_id)
                            .expect("future job id must exist");
                        let future_extended_job = extended_channel
                            .get_future_jobs()
                            .get(future_extended_job_id)
                            .expect("future job must exist");

                        let future_extended_job_message =
                            future_extended_job.get_job_message().clone().into_static();

                        // send this future job as new job message
                        // to be immediately activated with the subsequent SetNewPrevHash message
                        messages.push((
                            downstream_id,
                            AnyMessage::Mining(Mining::NewExtendedMiningJob(
                                future_extended_job_message,
                            )),
                        ));

                        // SetNewPrevHash message activates the future job
                        let prev_hash = last_new_prev_hash.prev_hash.clone();
                        let header_timestamp = last_new_prev_hash.header_timestamp;
                        let n_bits = last_new_prev_hash.n_bits;
                        let set_new_prev_hash_mining = SetNewPrevHash {
                            channel_id,
                            job_id: *future_extended_job_id,
                            prev_hash,
                            min_ntime: header_timestamp,
                            nbits: n_bits,
                        };
                        if let Err(e) = extended_channel.on_set_new_prev_hash(last_new_prev_hash) {
                            let error = OpenMiningChannelError {
                                request_id,
                                error_code: "max-target-out-of-range"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return vec![(
                                downstream_id,
                                AnyMessage::Mining(Mining::OpenMiningChannelError(error)),
                            )];
                        };
                        messages.push((
                            downstream_id,
                            AnyMessage::Mining(Mining::SetNewPrevHash(set_new_prev_hash_mining)),
                        ));

                        let vardiff = Box::new(VardiffState::new().unwrap());
                        data.extended_channels.insert(channel_id, extended_channel);
                        channel_manager_data
                            .channel_id_to_downstream_id
                            .insert(channel_id, downstream_id);
                        data.vardiff.insert(channel_id, vardiff);

                        messages
                    });

                    messages
                });

        for (downstream_id, message) in messages {
            self.channel_manager_channel
                .downstream_sender
                .send((downstream_id, message));
        }

        Ok(())
    }

    async fn handle_update_channel(&mut self, msg: UpdateChannel<'_>) -> Result<(), Error> {
        info!("Received handle_update_channel from Downstream");

        let channel_id = msg.channel_id;
        let new_nominal_hash_rate = msg.nominal_hash_rate;
        let requested_maximum_target = msg.maximum_target.into_static();

        let messages = self
            .channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                let downstream_id = channel_manager_data
                    .channel_id_to_downstream_id
                    .get(&channel_id);
                let Some(downstream_id) = downstream_id else {
                    return vec![];
                };
                let Some(downstream) = channel_manager_data.downstream.get_mut(downstream_id)
                else {
                    return vec![];
                };

                let messages = downstream.downstream_data.super_safe_lock(|data| {
                    let mut messages: Vec<(u32, AnyMessage)> = vec![];

                    if let Some(standard_channel) = data.standard_channels.get_mut(&channel_id) {
                        let update_channel = standard_channel.update_channel(
                            new_nominal_hash_rate,
                            Some(requested_maximum_target.into()),
                        );
                        let new_target = standard_channel.get_target().clone();

                        match update_channel {
                            Ok(_) => {}
                            Err(e) => {
                                error!("UpdateChannelError: {:?}", e);
                                match e {
                                    StandardChannelError::InvalidNominalHashrate => {
                                        error!("UpdateChannelError: invalid-nominal-hashrate");
                                        let update_channel_error = UpdateChannelError {
                                            channel_id,
                                            error_code: "invalid-nominal-hashrate"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        messages.push((
                                            downstream.downstream_id,
                                            AnyMessage::Mining(Mining::UpdateChannelError(
                                                update_channel_error.into(),
                                            )),
                                        ));
                                    }
                                    StandardChannelError::RequestedMaxTargetOutOfRange => {
                                        error!(
                                            "UpdateChannelError: requested-max-target-out-of-range"
                                        );
                                        let update_channel_error = UpdateChannelError {
                                            channel_id,
                                            error_code: "requested-max-target-out-of-range"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        messages.push((
                                            downstream.downstream_id,
                                            AnyMessage::Mining(Mining::UpdateChannelError(
                                                update_channel_error.into(),
                                            )),
                                        ));
                                    }
                                    _ => {
                                        error!(
                                            "UpdateChannelError: requested-max-target-out-of-range"
                                        );
                                        let update_channel_error = UpdateChannelError {
                                            channel_id,
                                            error_code: "requested-max-target-out-of-range"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        messages.push((
                                            downstream.downstream_id,
                                            AnyMessage::Mining(Mining::UpdateChannelError(
                                                update_channel_error.into(),
                                            )),
                                        ));
                                    }
                                }
                                let set_target = SetTarget {
                                    channel_id,
                                    maximum_target: new_target.clone().into(),
                                };
                                messages.push((
                                    downstream.downstream_id,
                                    AnyMessage::Mining(Mining::SetTarget(set_target)),
                                ));
                            }
                        }
                    } else if let Some(extended_channel) =
                        data.extended_channels.get_mut(&channel_id)
                    {
                        let update_channel = extended_channel.update_channel(
                            new_nominal_hash_rate,
                            Some(requested_maximum_target.into()),
                        );
                        let new_target = extended_channel.get_target().clone();

                        match update_channel {
                            Ok(_) => {}
                            Err(e) => {
                                error!("UpdateChannelError: {:?}", e);
                                match e {
                                    ExtendedChannelError::InvalidNominalHashrate => {
                                        error!("UpdateChannelError: invalid-nominal-hashrate");
                                        let update_channel_error = UpdateChannelError {
                                            channel_id,
                                            error_code: "invalid-nominal-hashrate"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        messages.push((
                                            downstream.downstream_id,
                                            AnyMessage::Mining(Mining::UpdateChannelError(
                                                update_channel_error.into(),
                                            )),
                                        ));
                                    }
                                    ExtendedChannelError::RequestedMaxTargetOutOfRange => {
                                        error!(
                                            "UpdateChannelError: requested-max-target-out-of-range"
                                        );
                                        let update_channel_error = UpdateChannelError {
                                            channel_id,
                                            error_code: "requested-max-target-out-of-range"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        messages.push((
                                            downstream.downstream_id,
                                            AnyMessage::Mining(Mining::UpdateChannelError(
                                                update_channel_error.into(),
                                            )),
                                        ));
                                    }
                                    _ => {
                                        error!(
                                            "UpdateChannelError: requested-max-target-out-of-range"
                                        );
                                        let update_channel_error = UpdateChannelError {
                                            channel_id,
                                            error_code: "requested-max-target-out-of-range"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                        messages.push((
                                            downstream.downstream_id,
                                            AnyMessage::Mining(Mining::UpdateChannelError(
                                                update_channel_error.into(),
                                            )),
                                        ));
                                    }
                                }
                                let set_target = SetTarget {
                                    channel_id,
                                    maximum_target: new_target.clone().into(),
                                };
                                messages.push((
                                    downstream.downstream_id,
                                    AnyMessage::Mining(Mining::SetTarget(set_target)),
                                ));
                            }
                        }
                    } else {
                        error!("UpdateChannelError: invalid-channel-id");
                        let update_channel_error = UpdateChannelError {
                            channel_id,
                            error_code: "invalid-channel-id"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        return vec![(
                            *downstream_id,
                            AnyMessage::Mining(Mining::UpdateChannelError(
                                update_channel_error.into(),
                            )),
                        )];
                    }

                    messages
                });

                messages
            });

        for (downstream_id, message) in messages {
            self.channel_manager_channel
                .downstream_sender
                .send((downstream_id, message));
        }
        Ok(())
    }

    async fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<(), Error> {
        info!("Received handle_submit_shares_standard from Downstream");
        let channel_id = msg.channel_id;
        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let Some(downstream_id) = channel_manager_data.channel_id_to_downstream_id.get(&channel_id) else {
               return vec![];
            };

            let Some(downstream) = channel_manager_data.downstream.get_mut(&channel_id) else {
                return vec![];
            };

            let Some(ref prev_hash) = channel_manager_data.last_new_prev_hash else {
                return vec![];
            };

            downstream.downstream_data.super_safe_lock(|data| {
                let mut messages: Vec<(u32, AnyMessage)> = vec![];

                let Some(standard_channel) = data.standard_channels.get_mut(&channel_id) else {
                    let error = SubmitSharesError {
                        channel_id,
                        sequence_number: msg.sequence_number,
                        error_code: "invalid-channel-id"
                            .to_string()
                            .try_into()
                            .expect("Error code must be a valid string"),
                    };
                    error!("SubmitSharesError: channel_id: {channel_id}, sequence_number: {}, error_code: invalid-channel-id", msg.sequence_number);
                    return vec![(*downstream_id, AnyMessage::Mining(Mining::SubmitSharesError(error)))];
                };

                let extranonce = standard_channel.get_extranonce_prefix().clone();
                let mut vardiff = data.vardiff.get_mut(&channel_id).unwrap();
                vardiff.increment_shares_since_last_update();
                let res = standard_channel.validate_share(msg.clone());

                match res {
                    Ok(ShareValidationResult::Valid) => {
                        info!(
                            "SubmitSharesStandard: valid share | channel_id: {}, sequence_number: {} ☑️",
                            channel_id, msg.sequence_number
                        );
                    }
                    Ok(ShareValidationResult::ValidWithAcknowledgement(
                        last_sequence_number,
                        new_submits_accepted_count,
                        new_shares_sum,
                    )) => {
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number,
                            new_submits_accepted_count,
                            new_shares_sum,
                        };
                        info!("SubmitSharesStandard: {} ✅", success);
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesSuccess(success)),
                        ));
                    }
                    Ok(ShareValidationResult::BlockFound(template_id, coinbase)) => {
                        info!("SubmitSharesStandard: 💰 Block Found!!! 💰");
                        if let Some(template_id) = template_id {
                            info!("SubmitSharesStandard: Propagating solution to the Template Provider.");
                            let solution = SubmitSolution {
                                template_id,
                                version: msg.version,
                                header_timestamp: msg.ntime,
                                header_nonce: msg.nonce,
                                coinbase_tx: coinbase.try_into().unwrap(),
                            };
                            // speak with gitgab on this
                            let push_solution = PushSolution {
                                extranonce: extranonce.try_into().unwrap(),
                                ntime: msg.ntime,
                                nonce: msg.nonce,
                                version: msg.version,
                                nbits: prev_hash.n_bits,
                                prev_hash: prev_hash.prev_hash.clone(),
                            };
                            let any_message = AnyMessage::TemplateDistribution(
                                TemplateDistribution::SubmitSolution(solution.clone()),
                            );
                            messages.push((0, any_message));
                            let any_message =
                                AnyMessage::JobDeclaration(JobDeclaration::PushSolution(push_solution))
                                    .into_static();
                            messages.push((0, any_message));
                        }
                        let share_accounting = standard_channel.get_share_accounting().clone();
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number: share_accounting.get_last_share_sequence_number(),
                            new_submits_accepted_count: share_accounting.get_shares_accepted(),
                            new_shares_sum: share_accounting.get_share_work_sum(),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesSuccess(success)),
                        ));
                    }
                    Err(ShareValidationError::Invalid) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: invalid-share ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "invalid-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    Err(ShareValidationError::Stale) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: stale-share ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "stale-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    Err(ShareValidationError::InvalidJobId) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: invalid-job-id ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "invalid-job-id"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    Err(ShareValidationError::DoesNotMeetTarget) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: difficulty-too-low ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "difficulty-too-low"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    Err(ShareValidationError::DuplicateShare) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: duplicate-share ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "duplicate-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    _ => {
                        unreachable!()
                    }
                }
                messages
            })
        });

        for (downstream_id, message) in messages {
            if downstream_id == 0 {
                match message {
                    AnyMessage::JobDeclaration(m) => {
                        let any_message = AnyMessage::JobDeclaration(m);
                        let frame: StdFrame = any_message.try_into().unwrap();
                        self.channel_manager_channel
                            .jd_sender
                            .send(frame.into())
                            .await;
                    }
                    AnyMessage::TemplateDistribution(m) => {
                        let any_message = AnyMessage::TemplateDistribution(m);
                        let frame: StdFrame = any_message.try_into().unwrap();
                        self.channel_manager_channel
                            .tp_sender
                            .send(frame.into())
                            .await;
                    }
                    _ => {}
                }
            } else {
                self.channel_manager_channel
                    .downstream_sender
                    .send((downstream_id, message));
            }
        }

        Ok(())
    }

    async fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_submit_shares_extended from Downstream");
        let channel_id = msg.channel_id;
        let any_message =
            AnyMessage::Mining(Mining::SubmitSharesExtended(msg.clone().into_static()));
        let frame: StdFrame = any_message.try_into().unwrap();

        self.channel_manager_channel
            .upstream_sender
            .send(frame.into())
            .await;

        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let Some(downstream_id) = channel_manager_data.channel_id_to_downstream_id.get(&channel_id) else {
               return vec![];
            };

            let Some(downstream) = channel_manager_data.downstream.get_mut(&channel_id) else {
                return vec![];
            };

            let Some(ref prev_hash) = channel_manager_data.last_new_prev_hash else {
                return vec![];
            };

            downstream.downstream_data.super_safe_lock(|data| {
                let mut messages: Vec<(u32, AnyMessage)> = vec![];

                let Some(extended_channel) = data.extended_channels.get_mut(&channel_id) else {
                    let error = SubmitSharesError {
                        channel_id,
                        sequence_number: msg.sequence_number,
                        error_code: "invalid-channel-id"
                            .to_string()
                            .try_into()
                            .expect("Error code must be a valid string"),
                    };
                    error!("SubmitSharesError: channel_id: {channel_id}, sequence_number: {}, error_code: invalid-channel-id", msg.sequence_number);
                    return vec![(*downstream_id, AnyMessage::Mining(Mining::SubmitSharesError(error)))];
                };

                let mut vardiff = data.vardiff.get_mut(&channel_id).unwrap();
                vardiff.increment_shares_since_last_update();
                let res = extended_channel.validate_share(msg.clone());
                match res {
                    Ok(ShareValidationResult::Valid) => {
                        info!(
                            "SubmitSharesExtended: valid share | channel_id: {}, sequence_number: {} ☑️",
                            channel_id, msg.sequence_number
                        );
                    }
                    Ok(ShareValidationResult::ValidWithAcknowledgement(
                        last_sequence_number,
                        new_submits_accepted_count,
                        new_shares_sum,
                    )) => {
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number,
                            new_submits_accepted_count,
                            new_shares_sum,
                        };
                        info!("SubmitSharesExtended: {} ✅", success);
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesSuccess(success)),
                        ));
                    }
                    Ok(ShareValidationResult::BlockFound(template_id, coinbase)) => {
                        info!("SubmitSharesExtended: 💰 Block Found!!! 💰");
                        if let Some(template_id) = template_id {
                            info!("SubmitSharesExtended: Propagating solution to the Template Provider.");
                            let solution = SubmitSolution {
                                template_id,
                                version: msg.version,
                                header_timestamp: msg.ntime,
                                header_nonce: msg.nonce,
                                coinbase_tx: coinbase.try_into().unwrap(),
                            };

                            let push_solution = PushSolution {
                                extranonce: msg.extranonce,
                                ntime: msg.ntime,
                                nonce: msg.nonce,
                                version: msg.version,
                                nbits: prev_hash.n_bits,
                                prev_hash: prev_hash.prev_hash.clone(),
                            };

                            let any_message = AnyMessage::TemplateDistribution(
                                TemplateDistribution::SubmitSolution(solution.clone()),
                            );
                            messages.push((0, any_message));
                            let any_message =
                                AnyMessage::JobDeclaration(JobDeclaration::PushSolution(push_solution))
                                    .into_static();
                            messages.push((0, any_message));
                        }
                        let share_accounting = extended_channel.get_share_accounting().clone();
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number: share_accounting.get_last_share_sequence_number(),
                            new_submits_accepted_count: share_accounting.get_shares_accepted(),
                            new_shares_sum: share_accounting.get_share_work_sum(),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesSuccess(success)),
                        ));
                    }
                    Err(ShareValidationError::Invalid) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: invalid-share ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "invalid-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    Err(ShareValidationError::Stale) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: stale-share ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "stale-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    Err(ShareValidationError::InvalidJobId) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: invalid-job-id ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "invalid-job-id"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    Err(ShareValidationError::DoesNotMeetTarget) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: difficulty-too-low ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "difficulty-too-low"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    Err(ShareValidationError::DuplicateShare) => {
                        error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: duplicate-share ❌", channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "duplicate-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((
                            downstream.downstream_id,
                            AnyMessage::Mining(Mining::SubmitSharesError(error)),
                        ));
                    }
                    _ => {
                        // any other error variations should never happen
                        unreachable!()
                    }
                }
                messages
            })
        });

        for (downstream_id, message) in messages {
            if downstream_id == 0 {
                match message {
                    AnyMessage::JobDeclaration(m) => {
                        let any_message = AnyMessage::JobDeclaration(m);
                        let frame: StdFrame = any_message.try_into().unwrap();
                        self.channel_manager_channel
                            .jd_sender
                            .send(frame.into())
                            .await;
                    }
                    AnyMessage::TemplateDistribution(m) => {
                        let any_message = AnyMessage::TemplateDistribution(m);
                        let frame: StdFrame = any_message.try_into().unwrap();
                        self.channel_manager_channel
                            .tp_sender
                            .send(frame.into())
                            .await;
                    }
                    _ => {}
                }
            } else {
                self.channel_manager_channel
                    .downstream_sender
                    .send((downstream_id, message));
            }
        }

        Ok(())
    }

    async fn handle_set_custom_mining_job(
        &mut self,
        msg: SetCustomMiningJob<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_set_custom_mining_job from Downstream");
        Ok(())
    }
}
