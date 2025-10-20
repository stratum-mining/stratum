use std::sync::atomic::Ordering;

use stratum_apps::stratum_core::{
    binary_sv2::Str0255,
    bitcoin::{consensus::Decodable, Amount, Target, TxOut},
    channels_sv2::{
        server::{
            error::{ExtendedChannelError, StandardChannelError},
            extended::ExtendedChannel,
            group::GroupChannel,
            jobs::job_store::DefaultJobStore,
            share_accounting::{ShareValidationError, ShareValidationResult},
            standard::StandardChannel,
        },
        Vardiff, VardiffState,
    },
    handlers_sv2::{HandleMiningMessagesFromClientAsync, SupportedChannelTypes},
    mining_sv2::*,
    parsers_sv2::{Mining, TemplateDistribution},
    template_distribution_sv2::SubmitSolution,
};
use tracing::{error, info};

use crate::{
    channel_manager::{ChannelManager, RouteMessageTo, FULL_EXTRANONCE_SIZE},
    error::PoolError,
};

impl HandleMiningMessagesFromClientAsync for ChannelManager {
    type Error = PoolError;

    fn get_channel_type_for_client(&self, _client_id: Option<usize>) -> SupportedChannelTypes {
        SupportedChannelTypes::GroupAndExtended
    }

    fn is_work_selection_enabled_for_client(&self, _client_id: Option<usize>) -> bool {
        true
    }

    fn is_client_authorized(
        &self,
        _client_id: Option<usize>,
        _user_identity: &Str0255,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn handle_close_channel(
        &mut self,
        client_id: Option<usize>,
        msg: CloseChannel<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received Close Channel: {msg}");
        let downstream_id =
            client_id.expect("client_id must be present for downstream_id extraction") as u32;
        self.channel_manager_data
            .super_safe_lock(|channel_manager_data| {
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
        client_id: Option<usize>,
        msg: OpenStandardMiningChannel<'_>,
    ) -> Result<(), Self::Error> {
        let request_id = msg.get_request_id_as_u32();
        let user_identity = msg.user_identity.as_utf8_or_hex();
        let downstream_id =
            client_id.expect("client_id must be present for downstream_id extraction") as u32;

        info!("Received OpenStandardMiningChannel: {}", msg);

        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let Some(downstream) = channel_manager_data.downstream.get_mut(&downstream_id) else {
                return Err(PoolError::DownstreamIdNotFound);
            };

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
                    let group_channel_id = downstream_data.channel_id_factory.fetch_add(1, Ordering::SeqCst);
                    let job_store = DefaultJobStore::new();

                    let mut group_channel = match GroupChannel::new_for_pool(group_channel_id as u32, job_store, FULL_EXTRANONCE_SIZE, self.pool_tag_string.clone()) {
                        Ok(channel) => channel,
                        Err(e) => {
                            error!(?e, "Failed to create group channel");
                            return Err(PoolError::FailedToCreateGroupChannel(e));
                        }
                    };
                    group_channel.on_new_template(last_future_template.clone(), vec![pool_coinbase_output.clone()])?;

                    group_channel.on_set_new_prev_hash(last_set_new_prev_hash_tdp.clone())?;
                    downstream_data.group_channels = Some(group_channel);
                }
                let nominal_hash_rate = msg.nominal_hash_rate;
                let requested_max_target = Target::from_le_bytes(msg.max_target.inner_as_ref().try_into().unwrap());
                let extranonce_prefix = channel_manager_data.extranonce_prefix_factory_standard.next_prefix_standard()?;

                let channel_id = downstream_data.channel_id_factory.fetch_add(1, Ordering::SeqCst);
                let job_store = DefaultJobStore::new();

                let mut standard_channel = match StandardChannel::new_for_pool(channel_id as u32, user_identity.to_string(), extranonce_prefix.to_vec(), requested_max_target, nominal_hash_rate, self.share_batch_size, self.shares_per_minute, job_store, self.pool_tag_string.clone(), self.persistence.clone()) {
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
                    channel_id: channel_id as u32,
                    target: standard_channel.get_target().to_le_bytes().into(),
                    extranonce_prefix: standard_channel.get_extranonce_prefix().clone().try_into().expect("Extranonce_prefix must be valid"),
                    group_channel_id
                }.into_static();

                let mut  messages: Vec<RouteMessageTo> = Vec::new();

                messages.push((downstream_id, Mining::OpenStandardMiningChannelSuccess(open_standard_mining_channel_success)).into());

                let template_id = last_future_template.template_id;

                // create a future standard job based on the last future template
                standard_channel.on_new_template(last_future_template, vec![pool_coinbase_output.clone()])?;
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
                    channel_id: channel_id as u32,
                    job_id: *future_standard_job_id,
                    prev_hash,
                    min_ntime: header_timestamp,
                    nbits: n_bits,
                };


                standard_channel
                .on_set_new_prev_hash(last_set_new_prev_hash_tdp.clone())?;

                messages.push((downstream_id, Mining::SetNewPrevHash(set_new_prev_hash_mining)).into());

                downstream_data.standard_channels.insert(channel_id as u32, standard_channel);
                if let Some(group_channel) = downstream_data.group_channels.as_mut() {
                    group_channel.add_standard_channel_id(channel_id as u32);
                }
                let vardiff = VardiffState::new()?;
                channel_manager_data.vardiff.insert((downstream_id, channel_id as u32), vardiff);

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
        client_id: Option<usize>,
        msg: OpenExtendedMiningChannel<'_>,
    ) -> Result<(), Self::Error> {
        let request_id = msg.get_request_id_as_u32();
        let user_identity = msg.user_identity.as_utf8_or_hex();
        let downstream_id =
            client_id.expect("client_id must be present for downstream_id extraction") as u32;
        info!("Received OpenExtendedMiningChannel: {}", msg);

        let nominal_hash_rate = msg.nominal_hash_rate;
        let requested_max_target =
            Target::from_le_bytes(msg.max_target.inner_as_ref().try_into().unwrap());
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
                                return Ok(vec![(
                                    downstream_id,
                                    Mining::OpenMiningChannelError(
                                        open_extended_mining_channel_error,
                                    ),
                                )
                                    .into()]);
                            }
                        };

                        let channel_id = downstream_data
                            .channel_id_factory
                            .fetch_add(1, Ordering::SeqCst);
                        let job_store = DefaultJobStore::new();

                        let mut extended_channel = match ExtendedChannel::new_for_pool(
                            channel_id as u32,
                            user_identity.to_string(),
                            extranonce_prefix,
                            requested_max_target,
                            nominal_hash_rate,
                            true, // version rolling always allowed
                            requested_min_rollable_extranonce_size,
                            self.share_batch_size,
                            self.shares_per_minute,
                            job_store,
                            self.pool_tag_string.clone(),
                            self.persistence.clone(),
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
                                    return Ok(vec![(
                                        downstream_id,
                                        Mining::OpenMiningChannelError(
                                            open_extended_mining_channel_error,
                                        ),
                                    )
                                        .into()]);
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
                                    return Ok(vec![(
                                        downstream_id,
                                        Mining::OpenMiningChannelError(
                                            open_extended_mining_channel_error,
                                        ),
                                    )
                                        .into()]);
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
                                    return Ok(vec![(
                                        downstream_id,
                                        Mining::OpenMiningChannelError(
                                            open_extended_mining_channel_error,
                                        ),
                                    )
                                        .into()]);
                                }
                                e => {
                                    error!("error in handle_open_extended_mining_channel: {:?}", e);
                                    return Err(e)?;
                                }
                            },
                        };

                        let open_extended_mining_channel_success =
                            OpenExtendedMiningChannelSuccess {
                                request_id,
                                channel_id: channel_id as u32,
                                target: extended_channel.get_target().to_le_bytes().into(),
                                extranonce_prefix: extended_channel
                                    .get_extranonce_prefix()
                                    .clone()
                                    .try_into()?,
                                extranonce_size: extended_channel.get_rollable_extranonce_size(),
                            }
                            .into_static();
                        info!("Sending OpenExtendedMiningChannel.Success (downstream_id: {downstream_id}): {open_extended_mining_channel_success}");

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
                            extended_channel.on_set_new_prev_hash(last_set_new_prev_hash_tdp)?;
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

                            extended_channel.on_new_template(
                                last_future_template.clone(),
                                vec![pool_coinbase_output],
                            )?;

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
                                channel_id: channel_id as u32,
                                job_id: *future_extended_job_id,
                                prev_hash,
                                min_ntime: header_timestamp,
                                nbits: n_bits,
                            };

                            extended_channel.on_set_new_prev_hash(last_set_new_prev_hash_tdp)?;

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
                            .insert(channel_id as u32, extended_channel);
                        let vardiff = VardiffState::new()?;
                        channel_manager_data
                            .vardiff
                            .insert((downstream_id, channel_id as u32), vardiff);

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
        client_id: Option<usize>,
        msg: SubmitSharesStandard,
    ) -> Result<(), Self::Error> {
        info!("Received SubmitSharesStandard: {msg}");
        let downstream_id =
            client_id.expect("client_id must be present for downstream_id extraction") as u32;

        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let channel_id = msg.channel_id;

            let Some(downstream) = channel_manager_data.downstream.get(&downstream_id) else {
                return Err(PoolError::DownstreamNotFound(downstream_id));
            };

            downstream.downstream_data.super_safe_lock(|downstream_data| {
                let mut messages: Vec<RouteMessageTo> = Vec::new();
                let Some(standard_channel) = downstream_data.standard_channels.get_mut(&channel_id) else {
                    let submit_shares_error = SubmitSharesError {
                        channel_id,
                        sequence_number: msg.sequence_number,
                        error_code: "invalid-channel-id"
                            .to_string()
                            .try_into()
                            .expect("error code must be valid string"),
                    };
                    error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: invalid-channel-id ❌", downstream_id, channel_id, msg.sequence_number);
                    return Ok(vec![(downstream_id, Mining::SubmitSharesError(submit_shares_error)).into()]);
                };

                let Some(vardiff) = channel_manager_data.vardiff.get_mut(&(downstream_id, channel_id)) else {
                    return Err(PoolError::VardiffNotFound(channel_id));
                };

                let res = standard_channel.validate_share(msg.clone());
                vardiff.increment_shares_since_last_update();


                match res {
                    Ok(ShareValidationResult::Valid) => {
                        info!(
                            "SubmitSharesStandard: valid share | downstream_id: {}, channel_id: {}, sequence_number: {} ✅",
                            downstream_id, channel_id, msg.sequence_number
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
                        messages.push((downstream_id, Mining::SubmitSharesSuccess(success)).into());
                    }
                    Ok(ShareValidationResult::BlockFound(template_id, coinbase)) => {
                        info!("SubmitSharesStandard: 💰 Block Found!!! 💰");
                        // if we have a template id (i.e.: this was not a custom job)
                        // we can propagate the solution to the TP
                        if let Some(template_id) = template_id {
                            info!("SubmitSharesStandard: Propagating solution to the Template Provider.");
                            let solution = SubmitSolution {
                                template_id,
                                version: msg.version,
                                header_timestamp: msg.ntime,
                                header_nonce: msg.nonce,
                                coinbase_tx: coinbase.try_into()?,
                            };
                            messages.push(TemplateDistribution::SubmitSolution(solution).into());
                        }
                        let share_accounting = standard_channel.get_share_accounting();
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number: share_accounting.get_last_share_sequence_number(),
                            new_submits_accepted_count: share_accounting.get_shares_accepted(),
                            new_shares_sum: share_accounting.get_share_work_sum(),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesSuccess(success)).into());
                    }
                    Err(ShareValidationError::Invalid) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: invalid-share ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "invalid-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };

                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(ShareValidationError::Stale) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: stale-share ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "stale-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(ShareValidationError::InvalidJobId) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: invalid-job-id ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "invalid-job-id"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(ShareValidationError::DoesNotMeetTarget) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: difficulty-too-low ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "difficulty-too-low"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(ShareValidationError::DuplicateShare) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: duplicate-share ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "duplicate-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(e) => {
                        return Err(e)?;
                    }
                }

                Ok(messages)
            })
        })?;

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    async fn handle_submit_shares_extended(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSharesExtended<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received SubmitSharesExtended: {msg}");
        let downstream_id =
            client_id.expect("client_id must be present for downstream_id extraction") as u32;
        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let channel_id = msg.channel_id;
            let Some(downstream) = channel_manager_data.downstream.get(&downstream_id) else {
                return Err(PoolError::DownstreamNotFound(downstream_id));
            };

            downstream.downstream_data.super_safe_lock(|downstream_data| {
                let mut messages: Vec<RouteMessageTo> = Vec::new();
                let Some(extended_channel) = downstream_data.extended_channels.get_mut(&channel_id) else {
                    let error = SubmitSharesError {
                        channel_id,
                        sequence_number: msg.sequence_number,
                        error_code: "invalid-channel-id"
                            .to_string()
                            .try_into()
                            .expect("error code must be valid string"),
                    };
                    error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: invalid-channel-id ❌", downstream_id, channel_id, msg.sequence_number);
                    return Ok(vec![(downstream_id, Mining::SubmitSharesError(error)).into()]);
                };

                let Some(vardiff) = channel_manager_data.vardiff.get_mut(&(downstream_id, channel_id)) else {
                    return Err(PoolError::VardiffNotFound(channel_id));
                };

                let res = extended_channel.validate_share(msg.clone());
                vardiff.increment_shares_since_last_update();

                match res {
                    Ok(ShareValidationResult::Valid) => {
                        info!(
                            "SubmitSharesExtended: valid share | downstream_id: {}, channel_id: {}, sequence_number: {} ✅",
                            downstream_id, channel_id, msg.sequence_number
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
                        messages.push((downstream_id, Mining::SubmitSharesSuccess(success)).into());
                    }
                    Ok(ShareValidationResult::BlockFound(template_id, coinbase)) => {
                        info!("SubmitSharesExtended: 💰 Block Found!!! 💰");
                        // if we have a template id (i.e.: this was not a custom job)
                        // we can propagate the solution to the TP
                        if let Some(template_id) = template_id {
                            info!("SubmitSharesExtended: Propagating solution to the Template Provider.");
                            let solution = SubmitSolution {
                                template_id,
                                version: msg.version,
                                header_timestamp: msg.ntime,
                                header_nonce: msg.nonce,
                                coinbase_tx: coinbase.try_into()?,
                            };
                            messages.push(TemplateDistribution::SubmitSolution(solution).into());
                        }
                        let share_accounting = extended_channel.get_share_accounting();
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number: share_accounting.get_last_share_sequence_number(),
                            new_submits_accepted_count: share_accounting.get_shares_accepted(),
                            new_shares_sum: share_accounting.get_share_work_sum(),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesSuccess(success)).into());
                    }
                    Err(ShareValidationError::Invalid) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: invalid-share ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "invalid-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(ShareValidationError::Stale) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: stale-share ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "stale-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(ShareValidationError::InvalidJobId) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: invalid-job-id ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "invalid-job-id"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(ShareValidationError::DoesNotMeetTarget) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: difficulty-too-low ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "difficulty-too-low"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(ShareValidationError::DuplicateShare) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: duplicate-share ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "duplicate-share"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(ShareValidationError::BadExtranonceSize) => {
                        error!("SubmitSharesError: downstream_id: {}, channel_id: {}, sequence_number: {}, error_code: bad-extranonce-size ❌", downstream_id, channel_id, msg.sequence_number);
                        let error = SubmitSharesError {
                            channel_id: msg.channel_id,
                            sequence_number: msg.sequence_number,
                            error_code: "bad-extranonce-size"
                                .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                        };
                        messages.push((downstream_id, Mining::SubmitSharesError(error)).into());
                    }
                    Err(e) => {
                        return Err(e)?;
                    }
                }

                Ok(messages)
            })
        })?;

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    async fn handle_update_channel(
        &mut self,
        client_id: Option<usize>,
        msg: UpdateChannel<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        let downstream_id =
            client_id.expect("client_id must be present for downstream_id extraction") as u32;

        let messages: Vec<RouteMessageTo> = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let Some(downstream) = channel_manager_data.downstream.get(&downstream_id) else {
                return Err(PoolError::DownstreamNotFound(downstream_id));
            };

            downstream.downstream_data.super_safe_lock(|downstream_data| {
                let mut messages = Vec::new();
                let channel_id = msg.channel_id;
                let new_nominal_hash_rate = msg.nominal_hash_rate;
                let requested_maximum_target = Target::from_le_bytes(msg.maximum_target.inner_as_ref().try_into().unwrap());

                if let Some(standard_channel) = downstream_data.standard_channels.get_mut(&channel_id) {
                    let res = standard_channel
                                    .update_channel(new_nominal_hash_rate, Some(requested_maximum_target));
                    match res {
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
                                    messages.push((downstream_id, Mining::UpdateChannelError(update_channel_error)).into());
                                }
                                StandardChannelError::RequestedMaxTargetOutOfRange => {
                                    error!("UpdateChannelError: requested-max-target-out-of-range");
                                    let update_channel_error = UpdateChannelError {
                                        channel_id,
                                        error_code: "requested-max-target-out-of-range"
                                            .to_string()
                                            .try_into()
                                            .expect("error code must be valid string"),
                                    };
                                    messages.push((downstream_id, Mining::UpdateChannelError(update_channel_error)).into());
                                }
                                standard_channel_error => {
                                    return Err(standard_channel_error)?;
                                }
                            }
                        }
                    }
                    let new_target = standard_channel.get_target();
                    let set_target = SetTarget {
                        channel_id,
                        maximum_target: new_target.to_le_bytes().into(),
                    };
                    messages.push((downstream_id, Mining::SetTarget(set_target)).into());
                } else if let Some(extended_channel) = downstream_data.extended_channels.get_mut(&channel_id) {
                    let res = extended_channel
                                    .update_channel(new_nominal_hash_rate, Some(requested_maximum_target));
                    match res {
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
                                    messages.push((downstream_id, Mining::UpdateChannelError(update_channel_error)).into());
                                }
                                ExtendedChannelError::RequestedMaxTargetOutOfRange => {
                                    error!("UpdateChannelError: max-target-out-of-range");
                                    let update_channel_error = UpdateChannelError {
                                        channel_id,
                                        error_code: "max-target-out-of-range"
                                            .to_string()
                                            .try_into()
                                            .expect("error code must be valid string"),
                                    };
                                    messages.push((downstream_id, Mining::UpdateChannelError(update_channel_error)).into());
                                }
                                extended_channel_error => {
                                    return Err(extended_channel_error)?;
                                }
                            }
                        }
                    }
                    let new_target = extended_channel.get_target();
                    let set_target = SetTarget {
                        channel_id,
                        maximum_target: new_target.to_le_bytes().into(),
                    };
                    messages.push((downstream_id, Mining::SetTarget(set_target)).into());
                } else {
                    error!("UpdateChannelError: invalid-channel-id");
                    let update_channel_error = UpdateChannelError {
                        channel_id,
                        error_code: "invalid-channel-id"
                            .to_string()
                            .try_into()
                            .expect("error code must be valid string"),
                    };
                    messages.push((downstream_id, Mining::UpdateChannelError(update_channel_error)).into());
                }

                Ok(messages)
            })
        })?;

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    async fn handle_set_custom_mining_job(
        &mut self,
        client_id: Option<usize>,
        msg: SetCustomMiningJob<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        let downstream_id =
            client_id.expect("client_id must be present for downstream_id extraction") as u32;

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

                        return Ok((downstream_id, Mining::SetCustomMiningJobError(error)).into());
                    }

                    let Some(downstream) = channel_manager_data.downstream.get_mut(&downstream_id)
                    else {
                        return Err(PoolError::DownstreamNotFound(downstream_id));
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
                                return Ok(
                                    (downstream_id, Mining::SetCustomMiningJobError(error)).into()
                                );
                            };

                            let job_id = extended_channel
                                .on_set_custom_mining_job(msg.clone().into_static())?;

                            let success = SetCustomMiningJobSuccess {
                                channel_id: msg.channel_id,
                                request_id: msg.request_id,
                                job_id,
                            };
                            Ok((downstream_id, Mining::SetCustomMiningJobSuccess(success)).into())
                        })
                })?;

        message.forward(&self.channel_manager_channel).await;
        Ok(())
    }
}
