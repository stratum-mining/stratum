use stratum_common::roles_logic_sv2::{
    bitcoin::{consensus, hashes::Hash, p2p::message, Amount, Transaction, TxOut},
    channels_sv2::chain_tip::ChainTip,
    codec_sv2::binary_sv2::{Seq064K, U256},
    handlers_sv2::{HandleTemplateDistributionMessagesFromServerAsync, HandlerError as Error},
    job_declaration_sv2::DeclareMiningJob,
    mining_sv2::{
        NewExtendedMiningJob, NewMiningJob, SetCustomMiningJob, SetNewPrevHash as SetNewPrevHashMp,
    },
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::*,
};
use tracing::info;

use crate::{
    channel_manager::{ChannelManager, LastDeclareJob},
    jd_mode::{get_jd_mode, JdMode},
    utils::{deserialize_coinbase_output, StdFrame},
};

impl HandleTemplateDistributionMessagesFromServerAsync for ChannelManager {
    async fn handle_new_template(&mut self, msg: NewTemplate<'_>) -> Result<(), Error> {
        info!("Received handle_new_template from Template provider");

        self.channel_manager_data.super_safe_lock(|data| {
            data.template_store
                .insert(msg.template_id, msg.clone().into_static());
            if msg.future_template {
                data.last_future_template = Some(msg.clone().into_static());
            }
        });

        if get_jd_mode() == JdMode::TemplateOnly && !msg.future_template {
            let tx_data_request = AnyMessage::TemplateDistribution(
                TemplateDistribution::RequestTransactionData(RequestTransactionData {
                    template_id: msg.template_id,
                }),
            );
            let frame: StdFrame = tx_data_request.try_into().unwrap();
            self.channel_manager_channel
                .tp_sender
                .send(frame.into())
                .await;
        }

        // Rethink handling of standard and group channels, something doesn't feel right
        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let mut messages: Vec<(u32, AnyMessage)> = Vec::new();
            let mut coinbase_output = deserialize_coinbase_output(&channel_manager_data.coinbase_outputs);
            coinbase_output[0].value = Amount::from_sat(msg.coinbase_tx_value_remaining);

            for (downstream_id, downstream) in channel_manager_data.downstream.iter_mut() {

                let downstream_messages = downstream.downstream_data.super_safe_lock(|data| {

                    let mut messages: Vec<(u32, AnyMessage)> = vec![];

                    let group_channel_job = if let Some(ref mut group_channel) = data.group_channels {
                        if let Ok(_) = group_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
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
                            // return a none in job
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(upstream) = channel_manager_data.upstream_channel.as_mut() {
                        upstream.on_new_template(msg.clone().into_static(), coinbase_output.clone());
                        if !msg.future_template && get_jd_mode() == JdMode::CoinbaseOnly {
                            if let Some(active_job) = upstream.get_active_job().cloned() {
                                if let (Some(token), Some(prevhash)) = (
                                    channel_manager_data.allocate_tokens.clone(),
                                    channel_manager_data.last_new_prev_hash.clone(),
                                ) {
                                    let request_id = channel_manager_data.request_id_factory.next();
                                    let chain_tip = ChainTip::new(
                                        prevhash.prev_hash.clone(),
                                        prevhash.n_bits,
                                        prevhash.header_timestamp,
                                    );
                                    if let Ok(custom_job) = active_job.into_custom_job(
                                        request_id,
                                        token.mining_job_token.clone(),
                                        chain_tip,
                                    ) {
                                        let last_declare = LastDeclareJob {
                                            mining_job_token: token,
                                            declare_job: None,
                                            template: msg.clone().into_static(),
                                            prev_hash: Some(prevhash),
                                            custom_job: None,
                                            coinbase_output: channel_manager_data.coinbase_outputs.clone(),
                                            tx_list: Vec::new(),
                                        };
                                        channel_manager_data
                                            .last_declare_job_store
                                            .insert(request_id, last_declare);
                                        messages.push((
                                            0,
                                            AnyMessage::Mining(Mining::SetCustomMiningJob(custom_job)),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    match msg.future_template {
                        true => {
                            for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                                if data.group_channels.is_none() {
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                        // do something here, I guess just ignore??
                                        tracing::error!("Error while adding template to standard channel: {channel_id:?}");
                                        continue;
                                    }
                                    let standard_job_id = standard_channel.get_future_template_to_job_id().get(&msg.template_id).expect("job_id must exist");
                                    let standard_job = standard_channel.get_future_jobs().get(standard_job_id).expect("standard job must exist");
                                    channel_manager_data.downstream_channel_id_and_job_id_to_template_id.insert((*channel_id, *standard_job_id), msg.template_id);
                                    channel_manager_data.template_id_to_downstream_channel_id_and_job_id.insert(msg.template_id, (*channel_id, *standard_job_id));
                                    let standard_job_message = standard_job.get_job_message();
                                    messages.push((*downstream_id, AnyMessage::Mining(Mining::NewMiningJob(standard_job_message.clone().into_static()))));
                                }
                                if let Some(ref group_channel_job) = group_channel_job {
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                        // do something here, I guess just ignore??
                                        tracing::error!("Error while adding template to standard channel: {channel_id:?}");
                                        continue;
                                    }
                                    standard_channel
                                    .on_group_channel_job(group_channel_job.clone());
                                }
                            }
                            if let Some(group_channel_job) = group_channel_job {
                                let job_message = group_channel_job.get_job_message();
                                messages.push((*downstream_id, AnyMessage::Mining(Mining::NewExtendedMiningJob(job_message.clone().into_static()))));
                            }

                            for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                                if let Err(e) = extended_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                    // do something here, I guess just ignore??
                                    tracing::error!("Error while adding template to standard channel: {channel_id:?}");
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
                                channel_manager_data.template_id_to_downstream_channel_id_and_job_id.insert(msg.template_id, (*channel_id, *extended_job_id));
                                let extended_job_message = extended_job.get_job_message();

                                messages.push((*downstream_id, AnyMessage::Mining(Mining::NewExtendedMiningJob(extended_job_message.clone().into_static()))));
                            }
                        }
                        false => {
                            for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                                if data.group_channels.is_none() {
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                        // do something here, I guess just ignore??
                                        tracing::error!("Error while adding template to standard channel: {channel_id:?}");
                                        continue;
                                    }
                                    let standard_job = standard_channel.get_active_job().expect("standard job must exist");
                                    channel_manager_data.downstream_channel_id_and_job_id_to_template_id.insert((*channel_id, standard_job.get_job_id()), msg.template_id);
                                    channel_manager_data.template_id_to_downstream_channel_id_and_job_id.insert(msg.template_id, (*channel_id, standard_job.get_job_id()));
                                    let standard_job_message = standard_job.get_job_message();
                                    messages.push((*downstream_id, AnyMessage::Mining(Mining::NewMiningJob(standard_job_message.clone().into_static()))));
                                }
                                if let Some(ref group_channel_job) = group_channel_job {
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                        // do something here, I guess just ignore??
                                        tracing::error!("Error while adding template to standard channel: {channel_id:?}");
                                        continue;
                                    }
                                    standard_channel
                                    .on_group_channel_job(group_channel_job.clone());
                                }
                            }
                            if let Some(group_channel_job) = group_channel_job {
                                let job_message = group_channel_job.get_job_message();
                                messages.push((*downstream_id, AnyMessage::Mining(Mining::NewExtendedMiningJob(job_message.clone().into_static()))));
                            }

                            for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                                if let Err(e) = extended_channel.on_new_template(msg.clone().into_static(), coinbase_output.clone()) {
                                    // do something here, I guess just ignore??
                                    tracing::error!("Error while adding template to standard channel: {channel_id:?}");
                                    continue;
                                }
                                let extended_job = extended_channel
                                    .get_active_job()
                                    .expect("extended job must exist");

                                channel_manager_data.downstream_channel_id_and_job_id_to_template_id.insert((*channel_id, extended_job.get_job_id()), msg.template_id);
                                channel_manager_data.template_id_to_downstream_channel_id_and_job_id.insert(msg.template_id, (*channel_id, extended_job.get_job_id()));
                                let extended_job_message = extended_job.get_job_message();

                                messages.push((*downstream_id, AnyMessage::Mining(Mining::NewExtendedMiningJob(extended_job_message.clone().into_static()))));
                            }
                        }
                    }

                    messages

                });
                messages.extend(downstream_messages);
            }
            messages
        });

        if get_jd_mode() == JdMode::CoinbaseOnly && !msg.future_template {
            self.allocate_tokens(1).await;
        }

        for (downstream_id, message) in messages {
            if downstream_id == 0 {
                let frame: StdFrame = message.try_into().unwrap();
                self.channel_manager_channel
                    .upstream_sender
                    .send(frame.into())
                    .await;
            } else {
                self.channel_manager_channel
                    .downstream_sender
                    .send((downstream_id, message));
            }
        }

        Ok(())
    }

    async fn handle_request_tx_data_error(
        &mut self,
        msg: RequestTransactionDataError<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_request_tx_data_error from Template provider");
        Ok(())
    }

    async fn handle_request_tx_data_success(
        &mut self,
        msg: RequestTransactionDataSuccess<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_request_tx_data_success from Template provider");

        let transactions_data = msg.transaction_list;
        let excess_data = msg.excess_data;

        // Grab token, template, request id, channel id in one lock
        let (token, template_message, request_id, upstream_channel_id) =
            self.channel_manager_data.super_safe_lock(|data| {
                (
                    data.allocate_tokens.take(),
                    data.template_store.get(&msg.template_id).cloned(),
                    data.request_id_factory.next(),
                    data.upstream_channel
                        .as_ref()
                        .map(|uc| uc.get_channel_id())
                        .unwrap_or_default(),
                )
            });

        // Refresh allocate_tokens for next use
        self.allocate_tokens(1).await;

        let token = token.expect("missing allocate_tokens");
        let template_message = template_message.expect("missing template for template_id");

        // Rebuild coinbase outputs with updated value
        let mining_token = token.mining_job_token.clone();
        let mut deserialized_outputs: Vec<TxOut> =
            consensus::deserialize(&token.coinbase_outputs.to_vec())
                .expect("invalid coinbase outputs");

        deserialized_outputs[0].value =
            Amount::from_sat(template_message.coinbase_tx_value_remaining);
        let reserialized_outputs = consensus::serialize(&deserialized_outputs);

        // Deserialize transactions + collect txids
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

        let tx_ids = Seq064K::new(txids_as_u256).expect("Failed to create Seq064K");

        let declare_job = self.channel_manager_data.super_safe_lock(|data| {
            let upstream = data.upstream_channel.as_mut()?;
            let active_job = upstream.get_active_job().cloned()?;
            let prevhash = data.last_new_prev_hash.clone()?;

            let coinbase_prefix = active_job.get_coinbase_tx_prefix_with_bip141();
            let coinbase_suffix = active_job.get_coinbase_tx_suffix_with_bip141();
            let coinbase_outputs = active_job.get_coinbase_outputs().clone();
            let version = active_job.get_version();

            let declare_job = DeclareMiningJob {
                request_id,
                mining_job_token: mining_token.to_vec().try_into().unwrap(),
                version,
                coinbase_prefix: coinbase_prefix.try_into().unwrap(),
                coinbase_suffix: coinbase_suffix.try_into().unwrap(),
                tx_ids_list: tx_ids,
                excess_data: excess_data.to_vec().try_into().unwrap(),
            };

            let chain_tip = ChainTip::new(
                prevhash.prev_hash,
                prevhash.n_bits,
                prevhash.header_timestamp,
            );

            let custom_job = active_job
                .into_custom_job(request_id, token.mining_job_token.clone(), chain_tip)
                .ok()
                .map(|job| job.into_static())?;

            let last_declare = LastDeclareJob {
                mining_job_token: token,
                declare_job: Some(declare_job.clone()),
                template: template_message,
                prev_hash: data.last_new_prev_hash.clone(),
                custom_job: Some(custom_job),
                coinbase_output: reserialized_outputs,
                tx_list: transactions_data.to_vec(),
            };

            data.last_declare_job_store.insert(request_id, last_declare);

            Some(declare_job)
        });

        if let Some(declare_job) = declare_job {
            let frame: StdFrame =
                AnyMessage::JobDeclaration(JobDeclaration::DeclareMiningJob(declare_job))
                    .try_into()
                    .unwrap();

            self.channel_manager_channel
                .jd_sender
                .send(frame.into())
                .await;
        }

        Ok(())
    }

    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash<'_>) -> Result<(), Error> {
        info!("Received handle_set_new_prev_hash from Template provider");

        let future_template = self
            .channel_manager_data
            .super_safe_lock(|data| data.last_future_template.clone());

        if get_jd_mode() == JdMode::TemplateOnly {
            if let Some(ref future_template) = future_template {
                let tx_data_request = AnyMessage::TemplateDistribution(
                    TemplateDistribution::RequestTransactionData(RequestTransactionData {
                        template_id: future_template.template_id,
                    }),
                );
                let frame: StdFrame = tx_data_request.try_into().unwrap();
                self.channel_manager_channel
                    .tp_sender
                    .send(frame.into())
                    .await;
            }
        }

        if get_jd_mode() == JdMode::CoinbaseOnly {
            if let Some(future_template) = future_template {
                let (channel_id, token, request_id, coinbase_tx_outputs) =
                    self.channel_manager_data.super_safe_lock(|data| {
                        (
                            data.upstream_channel
                                .as_ref()
                                .map(|uc| uc.get_channel_id())
                                .unwrap_or_default(),
                            data.allocate_tokens.take(),
                            data.request_id_factory.next(),
                            data.coinbase_outputs.clone(),
                        )
                    });
                let new_prev_hash = msg.clone().into_static();
                let mining_job_token = token.unwrap();

                let last_declare = LastDeclareJob {
                    mining_job_token: mining_job_token.clone(),
                    declare_job: None,
                    template: future_template.clone().into_static(),
                    prev_hash: Some(new_prev_hash.clone()),
                    custom_job: None,
                    coinbase_output: coinbase_tx_outputs.clone(),
                    tx_list: vec![],
                };

                self.channel_manager_data.super_safe_lock(|data| {
                    data.last_declare_job_store.insert(request_id, last_declare);
                });
            }
        }

        let messages = self.channel_manager_data.super_safe_lock(|data| {
            data.last_new_prev_hash = Some(msg.clone().into_static());
            data.last_declare_job_store.iter_mut().for_each(|(k, v)| {
                if v.template.future_template && v.template.template_id == msg.template_id {
                    v.prev_hash = Some(msg.clone().into_static());
                    v.template.future_template = false;
                }
            });

            let mut messages: Vec<(u32, AnyMessage)> = vec![];

            if let Some(ref mut upstream) = data.upstream_channel {
                upstream.on_set_new_prev_hash(msg.clone().into_static());

                if get_jd_mode() == JdMode::CoinbaseOnly {
                    let mut active_job = upstream.get_active_job().cloned();
                    if let Some(mut active_job) = active_job {
                        let request_id = data.request_id_factory.next();
                        let token = data.allocate_tokens.clone().unwrap().clone();
                        let chain_tip = ChainTip::new(
                            msg.prev_hash.clone().into_static(),
                            msg.n_bits,
                            msg.header_timestamp,
                        );
                        let custom_job = active_job
                            .into_custom_job(request_id, token.mining_job_token, chain_tip)
                            .unwrap();
                        messages.push((
                            0,
                            AnyMessage::Mining(Mining::SetCustomMiningJob(custom_job)),
                        ));
                    }
                }
            }

            for (downstream_id, downstream) in data.downstream.iter_mut() {
                let downstrea_messages = downstream.downstream_data.super_safe_lock(|data| {
                    let mut messages: Vec<(u32, AnyMessage)> = vec![];
                    if let Some(ref mut group_channel) = data.group_channels {
                        group_channel.on_set_new_prev_hash(msg.clone().into_static());
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
                        messages.push((
                            *downstream_id,
                            AnyMessage::Mining(Mining::SetNewPrevHash(
                                set_new_prev_hash_message.into_static(),
                            )),
                        ));
                    }

                    for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                        if let Err(e) =
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
                            messages.push((
                                *downstream_id,
                                AnyMessage::Mining(Mining::SetNewPrevHash(
                                    set_new_prev_hash_message.into_static(),
                                )),
                            ));
                        }
                    }

                    for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                        if let Err(e) =
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
                        messages.push((
                            *downstream_id,
                            AnyMessage::Mining(Mining::SetNewPrevHash(
                                set_new_prev_hash_message.into_static(),
                            )),
                        ));
                    }

                    messages
                });

                messages.extend(downstrea_messages);
            }

            messages
        });

        if get_jd_mode() == JdMode::CoinbaseOnly {
            self.allocate_tokens(1).await;
        }

        for (downstream_id, message) in messages {
            if downstream_id == 0 {
                let frame: StdFrame = message.try_into().unwrap();
                self.channel_manager_channel
                    .upstream_sender
                    .send(frame.into())
                    .await;
            } else {
                self.channel_manager_channel
                    .downstream_sender
                    .send((downstream_id, message));
            }
        }

        Ok(())
    }
}
