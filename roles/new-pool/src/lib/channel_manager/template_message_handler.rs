use std::sync::atomic::Ordering;

use stratum_common::roles_logic_sv2::{
    bitcoin::{Amount, Transaction, TxOut, consensus, hashes::Hash},
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
    channel_manager::{ChannelManager, RouteMessageTo},
    error::PoolError,
    utils::{StdFrame, deserialize_coinbase_outputs},
};

impl HandleTemplateDistributionMessagesFromServerAsync for ChannelManager {
    type Error = PoolError;

    async fn handle_new_template(&mut self, msg: NewTemplate<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            channel_manager_data.last_future_template = Some(msg.clone().into_static());

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

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    async fn handle_request_tx_data_error(
        &mut self,
        msg: RequestTransactionDataError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        Ok(())
    }

    async fn handle_request_tx_data_success(
        &mut self,
        msg: RequestTransactionDataSuccess<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_set_new_prev_hash(
        &mut self,
        msg: SetNewPrevHash<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        let messages = self.channel_manager_data.super_safe_lock(|data| {
            data.last_new_prev_hash = Some(msg.clone().into_static());

            let mut messages: Vec<RouteMessageTo> = vec![];

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
                        if let Err(e) = standard_channel.on_set_new_prev_hash(msg.clone().into_static()) {
                            tracing::error!("Error while adding new prev hash to standard channel: {channel_id:?} {e:?}");
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
                        if let Err(e) = extended_channel.on_set_new_prev_hash(msg.clone().into_static()) {
                            tracing::error!("Error while adding new prev hash to extended channel: {channel_id:?} {e:?}");
                            continue;
                        };

                        // don't send any SetNewPrevHash messages to Extended Channels
                        // if the downstream requires custom work
                        if downstream.requires_custom_work.load(Ordering::SeqCst) {
                            continue;
                        }

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

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }
}
