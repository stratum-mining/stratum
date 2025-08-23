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
        bitcoin::TxOut,
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
        mining_sv2::{
            ExtendedExtranonce, OpenExtendedMiningChannel, SetCustomMiningJob, FULL_EXTRANONCE_LEN,
        },
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
    select,
    sync::{broadcast, mpsc},
};
use tracing::{debug, error, info, instrument, warn, Instrument, Span};

use crate::{
    config::JobDeclaratorClientConfig,
    downstream::Downstream,
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{
        message_from_frame, AtomicUpstreamState, EitherFrame, Message, ShutdownMessage, StdFrame,
        UpstreamState,
    },
};
mod downstream_message_handler;
mod jd_message_handler;
mod template_message_handler;
mod upstream_message_handler;

#[derive(Clone, Debug)]
pub struct LastDeclareJob {
    mining_job_token: AllocateMiningJobTokenSuccess<'static>,
    declare_job: Option<DeclareMiningJob<'static>>,
    template: NewTemplate<'static>,
    prev_hash: Option<SetNewPrevHashTdp<'static>>,
    custom_job: Option<SetCustomMiningJob<'static>>,
    coinbase_output: Vec<u8>,
    tx_list: Vec<Vec<u8>>,
}

pub struct ChannelManagerData {
    // downstream_id, downstream object
    downstream: HashMap<u32, Downstream>,
    extranonce_prefix_factory_extended: ExtendedExtranonce,
    extranonce_prefix_factory_standard: ExtendedExtranonce,
    request_id_factory: IdFactory,
    downstream_id_factory: IdFactory,
    channel_id_factory: IdFactory,
    last_future_template: Option<NewTemplate<'static>>,
    last_new_prev_hash: Option<SetNewPrevHashTdp<'static>>,
    allocate_tokens: Option<AllocateMiningJobTokenSuccess<'static>>,
    template_store: HashMap<u64, NewTemplate<'static>>,
    last_declare_job_store: HashMap<u32, LastDeclareJob>,
    job_id_to_template: HashMap<u32, LastDeclareJob>,
    template_id_to_upstream_job_id: HashMap<u64, u64>,
    template_id_to_downstream_channel_id_and_job_id: HashMap<u64, (u32, u32)>,
    downstream_channel_id_and_job_id_to_template_id: HashMap<(u32, u32), u64>,
    coinbase_outputs: Vec<u8>,
    channel_id_to_downstream_id: HashMap<u32, u32>,
    upstream_channel: Option<ExtendedChannel<'static>>,
    pool_tag_string: Option<String>,
    pending_downstream_requests: Vec<Mining<'static>>,
}

impl ChannelManagerData {
    #[instrument(skip_all)]
    pub fn reset(&mut self, coinbase_outputs: Vec<u8>) {
        self.downstream.clear();
        self.template_store.clear();
        self.last_declare_job_store.clear();
        self.job_id_to_template.clear();
        self.template_id_to_upstream_job_id.clear();
        self.template_id_to_downstream_channel_id_and_job_id.clear();
        self.downstream_channel_id_and_job_id_to_template_id.clear();
        self.channel_id_to_downstream_id.clear();
        self.pending_downstream_requests.clear();

        self.downstream_id_factory = IdFactory::new();
        self.request_id_factory = IdFactory::new();
        self.channel_id_factory = IdFactory::new();

        let (range_0, range_1, range_2) = {
            let range_1 = 0..8;
            (
                0..range_1.start,
                range_1.clone(),
                range_1.end..FULL_EXTRANONCE_LEN,
            )
        };
        self.extranonce_prefix_factory_extended =
            ExtendedExtranonce::new(range_0.clone(), range_1.clone(), range_2.clone(), None)
                .expect("valid ranges");
        self.extranonce_prefix_factory_standard =
            ExtendedExtranonce::new(range_0, range_1, range_2, None).expect("valid ranges");

        self.last_future_template = None;
        self.last_new_prev_hash = None;
        self.allocate_tokens = None;
        self.upstream_channel = None;
        self.pool_tag_string = None;

        self.coinbase_outputs = coinbase_outputs;
    }
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
    miner_tag_string: String,
    share_batch_size: usize,
    shares_per_minute: f32,
    coinbase_reward_script: CoinbaseRewardScript,
    user_identity: String,
    min_extranonce_size: u16,
    upstream_state: AtomicUpstreamState,
}

impl ChannelManager {
    #[instrument(skip_all)]
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
        coinbase_outputs: Vec<u8>,
    ) -> Result<Self, JDCError> {
        let (range_0, range_1, range_2) = {
            let range_1 = 0..8;
            (
                0..range_1.start,
                range_1.clone(),
                range_1.end..FULL_EXTRANONCE_LEN,
            )
        };

        let make_extranonce_factory = || {
            ExtendedExtranonce::new(range_0.clone(), range_1.clone(), range_2.clone(), None)
                .expect("Failed to create ExtendedExtranonce with valid ranges")
        };

        let extranonce_prefix_factory_extended = make_extranonce_factory();
        let extranonce_prefix_factory_standard = make_extranonce_factory();

        let channel_manager_data = Arc::new(Mutex::new(ChannelManagerData {
            downstream: HashMap::new(),
            extranonce_prefix_factory_extended,
            extranonce_prefix_factory_standard,
            downstream_id_factory: IdFactory::new(),
            request_id_factory: IdFactory::new(),
            channel_id_factory: IdFactory::new(),
            last_future_template: None,
            last_new_prev_hash: None,
            allocate_tokens: None,
            template_store: HashMap::new(),
            last_declare_job_store: HashMap::new(),
            job_id_to_template: HashMap::new(),
            template_id_to_upstream_job_id: HashMap::new(),
            template_id_to_downstream_channel_id_and_job_id: HashMap::new(),
            downstream_channel_id_and_job_id_to_template_id: HashMap::new(),
            coinbase_outputs,
            channel_id_to_downstream_id: HashMap::new(),
            upstream_channel: None,
            pool_tag_string: None,
            pending_downstream_requests: Vec::new(),
        }));

        let channel_manager_channel = ChannelManagerChannel {
            upstream_sender,
            upstream_receiver,
            jd_sender,
            jd_receiver,
            tp_sender,
            tp_receiver,
            downstream_sender,
            downstream_receiver,
        };

        let channel_manager = ChannelManager {
            channel_manager_data,
            channel_manager_channel,
            share_batch_size: config.share_batch_size() as usize,
            shares_per_minute: config.shares_per_minute() as f32,
            miner_tag_string: config.jdc_signature().to_string(),
            coinbase_reward_script: config.coinbase_reward_script.clone(),
            user_identity: config.user_identity().to_string(),
            min_extranonce_size: config.min_extranonce_size(),
            upstream_state: AtomicUpstreamState::new(UpstreamState::NotConnected),
        };

        Ok(channel_manager)
    }

    #[instrument(skip_all, fields(listening_address = %listening_address))]
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
        let server = TcpListener::bind(listening_address).await.map_err(|e| {
            error!(error = ?e, "Failed to bind downstream server at {listening_address}");
            e
        })?;

        let mut shutdown_rx = notify_shutdown.subscribe();

        let task_manager_clone = task_manager.clone();
        task_manager.spawn(async move {

            loop {
                select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                info!("Channel Manager: received shutdown signal");
                                break;
                            }
                            Ok(ShutdownMessage::JobDeclaratorShutdownFallback(_)) => {
                                info!("Downstream Server: received job declarator shutdown signal");
                                break;
                            }
                            Ok(ShutdownMessage::UpstreamShutdownFallback(_)) => {
                                info!("Downstream Server: received upstream shutdown signal");
                                break;
                            }
                            Err(e) => {
                                warn!(error = ?e, "shutdown channel closed unexpectedly");
                                break;
                            }
                            _ => {}
                        }
                    }
                    res = server.accept() => {
                        match res {
                            Ok((stream, socket_address)) => {
                                info!(%socket_address, "New downstream connection");
                                let responder = match Responder::from_authority_kp(
                                    &authority_public_key.into_bytes(),
                                    &authority_secret_key.into_bytes(),
                                    std::time::Duration::from_secs(cert_validity_sec),
                                ) {
                                    Ok(r) => r,
                                    Err(e) => {
                                        error!(error = ?e, "Failed to create responder");
                                        continue;
                                    }
                                };
                                let noise_stream = match NoiseTcpStream::<Message>::new(
                                    stream,
                                    stratum_common::roles_logic_sv2::codec_sv2::HandshakeRole::Responder(responder),
                                )
                                .await
                                {
                                    Ok(ns) => ns,
                                    Err(e) => {
                                        error!(error = ?e, "Noise handshake failed");
                                        continue;
                                    }
                                };

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

                                Err(e) => {
                                    error!(error = ?e, "Failed to accept new downstream connection");
                                }
                            }
                    }
                }

                info!("Downstream server: Unified loop break");
            }
            drop(shutdown_complete_tx);
        }.instrument(Span::current()));
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn start(
        mut self,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::ChannelManager(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        let cm = self.clone();

        task_manager.spawn(async move {
            let cm = self.clone();
            loop {
                let mut cm_jds = cm.clone();
                let mut cm_pool = cm.clone();
                let mut cm_template = cm.clone();
                let mut cm_downstreams = cm.clone();
                tokio::select! {
                    message = shutdown_rx.recv() => {
                        match message {
                            Ok(ShutdownMessage::ShutdownAll) => {
                                info!("Channel Manager: received shutdown signal");
                                break;
                            }
                            Ok(ShutdownMessage::DownstreamShutdown(downstream_id)) => {
                                info!(%downstream_id, "Channel Manager: removing downstream after shutdown");
                                if let Err(e) = self.remove_downstream(downstream_id) {
                                    tracing::error!(%downstream_id, error = ?e, "Failed to remove downstream");
                                }
                            }
                            Ok(ShutdownMessage::JobDeclaratorShutdownFallback(coinbase_outputs)) => {
                                info!("Channel Manager: Job declarator shutdown signal");
                                self.channel_manager_data.super_safe_lock(|data| data.reset(coinbase_outputs));
                            }
                            Ok(ShutdownMessage::UpstreamShutdownFallback(coinbase_outputs)) => {
                                info!("Channel Manager: Upstream shutdown signal");
                                self.channel_manager_data.super_safe_lock(|data| data.reset(coinbase_outputs));
                            }
                            Err(e) => {
                                warn!(error = ?e, "shutdown channel closed unexpectedly");
                                break;
                            }
                            _ => {}
                        }
                    }
                    res = cm_jds.handle_jds_message() => {
                        if let Err(e) = res {
                            error!(error = ?e, "Error handling JDS message");
                            handle_error(&status_sender, e).await;
                            return;
                        }
                    }
                    res = cm_pool.handle_pool_message() => {
                        if let Err(e) = res {
                            error!(error = ?e, "Error handling Pool message");
                            handle_error(&status_sender, e).await;
                            return;
                        }
                    }
                    res = cm_template.handle_template_receiver_message() => {
                        if let Err(e) = res {
                            error!(error = ?e, "Error handling Template Receiver message");
                            handle_error(&status_sender, e).await;
                            return;
                        }
                    }
                    res = cm_downstreams.handle_downstreams_message() => {
                        if let Err(e) = res {
                            error!(error = ?e, "Error handling Downstreams message");
                            handle_error(&status_sender, e).await;
                            return;
                        }
                    }
                }
            }
        }.instrument(Span::current()));
    }

    fn remove_downstream(&mut self, downstream_id: u32) -> Result<(), JDCError> {
        self.channel_manager_data.super_safe_lock(|cm_data| {
            if let Some(downstream) = cm_data.downstream.remove(&downstream_id) {
                downstream.downstream_data.super_safe_lock(|ds_data| {
                    for k in ds_data
                        .standard_channels
                        .keys()
                        .chain(ds_data.extended_channels.keys())
                    {
                        cm_data.channel_id_to_downstream_id.remove(k);
                    }
                });
            }
        });
        Ok(())
    }

    #[instrument(name = "jds_message", skip_all)]
    async fn handle_jds_message(&mut self) -> Result<(), JDCError> {
        if let Ok(read_frame) = self.channel_manager_channel.jd_receiver.recv().await {
            match read_frame {
                EitherFrame::Sv2(sv2_frame) => {
                    let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                        sv2_frame.clone().into();
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

    #[instrument(name = "pool_message", skip_all)]
    async fn handle_pool_message(&mut self) -> Result<(), JDCError> {
        if let Ok(read_frame) = self.channel_manager_channel.upstream_receiver.recv().await {
            match read_frame {
                EitherFrame::Sv2(sv2_frame) => {
                    let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                        sv2_frame.clone().into();
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

    #[instrument(name = "template_receiver_message", skip_all)]
    async fn handle_template_receiver_message(&mut self) -> Result<(), JDCError> {
        if let Ok(read_frame) = self.channel_manager_channel.tp_receiver.recv().await {
            match read_frame {
                EitherFrame::Sv2(sv2_frame) => {
                    let mut frame: codec_sv2::Frame<AnyMessage<'static>, buffer_sv2::Slice> =
                        sv2_frame.clone().into();
                    let (message_type, mut payload, parsed_message) =
                        message_from_frame(&mut frame)?;

                    match parsed_message {
                        AnyMessage::TemplateDistribution(_) => {
                            self.handle_template_distribution_message_from_server(
                                message_type,
                                &mut payload,
                            )
                            .await?;
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
        if let Ok((downstream_id, read_frame)) = self
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

                                let downstream_msg = Mining::OpenExtendedMiningChannel(x.clone());

                                match self.upstream_state.get() {
                                    UpstreamState::NotConnected => {
                                        self.channel_manager_data.super_safe_lock(|data| {
                                            data.pending_downstream_requests.push(downstream_msg);
                                        });

                                        if self
                                            .upstream_state
                                            .compare_and_set(
                                                UpstreamState::NotConnected,
                                                UpstreamState::Pending,
                                            )
                                            .is_ok()
                                        {
                                            let mut upstream_message = x;
                                            upstream_message.user_identity =
                                                self.user_identity.clone().try_into().unwrap();
                                            upstream_message.request_id = 1;
                                            let upstream_message = AnyMessage::Mining(
                                                Mining::OpenExtendedMiningChannel(upstream_message),
                                            );
                                            let frame: StdFrame = upstream_message.try_into()?;

                                            self.channel_manager_channel
                                                .upstream_sender
                                                .send(frame.into())
                                                .await
                                                .map_err(|_| JDCError::ChannelErrorSender)?;
                                        }
                                    }
                                    UpstreamState::Pending => {
                                        self.channel_manager_data.super_safe_lock(|data| {
                                            data.pending_downstream_requests.push(downstream_msg);
                                        });
                                    }
                                    UpstreamState::Connected => {
                                        self.forward_downstream_channel_open_request(
                                            downstream_msg,
                                            message_type,
                                        )
                                        .await?;
                                    }
                                    UpstreamState::SoloMining => {
                                        self.forward_downstream_channel_open_request(
                                            downstream_msg,
                                            message_type,
                                        )
                                        .await?;
                                    }
                                }
                            }
                            Mining::OpenStandardMiningChannel(mut x) => {
                                let user_identity =
                                    format!("{}#{}", x.user_identity, downstream_id);
                                x.user_identity = user_identity.try_into().unwrap();

                                let downstream_msg = Mining::OpenStandardMiningChannel(x.clone());

                                match self.upstream_state.get() {
                                    UpstreamState::NotConnected => {
                                        self.channel_manager_data.super_safe_lock(|data| {
                                            data.pending_downstream_requests.push(downstream_msg)
                                        });

                                        if self
                                            .upstream_state
                                            .compare_and_set(
                                                UpstreamState::NotConnected,
                                                UpstreamState::Pending,
                                            )
                                            .is_ok()
                                        {
                                            let mut upstream_open = OpenExtendedMiningChannel {
                                                user_identity: self
                                                    .user_identity
                                                    .clone()
                                                    .try_into()
                                                    .unwrap(),
                                                request_id: 1,
                                                nominal_hash_rate: x.nominal_hash_rate,
                                                max_target: x.max_target,
                                                min_extranonce_size: self.min_extranonce_size,
                                            };

                                            let frame: StdFrame = AnyMessage::Mining(
                                                Mining::OpenExtendedMiningChannel(upstream_open),
                                            )
                                            .try_into()?;
                                            self.channel_manager_channel
                                                .upstream_sender
                                                .send(frame.into())
                                                .await
                                                .map_err(|_| JDCError::ChannelErrorSender)?;
                                        }
                                    }
                                    UpstreamState::Pending => {
                                        self.channel_manager_data.super_safe_lock(|data| {
                                            data.pending_downstream_requests.push(downstream_msg)
                                        });
                                    }
                                    UpstreamState::Connected => {
                                        self.forward_downstream_channel_open_request(
                                            downstream_msg,
                                            message_type,
                                        )
                                        .await?;
                                    }
                                    UpstreamState::SoloMining => {
                                        self.forward_downstream_channel_open_request(
                                            downstream_msg,
                                            message_type,
                                        )
                                        .await?;
                                    }
                                }
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

    #[instrument(skip_all, fields(message_type = message_type))]
    async fn forward_downstream_channel_open_request(
        &mut self,
        mining_msg: Mining<'static>,
        message_type: u8,
    ) -> Result<(), JDCError> {
        let sv2_frame: Sv2Frame<Mining<'static>, Vec<u8>> = match Sv2Frame::from_message(
            mining_msg,
            message_type,
            0,
            false,
        ) {
            Some(f) => f,
            None => {
                warn!(%message_type, "Failed to build Sv2Frame from mining message; dropping request");
                return Ok(());
            }
        };

        let mut serialized = vec![0u8; sv2_frame.encoded_length()];
        if let Err(e) = sv2_frame.serialize(&mut serialized) {
            warn!(?e, %message_type, len = serialized.len(), "Failed to serialize Sv2Frame; dropping request");
            return Ok(());
        }

        let mut deserialized_frame =
            match Sv2Frame::<Mining<'static>, Vec<u8>>::from_bytes(serialized) {
                Ok(f) => f,
                Err(e) => {
                    warn!(?e, %message_type, "Failed to deserialize Sv2Frame; dropping request");
                    return Ok(());
                }
            };

        let mut payload = deserialized_frame.payload();
        self.handle_mining_message_from_client(message_type, &mut payload)
            .await?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn allocate_tokens(&self, token_to_allocate: u32) -> Result<(), JDCError> {
        debug!("Allocating {} job tokens", token_to_allocate);

        for i in 0..token_to_allocate {
            let request_id = self
                .channel_manager_data
                .super_safe_lock(|data| data.request_id_factory.next());

            debug!(
                request_id,
                "Allocating token {}/{}",
                i + 1,
                token_to_allocate
            );

            let message = JobDeclaration::AllocateMiningJobToken(AllocateMiningJobToken {
                user_identifier: self
                    .user_identity
                    .to_string()
                    .try_into()
                    .expect("Static string should always convert"),
                request_id,
            });

            let frame: StdFrame = AnyMessage::JobDeclaration(message)
                .try_into()
                .map_err(|e| {
                    info!(error = ?e, "Failed to convert AllocateMiningJobToken to frame");
                    e
                })?;

            self.channel_manager_channel
                .jd_sender
                .send(frame.into())
                .await
                .map_err(|e| {
                    info!(error = ?e, "Failed to send AllocateMiningJobToken frame");
                    JDCError::ChannelErrorSender
                })?;
        }

        info!("Successfully allocated {token_to_allocate} job tokens");
        Ok(())
    }
}
