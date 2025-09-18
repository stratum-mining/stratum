//! Template Receiver module
//!
//! This module defines the [`TemplateReceiver`] struct, which manages a connection
//! to a Template Provider (TP).
//!
//! Responsibilities:
//! - Establish TCP + Noise encrypted connection to the template provider
//! - Perform `SetupConnection` handshake
//! - Forward SV2 `TemplateDistribution` messages to the channel manager
//! - Forward messages from the channel manager upstream to the template provider
//! - Send [`CoinbaseOutputConstraints`] to the template provider

use std::{net::SocketAddr, sync::Arc};

use async_channel::{unbounded, Receiver, Sender};
use key_utils::Secp256k1PublicKey;
use stratum_common::{
    network_helpers_sv2::noise_stream::NoiseTcpStream,
    roles_logic_sv2::{
        bitcoin::{
            self, absolute::LockTime, transaction::Version, OutPoint, ScriptBuf, Sequence,
            Transaction, TxIn, TxOut, Witness,
        },
        codec_sv2::{self, framing_sv2, HandshakeRole, Initiator},
        handlers_sv2::HandleCommonMessagesFromServerAsync,
        parsers_sv2::{AnyMessage, TemplateDistribution},
        template_distribution_sv2::CoinbaseOutputConstraints,
        utils::Mutex,
    },
};
use tokio::{net::TcpStream, sync::broadcast};
use tracing::{debug, error, info, warn};

use crate::{
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{
        get_setup_connection_message_tp, protocol_message_type, spawn_io_tasks, Message,
        MessageType, SV2Frame, ShutdownMessage, StdFrame,
    },
};

mod message_handler;

/// Placeholder for future template receiver–specific state.
pub struct TemplateReceiverData;

/// Holds communication channels between the template receiver, channel manager,
/// and upstream template provider.
///
/// - `channel_manager_sender` → sends frames to the channel manager
/// - `channel_manager_receiver` → receives frames from the channel manager
/// - `outbound_tx` → sends frames upstream to the template provider
/// - `inbound_rx` → receives frames from the template provider
#[derive(Clone)]
pub struct TemplateReceiverChannel {
    channel_manager_sender: Sender<SV2Frame>,
    channel_manager_receiver: Receiver<SV2Frame>,
    tp_sender: Sender<SV2Frame>,
    tp_receiver: Receiver<SV2Frame>,
}

/// Manages communication with a Stratum V2 Template Provider.
///
/// Responsibilities:
/// - Establishes TCP + Noise connection to TP
/// - Performs handshake (`SetupConnection`)
/// - Sends [`CoinbaseOutputConstraints`] to TP
/// - Routes messages between TP and channel manager
/// - Handles shutdown/fallback notifications
#[allow(warnings)]
#[derive(Clone)]
pub struct TemplateReceiver {
    /// Internal state
    template_receiver_data: Arc<Mutex<TemplateReceiverData>>,
    /// Messaging channels to/from the channel manager and TP.
    template_receiver_channel: TemplateReceiverChannel,
    /// Address of the template provider (string form)
    tp_address: String,
}

impl TemplateReceiver {
    /// Establish a new connection to a Template Provider.
    ///
    /// - Opens a TCP connection
    /// - Performs Noise handshake
    /// - Spawns IO tasks for inbound/outbound frames
    ///
    /// Retries up to 3 times before returning [`JDCError::Shutdown`].
    pub async fn new(
        tp_address: String,
        public_key: Option<Secp256k1PublicKey>,
        channel_manager_receiver: Receiver<SV2Frame>,
        channel_manager_sender: Sender<SV2Frame>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Result<TemplateReceiver, JDCError> {
        const MAX_RETRIES: usize = 3;

        for attempt in 1..=MAX_RETRIES {
            info!(attempt, MAX_RETRIES, "Connecting to template provider");

            let initiator = match public_key {
                Some(pub_key) => {
                    debug!(attempt, "Using public key for initiator handshake");
                    Initiator::from_raw_k(pub_key.into_bytes())
                }
                None => {
                    debug!(attempt, "Using anonymous initiator (no public key)");
                    Initiator::without_pk()
                }
            }?;

            match TcpStream::connect(tp_address.as_str()).await {
                Ok(stream) => {
                    info!(
                        attempt,
                        "TCP connection established, starting Noise handshake"
                    );

                    match NoiseTcpStream::<Message>::new(
                        stream,
                        HandshakeRole::Initiator(initiator),
                    )
                    .await
                    {
                        Ok(noise_stream) => {
                            info!(attempt, "Noise handshake completed successfully");

                            let (noise_stream_reader, noise_stream_writer) =
                                noise_stream.into_split();

                            let status_sender = StatusSender::TemplateReceiver(status_sender);
                            let (inbound_tx, inbound_rx) = unbounded::<SV2Frame>();
                            let (outbound_tx, outbound_rx) = unbounded::<SV2Frame>();

                            info!(attempt, "Spawning IO tasks for template receiver");
                            spawn_io_tasks(
                                task_manager.clone(),
                                noise_stream_reader,
                                noise_stream_writer,
                                outbound_rx,
                                inbound_tx,
                                notify_shutdown,
                                status_sender,
                            );

                            let template_receiver_data = Arc::new(Mutex::new(TemplateReceiverData));
                            let template_receiver_channel = TemplateReceiverChannel {
                                channel_manager_receiver,
                                channel_manager_sender,
                                tp_receiver: inbound_rx,
                                tp_sender: outbound_tx,
                            };

                            info!(attempt, "TemplateReceiver initialized successfully");
                            return Ok(TemplateReceiver {
                                template_receiver_channel,
                                template_receiver_data,
                                tp_address,
                            });
                        }
                        Err(e) => {
                            error!(attempt, error = ?e, "Noise handshake failed");
                        }
                    }
                }
                Err(e) => {
                    warn!(attempt, MAX_RETRIES, error = ?e, "Failed to connect to template provider");
                }
            }

            if attempt < MAX_RETRIES {
                debug!(attempt, "Retrying connection after backoff");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }

        error!("Exhausted all connection attempts, shutting down TemplateReceiver");
        Err(JDCError::Shutdown)
    }

    /// Start unified message loop for template receiver.
    ///
    /// Responsibilities:
    /// - Run handshake (`setup_connection`)
    /// - Send [`CoinbaseOutputConstraints`]
    /// - Handle:
    ///   - Messages from template provider
    ///   - Messages from channel manager
    ///   - Shutdown signals (upstream/job-declarator fallback)
    pub async fn start(
        mut self,
        socket_address: String,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
        coinbase_outputs: Vec<u8>,
    ) {
        let status_sender = StatusSender::TemplateReceiver(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        info!("Initialized state for starting template receiver");
        _ = self.setup_connection(socket_address).await;

        _ = self.coinbase_constraints(coinbase_outputs).await;

        info!("Setup Connection done. connection with template receiver is now done");
        task_manager.spawn(
            async move {
                loop {
                    let mut self_clone_1 = self.clone();
                    let self_clone_2 = self.clone();
                    tokio::select! {
                        message = shutdown_rx.recv() => {
                            match message {
                                Ok(ShutdownMessage::ShutdownAll) => {
                                    info!("Template Receiver: received shutdown signal");
                                    break;
                                },
                                Ok(ShutdownMessage::UpstreamShutdownFallback((coinbase_outputs,tx))) => {
                                    info!("Template provider: Received Upstream shutdown.");
                                    _ = self.coinbase_constraints(coinbase_outputs).await;
                                    drop(tx);
                                }
                                Ok(ShutdownMessage::JobDeclaratorShutdownFallback((coinbase_outputs, tx))) => {
                                    info!("Template provider: Received Job declarator shutdown.");
                                    _ = self.coinbase_constraints(coinbase_outputs).await;
                                    drop(tx);
                                }
                                Err(e) => {
                                    warn!(error = ?e, "Template Receiver: shutdown channel closed unexpectedly");
                                    break;
                                }
                                _ => {}
                            }
                        }
                        res = self_clone_1.handle_template_provider_message() => {
                            if let Err(e) = res {
                                error!("TemplateReceiver template provider handler failed: {e:?}");
                                handle_error(&status_sender, e).await;
                                break;
                            }
                        }
                        res = self_clone_2.handle_channel_manager_message() => {
                            if let Err(e) = res {
                                error!("TemplateReceiver channel manager handler failed: {e:?}");
                                handle_error(&status_sender, e).await;
                                break;
                            }
                        },
                    }
                }
                warn!("TemplateReceiver: unified message loop exited.");
            },
        );
    }

    /// Handle inbound messages from the template provider.
    ///
    /// Routes:
    /// - `Common` messages → handled locally
    /// - `TemplateDistribution` messages → forwarded to channel manager
    /// - Unsupported messages → logged and ignored
    pub async fn handle_template_provider_message(&mut self) -> Result<(), JDCError> {
        let mut sv2_frame = self.template_receiver_channel.tp_receiver.recv().await?;

        debug!("Received SV2 frame from Template provider.");
        let Some(message_type) = sv2_frame.get_header().map(|m| m.msg_type()) else {
            return Ok(());
        };
        match protocol_message_type(message_type) {
            MessageType::Common => {
                info!(
                    ?message_type,
                    "Handling common message from Template provider."
                );
                self.handle_common_message_frame_from_server(message_type, sv2_frame.payload())
                    .await?;
            }
            MessageType::TemplateDistribution => {
                self.template_receiver_channel
                    .channel_manager_sender
                    .send(sv2_frame)
                    .await
                    .map_err(|e| {
                        error!(error=?e, "Failed to send template distribution message to channel manager.");
                        JDCError::ChannelErrorSender
                    })?;
            }
            _ => {
                warn!("Received unsupported message type from template provider: {message_type}");
            }
        }
        Ok(())
    }

    /// Handle messages from channel manager → template provider.
    ///
    /// Forwards outbound frames upstream
    pub async fn handle_channel_manager_message(&self) -> Result<(), JDCError> {
        let msg = self
            .template_receiver_channel
            .channel_manager_receiver
            .recv()
            .await?;
        debug!("Forwarding message from channel manager to outbound_tx");
        self.template_receiver_channel
            .tp_sender
            .send(msg)
            .await
            .map_err(|_| JDCError::ChannelErrorSender)?;

        Ok(())
    }

    /// Build and send [`CoinbaseOutputConstraints`] upstream TP.
    pub async fn coinbase_constraints(
        &mut self,
        coinbase_outputs: Vec<u8>,
    ) -> Result<(), JDCError> {
        debug!(
            "Deserializing coinbase outputs ({} bytes)",
            coinbase_outputs.len()
        );
        let outputs: Vec<TxOut> = bitcoin::consensus::deserialize(&coinbase_outputs)?;

        let max_size: u32 = outputs.iter().map(|o| o.size() as u32).sum();
        debug!(
            max_size,
            outputs_count = outputs.len(),
            "Calculated max coinbase output size"
        );

        let dummy_coinbase = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::from(vec![vec![0; 32]]),
            }],
            output: outputs,
        };

        let max_sigops = dummy_coinbase.total_sigop_cost(|_| None) as u16;
        debug!(max_sigops, "Calculated max sigops for coinbase");

        let constraints = CoinbaseOutputConstraints {
            coinbase_output_max_additional_size: max_size,
            coinbase_output_max_additional_sigops: max_sigops,
        };

        let msg = AnyMessage::TemplateDistribution(
            TemplateDistribution::CoinbaseOutputConstraints(constraints),
        );

        let frame: StdFrame = msg.try_into()?;
        info!("Sending CoinbaseOutputConstraints message upstream");
        self.template_receiver_channel
            .tp_sender
            .send(frame)
            .await
            .map_err(|_| {
                error!("Failed to send CoinbaseOutputConstraints message upstream");
                JDCError::ChannelErrorSender
            })?;

        Ok(())
    }

    // Performs the initial handshake with template provider.
    pub async fn setup_connection(&mut self, addr: String) -> Result<(), JDCError> {
        let socket: SocketAddr = addr.parse().map_err(|_| {
            error!(%addr, "Invalid socket address");
            JDCError::InvalidSocketAddress(addr.clone())
        })?;

        info!(%socket, "Building setup connection message for upstream");
        let setup_msg = get_setup_connection_message_tp(socket);
        let frame: StdFrame = Message::Common(setup_msg.into()).try_into()?;

        info!("Sending setup connection message to upstream");
        self.template_receiver_channel
            .tp_sender
            .send(frame)
            .await
            .map_err(|_| {
                error!("Failed to send setup connection message upstream");
                JDCError::ChannelErrorSender
            })?;

        info!("Waiting for upstream handshake response");
        let mut incoming: StdFrame = self
            .template_receiver_channel
            .tp_receiver
            .recv()
            .await
            .map_err(|e| {
                error!(?e, "Upstream connection closed during handshake");
                JDCError::CodecNoise(codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage)
            })?;

        let msg_type = incoming
            .get_header()
            .ok_or(framing_sv2::Error::ExpectedHandshakeFrame)?
            .msg_type();
        debug!(?msg_type, "Received upstream handshake response");

        self.handle_common_message_frame_from_server(msg_type, incoming.payload())
            .await?;
        info!("Handshake with upstream completed successfully");
        Ok(())
    }
}
