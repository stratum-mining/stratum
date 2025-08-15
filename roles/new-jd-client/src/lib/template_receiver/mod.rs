use std::{
    net::{Ipv4Addr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

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
use tracing::{debug, error, info, warn};

use crate::{
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::{self, TaskManager},
    utils::{
        get_setup_connection_message_tp, message_from_frame, spawn_io_tasks, EitherFrame, Message,
        ShutdownMessage, StdFrame,
    },
};

mod message_handler;

pub struct TemplateReceiverData;

#[derive(Clone)]
pub struct TemplateReceiverChannel {
    channel_manager_sender: Sender<EitherFrame>,
    channel_manager_receiver: Receiver<EitherFrame>,
    outbound_tx: Sender<EitherFrame>,
    inbound_rx: Receiver<EitherFrame>,
}

#[derive(Clone)]
pub struct TemplateReceiver {
    template_receiver_data: Arc<Mutex<TemplateReceiverData>>,
    template_receiver_channel: TemplateReceiverChannel,
}

impl TemplateReceiver {
    pub async fn new(
        tp_address: String,
        public_key: Option<Secp256k1PublicKey>,
        channel_manager_receiver: Receiver<EitherFrame>,
        channel_manager_sender: Sender<EitherFrame>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: tokio::sync::mpsc::Sender<()>,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Result<TemplateReceiver, JDCError> {
        const MAX_RETRIES: usize = 3;

        for attempt in 1..=MAX_RETRIES {
            info!(
                "Connecting to template provider: attempt {}/{}",
                attempt, MAX_RETRIES
            );

            let initiator = match public_key {
                Some(pub_key) => Initiator::from_raw_k(pub_key.into_bytes()),
                None => Initiator::without_pk(),
            }?;

            match TcpStream::connect(tp_address.as_str()).await {
                Ok(stream) => {
                    let (noise_stream_reader, noise_stream_writer) =
                        NoiseTcpStream::<Message>::new(stream, HandshakeRole::Initiator(initiator))
                            .await?
                            .into_split();

                    let status_sender = StatusSender::TemplateReceiver(status_sender);
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

                    let template_receiver_data = Arc::new(Mutex::new(TemplateReceiverData));
                    let template_receiver_channel = TemplateReceiverChannel {
                        channel_manager_receiver,
                        channel_manager_sender,
                        inbound_rx,
                        outbound_tx,
                    };

                    return Ok(TemplateReceiver {
                        template_receiver_channel,
                        template_receiver_data,
                    });
                }
                Err(e) => {
                    warn!(
                        "Failed to connect to template provider (attempt {}/{}): {:?}",
                        attempt, MAX_RETRIES, e
                    );

                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }

        Err(JDCError::Shutdown)
    }

    pub async fn start(
        mut self,
        socket_address: String,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::TemplateReceiver(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        info!("Initialized state for starting template receiver");
        self.setup_connection(socket_address).await;

        info!("Setup Connection done. connection with template receiver is now done");
        task_manager.spawn(async move {
            loop {
                let mut self_clone_1 = self.clone();
                let self_clone_2 = self.clone();
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        if let Ok(ShutdownMessage::ShutdownAll) = message {
                            info!("Template Receiver: received shutdown signal");
                            break;
                        }
                    }
                    res = self_clone_1.handle_template_provider_message() => {
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
                    },
                }
            }
            warn!("TemplateReceiver: unified message loop exited.");
        });
    }

    pub async fn handle_template_provider_message(&mut self) -> Result<(), JDCError> {
        error!("I am in handle template provider message");
        let read_frame = self.template_receiver_channel.inbound_rx.recv().await?;
        error!("I received a frame in template provider message: {read_frame:?}");
        match read_frame {
            EitherFrame::Sv2(sv2_frame) => {
                let std_frame: StdFrame = sv2_frame;
                let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                    std_frame.clone().into();
                let (message_type, mut payload, parsed_message) = message_from_frame(&mut frame)?;

                match parsed_message {
                    AnyMessage::Common(_) => {
                        self.handle_common_message_from_server(message_type, &mut payload)
                            .await?;
                    }
                    AnyMessage::TemplateDistribution(_) => {
                        let frame_to_forward = EitherFrame::Sv2(std_frame);
                        self.template_receiver_channel
                            .channel_manager_sender
                            .send(frame_to_forward)
                            .await
                            .map_err(|e| {
                                error!("Failed to send mining message to channel manager: {:?}", e);
                                JDCError::ChannelErrorSender
                            })?;
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

    pub async fn handle_channel_manager_message(&self) -> Result<(), JDCError> {
        while let Ok(msg) = self
            .template_receiver_channel
            .channel_manager_receiver
            .recv()
            .await
        {
            self.template_receiver_channel
                .outbound_tx
                .send(msg)
                .await
                .map_err(|e| {
                    error!("Failed to send mining message to channel manager: {:?}", e);
                    JDCError::ChannelErrorSender
                })?;
        }
        Ok(())
    }

    pub async fn setup_connection(&mut self, socket_address: String) -> Result<(), JDCError> {
        let socket_iter = socket_address.split_once(":").unwrap();
        let socket_address = SocketAddr::new(
            std::net::IpAddr::V4(Ipv4Addr::from_str(socket_iter.0).unwrap()),
            socket_iter.1.parse::<u16>().unwrap(),
        );
        let setup_connection = get_setup_connection_message_tp(socket_address);
        let sv2_frame: StdFrame = Message::Common(setup_connection.into()).try_into()?;
        self.template_receiver_channel
            .outbound_tx
            .send(sv2_frame.into())
            .await;

        let incoming_frame = self
            .template_receiver_channel
            .inbound_rx
            .recv()
            .await
            .map_err(|e| {
                error!("Upstream connection closed: {:?}", e);
                JDCError::CodecNoise(codec_sv2::noise_sv2::Error::ExpectedIncomingHandshakeMessage)
            })?;

        let mut incoming: StdFrame = incoming_frame.try_into()?;

        let message_type = incoming
            .get_header()
            .ok_or(framing_sv2::Error::ExpectedHandshakeFrame)?
            .msg_type();

        self.handle_common_message_from_server(message_type, incoming.payload())
            .await?;
        Ok(())
    }
}
