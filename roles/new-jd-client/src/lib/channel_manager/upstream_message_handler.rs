use stratum_common::roles_logic_sv2::{
    channels_sv2::server::{extended::ExtendedChannel, jobs::job_store::DefaultJobStore},
    handlers_sv2::{
        HandleMiningMessagesFromServerAsync, HandlerError as Error, SupportedChannelTypes,
    },
    mining_sv2::*,
    parsers_sv2::{AnyMessage, IsSv2Message, Mining},
};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    channel_manager::ChannelManager,
    error::JDCError,
    utils::{deserialize_coinbase_output, UpstreamState},
};

impl HandleMiningMessagesFromServerAsync for ChannelManager {
    fn get_channel_type_for_server(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }
    fn is_work_selection_enabled_for_server(&self) -> bool {
        true
    }
    async fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: OpenStandardMiningChannelSuccess<'_>,
    ) -> Result<(), Error> {
        // Can't have standard channel with the pool under JD. fallback, if you
        // ever receive this from pool.
        info!("Received handle_open_standard_mining_channel_success from Pool");
        Ok(())
    }

    #[instrument(skip_all, fields(request_id = msg.request_id, channel_id = msg.channel_id))]
    async fn handle_open_extended_mining_channel_success(
        &mut self,
        msg: OpenExtendedMiningChannelSuccess<'_>,
    ) -> Result<(), Error> {
        info!("Handling OpenExtendedMiningChannelSuccess");

        self.channel_manager_data.super_safe_lock(|data| {
            let downstream_instance = data.pending_downstream_requests[0].clone();

            let (hashrate, min_extranonce_size) = data
                .pending_downstream_requests
                .get(0)
                .map(|req| match req {
                    Mining::OpenExtendedMiningChannel(m) => {
                        (m.nominal_hash_rate, m.min_extranonce_size)
                    }
                    Mining::OpenStandardMiningChannel(m) => {
                        (m.nominal_hash_rate, self.min_extranonce_size)
                    }
                    _ => (100_000.0, self.min_extranonce_size),
                })
                .unwrap_or((100_000.0, self.min_extranonce_size));

            let prefix_len = msg.extranonce_prefix.len();
            let jdc_extranonce_len = std::cmp::min(
                (msg.extranonce_size as usize).saturating_sub(min_extranonce_size as usize),
                self.min_extranonce_size as usize,
            );
            let total_len = prefix_len + msg.extranonce_size as usize;
            let range_0 = 0..prefix_len;
            let range_1 = prefix_len..prefix_len + jdc_extranonce_len;
            let range_2 = prefix_len + jdc_extranonce_len..total_len;

            debug!(
                prefix_len,
                extranonce_size = msg.extranonce_size,
                jdc_extranonce_len,
                total_len,
                "Calculated extranonce ranges"
            );

            let extranonces = match ExtendedExtranonce::from_upstream_extranonce(
                msg.extranonce_prefix.clone().into(),
                range_0,
                range_1,
                range_2,
            ) {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to build extranonce factory: {e:?}");
                    return;
                }
            };

            let job_store = Box::new(DefaultJobStore::new());
            let mut extended_channel = match ExtendedChannel::new_for_job_declaration_client(
                msg.channel_id,
                self.user_identity.clone(),
                msg.extranonce_prefix.to_vec(),
                msg.target.into(),
                hashrate,
                true,
                min_extranonce_size as u16,
                self.share_batch_size,
                self.shares_per_minute,
                job_store,
                data.pool_tag_string.clone(),
                self.miner_tag_string.clone(),
            ) {
                Ok(ch) => ch,
                Err(e) => {
                    warn!("Failed to create ExtendedChannel: {e:?}");
                    return;
                }
            };

            if let Some(ref mut last_template) = data.last_future_template {
                extended_channel.on_new_template(
                    last_template.clone(),
                    deserialize_coinbase_output(&data.coinbase_outputs),
                );
                debug!("Applied last_future_template to new extended channel");
            }

            if let Some(ref mut prevhash) = data.last_new_prev_hash {
                extended_channel.on_set_new_prev_hash(prevhash.clone());
                debug!("Applied last_new_prev_hash to new extended channel");
            }

            data.extranonce_prefix_factory_extended = extranonces.clone();
            data.extranonce_prefix_factory_standard = extranonces;
            data.upstream_channel = Some(extended_channel);
            self.upstream_state.set(UpstreamState::Connected);

            info!("Extended mining channel successfully initialized");
        });

        let pending_downstreams = self
            .channel_manager_data
            .super_safe_lock(|data| std::mem::take(&mut data.pending_downstream_requests));

        for pending_downstream in pending_downstreams {
            let message_type = pending_downstream.message_type();
            self.forward_downstream_channel_open_request(pending_downstream, message_type)
                .await
                .map_err(|e| Error::External(Box::new(e)))?;
        }

        Ok(())
    }

    #[instrument(skip_all, fields(request_id = msg.request_id))]
    async fn handle_open_mining_channel_error(
        &mut self,
        msg: OpenMiningChannelError<'_>,
    ) -> Result<(), Error> {
        warn!(?msg, "Received OpenMiningChannelError from Pool");
        Error::External(JDCError::Shutdown.into());
        Ok(())
    }

    #[instrument(skip_all, fields(channel_id = msg.channel_id))]
    async fn handle_update_channel_error(
        &mut self,
        msg: UpdateChannelError<'_>,
    ) -> Result<(), Error> {
        warn!(?msg, "Received UpdateChannelError from Pool");
        Ok(())
    }

    #[instrument(skip_all, fields(channel_id = msg.channel_id))]
    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Error> {
        warn!(
            ?msg,
            "Received CloseChannel from Pool — closing upstream channel"
        );
        self.channel_manager_data.super_safe_lock(|data| {
            data.upstream_channel = None;
        });
        Ok(())
    }

    #[instrument(skip_all, fields(channel_id = msg.channel_id))]
    async fn handle_set_extranonce_prefix(
        &mut self,
        msg: SetExtranoncePrefix<'_>,
    ) -> Result<(), Error> {
        info!(?msg, "Received SetExtranoncePrefix from Pool");
        Ok(())
    }

    #[instrument(skip_all, fields(channel_id = msg.channel_id))]
    async fn handle_submit_shares_success(
        &mut self,
        msg: SubmitSharesSuccess,
    ) -> Result<(), Error> {
        info!(?msg, "Received SubmitSharesSuccess from Pool");
        Ok(())
    }

    #[instrument(skip_all, fields(channel_id = msg.channel_id))]
    async fn handle_submit_shares_error(
        &mut self,
        msg: SubmitSharesError<'_>,
    ) -> Result<(), Error> {
        error!(?msg, "Received SubmitSharesError from Pool");
        Ok(())
    }

    #[instrument(skip_all, fields(channel_id = msg.channel_id, job_id = msg.job_id))]
    async fn handle_new_mining_job(&mut self, msg: NewMiningJob<'_>) -> Result<(), Error> {
        warn!(
            ?msg,
            "Ignoring NewMiningJob from upstream — proxy relies on JDC jobs"
        );
        Ok(())
    }

    #[instrument(skip_all, fields(channel_id = msg.channel_id, job_id = msg.job_id))]
    async fn handle_new_extended_mining_job(
        &mut self,
        msg: NewExtendedMiningJob<'_>,
    ) -> Result<(), Error> {
        info!(?msg, "Received NewExtendedMiningJob from Pool");
        Ok(())
    }

    #[instrument(skip_all, fields(channel_id = msg.channel_id, job_id = msg.job_id))]
    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash<'_>) -> Result<(), Error> {
        debug!(
            ?msg,
            "Ignoring SetNewPrevHash from upstream — proxy relies on JDC"
        );
        Ok(())
    }

    #[instrument(skip_all, fields(request_id = msg.request_id, job_id = msg.job_id))]
    async fn handle_set_custom_mining_job_success(
        &mut self,
        msg: SetCustomMiningJobSuccess,
    ) -> Result<(), Error> {
        info!("Received SetCustomMiningJobSuccess from Pool");
        self.channel_manager_data.super_safe_lock(|data| {
            if let Some(last_declare_job) = data.last_declare_job_store.remove(&msg.request_id) {
                data.template_id_to_upstream_job_id
                    .insert(last_declare_job.template.template_id, msg.job_id as u64);
                debug!(job_id = msg.job_id, "Mapped custom job into template store");
            } else {
                warn!(
                    request_id = msg.request_id,
                    "No matching declare job found for custom job success"
                );
            }
        });
        Ok(())
    }

    #[instrument(skip_all, fields(request_id = msg.request_id))]
    async fn handle_set_custom_mining_job_error(
        &mut self,
        msg: SetCustomMiningJobError<'_>,
    ) -> Result<(), Error> {
        warn!(?msg, "Received SetCustomMiningJobError from Pool");
        Error::External(JDCError::Shutdown.into());
        Ok(())
    }

    #[instrument(skip_all, fields(channel_id = msg.channel_id))]
    async fn handle_set_target(&mut self, msg: SetTarget<'_>) -> Result<(), Error> {
        info!(?msg, "Received SetTarget from Pool");
        let mut messages: Vec<(u32, AnyMessage)> = vec![];
        self.channel_manager_data.super_safe_lock(|data| {
            if let Some(ref mut upstream) = data.upstream_channel {
                upstream.set_target(msg.maximum_target.clone().into());
                // We need to update downstream channel with corrected downstream vardiff found
                // targets only when we send update channel message to upstream and
                // get the corresponding SetTarget message and update it
                // accordingly. for (downstream_id, downstream) in
                // data.downstream.iter_mut() {     downstream.downstream_data.
                // super_safe_lock(|data| {         for (channel_id,
                // standard_channel) in data.standard_channels.iter_mut() {
                //             let mut target_msg = msg.clone();
                //             target_msg.channel_id = *channel_id;
                //             messages.push((
                //                 *downstream_id,
                //                 AnyMessage::Mining(Mining::SetTarget(target_msg.into_static())),
                //             ));
                //             standard_channel.set_target(msg.maximum_target.clone().into());
                //         }

                //         for (channel_id, extended_channel) in data.extended_channels.iter_mut() {
                //             let mut target_msg = msg.clone();
                //             target_msg.channel_id = *channel_id;
                //             messages.push((
                //                 *downstream_id,
                //                 AnyMessage::Mining(Mining::SetTarget(target_msg.into_static())),
                //             ));
                //             extended_channel.set_target(msg.maximum_target.clone().into());
                //         }
                //     });
                // }
            }
        });

        for (downstream_id, target) in messages {
            self.channel_manager_channel
                .downstream_sender
                .send((downstream_id, target));
        }
        Ok(())
    }

    #[instrument(skip_all)]
    async fn handle_set_group_channel(&mut self, msg: SetGroupChannel<'_>) -> Result<(), Error> {
        warn!(?msg, "Received SetGroupChannel from Pool (unsupported)");
        Ok(())
    }
}
