use std::sync::atomic::Ordering;

use stratum_common::roles_logic_sv2::{
    self, Vardiff, VardiffState,
    bitcoin::{Amount, TxOut, consensus::Decodable},
    channels_sv2::{
        client,
        server::{
            error::{ExtendedChannelError, StandardChannelError},
            extended::ExtendedChannel,
            group::GroupChannel,
            jobs::job_store::DefaultJobStore,
            share_accounting::{ShareValidationError, ShareValidationResult},
            standard::StandardChannel,
        },
    },
    codec_sv2::binary_sv2::Str0255,
    handlers_sv2::{HandleMiningMessagesFromClientAsync, SupportedChannelTypes},
    job_declaration_sv2::PushSolution,
    mining_sv2::*,
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::SubmitSolution,
};
use tracing::{debug, error, info, warn};

use crate::{
    channel_manager::{ChannelManager, ChannelManagerChannel, RouteMessageTo},
    error::PoolError,
    utils::{StdFrame, deserialize_coinbase_outputs},
};

impl HandleMiningMessagesFromClientAsync for ChannelManager {
    type Error = PoolError;

    fn get_channel_type_for_client(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::GroupAndExtended
    }

    fn is_work_selection_enabled_for_client(&self) -> bool {
        true
    }

    fn is_client_authorized(&self, _user_identity: &Str0255) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Self::Error> {
        info!("Received Close Channel: {msg}");
        self.channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                let Some(downstream_id) = channel_manager_data
                    .channel_id_to_downstream_id
                    .remove(&msg.channel_id)
                else {
                    return Err(PoolError::DownstreamNotFoundWithChannelId(msg.channel_id));
                };

                let Some(downstream) = channel_manager_data.downstream.get_mut(&downstream_id)
                else {
                    return Err(PoolError::DownstreamNotFound(downstream_id));
                };

                downstream
                    .downstream_data
                    .super_safe_lock(|downstream_data| {
                        downstream_data.standard_channels.remove(&msg.channel_id);
                        downstream_data.extended_channels.remove(&msg.channel_id);
                    });
                Ok(())
            })
    }

    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel<'_>,
    ) -> Result<(), Self::Error> {
        let request_id = msg.get_request_id_as_u32();
        let user_string = msg.user_identity.as_utf8_or_hex();

        let (user_identity, downstream_id) = match user_string.rsplit_once('#') {
            Some((user_identity, id)) => match id.parse::<u32>() {
                Ok(id) => (user_identity, id),
                Err(e) => {
                    warn!(
                        ?e,
                        user_string, "Failed to parse downstream_id from user_identity"
                    );
                    return Err(PoolError::ParseInt(e));
                }
            },
            None => {
                warn!(user_string, "User identity missing downstream_id");
                return Err(PoolError::DownstreamIdNotFound);
            }
        };

        info!("Received OpenStandardMiningChannel: {}", msg);

        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let Some(downstream) = channel_manager_data.downstream.get_mut(&downstream_id) else {
                return Err(PoolError::DownstreamIdNotFound);
            };

            let message: Vec<RouteMessageTo> = Vec::new();

            if downstream.requires_custom_work.load(Ordering::SeqCst) {
                error!("OpenStandardMiningChannel: Standard Channels are not supported for this connection");
                let open_standard_mining_channel_error = OpenMiningChannelError {
                    request_id,
                    error_code: "standard-channels-not-supported-for-custom-work"
                        .to_string()
                        .try_into()
                        .expect("error code must be valid string"),
                };
                return Ok(vec![(downstream_id, Mining::OpenMiningChannelError(open_standard_mining_channel_error)).into()]);
            }

            let Some(last_future_template) = channel_manager_data.last_future_template.clone() else {
                return Err(PoolError::FutureTemplateNotPresent);
            };

            let Some(last_set_new_prev_hash_tdp) = channel_manager_data.last_new_prev_hash.clone() else {
                return Err(PoolError::LastNewPrevhashNotFound);
            };


            let pool_coinbase_output = TxOut {
                value: Amount::from_sat(last_future_template.coinbase_tx_value_remaining),
                script_pubkey: self.coinbase_reward_script.script_pubkey(),
            };

            downstream.downstream_data.super_safe_lock(|downstream_data| {
                if !downstream.requires_standard_jobs.load(Ordering::SeqCst) && downstream_data.group_channels.is_none() {
                    let group_channel_id = channel_manager_data.channel_id_factory.next();
                    let job_store = DefaultJobStore::new();

                    let mut group_channel = GroupChannel::new_for_pool(group_channel_id, job_store, self.pool_tag_string.clone());
                    if let Err(e) = group_channel.on_new_template(last_future_template.clone(), vec![pool_coinbase_output.clone()]) {
                        return Err(PoolError::ChannelErrorSender);
                    }

                    if let Err(e) = group_channel.on_set_new_prev_hash(last_set_new_prev_hash_tdp.clone()) {
                        return Err(PoolError::ChannelErrorSender);
                    }

                    downstream_data.group_channels = Some(group_channel);
                }
                let nominal_hash_rate = msg.nominal_hash_rate;
                let requested_max_target = msg.max_target.into_static();
                let extranonce_prefix = channel_manager_data.extranonce_prefix_factory_standard.next_prefix_standard().map_err(|_| PoolError::ChannelErrorSender)?;

                let channel_id = channel_manager_data.channel_id_factory.next();
                let job_store = DefaultJobStore::new();

                let mut standard_channel = match StandardChannel::new_for_pool(channel_id, user_identity.to_string(), extranonce_prefix.to_vec(), requested_max_target.into(), nominal_hash_rate, self.share_batch_size, self.shares_per_minute, job_store, self.pool_tag_string.clone()) {
                    Ok(channel) => channel,
                    Err(e) => match e {
                        StandardChannelError::InvalidNominalHashrate => {
                            error!("OpenMiningChannelError: invalid-nominal-hashrate");
                            let open_standard_mining_channel_error = OpenMiningChannelError {
                                request_id,
                                error_code: "invalid-nominal-hashrate"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return Ok(vec![(downstream_id, Mining::OpenMiningChannelError(open_standard_mining_channel_error)).into()]);
                        }
                        StandardChannelError::RequestedMaxTargetOutOfRange => {
                            error!("OpenMiningChannelError: max-target-out-of-range");
                            let open_standard_mining_channel_error = OpenMiningChannelError {
                                request_id,
                                error_code: "max-target-out-of-range"
                                    .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                            };
                            return Ok(vec![(downstream_id, Mining::OpenMiningChannelError(open_standard_mining_channel_error)).into()]);
                        }
                        _ => {
                            error!("error in handle_open_standard_mining_channel: {:?}", e);
                            return Err(PoolError::ChannelErrorSender);
                        }
                    },
                };

                let group_channel_id = downstream_data.group_channels.as_ref().map(|channel| channel.get_group_channel_id()).unwrap_or(0);

                let open_standard_mining_channel_success = OpenStandardMiningChannelSuccess {
                    request_id: msg.request_id,
                    channel_id,
                    target: standard_channel.get_target().clone().into(),
                    extranonce_prefix: standard_channel.get_extranonce_prefix().clone().try_into().expect("Extranonce_prefix must be valid"),
                    group_channel_id
                }.into_static();

                let mut  messages: Vec<RouteMessageTo> = Vec::new();

                messages.push((downstream_id, Mining::OpenStandardMiningChannelSuccess(open_standard_mining_channel_success)).into());

                let template_id = last_future_template.template_id;

                // create a future standard job based on the last future template
                if let Err(e) = standard_channel.on_new_template(last_future_template, vec![pool_coinbase_output.clone()]) {
                    return Err(PoolError::ChannelErrorSender);
                }

                let future_standard_job_id = standard_channel
                    .get_future_template_to_job_id()
                    .get(&template_id)
                    .expect("future job id must exist");
                let future_standard_job = standard_channel
                    .get_future_jobs()
                    .get(future_standard_job_id)
                    .expect("future job must exist");
                let future_standard_job_message =
                    future_standard_job.get_job_message().clone().into_static();

                messages.push((downstream_id, Mining::NewMiningJob(future_standard_job_message)).into());
                let prev_hash = last_set_new_prev_hash_tdp.prev_hash.clone();
                let header_timestamp = last_set_new_prev_hash_tdp.header_timestamp;
                let n_bits = last_set_new_prev_hash_tdp.n_bits;
                let set_new_prev_hash_mining = SetNewPrevHash {
                    channel_id,
                    job_id: *future_standard_job_id,
                    prev_hash,
                    min_ntime: header_timestamp,
                    nbits: n_bits,
                };


                if let Err(e) = standard_channel
                .on_set_new_prev_hash(last_set_new_prev_hash_tdp.clone()) {
                    return Err(PoolError::ChannelErrorSender);
                };

                messages.push((downstream_id, Mining::SetNewPrevHash(set_new_prev_hash_mining)).into());

                downstream_data.standard_channels.insert(channel_id, standard_channel);
                channel_manager_data.channel_id_to_downstream_id.insert(channel_id, downstream_id);
                if let Some(group_channel) = downstream_data.group_channels.as_mut() {
                    group_channel.add_standard_channel_id(channel_id);
                }

                Ok(messages)
            })
        })?;

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    async fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel<'_>,
    ) -> Result<(), Self::Error> {
        let request_id = msg.get_request_id_as_u32();
        let user_string = msg.user_identity.as_utf8_or_hex();

        let (user_identity, downstream_id) = match user_string.rsplit_once('#') {
            Some((user_identity, id)) => match id.parse::<u32>() {
                Ok(id) => (user_identity, id),
                Err(e) => {
                    warn!(
                        ?e,
                        user_string, "Failed to parse downstream_id from user_identity"
                    );
                    return Err(PoolError::ParseInt(e));
                }
            },
            None => {
                warn!(user_string, "User identity missing downstream_id");
                return Err(PoolError::DownstreamIdNotFound);
            }
        };
        info!("Received OpenExtendedMiningChannel: {}", msg);

        let nominal_hash_rate = msg.nominal_hash_rate;
        let requested_max_target = msg.max_target.into_static();
        let requested_min_rollable_extranonce_size = msg.min_extranonce_size;

        let messages = self
            .channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                let Some(downstream) = channel_manager_data.downstream.get_mut(&downstream_id)
                else {
                    return Err(PoolError::DownstreamIdNotFound);
                };
                downstream
                    .downstream_data
                    .super_safe_lock(|downstream_data| {
                        let mut messages: Vec<RouteMessageTo> = Vec::new();

                        let extranonce_prefix = match channel_manager_data
                            .extranonce_prefix_factory_extended
                            .next_prefix_extended(requested_min_rollable_extranonce_size.into())
                        {
                            Ok(extranonce_prefix) => extranonce_prefix.to_vec(),
                            Err(_) => {
                                error!("OpenMiningChannelError: min-extranonce-size-too-large");
                                let open_extended_mining_channel_error = OpenMiningChannelError {
                                    request_id,
                                    error_code: "min-extranonce-size-too-large"
                                        .to_string()
                                        .try_into()
                                        .expect("error code must be valid string"),
                                };
                                return Ok(vec![
                                    (
                                        downstream_id,
                                        Mining::OpenMiningChannelError(
                                            open_extended_mining_channel_error,
                                        ),
                                    )
                                        .into(),
                                ]);
                            }
                        };

                        let channel_id = channel_manager_data.channel_id_factory.next();
                        let job_store = DefaultJobStore::new();

                        let mut extended_channel = match ExtendedChannel::new_for_pool(
                            channel_id,
                            user_identity.to_string(),
                            extranonce_prefix,
                            requested_max_target.into(),
                            nominal_hash_rate,
                            true, // version rolling always allowed
                            requested_min_rollable_extranonce_size,
                            self.share_batch_size,
                            self.shares_per_minute,
                            job_store,
                            self.pool_tag_string.clone(),
                        ) {
                            Ok(channel) => channel,
                            Err(e) => match e {
                                ExtendedChannelError::InvalidNominalHashrate => {
                                    error!("OpenMiningChannelError: invalid-nominal-hashrate");
                                    let open_extended_mining_channel_error =
                                        OpenMiningChannelError {
                                            request_id,
                                            error_code: "invalid-nominal-hashrate"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                    return Ok(vec![
                                        (
                                            downstream_id,
                                            Mining::OpenMiningChannelError(
                                                open_extended_mining_channel_error,
                                            ),
                                        )
                                            .into(),
                                    ]);
                                }
                                ExtendedChannelError::RequestedMaxTargetOutOfRange => {
                                    error!("OpenMiningChannelError: max-target-out-of-range");
                                    let open_extended_mining_channel_error =
                                        OpenMiningChannelError {
                                            request_id,
                                            error_code: "max-target-out-of-range"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                    return Ok(vec![
                                        (
                                            downstream_id,
                                            Mining::OpenMiningChannelError(
                                                open_extended_mining_channel_error,
                                            ),
                                        )
                                            .into(),
                                    ]);
                                }
                                ExtendedChannelError::RequestedMinExtranonceSizeTooLarge => {
                                    error!("OpenMiningChannelError: min-extranonce-size-too-large");
                                    let open_extended_mining_channel_error =
                                        OpenMiningChannelError {
                                            request_id,
                                            error_code: "min-extranonce-size-too-large"
                                                .to_string()
                                                .try_into()
                                                .expect("error code must be valid string"),
                                        };
                                    return Ok(vec![
                                        (
                                            downstream_id,
                                            Mining::OpenMiningChannelError(
                                                open_extended_mining_channel_error,
                                            ),
                                        )
                                            .into(),
                                    ]);
                                }
                                _ => {
                                    error!("error in handle_open_extended_mining_channel: {:?}", e);
                                    return Err(PoolError::ChannelErrorSender);
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
                                    .try_into()?,
                                extranonce_size: extended_channel.get_rollable_extranonce_size(),
                            }
                            .into_static();

                        messages.push(
                            (
                                downstream_id,
                                Mining::OpenExtendedMiningChannelSuccess(
                                    open_extended_mining_channel_success,
                                ),
                            )
                                .into(),
                        );

                        let Some(last_set_new_prev_hash_tdp) =
                            channel_manager_data.last_new_prev_hash.clone()
                        else {
                            return Err(PoolError::LastNewPrevhashNotFound);
                        };

                        let Some(last_future_template) =
                            channel_manager_data.last_future_template.clone()
                        else {
                            return Err(PoolError::FutureTemplateNotPresent);
                        };

                        // if the client requires custom work, we don't need to send any extended
                        // jobs so we just process the SetNewPrevHash
                        // message
                        if downstream.requires_custom_work.load(Ordering::SeqCst) {
                            if let Err(e) =
                                extended_channel.on_set_new_prev_hash(last_set_new_prev_hash_tdp)
                            {
                                return Err(PoolError::ChannelErrorSender);
                            }
                            // if the client does not require custom work, we need to send the
                            // future extended job
                            // and the SetNewPrevHash message
                        } else {
                            let pool_coinbase_output = TxOut {
                                value: Amount::from_sat(
                                    last_future_template.coinbase_tx_value_remaining,
                                ),
                                script_pubkey: self.coinbase_reward_script.script_pubkey(),
                            };

                            if let Err(e) = extended_channel.on_new_template(
                                last_future_template.clone(),
                                vec![pool_coinbase_output],
                            ) {
                                return Err(PoolError::ChannelErrorSender);
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
                            // to be immediately activated with the subsequent SetNewPrevHash
                            // message
                            messages.push(
                                (
                                    downstream_id,
                                    Mining::NewExtendedMiningJob(future_extended_job_message),
                                )
                                    .into(),
                            );

                            // SetNewPrevHash message activates the future job
                            let prev_hash = last_set_new_prev_hash_tdp.prev_hash.clone();
                            let header_timestamp = last_set_new_prev_hash_tdp.header_timestamp;
                            let n_bits = last_set_new_prev_hash_tdp.n_bits;
                            let set_new_prev_hash_mining = SetNewPrevHash {
                                channel_id,
                                job_id: *future_extended_job_id,
                                prev_hash,
                                min_ntime: header_timestamp,
                                nbits: n_bits,
                            };
                            if let Err(e) =
                                extended_channel.on_set_new_prev_hash(last_set_new_prev_hash_tdp)
                            {
                                return Err(PoolError::ChannelErrorSender);
                            };
                            messages.push(
                                (
                                    downstream_id,
                                    Mining::SetNewPrevHash(set_new_prev_hash_mining),
                                )
                                    .into(),
                            );
                        }

                        downstream_data
                            .extended_channels
                            .insert(channel_id, extended_channel);
                        channel_manager_data
                            .channel_id_to_downstream_id
                            .insert(channel_id, downstream_id);

                        Ok(messages)
                    })
            })?;

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    async fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<(), Self::Error> {
        info!("Received SubmitSharesStandard");
        Ok(())
    }

    async fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received SubmitSharesExtended");
        Ok(())
    }

    async fn handle_update_channel(&mut self, msg: UpdateChannel<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_set_custom_mining_job(
        &mut self,
        msg: SetCustomMiningJob<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        // this is a naive implementation, but ideally we should check the SetCustomMiningJob
        // message parameters, especially:
        // - the mining_job_token
        // - the amount of the pool payout output
        let custom_job_coinbase_outputs = Vec::<TxOut>::consensus_decode(
            &mut msg.coinbase_tx_outputs.inner_as_ref().to_vec().as_slice(),
        )?;

        let message: RouteMessageTo =
            self.channel_manager_data
                .super_safe_lock(|channel_manager_data| {
                    let Some(downstream_id) = channel_manager_data
                        .channel_id_to_downstream_id
                        .get(&msg.channel_id)
                    else {
                        return Err(PoolError::DownstreamNotFound(msg.channel_id));
                    };

                    // check that the script_pubkey from self.coinbase_reward_script
                    // is present in the custom job coinbase outputs
                    let missing_script = !custom_job_coinbase_outputs.iter().any(|pool_output| {
                        *pool_output.script_pubkey == *self.coinbase_reward_script.script_pubkey()
                    });

                    if missing_script {
                        error!("SetCustomMiningJobError: pool-payout-script-missing");

                        let error = SetCustomMiningJobError {
                            request_id: msg.request_id,
                            channel_id: msg.channel_id,
                            error_code: "pool-payout-script-missing"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };

                        return Ok((*downstream_id, Mining::SetCustomMiningJobError(error)).into());
                    }

                    let Some(downstream) = channel_manager_data.downstream.get_mut(downstream_id)
                    else {
                        return Err(PoolError::DownstreamNotFound(*downstream_id));
                    };

                    downstream
                        .downstream_data
                        .super_safe_lock(|downstream_data| {
                            let Some(extended_channel) =
                                downstream_data.extended_channels.get_mut(&msg.channel_id)
                            else {
                                error!("SetCustomMiningJobError: invalid-channel-id");
                                let error = SetCustomMiningJobError {
                                    request_id: msg.request_id,
                                    channel_id: msg.channel_id,
                                    error_code: "invalid-channel-id"
                                        .to_string()
                                        .try_into()
                                        .expect("error code must be valid string"),
                                };
                                return Ok((
                                    *downstream_id,
                                    Mining::SetCustomMiningJobError(error),
                                )
                                    .into());
                            };

                            let job_id = extended_channel
                                .on_set_custom_mining_job(msg.clone().into_static())
                                .map_err(|_| PoolError::DownstreamIdNotFound)?;

                            let success = SetCustomMiningJobSuccess {
                                channel_id: msg.channel_id,
                                request_id: msg.request_id,
                                job_id,
                            };
                            return Ok((
                                *downstream_id,
                                Mining::SetCustomMiningJobSuccess(success),
                            )
                                .into());
                        })
                })?;

        message.forward(&self.channel_manager_channel).await;
        Ok(())
    }
}
