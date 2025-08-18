use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use async_channel::{unbounded, Receiver, Sender};
use config_helpers_sv2::CoinbaseRewardScript;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use stratum_common::{
    network_helpers_sv2::noise_stream::NoiseTcpStream,
    roles_logic_sv2::{
        channels_sv2::server::{
            extended::ExtendedChannel, group::GroupChannel, standard::StandardChannel,
        },
        codec_sv2::{
            self,
            binary_sv2::{Seq064K, B016M},
            Responder, Sv2Frame,
        },
        handlers_sv2::{
            HandleJobDeclarationMessagesFromServerAsync, HandleMiningMessagesFromClientAsync,
            HandleMiningMessagesFromServerAsync, HandleTemplateDistributionMessagesFromServerAsync,
        },
        job_declaration_sv2::{
            AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        },
        mining_sv2::{ExtendedExtranonce, FULL_EXTRANONCE_LEN},
        parsers_sv2::{AnyMessage, IsSv2Message, JobDeclaration, Mining, TemplateDistribution},
        template_distribution_sv2::{
            CoinbaseOutputConstraints, NewTemplate, SetNewPrevHash as SetNewPrevHashTdp,
            SubmitSolution,
        },
        utils::{Id as IdFactory, Mutex},
        Vardiff,
    },
};
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc},
};
use tracing::{debug, error, info};

use crate::{
    config::JobDeclaratorClientConfig,
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

#[derive(Clone, Debug)]
pub struct LastDeclareJob {
    declare_job: DeclareMiningJob<'static>,
    template: NewTemplate<'static>,
    prev_hash: Option<SetNewPrevHashTdp<'static>>,
    coinbase_pool_output: Vec<u8>,
    tx_list: Vec<Vec<u8>>,
}

pub struct ChannelManagerData {
    // downstream_id, downstream object
    downstream: HashMap<u32, Downstream>,
    extranonce_prefix_factory_extended: ExtendedExtranonce,
    extranonce_prefix_factory_standard: ExtendedExtranonce,
    request_id_factory: IdFactory,
    downstream_id_factory: IdFactory,
    last_future_template: Option<NewTemplate<'static>>,
    last_new_prev_hash: Option<SetNewPrevHashTdp<'static>>,
    allocate_tokens: Option<AllocateMiningJobTokenSuccess<'static>>,
    template_store: HashMap<u64, NewTemplate<'static>>,
    last_declare_job_store: HashMap<u32, LastDeclareJob>,
    job_id_to_template: HashMap<u32, LastDeclareJob>,
    coinbase_outputs: Vec<u8>,
    channel_id_to_downstream_id: HashMap<u32, u32>,
    upstream_channel_id: u32,
}

#[derive(Clone)]
pub struct ChannelManagerChannel {
    upstream_sender: Sender<EitherFrame>,
    upstream_receiver: Receiver<EitherFrame>,
    jd_sender: Sender<EitherFrame>,
    jd_receiver: Receiver<EitherFrame>,
    tp_sender: Sender<EitherFrame>,
    tp_receiver: Receiver<EitherFrame>,
    downstream_sender: broadcast::Sender<(u32, Message)>,
    downstream_receiver: Receiver<(u32, EitherFrame)>,
}

#[derive(Clone)]
pub struct ChannelManager {
    channel_manager_data: Arc<Mutex<ChannelManagerData>>,
    channel_manager_channel: ChannelManagerChannel,
    pool_tag_string: Option<String>,
    miner_tag_string: String,
    share_batch_size: usize,
    shares_per_minute: f32,
    coinbase_reward_script: CoinbaseRewardScript,
}

impl ChannelManager {
    pub async fn new(
        config: JobDeclaratorClientConfig,
        task_manager: Arc<TaskManager>,
        upstream_sender: Sender<EitherFrame>,
        upstream_receiver: Receiver<EitherFrame>,
        jd_sender: Sender<EitherFrame>,
        jd_receiver: Receiver<EitherFrame>,
        tp_sender: Sender<EitherFrame>,
        tp_receiver: Receiver<EitherFrame>,
        downstream_sender: broadcast::Sender<(u32, Message)>,
        downstream_receiver: Receiver<(u32, EitherFrame)>,
        status_sender: Sender<Status>,
    ) -> Result<Self, JDCError> {
        let range_1_start = 0;
        let range_1_end = 8;

        // range_0 is not used here
        let range_0 = std::ops::Range {
            start: range_1_start,
            end: range_1_start,
        };
        let range_1 = std::ops::Range {
            start: range_1_start,
            end: range_1_end,
        };
        let range_2 = std::ops::Range {
            start: range_1_end,
            end: FULL_EXTRANONCE_LEN,
        };

        // static prefix we can tackle later
        let extranonce_prefix_factory_extended =
            ExtendedExtranonce::new(range_0.clone(), range_1.clone(), range_2.clone(), None)
                .expect("Failed to create ExtendedExtranonce with valid ranges");
        let extranonce_prefix_factory_standard =
            ExtendedExtranonce::new(range_0.clone(), range_1.clone(), range_2.clone(), None)
                .expect("Failed to create ExtendedExtranonce with valid ranges");

        // make share batch size and share per minute configurable by config
        let channel_manager_data = Arc::new(Mutex::new(ChannelManagerData {
            downstream: HashMap::new(),
            extranonce_prefix_factory_extended,
            extranonce_prefix_factory_standard,
            downstream_id_factory: IdFactory::new(),
            request_id_factory: IdFactory::new(),
            last_future_template: None,
            last_new_prev_hash: None,
            allocate_tokens: None,
            template_store: HashMap::new(),
            last_declare_job_store: HashMap::new(),
            job_id_to_template: HashMap::new(),
            coinbase_outputs: vec![],
            channel_id_to_downstream_id: HashMap::new(),
            upstream_channel_id: 1,
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
            share_batch_size: 1,
            shares_per_minute: 10.0,
            pool_tag_string: Some("pool".to_string()),
            miner_tag_string: "miner".to_string(),
            coinbase_reward_script: config.coinbase_reward_script,
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
        channel_manager_sender: Sender<(u32, EitherFrame)>,
        channel_manager_receiver: broadcast::Sender<(u32, Message)>,
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
                let downstream_id = self
                    .channel_manager_data
                    .super_safe_lock(|data| data.downstream_id_factory.next());
                let downstream = Downstream::new(
                    downstream_id,
                    channel_manager_sender.clone(),
                    channel_manager_receiver.clone(),
                    noise_stream,
                    notify_shutdown.clone(),
                    shutdown_complete_tx.clone(),
                    task_manager_clone.clone(),
                    status_sender.clone(),
                );

                self.channel_manager_data.super_safe_lock(|data| {
                    data.downstream.insert(downstream_id, downstream.clone());
                });

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
        // ask gitgab on default tokens to generate.
        self.allocate_tokens(1).await;

        task_manager.spawn(async move {
            loop {
                let mut self_clone_1 = self.clone();
                let mut self_clone_2 = self.clone();
                let mut self_clone_3 = self.clone();
                let mut self_clone_4 = self.clone();
                let mut self_clone_5 = self.clone();
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                info!("Channel Manager: received shutdown signal");
                                break;
                            }
                            Ok(ShutdownMessage::DownstreamShutdown(downstream_id)) => {
                                info!("Channel Manager: received downstream {downstream_id} shutdown signal");

                                self_clone_5.channel_manager_data.super_safe_lock(|cm_data| {
                                    if let Some(downstream) = cm_data.downstream.remove(&downstream_id) {
                                        downstream.downstream_data.super_safe_lock(|ds_data| {
                                            for k in ds_data.standard_channels.keys().chain(ds_data.extended_channels.keys()) {
                                                cm_data.channel_id_to_downstream_id.remove(k);
                                            }
                                        });
                                    }
                                });
                            }
                            _ => {}
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

    // we will make this lean
    async fn handle_downstreams_message(&mut self) -> Result<(), JDCError> {
        while let Ok((downstream_id, read_frame)) = self
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
                        AnyMessage::Mining(m) => match m {
                            Mining::OpenExtendedMiningChannel(mut x) => {
                                let user_identity = format!(
                                    "{}#{}",
                                    x.user_identity.as_utf8_or_hex(),
                                    downstream_id
                                );
                                x.user_identity = user_identity.try_into().unwrap();
                                let mining_message = Mining::OpenExtendedMiningChannel(x);

                                let mut any_message = mining_message.into_static();
                                let sv2_frame: Sv2Frame<Mining<'static>, Vec<u8>> =
                                    Sv2Frame::from_message(any_message, message_type, 0, false)
                                        .unwrap();

                                let mut serialized_frame = vec![0u8; sv2_frame.encoded_length()];
                                sv2_frame
                                    .serialize(&mut serialized_frame)
                                    .expect("Failed to serialize the frame");
                                let mut deserialized_frame =
                                    Sv2Frame::<Mining<'static>, Vec<u8>>::from_bytes(
                                        serialized_frame,
                                    )
                                    .expect("Failed to deserialize frame");

                                let mut payload = deserialized_frame.payload();

                                self.handle_mining_message_from_client(message_type, &mut payload)
                                    .await;
                            }
                            Mining::OpenStandardMiningChannel(mut x) => {
                                let user_identity =
                                    format!("{}#{}", x.user_identity, downstream_id);
                                x.user_identity = user_identity.try_into().unwrap();
                                let mining_message = Mining::OpenStandardMiningChannel(x);
                                let mut any_message = mining_message.into_static();
                                let sv2_frame: Sv2Frame<Mining<'static>, Vec<u8>> =
                                    Sv2Frame::from_message(any_message, message_type, 0, false)
                                        .unwrap();

                                let mut serialized_frame = vec![0u8; sv2_frame.encoded_length()];
                                sv2_frame
                                    .serialize(&mut serialized_frame)
                                    .expect("Failed to serialize the frame");
                                let mut deserialized_frame =
                                    Sv2Frame::<Mining<'static>, Vec<u8>>::from_bytes(
                                        serialized_frame,
                                    )
                                    .expect("Failed to deserialize frame");

                                let mut payload = deserialized_frame.payload();
                                self.handle_mining_message_from_client(message_type, &mut payload)
                                    .await;
                            }
                            _ => {
                                self.handle_mining_message_from_client(message_type, &mut payload)
                                    .await;
                            }
                        },
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

    async fn allocate_tokens(&self, token_to_allocate: u32) -> Result<(), JDCError> {
        for i in 0..token_to_allocate {
            let request_id = self
                .channel_manager_data
                .super_safe_lock(|data| data.request_id_factory.next());
            /// Ask gitgab about this
            let message = JobDeclaration::AllocateMiningJobToken(AllocateMiningJobToken {
                user_identifier: "jdc".to_string().try_into().unwrap(),
                request_id,
            });
            let frame: StdFrame = AnyMessage::JobDeclaration(message).try_into()?;
            self.channel_manager_channel
                .jd_sender
                .send(frame.into())
                .await
                .unwrap();
        }

        Ok(())
    }
}
