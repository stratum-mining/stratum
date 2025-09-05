use stratum_common::roles_logic_sv2::{
    self,
    channels_sv2::{client::extended::ExtendedChannel, server::jobs::factory::JobFactory},
    handlers_sv2::{
        HandleMiningMessagesFromServerAsync, HandlerError as Error, SupportedChannelTypes,
    },
    mining_sv2::*,
    parsers_sv2::{IsSv2Message, Mining},
};
use tracing::{debug, error, info, warn};

use crate::{
    channel_manager::{downstream_message_handler::RouteMessageTo, ChannelManager},
    error::JDCError,
    status::{State, Status},
    utils::UpstreamState,
};

impl HandleMiningMessagesFromServerAsync for ChannelManager {
    fn get_channel_type_for_server(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }
    fn is_work_selection_enabled_for_server(&self) -> bool {
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
        msg: OpenStandardMiningChannelSuccess<'_>,
    ) -> Result<(), Error> {
        info!(
            "Received OpenMiningChannelError from upstream. JDC only supports extended channels, preparing fallback for request_id={}",
            msg.request_id
        );
        debug!("OpenStandardMiningChannelSuccess: {msg:?}");
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
        msg: OpenExtendedMiningChannelSuccess<'_>,
    ) -> Result<(), Error> {
        info!(
            "Received OpenExtendedMiningChannelSuccess from upstream with channel_id: {}",
            msg.channel_id
        );
        debug!("OpenExtendedMiningChannelSuccess: {msg:?}");

        let channel_state = self.channel_manager_data.super_safe_lock(|data| {
            let (hashrate, min_extranonce_size) = data
                .pending_downstream_requests
                .first()
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
                    self.upstream_state.set(UpstreamState::NoChannel);
                    return self.upstream_state.get();
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
                msg.target.into(),
                hashrate,
                true,
                min_extranonce_size,
            );

            if let Some(ref mut prevhash) = data.last_new_prev_hash {
                _ = extended_channel.on_chain_tip_update(prevhash.clone().into());
                debug!("Applied last_new_prev_hash to new extended channel");
            }

            data.extranonce_prefix_factory_extended = extranonces.clone();
            data.extranonce_prefix_factory_standard = extranonces;
            data.upstream_channel = Some(extended_channel);
            data.job_factory = Some(job_factory);
            self.upstream_state.set(UpstreamState::Connected);

            info!("Extended mining channel successfully initialized");
            self.upstream_state.get()
        });

        if channel_state == UpstreamState::Connected {
            let pending_downstreams = self
                .channel_manager_data
                .super_safe_lock(|data| std::mem::take(&mut data.pending_downstream_requests));

            for pending_downstream in pending_downstreams {
                let message_type = pending_downstream.message_type();
                self.send_open_channel_request_to_mining_handler(pending_downstream, message_type)
                    .await?;
            }
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
        msg: OpenMiningChannelError<'_>,
    ) -> Result<(), Error> {
        info!(
            "Received OpenMiningChannelError from upstream with request_id: {}, Preparing fallback.",
            msg.request_id
        );
        debug!("OpenMiningChannelError: {msg:?}");

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
        msg: UpdateChannelError<'_>,
    ) -> Result<(), Error> {
        info!(
            "Received UpdateChannelError from upstream with channel_id: {}",
            msg.channel_id
        );
        error!("UpdateChannelError: {msg:?}");
        Ok(())
    }

    // Handles `CloseChannel` messages from upstream.
    //
    // Upon receiving this message, the upstream channel is immediately closed and
    // the system transitions into the upstream shutdown fallback state.
    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Error> {
        info!(
            "Received CloseChannel from upstream with channel_id: {}",
            msg.channel_id
        );
        debug!("CloseChannel: {msg:?}");

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
        msg: SetExtranoncePrefix<'_>,
    ) -> Result<(), Error> {
        info!(
            "Received SetExtranoncePrefix from upstream with channel_id: {}",
            msg.channel_id
        );
        debug!("SetExtranoncePrefix: {msg:?}");
        let messages_results = self
            .channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                let mut messages_results: Vec<Result<RouteMessageTo, JDCError>> = vec![];
                if let Some(upstream_channel) = channel_manager_data.upstream_channel.as_mut() {
                    if let Err(_e) =
                        upstream_channel.set_extranonce_prefix(msg.extranonce_prefix.to_vec())
                    {
                        // Correct these errors, we need Extended Channel Error but on client side.
                        return Err(JDCError::RolesSv2Logic(
                            roles_logic_sv2::Error::BadPayloadSize),
                        );
                    }

                    let prefix_len = msg.extranonce_prefix.len();
                    let extranonce_size = FULL_EXTRANONCE_LEN - prefix_len;
                    let jdc_extranonce_len = std::cmp::min(
                        (extranonce_size)
                            .saturating_sub(self.min_extranonce_size as usize),
                        self.min_extranonce_size as usize,
                    );
                    let total_len = prefix_len + extranonce_size;
                    let range_0 = 0..prefix_len;
                    let range_1 = prefix_len..prefix_len + jdc_extranonce_len;
                    let range_2 = prefix_len + jdc_extranonce_len..total_len;

                    debug!(
                        prefix_len,
                        extranonce_size,
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
                            return Err(JDCError::RolesSv2Logic(
                                roles_logic_sv2::Error::ExtranoncePrefixFactoryError(e),
                            ));
                        }
                    };

                    channel_manager_data.extranonce_prefix_factory_extended = extranonces.clone();
                    channel_manager_data.extranonce_prefix_factory_standard = extranonces;

                    for (downstream_id, downstream) in channel_manager_data.downstream.iter_mut() {
                        downstream.downstream_data.super_safe_lock(|data| {
                            for (channel_id, standard_channel) in data.standard_channels.iter_mut()
                            {
                                match channel_manager_data
                                    .extranonce_prefix_factory_standard
                                    .next_prefix_standard()
                                {
                                    Ok(prefix) => match standard_channel.set_extranonce_prefix(prefix.clone().to_vec()) {
                                        Ok(_) => {
                                            messages_results.push(Ok((
                                                *downstream_id,
                                                Mining::SetExtranoncePrefix(SetExtranoncePrefix {
                                                    channel_id: *channel_id,
                                                    extranonce_prefix: prefix.into(),
                                                }),
                                            )
                                                .into()));
                                        }
                                        Err(e) => {
                                            messages_results.push(Err(JDCError::RolesSv2Logic(
                                                roles_logic_sv2::Error::FailedToUpdateStandardChannel(e),
                                            )));
                                        }
                                    },
                                    Err(e) => {
                                        messages_results.push(Err(JDCError::RolesSv2Logic(
                                            roles_logic_sv2::Error::ExtranoncePrefixFactoryError(e),
                                        )));
                                    }
                                }
                            }
                            for (channel_id, extended_channel) in data.extended_channels.iter_mut()
                            {
                                match channel_manager_data
                                    .extranonce_prefix_factory_extended
                                    .next_prefix_extended(extended_channel.get_rollable_extranonce_size() as usize)
                                {
                                    Ok(prefix) => match extended_channel.set_extranonce_prefix(prefix.clone().to_vec()) {
                                        Ok(_) => {
                                            messages_results.push(Ok((
                                                *downstream_id,
                                                Mining::SetExtranoncePrefix(SetExtranoncePrefix {
                                                    channel_id: *channel_id,
                                                    extranonce_prefix: prefix.into(),
                                                }),
                                            )
                                                .into()));
                                        }
                                        Err(e) => {
                                            messages_results.push(Err(JDCError::RolesSv2Logic(
                                                roles_logic_sv2::Error::FailedToUpdateExtendedChannel(e),
                                            )));
                                        }
                                    },
                                    Err(e) => {
                                        messages_results.push(Err(JDCError::RolesSv2Logic(
                                            roles_logic_sv2::Error::ExtranoncePrefixFactoryError(e),
                                        )));
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
        msg: SubmitSharesSuccess,
    ) -> Result<(), Error> {
        info!(
            "Received SubmitSharesSuccess from upstream with channel_id: {}",
            msg.channel_id
        );
        debug!("SubmitSharesSuccess: {msg:?}");
        Ok(())
    }

    // Handles `SubmitSharesError` messages from upstream.
    async fn handle_submit_shares_error(
        &mut self,
        msg: SubmitSharesError<'_>,
    ) -> Result<(), Error> {
        info!(
            "Received SubmitSharesError from upstream with channel_id: {}",
            msg.channel_id
        );
        error!("SubmitSharesError: {msg:?}");
        Ok(())
    }

    // Handles `NewMiningJob` messages from upstream. JDC ignores it.
    async fn handle_new_mining_job(&mut self, msg: NewMiningJob<'_>) -> Result<(), Error> {
        info!("Received NewMiningJob from upstream. JDC does not expect upstream jobs — ignoring.");
        debug!("NewMiningJob: {msg:?}");
        Ok(())
    }

    // Handles `NewExtendedMiningJob` messages from upstream. JDC ignores it.
    async fn handle_new_extended_mining_job(
        &mut self,
        msg: NewExtendedMiningJob<'_>,
    ) -> Result<(), Error> {
        info!("Received NewExtendedMiningJob from Upstream");
        debug!("NewExtendedMiningJob: {msg:?}");
        Ok(())
    }

    // Handles `SetNewPrevHash` messages from upstream. JDC ignores it.
    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash<'_>) -> Result<(), Error> {
        debug!(
            ?msg,
            "Ignored SetNewPrevHash from upstream. JDC relies on its own prevhash updates."
        );
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
        msg: SetCustomMiningJobSuccess,
    ) -> Result<(), Error> {
        info!(
            "Received SetCustomMiningJobSuccess from upstream with channel_id: {}",
            msg.channel_id
        );
        debug!("SetCustomMiningJobSuccess: {msg:?}");
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
        msg: SetCustomMiningJobError<'_>,
    ) -> Result<(), Error> {
        info!("⚠️ Received SetCustomMiningJobError from upstream with channel_id: {} ❌. Starting fallback mechanism.", msg.channel_id);
        debug!("SetCustomMiningJobError: {msg:?}");
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
    async fn handle_set_target(&mut self, msg: SetTarget<'_>) -> Result<(), Error> {
        info!(
            "Received SetTarget from upstream with channel_id: {}",
            msg.channel_id
        );
        debug!("SetTarget: {msg:?}");
        self.channel_manager_data.super_safe_lock(|data| {
            if let Some(ref mut upstream) = data.upstream_channel {
                upstream.set_target(msg.maximum_target.clone().into());
            }
        });
        Ok(())
    }

    // Handles `SetGroupChannel` messages from upstream. JDC ignores it.
    async fn handle_set_group_channel(&mut self, msg: SetGroupChannel<'_>) -> Result<(), Error> {
        info!(
            "Received SetGroupChannel from upstream with group_channel_id: {}",
            msg.group_channel_id
        );
        debug!("SetGroupChannel: {msg:?}");
        Ok(())
    }
}
