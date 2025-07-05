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
use codec_sv2::Frame;
use roles_logic_sv2::{
    channels::client::extended::ExtendedChannel,
    handlers::mining::{ParseMiningMessagesFromUpstream, SendTo},
    mining_sv2::OpenExtendedMiningChannelSuccess,
    parsers::{AnyMessage, Mining},
    utils::Mutex,
};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

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
    channel_state: ChannelState,
    channel_manager_data: Arc<Mutex<ChannelManagerData>>,
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
        info!("Spawning run channel manager task");
        let status_sender = StatusSender::ChannelManager(status_sender);
        task_manager.spawn(async move {
            loop {
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        if let Ok(ShutdownMessage::ShutdownAll) = message {
                            info!("ChannelManager: received shutdown signal.");
                            break;
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
                let result = ParseMiningMessagesFromUpstream::handle_message_mining(
                    self.channel_manager_data.clone(),
                    message_type,
                    payload.as_mut_slice(),
                );

                let send_to = match result {
                    Ok(send_to) => send_to,
                    Err(e) => {
                        error!("Failed to handle mining message: {:?}", e);
                        return Err(TproxyError::RolesSv2LogicError(e));
                    }
                };

                match send_to {
                    SendTo::Respond(response) => {
                        let msg = Message::Mining(response);
                        let frame: EitherFrame = StdFrame::try_from(msg)
                            .map_err(|e| TproxyError::General(format!("Failed to frame: {e}")))?
                            .into();

                        self.channel_state
                            .upstream_sender
                            .send(frame)
                            .await
                            .map_err(|e| {
                                error!("Failed to send response upstream: {:?}", e);
                                TproxyError::ChannelErrorSender
                            })?;
                    }

                    SendTo::None(Some(mining_msg)) => {
                        use Mining::*;

                        match mining_msg {
                            SetNewPrevHash(prev_hash) => {
                                self.channel_state
                                    .sv1_server_sender
                                    .send(SetNewPrevHash(prev_hash.clone()))
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
                                            .get(&prev_hash.channel_id)
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
                                        .send(NewExtendedMiningJob(job))
                                        .await
                                        .map_err(|e| {
                                            error!("Failed to send NewExtendedMiningJob: {:?}", e);
                                            TproxyError::ChannelErrorSender
                                        })?;
                                }
                            }

                            NewExtendedMiningJob(job) => {
                                if !job.is_future() {
                                    self.channel_state
                                        .sv1_server_sender
                                        .send(NewExtendedMiningJob(job))
                                        .await
                                        .map_err(|e| {
                                            error!("Failed to send immediate NewExtendedMiningJob: {:?}", e);
                                            TproxyError::ChannelErrorSender
                                        })?;
                                }
                            }

                            OpenExtendedMiningChannelSuccess(success) => {
                                self.channel_state
                                    .sv1_server_sender
                                    .send(OpenExtendedMiningChannelSuccess(success.clone()))
                                    .await
                                    .map_err(|e| {
                                        error!(
                                            "Failed to send OpenExtendedMiningChannelSuccess: {:?}",
                                            e
                                        );
                                        TproxyError::ChannelErrorSender
                                    })?;
                            }

                            OpenMiningChannelError(_) => {
                                // TODO: Implement proper handler
                                todo!("OpenMiningChannelError not handled yet");
                            }

                            _ => {
                                // Unsupported mining message type
                                unreachable!("Unexpected mining message variant received");
                            }
                        }
                    }

                    _ => {
                        // No action needed
                    }
                }
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
                                // send the last active job to the sv1 server
                                let last_active_job =
                                    self.channel_manager_data.super_safe_lock(|c| {
                                        c.upstream_extended_channel
                                            .as_ref()
                                            .and_then(|ch| ch.read().ok())
                                            .and_then(|ch| ch.get_active_job().map(|j| j.0.clone()))
                                    });

                                if let Some(mut job) = last_active_job {
                                    job.channel_id = next_channel_id;
                                    self.channel_manager_data.super_safe_lock(|c| {
                                        if let Some(ch) = c.extended_channels.get(&next_channel_id)
                                        {
                                            ch.write()
                                                .unwrap()
                                                .on_new_extended_mining_job(job.clone());
                                        }
                                    });
                                    // this is done to make sure that the job is sent after the
                                    // the downstream is ready to receive the job (subscribed to the
                                    // broadcast receiver of the sv1 server)
                                    tokio::time::sleep(Duration::from_secs(3)).await;
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
                            format!("{}.translator-proxy", user_identity)
                        };
                        user_identity = translator_identity;
                        open_channel_msg.user_identity =
                            user_identity.as_bytes().to_vec().try_into().unwrap();
                    }
                }
                // Store the user identity and hashrate
                self.channel_manager_data.super_safe_lock(|c| {
                    c.pending_channels.insert(
                        open_channel_msg.request_id,
                        (user_identity, hashrate, min_extranonce_size),
                    );
                });

                let frame = StdFrame::try_from(Message::Mining(
                    roles_logic_sv2::parsers::Mining::OpenExtendedMiningChannel(open_channel_msg),
                ))
                .map_err(TproxyError::RolesSv2LogicError)?;
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
                        m.channel_id = upstream_extended_channel_id; // We need to set the channel id to the upstream extended
                                                                     // channel id
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
                    }
                    let frame: StdFrame = Message::Mining(Mining::SubmitSharesExtended(m))
                        .try_into()
                        .map_err(TproxyError::RolesSv2LogicError)?;
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
            _ => {}
        }

        Ok(())
    }
}
