use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use async_channel::{unbounded, Receiver, Sender};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use stratum_common::{
    network_helpers_sv2::noise_stream::NoiseTcpStream,
    roles_logic_sv2::{
        codec_sv2::{self, Responder},
        handlers_sv2::{
            HandleJobDeclarationMessagesFromServerAsync, HandleMiningMessagesFromClientAsync,
            HandleMiningMessagesFromServerAsync, HandleTemplateDistributionMessagesFromServerAsync,
        },
        parsers_sv2::AnyMessage,
        utils::Mutex,
    },
};
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc},
};
use tracing::{debug, error, info};

use crate::{
    downstream::Downstream,
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{message_from_frame, EitherFrame, Message, ShutdownMessage, StdFrame},
};
mod downstream_message_handler;
mod jd_message_handler;
mod template_message_handler;
mod upstream_message_handler;

pub struct ChannelManagerData {
    downstream: HashMap<u32, Downstream>,
}

#[derive(Clone)]
pub struct ChannelManagerChannel {
    upstream_sender: Sender<EitherFrame>,
    upstream_receiver: Receiver<EitherFrame>,
    jd_sender: Sender<EitherFrame>,
    jd_receiver: Receiver<EitherFrame>,
    tp_sender: Sender<EitherFrame>,
    tp_receiver: Receiver<EitherFrame>,
    downstream_sender: broadcast::Sender<Message>,
    downstream_receiver: Receiver<EitherFrame>,
}

#[derive(Clone)]
pub struct ChannelManager {
    channel_manager_data: Arc<Mutex<ChannelManagerData>>,
    channel_manager_channel: ChannelManagerChannel,
}

impl ChannelManager {
    pub async fn new(
        task_manager: Arc<TaskManager>,
        upstream_sender: Sender<EitherFrame>,
        upstream_receiver: Receiver<EitherFrame>,
        jd_sender: Sender<EitherFrame>,
        jd_receiver: Receiver<EitherFrame>,
        tp_sender: Sender<EitherFrame>,
        tp_receiver: Receiver<EitherFrame>,
        downstream_sender: broadcast::Sender<Message>,
        downstream_receiver: Receiver<EitherFrame>,
        status_sender: Sender<Status>,
    ) -> Result<Self, JDCError> {
        let channel_manager_data = Arc::new(Mutex::new(ChannelManagerData {
            downstream: HashMap::new(),
        }));
        let channel_manager_channel = ChannelManagerChannel {
            upstream_sender,
            upstream_receiver,
            jd_receiver,
            jd_sender,
            tp_receiver,
            tp_sender,
            downstream_receiver,
            downstream_sender,
        };
        let channel_manager = ChannelManager {
            channel_manager_data,
            channel_manager_channel,
        };

        Ok(channel_manager)
    }

    pub async fn start_downstream_server(
        self,
        authority_public_key: Secp256k1PublicKey,
        authority_secret_key: Secp256k1SecretKey,
        cert_validity_sec: u64,
        listening_address: SocketAddr,
        task_manager: Arc<TaskManager>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        channel_manager_sender: Sender<EitherFrame>,
        channel_manager_receiver: broadcast::Sender<Message>,
    ) -> Result<(), JDCError> {
        let server = TcpListener::bind(listening_address).await?;
        let task_manager_clone = task_manager.clone();
        task_manager.spawn(async move {
            while let Ok((stream, socket_address)) = server.accept().await {
                info!("Received connection request from socket address: {socket_address:?}");
                let responder = Responder::from_authority_kp(
                    &authority_public_key.into_bytes(),
                    &authority_secret_key.into_bytes(),
                    std::time::Duration::from_secs(cert_validity_sec),
                )
                .unwrap();
                let noise_stream = NoiseTcpStream::<Message>::new(
                    stream,
                    stratum_common::roles_logic_sv2::codec_sv2::HandshakeRole::Responder(responder),
                )
                .await
                .unwrap();
                let downstream = Downstream::new(
                    channel_manager_sender.clone(),
                    channel_manager_receiver.clone(),
                    noise_stream,
                    notify_shutdown.clone(),
                    shutdown_complete_tx.clone(),
                    task_manager_clone.clone(),
                    status_sender.clone(),
                );

                downstream
                    .start(
                        notify_shutdown.clone(),
                        shutdown_complete_tx.clone(),
                        status_sender.clone(),
                        task_manager_clone.clone(),
                    )
                    .await;
            }
        });
        Ok(())
    }

    pub async fn start(
        mut self,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::ChannelManager(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        task_manager.spawn(async move {
            loop {
                let mut self_clone_1 = self.clone();
                let mut self_clone_2 = self.clone();
                let mut self_clone_3 = self.clone();
                let mut self_clone_4 = self.clone();
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        if let Ok(ShutdownMessage::ShutdownAll) = message {
                            info!("Template Receiver: received shutdown signal");
                            break;
                        }
                    }
                    res = self_clone_1.handle_jds_message() => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = self_clone_2.handle_pool_message() => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = self_clone_3.handle_template_receiver_message() => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = self_clone_4.handle_downstreams_message() => {
                        if let Err(e) = res {
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                }
            }
        });
    }

    async fn handle_jds_message(&mut self) -> Result<(), JDCError> {
        while let Ok(read_frame) = self.channel_manager_channel.jd_receiver.recv().await {
            match read_frame {
                EitherFrame::Sv2(sv2_frame) => {
                    let std_frame: StdFrame = sv2_frame;
                    let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                        std_frame.clone().into();
                    let (message_type, mut payload, parsed_message) =
                        message_from_frame(&mut frame)?;

                    match parsed_message {
                        AnyMessage::JobDeclaration(_) => {
                            self.handle_job_declaration_message_from_server(
                                message_type,
                                &mut payload,
                            )
                            .await;
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
        }
        Ok(())
    }

    async fn handle_pool_message(&mut self) -> Result<(), JDCError> {
        while let Ok(read_frame) = self.channel_manager_channel.upstream_receiver.recv().await {
            match read_frame {
                EitherFrame::Sv2(sv2_frame) => {
                    let std_frame: StdFrame = sv2_frame;
                    let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                        std_frame.clone().into();
                    let (message_type, mut payload, parsed_message) =
                        message_from_frame(&mut frame)?;

                    match parsed_message {
                        AnyMessage::Mining(_) => {
                            self.handle_mining_message_from_server(message_type, &mut payload)
                                .await;
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
        }
        Ok(())
    }

    async fn handle_template_receiver_message(&mut self) -> Result<(), JDCError> {
        while let Ok(read_frame) = self.channel_manager_channel.tp_receiver.recv().await {
            match read_frame {
                EitherFrame::Sv2(sv2_frame) => {
                    let std_frame: StdFrame = sv2_frame;
                    let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                        std_frame.clone().into();
                    let (message_type, mut payload, parsed_message) =
                        message_from_frame(&mut frame)?;

                    match parsed_message {
                        AnyMessage::TemplateDistribution(_) => {
                            self.handle_template_distribution_message_from_server(
                                message_type,
                                &mut payload,
                            )
                            .await;
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
        }
        Ok(())
    }

    async fn handle_downstreams_message(&mut self) -> Result<(), JDCError> {
        while let Ok(read_frame) = self
            .channel_manager_channel
            .downstream_receiver
            .recv()
            .await
        {
            match read_frame {
                EitherFrame::Sv2(sv2_frame) => {
                    let std_frame: StdFrame = sv2_frame;
                    let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                        std_frame.clone().into();
                    let (message_type, mut payload, parsed_message) =
                        message_from_frame(&mut frame)?;

                    match parsed_message {
                        AnyMessage::Mining(_) => {
                            self.handle_mining_message_from_client(message_type, &mut payload)
                                .await;
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
        }
        Ok(())
    }
}
