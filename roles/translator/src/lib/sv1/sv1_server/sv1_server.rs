use crate::{
    config::TranslatorConfig,
    error::TproxyError,
    status::{handle_error, Status, StatusSender},
    sv1::{
        downstream::{downstream::Downstream, DownstreamMessages},
        sv1_server::{
            channel::Sv1ServerChannelState, data::Sv1ServerData,
            difficulty_manager::DifficultyManager,
        },
    },
    task_manager::TaskManager,
    utils::ShutdownMessage,
};
use async_channel::{Receiver, Sender};
use network_helpers_sv2::{codec_sv2::binary_sv2::Str0255, sv1_connection::ConnectionSV1};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, RwLock,
    },
};
use stratum_common::roles_logic_sv2::{
    mining_sv2::{CloseChannel, SetTarget, Target},
    parsers_sv2::Mining,
    utils::{hash_rate_to_target, Mutex},
    vardiff::classic::VardiffState,
    Vardiff,
};
use stratum_translation::{
    sv1_to_sv2::{
        build_sv2_open_extended_mining_channel, build_sv2_submit_shares_extended_from_sv1_submit,
    },
    sv2_to_sv1::{build_sv1_notify_from_sv2, build_sv1_set_difficulty_from_sv2_target},
};
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc},
};
use tracing::{debug, error, info, warn};
use v1::IsServer;

/// SV1 server that handles connections from SV1 miners.
///
/// This struct manages the SV1 server component of the translator, which:
/// - Accepts connections from SV1 miners
/// - Manages difficulty adjustment for connected miners
/// - Coordinates with the SV2 channel manager for upstream communication
/// - Tracks mining jobs and share submissions
///
/// The server maintains state for multiple downstream connections and implements
/// variable difficulty adjustment based on share submission rates.
pub struct Sv1Server {
    sv1_server_channel_state: Sv1ServerChannelState,
    sv1_server_data: Arc<Mutex<Sv1ServerData>>,
    shares_per_minute: f32,
    listener_addr: SocketAddr,
    config: TranslatorConfig,
    clean_job: AtomicBool,
    sequence_counter: AtomicU32,
    miner_counter: AtomicU32,
}

impl Sv1Server {
    /// Drops the server's channel state, cleaning up resources.
    pub fn drop(&self) {
        self.sv1_server_channel_state.drop();
    }

    /// Creates a new SV1 server instance.
    ///
    /// # Arguments
    /// * `listener_addr` - The socket address to bind the server to
    /// * `channel_manager_receiver` - Channel to receive messages from the channel manager
    /// * `channel_manager_sender` - Channel to send messages to the channel manager
    /// * `config` - Configuration settings for the translator
    ///
    /// # Returns
    /// A new Sv1Server instance ready to accept connections
    pub fn new(
        listener_addr: SocketAddr,
        channel_manager_receiver: Receiver<Mining<'static>>,
        channel_manager_sender: Sender<Mining<'static>>,
        config: TranslatorConfig,
    ) -> Self {
        let shares_per_minute = config.downstream_difficulty_config.shares_per_minute;
        let sv1_server_channel_state =
            Sv1ServerChannelState::new(channel_manager_receiver, channel_manager_sender);
        let sv1_server_data = Arc::new(Mutex::new(Sv1ServerData::new(config.aggregate_channels)));
        Self {
            sv1_server_channel_state,
            sv1_server_data,
            config,
            listener_addr,
            shares_per_minute,
            clean_job: AtomicBool::new(true),
            miner_counter: AtomicU32::new(0),
            sequence_counter: AtomicU32::new(0),
        }
    }

    /// Starts the SV1 server and begins accepting connections.
    ///
    /// This method:
    /// - Binds to the configured listening address
    /// - Spawns the variable difficulty adjustment loop
    /// - Enters the main event loop to handle:
    ///   - New miner connections
    ///   - Shutdown signals
    ///   - Messages from downstream miners (submit shares)
    ///   - Messages from upstream SV2 channel manager
    ///
    /// The server will continue running until a shutdown signal is received.
    ///
    /// # Arguments
    /// * `notify_shutdown` - Broadcast channel for shutdown coordination
    /// * `shutdown_complete_tx` - Channel to signal shutdown completion
    /// * `status_sender` - Channel for sending status updates
    /// * `task_manager` - Manager for spawned async tasks
    ///
    /// # Returns
    /// * `Ok(())` - Server shut down gracefully
    /// * `Err(TproxyError)` - Server encountered an error
    pub async fn start(
        self: Arc<Self>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) -> Result<(), TproxyError> {
        info!("Starting SV1 server on {}", self.listener_addr);
        let mut shutdown_rx_main = notify_shutdown.subscribe();
        let shutdown_complete_tx_main_clone = shutdown_complete_tx.clone();

        // get the first target for the first set difficulty message
        let first_target: Target = hash_rate_to_target(
            self.config
                .downstream_difficulty_config
                .min_individual_miner_hashrate as f64,
            self.config.downstream_difficulty_config.shares_per_minute as f64,
        )
        .unwrap()
        .into();

        // Spawn vardiff loop only if enabled
        if self.config.downstream_difficulty_config.enable_vardiff {
            info!("Variable difficulty adjustment enabled - starting vardiff loop");
            task_manager.spawn(DifficultyManager::spawn_vardiff_loop(
                self.sv1_server_data.clone(),
                self.sv1_server_channel_state.channel_manager_sender.clone(),
                self.sv1_server_channel_state
                    .sv1_server_to_downstream_sender
                    .clone(),
                self.shares_per_minute,
                self.config.aggregate_channels,
                notify_shutdown.subscribe(),
                shutdown_complete_tx_main_clone.clone(),
            ));
        } else {
            info!("Variable difficulty adjustment disabled - upstream will manage difficulty, SV1 server will forward SetTarget messages to downstreams");
        }

        let listener = TcpListener::bind(self.listener_addr).await.map_err(|e| {
            error!("Failed to bind to {}: {}", self.listener_addr, e);
            e
        })?;

        info!("Translator Proxy: listening on {}", self.listener_addr);

        let sv1_status_sender = StatusSender::Sv1Server(status_sender.clone());

        loop {
            tokio::select! {
                message = shutdown_rx_main.recv() => {
                    match message {
                        Ok(ShutdownMessage::ShutdownAll) => {
                            debug!("SV1 Server: Vardiff loop received shutdown signal. Exiting.");
                            break;
                        }
                        Ok(ShutdownMessage::DownstreamShutdown(downstream_id)) => {
                            let current_downstream = self.sv1_server_data.super_safe_lock(|d| {
                                // Only remove from vardiff map if vardiff is enabled
                                if self.config.downstream_difficulty_config.enable_vardiff {
                                    d.vardiff.remove(&downstream_id);
                                }
                                d.downstreams.remove(&downstream_id)
                            });
                            if let Some(downstream) = current_downstream {
                                info!("🔌 Downstream: {downstream_id} disconnected and removed from sv1 server downstreams");

                                // In aggregated mode, send UpdateChannel to reflect the new state (only if vardiff enabled)
                                if self.config.downstream_difficulty_config.enable_vardiff {
                                    DifficultyManager::send_update_channel_on_downstream_state_change(
                                        &self.sv1_server_data,
                                        &self.sv1_server_channel_state.channel_manager_sender,
                                        self.config.aggregate_channels,
                                    ).await;
                                }

                                let channel_id = downstream.downstream_data.super_safe_lock(|d| d.channel_id);

                                if let Some(channel_id) = channel_id {
                                    if !self.config.aggregate_channels {
                                        info!("Sending CloseChannel message: {channel_id} for downstream: {downstream_id}");
                                        let reason_code =  Str0255::try_from("downstream disconnected".to_string()).unwrap();
                                        _ = self.sv1_server_channel_state
                                            .channel_manager_sender
                                            .send(Mining::CloseChannel(CloseChannel {
                                                channel_id,
                                                reason_code,
                                            }))
                                            .await;
                                    }
                                }
                            }
                        }
                        Ok(ShutdownMessage::DownstreamShutdownAll) => {
                            self.sv1_server_data.super_safe_lock(|d|{
                                if self.config.downstream_difficulty_config.enable_vardiff {
                                    d.vardiff = HashMap::new();
                                }
                                d.downstreams = HashMap::new();
                            });
                            info!("🔌 All downstreams removed from sv1 server as upstream changed");

                            // In aggregated mode, send UpdateChannel to reflect the new state (no downstreams)
                            if self.config.downstream_difficulty_config.enable_vardiff {
                                DifficultyManager::send_update_channel_on_downstream_state_change(
                                        &self.sv1_server_data,
                                        &self.sv1_server_channel_state.channel_manager_sender,
                                        self.config.aggregate_channels,
                                    ).await;
                            }
                        }
                        Ok(ShutdownMessage::UpstreamReconnectedResetAndShutdownDownstreams) => {
                            self.sv1_server_data.super_safe_lock(|d|{
                                if self.config.downstream_difficulty_config.enable_vardiff {
                                    d.vardiff = HashMap::new();
                                }
                                d.downstreams = HashMap::new();
                            });
                            info!("🔌 All downstreams removed from sv1 server as upstream reconnected");

                            // In aggregated mode, send UpdateChannel to reflect the new state (no downstreams)
                            if self.config.downstream_difficulty_config.enable_vardiff {
                                DifficultyManager::send_update_channel_on_downstream_state_change(
                                        &self.sv1_server_data,
                                        &self.sv1_server_channel_state.channel_manager_sender,
                                        self.config.aggregate_channels,
                                    ).await;
                            }
                        }
                        _ => {}
                    }
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            info!("New SV1 downstream connection from {}", addr);

                            let connection = ConnectionSV1::new(stream).await;
                            let downstream_id = self.sv1_server_data.super_safe_lock(|v| v.downstream_id_factory.next());
                            let downstream = Arc::new(Downstream::new(
                                downstream_id,
                                connection.sender().clone(),
                                connection.receiver().clone(),
                                self.sv1_server_channel_state.downstream_to_sv1_server_sender.clone(),
                                self.sv1_server_channel_state.sv1_server_to_downstream_sender.clone().subscribe(),
                                first_target.clone(),
                                Some(self.config.downstream_difficulty_config.min_individual_miner_hashrate),
                                self.sv1_server_data.clone(),
                            ));
                            // vardiff initialization (only if enabled)
                            _ = self.sv1_server_data
                                .safe_lock(|d| {
                                    d.downstreams.insert(downstream_id, downstream.clone());
                                    // Insert vardiff state for this downstream only if vardiff is enabled
                                    if self.config.downstream_difficulty_config.enable_vardiff {
                                        let vardiff = Arc::new(RwLock::new(VardiffState::new().expect("Failed to create vardiffstate")));
                                        d.vardiff.insert(downstream_id, vardiff);
                                    }
                                });
                            info!("Downstream {} registered successfully (channel will be opened after first message)", downstream_id);

                            // Start downstream tasks immediately, but defer channel opening until first message
                            let status_sender = StatusSender::Downstream {
                                downstream_id,
                                tx: status_sender.clone(),
                            };

                            Downstream::run_downstream_tasks(
                                downstream,
                                notify_shutdown.clone(),
                                shutdown_complete_tx.clone(),
                                status_sender,
                                task_manager.clone(),
                            );
                        }
                        Err(e) => {
                            warn!("Failed to accept new connection: {:?}", e);
                        }
                    }
                }
                res = Self::handle_downstream_message(
                    Arc::clone(&self)
                ) => {
                    if let Err(e) = res {
                        handle_error(&sv1_status_sender, e).await;
                        break;
                    }
                }
                res = Self::handle_upstream_message(
                    Arc::clone(&self),
                    first_target.clone(),
                ) => {
                    if let Err(e) = res {
                        handle_error(&sv1_status_sender, e).await;
                        break;
                    }
                }
            }
        }
        self.sv1_server_channel_state.drop();
        drop(shutdown_complete_tx);
        debug!("SV1 Server main listener loop exited.");
        Ok(())
    }

    /// Handles messages received from downstream SV1 miners.
    ///
    /// This method processes share submissions from miners by:
    /// - Updating variable difficulty counters
    /// - Extracting and validating share data
    /// - Converting SV1 share format to SV2 SubmitSharesExtended
    /// - Forwarding the share to the channel manager for upstream submission
    ///
    /// # Returns
    /// * `Ok(())` - Message processed successfully
    /// * `Err(TproxyError)` - Error processing the message
    pub async fn handle_downstream_message(self: Arc<Self>) -> Result<(), TproxyError> {
        let downstream_message = self
            .sv1_server_channel_state
            .downstream_to_sv1_server_receiver
            .recv()
            .await
            .map_err(TproxyError::ChannelErrorReceiver)?;

        match downstream_message {
            DownstreamMessages::SubmitShares(message) => {
                return self.handle_submit_shares(message).await;
            }
            DownstreamMessages::OpenChannel(downstream_id) => {
                return self.handle_open_channel_request(downstream_id).await;
            }
        }
    }

    /// Handles share submission messages from downstream.
    async fn handle_submit_shares(
        self: &Arc<Self>,
        message: crate::sv1::downstream::SubmitShareWithChannelId,
    ) -> Result<(), TproxyError> {
        // Increment vardiff counter for this downstream (only if vardiff is enabled)
        if self.config.downstream_difficulty_config.enable_vardiff {
            self.sv1_server_data.safe_lock(|v| {
                if let Some(vardiff_state) = v.vardiff.get(&message.downstream_id) {
                    vardiff_state
                        .write()
                        .unwrap()
                        .increment_shares_since_last_update();
                }
            })?;
        }

        let job_version = match message.job_version {
            Some(version) => version,
            None => {
                warn!("Received share submission without valid job version, skipping");
                return Ok(());
            }
        };

        let submit_share_extended = build_sv2_submit_shares_extended_from_sv1_submit(
            &message.share,
            message.channel_id,
            self.sequence_counter.load(Ordering::SeqCst),
            job_version,
            message.version_rolling_mask,
        )
        .map_err(|_| TproxyError::SV1Error)?;

        self.sv1_server_channel_state
            .channel_manager_sender
            .send(Mining::SubmitSharesExtended(submit_share_extended))
            .await
            .map_err(|_| TproxyError::ChannelErrorSender)?;

        self.sequence_counter.fetch_add(1, Ordering::SeqCst);

        Ok(())
    }

    /// Handles channel opening requests from downstream when they send their first message.
    async fn handle_open_channel_request(
        self: &Arc<Self>,
        downstream_id: u32,
    ) -> Result<(), TproxyError> {
        info!("SV1 Server: Opening extended mining channel for downstream {} after receiving first message", downstream_id);

        let downstreams = self
            .sv1_server_data
            .super_safe_lock(|v| v.downstreams.clone());
        if let Some(downstream) = Self::get_downstream(downstream_id, downstreams) {
            self.open_extended_mining_channel(downstream).await?;
        } else {
            error!(
                "Downstream {} not found when trying to open channel",
                downstream_id
            );
        }

        Ok(())
    }

    /// Handles messages received from the upstream SV2 server via the channel manager.
    ///
    /// This method processes various SV2 messages including:
    /// - OpenExtendedMiningChannelSuccess: Sets up downstream connections
    /// - NewExtendedMiningJob: Converts to SV1 notify messages
    /// - SetNewPrevHash: Updates block template information
    /// - Channel error messages (TODO: implement proper handling)
    ///
    /// # Arguments
    /// * `first_target` - Initial difficulty target for new connections
    /// * `notify_shutdown` - Broadcast channel for shutdown coordination
    /// * `shutdown_complete_tx` - Channel to signal shutdown completion
    /// * `status_sender` - Channel for sending status updates
    /// * `task_manager` - Manager for spawned async tasks
    ///
    /// # Returns
    /// * `Ok(())` - Message processed successfully
    /// * `Err(TproxyError)` - Error processing the message
    pub async fn handle_upstream_message(
        self: Arc<Self>,
        first_target: Target,
    ) -> Result<(), TproxyError> {
        let message = self
            .sv1_server_channel_state
            .channel_manager_receiver
            .recv()
            .await
            .map_err(TproxyError::ChannelErrorReceiver)?;

        match message {
            Mining::OpenExtendedMiningChannelSuccess(m) => {
                debug!(
                    "Received OpenExtendedMiningChannelSuccess for channel id: {}",
                    m.channel_id
                );
                let downstream_id = m.request_id;
                let downstreams = self
                    .sv1_server_data
                    .super_safe_lock(|v| v.downstreams.clone());
                if let Some(downstream) = Self::get_downstream(downstream_id, downstreams) {
                    let initial_target: Target = m.target.clone().into();
                    downstream.downstream_data.safe_lock(|d| {
                        d.extranonce1 = m.extranonce_prefix.to_vec();
                        d.extranonce2_len = m.extranonce_size.into();
                        d.channel_id = Some(m.channel_id);
                        // Set the initial upstream target from OpenExtendedMiningChannelSuccess
                        d.set_upstream_target(initial_target.clone());
                    })?;

                    // Process all queued messages now that channel is established
                    if let Ok(queued_messages) = downstream.downstream_data.safe_lock(|d| {
                        let messages = d.queued_sv1_handshake_messages.clone();
                        d.queued_sv1_handshake_messages.clear();
                        messages
                    }) {
                        if !queued_messages.is_empty() {
                            info!(
                                "Processing {} queued Sv1 messages for downstream {}",
                                queued_messages.len(),
                                downstream_id
                            );

                            // Set flag to indicate we're processing queued responses
                            downstream.downstream_data.super_safe_lock(|data| {
                                data.processing_queued_sv1_handshake_responses
                                    .store(true, std::sync::atomic::Ordering::SeqCst);
                            });

                            for message in queued_messages {
                                if let Ok(Some(response_msg)) = downstream
                                    .downstream_data
                                    .super_safe_lock(|data| data.handle_message(message))
                                {
                                    self.sv1_server_channel_state
                                        .sv1_server_to_downstream_sender
                                        .send((
                                            m.channel_id,
                                            Some(downstream_id),
                                            response_msg.into(),
                                        ))
                                        .map_err(|_| TproxyError::ChannelErrorSender)?;
                                }
                            }
                        }
                    }

                    let set_difficulty = build_sv1_set_difficulty_from_sv2_target(first_target)
                        .map_err(|_| {
                            TproxyError::General("Failed to generate set_difficulty".into())
                        })?;
                    // send the set_difficulty message to the downstream
                    self.sv1_server_channel_state
                        .sv1_server_to_downstream_sender
                        .send((m.channel_id, None, set_difficulty))
                        .map_err(|_| TproxyError::ChannelErrorSender)?;
                } else {
                    error!("Downstream not found for downstream_id: {}", downstream_id);
                }
            }

            Mining::NewExtendedMiningJob(m) => {
                debug!(
                    "Received NewExtendedMiningJob for channel id: {}",
                    m.channel_id
                );
                if let Some(prevhash) = self.sv1_server_data.super_safe_lock(|v| v.prevhash.clone())
                {
                    let notify = build_sv1_notify_from_sv2(
                        prevhash,
                        m.clone().into_static(),
                        self.clean_job.load(Ordering::SeqCst),
                    )?;
                    let clean_jobs = self.clean_job.load(Ordering::SeqCst);
                    self.clean_job.store(false, Ordering::SeqCst);

                    // Update job storage based on the configured mode
                    let notify_parsed = notify.clone();
                    self.sv1_server_data.super_safe_lock(|server_data| {
                        if let Some(ref mut aggregated_jobs) = server_data.aggregated_valid_jobs {
                            // Aggregated mode: all downstreams share the same jobs
                            if clean_jobs {
                                aggregated_jobs.clear();
                            }
                            aggregated_jobs.push(notify_parsed);
                        } else if let Some(ref mut non_aggregated_jobs) =
                            server_data.non_aggregated_valid_jobs
                        {
                            // Non-aggregated mode: per-downstream jobs
                            let channel_jobs = non_aggregated_jobs
                                .entry(m.channel_id)
                                .or_insert_with(Vec::new);
                            if clean_jobs {
                                channel_jobs.clear();
                            }
                            channel_jobs.push(notify_parsed);
                        }
                    });

                    let _ = self
                        .sv1_server_channel_state
                        .sv1_server_to_downstream_sender
                        .send((m.channel_id, None, notify.into()));
                }
            }

            Mining::SetNewPrevHash(m) => {
                debug!("Received SetNewPrevHash for channel id: {}", m.channel_id);
                self.clean_job.store(true, Ordering::SeqCst);
                self.sv1_server_data
                    .super_safe_lock(|v| v.prevhash = Some(m.clone().into_static()));
            }

            Mining::SetTarget(m) => {
                debug!("Received SetTarget for channel id: {}", m.channel_id);
                if self.config.downstream_difficulty_config.enable_vardiff {
                    // Vardiff enabled - use full difficulty management
                    DifficultyManager::handle_set_target_message(
                        m,
                        &self.sv1_server_data,
                        &self.sv1_server_channel_state.channel_manager_sender,
                        &self
                            .sv1_server_channel_state
                            .sv1_server_to_downstream_sender,
                        self.config.aggregate_channels,
                    )
                    .await;
                } else {
                    // Vardiff disabled - just forward the difficulty to downstreams
                    debug!("Vardiff disabled - forwarding SetTarget to downstreams");
                    self.handle_set_target_without_vardiff(m).await;
                }
            }

            Mining::CloseChannel(_) => {
                todo!("Handle CloseChannel message from upstream");
            }

            Mining::OpenMiningChannelError(_) => {
                todo!("Handle OpenMiningChannelError message from upstream");
            }

            Mining::UpdateChannelError(_) => {
                todo!("Handle UpdateChannelError message from upstream");
            }

            _ => unreachable!("Unexpected message type received from upstream"),
        }

        Ok(())
    }

    /// Opens an extended mining channel for a downstream connection.
    ///
    /// This method initiates the SV2 channel setup process by:
    /// - Calculating the initial target based on configuration
    /// - Generating a unique user identity for the miner
    /// - Creating an OpenExtendedMiningChannel message
    /// - Sending the request to the channel manager
    ///
    /// # Arguments
    /// * `downstream` - The downstream connection to set up a channel for
    ///
    /// # Returns
    /// * `Ok(())` - Channel setup request sent successfully
    /// * `Err(TproxyError)` - Error setting up the channel
    pub async fn open_extended_mining_channel(
        &self,
        downstream: Arc<Downstream>,
    ) -> Result<(), TproxyError> {
        let config = &self.config.downstream_difficulty_config;

        let hashrate = config.min_individual_miner_hashrate as f64;
        let shares_per_min = config.shares_per_minute as f64;
        let min_extranonce_size = self.config.downstream_extranonce2_size;
        let vardiff_enabled = config.enable_vardiff;

        let max_target = if vardiff_enabled {
            hash_rate_to_target(hashrate, shares_per_min)
                .unwrap()
                .into()
        } else {
            // If translator doesn't manage vardiff, we rely on upstream to do that,
            // so we give it more freedom by setting max_target to maximum possible value
            Target::from([0xff; 32])
        };

        // Store the initial target for use when no downstreams remain
        self.sv1_server_data.super_safe_lock(|data| {
            if data.initial_target.is_none() {
                data.initial_target = Some(max_target.clone());
            }
        });

        let miner_id = self.miner_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let user_identity = format!("{}.miner{}", self.config.user_identity, miner_id);

        downstream
            .downstream_data
            .safe_lock(|d| d.user_identity = user_identity.clone())?;

        if let Ok(open_channel_msg) = build_sv2_open_extended_mining_channel(
            downstream
                .downstream_data
                .super_safe_lock(|d| d.downstream_id),
            user_identity.clone(),
            hashrate as f32,
            max_target,
            min_extranonce_size,
        ) {
            self.sv1_server_channel_state
                .channel_manager_sender
                .send(Mining::OpenExtendedMiningChannel(open_channel_msg))
                .await
                .map_err(|_| TproxyError::ChannelErrorSender)?;
        } else {
            error!("Failed to build OpenExtendedMiningChannel message");
        }

        Ok(())
    }

    /// Retrieves a downstream connection by ID from the provided map.
    ///
    /// # Arguments
    /// * `downstream_id` - The ID of the downstream connection to find
    /// * `downstream` - HashMap containing downstream connections
    ///
    /// # Returns
    /// * `Some(Downstream)` - If a downstream with the given ID exists
    /// * `None` - If no downstream with the given ID is found
    pub fn get_downstream(
        downstream_id: u32,
        downstream: HashMap<u32, Arc<Downstream>>,
    ) -> Option<Arc<Downstream>> {
        downstream.get(&downstream_id).cloned()
    }

    /// Extracts the downstream ID from a Downstream instance.
    ///
    /// # Arguments
    /// * `downstream` - The downstream connection to get the ID from
    ///
    /// # Returns
    /// The downstream ID as a u32
    pub fn get_downstream_id(downstream: Downstream) -> u32 {
        downstream
            .downstream_data
            .super_safe_lock(|s| s.downstream_id)
    }

    /// Handles SetTarget messages when vardiff is disabled.
    ///
    /// This method forwards difficulty changes from upstream directly to downstream miners
    /// without any variable difficulty logic. It respects the aggregated/non-aggregated
    /// channel configuration.
    async fn handle_set_target_without_vardiff(&self, set_target: SetTarget<'_>) {
        let new_target: Target = set_target.maximum_target.clone().into();
        debug!(
            "Forwarding SetTarget to downstreams: channel_id={}, target={:?}",
            set_target.channel_id, new_target
        );

        if self.config.aggregate_channels {
            // Aggregated mode: send set_difficulty to ALL downstreams
            self.send_set_difficulty_to_all_downstreams(new_target)
                .await;
        } else {
            // Non-aggregated mode: send set_difficulty to specific downstream for this channel
            self.send_set_difficulty_to_specific_downstream(set_target.channel_id, new_target)
                .await;
        }
    }

    /// Sends set_difficulty to all downstreams (aggregated mode).
    /// Used only when vardiff is disabled.
    async fn send_set_difficulty_to_all_downstreams(&self, target: Target) {
        let downstreams = self
            .sv1_server_data
            .super_safe_lock(|data| data.downstreams.clone());

        for (downstream_id, downstream) in downstreams {
            let channel_id = downstream.downstream_data.super_safe_lock(|d| d.channel_id);

            if let Some(channel_id) = channel_id {
                // Update the downstream's target
                _ = downstream.downstream_data.safe_lock(|d| {
                    d.set_upstream_target(target.clone());
                    d.set_pending_target(target.clone());
                });

                // Send set_difficulty message
                if let Ok(set_difficulty_msg) =
                    build_sv1_set_difficulty_from_sv2_target(target.clone())
                {
                    if let Err(e) = self
                        .sv1_server_channel_state
                        .sv1_server_to_downstream_sender
                        .send((channel_id, Some(downstream_id), set_difficulty_msg))
                    {
                        error!(
                            "Failed to send SetDifficulty to downstream {}: {:?}",
                            downstream_id, e
                        );
                    } else {
                        debug!(
                            "Sent SetDifficulty to downstream {} (vardiff disabled)",
                            downstream_id
                        );
                    }
                }
            }
        }
    }

    /// Sends set_difficulty to the specific downstream associated with a channel (non-aggregated
    /// mode).
    /// Used only when vardiff is disabled.
    async fn send_set_difficulty_to_specific_downstream(&self, channel_id: u32, target: Target) {
        let affected_downstream = self.sv1_server_data.super_safe_lock(|data| {
            data.downstreams
                .iter()
                .find_map(|(downstream_id, downstream)| {
                    downstream.downstream_data.super_safe_lock(|d| {
                        if d.channel_id == Some(channel_id) {
                            Some((*downstream_id, downstream.clone()))
                        } else {
                            None
                        }
                    })
                })
        });

        if let Some((downstream_id, downstream)) = affected_downstream {
            // Update the downstream's target
            _ = downstream.downstream_data.safe_lock(|d| {
                d.set_upstream_target(target.clone());
                d.set_pending_target(target.clone());
            });

            // Send set_difficulty message
            if let Ok(set_difficulty_msg) = build_sv1_set_difficulty_from_sv2_target(target) {
                if let Err(e) = self
                    .sv1_server_channel_state
                    .sv1_server_to_downstream_sender
                    .send((channel_id, Some(downstream_id), set_difficulty_msg))
                {
                    error!(
                        "Failed to send SetDifficulty to downstream {}: {:?}",
                        downstream_id, e
                    );
                } else {
                    debug!(
                        "Sent SetDifficulty to downstream {} for channel {} (vardiff disabled)",
                        downstream_id, channel_id
                    );
                }
            }
        } else {
            warn!(
                "No downstream found for channel {} when vardiff disabled",
                channel_id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DownstreamDifficultyConfig, TranslatorConfig, Upstream};
    use async_channel::unbounded;
    use key_utils::Secp256k1PublicKey;
    use std::{collections::HashMap, str::FromStr};

    fn create_test_config() -> TranslatorConfig {
        let pubkey_str = "9bDuixKmZqAJnrmP746n8zU1wyAQRrus7th9dxnkPg6RzQvCnan";
        let pubkey = Secp256k1PublicKey::from_str(pubkey_str).unwrap();

        let upstream = Upstream::new("127.0.0.1".to_string(), 4444, pubkey);
        let difficulty_config = DownstreamDifficultyConfig::new(100.0, 5.0, true);

        TranslatorConfig::new(
            vec![upstream],
            "0.0.0.0".to_string(), // downstream_address
            3333,                  // downstream_port
            difficulty_config,     // downstream_difficulty_config
            2,                     // max_supported_version
            1,                     // min_supported_version
            4,                     // downstream_extranonce2_size
            "test_user".to_string(),
            true, // aggregate_channels
        )
    }

    fn create_test_sv1_server() -> Sv1Server {
        let (cm_sender, _cm_receiver) = unbounded();
        let (_downstream_sender, cm_receiver) = unbounded();
        let config = create_test_config();
        let addr = "127.0.0.1:3333".parse().unwrap();

        Sv1Server::new(addr, cm_receiver, cm_sender, config)
    }

    #[test]
    fn test_sv1_server_creation() {
        let server = create_test_sv1_server();

        assert_eq!(server.shares_per_minute, 5.0);
        assert_eq!(server.listener_addr.ip().to_string(), "127.0.0.1");
        assert_eq!(server.listener_addr.port(), 3333);
        assert_eq!(server.config.user_identity, "test_user");
        assert!(server.config.aggregate_channels);
    }

    #[test]
    fn test_sv1_server_aggregated_config() {
        let mut config = create_test_config();
        config.aggregate_channels = true;
        config.downstream_difficulty_config.enable_vardiff = true;

        let (cm_sender, _cm_receiver) = unbounded();
        let (_downstream_sender, cm_receiver) = unbounded();
        let addr = "127.0.0.1:3333".parse().unwrap();

        let server = Sv1Server::new(addr, cm_receiver, cm_sender, config);

        assert!(server.config.aggregate_channels);
        assert!(server.config.downstream_difficulty_config.enable_vardiff);
    }

    #[test]
    fn test_sv1_server_non_aggregated_config() {
        let mut config = create_test_config();
        config.aggregate_channels = false;
        config.downstream_difficulty_config.enable_vardiff = false;

        let (cm_sender, _cm_receiver) = unbounded();
        let (_downstream_sender, cm_receiver) = unbounded();
        let addr = "127.0.0.1:3333".parse().unwrap();

        let server = Sv1Server::new(addr, cm_receiver, cm_sender, config);

        assert!(!server.config.aggregate_channels);
        assert!(!server.config.downstream_difficulty_config.enable_vardiff);
    }

    #[test]
    fn test_get_downstream_basic() {
        let downstreams = HashMap::new();

        // Test non-existing downstream
        let not_found = Sv1Server::get_downstream(999, downstreams);
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_send_set_difficulty_to_all_downstreams_empty() {
        let server = create_test_sv1_server();
        let target: Target = hash_rate_to_target(200.0, 5.0).unwrap().into();

        // Test with empty downstreams
        server.send_set_difficulty_to_all_downstreams(target).await;

        // Should not crash with empty downstreams
    }

    #[tokio::test]
    async fn test_send_set_difficulty_to_specific_downstream_not_found() {
        let server = create_test_sv1_server();
        let target: Target = hash_rate_to_target(200.0, 5.0).unwrap().into();
        let channel_id = 1u32;

        // Test with no downstreams
        server
            .send_set_difficulty_to_specific_downstream(channel_id, target)
            .await;

        // Should not crash when no downstreams are found
    }

    #[tokio::test]
    async fn test_handle_set_target_without_vardiff_aggregated() {
        let mut config = create_test_config();
        config.downstream_difficulty_config.enable_vardiff = false;
        config.aggregate_channels = true;

        let (cm_sender, _cm_receiver) = unbounded();
        let (_downstream_sender, cm_receiver) = unbounded();
        let addr = "127.0.0.1:3333".parse().unwrap();

        let server = Sv1Server::new(addr, cm_receiver, cm_sender, config);
        let target: Target = hash_rate_to_target(200.0, 5.0).unwrap().into();

        let set_target = SetTarget {
            channel_id: 1,
            maximum_target: target.clone().into(),
        };

        // Test should not panic and should handle the message
        server.handle_set_target_without_vardiff(set_target).await;
    }

    #[tokio::test]
    async fn test_handle_set_target_without_vardiff_non_aggregated() {
        let mut config = create_test_config();
        config.downstream_difficulty_config.enable_vardiff = false;
        config.aggregate_channels = false;

        let (cm_sender, _cm_receiver) = unbounded();
        let (_downstream_sender, cm_receiver) = unbounded();
        let addr = "127.0.0.1:3333".parse().unwrap();

        let server = Sv1Server::new(addr, cm_receiver, cm_sender, config);
        let target: Target = hash_rate_to_target(200.0, 5.0).unwrap().into();

        let set_target = SetTarget {
            channel_id: 1,
            maximum_target: target.clone().into(),
        };

        // Test should not panic and should handle the message
        server.handle_set_target_without_vardiff(set_target).await;
    }

    #[test]
    fn test_sv1_server_counters() {
        let server = create_test_sv1_server();

        // Test initial values
        assert_eq!(server.miner_counter.load(Ordering::SeqCst), 0);
        assert_eq!(server.sequence_counter.load(Ordering::SeqCst), 0);

        // Test incrementing
        let miner_id = server.miner_counter.fetch_add(1, Ordering::SeqCst);
        assert_eq!(miner_id, 0);
        assert_eq!(server.miner_counter.load(Ordering::SeqCst), 1);

        let seq_id = server.sequence_counter.fetch_add(1, Ordering::SeqCst);
        assert_eq!(seq_id, 0);
        assert_eq!(server.sequence_counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_sv1_server_clean_job_flag() {
        let server = create_test_sv1_server();

        // Test initial value
        assert!(server.clean_job.load(Ordering::SeqCst));

        // Test setting to false
        server.clean_job.store(false, Ordering::SeqCst);
        assert!(!server.clean_job.load(Ordering::SeqCst));

        // Test setting back to true
        server.clean_job.store(true, Ordering::SeqCst);
        assert!(server.clean_job.load(Ordering::SeqCst));
    }
}
