use std::{net::SocketAddr, sync::Arc};

use async_channel::{unbounded, Receiver, Sender};
use key_utils::Secp256k1PublicKey;
use stratum_common::{
    network_helpers_sv2::noise_stream::NoiseTcpStream,
    roles_logic_sv2::{
        codec_sv2::{self, framing_sv2, HandshakeRole, Initiator},
        handlers_sv2::HandleCommonMessagesFromServerAsync,
        utils::Mutex,
    },
};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};
use tracing::{debug, error, info, warn};

use crate::{
    config::ConfigJDCMode,
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{
        get_setup_connection_message_jds, protocol_message_type, spawn_io_tasks, Message,
        MessageType, SV2Frame, ShutdownMessage, StdFrame,
    },
};

mod message_handler;

/// Shared state for Job Declarator
pub struct JobDeclaratorData;

/// Holds all channels required for Job Declarator communication.
#[derive(Clone)]
pub struct JobDeclaratorChannel {
    channel_manager_sender: Sender<SV2Frame>,
    channel_manager_receiver: Receiver<SV2Frame>,
    jds_sender: Sender<SV2Frame>,
    jds_receiver: Receiver<SV2Frame>,
}

/// Manages the lifecycle and communication with a Job Declarator (JDS)
#[allow(warnings)]
#[derive(Clone)]
pub struct JobDeclarator {
    /// Internal state
    job_declarator_data: Arc<Mutex<JobDeclaratorData>>,
    /// Messaging channels to/from the channel manager and JD.
    job_declarator_channel: JobDeclaratorChannel,
    /// Socket address of the Job Declarator server.
    socket_address: SocketAddr,
    /// Config JDC mode
    mode: ConfigJDCMode,
}

impl JobDeclarator {
    /// Creates a new JobDeclarator instance by connecting and performing a Noise handshake.
    ///
    /// - Establishes TCP connection.
    /// - Performs SV2 Noise handshake.
    /// - Spawns background IO tasks for reading/writing frames.
    pub async fn new(
        upstreams: &(SocketAddr, SocketAddr, Secp256k1PublicKey, bool),
        channel_manager_sender: Sender<SV2Frame>,
        channel_manager_receiver: Receiver<SV2Frame>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        mode: ConfigJDCMode,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Result<Self, JDCError> {
        let (_, addr, pubkey, _) = upstreams;
        info!("Connecting to JD Server at {addr}");
        let stream = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            TcpStream::connect(addr),
        )
        .await??;
        info!("Connection established with JD Server at {addr} in mode: {mode:?}");
        let initiator = Initiator::from_raw_k(pubkey.into_bytes())?;
        let (noise_stream_reader, noise_stream_writer) =
            NoiseTcpStream::<Message>::new(stream, HandshakeRole::Initiator(initiator))
                .await?
                .into_split();

        let status_sender = StatusSender::JobDeclarator(status_sender);
        let (inbound_tx, inbound_rx) = unbounded::<SV2Frame>();
        let (outbound_tx, outbound_rx) = unbounded::<SV2Frame>();

        spawn_io_tasks(
            task_manager,
            noise_stream_reader,
            noise_stream_writer,
            outbound_rx,
            inbound_tx,
            notify_shutdown,
            status_sender,
        );
        let job_declarator_data = Arc::new(Mutex::new(JobDeclaratorData));
        let job_declarator_channel = JobDeclaratorChannel {
            channel_manager_receiver,
            channel_manager_sender,
            jds_sender: outbound_tx,
            jds_receiver: inbound_rx,
        };
        Ok(JobDeclarator {
            job_declarator_channel,
            job_declarator_data,
            socket_address: *addr,
            mode,
        })
    }

    /// Starts the JobDeclarator message loop.
    ///
    /// - Waits for shutdown signals.
    /// - Handles incoming messages from Job Declarator and Channel Manager.
    /// - Cleans up on termination.
    pub async fn start(
        mut self,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::JobDeclarator(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        if let Err(e) = self.setup_connection().await {
            handle_error(&status_sender, e).await;
            return;
        }

        task_manager.spawn(
            async move {
                loop {
                    let mut self_clone_1 = self.clone();
                    let self_clone_2 = self.clone();
                    tokio::select! {
                        message = shutdown_rx.recv() => {
                            match message {
                                Ok(ShutdownMessage::ShutdownAll) => {
                                    info!("Job Declarator: received shutdown signal.");
                                    break;
                                }
                                Ok(ShutdownMessage::JobDeclaratorShutdownFallback(_)) => {
                                    info!("Job Declarator: Received Job declarator shutdown.");
                                    break;
                                }
                                Ok(ShutdownMessage::UpstreamShutdownFallback(_)) => {
                                    info!("Job Declarator: Received Upstream shutdown.");
                                    break;
                                }
                                Ok(ShutdownMessage::UpstreamShutdown(tx)) => {
                                    info!("Job declarator shutdown requested");
                                    drop(tx);
                                    break;
                                }
                                Ok(ShutdownMessage::JobDeclaratorShutdown(tx)) => {
                                    info!("Job declarator shutdown requested");
                                    drop(tx);
                                    break;
                                }
                                Err(e) => {
                                    warn!(error = ?e, "Job Declarator: shutdown channel closed unexpectedly");
                                    break;
                                }
                                _ => {}
                            }
                        }
                        res = self_clone_1.handle_job_declarator_message() => {
                            if let Err(e) = res {
                                error!(error = ?e, "Job Declarator message handling failed");
                                handle_error(&status_sender, e).await;
                                break;
                            }
                        }
                        res = self_clone_2.handle_channel_manager_message() => {
                            if let Err(e) = res {
                                error!(error = ?e, "Channel Manager message handling failed");
                                handle_error(&status_sender, e).await;
                                break;
                            }
                        },
                    }
                }
                drop(shutdown_complete_tx);
                warn!("JobDeclarator: unified message loop exited.");
            },
        );
    }

    /// Performs SV2 setup connection handshake with Job Declarator server.
    ///
    /// - Sends `SetupConnection` message.
    /// - Waits for and validates server response.
    /// - Completes SV2 protocol handshake.
    pub async fn setup_connection(&mut self) -> Result<(), JDCError> {
        info!("Sending SetupConnection to JDS at {}", self.socket_address);

        let setup_connection = get_setup_connection_message_jds(&self.socket_address, &self.mode);
        let sv2_frame: StdFrame = Message::Common(setup_connection.into())
            .try_into()
            .map_err(|e| {
                error!(error=?e, "Failed to serialize SetupConnection message.");
                JDCError::CodecNoise(codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage)
            })?;

        if let Err(e) = self.job_declarator_channel.jds_sender.send(sv2_frame).await {
            error!(error=?e, "Failed to send SetupConnection frame.");
            return Err(JDCError::ChannelErrorSender);
        }
        debug!("SetupConnection frame sent successfully.");

        let mut incoming = self
            .job_declarator_channel
            .jds_receiver
            .recv()
            .await
            .map_err(|e| {
                error!(error=?e, "No handshake response received from Job declarator.");
                JDCError::CodecNoise(codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage)
            })?;

        let message_type = incoming
            .get_header()
            .ok_or_else(|| {
                error!("Handshake frame missing header.");
                framing_sv2::Error::ExpectedHandshakeFrame
            })?
            .msg_type();

        debug!(?message_type, "Processing handshake response.");

        self.handle_common_message_frame_from_server(message_type, incoming.payload())
            .await?;

        info!("Job declarator: SV2 handshake completed successfully.");
        Ok(())
    }

    // Handles messages coming from the Channel Manager and forwards them to the Job Declarator.
    async fn handle_channel_manager_message(&self) -> Result<(), JDCError> {
        match self
            .job_declarator_channel
            .channel_manager_receiver
            .recv()
            .await
        {
            Ok(msg) => {
                debug!("Forwarding message from channel manager to JDS.");
                self.job_declarator_channel
                    .jds_sender
                    .send(msg)
                    .await
                    .map_err(|e| {
                        error!("Failed to send message to outbound channel: {:?}", e);
                        JDCError::ChannelErrorSender
                    })?;
            }
            Err(e) => {
                warn!("Channel manager receiver closed or errored: {:?}", e);
            }
        }
        Ok(())
    }

    // Handles messages received from the Job Declarator.
    //
    // - Forwards `JobDeclaration` messages to Channel Manager.
    // - Processes `Common` messages via handler.
    // - Rejects unsupported message types.
    async fn handle_job_declarator_message(&mut self) -> Result<(), JDCError> {
        let mut sv2_frame = self.job_declarator_channel.jds_receiver.recv().await?;

        debug!("Received SV2 frame from JDS.");
        let Some(message_type) = sv2_frame.get_header().map(|m| m.msg_type()) else {
            return Ok(());
        };

        match protocol_message_type(message_type) {
            MessageType::Common => {
                info!(?message_type, "Handling common message from Upstream.");
                self.handle_common_message_frame_from_server(message_type, sv2_frame.payload())
                    .await?;
            }
            MessageType::JobDeclaration => {
                self.job_declarator_channel
                    .channel_manager_sender
                    .send(sv2_frame)
                    .await
                    .map_err(|e| {
                        error!(error=?e, "Failed to send Job declaration message to channel manager.");
                        JDCError::ChannelErrorSender
                    })?;
            }
            _ => {
                warn!("Received unsupported message type from Job declarator: {message_type}");
            }
        }

        Ok(())
    }
}
