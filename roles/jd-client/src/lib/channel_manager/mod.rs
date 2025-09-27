use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::Arc,
};

use async_channel::{Receiver, Sender};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use stratum_common::{
    network_helpers_sv2::noise_stream::NoiseTcpStream,
    roles_logic_sv2::{
        self,
        channels_sv2::{
            client::extended::ExtendedChannel,
            server::{
                jobs::{
                    extended::ExtendedJob, factory::JobFactory, job_store::DefaultJobStore,
                    standard::StandardJob,
                },
                standard::StandardChannel,
            },
        },
        codec_sv2::{Responder, Sv2Frame},
        handlers_sv2::{
            HandleJobDeclarationMessagesFromServerAsync, HandleMiningMessagesFromClientAsync,
            HandleMiningMessagesFromServerAsync, HandleTemplateDistributionMessagesFromServerAsync,
        },
        job_declaration_sv2::{
            AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        },
        mining_sv2::{
            ExtendedExtranonce, OpenExtendedMiningChannel, SetCustomMiningJob, SetTarget, Target,
            UpdateChannel, MAX_EXTRANONCE_LEN, MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
        },
        parsers_sv2::{AnyMessage, JobDeclaration, Mining},
        template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTdp},
        utils::{Id as IdFactory, Mutex},
        Vardiff, VardiffState,
    },
};
use tokio::{net::TcpListener, select, sync::broadcast};
use tracing::{debug, error, info, warn};

use crate::{
    channel_manager::downstream_message_handler::RouteMessageTo,
    config::JobDeclaratorClientConfig,
    downstream::Downstream,
    error::JDCError,
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{
        AtomicUpstreamState, Message, PendingChannelRequest, SV2Frame, ShutdownMessage, StdFrame,
        UpstreamState,
    },
};
mod downstream_message_handler;
mod jd_message_handler;
mod template_message_handler;
mod upstream_message_handler;

pub const JDC_SEARCH_SPACE_BYTES: usize = 4;

/// A `DeclaredJob` encapsulates all the relevant data associated with a single
/// job declaration, including its template, optional messages, coinbase output,
/// and transaction list.
#[derive(Clone, Debug)]
pub struct DeclaredJob {
    // The original `DeclareMiningJob` message associated with this job,
    // if one was sent.
    declare_mining_job: Option<DeclareMiningJob<'static>>,
    // The template associated with the declared job.
    template: NewTemplate<'static>,
    // The `SetNewPrevHashTdp` message associated with this job, if available.
    prev_hash: Option<SetNewPrevHashTdp<'static>>,
    // The `SetCustomMiningJob` message associated with this job,
    // if a custom job was created.
    set_custom_mining_job: Option<SetCustomMiningJob<'static>>,
    // The coinbase output for this job.
    coinbase_output: Vec<u8>,
    // The list of transactions included in the job’s template.
    tx_list: Vec<Vec<u8>>,
}

/// Central state container for the **Channel Manager**.
///
/// `ChannelManagerData` holds all runtime state that the JDC
/// needs to manage downstream clients, upstream connections, extranonce allocation,
/// job tracking, and various ID factories.  
pub struct ChannelManagerData {
    // Mapping of `downstream_id` → `Downstream` object,
    // used by the channel manager to locate and interact with downstream clients.
    downstream: HashMap<u32, Downstream>,
    // Extranonce prefix factory for **extended downstream channels**.
    // Each new extended downstream receives a unique extranonce prefix.
    extranonce_prefix_factory_extended: ExtendedExtranonce,
    // Extranonce prefix factory for **standard downstream channels**.
    // Each new standard downstream receives a unique extranonce prefix.
    extranonce_prefix_factory_standard: ExtendedExtranonce,
    // Factory that generates **monotonically increasing request IDs**
    // for messages sent from the JDC.
    request_id_factory: IdFactory,
    // Factory that assigns a unique ID to each new **downstream connection**.
    downstream_id_factory: IdFactory,
    // Factory that assigns a unique **channel ID** to each channel.
    //
    // ⚠️ Note: In this version of the JDC, channel IDs are unique
    // across *all downstreams*, not scoped per downstream.
    channel_id_factory: IdFactory,
    // Factory that assigns a unique **sequence number** to each share
    // submitted from the JDC to the upstream.
    sequence_number_factory: IdFactory,
    // The last **future template** received from the upstream.
    last_future_template: Option<NewTemplate<'static>>,
    // The last **new prevhash** received from the upstream.
    last_new_prev_hash: Option<SetNewPrevHashTdp<'static>>,
    // The most recent set of **allocation tokens** received from the JDS.
    allocate_tokens: Option<AllocateMiningJobTokenSuccess<'static>>,
    // Stores new templates as they arrive, mapped by their **template ID**.
    template_store: HashMap<u64, NewTemplate<'static>>,
    // Stores the last declared job, keyed by the `request_id` used when
    // declaring the job to the JDS.
    // This is later used to send a `SetCustomMiningJob`.
    last_declare_job_store: HashMap<u32, DeclaredJob>,
    // Maps a template ID → corresponding upstream job ID.
    template_id_to_upstream_job_id: HashMap<u64, u64>,
    // Maps a downstream channel ID + job ID → corresponding template ID.
    downstream_channel_id_and_job_id_to_template_id: HashMap<(u32, u32), u64>,
    // The coinbase outputs currently in use.
    coinbase_outputs: Vec<u8>,
    // Maps channel ID → downstream ID.
    channel_id_to_downstream_id: HashMap<u32, u32>,
    // The active upstream extended channel (client-side instance), if any.
    upstream_channel: Option<ExtendedChannel<'static>>,
    // Optional "pool tag" string, identifying the pool.
    pool_tag_string: Option<String>,
    // List of pending downstream connection requests,
    // persisted while the JDC is opening a channel with the upstream.
    pending_downstream_requests: VecDeque<PendingChannelRequest>,
    // Factory for creating **custom mining jobs**, if available.
    job_factory: Option<JobFactory>,
    // Mapping of `(downstream_id, channel_id)` → vardiff controller.
    // Each entry manages variable difficulty for a specific downstream channel.
    vardiff: HashMap<(u32, u32), VardiffState>,
}

impl ChannelManagerData {
    /// Resets the internal state of the Channel Manager.
    ///
    /// This method is primarily used during **fallback scenarios** to clear and
    /// reinitialize all internal data structures. It ensures that the Channel Manager
    /// returns to a clean state, ready to handle fresh upstream or downstream connections.
    pub fn reset(&mut self, coinbase_outputs: Vec<u8>) {
        self.downstream.clear();
        self.template_store.clear();
        self.last_declare_job_store.clear();
        self.template_id_to_upstream_job_id.clear();
        self.downstream_channel_id_and_job_id_to_template_id.clear();
        self.channel_id_to_downstream_id.clear();
        self.pending_downstream_requests.clear();

        self.downstream_id_factory = IdFactory::new();
        self.request_id_factory = IdFactory::new();
        self.channel_id_factory = IdFactory::new();

        let (range_0, range_1, range_2) = {
            let range_1 = 0..JDC_SEARCH_SPACE_BYTES;
            (
                0..range_1.start,
                range_1.clone(),
                range_1.end..MAX_EXTRANONCE_LEN,
            )
        };
        self.extranonce_prefix_factory_extended =
            ExtendedExtranonce::new(range_0.clone(), range_1.clone(), range_2.clone(), None)
                .expect("valid ranges");
        self.extranonce_prefix_factory_standard =
            ExtendedExtranonce::new(range_0, range_1, range_2, None).expect("valid ranges");

        self.allocate_tokens = None;
        self.upstream_channel = None;
        self.pool_tag_string = None;

        self.coinbase_outputs = coinbase_outputs;
    }
}

/// Represents all communication channels managed by the Channel Manager.
///
/// The `ChannelManagerChannel` holds all the asynchronous communication primitives
/// required for message exchange between the **Channel Manager** and other subsystems.
/// It ensures decoupled, structured communication between upstreams, downstreams,
/// the Job Dispatcher Service (JDS), and the Template Provider (TP).
///
/// # Channels
/// 1. **Upstream**:
///    - `(upstream_sender, upstream_receiver)` Used to send and receive messages from the upstream
///      subsystem.
///
/// 2. **JDS**:
///    - `(jd_sender, jd_receiver)` Handles communication with JDS.
///
/// 3. **Template Provider**:
///    - `(tp_sender, tp_receiver)` Manages communication with the Template Provider.
///
/// 4. **Downstream**:
///    - `(downstream_sender, downstream_receiver)` Broadcasts messages to all downstream clients
///      and receives messages from them.
///
/// 5. **Status**:
///    - `status_sender` Allows the Channel Manager to notify the main status loop of critical state
///      changes.

#[derive(Clone)]
pub struct ChannelManagerChannel {
    upstream_sender: Sender<SV2Frame>,
    upstream_receiver: Receiver<SV2Frame>,
    jd_sender: Sender<SV2Frame>,
    jd_receiver: Receiver<SV2Frame>,
    tp_sender: Sender<SV2Frame>,
    tp_receiver: Receiver<SV2Frame>,
    downstream_sender: broadcast::Sender<(u32, Message)>,
    downstream_receiver: Receiver<(u32, SV2Frame)>,
    status_sender: Sender<Status>,
}

/// Contains all the state of mutable and immutable data required
/// by channel manager to process its task along with channels
/// to perform message traversal.
#[derive(Clone)]
pub struct ChannelManager {
    channel_manager_data: Arc<Mutex<ChannelManagerData>>,
    channel_manager_channel: ChannelManagerChannel,
    miner_tag_string: String,
    share_batch_size: usize,
    shares_per_minute: f32,
    user_identity: String,
    /// This represent the current state of Upstream channel
    /// 1. NoChannel: No active upstream connection.
    /// 2. Pending: A channel request has been sent, awaiting response.
    /// 3. Connected: An upstream channel is successfully established.
    /// 4. SoloMining: No upstream is available; the JDC operates in solo mining mode. case.
    pub upstream_state: AtomicUpstreamState,
}

impl ChannelManager {
    /// Constructor method used to instantiate the Channel Manager
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        config: JobDeclaratorClientConfig,
        upstream_sender: Sender<SV2Frame>,
        upstream_receiver: Receiver<SV2Frame>,
        jd_sender: Sender<SV2Frame>,
        jd_receiver: Receiver<SV2Frame>,
        tp_sender: Sender<SV2Frame>,
        tp_receiver: Receiver<SV2Frame>,
        downstream_sender: broadcast::Sender<(u32, Message)>,
        downstream_receiver: Receiver<(u32, SV2Frame)>,
        status_sender: Sender<Status>,
        coinbase_outputs: Vec<u8>,
    ) -> Result<Self, JDCError> {
        let (range_0, range_1, range_2) = {
            let range_1 = 0..JDC_SEARCH_SPACE_BYTES;
            (
                0..range_1.start,
                range_1.clone(),
                range_1.end..MAX_EXTRANONCE_LEN,
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
            sequence_number_factory: IdFactory::new(),
            last_future_template: None,
            last_new_prev_hash: None,
            allocate_tokens: None,
            template_store: HashMap::new(),
            last_declare_job_store: HashMap::new(),
            template_id_to_upstream_job_id: HashMap::new(),
            downstream_channel_id_and_job_id_to_template_id: HashMap::new(),
            coinbase_outputs,
            channel_id_to_downstream_id: HashMap::new(),
            upstream_channel: None,
            pool_tag_string: None,
            pending_downstream_requests: VecDeque::new(),
            job_factory: None,
            vardiff: HashMap::new(),
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
            status_sender,
        };

        let channel_manager = ChannelManager {
            channel_manager_data,
            channel_manager_channel,
            share_batch_size: config.share_batch_size() as usize,
            shares_per_minute: config.shares_per_minute() as f32,
            miner_tag_string: config.jdc_signature().to_string(),
            user_identity: config.user_identity().to_string(),
            upstream_state: AtomicUpstreamState::new(UpstreamState::SoloMining),
        };

        Ok(channel_manager)
    }

    /// Starts the downstream server, and accepts new connection request.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_downstream_server(
        self,
        authority_public_key: Secp256k1PublicKey,
        authority_secret_key: Secp256k1SecretKey,
        cert_validity_sec: u64,
        listening_address: SocketAddr,
        task_manager: Arc<TaskManager>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        status_sender: Sender<Status>,
        channel_manager_sender: Sender<(u32, SV2Frame)>,
        channel_manager_receiver: broadcast::Sender<(u32, Message)>,
    ) -> Result<(), JDCError> {
        info!("Starting downstream server at {listening_address}");
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
                                    task_manager_clone.clone(),
                                    status_sender.clone(),
                                );

                                self.channel_manager_data.super_safe_lock(|data| {
                                    data.downstream.insert(downstream_id, downstream.clone());
                                });

                                downstream
                                    .start(
                                        notify_shutdown.clone(),
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
            }
            info!("Downstream server: Unified loop break");
        });
        Ok(())
    }

    /// The central orchestrator of the Channel Manager.  
    ///  
    /// Responsible for receiving messages from all subsystems, processing them,  
    /// and either forwarding them to the appropriate subsystem or updating  
    /// the internal state of the Channel Manager as needed.
    pub async fn start(
        mut self,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) {
        let status_sender = StatusSender::ChannelManager(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        task_manager.spawn(async move {
            let cm = self.clone();
            let vd = self.clone();
            let vardiff_future = vd.run_vardiff_loop();
            tokio::pin!(vardiff_future);
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
                            Ok(ShutdownMessage::JobDeclaratorShutdownFallback((coinbase_outputs,tx))) => {
                                info!("Channel Manager: Job declarator shutdown signal");
                                self.upstream_state.set(UpstreamState::SoloMining);
                                self.channel_manager_data.super_safe_lock(|data| data.reset(coinbase_outputs));
                                drop(tx);
                            }
                            Ok(ShutdownMessage::UpstreamShutdownFallback((coinbase_outputs,tx))) => {
                                info!("Channel Manager: Upstream shutdown signal");
                                self.upstream_state.set(UpstreamState::SoloMining);
                                self.channel_manager_data.super_safe_lock(|data| data.reset(coinbase_outputs));
                                drop(tx);
                            }
                            Err(e) => {
                                warn!(error = ?e, "shutdown channel closed unexpectedly");
                                break;
                            }
                            _ => {}
                        }
                    }
                    res = &mut vardiff_future => {
                        info!("Vardiff loop completed with: {res:?}");
                    }
                    res = cm_jds.handle_jds_message() => {
                        if let Err(e) = res {
                            if !e.is_critical() {
                                continue;
                            }
                            error!(error = ?e, "Error handling JDS message");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = cm_pool.handle_pool_message() => {
                        if let Err(e) = res {
                            if !e.is_critical() {
                                continue;
                            }
                            error!(error = ?e, "Error handling Pool message");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = cm_template.handle_template_provider_message() => {
                        if let Err(e) = res {
                            if !e.is_critical() {
                                continue;
                            }
                            error!(error = ?e, "Error handling Template Receiver message");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = cm_downstreams.handle_downstream_message() => {
                        if let Err(e) = res {
                            if !e.is_critical() {
                                continue;
                            }
                            error!(error = ?e, "Error handling Downstreams message");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                }
            }
        });
    }

    // Removes a downstream entry from the Channel Manager’s state.
    //
    // Given a `downstream_id`, this method:
    // 1. Removes the corresponding downstream from the `downstream` map.
    // 2. Cleans up all associated channel mappings (both standard and extended) by removing their
    //    entries from `channel_id_to_downstream_id`.
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

    /// Handles messages received from the JDS subsystem.  
    ///  
    /// This method listens for incoming frames on the `jd_receiver` channel.  
    /// - If the frame contains a JobDeclaration message, it forwards it to the   job declaration
    ///   message handler.
    /// - If the frame contains any unsupported message type, an error is returned.
    async fn handle_jds_message(&mut self) -> Result<(), JDCError> {
        if let Ok(mut sv2_frame) = self.channel_manager_channel.jd_receiver.recv().await {
            let Some(message_type) = sv2_frame.get_header().map(|m| m.msg_type()) else {
                return Ok(());
            };

            self.handle_job_declaration_message_frame_from_server(
                message_type,
                sv2_frame.payload(),
            )
            .await?;
        }
        Ok(())
    }

    /// Handles messages received from the Upstream subsystem.  
    ///  
    /// This method listens for incoming frames on the `upstream_receiver` channel.  
    /// - If the frame contains a **Mining** message, it forwards it to the   mining message
    ///   handler.
    /// - If the frame contains any unsupported message type, an error is returned.
    async fn handle_pool_message(&mut self) -> Result<(), JDCError> {
        if let Ok(mut sv2_frame) = self.channel_manager_channel.upstream_receiver.recv().await {
            let Some(message_type) = sv2_frame.get_header().map(|m| m.msg_type()) else {
                return Ok(());
            };

            self.handle_mining_message_frame_from_server(message_type, sv2_frame.payload())
                .await?;
        }
        Ok(())
    }

    // Handles messages received from the TP subsystem.
    //
    // This method listens for incoming frames on the `tp_receiver` channel.
    // - If the frame contains a TemplateDistribution message, it forwards it to the   template
    //   distribution message handler.
    // - If the frame contains any unsupported message type, an error is returned.
    async fn handle_template_provider_message(&mut self) -> Result<(), JDCError> {
        if let Ok(mut sv2_frame) = self.channel_manager_channel.tp_receiver.recv().await {
            let Some(message_type) = sv2_frame.get_header().map(|m| m.msg_type()) else {
                return Ok(());
            };
            self.handle_template_distribution_message_frame_from_server(
                message_type,
                sv2_frame.payload(),
            )
            .await?;
        }
        Ok(())
    }

    // Handles messages received from downstream clients and routes them appropriately.
    //
    // # Overview
    // This method is similar to the upstream JDS message handler, but introduces additional
    // logic for handling OpenChannel requests (both standard and extended).
    //
    // # Message Flow
    // - For most mining messages: The message is forwarded directly to
    //   `handle_mining_message_from_client`, and the `channel_id_to_downstream_id` map is used to
    //   determine the origin downstream.
    //
    // - For OpenChannel messages: At the time of request, the `channel_id` is not yet assigned, so
    //   we cannot map the message back to the downstream. To solve this:
    //   1. The `downstream_id` is appended to the `user_identity` (e.g.,
    //      `"identity#downstream_id"`).
    //   2. Later, the appended downstream ID is stripped and used by the message handler to
    //      correctly attribute the request.
    //
    // # Channel Establishment Logic
    // - NoChannel → Pending:
    //   - The first downstream OpenChannel request is stored in `pending_downstream_requests`.
    //   - The upstream state transitions from `NoChannel` to `Pending`.
    //   - A single channel request is then sent to the upstream (JDC → upstream).
    //
    // - Pending:
    //   - Additional downstream OpenChannel requests are stored in `pending_downstream_requests`
    //     until the upstream connection is established.
    //
    // - Connected / SoloMining:
    //   - Downstream OpenChannel requests are immediately forwarded to the mining handler.
    //
    // # Notes
    // - Only one upstream channel is created per JDC instance.
    // - After the upstream channel is established, all new downstream requests bypass the pending
    //   mechanism and are sent directly to the mining handler.
    async fn handle_downstream_message(&mut self) -> Result<(), JDCError> {
        if let Ok((downstream_id, mut sv2_frame)) = self
            .channel_manager_channel
            .downstream_receiver
            .recv()
            .await
        {
            let Some(message_type) = sv2_frame.get_header().map(|m| m.msg_type()) else {
                return Err(JDCError::UnexpectedMessage(0));
            };

            match message_type {
                MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL => {
                    let message: Mining = (message_type, sv2_frame.payload()).try_into()?;
                    let Mining::OpenExtendedMiningChannel(mut downstream_channel_request) = message
                    else {
                        return Err(JDCError::UnexpectedMessage(message_type));
                    };
                    let user_identity = format!(
                        "{}#{}",
                        downstream_channel_request.user_identity.as_utf8_or_hex(),
                        downstream_id
                    );
                    downstream_channel_request.user_identity = user_identity.try_into()?;

                    let downstream_msg = downstream_channel_request.clone().into_static();

                    match self.upstream_state.get() {
                        UpstreamState::NoChannel => {
                            self.channel_manager_data.super_safe_lock(|data| {
                                data.pending_downstream_requests
                                    .push_front(downstream_msg.into());
                            });

                            if self
                                .upstream_state
                                .compare_and_set(UpstreamState::NoChannel, UpstreamState::Pending)
                                .is_ok()
                            {
                                let mut upstream_message = downstream_channel_request;
                                upstream_message.user_identity =
                                    self.user_identity.clone().try_into()?;
                                upstream_message.request_id = 1;
                                upstream_message.min_extranonce_size +=
                                    JDC_SEARCH_SPACE_BYTES as u16;
                                let upstream_message = AnyMessage::Mining(
                                    Mining::OpenExtendedMiningChannel(upstream_message)
                                        .into_static(),
                                );
                                let frame: StdFrame = upstream_message.try_into()?;

                                self.channel_manager_channel
                                    .upstream_sender
                                    .send(frame)
                                    .await
                                    .map_err(|_| JDCError::ChannelErrorSender)?;
                            }
                        }
                        UpstreamState::Pending => {
                            self.channel_manager_data.super_safe_lock(|data| {
                                data.pending_downstream_requests
                                    .push_back(downstream_msg.into());
                            });
                        }
                        UpstreamState::Connected => {
                            self.send_open_channel_request_to_mining_handler(
                                Mining::OpenExtendedMiningChannel(downstream_msg),
                                message_type,
                            )
                            .await?;
                        }
                        UpstreamState::SoloMining => {
                            self.send_open_channel_request_to_mining_handler(
                                Mining::OpenExtendedMiningChannel(downstream_msg),
                                message_type,
                            )
                            .await?;
                        }
                    }
                }
                MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL => {
                    let message: Mining = (message_type, sv2_frame.payload()).try_into()?;
                    let Mining::OpenStandardMiningChannel(mut downstream_channel_request) = message
                    else {
                        return Err(JDCError::UnexpectedMessage(message_type));
                    };

                    let user_identity = format!(
                        "{:?}#{}",
                        downstream_channel_request.user_identity, downstream_id
                    );
                    downstream_channel_request.user_identity = user_identity.try_into()?;

                    let downstream_msg = downstream_channel_request.clone().into_static();

                    match self.upstream_state.get() {
                        UpstreamState::NoChannel => {
                            self.channel_manager_data.super_safe_lock(|data| {
                                data.pending_downstream_requests
                                    .push_front(downstream_msg.into())
                            });

                            if self
                                .upstream_state
                                .compare_and_set(UpstreamState::NoChannel, UpstreamState::Pending)
                                .is_ok()
                            {
                                let upstream_open = OpenExtendedMiningChannel {
                                    user_identity: self.user_identity.clone().try_into().unwrap(),
                                    request_id: 1,
                                    nominal_hash_rate: downstream_channel_request.nominal_hash_rate,
                                    max_target: downstream_channel_request.max_target,
                                    min_extranonce_size: JDC_SEARCH_SPACE_BYTES as u16,
                                };

                                let frame: StdFrame = AnyMessage::Mining(
                                    Mining::OpenExtendedMiningChannel(upstream_open).into_static(),
                                )
                                .try_into()?;
                                self.channel_manager_channel
                                    .upstream_sender
                                    .send(frame)
                                    .await
                                    .map_err(|_| JDCError::ChannelErrorSender)?;
                            }
                        }
                        UpstreamState::Pending => {
                            self.channel_manager_data.super_safe_lock(|data| {
                                data.pending_downstream_requests
                                    .push_back(downstream_msg.into())
                            });
                        }
                        UpstreamState::Connected => {
                            self.send_open_channel_request_to_mining_handler(
                                Mining::OpenStandardMiningChannel(downstream_msg),
                                message_type,
                            )
                            .await?;
                        }
                        UpstreamState::SoloMining => {
                            self.send_open_channel_request_to_mining_handler(
                                Mining::OpenStandardMiningChannel(downstream_msg),
                                message_type,
                            )
                            .await?;
                        }
                    }
                }
                _ => {
                    self.handle_mining_message_frame_from_client(message_type, sv2_frame.payload())
                        .await?;
                }
            }
        }

        Ok(())
    }

    // Utility method to send open channel request from downstream to message handler.
    async fn send_open_channel_request_to_mining_handler(
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
                return Err(JDCError::FrameConversionError);
            }
        };

        let mut serialized = vec![0u8; sv2_frame.encoded_length()];
        if let Err(e) = sv2_frame.serialize(&mut serialized) {
            warn!(?e, %message_type, len = serialized.len(), "Failed to serialize Sv2Frame; dropping request");
            return Err(JDCError::FramingSv2(e));
        }

        let mut deserialized_frame =
            match Sv2Frame::<Mining<'static>, Vec<u8>>::from_bytes(serialized) {
                Ok(f) => f,
                Err(e) => {
                    warn!(?e, %message_type, "Failed to deserialize Sv2Frame; dropping request");
                    return Err(JDCError::FrameConversionError);
                }
            };

        let payload = deserialized_frame.payload();
        self.handle_mining_message_frame_from_client(message_type, payload)
            .await?;
        Ok(())
    }

    /// Utility method to request for more token to JDS.
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
                .send(frame)
                .await
                .map_err(|e| {
                    info!(error = ?e, "Failed to send AllocateMiningJobToken frame");
                    JDCError::ChannelErrorSender
                })?;
        }

        info!("Requested allocation of {token_to_allocate} mining job tokens to JDS");
        Ok(())
    }

    // Runs the vardiff on extended channel.
    fn run_vardiff_on_extended_channel(
        downstream_id: u32,
        channel_id: u32,
        channel_state: &mut roles_logic_sv2::channels_sv2::server::extended::ExtendedChannel<
            'static,
            DefaultJobStore<ExtendedJob<'static>>,
        >,
        vardiff_state: &mut VardiffState,
        updates: &mut Vec<RouteMessageTo>,
    ) {
        let (hashrate, target, shares_per_minute) = (
            channel_state.get_nominal_hashrate(),
            channel_state.get_target(),
            channel_state.get_shares_per_minute(),
        );

        let Ok(new_hashrate_opt) = vardiff_state.try_vardiff(hashrate, target, shares_per_minute)
        else {
            debug!("Vardiff computation failed for extended channel {channel_id}");
            return;
        };

        let Some(new_hashrate) = new_hashrate_opt else {
            return;
        };

        match channel_state.update_channel(new_hashrate, None) {
            Ok(()) => {
                let updated_target = channel_state.get_target();
                updates.push(
                    (
                        downstream_id,
                        Mining::SetTarget(SetTarget {
                            channel_id,
                            maximum_target: updated_target.clone().into(),
                        }),
                    )
                        .into(),
                );
                debug!("Updated target for extended channel_id={channel_id} to {updated_target:?}",);
            }
            Err(e) => warn!(
                "Failed to update extended channel channel_id={channel_id} during vardiff {e:?}"
            ),
        }
    }

    // Runs the vardiff on the standard channel.
    fn run_vardiff_on_standard_channel(
        downstream_id: u32,
        channel_id: u32,
        channel: &mut StandardChannel<'static, DefaultJobStore<StandardJob<'static>>>,
        vardiff_state: &mut VardiffState,
        updates: &mut Vec<RouteMessageTo>,
    ) {
        let hashrate = channel.get_nominal_hashrate();
        let target = channel.get_target();
        let shares_per_minute = channel.get_shares_per_minute();

        let Ok(new_hashrate_opt) = vardiff_state.try_vardiff(hashrate, target, shares_per_minute)
        else {
            debug!("Vardiff computation failed for standard channel {channel_id}");
            return;
        };

        if let Some(new_hashrate) = new_hashrate_opt {
            match channel.update_channel(new_hashrate, None) {
                Ok(()) => {
                    let updated_target = channel.get_target();
                    updates.push(
                        (
                            downstream_id,
                            Mining::SetTarget(SetTarget {
                                channel_id,
                                maximum_target: updated_target.clone().into(),
                            }),
                        )
                            .into(),
                    );
                    debug!("Updated target for standard channel channel_id={channel_id} to {updated_target:?}");
                }
                Err(e) => warn!(
                    "Failed to update standard channel channel_id={channel_id} during vardiff {e:?}"
                ),
            }
        }
    }

    // Periodic vardiff task loop.
    //
    // # Purpose
    // - Executes the vardiff cycle every 60 seconds for all downstreams.
    // - Delegates to [`Self::run_vardiff`] on each tick.
    async fn run_vardiff_loop(&self) -> Result<(), JDCError> {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            ticker.tick().await;
            info!("Starting vardiff loop for downstreams");

            if let Err(e) = self.run_vardiff().await {
                error!(error = ?e, "Vardiff iteration failed");
            }
        }
    }

    // Runs vardiff across **all channels** and generates updates.
    //
    // # Purpose
    // - Iterates through all downstream channels (both standard and extended).
    // - Runs vardiff for each channel and collects the resulting updates.
    // - Propagates difficulty changes to downstreams and also sends an `UpdateChannel` message
    //   upstream if applicable.
    async fn run_vardiff(&self) -> Result<(), JDCError> {
        let mut messages: Vec<RouteMessageTo> = vec![];
        self.channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                for ((channel_id, downstream_id), vardiff_state) in
                    channel_manager_data.vardiff.iter_mut()
                {
                    let Some(downstream) = channel_manager_data.downstream.get_mut(downstream_id)
                    else {
                        continue;
                    };
                    downstream.downstream_data.super_safe_lock(|data| {
                        if let Some(standard_channel) = data.standard_channels.get_mut(channel_id) {
                            Self::run_vardiff_on_standard_channel(
                                *downstream_id,
                                *channel_id,
                                standard_channel,
                                vardiff_state,
                                &mut messages,
                            );
                        }
                        if let Some(extended_channel) = data.extended_channels.get_mut(channel_id) {
                            Self::run_vardiff_on_extended_channel(
                                *downstream_id,
                                *channel_id,
                                extended_channel,
                                vardiff_state,
                                &mut messages,
                            );
                        }
                    });
                }

                if !messages.is_empty() {
                    let mut downstream_hashrate = 0.0;
                    let mut min_target: Target = [0xff; 32].into();

                    for (_, downstream) in channel_manager_data.downstream.iter() {
                        downstream.downstream_data.super_safe_lock(|data| {
                            let mut update_from_channel = |hashrate: f32, target: &Target| {
                                downstream_hashrate += hashrate;
                                min_target = std::cmp::min(target.clone(), min_target.clone());
                            };

                            for (_, channel) in data.standard_channels.iter() {
                                update_from_channel(
                                    channel.get_nominal_hashrate(),
                                    channel.get_target(),
                                );
                            }

                            for (_, channel) in data.extended_channels.iter() {
                                update_from_channel(
                                    channel.get_nominal_hashrate(),
                                    channel.get_target(),
                                );
                            }
                        });
                    }

                    if let Some(ref upstream_channel) = channel_manager_data.upstream_channel {
                        debug!(
                            "Checking upstream channel {} with hashrate {} and target {:?}",
                            upstream_channel.get_channel_id(),
                            upstream_channel.get_nominal_hashrate(),
                            upstream_channel.get_target()
                        );

                        info!("Sending update channel message upstream");
                        messages.push(
                            Mining::UpdateChannel(UpdateChannel {
                                channel_id: upstream_channel.get_channel_id(),
                                nominal_hash_rate: downstream_hashrate,
                                maximum_target: min_target.into(),
                            })
                            .into(),
                        )
                    }
                }
            });

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        info!("Vardiff update cycle complete");
        Ok(())
    }
}
