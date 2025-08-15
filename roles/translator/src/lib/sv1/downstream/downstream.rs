use super::DownstreamMessages;
use crate::{
    error::TproxyError,
    status::{handle_error, StatusSender},
    sv1::downstream::{channel::DownstreamChannelState, data::DownstreamData},
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
    ///
    /// # Returns
    /// A new Downstream instance ready to handle miner communication
    pub fn new(
        downstream_id: u32,
        downstream_sv1_sender: Sender<json_rpc::Message>,
        downstream_sv1_receiver: Receiver<json_rpc::Message>,
        sv1_server_sender: Sender<DownstreamMessages>,
        sv1_server_receiver: broadcast::Receiver<(u32, Option<u32>, json_rpc::Message)>,
        target: Target,
        hashrate: f32,
    ) -> Self {
        let downstream_data = Arc::new(Mutex::new(DownstreamData::new(
            downstream_id,
            target,
            hashrate,
            sv1_server_sender.clone(),
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

        info!("Downstream {downstream_id}: spawning unified task");

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
    /// connections. It implements special logic to handle the timing issue where
    /// `mining.notify` messages might arrive before `mining.set_difficulty` messages.
    ///
    /// Key behaviors:
    /// - Filters messages by channel ID and downstream ID
    /// - For `mining.set_difficulty`: Updates target/hashrate and processes any waiting notify
    /// - For `mining.notify`: Ensures set_difficulty is sent first, handles first-notify timing
    /// - For other messages: Forwards directly to the miner
    ///
    /// The method ensures that miners always receive `set_difficulty` before `notify`
    /// for the first message pair, which prevents miners from being unable to start working.
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
                let (my_channel_id, my_downstream_id) = self
                    .downstream_data
                    .super_safe_lock(|d| (d.channel_id, d.downstream_id));

                let id_matches = (my_channel_id == Some(channel_id) || channel_id == 0)
                    && (downstream_id.is_none() || downstream_id == Some(my_downstream_id));

                if !id_matches {
                    return Ok(()); // Message not intended for this downstream
                }

                if let Message::Notification(notification) = &message {
                    match notification.method.as_str() {
                        "mining.set_difficulty" => {
                            self.downstream_data.super_safe_lock(|d| {
                                d.pending_set_difficulty = Some(message.clone());
                                d.first_set_difficulty_received = true;
                            });

                            // Check if we have a waiting first notify to process
                            let waiting_notify = self
                                .downstream_data
                                .super_safe_lock(|d| d.waiting_first_notify.take());

                            if let Some(notify_msg) = waiting_notify {
                                debug!("Down: Processing waiting first notify after receiving set_difficulty");
                                // Process the waiting notify message
                                if let Message::Notification(notify_notification) = &notify_msg {
                                    if let Ok(notify) = server_to_client::Notify::try_from(
                                        notify_notification.clone(),
                                    ) {
                                        // Send set_difficulty first
                                        if let Some(set_difficulty_msg) = self
                                            .downstream_data
                                            .super_safe_lock(|d| d.pending_set_difficulty.clone())
                                        {
                                            self.downstream_channel_state
                                                .downstream_sv1_sender
                                                .send(set_difficulty_msg)
                                                .await
                                                .map_err(|e| {
                                                    error!("Failed to send set_difficulty to downstream: {:?}", e);
                                                    TproxyError::ChannelErrorSender
                                                })?;

                                            self.downstream_data.super_safe_lock(|d| {
                                                if let Some(new_target) = d.pending_target.take() {
                                                    d.target = new_target;
                                                }
                                                if let Some(new_hashrate) =
                                                    d.pending_hashrate.take()
                                                {
                                                    d.hashrate = new_hashrate;
                                                }
                                                d.pending_set_difficulty = None;
                                            });
                                        }

                                        // Now send the notify
                                        self.downstream_data.super_safe_lock(|d| {
                                            d.last_job_version_field = Some(notify.version.0);
                                            if notify.clean_jobs {
                                                d.valid_jobs.clear();
                                            }
                                            d.valid_jobs.push(notify.clone());
                                        });

                                        self.downstream_channel_state
                                            .downstream_sv1_sender
                                            .send(notify.into())
                                            .await
                                            .map_err(|e| {
                                                error!(
                                                    "Failed to send notify to downstream: {:?}",
                                                    e
                                                );
                                                TproxyError::ChannelErrorSender
                                            })?;
                                    }
                                }
                            }
                            return Ok(()); // set_difficulty handled
                        }
                        "mining.notify" => {
                            debug!("Down: Received notify notification");
                            // If this is the first notify and we haven't received set_difficulty
                            // yet, store it and wait
                            let should_wait = self.downstream_data.super_safe_lock(|d| {
                                !d.first_set_difficulty_received && d.valid_jobs.is_empty()
                            });

                            if should_wait {
                                debug!("Down: First notify received before set_difficulty, storing and waiting...");
                                self.downstream_data.super_safe_lock(|d| {
                                    d.waiting_first_notify = Some(message.clone());
                                });
                                return Ok(()); // Store and wait for set_difficulty
                            }

                            let pending_set_difficulty = self
                                .downstream_data
                                .super_safe_lock(|d| d.pending_set_difficulty.clone());

                            if let Some(set_difficulty_msg) = &pending_set_difficulty {
                                debug!("Down: Sending pending set_difficulty before notify");
                                self.downstream_channel_state
                                    .downstream_sv1_sender
                                    .send(set_difficulty_msg.clone())
                                    .await
                                    .map_err(|e| {
                                        error!(
                                            "Failed to send set_difficulty to downstream: {:?}",
                                            e
                                        );
                                        TproxyError::ChannelErrorSender
                                    })?;

                                self.downstream_data.super_safe_lock(|d| {
                                    if let Some(new_target) = d.pending_target.take() {
                                        d.target = new_target;
                                    }
                                    if let Some(new_hashrate) = d.pending_hashrate.take() {
                                        d.hashrate = new_hashrate;
                                    }
                                    d.pending_set_difficulty = None;
                                    debug!(
                                        "Downstream {}: Updated target and hashrate after sending set_difficulty",
                                        d.downstream_id
                                    );
                                });
                            }

                            if let Ok(mut notify) =
                                server_to_client::Notify::try_from(notification.clone())
                            {
                                let original_clean_jobs = notify.clean_jobs;

                                if pending_set_difficulty.is_some() {
                                    notify.clean_jobs = true;
                                    debug!(
                                        "Down: Sending notify with clean_jobs=true after set_difficulty"
                                    );
                                }

                                self.downstream_data.super_safe_lock(|d| {
                                    d.last_job_version_field = Some(notify.version.0);
                                    if original_clean_jobs {
                                        d.valid_jobs.clear();
                                    }
                                    d.valid_jobs.push(notify.clone());
                                    debug!("Updated valid jobs: {:?}", d.valid_jobs);
                                });

                                self.downstream_channel_state
                                    .downstream_sv1_sender
                                    .send(notify.into())
                                    .await
                                    .map_err(|e| {
                                        error!("Failed to send notify to downstream: {:?}", e);
                                        TproxyError::ChannelErrorSender
                                    })?;

                                return Ok(()); // Notify handled, don't fall through
                            }
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
                        error!("Failed to send message to downstream: {:?}", e);
                        TproxyError::ChannelErrorSender
                    })?;
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
            .super_safe_lock(|data| data.handle_message(message));

        match response {
            Ok(Some(response_msg)) => {
                if let Some(_channel_id) = self.downstream_data.super_safe_lock(|d| d.channel_id) {
                    self.downstream_channel_state
                        .downstream_sv1_sender
                        .send(response_msg.into())
                        .await
                        .map_err(|e| {
                            error!("Failed to send message to downstream: {:?}", e);
                            TproxyError::ChannelErrorSender
                        })?;
                }
            }
            Ok(None) => {
                // Message was handled but no response needed
            }
            Err(e) => {
                error!("Error handling downstream message: {:?}", e);
                return Err(e.into());
            }
        }

        Ok(())
    }
}
