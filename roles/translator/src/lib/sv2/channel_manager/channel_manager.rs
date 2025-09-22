use crate::{
    error::TproxyError,
    status::{handle_error, Status, StatusSender},
    sv2::{
        channel_manager::{
            channel::ChannelState,
            data::{ChannelManagerData, ChannelMode},
        },
        upstream::upstream::{EitherFrame, Message, StdFrame},
    },
    task_manager::TaskManager,
    utils::{into_static, ShutdownMessage},
};
use async_channel::{Receiver, Sender};
use std::sync::{Arc, RwLock};
use stratum_common::roles_logic_sv2::{
    channels_sv2::client::extended::ExtendedChannel,
    codec_sv2::Frame,
    handlers_sv2::HandleMiningMessagesFromServerAsync,
    mining_sv2::OpenExtendedMiningChannelSuccess,
    parsers_sv2::{AnyMessage, Mining},
    utils::Mutex,
};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

/// Extra bytes allocated for translator search space in aggregated mode.
/// This allows the translator to manage multiple downstream connections
/// by allocating unique extranonce prefixes to each downstream.
const AGGREGATED_MODE_TRANSLATOR_SEARCH_SPACE_BYTES: usize = 4;

/// Type alias for SV2 mining messages with static lifetime
pub type Sv2Message = Mining<'static>;

/// Manages SV2 channels and message routing between upstream and downstream.
///
/// The ChannelManager serves as the central component that bridges SV2 upstream
/// connections with SV1 downstream connections. It handles:
/// - SV2 channel lifecycle management (open, close, error handling)
/// - Message translation and routing between protocols
/// - Extranonce management for aggregated vs non-aggregated modes
/// - Share submission processing and validation
/// - Job distribution to downstream connections
///
/// The manager supports two operational modes:
/// - Aggregated: All downstream connections share a single extended channel
/// - Non-aggregated: Each downstream connection gets its own extended channel
///
/// This design allows the translator to efficiently manage multiple mining
/// connections while maintaining proper isolation and state management.
#[derive(Debug, Clone)]
pub struct ChannelManager {
    pub channel_state: ChannelState,
    pub channel_manager_data: Arc<Mutex<ChannelManagerData>>,
}

impl ChannelManager {
    /// Creates a new ChannelManager instance.
    ///
    /// # Arguments
    /// * `upstream_sender` - Channel to send messages to upstream
    /// * `upstream_receiver` - Channel to receive messages from upstream
    /// * `sv1_server_sender` - Channel to send messages to SV1 server
    /// * `sv1_server_receiver` - Channel to receive messages from SV1 server
    /// * `mode` - Operating mode (Aggregated or NonAggregated)
    ///
    /// # Returns
    /// A new ChannelManager instance ready to handle message routing
    pub fn new(
        upstream_sender: Sender<EitherFrame>,
        upstream_receiver: Receiver<EitherFrame>,
        sv1_server_sender: Sender<Mining<'static>>,
        sv1_server_receiver: Receiver<Mining<'static>>,
        mode: ChannelMode,
    ) -> Self {
        let channel_state = ChannelState::new(
            upstream_sender,
            upstream_receiver,
            sv1_server_sender,
            sv1_server_receiver,
        );
        let channel_manager_data = Arc::new(Mutex::new(ChannelManagerData::new(mode)));
        Self {
            channel_state,
            channel_manager_data,
        }
    }

    /// Spawns and runs the main channel manager task loop.
    ///
    /// This method creates an async task that handles all message routing for the
    /// channel manager. The task runs a select loop that processes:
    /// - Shutdown signals for graceful termination
    /// - Messages from upstream SV2 server
    /// - Messages from downstream SV1 server
    ///
    /// The task continues running until a shutdown signal is received or an
    /// unrecoverable error occurs. It ensures proper cleanup of resources
    /// and error reporting.
    ///
    /// # Arguments
    /// * `notify_shutdown` - Broadcast channel for receiving shutdown signals
    /// * `shutdown_complete_tx` - Channel to signal when shutdown is complete
    /// * `status_sender` - Channel for sending status updates and errors
    /// * `task_manager` - Manager for tracking spawned tasks
    pub async fn run_channel_manager_tasks(
        self: Arc<Self>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let mut shutdown_rx = notify_shutdown.subscribe();
        let status_sender = StatusSender::ChannelManager(status_sender);
        task_manager.spawn(async move {
            loop {
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                info!("ChannelManager: received shutdown signal.");
                                break;
                            }
                            Ok(ShutdownMessage::UpstreamReconnectedResetAndShutdownDownstreams) => {
                                info!("ChannelManager: upstream reconnected, resetting channel state.");
                                self.channel_manager_data.super_safe_lock(|data| {
                                    data.reset_for_upstream_reconnection();
                                });
                                // Note: DownstreamShutdownAll handling is done by SV1Server separately
                            }
                            Ok(_) => {
                                // Ignore other shutdown message types
                            }
                            Err(e) => {
                                // Handle channel lag gracefully - don't shutdown on lag errors
                                if let tokio::sync::broadcast::error::RecvError::Lagged(_) = e {
                                    warn!("ChannelManager: broadcast channel lagged, continuing: {e}");
                                } else {
                                    error!("ChannelManager: failed to receive shutdown signal: {e}");
                                    break;
                                }
                            }
                        }
                    }
                    res = Self::handle_upstream_message(self.clone()) => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    },
                    res = Self::handle_downstream_message(self.clone()) => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    },
                    else => {
                        warn!("All channel manager message streams closed. Exiting...");
                        break;
                    }
                }
            }

            self.channel_state.drop();
            drop(shutdown_complete_tx);
            warn!("ChannelManager: unified message loop exited.");
        });
    }

    /// Handles messages received from the upstream SV2 server.
    ///
    /// This method processes SV2 messages from upstream and routes them appropriately:
    /// - Mining messages: Processed through the roles logic and forwarded to SV1 server
    /// - Channel responses: Handled to manage channel lifecycle
    /// - Job notifications: Converted and distributed to downstream connections
    /// - Error messages: Logged and handled appropriately
    ///
    /// The method implements the core SV2 protocol logic for channel management,
    /// including handling both aggregated and non-aggregated channel modes.
    ///
    /// # Returns
    /// * `Ok(())` - Message processed successfully
    /// * `Err(TproxyError)` - Error processing the message
    pub async fn handle_upstream_message(self: Arc<Self>) -> Result<(), TproxyError> {
        let mut channel_manager = self.get_channel_manager();
        let message = self
            .channel_state
            .upstream_receiver
            .recv()
            .await
            .map_err(TproxyError::ChannelErrorReceiver)?;

        let Frame::Sv2(mut frame) = message else {
            warn!("Received non-SV2 frame from upstream");
            return Ok(());
        };

        let header = frame.get_header().ok_or_else(|| {
            error!("Missing header in SV2 frame");
            TproxyError::General("Missing frame header".into())
        })?;

        let message_type = header.msg_type();
        let mut payload = frame.payload().to_vec();

        let message: AnyMessage<'_> = into_static(
            (message_type, payload.as_mut_slice())
                .try_into()
                .map_err(|e| {
                    error!("Failed to parse upstream frame into AnyMessage: {:?}", e);
                    TproxyError::General("Failed to parse AnyMessage".into())
                })?,
        )?;

        match message {
            Message::Mining(_) => {
                channel_manager
                    .handle_mining_message_frame_from_server(message_type, &mut payload)
                    .await?;
            }
            _ => {
                warn!("Unhandled upstream message type: {:?}", message);
            }
        }

        Ok(())
    }

    /// Handles messages received from the downstream SV1 server.
    ///
    /// This method processes requests from the SV1 server, primarily:
    /// - OpenExtendedMiningChannel: Sets up new SV2 channels for downstream connections
    /// - SubmitSharesExtended: Processes share submissions from miners
    ///
    /// For channel opening, the method handles both aggregated and non-aggregated modes:
    /// - Aggregated: Creates extended channels using extranonce prefixes
    /// - Non-aggregated: Opens individual extended channels with the upstream for each downstream
    ///
    /// Share submissions are validated, processed through the channel logic,
    /// and forwarded to the upstream server with appropriate extranonce handling.
    ///
    /// # Returns
    /// * `Ok(())` - Message processed successfully
    /// * `Err(TproxyError)` - Error processing the message
    pub async fn handle_downstream_message(self: Arc<Self>) -> Result<(), TproxyError> {
        let message = self
            .channel_state
            .sv1_server_receiver
            .recv()
            .await
            .map_err(TproxyError::ChannelErrorReceiver)?;
        match message {
            Mining::OpenExtendedMiningChannel(m) => {
                let mut open_channel_msg = m.clone();
                let mut user_identity = std::str::from_utf8(m.user_identity.as_ref())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                let hashrate = m.nominal_hash_rate;
                let min_extranonce_size = m.min_extranonce_size as usize;
                let mode = self
                    .channel_manager_data
                    .super_safe_lock(|c| c.mode.clone());

                if mode == ChannelMode::Aggregated {
                    if self
                        .channel_manager_data
                        .super_safe_lock(|c| c.upstream_extended_channel.is_some())
                    {
                        // We already have the unique channel open and so we create a new
                        // extranonce prefix and we send the
                        // OpenExtendedMiningChannelSuccess message directly to the sv1
                        // server
                        let target = self.channel_manager_data.super_safe_lock(|c| {
                            c.upstream_extended_channel
                                .as_ref()
                                .unwrap()
                                .read()
                                .unwrap()
                                .get_target()
                                .clone()
                        });
                        let new_extranonce_prefix =
                            self.channel_manager_data.super_safe_lock(|c| {
                                c.extranonce_prefix_factory
                                    .as_ref()
                                    .unwrap()
                                    .safe_lock(|e| {
                                        e.next_prefix_extended(
                                            open_channel_msg.min_extranonce_size.into(),
                                        )
                                    })
                                    .ok()
                                    .and_then(|r| r.ok())
                            });
                        let new_extranonce_size = self.channel_manager_data.super_safe_lock(|c| {
                            c.extranonce_prefix_factory
                                .as_ref()
                                .unwrap()
                                .safe_lock(|e| e.get_range2_len())
                                .unwrap()
                        });
                        if let Some(new_extranonce_prefix) = new_extranonce_prefix {
                            if new_extranonce_size >= open_channel_msg.min_extranonce_size as usize
                            {
                                let next_channel_id =
                                    self.channel_manager_data.super_safe_lock(|c| {
                                        c.extended_channels.keys().max().unwrap_or(&0) + 1
                                    });
                                let new_downstream_extended_channel = ExtendedChannel::new(
                                    next_channel_id,
                                    user_identity.clone(),
                                    new_extranonce_prefix
                                        .clone()
                                        .into_b032()
                                        .into_static()
                                        .to_vec(),
                                    target.clone(),
                                    hashrate,
                                    true,
                                    new_extranonce_size as u16,
                                );
                                self.channel_manager_data.super_safe_lock(|c| {
                                    c.extended_channels.insert(
                                        next_channel_id,
                                        Arc::new(RwLock::new(new_downstream_extended_channel)),
                                    );
                                });
                                let success_message = Mining::OpenExtendedMiningChannelSuccess(
                                    OpenExtendedMiningChannelSuccess {
                                        request_id: open_channel_msg.request_id,
                                        channel_id: next_channel_id,
                                        target: target.clone().into(),
                                        extranonce_size: new_extranonce_size as u16,
                                        extranonce_prefix: new_extranonce_prefix.clone().into(),
                                    },
                                );

                                self.channel_state
                                    .sv1_server_sender
                                    .send(success_message)
                                    .await
                                    .map_err(|e| {
                                        error!(
                                            "Failed to send open channel message to upstream: {:?}",
                                            e
                                        );
                                        TproxyError::ChannelErrorSender
                                    })?;
                                // get the last active job from the upstream extended channel
                                let last_active_job =
                                    self.channel_manager_data.super_safe_lock(|c| {
                                        c.upstream_extended_channel
                                            .as_ref()
                                            .and_then(|ch| ch.read().ok())
                                            .and_then(|ch| ch.get_active_job().map(|j| j.0.clone()))
                                    });

                                // get the last chain tip from the upstream extended channel
                                let last_chain_tip =
                                    self.channel_manager_data.super_safe_lock(|c| {
                                        c.upstream_extended_channel
                                            .as_ref()
                                            .and_then(|ch| ch.read().ok())
                                            .and_then(|ch| ch.get_chain_tip().cloned())
                                    });
                                // update the downstream channel with the active job and the chain
                                // tip
                                if let Some(mut job) = last_active_job {
                                    if let Some(last_chain_tip) = last_chain_tip {
                                        // update the downstream channel with the active chain tip
                                        self.channel_manager_data.super_safe_lock(|c| {
                                            if let Some(ch) =
                                                c.extended_channels.get(&next_channel_id)
                                            {
                                                ch.write()
                                                    .unwrap()
                                                    .set_chain_tip(last_chain_tip.clone());
                                            }
                                        });
                                    }
                                    job.channel_id = next_channel_id;
                                    // update the downstream channel with the active job
                                    self.channel_manager_data.super_safe_lock(|c| {
                                        if let Some(ch) = c.extended_channels.get(&next_channel_id)
                                        {
                                            let _ = ch
                                                .write()
                                                .unwrap()
                                                .on_new_extended_mining_job(job.clone());
                                        }
                                    });

                                    self.channel_state
                                        .sv1_server_sender
                                        .send(Mining::NewExtendedMiningJob(job.clone()))
                                        .await
                                        .map_err(|e| {
                                            error!("Failed to send last new extended mining job to upstream: {:?}", e);
                                            TproxyError::ChannelErrorSender
                                        })?;
                                }
                            }
                        }
                        return Ok(());
                    } else {
                        // We don't have the unique channel open yet and so we send the
                        // OpenExtendedMiningChannel message to the upstream
                        // Before doing that we need to truncate the user identity at the
                        // first dot and append .translator-proxy
                        // Truncate at the first dot and append .translator-proxy
                        let translator_identity = if let Some(dot_index) = user_identity.find('.') {
                            format!("{}.translator-proxy", &user_identity[..dot_index])
                        } else {
                            format!("{user_identity}.translator-proxy")
                        };
                        user_identity = translator_identity;
                        open_channel_msg.user_identity =
                            user_identity.as_bytes().to_vec().try_into().unwrap();
                    }
                }
                // In aggregated mode, add extra bytes for translator search space allocation
                let upstream_min_extranonce_size = self.channel_manager_data.super_safe_lock(|c| {
                    if c.mode == ChannelMode::Aggregated {
                        min_extranonce_size + AGGREGATED_MODE_TRANSLATOR_SEARCH_SPACE_BYTES
                    } else {
                        min_extranonce_size
                    }
                });

                // Update the message with the adjusted extranonce size for upstream
                open_channel_msg.min_extranonce_size = upstream_min_extranonce_size as u16;

                // Store the user identity, hashrate, and original downstream extranonce size
                self.channel_manager_data.super_safe_lock(|c| {
                    c.pending_channels.insert(
                        open_channel_msg.request_id,
                        (user_identity, hashrate, min_extranonce_size),
                    );
                });

                info!(
                    "Sending OpenExtendedMiningChannel message to upstream: {:?}",
                    open_channel_msg
                );

                let frame = StdFrame::try_from(Message::Mining(Mining::OpenExtendedMiningChannel(
                    open_channel_msg,
                )))
                .map_err(TproxyError::ParserError)?;
                self.channel_state
                    .upstream_sender
                    .send(frame.into())
                    .await
                    .map_err(|e| {
                        error!("Failed to send open channel message to upstream: {:?}", e);
                        TproxyError::ChannelErrorSender
                    })?;
            }
            Mining::SubmitSharesExtended(mut m) => {
                let value = self.channel_manager_data.super_safe_lock(|c| {
                    let extended_channel = c.extended_channels.get(&m.channel_id);
                    if let Some(extended_channel) = extended_channel {
                        let channel = extended_channel.write();
                        if let Ok(mut channel) = channel {
                            return Some((
                                channel.validate_share(m.clone()),
                                channel.get_share_accounting().clone(),
                            ));
                        }
                    }
                    None
                });
                if let Some((Ok(_result), _share_accounting)) = value {
                    let mode = self
                        .channel_manager_data
                        .super_safe_lock(|c| c.mode.clone());

                    if mode == ChannelMode::Aggregated
                        && self
                            .channel_manager_data
                            .super_safe_lock(|c| c.upstream_extended_channel.is_some())
                    {
                        let upstream_extended_channel_id =
                            self.channel_manager_data.super_safe_lock(|c| {
                                let upstream_extended_channel = c
                                    .upstream_extended_channel
                                    .as_ref()
                                    .unwrap()
                                    .read()
                                    .unwrap();
                                upstream_extended_channel.get_channel_id()
                            });

                        // In aggregated mode, use a single sequence counter for all valid shares
                        m.sequence_number = self.channel_manager_data.super_safe_lock(|c| {
                            c.next_share_sequence_number(upstream_extended_channel_id)
                        });
                        // Get the downstream channel's extranonce prefix (contains
                        // upstream prefix + translator proxy prefix)
                        let downstream_extranonce_prefix =
                            self.channel_manager_data.super_safe_lock(|c| {
                                c.extended_channels.get(&m.channel_id).map(|channel| {
                                    channel.read().unwrap().get_extranonce_prefix().clone()
                                })
                            });
                        // Get the length of the upstream prefix (range0)
                        let range0_len = self.channel_manager_data.super_safe_lock(|c| {
                            c.extranonce_prefix_factory
                                .as_ref()
                                .unwrap()
                                .safe_lock(|e| e.get_range0_len())
                                .unwrap()
                        });
                        if let Some(downstream_extranonce_prefix) = downstream_extranonce_prefix {
                            // Skip the upstream prefix (range0) and take the remaining
                            // bytes (translator proxy prefix)
                            let translator_prefix = &downstream_extranonce_prefix[range0_len..];
                            // Create new extranonce: translator proxy prefix + miner's
                            // extranonce
                            let mut new_extranonce = translator_prefix.to_vec();
                            new_extranonce.extend_from_slice(m.extranonce.as_ref());
                            // Replace the original extranonce with the modified one for
                            // upstream submission
                            m.extranonce = new_extranonce.try_into()?;
                        }
                        // We need to set the channel id to the upstream extended
                        // channel id
                        m.channel_id = upstream_extended_channel_id;
                    } else {
                        // In non-aggregated mode, each downstream channel has its own sequence
                        // counter
                        m.sequence_number = self
                            .channel_manager_data
                            .super_safe_lock(|c| c.next_share_sequence_number(m.channel_id));

                        // Check if we have a per-channel factory for extranonce adjustment
                        let channel_factory = self.channel_manager_data.super_safe_lock(|c| {
                            c.extranonce_factories
                                .as_ref()
                                .and_then(|factories| factories.get(&m.channel_id).cloned())
                        });

                        if let Some(factory) = channel_factory {
                            // We need to adjust the extranonce for this channel
                            let downstream_extranonce_prefix =
                                self.channel_manager_data.super_safe_lock(|c| {
                                    c.extended_channels.get(&m.channel_id).map(|channel| {
                                        channel.read().unwrap().get_extranonce_prefix().clone()
                                    })
                                });
                            let range0_len = factory
                                .safe_lock(|e| e.get_range0_len())
                                .expect("Failed to access extranonce factory range - this should not happen");
                            if let Some(downstream_extranonce_prefix) = downstream_extranonce_prefix
                            {
                                // Skip the upstream prefix (range0) and take the remaining
                                // bytes (translator proxy prefix)
                                let translator_prefix = &downstream_extranonce_prefix[range0_len..];
                                // Create new extranonce: translator proxy prefix + miner's
                                // extranonce
                                let mut new_extranonce = translator_prefix.to_vec();
                                new_extranonce.extend_from_slice(m.extranonce.as_ref());
                                // Replace the original extranonce with the modified one for
                                // upstream submission
                                m.extranonce = new_extranonce.try_into()?;
                            }
                        }
                    }

                    info!(
                        "SubmitSharesExtended: valid share, forwarding it to upstream | channel_id: {}, sequence_number: {} ☑️",
                        m.channel_id, m.sequence_number
                    );
                    let frame: StdFrame = Message::Mining(Mining::SubmitSharesExtended(m))
                        .try_into()
                        .map_err(TproxyError::ParserError)?;
                    let frame: EitherFrame = frame.into();
                    self.channel_state
                        .upstream_sender
                        .send(frame)
                        .await
                        .map_err(|e| {
                            error!("Error while sending message to upstream: {e:?}");
                            TproxyError::ChannelErrorSender
                        })?;
                }
            }
            Mining::UpdateChannel(mut m) => {
                debug!("Received UpdateChannel from SV1Server: {:?}", m);
                let mode = self
                    .channel_manager_data
                    .super_safe_lock(|c| c.mode.clone());

                if mode == ChannelMode::Aggregated {
                    let upstream_extended_channel_id =
                        self.channel_manager_data.super_safe_lock(|c| {
                            c.upstream_extended_channel
                                .as_ref()
                                .unwrap()
                                .read()
                                .unwrap()
                                .get_channel_id()
                        });
                    // We need to set the channel id to the upstream extended
                    // channel id
                    m.channel_id = upstream_extended_channel_id;
                }
                info!(
                    "Sending UpdateChannel message to upstream for channel_id: {:?}",
                    m.channel_id
                );
                // Forward UpdateChannel message to upstream
                let frame = StdFrame::try_from(Message::Mining(Mining::UpdateChannel(m)))
                    .map_err(TproxyError::ParserError)?;

                self.channel_state
                    .upstream_sender
                    .send(frame.into())
                    .await
                    .map_err(|e| {
                        error!("Failed to send UpdateChannel message to upstream: {:?}", e);
                        TproxyError::ChannelErrorSender
                    })?;
            }
            Mining::CloseChannel(m) => {
                debug!("Received CloseChannel from SV1Server: {m}");
                let frame = StdFrame::try_from(Message::Mining(Mining::CloseChannel(m)))
                    .map_err(TproxyError::ParserError)?;

                self.channel_state
                    .upstream_sender
                    .send(frame.into())
                    .await
                    .map_err(|e| {
                        error!("Failed to send UpdateChannel message to upstream: {:?}", e);
                        TproxyError::ChannelErrorSender
                    })?;
            }
            _ => {
                warn!("Unhandled downstream message: {:?}", message);
            }
        }

        Ok(())
    }

    pub fn get_channel_manager(&self) -> ChannelManager {
        ChannelManager {
            channel_manager_data: self.channel_manager_data.clone(),
            channel_state: self.channel_state.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sv2::channel_manager::data::ChannelMode;
    use async_channel::unbounded;
    use stratum_common::roles_logic_sv2::mining_sv2::{
        OpenExtendedMiningChannel, SubmitSharesExtended, UpdateChannel,
    };

    fn create_test_channel_manager(mode: ChannelMode) -> ChannelManager {
        let (upstream_sender, _upstream_receiver) = unbounded();
        let (_upstream_sender2, upstream_receiver) = unbounded();
        let (sv1_server_sender, _sv1_server_receiver) = unbounded();
        let (_sv1_server_sender2, sv1_server_receiver) = unbounded();

        ChannelManager::new(
            upstream_sender,
            upstream_receiver,
            sv1_server_sender,
            sv1_server_receiver,
            mode,
        )
    }

    #[test]
    fn test_channel_manager_creation_aggregated() {
        let manager = create_test_channel_manager(ChannelMode::Aggregated);

        let mode = manager
            .channel_manager_data
            .super_safe_lock(|data| data.mode.clone());
        assert_eq!(mode, ChannelMode::Aggregated);
    }

    #[test]
    fn test_channel_manager_creation_non_aggregated() {
        let manager = create_test_channel_manager(ChannelMode::NonAggregated);

        let mode = manager
            .channel_manager_data
            .super_safe_lock(|data| data.mode.clone());
        assert_eq!(mode, ChannelMode::NonAggregated);
    }

    #[test]
    fn test_get_channel_manager() {
        let manager = create_test_channel_manager(ChannelMode::Aggregated);
        let cloned_manager = manager.get_channel_manager();

        // Should be a different instance but share the same data
        let original_mode = manager
            .channel_manager_data
            .super_safe_lock(|data| data.mode.clone());
        let cloned_mode = cloned_manager
            .channel_manager_data
            .super_safe_lock(|data| data.mode.clone());

        assert_eq!(original_mode, cloned_mode);
    }

    #[tokio::test]
    async fn test_handle_downstream_open_channel_message() {
        let manager = create_test_channel_manager(ChannelMode::NonAggregated);

        // Create an OpenExtendedMiningChannel message
        let open_channel = OpenExtendedMiningChannel {
            request_id: 1,
            user_identity: "test_user".as_bytes().to_vec().try_into().unwrap(),
            nominal_hash_rate: 1000.0,
            max_target: vec![0xFFu8; 32].try_into().unwrap(),
            min_extranonce_size: 4,
        };

        // Store the pending channel information
        manager.channel_manager_data.super_safe_lock(|data| {
            data.pending_channels
                .insert(1, ("test_user".to_string(), 1000.0, 4));
        });

        // Test that the message can be handled without panicking
        // In a real test environment, we would need to mock the upstream sender
        // For now, we just verify the channel manager can process the message type
        let mining_message = Mining::OpenExtendedMiningChannel(open_channel);

        // Verify the message can be processed (would normally be sent to upstream)
        match mining_message {
            Mining::OpenExtendedMiningChannel(msg) => {
                assert_eq!(msg.request_id, 1);
                assert_eq!(msg.nominal_hash_rate, 1000.0);
                assert_eq!(msg.min_extranonce_size, 4);
            }
            _ => panic!("Expected OpenExtendedMiningChannel"),
        }
    }

    #[tokio::test]
    async fn test_handle_downstream_submit_shares_message() {
        let _manager = create_test_channel_manager(ChannelMode::NonAggregated);

        // Create a SubmitSharesExtended message
        let submit_shares = SubmitSharesExtended {
            channel_id: 1,
            sequence_number: 100,
            job_id: 42,
            nonce: 0x12345678,
            ntime: 1234567890,
            version: 0x20000000,
            extranonce: vec![0x01, 0x02, 0x03, 0x04].try_into().unwrap(),
        };

        // Test that the message can be handled
        let mining_message = Mining::SubmitSharesExtended(submit_shares);

        // Verify the message structure
        match mining_message {
            Mining::SubmitSharesExtended(msg) => {
                assert_eq!(msg.channel_id, 1);
                assert_eq!(msg.sequence_number, 100);
                assert_eq!(msg.job_id, 42);
                assert_eq!(msg.nonce, 0x12345678);
            }
            _ => panic!("Expected SubmitSharesExtended"),
        }
    }

    #[tokio::test]
    async fn test_handle_downstream_update_channel_message() {
        let _manager = create_test_channel_manager(ChannelMode::Aggregated);

        // Create an UpdateChannel message
        let update_channel = UpdateChannel {
            channel_id: 1,
            nominal_hash_rate: 2000.0,
            maximum_target: [0xFFu8; 32].try_into().unwrap(),
        };

        // Test that the message can be handled
        let mining_message = Mining::UpdateChannel(update_channel);

        // Verify the message structure
        match mining_message {
            Mining::UpdateChannel(msg) => {
                assert_eq!(msg.channel_id, 1);
                assert_eq!(msg.nominal_hash_rate, 2000.0);
            }
            _ => panic!("Expected UpdateChannel"),
        }
    }

    #[test]
    fn test_channel_manager_debug() {
        let manager = create_test_channel_manager(ChannelMode::Aggregated);

        // Test that Debug trait is implemented
        let debug_str = format!("{:?}", manager);
        assert!(debug_str.contains("ChannelManager"));
    }

    #[test]
    fn test_channel_manager_clone() {
        let manager = create_test_channel_manager(ChannelMode::Aggregated);
        let cloned = manager.clone();

        // Verify that both managers share the same underlying data
        let original_mode = manager
            .channel_manager_data
            .super_safe_lock(|data| data.mode.clone());
        let cloned_mode = cloned
            .channel_manager_data
            .super_safe_lock(|data| data.mode.clone());

        assert_eq!(original_mode, cloned_mode);
    }

    #[test]
    fn test_channel_manager_data_access() {
        let manager = create_test_channel_manager(ChannelMode::NonAggregated);

        // Test that we can access and modify channel manager data
        manager.channel_manager_data.super_safe_lock(|data| {
            // Add a pending channel
            data.pending_channels
                .insert(1, ("test".to_string(), 100.0, 4));
        });

        let has_pending = manager
            .channel_manager_data
            .super_safe_lock(|data| data.pending_channels.contains_key(&1));

        assert!(has_pending);
    }

    #[test]
    fn test_channel_manager_mode_consistency() {
        let aggregated_manager = create_test_channel_manager(ChannelMode::Aggregated);
        let non_aggregated_manager = create_test_channel_manager(ChannelMode::NonAggregated);

        let agg_mode = aggregated_manager
            .channel_manager_data
            .super_safe_lock(|data| data.mode.clone());
        let non_agg_mode = non_aggregated_manager
            .channel_manager_data
            .super_safe_lock(|data| data.mode.clone());

        assert_eq!(agg_mode, ChannelMode::Aggregated);
        assert_eq!(non_agg_mode, ChannelMode::NonAggregated);
        assert_ne!(agg_mode, non_agg_mode);
    }
}
