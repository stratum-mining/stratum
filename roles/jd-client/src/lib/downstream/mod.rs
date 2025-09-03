use std::{collections::HashMap, sync::Arc};

use async_channel::{unbounded, Receiver, Sender};
use stratum_common::{
    network_helpers_sv2::noise_stream::NoiseTcpStream,
    roles_logic_sv2::{
        channels_sv2::server::{
            extended::ExtendedChannel, group::GroupChannel, standard::StandardChannel,
        },
        codec_sv2,
        handlers_sv2::HandleCommonMessagesFromClientAsync,
        parsers_sv2::{AnyMessage, CommonMessages, IsSv2Message},
        utils::Mutex,
    },
};

use tokio::sync::broadcast;
use tracing::{debug, error, warn, Instrument, Span};

use crate::{
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{
        message_from_frame, spawn_io_tasks, EitherFrame, Message, SV2Frame, ShutdownMessage,
        StdFrame,
    },
};

mod message_handler;

/// Holds state related to a downstream connection's mining channels.
///
/// This includes:
/// - Whether the downstream requires a standard job (`require_std_job`).
/// - An optional [`GroupChannel`] if group channeling is used.
/// - Active [`ExtendedChannel`]s keyed by channel ID.
/// - Active [`StandardChannel`]s keyed by channel ID.
pub struct DownstreamData {
    pub require_std_job: bool,
    pub group_channels: Option<GroupChannel<'static>>,
    pub extended_channels: HashMap<u32, ExtendedChannel<'static>>,
    pub standard_channels: HashMap<u32, StandardChannel<'static>>,
}

/// Communication layer for a downstream connection.
///
/// Provides the messaging primitives for interacting with the
/// channel manager and the downstream peer:
/// - `channel_manager_sender`: sends frames to the channel manager.
/// - `channel_manager_receiver`: receives messages from the channel manager.
/// - `downstream_sender`: sends frames to the downstream.
/// - `downstream_receiver`: receives frames from the downstream.
#[derive(Clone)]
pub struct DownstreamChannel {
    channel_manager_sender: Sender<(u32, SV2Frame)>,
    channel_manager_receiver: broadcast::Sender<(u32, Message)>,
    downstream_sender: Sender<EitherFrame>,
    downstream_receiver: Receiver<EitherFrame>,
}

/// Represents a downstream client connected to this node.
#[derive(Clone)]
pub struct Downstream {
    pub downstream_data: Arc<Mutex<DownstreamData>>,
    downstream_channel: DownstreamChannel,
    pub downstream_id: u32,
}

impl Downstream {
    /// Creates a new [`Downstream`] instance and spawns the necessary I/O tasks.
    pub fn new(
        downstream_id: u32,
        channel_manager_sender: Sender<(u32, SV2Frame)>,
        channel_manager_receiver: broadcast::Sender<(u32, Message)>,
        noise_stream: NoiseTcpStream<Message>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Self {
        let (noise_stream_reader, noise_stream_writer) = noise_stream.into_split();
        let status_sender = StatusSender::Downstream {
            downstream_id,
            tx: status_sender,
        };
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

        let downstream_channel = DownstreamChannel {
            channel_manager_receiver,
            channel_manager_sender,
            downstream_sender: outbound_tx,
            downstream_receiver: inbound_rx,
        };
        let downstream_data = Arc::new(Mutex::new(DownstreamData {
            require_std_job: false,
            extended_channels: HashMap::new(),
            standard_channels: HashMap::new(),
            group_channels: None,
        }));
        Downstream {
            downstream_channel,
            downstream_data,
            downstream_id,
        }
    }

    /// Starts the downstream loop.
    ///
    /// Responsibilities:
    /// - Performs the initial `SetupConnection` handshake with the downstream.
    /// - Forwards mining-related messages to the channel manager.
    /// - Forwards channel manager messages back to the downstream peer.
    pub async fn start(
        mut self,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::Downstream {
            downstream_id: self.downstream_id,
            tx: status_sender,
        };

        let mut shutdown_rx = notify_shutdown.subscribe();

        // Setup initial connection
        if let Err(e) = self.setup_connection_with_downstream().await {
            error!(?e, "Failed to set up downstream connection");
            handle_error(&status_sender, e).await;
            return;
        }

        let mut receiver = self.downstream_channel.channel_manager_receiver.subscribe();
        task_manager.spawn(async move {
            loop {
                let self_clone_1 = self.clone();
                let downstream_id = self_clone_1.downstream_id;
                let self_clone_2 = self.clone();
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                debug!("Downstream {downstream_id}: Received global shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::DownstreamShutdown(id)) if downstream_id == id => {
                                debug!("Downstream {downstream_id}: Received downstream {id} shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::JobDeclaratorShutdownFallback(_))  => {
                                debug!("Downstream {downstream_id}: Received job declaratorShutdown shutdown");
                                break;
                            }
                            Ok(ShutdownMessage::UpstreamShutdownFallback(_))  => {
                                debug!("Downstream {downstream_id}: Received job Upstream shutdown");
                                break;
                            }
                            _ => {}
                        }
                    }
                    res = self_clone_1.handle_downstream_message() => {
                        if let Err(e) = res {
                            error!(?e, "Error handling downstream message for {downstream_id}");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = self_clone_2.handle_channel_manager_message(&mut receiver) => {
                        if let Err(e) = res {
                            error!(?e, "Error handling channel manager message for {downstream_id}");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }

                }
            }
            warn!("Downstream: unified message loop exited.");
        }.instrument(Span::current()));
    }

    // Performs the initial handshake with a downstream peer.
    async fn setup_connection_with_downstream(&mut self) -> Result<(), JDCError> {
        let mut frame = self.downstream_channel.downstream_receiver.recv().await?;
        let (msg_type, mut payload, parsed) = message_from_frame(&mut frame).map_err(|e| {
            error!(?e, "Failed to parse incoming frame");
            e
        })?;

        if let AnyMessage::Common(CommonMessages::SetupConnection(_)) = parsed {
            self.handle_common_message_from_client(msg_type, &mut payload)
                .await?;
            return Ok(());
        }

        Err(JDCError::UnexpectedMessage)
    }

    // Handles messages sent from the channel manager to this downstream.
    async fn handle_channel_manager_message(
        self,
        receiver: &mut broadcast::Receiver<(u32, AnyMessage<'static>)>,
    ) -> Result<(), JDCError> {
        let (downstream_id, frame) = match receiver.recv().await {
            Ok(msg) => msg,
            Err(e) => {
                warn!(?e, "Broadcast receive failed");
                return Ok(());
            }
        };

        if downstream_id != self.downstream_id {
            debug!(
                ?downstream_id,
                "Message ignored for non-matching downstream"
            );
            return Ok(());
        }

        let message_type = frame.message_type();
        let std_frame = match StdFrame::from_message(frame, message_type, 0, true) {
            Some(f) => f,
            None => {
                debug!("Invalid frame conversion; skipping message");
                return Ok(());
            }
        };

        self.downstream_channel
            .downstream_sender
            .send(std_frame.into())
            .await
            .map_err(|e| {
                error!(?e, "Downstream send failed");
                JDCError::CodecNoise(codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage)
            })?;

        Ok(())
    }

    // Handles incoming messages from the downstream peer.
    async fn handle_downstream_message(self) -> Result<(), JDCError> {
        let read_frame = self.downstream_channel.downstream_receiver.recv().await?;

        match read_frame {
            EitherFrame::Sv2(sv2_frame) => {
                debug!("Received SV2 frame from downstream.");
                self.downstream_channel
                    .channel_manager_sender
                    .send((self.downstream_id, sv2_frame))
                    .await
                    .map_err(|e| {
                        error!(error=?e, "Failed to send mining message to channel manager.");
                        JDCError::ChannelErrorSender
                    })?;
            }
            EitherFrame::HandShake(handshake_frame) => {
                debug!(?handshake_frame, "Received handshake frame");
            }
        }
        Ok(())
    }
}
