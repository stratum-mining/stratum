use std::sync::Arc;

use async_channel::{Receiver, Sender};
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
    utils::{message_from_frame, EitherFrame, Message, ShutdownMessage, StdFrame},
};

mod message_handler;

pub struct DownstreamData {
    noise_stream_reader: Option<NoiseTcpReadHalf<Message>>,
    noise_stream_writer: Option<NoiseTcpWriteHalf<Message>>,
}

#[derive(Clone)]
pub struct DownstreamChannel {
    channel_manager_sender: Sender<EitherFrame>,
    channel_manager_receiver: broadcast::Sender<Message>,
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
    ) -> Self {
        let downstream_channel = DownstreamChannel {
            channel_manager_receiver,
            channel_manager_sender,
        };
        let (noise_stream_reader, noise_stream_writer) = noise_stream.into_split();
        let downstream_data = Arc::new(Mutex::new(DownstreamData {
            noise_stream_reader: Some(noise_stream_reader),
            noise_stream_writer: Some(noise_stream_writer),
        }));
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
        let (mut noise_stream_reader, mut noise_stream_writer) =
            self.downstream_data.super_safe_lock(|data| {
                (
                    data.noise_stream_reader.take().unwrap(),
                    data.noise_stream_writer.take().unwrap(),
                )
            });
        let mut shutdown_rx = notify_shutdown.subscribe();
        self.setup_connection(&mut noise_stream_reader, &mut noise_stream_writer)
            .await;
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
                    res = self_clone_1.handle_downstream_message(&mut noise_stream_reader) => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = self_clone_2.handle_channel_manager_message(&mut noise_stream_writer) => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }

                }
            }
            warn!("Upstream: unified message loop exited.");
        });
    }

    async fn setup_connection(
        &mut self,
        noise_stream_reader: &mut NoiseTcpReadHalf<Message>,
        noise_stream_writer: &mut NoiseTcpWriteHalf<Message>,
    ) -> Result<(), JDCError> {
        let mut frame = noise_stream_reader.read_frame().await?;
        let (message_type, mut payload, parsed_message) = message_from_frame(&mut frame).unwrap();
        match parsed_message {
            AnyMessage::Common(_) => {
                self.handle_common_message(message_type, &mut payload)
                    .await?;
                // need to look at this
            }
            _ => {
                return Err(JDCError::UnexpectedMessage);
            }
        }
        Ok(())
    }

    async fn handle_channel_manager_message(
        mut self,
        noise_stream_writer: &mut NoiseTcpWriteHalf<Message>,
    ) -> Result<(), JDCError> {
        let mut receiver = self.downstream_channel.channel_manager_receiver.subscribe();
        while let Ok(frame) = receiver.recv().await {
            info!("Got message from channel manager: {frame:?}");
            let message_type = frame.message_type();
            let std_frame = StdFrame::from_message(frame, message_type, 0, true).unwrap();
            noise_stream_writer.write_frame(std_frame.into()).await?;
        }
        Ok(())
    }

    async fn handle_downstream_message(
        mut self,
        noise_stream_reader: &mut NoiseTcpReadHalf<Message>,
    ) -> Result<(), JDCError> {
        let read_frame = noise_stream_reader.read_frame().await?;
        match read_frame {
            EitherFrame::Sv2(sv2_frame) => {
                let std_frame: StdFrame = sv2_frame;
                let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                    std_frame.clone().into();
                let (message_type, mut payload, parsed_message) = message_from_frame(&mut frame)?;

                match parsed_message {
                    AnyMessage::Common(_) => {
                        self.handle_common_message(message_type, &mut payload)
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
