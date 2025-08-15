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
use tracing::{debug, error, info, warn};

use crate::{
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{
        get_setup_connection_message_jds, message_from_frame, spawn_io_tasks, EitherFrame, Message,
        ShutdownMessage, StdFrame,
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
    pub async fn init(
        upstreams: &(SocketAddr, SocketAddr, Secp256k1PublicKey),
        channel_manager_sender: Sender<EitherFrame>,
        channel_manager_receiver: Receiver<EitherFrame>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        task_manager: Arc<TaskManager>,
        status_sender: Sender<Status>,
    ) -> Result<Self, JDCError> {
        let (_, addr, pubkey) = upstreams;
        info!("Trying to connect to JD Server at {addr}");
        let stream = TcpStream::connect(addr).await?;
        info!("Connected to JD Server at {}", addr);
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

    pub async fn start(
        mut self,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::JobDeclarator(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        self.setup_connection().await;

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
                    res = self_clone_1.handle_job_declarator_message() => {
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

    pub async fn setup_connection(&mut self) -> Result<(), JDCError> {
        info!("Upstream: initiating SV2 handshake...");
        let setup_connection = get_setup_connection_message_jds(&self.socket_address);
        let sv2_frame: StdFrame = Message::Common(setup_connection.into()).try_into()?;
        self.job_declarator_channel
            .outbound_tx
            .send(sv2_frame.into())
            .await;

        let incoming_frame = self
            .job_declarator_channel
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

    async fn handle_channel_manager_message(&self) -> Result<(), JDCError> {
        while let Ok(msg) = self
            .job_declarator_channel
            .channel_manager_receiver
            .recv()
            .await
        {
            self.job_declarator_channel
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

    async fn handle_job_declarator_message(&mut self) -> Result<(), JDCError> {
        let read_frame = self.job_declarator_channel.inbound_rx.recv().await?;
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
                    AnyMessage::JobDeclaration(_) => {
                        let frame_to_forward = EitherFrame::Sv2(std_frame);
                        self.job_declarator_channel
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
}
