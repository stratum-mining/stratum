use stratum_common::roles_logic_sv2::{
    bitcoin::{consensus, hashes::Hash, Amount, Transaction, TxOut},
    channels_sv2::chain_tip::ChainTip,
    codec_sv2::binary_sv2::{Seq064K, U256},
    handlers_sv2::HandleTemplateDistributionMessagesFromServerAsync,
    job_declaration_sv2::DeclareMiningJob,
    mining_sv2::SetNewPrevHash as SetNewPrevHashMp,
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::*,
};
use tracing::{error, info, warn};

use crate::{
    channel_manager::{downstream_message_handler::RouteMessageTo, ChannelManager, DeclaredJob},
    error::JDCError,
    jd_mode::{get_jd_mode, JdMode},
    utils::{deserialize_coinbase_outputs, StdFrame},
};

impl HandleTemplateDistributionMessagesFromServerAsync for ChannelManager {
    type Error = JDCError;

    // Handles a `NewTemplate` message from the Template Provider.
    //
    // Behavior depends on the JD mode:
    // - FullTemplate: sends a `RequestTransactionData` to start the declare-mining-job flow.
    // - CoinbaseOnly: sends a `SetCustomMiningJob` and continues with that flow.
    //
    // In both modes, the new template is stored and propagated to all
    // downstream channels, updating their state and dispatching the
    // appropriate mining job messages (standard, group, or extended).
    //
    // Also updates future/active template state and triggers token
    // allocation if needed.
    async fn handle_new_template(&mut self, msg: NewTemplate<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        self.channel_manager_data.super_safe_lock(|data| {
            data.template_store
                .insert(msg.template_id, msg.clone().into_static());
            if msg.future_template {
                data.last_future_template = Some(msg.clone().into_static());
            }
        });

        if get_jd_mode() == JdMode::FullTemplate {
            let tx_data_request = AnyMessage::TemplateDistribution(
                TemplateDistribution::RequestTransactionData(RequestTransactionData {
                    template_id: msg.template_id,
                }),
            );
            let frame: StdFrame = tx_data_request.try_into()?;
            self.channel_manager_channel
                .tp_sender
                .send(frame)
                .await
                .map_err(|_e| JDCError::ChannelErrorSender)?;
        }

        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let mut messages: Vec<RouteMessageTo> = Vec::new();
            let mut coinbase_output = deserialize_coinbase_outputs(&channel_manager_data.coinbase_outputs);
            coinbase_output[0].value = Amount::from_sat(msg.coinbase_tx_value_remaining);

            for (downstream_id, downstream) in channel_manager_data.downstream.iter_mut() {

                let  messages_ = downstream.downstream_data.super_safe_lock(|data| {

                    let mut messages: Vec<RouteMessageTo> = vec![];

                    let group_channel_job = if let Some(ref mut group_channel) = data.group_channels {
                        if group_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()).is_ok() {
                            match msg.future_template {
                                true => {
                                    let future_job_id = group_channel
                                            .get_future_template_to_job_id()
                                            .get(&msg.template_id)
                                            .expect("job_id must exist");
                                    Some(group_channel
                                        .get_future_jobs()
                                        .get(future_job_id)
                                        .expect("future job must exist")).cloned()
                                },
                                false => {
                                    Some(group_channel
                                        .get_active_job()
                                        .expect("active job must exist")).cloned()
                                }
                            }
                        } else {
                            tracing::error!("Some issue with downstream: {downstream_id}, group channel");
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(upstream_channel) = channel_manager_data.upstream_channel.as_mut() {
                        if !msg.future_template && get_jd_mode() == JdMode::CoinbaseOnly {
                                if let (Some(token), Some(prevhash)) = (
                                    channel_manager_data.allocate_tokens.clone(),
                                    channel_manager_data.last_new_prev_hash.clone(),
                                ) {
                                    let request_id = channel_manager_data.request_id_factory.next();
                                    let output = deserialize_coinbase_outputs(&channel_manager_data.coinbase_outputs);
                                    let job_factory = channel_manager_data.job_factory.as_mut().unwrap();
                                    let custom_job = job_factory.new_custom_job(upstream_channel.get_channel_id(), request_id, token.clone().mining_job_token, prevhash.clone().into(), msg.clone(), output);

                                    if let Ok(custom_job) = custom_job{
                                        let last_declare = DeclaredJob {
                                            declare_mining_job: None,
                                            template: msg.clone().into_static(),
                                            prev_hash: Some(prevhash),
                                            set_custom_mining_job: Some(custom_job.clone().into_static()),
                                            coinbase_output: channel_manager_data.coinbase_outputs.clone(),
                                            tx_list: Vec::new(),
                                        };
                                        channel_manager_data
                                            .last_declare_job_store
                                            .insert(request_id, last_declare);
                                        messages.push(
                                            Mining::SetCustomMiningJob(custom_job).into()
                                        );
                                    }
                                }
                        }
                    }
                    match msg.future_template {
                        true => {
                            for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                                if data.group_channels.is_none() {
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                        tracing::error!("Error while adding template to standard channel: {channel_id:?} {e:?}");
                                        continue;
                                    }
                                    let standard_job_id = standard_channel.get_future_template_to_job_id().get(&msg.template_id).expect("job_id must exist");
                                    let standard_job = standard_channel.get_future_jobs().get(standard_job_id).expect("standard job must exist");
                                    channel_manager_data.downstream_channel_id_and_job_id_to_template_id.insert((*channel_id, *standard_job_id), msg.template_id);
                                    let standard_job_message = standard_job.get_job_message();
                                    messages.push((*downstream_id, Mining::NewMiningJob(standard_job_message.clone())).into());
                                }
                                if let Some(ref group_channel_job) = group_channel_job {
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                        tracing::error!("Error while adding template to standard channel: {channel_id:?} {e:?}");
                                        continue;
                                    }
                                    _ = standard_channel
                                    .on_group_channel_job(group_channel_job.clone());
                                }
                            }
                            if let Some(group_channel_job) = group_channel_job {
                                let job_message = group_channel_job.get_job_message();
                                messages.push((*downstream_id, Mining::NewExtendedMiningJob(job_message.clone())).into());
                            }

                            for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                                if let Err(e) = extended_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                    tracing::error!("Error while adding template to standard channel: {channel_id:?} {e:?}");
                                    continue;
                                }
                                let extended_job_id = extended_channel
                                    .get_future_template_to_job_id()
                                    .get(&msg.template_id)
                                    .expect("job_id must exist");

                                let extended_job = extended_channel
                                    .get_future_jobs()
                                    .get(extended_job_id)
                                    .expect("extended job must exist");

                                channel_manager_data.downstream_channel_id_and_job_id_to_template_id.insert((*channel_id, *extended_job_id), msg.template_id);
                                let extended_job_message = extended_job.get_job_message();

                                messages.push((*downstream_id,Mining::NewExtendedMiningJob(extended_job_message.clone())).into());
                            }
                        }
                        false => {
                            for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                                if data.group_channels.is_none() {
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                        tracing::error!("Error while adding template to standard channel: {channel_id:?} {e:?}");
                                        continue;
                                    }
                                    let standard_job = standard_channel.get_active_job().expect("standard job must exist");
                                    channel_manager_data.downstream_channel_id_and_job_id_to_template_id.insert((*channel_id, standard_job.get_job_id()), msg.template_id);
                                    let standard_job_message = standard_job.get_job_message();
                                    messages.push((*downstream_id, Mining::NewMiningJob(standard_job_message.clone())).into());
                                }
                                if let Some(ref group_channel_job) = group_channel_job {
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                        tracing::error!("Error while adding template to standard channel: {channel_id:?} {e:?}");
                                        continue;
                                    }
                                    _ = standard_channel
                                    .on_group_channel_job(group_channel_job.clone());
                                }
                            }
                            if let Some(group_channel_job) = group_channel_job {
                                let job_message = group_channel_job.get_job_message();
                                messages.push((*downstream_id, Mining::NewExtendedMiningJob(job_message.clone())).into());
                            }

                            for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                                if let Err(e) = extended_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                    tracing::error!("Error while adding template to standard channel: {channel_id:?} {e:?}");
                                    continue;
                                }
                                let extended_job = extended_channel
                                    .get_active_job()
                                    .expect("extended job must exist");

                                channel_manager_data.downstream_channel_id_and_job_id_to_template_id.insert((*channel_id, extended_job.get_job_id()), msg.template_id);
                                let extended_job_message = extended_job.get_job_message();

                                messages.push((*downstream_id,Mining::NewExtendedMiningJob(extended_job_message.clone())).into());
                            }
                        }
                    }

                    messages

                });
                messages.extend(messages_);
            }
            messages
        });

        if get_jd_mode() == JdMode::CoinbaseOnly && !msg.future_template {
            _ = self.allocate_tokens(1).await;
        }

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    // Handles a `RequestTransactionDataError` message from the Template Provider.
    async fn handle_request_tx_data_error(
        &mut self,
        msg: RequestTransactionDataError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        let error_code = msg.error_code.as_utf8_or_hex();

        if matches!(
            error_code.as_str(),
            "template-id-not-found" | "stale-template-id"
        ) {
            return Ok(());
        }
        Err(JDCError::TxDataError)
    }

    // Handles a `RequestTransactionDataSuccess` message from the Template Provider.
    //
    // Flow:
    // - If the template is not a future template, immediately declare a mining job to JDS.
    // - If the template is a future template:
    //   - Check if the current `prevhash` activates this template.
    //   - If activated → proceed with the normal declare job flow.
    //   - If not activated → cache it as a declare job for later propagation.
    async fn handle_request_tx_data_success(
        &mut self,
        msg: RequestTransactionDataSuccess<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        let transactions_data = msg.transaction_list;
        let excess_data = msg.excess_data;

        let (token, template_message, request_id, prevhash) =
            self.channel_manager_data.super_safe_lock(|data| {
                (
                    data.allocate_tokens.clone(),
                    data.template_store.remove(&msg.template_id),
                    data.request_id_factory.next(),
                    data.last_new_prev_hash.clone(),
                )
            });

        _ = self.allocate_tokens(1).await;
        let Some(token) = token else {
            error!("Token not found, template id: {}", msg.template_id);
            return Err(JDCError::TokenNotFound);
        };

        let Some(template_message) = template_message else {
            error!("Template not found, template id: {}", msg.template_id);
            return Err(JDCError::TemplateNotFound(msg.template_id));
        };

        let mining_token = token.mining_job_token.clone();
        let mut deserialized_outputs: Vec<TxOut> =
            deserialize_coinbase_outputs(&token.coinbase_outputs.to_vec());
        deserialized_outputs[0].value =
            Amount::from_sat(template_message.coinbase_tx_value_remaining);
        let reserialized_outputs = consensus::serialize(&deserialized_outputs);

        let tx_list: Vec<Transaction> = transactions_data
            .to_vec()
            .iter()
            .map(|raw_tx| consensus::deserialize(raw_tx).expect("invalid tx"))
            .collect();

        let txids_as_u256: Vec<U256<'static>> = tx_list
            .iter()
            .map(|tx| {
                let txid = tx.compute_txid();
                let byte_array: [u8; 32] = *txid.as_byte_array();
                U256::Owned(byte_array.to_vec())
            })
            .collect();

        let tx_ids = Seq064K::new(txids_as_u256).map_err(JDCError::BinarySv2)?;
        let is_activated_future_template = template_message.future_template
            && prevhash
                .map(|prev_hash| prev_hash.template_id != template_message.template_id)
                .unwrap_or(true);

        let declare_job = self.channel_manager_data.super_safe_lock(|data| {
            let job_factory = data.job_factory.as_mut()?;
            let mut output = deserialize_coinbase_outputs(&data.coinbase_outputs);
            output[0].value = Amount::from_sat(template_message.coinbase_tx_value_remaining);

            if let Ok((coinbase_tx_prefix, coinbase_tx_suffix)) =
                job_factory.new_coinbase_tx_prefix_and_suffix(template_message.clone(), output)
            {
                let version = template_message.version;

                let declare_job = DeclareMiningJob {
                    request_id,
                    mining_job_token: mining_token.to_vec().try_into().unwrap(),
                    version,
                    coinbase_tx_prefix: coinbase_tx_prefix.try_into().unwrap(),
                    coinbase_tx_suffix: coinbase_tx_suffix.try_into().unwrap(),
                    tx_ids_list: tx_ids,
                    excess_data: excess_data.to_vec().try_into().unwrap(),
                };

                let last_declare = DeclaredJob {
                    declare_mining_job: Some(declare_job.clone()),
                    template: template_message,
                    prev_hash: data.last_new_prev_hash.clone(),
                    set_custom_mining_job: None,
                    coinbase_output: reserialized_outputs,
                    tx_list: transactions_data.to_vec(),
                };

                data.last_declare_job_store.insert(request_id, last_declare);

                return Some(declare_job);
            }
            None
        });

        if is_activated_future_template {
            return Ok(());
        }

        if let Some(declare_job) = declare_job {
            let frame: StdFrame =
                AnyMessage::JobDeclaration(JobDeclaration::DeclareMiningJob(declare_job))
                    .try_into()?;

            _ = self.channel_manager_channel.jd_sender.send(frame).await;
        }

        Ok(())
    }

    // Handles a `SetNewPrevHash` message:
    //
    // - Check `declare_job_cache` to see if the `prevhash` activates a future template.
    // - In FullTemplate mode → send a `DeclareMiningJob`.
    // - In CoinbaseOnly mode → send a `CustomMiningJob` for the activated future template.
    // - Update the upstream channel state.
    // - Update all downstream channels and propagate the new `prevhash` via `SetNewPrevHash`.
    async fn handle_set_new_prev_hash(
        &mut self,
        msg: SetNewPrevHash<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        let (future_template, declare_job) = self.channel_manager_data.super_safe_lock(|data| {
            if let Some(upstream_channel) = data.upstream_channel.as_mut() {
                if let Err(e) = upstream_channel.on_chain_tip_update(msg.clone().into()) {
                    error!(
                        "Couldn't update chaintip of the upstream channel: {msg}, error: {e:#?}"
                    );
                }
            }

            let declare_job = data
                .last_declare_job_store
                .values()
                .find(|declared_job| {
                    Some(declared_job.template.template_id)
                        == data.last_future_template.as_ref().map(|t| t.template_id)
                })
                .map(|declared_job| declared_job.declare_mining_job.clone());

            (data.last_future_template.clone(), declare_job)
        });

        if get_jd_mode() == JdMode::FullTemplate {
            if let Some(Some(job)) = declare_job {
                let frame: StdFrame =
                    AnyMessage::JobDeclaration(JobDeclaration::DeclareMiningJob(job)).try_into()?;

                self.channel_manager_channel
                    .jd_sender
                    .send(frame)
                    .await
                    .map_err(|_e| JDCError::ChannelErrorSender)?;
            }
        }

        let messages = self.channel_manager_data.super_safe_lock(|data| {
            data.last_new_prev_hash = Some(msg.clone().into_static());
            data.last_declare_job_store.iter_mut().for_each(|(_k, v)| {
                if v.template.future_template && v.template.template_id == msg.template_id {
                    v.prev_hash = Some(msg.clone().into_static());
                    v.template.future_template = false;
                }
            });

            let mut messages: Vec<RouteMessageTo> = vec![];

            if let Some(ref mut upstream_channel) = data.upstream_channel {
                _ = upstream_channel.on_chain_tip_update(msg.clone().into());

                if get_jd_mode() == JdMode::CoinbaseOnly {
                    if let (Some(job_factory), Some(token), Some(template)) = (
                        data.job_factory.as_mut(),
                        data.allocate_tokens.clone(),
                        future_template.clone(),
                    ) {
                        let request_id = data.request_id_factory.next();
                        let chain_tip = ChainTip::new(
                            msg.prev_hash.clone().into_static(),
                            msg.n_bits,
                            msg.header_timestamp,
                        );
                        let outputs = deserialize_coinbase_outputs(&data.coinbase_outputs);

                        if let Ok(custom_job) = job_factory.new_custom_job(
                            upstream_channel.get_channel_id(),
                            request_id,
                            token.clone().mining_job_token,
                            chain_tip,
                            template.clone(),
                            outputs,
                        ) {
                            let last_declare = DeclaredJob {
                                declare_mining_job: None,
                                template: template.into_static(),
                                prev_hash: Some(msg.clone().into_static()),
                                set_custom_mining_job: Some(custom_job.clone().into_static()),
                                coinbase_output: data.coinbase_outputs.clone(),
                                tx_list: vec![],
                            };

                            data.last_declare_job_store.insert(request_id, last_declare);
                            messages.push(Mining::SetCustomMiningJob(custom_job).into());
                        }
                    }
                }
            }

            for (downstream_id, downstream) in data.downstream.iter_mut() {
                let downstream_messages = downstream.downstream_data.super_safe_lock(|data| {
                    let mut messages: Vec<RouteMessageTo> = vec![];
                    if let Some(ref mut group_channel) = data.group_channels {
                        _ = group_channel.on_set_new_prev_hash(msg.clone().into_static());
                        let group_channel_id = group_channel.get_group_channel_id();
                        let activated_group_job_id = group_channel
                            .get_active_job()
                            .expect("active job must exist")
                            .get_job_id();

                        let set_new_prev_hash_message = SetNewPrevHashMp {
                            channel_id: group_channel_id,
                            job_id: activated_group_job_id,
                            prev_hash: msg.prev_hash.clone(),
                            min_ntime: msg.header_timestamp,
                            nbits: msg.n_bits,
                        };
                        messages.push(
                            (
                                *downstream_id,
                                Mining::SetNewPrevHash(set_new_prev_hash_message),
                            )
                                .into(),
                        );
                    }

                    for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                        if let Err(_e) =
                            standard_channel.on_set_new_prev_hash(msg.clone().into_static())
                        {
                            continue;
                        };

                        // did SetupConnection have the REQUIRES_STANDARD_JOBS flag set?
                        // if yes, there's no group channel, so we need to send the SetNewPrevHashMp
                        // to each standard channel
                        if data.group_channels.is_none() {
                            let activated_standard_job_id = standard_channel
                                .get_active_job()
                                .expect("active job must exist")
                                .get_job_id();
                            let set_new_prev_hash_message = SetNewPrevHashMp {
                                channel_id: *channel_id,
                                job_id: activated_standard_job_id,
                                prev_hash: msg.prev_hash.clone(),
                                min_ntime: msg.header_timestamp,
                                nbits: msg.n_bits,
                            };
                            messages.push(
                                (
                                    *downstream_id,
                                    Mining::SetNewPrevHash(set_new_prev_hash_message),
                                )
                                    .into(),
                            );
                        }
                    }

                    for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                        if let Err(_e) =
                            extended_channel.on_set_new_prev_hash(msg.clone().into_static())
                        {
                            continue;
                        };

                        let activated_extended_job_id = extended_channel
                            .get_active_job()
                            .expect("active job must exist")
                            .get_job_id();
                        let set_new_prev_hash_message = SetNewPrevHashMp {
                            channel_id: *channel_id,
                            job_id: activated_extended_job_id,
                            prev_hash: msg.prev_hash.clone(),
                            min_ntime: msg.header_timestamp,
                            nbits: msg.n_bits,
                        };
                        messages.push(
                            (
                                *downstream_id,
                                Mining::SetNewPrevHash(set_new_prev_hash_message),
                            )
                                .into(),
                        );
                    }

                    messages
                });

                messages.extend(downstream_messages);
            }

            messages
        });

        if get_jd_mode() == JdMode::CoinbaseOnly {
            _ = self.allocate_tokens(1).await;
        }

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }
}
