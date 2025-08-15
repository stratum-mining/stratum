use std::sync::Arc;

use async_channel::{unbounded, Receiver, Sender};
use stratum_common::{
    network_helpers_sv2::noise_stream::{NoiseTcpReadHalf, NoiseTcpStream, NoiseTcpWriteHalf},
    roles_logic_sv2::{
        codec_sv2::{self, Frame, Sv2Frame},
        common_messages_sv2::SetupConnectionSuccess,
        handlers_sv2::HandleCommonMessagesFromClientAsync,
        parsers_sv2::{AnyMessage, IsSv2Message},
        utils::Mutex,
    },
};

use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::{
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{message_from_frame, spawn_io_tasks, EitherFrame, Message, ShutdownMessage, StdFrame},
};

mod message_handler;

pub struct DownstreamData;

#[derive(Clone)]
pub struct DownstreamChannel {
    channel_manager_sender: Sender<EitherFrame>,
    channel_manager_receiver: broadcast::Sender<Message>,
    outbound_tx: Sender<EitherFrame>,
    inbound_rx: Receiver<EitherFrame>,
}

#[derive(Clone)]
pub struct Downstream {
    downstream_data: Arc<Mutex<DownstreamData>>,
    downstream_channel: DownstreamChannel,
}

impl Downstream {
    pub fn new(
        channel_manager_sender: Sender<EitherFrame>,
        channel_manager_receiver: broadcast::Sender<Message>,
        noise_stream: NoiseTcpStream<Message>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Self {
        let (noise_stream_reader, noise_stream_writer) = noise_stream.into_split();
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
        );

        let downstream_channel = DownstreamChannel {
            channel_manager_receiver,
            channel_manager_sender,
            outbound_tx,
            inbound_rx,
        };
        let downstream_data = Arc::new(Mutex::new(DownstreamData));
        Downstream {
            downstream_channel,
            downstream_data,
        }
    }

    pub async fn start(
        mut self,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::Downstream {
            downstream_id: 1,
            tx: status_sender,
        };

        let mut shutdown_rx = notify_shutdown.subscribe();
        self.setup_connection().await;
        task_manager.spawn(async move {
            loop {
                let mut self_clone_1 = self.clone();
                let mut self_clone_2 = self.clone();
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        if let Ok(ShutdownMessage::ShutdownAll) = message {
                            info!("Upstream: received shutdown signal.");
                            break;
                        }
                    }
                    res = self_clone_1.handle_downstream_message() => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = self_clone_2.handle_channel_manager_message() => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }

                }
            }
            warn!("Downstream: unified message loop exited.");
        });
    }

    async fn setup_connection(&mut self) -> Result<(), JDCError> {
        let mut frame = self.downstream_channel.inbound_rx.recv().await?;
        let (message_type, mut payload, parsed_message) = message_from_frame(&mut frame).unwrap();
        match parsed_message {
            AnyMessage::Common(_) => {
                self.handle_common_message_from_client(message_type, &mut payload)
                    .await?;
            }
            _ => {
                return Err(JDCError::UnexpectedMessage);
            }
        }
        Ok(())
    }

    async fn handle_channel_manager_message(mut self) -> Result<(), JDCError> {
        let mut receiver = self.downstream_channel.channel_manager_receiver.subscribe();
        while let Ok(frame) = receiver.recv().await {
            info!("Got message from channel manager: {frame:?}");
            let message_type = frame.message_type();
            let std_frame = StdFrame::from_message(frame, message_type, 0, true).unwrap();
            self.downstream_channel
                .outbound_tx
                .send(std_frame.into())
                .await
                .map_err(|e| {
                    error!("Upstream connection closed: {:?}", e);
                    JDCError::CodecNoise(
                        codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage,
                    )
                })?;
        }
        Ok(())
    }

    async fn handle_downstream_message(mut self) -> Result<(), JDCError> {
        error!("I am in handle Downstream message");
        let read_frame = self.downstream_channel.inbound_rx.recv().await?;
        error!("I received a frame in Downstream message: {read_frame:?}");

        match read_frame {
            EitherFrame::Sv2(sv2_frame) => {
                let std_frame: StdFrame = sv2_frame;
                let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                    std_frame.clone().into();
                let (message_type, mut payload, parsed_message) = message_from_frame(&mut frame)?;

                match parsed_message {
                    AnyMessage::Common(_) => {
                        self.handle_common_message_from_client(message_type, &mut payload)
                            .await?;
                    }
                    AnyMessage::Mining(_) => {
                        let frame_to_forward = EitherFrame::Sv2(std_frame);
                        self.downstream_channel
                            .channel_manager_sender
                            .send(frame_to_forward)
                            .await
                            .map_err(
                                |e: async_channel::SendError<
                                    Frame<AnyMessage<'static>, buffer_sv2::Slice>,
                                >| {
                                    error!(
                                        "Failed to send mining message to channel manager: {:?}",
                                        e
                                    );
                                    JDCError::ChannelErrorSender
                                },
                            )?;
                    }
                    _ => {
                        error!("Received unsupported message type from upstream.");
                        return Err(JDCError::UnexpectedMessage);
                    }
                }
            }
            EitherFrame::HandShake(handshake_frame) => {
                debug!("Received handshake frame: {:?}", handshake_frame);
            }
        }
        Ok(())
    }
}
