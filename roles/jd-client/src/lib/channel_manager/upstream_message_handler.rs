use std::sync::atomic::Ordering;

use stratum_apps::stratum_core::{
    bitcoin::Target,
    channels_sv2::{
        client::extended::ExtendedChannel, outputs::deserialize_outputs,
        server::jobs::factory::JobFactory,
    },
    handlers_sv2::{HandleMiningMessagesFromServerAsync, SupportedChannelTypes},
    mining_sv2::*,
    parsers_sv2::{AnyMessage, Mining, TemplateDistribution},
    template_distribution_sv2::RequestTransactionData,
};
use tracing::{debug, error, info, warn};

use crate::{
    channel_manager::{
        downstream_message_handler::RouteMessageTo, ChannelManager, DeclaredJob, Persistence,
        JDC_SEARCH_SPACE_BYTES,
    },
    error::{ChannelSv2Error, JDCError},
    jd_mode::{get_jd_mode, JdMode},
    status::{State, Status},
    utils::{create_close_channel_msg, PendingChannelRequest, StdFrame, UpstreamState},
};

impl HandleMiningMessagesFromServerAsync for ChannelManager {
    type Error = JDCError;

    fn get_channel_type_for_server(&self, _server_id: Option<usize>) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }
    fn is_work_selection_enabled_for_server(&self, _server_id: Option<usize>) -> bool {
        true
    }

    // Handles an unexpected `OpenStandardMiningChannelSuccess` message from the upstream.
    //
    // The Job Declarator Client (JDC) only supports extended channel when
    // communicating with upstream peer. Receiving a standard channel success
    // indicates either misbehavior or a protocol violation by the upstream.
    //
    // In such cases, the event is treated as malicious, and a fallback
    // (`UpstreamShutdownFallback`) is immediately triggered to protect the system.
    async fn handle_open_standard_mining_channel_success(
        &mut self,
        _server_id: Option<usize>,
        msg: OpenStandardMiningChannelSuccess<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        info!(
            "⚠️ JDC can only open extended channels with the upstream server, preparing fallback."
        );
        _ = self
            .channel_manager_channel
            .status_sender
            .send(Status {
                state: State::UpstreamShutdownFallback(JDCError::Shutdown),
            })
            .await;
        Ok(())
    }

    // Handles `OpenExtendedMiningChannelSuccess` messages from upstream.
    //
    // On success, this establishes a client-side extended channel:
    // - If initialization fails at any step, the upstream state is reverted from `Pending` to
    //   `NoChannel`.
    // - If initialization succeeds, we configure the extranonce factory, create a new
    //   `ExtendedChannel` and `JobFactory`, and update the upstream state from `Pending` to
    //   `Connected`.
    //
    // Once the upstream state transitions to `Connected`, all pending downstream requests are
    // processed, and downstream channels are opened accordingly.
    async fn handle_open_extended_mining_channel_success(
        &mut self,
        _server_id: Option<usize>,
        msg: OpenExtendedMiningChannelSuccess<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        let coinbase_outputs = self
            .channel_manager_data
            .super_safe_lock(|data| data.coinbase_outputs.clone());

        let outputs = deserialize_outputs(coinbase_outputs)
            .map_err(|_| JDCError::DeclaredJobHasBadCoinbaseOutputs)?;

        let (channel_state, template, custom_job, close_channel) =
            self.channel_manager_data.super_safe_lock(|data| {
                let Some(pending_request) = data.pending_downstream_requests.front() else {
                    self.upstream_state.set(UpstreamState::NoChannel);
                    let close_channel =
                        create_close_channel_msg(msg.channel_id, "downstream not available");
                    return (self.upstream_state.get(), None, None, Some(close_channel));
                };

                let hashrate = match pending_request {
                    PendingChannelRequest::ExtendedChannel(m) => m.nominal_hash_rate,
                    PendingChannelRequest::StandardChannel(m) => m.nominal_hash_rate,
                };

                let prefix_len = msg.extranonce_prefix.len();

                let total_len = prefix_len + msg.extranonce_size as usize;
                let range_0 = 0..prefix_len;
                let range_1 = prefix_len..prefix_len + JDC_SEARCH_SPACE_BYTES;
                let range_2 = prefix_len + JDC_SEARCH_SPACE_BYTES..total_len;

                debug!(
                    prefix_len,
                    extranonce_size = msg.extranonce_size,
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
                        self.upstream_state.set(UpstreamState::NoChannel);
                        let close_channel =
                            create_close_channel_msg(msg.channel_id, "downstream not available");
                        return (self.upstream_state.get(), None, None, Some(close_channel));
                    }
                };

                let job_factory = JobFactory::new(
                    true,
                    data.pool_tag_string.clone(),
                    Some(self.miner_tag_string.clone()),
                );

                let mut extended_channel = ExtendedChannel::new(
                    msg.channel_id,
                    self.user_identity.clone(),
                    msg.extranonce_prefix.to_vec(),
                    Target::from_le_bytes(msg.target.inner_as_ref().try_into().unwrap()),
                    hashrate,
                    true,
                    msg.extranonce_size,
                    Persistence::default(),
                );

                if let Some(ref mut prevhash) = data.last_new_prev_hash {
                    _ = extended_channel.on_chain_tip_update(prevhash.clone().into());
                    debug!("Applied last_new_prev_hash to new extended channel");
                }

                let set_custom_job = if get_jd_mode() == JdMode::CoinbaseOnly {
                    if let (Some(job_factory), Some(token), Some(template), Some(prevhash)) = (
                        data.job_factory.as_mut(),
                        data.allocate_tokens.clone(),
                        data.last_future_template.clone(),
                        data.last_new_prev_hash.clone(),
                    ) {
                        let request_id = data.request_id_factory.fetch_add(1, Ordering::Relaxed);

                        let full_extranonce_size = extended_channel.get_full_extranonce_size();

                        if let Ok(custom_job) = job_factory.new_custom_job(
                            extended_channel.get_channel_id(),
                            request_id,
                            token.clone().mining_job_token,
                            prevhash.clone().into(),
                            template.clone(),
                            outputs,
                            full_extranonce_size,
                        ) {
                            let last_declare = DeclaredJob {
                                declare_mining_job: None,
                                template: template.into_static(),
                                prev_hash: Some(prevhash.into_static()),
                                set_custom_mining_job: Some(custom_job.clone().into_static()),
                                coinbase_output: data.coinbase_outputs.clone(),
                                tx_list: vec![],
                            };

                            data.last_declare_job_store.insert(request_id, last_declare);
                            Some(custom_job)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                data.extranonce_prefix_factory_extended = extranonces.clone();
                data.extranonce_prefix_factory_standard = extranonces;
                data.upstream_channel = Some(extended_channel);
                data.job_factory = Some(job_factory);
                self.upstream_state.set(UpstreamState::Connected);

                info!("Extended mining channel successfully initialized");
                (
                    self.upstream_state.get(),
                    data.last_future_template.clone(),
                    set_custom_job,
                    None,
                )
            });

        if channel_state == UpstreamState::Connected {
            if get_jd_mode() == JdMode::FullTemplate {
                if let Some(template) = template {
                    let tx_data_request = AnyMessage::TemplateDistribution(
                        TemplateDistribution::RequestTransactionData(RequestTransactionData {
                            template_id: template.template_id,
                        }),
                    );
                    let frame: StdFrame = tx_data_request.try_into()?;
                    self.channel_manager_channel
                        .tp_sender
                        .send(frame)
                        .await
                        .map_err(|_e| JDCError::ChannelErrorSender)?;
                }
            }

            if get_jd_mode() == JdMode::CoinbaseOnly {
                if let Some(custom_job) = custom_job {
                    let set_custom_job = AnyMessage::Mining(Mining::SetCustomMiningJob(custom_job));
                    let frame: StdFrame = set_custom_job.try_into()?;
                    self.channel_manager_channel
                        .jd_sender
                        .send(frame)
                        .await
                        .map_err(|_e| JDCError::ChannelErrorSender)?;
                    _ = self.allocate_tokens(1).await;
                }
            }

            let pending_downstreams = self
                .channel_manager_data
                .super_safe_lock(|data| std::mem::take(&mut data.pending_downstream_requests));

            for pending_downstream in pending_downstreams {
                let message_type = pending_downstream.message_type();
                self.send_open_channel_request_to_mining_handler(
                    pending_downstream.into(),
                    message_type,
                )
                .await?;
            }
        }

        // In case of failure, close the channel with upstream.
        if let Some(close_channel) = close_channel {
            let close_channel = AnyMessage::Mining(Mining::CloseChannel(close_channel));
            let frame: StdFrame = close_channel.try_into()?;
            self.channel_manager_channel
                .upstream_sender
                .send(frame)
                .await
                .map_err(|_e| JDCError::ChannelErrorSender)?;
            _ = self.allocate_tokens(1).await;
        }

        Ok(())
    }

    // Handles `OpenMiningChannelError` messages received from upstream.
    //
    // Receiving this message is treated as malicious behavior, since JDC only supports
    // extended channels. When encountered, we immediately trigger the fallback mechanism
    // by transitioning the upstream state into a shutdown-fallback mode.
    async fn handle_open_mining_channel_error(
        &mut self,
        _server_id: Option<usize>,
        msg: OpenMiningChannelError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        warn!("⚠️ Cannot open extended channel with the upstream server, preparing fallback.");

        _ = self
            .channel_manager_channel
            .status_sender
            .send(Status {
                state: State::UpstreamShutdownFallback(JDCError::Shutdown),
            })
            .await;
        Ok(())
    }

    // Handles `UpdateChannelError` messages from upstream.
    async fn handle_update_channel_error(
        &mut self,
        _server_id: Option<usize>,
        msg: UpdateChannelError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        Ok(())
    }

    // Handles `CloseChannel` messages from upstream.
    //
    // Upon receiving this message, the upstream channel is immediately closed and
    // the system transitions into the upstream shutdown fallback state.
    async fn handle_close_channel(
        &mut self,
        _server_id: Option<usize>,
        msg: CloseChannel<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        self.channel_manager_data.super_safe_lock(|data| {
            data.upstream_channel = None;
        });
        _ = self
            .channel_manager_channel
            .status_sender
            .send(Status {
                state: State::UpstreamShutdownFallback(JDCError::Shutdown),
            })
            .await;
        Ok(())
    }

    // Handles `SetExtranoncePrefix` messages from upstream.
    //
    // When received, this updates the current extranonce prefix and rebuilds both the
    // standard and extended extranonce factories. Each active downstream channel is then
    // assigned a new extranonce prefix, and a corresponding `SetExtranoncePrefix` message
    // is sent downstream to synchronize state.
    async fn handle_set_extranonce_prefix(
        &mut self,
        _server_id: Option<usize>,
        msg: SetExtranoncePrefix<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        let messages_results =
            self.channel_manager_data
                .super_safe_lock(|channel_manager_data| {
                    let mut messages_results: Vec<Result<RouteMessageTo, Self::Error>> = vec![];
                    if let Some(upstream_channel) = channel_manager_data.upstream_channel.as_mut() {
                        if let Err(e) =
                            upstream_channel.set_extranonce_prefix(msg.extranonce_prefix.to_vec())
                        {
                            return Err(JDCError::ChannelSv2(
                                ChannelSv2Error::ExtendedChannelClientSide(e),
                            ));
                        }

                        let new_prefix_len = msg.extranonce_prefix.len();
                        let rollable_extranonce_size =
                            upstream_channel.get_rollable_extranonce_size();
                        let full_extranonce_size =
                            new_prefix_len + rollable_extranonce_size as usize;
                        if full_extranonce_size > MAX_EXTRANONCE_LEN {
                            return Err(JDCError::ExtranonceSizeTooLarge);
                        }

                        let range_0 = 0..new_prefix_len;
                        let range_1 = new_prefix_len..new_prefix_len + JDC_SEARCH_SPACE_BYTES;
                        let range_2 = new_prefix_len + JDC_SEARCH_SPACE_BYTES..full_extranonce_size;

                        debug!(
                            new_prefix_len,
                            rollable_extranonce_size,
                            full_extranonce_size,
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
                                return Err(JDCError::ExtranoncePrefixFactoryError(e));
                            }
                        };

                        channel_manager_data.extranonce_prefix_factory_extended =
                            extranonces.clone();
                        channel_manager_data.extranonce_prefix_factory_standard = extranonces;

                        for (downstream_id, downstream) in
                            channel_manager_data.downstream.iter_mut()
                        {
                            downstream.downstream_data.super_safe_lock(|data| {
                                for (channel_id, standard_channel) in
                                    data.standard_channels.iter_mut()
                                {
                                    match channel_manager_data
                                        .extranonce_prefix_factory_standard
                                        .next_prefix_standard()
                                    {
                                        Ok(prefix) => match standard_channel
                                            .set_extranonce_prefix(prefix.clone().to_vec())
                                        {
                                            Ok(_) => {
                                                messages_results.push(Ok((
                                                    *downstream_id,
                                                    Mining::SetExtranoncePrefix(
                                                        SetExtranoncePrefix {
                                                            channel_id: *channel_id,
                                                            extranonce_prefix: prefix.into(),
                                                        },
                                                    ),
                                                )
                                                    .into()));
                                            }
                                            Err(e) => {
                                                messages_results.push(Err(JDCError::ChannelSv2(
                                                    ChannelSv2Error::StandardChannelServerSide(e),
                                                )));
                                            }
                                        },
                                        Err(e) => {
                                            messages_results.push(Err(
                                                JDCError::ExtranoncePrefixFactoryError(e),
                                            ));
                                        }
                                    }
                                }
                                for (channel_id, extended_channel) in
                                    data.extended_channels.iter_mut()
                                {
                                    match channel_manager_data
                                        .extranonce_prefix_factory_extended
                                        .next_prefix_extended(
                                            extended_channel.get_rollable_extranonce_size()
                                                as usize,
                                        ) {
                                        Ok(prefix) => match extended_channel
                                            .set_extranonce_prefix(prefix.clone().to_vec())
                                        {
                                            Ok(_) => {
                                                messages_results.push(Ok((
                                                    *downstream_id,
                                                    Mining::SetExtranoncePrefix(
                                                        SetExtranoncePrefix {
                                                            channel_id: *channel_id,
                                                            extranonce_prefix: prefix.into(),
                                                        },
                                                    ),
                                                )
                                                    .into()));
                                            }
                                            Err(e) => {
                                                messages_results.push(Err(JDCError::ChannelSv2(
                                                    ChannelSv2Error::ExtendedChannelServerSide(e),
                                                )));
                                            }
                                        },
                                        Err(e) => {
                                            messages_results.push(Err(
                                                JDCError::ExtranoncePrefixFactoryError(e),
                                            ));
                                        }
                                    }
                                }
                            });
                        }
                    }
                    Ok(messages_results)
                })?;

        for message in messages_results.into_iter().flatten() {
            message.forward(&self.channel_manager_channel).await;
        }
        Ok(())
    }

    // Handles `SubmitSharesSuccess` messages from upstream.
    async fn handle_submit_shares_success(
        &mut self,
        _server_id: Option<usize>,
        msg: SubmitSharesSuccess,
    ) -> Result<(), Self::Error> {
        info!("Received: {} ✅", msg);
        Ok(())
    }

    // Handles `SubmitSharesError` messages from upstream.
    async fn handle_submit_shares_error(
        &mut self,
        _server_id: Option<usize>,
        msg: SubmitSharesError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {} ❌", msg);
        Ok(())
    }

    // Handles `NewMiningJob` messages from upstream. JDC ignores it.
    async fn handle_new_mining_job(
        &mut self,
        _server_id: Option<usize>,
        msg: NewMiningJob<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        warn!("⚠️ JDC does not expect jobs from the upstream server — ignoring.");
        Ok(())
    }

    // Handles `NewExtendedMiningJob` messages from upstream. JDC ignores it.
    async fn handle_new_extended_mining_job(
        &mut self,
        _server_id: Option<usize>,
        msg: NewExtendedMiningJob<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        warn!("⚠️ JDC does not expect jobs from the upstream server — ignoring.");
        Ok(())
    }

    // Handles `SetNewPrevHash` messages from upstream. JDC ignores it.
    async fn handle_set_new_prev_hash(
        &mut self,
        _server_id: Option<usize>,
        msg: SetNewPrevHash<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        warn!("⚠️ JDC does not expect prevhash updates from the upstream server — ignoring.");
        Ok(())
    }

    // Handles `SetCustomMiningJobSuccess` messages from upstream.
    //
    // On success:
    // - Updates the `job_id_to_template_id` mapping.
    // - Updates the channel state accordingly.
    // - Removes the associated `last_declare_job`, completing its lifecycle.
    async fn handle_set_custom_mining_job_success(
        &mut self,
        _server_id: Option<usize>,
        msg: SetCustomMiningJobSuccess,
    ) -> Result<(), Self::Error> {
        info!("Received: {} ✅", msg);
        self.channel_manager_data.super_safe_lock(|data| {
            if let Some(last_declare_job) = data.last_declare_job_store.remove(&msg.request_id) {
                let template_id = last_declare_job.template.template_id;
                data.last_declare_job_store
                    .retain(|_, job| job.template.template_id != template_id);

                data.template_id_to_upstream_job_id
                    .insert(last_declare_job.template.template_id, msg.job_id as u64);
                debug!(job_id = msg.job_id, "Mapped custom job into template store");
                if let (Some(upstream_channel), Some(set_custom_job)) = (
                    data.upstream_channel.as_mut(),
                    last_declare_job.set_custom_mining_job,
                ) {
                    if let Err(e) =
                        upstream_channel.on_set_custom_mining_job_success(set_custom_job, msg)
                    {
                        error!("Custom mining job success validation failed: {e:#?}");
                    }
                }
            } else {
                warn!(
                    request_id = msg.request_id,
                    "No matching declare job found for custom job success"
                );
            }
        });
        Ok(())
    }

    // Handles a `SetCustomMiningJobError` from upstream.
    //
    // Receiving this is treated as malicious behavior, so we immediately
    // trigger the fallback mechanism.
    async fn handle_set_custom_mining_job_error(
        &mut self,
        _server_id: Option<usize>,
        msg: SetCustomMiningJobError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("⚠️ Received: {} ❌", msg);
        warn!("⚠️ Starting fallback mechanism.");
        _ = self
            .channel_manager_channel
            .status_sender
            .send(Status {
                state: State::UpstreamShutdownFallback(JDCError::Shutdown),
            })
            .await;
        Ok(())
    }

    // Handles a `SetTarget` message from upstream.
    //
    // Updates the corresponding upstream channel's target state.
    async fn handle_set_target(
        &mut self,
        _server_id: Option<usize>,
        msg: SetTarget<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        self.channel_manager_data.super_safe_lock(|data| {
            if let Some(ref mut upstream) = data.upstream_channel {
                upstream.set_target(Target::from_le_bytes(
                    msg.maximum_target.clone().as_ref().try_into().unwrap(),
                ));
            }
        });
        Ok(())
    }

    // Handles `SetGroupChannel` messages from upstream. JDC ignores it.
    async fn handle_set_group_channel(
        &mut self,
        _server_id: Option<usize>,
        msg: SetGroupChannel<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        warn!("⚠️ JDC does not expect group channel updates from the upstream server — ignoring.");
        Ok(())
    }
}
