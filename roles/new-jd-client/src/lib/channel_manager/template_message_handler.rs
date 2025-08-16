use stratum_common::roles_logic_sv2::{
    bitcoin::{consensus, hashes::Hash, Amount, Transaction, TxOut}, codec_sv2::binary_sv2::{Seq064K, U256}, handlers_sv2::{HandleTemplateDistributionMessagesFromServerAsync, HandlerError as Error}, job_declaration_sv2::DeclareMiningJob, mining_sv2::{NewExtendedMiningJob, NewMiningJob}, parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution}, template_distribution_sv2::*
};
use tracing::info;

use crate::{
    channel_manager::{ChannelManager, LastDeclareJob},
    utils::StdFrame,
};

impl HandleTemplateDistributionMessagesFromServerAsync for ChannelManager {
    async fn handle_new_template(&mut self, msg: NewTemplate<'_>) -> Result<(), Error> {
        info!("Received handle_new_template from Template provider");

        self.channel_manager_data.super_safe_lock(|data| {
            data.template_store
                .insert(msg.template_id, msg.clone().into_static());
        });
        if msg.future_template {
            self.channel_manager_data.super_safe_lock(|data| {
                data.last_future_template = Some(msg.clone().into_static());
            });
        }

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


        // Rethink handling of standard and group channels, something doesn't feel right
        let messages = self.channel_manager_data.super_safe_lock(|data| {
            let mut messages: Vec<(u32, AnyMessage)> = Vec::new();
            let pool_coinbase_output = TxOut {
                value: Amount::from_sat(msg.coinbase_tx_value_remaining),
                script_pubkey: self.coinbase_reward_script.script_pubkey()
            };
            match msg.future_template {
                true => {
                    for (channel_id, standard_channel) in data.standard_channels.iter_mut() {

                        if data.group_channel.is_none() {
                            standard_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]).unwrap();
                            let standard_job_id = standard_channel.get_future_template_to_job_id().get(&msg.template_id).expect("Job_id must exist");
                            let standard_job = standard_channel.get_future_jobs().get(standard_job_id).expect("standard job must exist");
                            let standard_job_message = standard_job.get_job_message();
                            let downstream_id = data.channel_id_to_downstream_id.get(channel_id).unwrap();
                            messages.push((*downstream_id,AnyMessage::Mining(Mining::NewMiningJob(standard_job_message.clone().into_static()))));

                        }
                    }

                    if let Some(ref mut group_channel) = data.group_channel {
                        group_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]).unwrap();
                        let future_job_id = group_channel.get_future_template_to_job_id().get(&msg.template_id).expect("job_id must exist");
                        let future_job = group_channel.get_future_jobs().get(future_job_id).expect("future job must exist");

                        for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                            standard_channel.on_group_channel_job(future_job.clone()).unwrap();
                        }

                        let future_job_message = future_job.get_job_message().clone();
                        messages.push((0, AnyMessage::Mining(Mining::NewExtendedMiningJob(future_job_message))));
                        
                    }

                    for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                        extended_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]).unwrap();
                        let future_job_id = extended_channel.get_future_template_to_job_id().get(&msg.template_id).expect("job_id must exist");
                        let future_job = extended_channel.get_future_jobs().get(future_job_id).expect("future job must exist");
                        let future_job_message = future_job.get_job_message().clone();
                        let downstream_id = data.channel_id_to_downstream_id.get(channel_id).unwrap();
                        messages.push((*downstream_id, AnyMessage::Mining(Mining::NewExtendedMiningJob(future_job_message))));
                    }
                }
                false => {
                    for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                        if data.group_channel.is_none() {
                            standard_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]).unwrap();
                            let standard_job_id = standard_channel.get_future_template_to_job_id().get(&msg.template_id).expect("Job_id must exist");
                            let standard_job = standard_channel.get_future_jobs().get(standard_job_id).expect("standard job must exist");
                            let standard_job_message = standard_job.get_job_message();
                            let downstream_id = data.channel_id_to_downstream_id.get(channel_id).unwrap();
                            messages.push((*downstream_id, AnyMessage::Mining(Mining::NewMiningJob(standard_job_message.clone().into_static()))));

                        }
                    }

                    if let Some(ref mut group_channel) = data.group_channel {
                        group_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]).unwrap();
                        let active_job = group_channel.get_active_job().expect("active job must exist");
                        for (channel_id, standard_channel) in data.standard_channels.iter_mut() {
                            standard_channel.on_group_channel_job(active_job.clone()).unwrap();
                        }

                        let active_job_message = active_job.get_job_message().clone();
                        messages.push((0, AnyMessage::Mining(Mining::NewExtendedMiningJob(active_job_message))));
                        
                    }

                    for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                        extended_channel.on_new_template(msg.clone().into_static(), vec![pool_coinbase_output.clone()]).unwrap();
                        let active_job = extended_channel.get_active_job().expect("future job must exist");
                        let active_job_message = active_job.get_job_message().clone();
                        let downstream_id = data.channel_id_to_downstream_id.get(channel_id).unwrap();
                        messages.push((*downstream_id, AnyMessage::Mining(Mining::NewExtendedMiningJob(active_job_message))));
                    }
                }
            }
            messages
        });

        for (downstream_id, message) in messages {
            self.channel_manager_channel.downstream_sender.send((downstream_id, message));
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

        let (token, template_message, request_id) =
            self.channel_manager_data.super_safe_lock(|data| {
                (
                    data.allocate_tokens.take(),
                    data.template_store.get(&msg.template_id).cloned(),
                    data.request_id_factory.next(),
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
            declare_job: declare_job.clone(),
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
        self.channel_manager_data.super_safe_lock(|data| {
            data.last_new_prev_hash = Some(msg.clone().into_static());
            data.last_declare_job_store.iter_mut().for_each(|(k, v)| {
                if v.template.future_template && v.template.template_id == msg.template_id {
                    v.prev_hash = Some(msg.clone().into_static());
                    v.template.future_template = false;
                }
            });
        });
        // active the already present future job, and then send the jobs downstream and custom job
        // to upstream.
        Ok(())
    }
}
