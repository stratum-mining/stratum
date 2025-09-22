use std::sync::{Arc, RwLock};

use crate::{
    error::TproxyError,
    sv2::{channel_manager::ChannelMode, ChannelManager},
    utils::proxy_extranonce_prefix_len,
};
use stratum_common::roles_logic_sv2::{
    channels_sv2::client::extended::ExtendedChannel,
    handlers_sv2::{HandleMiningMessagesFromServerAsync, SupportedChannelTypes},
    mining_sv2::{
        CloseChannel, ExtendedExtranonce, Extranonce, NewExtendedMiningJob, NewMiningJob,
        OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannelSuccess,
        SetCustomMiningJobError, SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel,
        SetNewPrevHash, SetTarget, SubmitSharesError, SubmitSharesSuccess, UpdateChannelError,
        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR, MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
        MESSAGE_TYPE_SET_GROUP_CHANNEL,
    },
    parsers_sv2::Mining,
    utils::Mutex,
};

use tracing::{debug, error, info, warn};

impl HandleMiningMessagesFromServerAsync for ChannelManager {
    type Error = TproxyError;

    fn get_channel_type_for_server(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }

    fn is_work_selection_enabled_for_server(&self) -> bool {
        false
    }

    async fn handle_open_standard_mining_channel_success(
        &mut self,
        m: OpenStandardMiningChannelSuccess<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", m);
        Err(Self::Error::UnexpectedMessage(
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
        ))
    }

    async fn handle_open_extended_mining_channel_success(
        &mut self,
        m: OpenExtendedMiningChannelSuccess<'_>,
    ) -> Result<(), Self::Error> {
        // Check if we have the pending channel data, return error if not
        let (user_identity, nominal_hashrate, downstream_extranonce_len) = self
            .channel_manager_data
            .safe_lock(|channel_manager_data| {
                channel_manager_data.pending_channels.remove(&m.request_id)
            })
            .map_err(|e| {
                error!("Failed to lock channel manager data: {:?}", e);
                TproxyError::PoisonLock
            })?
            .ok_or_else(|| {
                error!("No pending channel found for request_id: {}", m.request_id);
                TproxyError::PendingChannelNotFound(m.request_id)
            })?;

        let success = self
            .channel_manager_data
            .safe_lock(|channel_manager_data| {
                info!(
                    "Received: {}, user_identity: {}, nominal_hashrate: {}",
                    m, user_identity, nominal_hashrate
                );
                let extranonce_prefix = m.extranonce_prefix.clone().into_static().to_vec();
                let target = m.target.clone().into_static();
                let version_rolling = true; // we assume this is always true on extended channels
                let extended_channel = ExtendedChannel::new(
                    m.channel_id,
                    user_identity.clone(),
                    extranonce_prefix.clone(),
                    target.clone().into(),
                    nominal_hashrate,
                    version_rolling,
                    m.extranonce_size,
                );

                // If we are in aggregated mode, we need to create a new extranonce prefix and
                // insert the extended channel into the map
                if channel_manager_data.mode == ChannelMode::Aggregated {
                    channel_manager_data.upstream_extended_channel =
                        Some(Arc::new(RwLock::new(extended_channel.clone())));

                    let upstream_extranonce_prefix: Extranonce = m.extranonce_prefix.clone().into();
                    let translator_proxy_extranonce_prefix_len = proxy_extranonce_prefix_len(
                        m.extranonce_size.into(),
                        downstream_extranonce_len,
                    );

                    // range 0 is the extranonce1 from upstream
                    // range 1 is the extranonce1 added by the tproxy
                    // range 2 is the extranonce2 used by the miner for rolling (this is the one
                    // that is used for rolling)
                    let range_0 = 0..extranonce_prefix.len();
                    let range1 = range_0.end..range_0.end + translator_proxy_extranonce_prefix_len;
                    let range2 = range1.end..range1.end + downstream_extranonce_len;
                    debug!("\n\nrange_0: {:?}, range1: {:?}, range2: {:?}\n\n", range_0, range1, range2);
                    let extended_extranonce_factory = ExtendedExtranonce::from_upstream_extranonce(
                        upstream_extranonce_prefix,
                        range_0,
                        range1,
                        range2,
                    )
                    .expect("Failed to create ExtendedExtranonce from upstream extranonce");
                    channel_manager_data.extranonce_prefix_factory =
                        Some(Arc::new(Mutex::new(extended_extranonce_factory)));

                    let factory = channel_manager_data
                        .extranonce_prefix_factory
                        .as_ref()
                        .expect("extranonce_prefix_factory should be set after creation");
                    let new_extranonce_size = factory
                        .safe_lock(|f| f.get_range2_len())
                        .expect("extranonce_prefix_factory mutex should not be poisoned")
                        as u16;
                    let new_extranonce_prefix = factory
                        .safe_lock(|f| f.next_prefix_extended(new_extranonce_size as usize))
                        .expect("extranonce_prefix_factory mutex should not be poisoned")
                        .expect("next_prefix_extended should return a value for valid input")
                        .into_b032();
                    let new_downstream_extended_channel = ExtendedChannel::new(
                        m.channel_id,
                        user_identity.clone(),
                        new_extranonce_prefix.clone().into_static().to_vec(),
                        target.clone().into(),
                        nominal_hashrate,
                        true,
                        new_extranonce_size,
                    );
                    channel_manager_data.extended_channels.insert(
                        m.channel_id,
                        Arc::new(RwLock::new(new_downstream_extended_channel)),
                    );
                    let new_open_extended_mining_channel_success =
                        OpenExtendedMiningChannelSuccess {
                            request_id: m.request_id,
                            channel_id: m.channel_id,
                            extranonce_prefix: new_extranonce_prefix,
                            extranonce_size: new_extranonce_size,
                            target: m.target.clone(),
                        };
                    new_open_extended_mining_channel_success.into_static()
                } else {
                    // Non-aggregated mode: check if we need to adjust extranonce size
                    if m.extranonce_size as usize != downstream_extranonce_len {
                        // We need to create an extranonce factory to ensure proper extranonce2_size
                        let upstream_extranonce_prefix: Extranonce = m.extranonce_prefix.clone().into();
                        let translator_proxy_extranonce_prefix_len = proxy_extranonce_prefix_len(
                            m.extranonce_size.into(),
                            downstream_extranonce_len,
                        );

                        // range 0 is the extranonce1 from upstream
                        // range 1 is the extranonce1 added by the tproxy
                        // range 2 is the extranonce2 used by the miner for rolling
                        let range_0 = 0..extranonce_prefix.len();
                        let range1 = range_0.end..range_0.end + translator_proxy_extranonce_prefix_len;
                        let range2 = range1.end..range1.end + downstream_extranonce_len;
                        debug!("\n\nrange_0: {:?}, range1: {:?}, range2: {:?}\n\n", range_0, range1, range2);
                        // Create the factory - this should succeed if configuration is valid
                        let extended_extranonce_factory = ExtendedExtranonce::from_upstream_extranonce(
                            upstream_extranonce_prefix,
                            range_0,
                            range1,
                            range2,
                        )
                        .expect("Failed to create ExtendedExtranonce factory - likely extranonce size configuration issue");
                        // Store the factory for this specific channel
                        let factory = Arc::new(Mutex::new(extended_extranonce_factory));
                        let new_extranonce_prefix = factory
                            .safe_lock(|f| f.next_prefix_extended(downstream_extranonce_len))
                            .expect("Failed to access extranonce factory")
                            .expect("Failed to generate extranonce prefix")
                            .into_b032();
                        // Create channel with the configured extranonce size
                        let new_downstream_extended_channel = ExtendedChannel::new(
                            m.channel_id,
                            user_identity.clone(),
                            new_extranonce_prefix.clone().into_static().to_vec(),
                            target.clone().into(),
                            nominal_hashrate,
                            true,
                            downstream_extranonce_len as u16,
                        );
                        channel_manager_data.extended_channels.insert(
                            m.channel_id,
                            Arc::new(RwLock::new(new_downstream_extended_channel)),
                        );
                        // Store factory for this channel (we'll need it for share processing)
                        if channel_manager_data.extranonce_factories.is_none() {
                            channel_manager_data.extranonce_factories = Some(std::collections::HashMap::new());
                        }
                        if let Some(ref mut factories) = channel_manager_data.extranonce_factories {
                            factories.insert(m.channel_id, factory);
                        }
                        let new_open_extended_mining_channel_success = OpenExtendedMiningChannelSuccess {
                            request_id: m.request_id,
                            channel_id: m.channel_id,
                            extranonce_prefix: new_extranonce_prefix,
                            extranonce_size: downstream_extranonce_len as u16,
                            target: m.target.clone(),
                        };
                        new_open_extended_mining_channel_success.into_static()
                    } else {
                        // Extranonce size matches, use as-is
                        channel_manager_data
                            .extended_channels
                            .insert(m.channel_id, Arc::new(RwLock::new(extended_channel)));
                        m.into_static()
                    }
                }
            })
            .map_err(|e| {
                error!("Failed to lock channel manager data: {:?}", e);
                TproxyError::PoisonLock
            })?;

        self.channel_state
            .sv1_server_sender
            .send(Mining::OpenExtendedMiningChannelSuccess(success.clone()))
            .await
            .map_err(|e| {
                error!("Failed to send OpenExtendedMiningChannelSuccess: {:?}", e);
                TproxyError::ChannelErrorSender
            })?;

        Ok(())
    }

    async fn handle_open_mining_channel_error(
        &mut self,
        m: OpenMiningChannelError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", m);
        todo!("OpenMiningChannelError not handled yet");
    }

    async fn handle_update_channel_error(
        &mut self,
        m: UpdateChannelError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", m);
        Ok(())
    }

    async fn handle_close_channel(&mut self, m: CloseChannel<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", m);
        _ = self.channel_manager_data.safe_lock(|channel_data_manager| {
            if channel_data_manager.mode == ChannelMode::Aggregated {
                if channel_data_manager.upstream_extended_channel.is_some() {
                    channel_data_manager.upstream_extended_channel = None;
                }
            } else {
                channel_data_manager.extended_channels.remove(&m.channel_id);
            }
        });
        Ok(())
    }

    async fn handle_set_extranonce_prefix(
        &mut self,
        m: SetExtranoncePrefix<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", m);
        warn!("⚠️ Cannot process SetExtranoncePrefix since set_extranonce is not supported for majority of sv1 clients. Ignoring.");
        Ok(())
    }

    async fn handle_submit_shares_success(
        &mut self,
        m: SubmitSharesSuccess,
    ) -> Result<(), Self::Error> {
        info!("Received: {} ✅", m);
        Ok(())
    }

    async fn handle_submit_shares_error(
        &mut self,
        m: SubmitSharesError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {} ❌", m);
        Ok(())
    }

    async fn handle_new_mining_job(&mut self, m: NewMiningJob<'_>) -> Result<(), Self::Error> {
        warn!("Received: {}", m);
        warn!("⚠️ Cannot process NewMiningJob since Translator Proxy supports only extended mining jobs. Ignoring.");
        Ok(())
    }

    async fn handle_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", m);
        let mut m_static = m.clone().into_static();
        _ = self.channel_manager_data.safe_lock(|channel_manage_data| {
            if channel_manage_data.mode == ChannelMode::Aggregated {
                if let Some(upstream_channel) = &channel_manage_data.upstream_extended_channel {
                    if let Ok(mut upstream_extended_channel) = upstream_channel.write() {
                        let _ =
                            upstream_extended_channel.on_new_extended_mining_job(m_static.clone());
                        m_static.channel_id = 0; // this is done so that every aggregated downstream
                                                 // will
                                                 // receive the NewExtendedMiningJob message
                    }
                }
                channel_manage_data
                    .extended_channels
                    .iter()
                    .for_each(|(_, channel)| {
                        if let Ok(mut channel) = channel.write() {
                            let _ = channel.on_new_extended_mining_job(m_static.clone());
                        }
                    });
            } else if let Some(channel) = channel_manage_data
                .extended_channels
                .get(&m_static.channel_id)
            {
                if let Ok(mut channel) = channel.write() {
                    let _ = channel.on_new_extended_mining_job(m_static.clone());
                }
            }
        });
        let job = m_static;
        if !job.is_future() {
            self.channel_state
                .sv1_server_sender
                .send(Mining::NewExtendedMiningJob(job))
                .await
                .map_err(|e| {
                    error!("Failed to send immediate NewExtendedMiningJob: {:?}", e);
                    TproxyError::ChannelErrorSender
                })?;
        }
        Ok(())
    }

    async fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", m);
        let m_static = m.clone().into_static();
        _ = self.channel_manager_data.safe_lock(|channel_manager_data| {
            if channel_manager_data.mode == ChannelMode::Aggregated {
                if let Some(upstream_channel) = &channel_manager_data.upstream_extended_channel {
                    if let Ok(mut upstream_extended_channel) = upstream_channel.write() {
                        _ = upstream_extended_channel.on_set_new_prev_hash(m_static.clone());
                    }
                }
                channel_manager_data
                    .extended_channels
                    .iter()
                    .for_each(|(_, channel)| {
                        if let Ok(mut channel) = channel.write() {
                            _ = channel.on_set_new_prev_hash(m_static.clone());
                        }
                    });
            } else if let Some(channel) = channel_manager_data
                .extended_channels
                .get(&m_static.channel_id)
            {
                if let Ok(mut channel) = channel.write() {
                    _ = channel.on_set_new_prev_hash(m_static.clone());
                }
            }
        });

        self.channel_state
            .sv1_server_sender
            .send(Mining::SetNewPrevHash(m_static.clone()))
            .await
            .map_err(|e| {
                error!("Failed to send SetNewPrevHash: {:?}", e);
                TproxyError::ChannelErrorSender
            })?;

        let mode = self
            .channel_manager_data
            .super_safe_lock(|c| c.mode.clone());

        let active_job = if mode == ChannelMode::Aggregated {
            self.channel_manager_data.super_safe_lock(|c| {
                c.upstream_extended_channel
                    .as_ref()
                    .and_then(|ch| ch.read().ok())
                    .and_then(|ch| ch.get_active_job().map(|j| j.0.clone()))
            })
        } else {
            self.channel_manager_data.super_safe_lock(|c| {
                c.extended_channels
                    .get(&m.channel_id)
                    .and_then(|ch| ch.read().ok())
                    .and_then(|ch| ch.get_active_job().map(|j| j.0.clone()))
            })
        };

        if let Some(mut job) = active_job {
            if mode == ChannelMode::Aggregated {
                job.channel_id = 0;
            }
            self.channel_state
                .sv1_server_sender
                .send(Mining::NewExtendedMiningJob(job))
                .await
                .map_err(|e| {
                    error!("Failed to send NewExtendedMiningJob: {:?}", e);
                    TproxyError::ChannelErrorSender
                })?;
        }
        Ok(())
    }

    async fn handle_set_custom_mining_job_success(
        &mut self,
        m: SetCustomMiningJobSuccess,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", m);
        warn!("⚠️ Cannot process SetCustomMiningJobSuccess since Translator Proxy does not support custom mining jobs. Ignoring.");
        Err(Self::Error::UnexpectedMessage(
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
        ))
    }

    async fn handle_set_custom_mining_job_error(
        &mut self,
        m: SetCustomMiningJobError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", m);
        warn!("⚠️ Cannot process SetCustomMiningJobError since Translator Proxy does not support custom mining jobs. Ignoring.");
        Err(Self::Error::UnexpectedMessage(
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
        ))
    }

    async fn handle_set_target(&mut self, m: SetTarget<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", m);

        // Update the channel targets in the channel manager
        _ = self.channel_manager_data.safe_lock(|channel_manager_data| {
            if channel_manager_data.mode == ChannelMode::Aggregated {
                if let Some(upstream_channel) = &channel_manager_data.upstream_extended_channel {
                    if let Ok(mut upstream_extended_channel) = upstream_channel.write() {
                        upstream_extended_channel.set_target(m.maximum_target.clone().into());
                    }
                }
                channel_manager_data
                    .extended_channels
                    .iter()
                    .for_each(|(_, channel)| {
                        if let Ok(mut channel) = channel.write() {
                            channel.set_target(m.maximum_target.clone().into());
                        }
                    });
            } else if let Some(channel) = channel_manager_data.extended_channels.get(&m.channel_id)
            {
                if let Ok(mut channel) = channel.write() {
                    channel.set_target(m.maximum_target.clone().into());
                }
            }
        });

        // Forward SetTarget message to SV1Server for vardiff processing
        self.channel_state
            .sv1_server_sender
            .send(Mining::SetTarget(m.clone().into_static()))
            .await
            .map_err(|e| {
                error!("Failed to forward SetTarget message to SV1Server: {:?}", e);
                TproxyError::ChannelErrorSender
            })?;

        Ok(())
    }

    async fn handle_set_group_channel(
        &mut self,
        m: SetGroupChannel<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", m);
        warn!("⚠️ Cannot process SetGroupChannel since Translator Proxy does not support group channels. Ignoring.");
        Err(Self::Error::UnexpectedMessage(
            MESSAGE_TYPE_SET_GROUP_CHANNEL,
        ))
    }
}
