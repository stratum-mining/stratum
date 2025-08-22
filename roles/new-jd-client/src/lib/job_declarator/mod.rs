use std::{net::SocketAddr, sync::Arc};

use async_channel::{unbounded, Receiver, Sender};
use key_utils::Secp256k1PublicKey;
use stratum_common::{
    network_helpers_sv2::noise_stream::{NoiseTcpReadHalf, NoiseTcpStream, NoiseTcpWriteHalf},
    roles_logic_sv2::{
        codec_sv2::{self, framing_sv2, HandshakeRole, Initiator},
        handlers_sv2::HandleCommonMessagesFromServerAsync,
        parsers_sv2::AnyMessage,
        utils::Mutex,
    },
};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};
use tracing::{debug, error, info, instrument, warn, Instrument, Span};

use crate::{
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{
        get_setup_connection_message_jds, message_from_frame,
        spawn_io_tasks, EitherFrame, Message, ShutdownMessage, StdFrame,
    },
};

mod message_handler;

pub struct JobDeclaratorData;

#[derive(Clone)]
pub struct JobDeclaratorChannel {
    channel_manager_sender: Sender<EitherFrame>,
    channel_manager_receiver: Receiver<EitherFrame>,
    outbound_tx: Sender<EitherFrame>,
    inbound_rx: Receiver<EitherFrame>,
}

#[derive(Clone)]
pub struct JobDeclarator {
    job_declarator_data: Arc<Mutex<JobDeclaratorData>>,
    job_declarator_channel: JobDeclaratorChannel,
    socket_address: SocketAddr,
}

impl JobDeclarator {
    #[instrument(skip_all, fields(jds_addr = %upstreams.1))]
    pub async fn new(
        upstreams: &(SocketAddr, SocketAddr, Secp256k1PublicKey),
        channel_manager_sender: Sender<EitherFrame>,
        channel_manager_receiver: Receiver<EitherFrame>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Result<Self, JDCError> {
        let (_, addr, pubkey) = upstreams;
        info!("Connecting to JD Server at {addr}");
        let stream = TcpStream::connect(addr).await?;
        info!("Connection established with JD Server at {addr}");
        let initiator = Initiator::from_raw_k(pubkey.into_bytes())?;
        let (noise_stream_reader, noise_stream_writer) =
            NoiseTcpStream::<Message>::new(stream, HandshakeRole::Initiator(initiator))
                .await?
                .into_split();

        let status_sender = StatusSender::JobDeclarator(status_sender);
        let (inbound_tx, inbound_rx) = unbounded::<EitherFrame>();
        let (outbound_tx, outbound_rx) = unbounded::<EitherFrame>();

        spawn_io_tasks(
            task_manager,
            noise_stream_reader,
            noise_stream_writer,
            outbound_rx,
            inbound_tx,
            notify_shutdown,
            status_sender,
            Span::current(),
        );
        let job_declarator_data = Arc::new(Mutex::new(JobDeclaratorData));
        let job_declarator_channel = JobDeclaratorChannel {
            channel_manager_receiver,
            channel_manager_sender,
            outbound_tx,
            inbound_rx,
        };
        Ok(JobDeclarator {
            job_declarator_channel,
            job_declarator_data,
            socket_address: *addr,
        })
    }

    #[instrument(
        skip_all,
        fields(jds_addr = %self.socket_address)
    )]
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
                                Ok(ShutdownMessage::JobDeclaratorShutdown) => {
                                    info!("Job Declarator: Received Job declarator shutdown.");
                                    break;
                                }
                                Ok(ShutdownMessage::UpstreamShutdown) => {
                                    info!("Job Declarator: Received Upstream shutdown.");
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
                warn!("JobDeclarator: unified message loop exited.");
            }
            .instrument(Span::current()),
        );
    }

    #[instrument(skip_all)]
    pub async fn setup_connection(&mut self) -> Result<(), JDCError> {
        info!(
            "Upstream: initiating SV2 handshake with {}",
            self.socket_address
        );

        let setup_connection = get_setup_connection_message_jds(&self.socket_address);
        let sv2_frame: StdFrame = Message::Common(setup_connection.into())
            .try_into()
            .map_err(|e| {
                error!(error=?e, "Failed to serialize SetupConnection message.");
                JDCError::CodecNoise(codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage)
            })?;

        if let Err(e) = self
            .job_declarator_channel
            .outbound_tx
            .send(sv2_frame.into())
            .await
        {
            error!(error=?e, "Failed to send SetupConnection frame.");
            return Err(JDCError::ChannelErrorSender);
        }
        debug!("SetupConnection frame sent successfully.");

        let incoming_frame = self
            .job_declarator_channel
            .inbound_rx
            .recv()
            .await
            .map_err(|e| {
                error!(error=?e, "No handshake response received from Job declarator.");
                JDCError::CodecNoise(codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage)
            })?;

        let mut incoming: StdFrame = incoming_frame.try_into().map_err(|e| {
            error!(error=?e, "Failed to decode incoming handshake frame.");
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

        self.handle_common_message_from_server(message_type, incoming.payload())
            .await?;

        info!("Job declarator: SV2 handshake completed successfully.");
        Ok(())
    }

    #[instrument(name = "channel_manager_message", skip_all)]
    async fn handle_channel_manager_message(&self) -> Result<(), JDCError> {
        match self
            .job_declarator_channel
            .channel_manager_receiver
            .recv()
            .await
        {
            Ok(msg) => {
                debug!("Forwarding message from channel manager to outbound channel.");
                self.job_declarator_channel
                    .outbound_tx
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

    #[instrument(name = "job_declarator_message", skip_all)]
    async fn handle_job_declarator_message(&mut self) -> Result<(), JDCError> {
        let read_frame = self.job_declarator_channel.inbound_rx.recv().await?;
        match read_frame {
            EitherFrame::Sv2(sv2_frame) => {
                debug!("Received SV2 frame from job declarator.");

                let std_frame: StdFrame = sv2_frame;
                let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                    std_frame.clone().into();
                let (message_type, mut payload, parsed_message) = message_from_frame(&mut frame)
                    .map_err(|e| {
                        error!(error=?e, "Failed to parse SV2 frame.");
                        e
                    })?;

                match parsed_message {
                    AnyMessage::Common(_) => {
                        info!(
                            ?message_type,
                            "Handling common message from job declarator."
                        );
                        self.handle_common_message_from_server(message_type, &mut payload)
                            .await?;
                    }
                    AnyMessage::JobDeclaration(_) => {
                        debug!(
                            ?message_type,
                            "Forwarding JobDeclaration message to channel manager."
                        );
                        let frame_to_forward = EitherFrame::Sv2(std_frame);
                        self.job_declarator_channel
                            .channel_manager_sender
                            .send(frame_to_forward)
                            .await
                            .map_err(|e| {
                                error!(error=?e, "Failed to send JobDeclaration message to channel manager.");
                                JDCError::ChannelErrorSender
                            })?;
                    }
                    _ => {
                        warn!(
                            ?message_type,
                            "Received unsupported message type from job declarator."
                        );
                        return Err(JDCError::UnexpectedMessage);
                    }
                }
            }
            EitherFrame::HandShake(handshake_frame) => {
                debug!(
                    ?handshake_frame,
                    "Received handshake frame from job declarator."
                );
            }
        }
        Ok(())
    }
}
