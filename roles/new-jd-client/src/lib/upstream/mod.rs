use std::{net::SocketAddr, sync::Arc};

use async_channel::{unbounded, Receiver, Sender};
use key_utils::Secp256k1PublicKey;
use stratum_common::{
    network_helpers_sv2::{
        noise_connection::Connection,
        noise_stream::{NoiseTcpReadHalf, NoiseTcpStream, NoiseTcpWriteHalf},
    },
    roles_logic_sv2::{
        codec_sv2::{
            self, framing_sv2, HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame,
        },
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
        get_setup_connection_message, message_from_frame, spawn_io_tasks, EitherFrame, Message,
        ShutdownMessage, StdFrame,
    },
};

mod message_handler;

pub struct UpstreamData;

#[derive(Clone)]
pub struct UpstreamChannel {
    channel_manager_sender: Sender<EitherFrame>,
    channel_manager_receiver: Receiver<EitherFrame>,
    outbound_tx: Sender<EitherFrame>,
    inbound_rx: Receiver<EitherFrame>,
}

#[derive(Clone)]
pub struct Upstream {
    upstream_data: Arc<Mutex<UpstreamData>>,
    upstream_channel: UpstreamChannel,
    upstream_addr: SocketAddr,
}

impl Upstream {
    pub async fn new(
        upstreams: &(SocketAddr, SocketAddr, Secp256k1PublicKey),
        channel_manager_sender: Sender<EitherFrame>,
        channel_manager_receiver: Receiver<EitherFrame>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Result<Self, JDCError> {
        let (addr, _, pubkey) = upstreams;
        let stream = TcpStream::connect(addr).await?;
        info!("Connected to upstream at {}", addr);
        let initiator = Initiator::from_raw_k(pubkey.into_bytes())?;
        info!("Begin with noise setup in upstream connection");
        let (noise_stream_reader, noise_stream_writer) =
            NoiseTcpStream::<Message>::new(stream, HandshakeRole::Initiator(initiator))
                .await?
                .into_split();

        let status_sender = StatusSender::Upstream(status_sender);
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
        );

        info!("Noise setup done  in upstream connection");
        let upstream_data = Arc::new(Mutex::new(UpstreamData));
        let upstream_channel = UpstreamChannel {
            channel_manager_receiver,
            channel_manager_sender,
            outbound_tx,
            inbound_rx,
        };
        // drop(shutdown_complete_tx);
        return Ok(Upstream {
            upstream_data,
            upstream_channel,
            upstream_addr: addr.clone(),
        });
    }

    #[instrument(
        name = "setup_connection",
        skip_all,
        fields(
            min_version = min_version,
            max_version = max_version,
        )
    )]
    pub async fn setup_connection(
        &mut self,
        min_version: u16,
        max_version: u16,
    ) -> Result<(), JDCError> {
        info!("Upstream: initiating SV2 handshake...");
        let setup_connection = get_setup_connection_message(min_version, max_version, true)?;
        debug!(?setup_connection, "Prepared `SetupConnection` message");
        let sv2_frame: StdFrame = Message::Common(setup_connection.into()).try_into()?;
        debug!(?sv2_frame, "Encoded `SetupConnection` frame");

        // Send SetupConnection
        if let Err(e) = self
            .upstream_channel
            .outbound_tx
            .send(sv2_frame.into())
            .await
        {
            error!(?e, "Failed to send `SetupConnection` frame to upstream");
            return Err(JDCError::CodecNoise(
                codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage,
            ));
        }
        info!("Sent `SetupConnection` to upstream, awaiting response...");

        let incoming_frame = match self.upstream_channel.inbound_rx.recv().await {
            Ok(frame) => {
                debug!(?frame, "Received raw inbound frame during handshake");
                frame
            }
            Err(e) => {
                error!(?e, "Upstream closed connection during handshake");
                return Err(JDCError::CodecNoise(
                    codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage,
                ));
            }
        };

        let mut incoming: StdFrame = incoming_frame.try_into()?;
        debug!(?incoming, "Decoded inbound handshake frame");

        let message_type = incoming
            .get_header()
            .ok_or(framing_sv2::Error::ExpectedHandshakeFrame)?
            .msg_type();

        info!(?message_type, "Dispatching inbound handshake message");
        self.handle_common_message_from_server(message_type, incoming.payload())
            .await?;
        Ok(())
    }

    #[instrument(skip_all, fields(
        upstream_addr = ?self.upstream_addr,
    ))]
    pub async fn start(
        mut self,
        min_version: u16,
        max_version: u16,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::Upstream(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        if let Err(e) = self.setup_connection(min_version, max_version).await {
            error!(error = ?e, "Upstream: connection setup failed.");
            return;
        }

        task_manager.spawn(async move {
            let mut self_clone_1 = self.clone();
            let mut self_clone_2 = self.clone();
            loop {
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                info!("Upstream: received shutdown signal.");
                                break;
                            }
                            Ok(ShutdownMessage::JobDeclaratorShutdown) => {
                                info!("Upstream: Received Job declarator shutdown.");
                                break;
                            }
                            Err(_) => {
                                warn!("Upstream: shutdown channel closed unexpectedly.");
                                break;
                            }
                            _ => {}
                        }
                    }
                    res = self_clone_1.handle_pool_message() => {
                        if let Err(e) = res {
                            error!(error = ?e, "Upstream: error handling pool message.");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = self_clone_2.handle_channel_manager_message() => {
                        if let Err(e) = res {
                            error!(error = ?e, "Upstream: error handling channel manager message.");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }

                }
            }
            warn!("Upstream: unified message loop exited.");
        }.instrument(Span::current()));
    }

    #[instrument(name = "pool_message", skip_all)]
    async fn handle_pool_message(&mut self) -> Result<(), JDCError> {
        let read_frame = self.upstream_channel.inbound_rx.recv().await?;
        match read_frame {
            EitherFrame::Sv2(sv2_frame) => {
                debug!("Received SV2 frame from upstream.");

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
                        info!(?message_type, "Handling common message from Upstream.");
                        self.handle_common_message_from_server(message_type, &mut payload)
                            .await?;
                    }
                    AnyMessage::Mining(_) => {
                        debug!(
                            ?message_type,
                            "Forwarding mining message to channel manager."
                        );
                        let frame_to_forward = EitherFrame::Sv2(std_frame);
                        self.upstream_channel
                            .channel_manager_sender
                            .send(frame_to_forward)
                            .await
                            .map_err(|e| {
                                error!(error=?e, "Failed to send mining message to channel manager.");
                                JDCError::ChannelErrorSender
                            })?;
                    }
                    _ => {
                        warn!(
                            ?message_type,
                            "Received unsupported message type from upstream."
                        );
                        return Err(JDCError::UnexpectedMessage);
                    }
                }
            }
            EitherFrame::HandShake(handshake_frame) => {
                debug!(?handshake_frame, "Received handshake frame.");
            }
        }

        Ok(())
    }

    #[instrument(name = "channel_manager_message", skip_all)]
    async fn handle_channel_manager_message(&mut self) -> Result<(), JDCError> {
        match self.upstream_channel.channel_manager_receiver.recv().await {
            Ok(msg) => {
                debug!("Received message from channel manager, forwarding upstream.");
                self.upstream_channel
                    .outbound_tx
                    .send(msg)
                    .await
                    .map_err(|e| {
                        error!(error=?e, "Failed to send outbound message to upstream.");
                        JDCError::CodecNoise(
                            codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage,
                        )
                    })?;
            }
            Err(e) => {
                warn!(error=?e, "Channel manager receiver closed or errored.");
            }
        }
        Ok(())
    }
}
