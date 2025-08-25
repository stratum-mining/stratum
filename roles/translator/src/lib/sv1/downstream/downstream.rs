use super::DownstreamMessages;
use crate::{
    error::TproxyError,
    status::{handle_error, StatusSender},
    sv1::{
        downstream::{channel::DownstreamChannelState, data::DownstreamData},
        sv1_server::data::Sv1ServerData,
    },
    task_manager::TaskManager,
    utils::ShutdownMessage,
};
use async_channel::{Receiver, Sender};
use roles_logic_sv2::{mining_sv2::Target, utils::Mutex};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};
use v1::{
    json_rpc::{self, Message},
    server_to_client, IsServer,
};

/// Represents a downstream SV1 miner connection.
///
/// This struct manages the state and communication for a single SV1 miner connected
/// to the translator. It handles:
/// - SV1 protocol message processing (subscribe, authorize, submit)
/// - Bidirectional message routing between miner and SV1 server
/// - Mining job tracking and share validation
/// - Difficulty adjustment coordination
/// - Connection lifecycle management
///
/// Each downstream connection runs in its own async task that processes messages
/// from both the miner and the server, ensuring proper message ordering and
/// handling connection-specific state.
#[derive(Debug)]
pub struct Downstream {
    pub downstream_data: Arc<Mutex<DownstreamData>>,
    downstream_channel_state: DownstreamChannelState,
}

impl Downstream {
    /// Creates a new downstream connection instance.
    ///
    /// # Arguments
    /// * `downstream_id` - Unique identifier for this downstream connection
    /// * `downstream_sv1_sender` - Channel to send messages to the miner
    /// * `downstream_sv1_receiver` - Channel to receive messages from the miner
    /// * `sv1_server_sender` - Channel to send messages to the SV1 server
    /// * `sv1_server_receiver` - Broadcast channel to receive messages from the SV1 server
    /// * `target` - Initial difficulty target for this connection
    /// * `hashrate` - Initial hashrate estimate for this connection
    /// * `sv1_server_data` - Reference to shared SV1 server data for job access
    ///
    /// # Returns
    /// A new Downstream instance ready to handle miner communication
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        downstream_id: u32,
        downstream_sv1_sender: Sender<json_rpc::Message>,
        downstream_sv1_receiver: Receiver<json_rpc::Message>,
        sv1_server_sender: Sender<DownstreamMessages>,
        sv1_server_receiver: broadcast::Receiver<(u32, Option<u32>, json_rpc::Message)>,
        target: Target,
        hashrate: Option<f32>,
        sv1_server_data: Arc<Mutex<Sv1ServerData>>,
    ) -> Self {
        let downstream_data = Arc::new(Mutex::new(DownstreamData::new(
            downstream_id,
            target,
            hashrate,
            sv1_server_data,
        )));
        let downstream_channel_state = DownstreamChannelState::new(
            downstream_sv1_sender,
            downstream_sv1_receiver,
            sv1_server_sender,
            sv1_server_receiver,
        );
        Self {
            downstream_data,
            downstream_channel_state,
        }
    }

    /// Spawns and runs the main task loop for this downstream connection.
    ///
    /// This method creates an async task that handles all communication for this
    /// downstream connection. The task runs a select loop that processes:
    /// - Shutdown signals (global, targeted, or all-downstream)
    /// - Messages from the miner (subscribe, authorize, submit)
    /// - Messages from the SV1 server (notify, set_difficulty, etc.)
    ///
    /// The task will continue running until a shutdown signal is received or
    /// an unrecoverable error occurs. It ensures graceful cleanup of resources
    /// and proper error reporting.
    ///
    /// # Arguments
    /// * `notify_shutdown` - Broadcast channel for receiving shutdown signals
    /// * `shutdown_complete_tx` - Channel to signal when shutdown is complete
    /// * `status_sender` - Channel for sending status updates and errors
    /// * `task_manager` - Manager for tracking spawned tasks
    pub fn run_downstream_tasks(
        self: Arc<Self>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: StatusSender,
        task_manager: Arc<TaskManager>,
    ) {
        let mut sv1_server_receiver = self
            .downstream_channel_state
            .sv1_server_receiver
            .resubscribe();
        let mut shutdown_rx = notify_shutdown.subscribe();
        let downstream_id = self.downstream_data.super_safe_lock(|d| d.downstream_id);
        task_manager.spawn(async move {
            loop {
                tokio::select! {
                    msg = shutdown_rx.recv() => {
                        match msg {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                info!("Downstream {downstream_id}: received global shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::DownstreamShutdown(id)) if id == downstream_id => {
                                info!("Downstream {downstream_id}: received targeted shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::DownstreamShutdownAll) => {
                                info!("All downstream shutdown message received");
                                break;
                            }
                            Ok(ShutdownMessage::UpstreamReconnectedResetAndShutdownDownstreams) => {
                                info!("All downstream shutdown message received (upstream reconnected)");
                                break;
                            }
                            Ok(_) => {
                                // shutdown for other downstream
                            }
                            Err(e) => {
                                warn!("Downstream {downstream_id}: shutdown channel closed: {e}");
                                break;
                            }
                        }
                    }

                    // Handle downstream -> server message
                    res = Self::handle_downstream_message(self.clone()) => {
                        if let Err(e) = res {
                            error!("Downstream {downstream_id}: error in downstream message handler: {e:?}");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }

                    // Handle server -> downstream message
                    res = Self::handle_sv1_server_message(self.clone(),&mut sv1_server_receiver) => {
                        if let Err(e) = res {
                            error!("Downstream {downstream_id}: error in server message handler: {e:?}");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }

                    else => {
                        warn!("Downstream {downstream_id}: all channels closed; exiting task");
                        break;
                    }
                }
            }

            warn!("Downstream {downstream_id}: unified task shutting down");
            self.downstream_channel_state.drop();
            drop(shutdown_complete_tx);
        });
    }

    /// Handles messages received from the SV1 server.
    ///
    /// This method processes messages broadcast from the SV1 server to downstream
    /// connections. Since `mining.notify` messages are guaranteed to never arrive
    /// before their corresponding `mining.set_difficulty` message, the logic is
    /// simplified to handle only handshake completion timing.
    ///
    /// Key behaviors:
    /// - Filters messages by channel ID and downstream ID
    /// - For `mining.set_difficulty`: Always caches the message (never sent immediately)
    /// - For `mining.notify`: Sends any pending set_difficulty first, then forwards the notify
    /// - For other messages: Forwards directly to the miner
    /// - Caches both `mining.set_difficulty` and `mining.notify` messages if handshake is not yet
    ///   complete
    /// - On handshake completion: sends cached messages in correct order (set_difficulty first,
    ///   then notify)
    ///
    /// # Arguments
    /// * `sv1_server_receiver` - Broadcast receiver for messages from the SV1 server
    ///
    /// # Returns
    /// * `Ok(())` - Message processed successfully
    /// * `Err(TproxyError)` - Error processing the message
    pub async fn handle_sv1_server_message(
        self: Arc<Self>,
        sv1_server_receiver: &mut broadcast::Receiver<(u32, Option<u32>, json_rpc::Message)>,
    ) -> Result<(), TproxyError> {
        match sv1_server_receiver.recv().await {
            Ok((channel_id, downstream_id, message)) => {
                let (my_channel_id, my_downstream_id, handshake_complete) = self
                    .downstream_data
                    .super_safe_lock(|d| (d.channel_id, d.downstream_id, d.sv1_handshake_complete));

                let id_matches = (my_channel_id == Some(channel_id) || channel_id == 0)
                    && (downstream_id.is_none() || downstream_id == Some(my_downstream_id));

                if !id_matches {
                    return Ok(()); // Message not intended for this downstream
                }

                // SV1 handshake complete - send messages immediately
                if handshake_complete {
                    if let Message::Notification(notification) = &message {
                        match notification.method.as_str() {
                            "mining.set_difficulty" => {
                                // Cache the set_difficulty message to be sent before the next
                                // notify
                                debug!("Down: Caching mining.set_difficulty to send before next mining.notify");
                                self.downstream_data.super_safe_lock(|d| {
                                    d.cached_set_difficulty = Some(message.clone());
                                });
                                return Ok(());
                            }
                            "mining.notify" => {
                                // Send any pending set_difficulty first
                                let pending_set_difficulty = self
                                    .downstream_data
                                    .super_safe_lock(|d| d.cached_set_difficulty.take());

                                if let Some(set_difficulty_msg) = &pending_set_difficulty {
                                    debug!("Down: Sending pending mining.set_difficulty before mining.notify");
                                    self.downstream_channel_state
                                        .downstream_sv1_sender
                                        .send(set_difficulty_msg.clone())
                                        .await
                                        .map_err(|e| {
                                            error!(
                                                "Down: Failed to send mining.set_difficulty to downstream: {:?}",
                                                e
                                            );
                                            TproxyError::ChannelErrorSender
                                        })?;

                                    self.downstream_data.super_safe_lock(|d| {
                                        if let Some(new_target) = d.pending_target.take() {
                                            d.target = new_target;
                                        }
                                        if let Some(new_hashrate) = d.pending_hashrate.take() {
                                            d.hashrate = Some(new_hashrate);
                                        }
                                    });
                                }

                                // Send the notify
                                if let Ok(mut notify) =
                                    server_to_client::Notify::try_from(notification.clone())
                                {
                                    if pending_set_difficulty.is_some() {
                                        notify.clean_jobs = true;
                                        debug!("Down: Sending mining.notify with clean_jobs=true after mining.set_difficulty");
                                    }

                                    self.downstream_data.super_safe_lock(|d| {
                                        d.last_job_version_field = Some(notify.version.0);
                                    });

                                    self.downstream_channel_state
                                        .downstream_sv1_sender
                                        .send(notify.into())
                                        .await
                                        .map_err(|e| {
                                            error!("Down: Failed to send mining.notify to downstream: {:?}", e);
                                            TproxyError::ChannelErrorSender
                                        })?;
                                }
                                return Ok(());
                            }
                            _ => {} // Not a special message, proceed below
                        }
                    }

                    // Default path: forward all other messages
                    self.downstream_channel_state
                        .downstream_sv1_sender
                        .send(message.clone())
                        .await
                        .map_err(|e| {
                            error!("Down: Failed to send message to downstream: {:?}", e);
                            TproxyError::ChannelErrorSender
                        })?;
                } else {
                    // Handshake not complete - cache only mining.set_difficulty and mining.notify
                    // messages
                    if let Message::Notification(notification) = &message {
                        match notification.method.as_str() {
                            "mining.set_difficulty" => {
                                debug!("Down: SV1 handshake not complete, caching mining.set_difficulty");
                                self.downstream_data.super_safe_lock(|d| {
                                    d.cached_set_difficulty = Some(message.clone());
                                });
                                return Ok(());
                            }
                            "mining.notify" => {
                                debug!("Down: SV1 handshake not complete, caching mining.notify");
                                self.downstream_data.super_safe_lock(|d| {
                                    d.cached_notify = Some(message.clone());
                                });
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                    debug!("Down: SV1 handshake not complete, skipping other message");
                }
            }
            Err(e) => {
                let downstream_id = self.downstream_data.super_safe_lock(|d| d.downstream_id);
                error!(
                    "Sv1 message handler error for downstream {}: {:?}",
                    downstream_id, e
                );
                return Err(TproxyError::BroadcastChannelErrorReceiver(e));
            }
        }

        Ok(())
    }

    /// Handles messages received from the downstream SV1 miner.
    ///
    /// This method processes SV1 protocol messages sent by the miner, including:
    /// - `mining.subscribe` - Subscription requests
    /// - `mining.authorize` - Authorization requests
    /// - `mining.submit` - Share submissions
    /// - Other SV1 protocol messages
    ///
    /// The method delegates message processing to the downstream data handler,
    /// which implements the SV1 protocol logic and generates appropriate responses.
    /// Responses are sent back to the miner, while share submissions are forwarded
    /// to the SV1 server for upstream processing.
    ///
    /// # Returns
    /// * `Ok(())` - Message processed successfully
    /// * `Err(TproxyError)` - Error receiving or processing the message
    pub async fn handle_downstream_message(self: Arc<Self>) -> Result<(), TproxyError> {
        let message = match self
            .downstream_channel_state
            .downstream_sv1_receiver
            .recv()
            .await
        {
            Ok(msg) => msg,
            Err(e) => {
                error!("Error receiving downstream message: {:?}", e);
                return Err(TproxyError::ChannelErrorReceiver(e));
            }
        };

        let response = self
            .downstream_data
            .super_safe_lock(|data| data.handle_message(message.clone()));

        match response {
            Ok(Some(response_msg)) => {
                if let Some(_channel_id) = self.downstream_data.super_safe_lock(|d| d.channel_id) {
                    self.downstream_channel_state
                        .downstream_sv1_sender
                        .send(response_msg.into())
                        .await
                        .map_err(|e| {
                            error!("Down: Failed to send message to downstream: {:?}", e);
                            TproxyError::ChannelErrorSender
                        })?;

                    // Check if this was an authorize message and handle handshake completion
                    if let v1::json_rpc::Message::StandardRequest(request) = &message {
                        if request.method == "mining.authorize" {
                            if let Err(e) = self.handle_sv1_handshake_completion().await {
                                error!("Down: Failed to handle handshake completion: {:?}", e);
                                return Err(e);
                            }
                        }
                    }
                }
            }
            Ok(None) => {
                // Message was handled but no response needed
            }
            Err(e) => {
                error!("Down: Error handling downstream message: {:?}", e);
                return Err(e.into());
            }
        }

        // Check if there's a pending share to send to the Sv1Server
        let pending_share = self
            .downstream_data
            .super_safe_lock(|d| d.pending_share.take());
        if let Some(share) = pending_share {
            if let Err(e) = self
                .downstream_channel_state
                .sv1_server_sender
                .try_send(DownstreamMessages::SubmitShares(share))
            {
                error!("Down: Failed to send share to SV1 server: {:?}", e);
            }
        }

        Ok(())
    }

    /// Handles SV1 handshake completion after mining.authorize.
    ///
    /// This method is called when the downstream completes the SV1 handshake
    /// (subscribe + authorize). It sends any cached messages in the correct order:
    /// set_difficulty first, then notify.
    ///
    /// # Returns
    /// * `Ok(())` - Handshake completion handled successfully
    /// * `Err(TproxyError)` - Error sending cached messages
    async fn handle_sv1_handshake_completion(self: &Arc<Self>) -> Result<(), TproxyError> {
        let (cached_set_difficulty, cached_notify) = self.downstream_data.super_safe_lock(|d| {
            d.sv1_handshake_complete = true;
            (d.cached_set_difficulty.take(), d.cached_notify.take())
        });
        debug!("Down: SV1 handshake completed for downstream");

        // Send cached messages in correct order: set_difficulty first, then notify
        if let Some(set_difficulty_msg) = cached_set_difficulty {
            debug!("Down: Sending cached mining.set_difficulty after handshake completion");
            self.downstream_channel_state
                .downstream_sv1_sender
                .send(set_difficulty_msg)
                .await
                .map_err(|e| {
                    error!(
                        "Down: Failed to send cached mining.set_difficulty to downstream: {:?}",
                        e
                    );
                    TproxyError::ChannelErrorSender
                })?;

            // Update target and hashrate after sending set_difficulty
            self.downstream_data.super_safe_lock(|d| {
                if let Some(new_target) = d.pending_target.take() {
                    d.target = new_target;
                }
                if let Some(new_hashrate) = d.pending_hashrate.take() {
                    d.hashrate = Some(new_hashrate);
                }
            });
        }

        if let Some(notify_msg) = cached_notify {
            debug!("Down: Sending cached mining.notify after handshake completion");
            self.downstream_channel_state
                .downstream_sv1_sender
                .send(notify_msg)
                .await
                .map_err(|e| {
                    error!(
                        "Down: Failed to send cached mining.notify to downstream: {:?}",
                        e
                    );
                    TproxyError::ChannelErrorSender
                })?;
        }

        Ok(())
    }
}
