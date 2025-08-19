use stratum_common::roles_logic_sv2::{
    bitcoin::{consensus, hashes::Hash, p2p::message, Amount, Transaction, TxOut},
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
    utils::StdFrame,
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

        if get_jd_mode() == JdMode::CoinbaseOnly && !msg.future_template {
            let (channel_id, prevhash, token, request_id, coinbase_tx_outputs) =
                self.channel_manager_data.super_safe_lock(|data| {
                    (
                        data.upstream_channel_id,
                        data.last_new_prev_hash.clone(),
                        data.allocate_tokens.take(),
                        data.request_id_factory.next(),
                        data.coinbase_outputs.clone(),
                    )
                });
            let Some(new_prev_hash) = prevhash else {
                return Ok(());
            };
            let Some(mining_job_token) = token else {
                return Ok(());
            };

            let last_declare = LastDeclareJob {
                channel_id,
                mining_job_token: mining_job_token.clone(),
                declare_job: None,
                template: msg.clone().into_static(),
                prev_hash: Some(new_prev_hash.clone()),
                coinbase_pool_output: coinbase_tx_outputs.clone(),
                tx_list: vec![],
            };

            let to_send = SetCustomMiningJob {
                channel_id,
                request_id,
                token: mining_job_token.mining_job_token.into(),
                version: msg.version,
                prev_hash: new_prev_hash.prev_hash,
                min_ntime: new_prev_hash.header_timestamp,
                nbits: new_prev_hash.n_bits,
                coinbase_tx_version: msg.coinbase_tx_version,
                coinbase_prefix: msg.clone().coinbase_prefix,
                coinbase_tx_input_n_sequence: msg.coinbase_tx_input_sequence,
                coinbase_tx_outputs: coinbase_tx_outputs.try_into().unwrap(),
                coinbase_tx_locktime: msg.coinbase_tx_locktime,
                merkle_path: msg.merkle_path.clone(),
            };

            self.allocate_tokens(1).await;

            let message = AnyMessage::Mining(Mining::SetCustomMiningJob(to_send.into_static()));
            let frame: StdFrame = message.try_into().unwrap();

            self.channel_manager_channel
                .upstream_sender
                .send(frame.into())
                .await;

            self.channel_manager_data.super_safe_lock(|data| {
                data.last_declare_job_store.insert(request_id, last_declare);
            });
        }

        // Rethink handling of standard and group channels, something doesn't feel right
        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let mut messages: Vec<(u32, AnyMessage)> = Vec::new();
            let pool_coinbase_output = TxOut {
                value: Amount::from_sat(msg.coinbase_tx_value_remaining),
                script_pubkey: self.coinbase_reward_script.script_pubkey(),
            };
            for (downstream_id, downstream) in channel_manager_data.downstream.iter_mut() {

                let downstream_messages = downstream.downstream_data.super_safe_lock(|data| {

                    let mut messages: Vec<(u32, AnyMessage)> = vec![];

                    let group_channel_job = if let Some(ref mut group_channel) = data.group_channels {
                        if let Ok(_) = group_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]) {
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


                    match msg.future_template {
                        true => {
                            for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                                if data.group_channels.is_none() {
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]) {
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
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]) {
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
                                if let Err(e) = extended_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]) {
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
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]) {
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
                                    if let Err(e) = standard_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]) {
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
                                if let Err(e) = extended_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]) {
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

        for (downstream_id, message) in messages {
            self.channel_manager_channel
                .downstream_sender
                .send((downstream_id, message));
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

        let (token, template_message, request_id, upstream_channel_id) =
            self.channel_manager_data.super_safe_lock(|data| {
                (
                    data.allocate_tokens.take(),
                    data.template_store.get(&msg.template_id).cloned(),
                    data.request_id_factory.next(),
                    data.upstream_channel_id,
                )
            });

        self.allocate_tokens(1).await;

        let token = token.unwrap();
        let template_message = template_message.unwrap();

        let mining_token = token.mining_job_token.to_vec();
        let pool_coinbase_outputs = token.coinbase_outputs.to_vec();

        let mut deserialized_outputs: Vec<TxOut> =
            consensus::deserialize(&pool_coinbase_outputs).unwrap();

        deserialized_outputs[0].value =
            Amount::from_sat(template_message.coinbase_tx_value_remaining);

        let reserialized_outputs = consensus::serialize(&deserialized_outputs);

        let mut tx_list: Vec<Transaction> = Vec::new();
        let mut txids_as_u256: Vec<U256<'static>> = Vec::new();
        for tx in transactions_data.to_vec() {
            let tx: Transaction = consensus::deserialize(&tx).unwrap();
            let txid = tx.compute_txid();
            let byte_array: [u8; 32] = *txid.as_byte_array();
            let owned_vec: Vec<u8> = byte_array.into();
            let txid_as_u256 = U256::Owned(owned_vec);
            txids_as_u256.push(txid_as_u256);
            tx_list.push(tx);
        }
        let tx_ids = Seq064K::new(txids_as_u256).expect("Failed to create Seq064K");
        // check flag, send SetCustomMining directly if the flag is unset
        // https://stratumprotocol.org/specification/06-Job-Declaration-Protocol/#641-setupconnection-flags-for-job-declaration-protocol
        let declare_job = DeclareMiningJob {
            request_id,
            mining_job_token: mining_token.try_into().unwrap(),
            version: template_message.version,
            // fix these
            coinbase_prefix: vec![].try_into().unwrap(),
            coinbase_suffix: vec![].try_into().unwrap(),
            tx_ids_list: tx_ids,
            excess_data: excess_data.to_vec().try_into().unwrap(),
        };

        let prev_hash = self
            .channel_manager_data
            .super_safe_lock(|data| data.last_new_prev_hash.clone())
            .filter(|_| !template_message.future_template);

        let last_declare = LastDeclareJob {
            channel_id: upstream_channel_id,
            mining_job_token: token,
            declare_job: Some(declare_job.clone()),
            template: template_message,
            prev_hash,
            coinbase_pool_output: reserialized_outputs,
            tx_list: transactions_data.to_vec(),
        };

        let frame: StdFrame =
            AnyMessage::JobDeclaration(JobDeclaration::DeclareMiningJob(declare_job))
                .try_into()
                .unwrap();

        self.channel_manager_data.super_safe_lock(|data| {
            data.last_declare_job_store.insert(request_id, last_declare);
        });
        self.channel_manager_channel
            .jd_sender
            .send(frame.into())
            .await;

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
                            data.upstream_channel_id,
                            data.allocate_tokens.take(),
                            data.request_id_factory.next(),
                            data.coinbase_outputs.clone(),
                        )
                    });
                let new_prev_hash = msg.clone().into_static();
                let mining_job_token = token.unwrap();

                let last_declare = LastDeclareJob {
                    channel_id,
                    mining_job_token: mining_job_token.clone(),
                    declare_job: None,
                    template: future_template.clone().into_static(),
                    prev_hash: Some(new_prev_hash.clone()),
                    coinbase_pool_output: coinbase_tx_outputs.clone(),
                    tx_list: vec![],
                };

                let to_send = SetCustomMiningJob {
                    channel_id,
                    request_id,
                    token: mining_job_token.mining_job_token.into(),
                    version: future_template.version,
                    prev_hash: new_prev_hash.prev_hash,
                    min_ntime: new_prev_hash.header_timestamp,
                    nbits: new_prev_hash.n_bits,
                    coinbase_tx_version: future_template.coinbase_tx_version,
                    coinbase_prefix: future_template.clone().coinbase_prefix,
                    coinbase_tx_input_n_sequence: future_template.coinbase_tx_input_sequence,
                    coinbase_tx_outputs: coinbase_tx_outputs.try_into().unwrap(),
                    coinbase_tx_locktime: future_template.coinbase_tx_locktime,
                    merkle_path: future_template.merkle_path.clone(),
                };

                self.allocate_tokens(1).await;

                let message = AnyMessage::Mining(Mining::SetCustomMiningJob(to_send.into_static()));
                let frame: StdFrame = message.try_into().unwrap();

                self.channel_manager_channel
                    .upstream_sender
                    .send(frame.into())
                    .await;

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

        for (downstream_id, message) in messages {
            self.channel_manager_channel
                .downstream_sender
                .send((downstream_id, message));
        }

        Ok(())
    }
}
