use std::sync::{Arc, RwLock};

use crate::{
    error::TproxyError,
    sv2::{channel_manager::ChannelMode, ChannelManager},
    utils::proxy_extranonce_prefix_len,
};
use roles_logic_sv2::{
    channels_sv2::client::extended::ExtendedChannel,
    handlers_sv2::{HandleMiningMessagesFromServerAsync, HandlerError},
    mining_sv2::{
        ExtendedExtranonce, Extranonce, NewExtendedMiningJob, OpenExtendedMiningChannelSuccess,
        SetNewPrevHash, SetTarget, FULL_EXTRANONCE_LEN,
    },
    parsers_sv2::Mining,
    utils::Mutex,
};

use tracing::{debug, error, info, warn};

impl HandleMiningMessagesFromServerAsync for ChannelManager {
    fn get_channel_type_for_server(&self) -> roles_logic_sv2::handlers_sv2::SupportedChannelTypes {
        roles_logic_sv2::handlers_sv2::SupportedChannelTypes::Extended
    }

    fn is_work_selection_enabled_for_server(&self) -> bool {
        false
    }

    async fn handle_open_standard_mining_channel_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::OpenStandardMiningChannelSuccess<'_>,
    ) -> Result<(), HandlerError> {
        unreachable!()
    }

    async fn handle_open_extended_mining_channel_success(
        &mut self,
        m: OpenExtendedMiningChannelSuccess<'_>,
    ) -> Result<(), HandlerError> {
        let success  = self.channel_manager_data.safe_lock(|channel_manager_data| {
            // Get the stored user identity and hashrate using request_id as downstream_id
            let (user_identity, nominal_hashrate, downstream_extranonce_len) = channel_manager_data
            .pending_channels
            .remove(&m.request_id)
            .unwrap_or_else(|| ("unknown".to_string(), 100000.0, 0_usize));
            info!(
                "Received OpenExtendedMiningChannelSuccess from upstream with request id: {} and channel id: {}, user: {}, hashrate: {}",
                m.request_id, m.channel_id, user_identity, nominal_hashrate
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

            // If we are in aggregated mode, we need to create a new extranonce prefix and insert the
            // extended channel into the map
            if channel_manager_data.mode == ChannelMode::Aggregated {
                channel_manager_data.upstream_extended_channel = Some(Arc::new(RwLock::new(extended_channel.clone())));

                let upstream_extranonce_prefix: Extranonce = m.extranonce_prefix.clone().into();
                let translator_proxy_extranonce_prefix_len =
                    proxy_extranonce_prefix_len(m.extranonce_size.into(), downstream_extranonce_len);

                // range 0 is the extranonce1 from upstream
                // range 1 is the extranonce1 added by the tproxy
                // range 2 is the extranonce2 used by the miner for rolling (this is the one that is
                // used for rolling)
                let range_0 = 0..extranonce_prefix.len();
                let range1 = range_0.end..range_0.end + translator_proxy_extranonce_prefix_len;
                let range2 = range1.end..FULL_EXTRANONCE_LEN;
                let extended_extranonce_factory = ExtendedExtranonce::from_upstream_extranonce(
                    upstream_extranonce_prefix,
                    range_0,
                    range1,
                    range2,
                )
                .unwrap();
                channel_manager_data.extranonce_prefix_factory =
                    Some(Arc::new(Mutex::new(extended_extranonce_factory)));

                let factory = channel_manager_data.extranonce_prefix_factory.as_ref().unwrap();
                let new_extranonce_size = factory.safe_lock(|f| f.get_range2_len()).unwrap() as u16;
                if downstream_extranonce_len <= new_extranonce_size as usize {
                    let new_extranonce_prefix = factory
                        .safe_lock(|f| f.next_prefix_extended(new_extranonce_size as usize))
                        .unwrap()
                        .unwrap()
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
                    let new_open_extended_mining_channel_success = OpenExtendedMiningChannelSuccess {
                        request_id: m.request_id,
                        channel_id: m.channel_id,
                        extranonce_prefix: new_extranonce_prefix,
                        extranonce_size: new_extranonce_size,
                        target: m.target.clone(),
                    };
                    return new_open_extended_mining_channel_success.into_static();
                }
            }

            // If we are not in aggregated mode, we just insert the extended channel into the map
            channel_manager_data.extended_channels
                .insert(m.channel_id, Arc::new(RwLock::new(extended_channel)));

            m.into_static()
        }).unwrap();

        self.channel_state
            .sv1_server_sender
            .send(Mining::OpenExtendedMiningChannelSuccess(success.clone()))
            .await
            .map_err(|e| {
                error!("Failed to send OpenExtendedMiningChannelSuccess: {:?}", e);
                HandlerError::External(Box::new(TproxyError::ChannelErrorSender))
            })?;

        Ok(())
    }

    async fn handle_open_mining_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenMiningChannelError<'_>,
    ) -> Result<(), HandlerError> {
        error!(
            "Received OpenExtendedMiningChannelError from upstream with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        todo!("OpenExtendedMiningChannelError not handled yet");
    }

    async fn handle_update_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::UpdateChannelError<'_>,
    ) -> Result<(), HandlerError> {
        error!(
            "Received UpdateChannelError from upstream with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Ok(())
    }

    async fn handle_close_channel(
        &mut self,
        m: roles_logic_sv2::mining_sv2::CloseChannel<'_>,
    ) -> Result<(), HandlerError> {
        info!(
            "Received CloseChannel from upstream for channel id: {}",
            m.channel_id
        );
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
        _m: roles_logic_sv2::mining_sv2::SetExtranoncePrefix<'_>,
    ) -> Result<(), HandlerError> {
        unreachable!("Cannot process SetExtranoncePrefix since set_extranonce is not supported for majority of sv1 clients");
    }

    async fn handle_submit_shares_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesSuccess,
    ) -> Result<(), HandlerError> {
        info!(
            "Received SubmitSharesSuccess from upstream for channel id: {} ✅",
            m.channel_id
        );
        debug!("SubmitSharesSuccess: {:?}", m);
        Ok(())
    }

    async fn handle_submit_shares_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesError<'_>,
    ) -> Result<(), HandlerError> {
        warn!(
            "Received SubmitSharesError from upstream for channel id: {} ❌",
            m.channel_id
        );
        Ok(())
    }

    async fn handle_new_mining_job(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::NewMiningJob<'_>,
    ) -> Result<(), HandlerError> {
        unreachable!(
            "Cannot process NewMiningJob since Translator Proxy supports only extended mining jobs"
        )
    }

    async fn handle_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob<'_>,
    ) -> Result<(), HandlerError> {
        info!(
            "Received NewExtendedMiningJob from upstream for channel id: {}",
            m.channel_id
        );
        debug!("NewExtendedMiningJob: {:?}", m);
        let mut m_static = m.clone().into_static();
        _ = self.channel_manager_data.safe_lock(|channel_manage_data| {
            if channel_manage_data.mode == ChannelMode::Aggregated {
                if channel_manage_data.upstream_extended_channel.is_some() {
                    let mut upstream_extended_channel = channel_manage_data
                        .upstream_extended_channel
                        .as_ref()
                        .unwrap()
                        .write()
                        .unwrap();
                    let _ = upstream_extended_channel.on_new_extended_mining_job(m_static.clone());
                    m_static.channel_id = 0; // this is done so that every aggregated downstream
                                             // will
                                             // receive the NewExtendedMiningJob message
                }
                channel_manage_data
                    .extended_channels
                    .iter()
                    .for_each(|(_, channel)| {
                        let mut channel = channel.write().unwrap();
                        let _ = channel.on_new_extended_mining_job(m_static.clone());
                    });
            } else if let Some(channel) = channel_manage_data
                .extended_channels
                .get(&m_static.channel_id)
            {
                let mut channel = channel.write().unwrap();
                let _ = channel.on_new_extended_mining_job(m_static.clone());
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
                    HandlerError::External(Box::new(TproxyError::ChannelErrorSender))
                })?;
        }
        Ok(())
    }

    async fn handle_set_new_prev_hash(
        &mut self,
        m: SetNewPrevHash<'_>,
    ) -> Result<(), HandlerError> {
        let m_static = m.clone().into_static();
        _ = self.channel_manager_data.safe_lock(|channel_manager_data| {
            info!(
                "Received SetNewPrevHash from upstream for channel id: {}",
                m.channel_id
            );
            debug!("SetNewPrevHash: {:?}", m);

            if channel_manager_data.mode == ChannelMode::Aggregated {
                if channel_manager_data.upstream_extended_channel.is_some() {
                    let mut upstream_extended_channel = channel_manager_data
                        .upstream_extended_channel
                        .as_ref()
                        .unwrap()
                        .write()
                        .unwrap();
                    _ = upstream_extended_channel.on_set_new_prev_hash(m_static.clone());
                }
                channel_manager_data
                    .extended_channels
                    .iter()
                    .for_each(|(_, channel)| {
                        let mut channel = channel.write().unwrap();
                        _ = channel.on_set_new_prev_hash(m_static.clone());
                    });
            } else if let Some(channel) = channel_manager_data
                .extended_channels
                .get(&m_static.channel_id)
            {
                let mut channel = channel.write().unwrap();
                _ = channel.on_set_new_prev_hash(m_static.clone());
            }
        });

        self.channel_state
            .sv1_server_sender
            .send(Mining::SetNewPrevHash(m_static.clone()))
            .await
            .map_err(|e| {
                error!("Failed to send SetNewPrevHash: {:?}", e);
                HandlerError::External(Box::new(TproxyError::ChannelErrorSender))
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
                    HandlerError::External(Box::new(TproxyError::ChannelErrorSender))
                })?;
        }
        Ok(())
    }

    async fn handle_set_custom_mining_job_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetCustomMiningJobSuccess,
    ) -> Result<(), HandlerError> {
        unreachable!("Cannot process SetCustomMiningJobSuccess since Translator Proxy does not support custom mining jobs")
    }

    async fn handle_set_custom_mining_job_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetCustomMiningJobError<'_>,
    ) -> Result<(), HandlerError> {
        unreachable!("Cannot process SetCustomMiningJobError since Translator Proxy does not support custom mining jobs")
    }

    async fn handle_set_target(&mut self, m: SetTarget<'_>) -> Result<(), HandlerError> {
        info!(
            "Received SetTarget from upstream for channel id: {}",
            m.channel_id
        );
        debug!("SetTarget: {:?}", m);

        // Update the channel targets in the channel manager
        _ = self.channel_manager_data.safe_lock(|channel_manager_data| {
            if channel_manager_data.mode == ChannelMode::Aggregated {
                if channel_manager_data.upstream_extended_channel.is_some() {
                    let mut upstream_extended_channel = channel_manager_data
                        .upstream_extended_channel
                        .as_ref()
                        .unwrap()
                        .write()
                        .unwrap();
                    upstream_extended_channel.set_target(m.maximum_target.clone().into());
                }
                channel_manager_data
                    .extended_channels
                    .iter()
                    .for_each(|(_, channel)| {
                        let mut channel = channel.write().unwrap();
                        channel.set_target(m.maximum_target.clone().into());
                    });
            } else if let Some(channel) = channel_manager_data.extended_channels.get(&m.channel_id)
            {
                let mut channel = channel.write().unwrap();
                channel.set_target(m.maximum_target.clone().into());
            }
        });

        // Forward SetTarget message to SV1Server for vardiff processing
        self.channel_state
            .sv1_server_sender
            .send(Mining::SetTarget(m.clone().into_static()))
            .await
            .map_err(|e| {
                error!("Failed to forward SetTarget message to SV1Server: {:?}", e);
                HandlerError::External(Box::new(TproxyError::ChannelErrorSender))
            })?;

        Ok(())
    }

    async fn handle_set_group_channel(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetGroupChannel<'_>,
    ) -> Result<(), HandlerError> {
        unreachable!(
            "Cannot process SetGroupChannel since Translator Proxy does not support group channels"
        )
    }
}
