use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{atomic::AtomicUsize, Arc},
};

use async_channel::{Receiver, Sender};
use core::sync::atomic::Ordering;
use stratum_apps::{
    config_helpers::CoinbaseRewardScript,
    custom_mutex::Mutex,
    key_utils::{Secp256k1PublicKey, Secp256k1SecretKey},
    network_helpers::noise_stream::NoiseTcpStream,
    stratum_core::{
        channels_sv2::{
            persistence::Persistence,
            server::{
                extended::ExtendedChannel,
                jobs::{extended::ExtendedJob, job_store::DefaultJobStore, standard::StandardJob},
                standard::StandardChannel,
            },
            Vardiff, VardiffState,
        },
        handlers_sv2::{
            HandleMiningMessagesFromClientAsync, HandleTemplateDistributionMessagesFromServerAsync,
        },
        mining_sv2::{ExtendedExtranonce, SetTarget},
        noise_sv2::Responder,
        parsers_sv2::{Mining, TemplateDistribution},
        template_distribution_sv2::{NewTemplate, SetNewPrevHash},
    },
};
use tokio::{net::TcpListener, select, sync::broadcast};
use tracing::{debug, error, info, warn};

use crate::{
    config::PoolConfig,
    downstream::Downstream,
    error::PoolResult,
    share_persistence::{ShareFileHandler, ShareFilePersistence},
    status::{handle_error, Status, StatusSender},
    task_manager::TaskManager,
    utils::{Message, ShutdownMessage},
};

mod mining_message_handler;
mod template_distribution_message_handler;

/// Type alias for the pool's persistence implementation.
pub type PoolPersistence = Persistence<ShareFilePersistence>;

const POOL_ALLOCATION_BYTES: usize = 4;
const CLIENT_SEARCH_SPACE_BYTES: usize = 8;
pub const FULL_EXTRANONCE_SIZE: usize = POOL_ALLOCATION_BYTES + CLIENT_SEARCH_SPACE_BYTES;

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
    // Factory that assigns a unique ID to each new **downstream connection**.
    downstream_id_factory: AtomicUsize,
    // Mapping of `(downstream_id, channel_id)` → vardiff controller.
    // Each entry manages variable difficulty for a specific downstream channel.
    vardiff: HashMap<(u32, u32), VardiffState>,
    // Coinbase outputs
    coinbase_outputs: Vec<u8>,
    // Last new prevhash
    last_new_prev_hash: Option<SetNewPrevHash<'static>>,
    // Last future template
    last_future_template: Option<NewTemplate<'static>>,
}

#[derive(Clone)]
pub struct ChannelManagerChannel {
    tp_sender: Sender<TemplateDistribution<'static>>,
    tp_receiver: Receiver<TemplateDistribution<'static>>,
    downstream_sender: broadcast::Sender<(u32, Mining<'static>)>,
    downstream_receiver: Receiver<(u32, Mining<'static>)>,
}

/// Contains all the state of mutable and immutable data required
/// by channel manager to process its task along with channels
/// to perform message traversal.
#[derive(Clone)]
pub struct ChannelManager {
    channel_manager_data: Arc<Mutex<ChannelManagerData>>,
    channel_manager_channel: ChannelManagerChannel,
    pool_tag_string: String,
    share_batch_size: usize,
    shares_per_minute: f32,
    coinbase_reward_script: CoinbaseRewardScript,
    persistence: PoolPersistence,
}

impl ChannelManager {
    /// Constructor method used to instantiate the ChannelManager
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        config: PoolConfig,
        tp_sender: Sender<TemplateDistribution<'static>>,
        tp_receiver: Receiver<TemplateDistribution<'static>>,
        downstream_sender: broadcast::Sender<(u32, Mining<'static>)>,
        downstream_receiver: Receiver<(u32, Mining<'static>)>,
        coinbase_outputs: Vec<u8>,
        status_tx: StatusSender,
        task_manager: Arc<TaskManager>,
    ) -> PoolResult<Self> {
        let range_0 = 0..0;
        let range_1 = 0..POOL_ALLOCATION_BYTES;
        let range_2 = POOL_ALLOCATION_BYTES..POOL_ALLOCATION_BYTES + CLIENT_SEARCH_SPACE_BYTES;

        let make_extranonce_factory = || {
            // simulating a scenario where there are multiple mining servers
            // this static prefix allows unique extranonce_prefix allocation
            // for this mining server
            let static_prefix = config.server_id().to_be_bytes().to_vec();

            ExtendedExtranonce::new(
                range_0.clone(),
                range_1.clone(),
                range_2.clone(),
                Some(static_prefix),
            )
            .expect("Failed to create ExtendedExtranonce with valid ranges")
        };

        let extranonce_prefix_factory_extended = make_extranonce_factory();
        let extranonce_prefix_factory_standard = make_extranonce_factory();

        let channel_manager_data = Arc::new(Mutex::new(ChannelManagerData {
            downstream: HashMap::new(),
            extranonce_prefix_factory_extended,
            extranonce_prefix_factory_standard,
            downstream_id_factory: AtomicUsize::new(1),
            vardiff: HashMap::new(),
            coinbase_outputs,
            last_future_template: None,
            last_new_prev_hash: None,
        }));

        let channel_manager_channel = ChannelManagerChannel {
            tp_sender,
            tp_receiver,
            downstream_sender,
            downstream_receiver,
        };

        // Initialize persistence based on config
        let persistence = if let Some(path) = config.share_persistence_file_path() {
            info!("Initializing file-based share persistence: {}", path);
            let mut share_file_handler = ShareFileHandler::new(path, status_tx.clone()).await;
            let sender = share_file_handler.get_sender();
            let receiver = share_file_handler.get_receiver();

            // Spawn the share file handler task
            task_manager.spawn(async move {
                loop {
                    match receiver.recv().await {
                        Ok(event) => share_file_handler.write_event_to_file(event).await,
                        Err(_) => {
                            info!("Share persistence channel closed, stopping file handler");
                            break;
                        }
                    }
                }
            });

            Persistence::new(Some(ShareFilePersistence::new(sender)))
        } else {
            info!("Share persistence disabled");
            Persistence::new(None)
        };

        let channel_manager = ChannelManager {
            channel_manager_data,
            channel_manager_channel,
            share_batch_size: config.share_batch_size(),
            shares_per_minute: config.shares_per_minute(),
            pool_tag_string: config.pool_signature().to_string(),
            coinbase_reward_script: config.coinbase_reward_script().clone(),
            persistence,
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
        channel_manager_sender: Sender<(u32, Mining<'static>)>,
        channel_manager_receiver: broadcast::Sender<(u32, Mining<'static>)>,
    ) -> PoolResult<()> {
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
                                    stratum_apps::stratum_core::codec_sv2::HandshakeRole::Responder(responder),
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
                                    .super_safe_lock(|data| data.downstream_id_factory.fetch_add(1, Ordering::SeqCst) as u32);


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
        self,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) -> PoolResult<()> {
        let status_sender = StatusSender::ChannelManager(status_sender);
        let mut shutdown_rx = notify_shutdown.subscribe();

        task_manager.spawn(async move {
            let cm = self.clone();
            let vardiff_future = self.run_vardiff_loop();
            tokio::pin!(vardiff_future);
            loop {
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
                    res = cm_template.handle_template_provider_message() => {
                        if let Err(e) = res {
                            error!(error = ?e, "Error handling Template Receiver message");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                    res = cm_downstreams.handle_downstream_mining_message() => {
                        if let Err(e) = res {
                            error!(error = ?e, "Error handling Downstreams message");
                            handle_error(&status_sender, e).await;
                            break;
                        }
                    }
                }
            }
        });
        Ok(())
    }

    // Removes a Downstream entry from the ChannelManager’s state.
    //
    // Given a `downstream_id`, this method:
    // 1. Removes the corresponding Downstream from the `downstream` map.
    #[allow(clippy::result_large_err)]
    fn remove_downstream(&self, downstream_id: u32) -> PoolResult<()> {
        self.channel_manager_data.super_safe_lock(|cm_data| {
            cm_data.downstream.remove(&downstream_id);
        });
        Ok(())
    }

    // Handles messages received from the TP subsystem.
    //
    // This method listens for incoming frames on the `tp_receiver` channel.
    // - If the frame contains a TemplateDistribution message, it forwards it to the template
    //   distribution message handler.
    // - If the frame contains any unsupported message type, an error is returned.
    async fn handle_template_provider_message(&mut self) -> PoolResult<()> {
        if let Ok(message) = self.channel_manager_channel.tp_receiver.recv().await {
            self.handle_template_distribution_message_from_server(None, message)
                .await?;
        }
        Ok(())
    }

    async fn handle_downstream_mining_message(&mut self) -> PoolResult<()> {
        if let Ok((downstream_id, message)) = self
            .channel_manager_channel
            .downstream_receiver
            .recv()
            .await
        {
            self.handle_mining_message_from_client(Some(downstream_id as usize), message)
                .await?;
        }

        Ok(())
    }

    // Runs the vardiff on extended channel.
    fn run_vardiff_on_extended_channel(
        downstream_id: u32,
        channel_id: u32,
        channel_state: &mut ExtendedChannel<
            'static,
            DefaultJobStore<ExtendedJob<'static>>,
            PoolPersistence,
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
                            maximum_target: updated_target.to_le_bytes().into(),
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
        channel: &mut StandardChannel<
            'static,
            DefaultJobStore<StandardJob<'static>>,
            PoolPersistence,
        >,
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
                                maximum_target: updated_target.to_le_bytes().into(),
                            }),
                        )
                            .into(),
                    );
                    debug!(
                        "Updated target for standard channel channel_id={channel_id} to {updated_target:?}"
                    );
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
    async fn run_vardiff_loop(&self) -> PoolResult<()> {
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
    async fn run_vardiff(&self) -> PoolResult<()> {
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
            });

        for message in messages {
            message.forward(&self.channel_manager_channel).await;
        }

        info!("Vardiff update cycle complete");
        Ok(())
    }
}

#[derive(Clone)]
pub enum RouteMessageTo<'a> {
    /// Route to the template provider subsystem.
    TemplateProvider(TemplateDistribution<'a>),
    /// Route to a specific downstream client by ID, along with its mining message.
    Downstream((u32, Mining<'a>)),
}

impl<'a> From<TemplateDistribution<'a>> for RouteMessageTo<'a> {
    fn from(value: TemplateDistribution<'a>) -> Self {
        Self::TemplateProvider(value)
    }
}

impl<'a> From<(u32, Mining<'a>)> for RouteMessageTo<'a> {
    fn from(value: (u32, Mining<'a>)) -> Self {
        Self::Downstream(value)
    }
}

impl RouteMessageTo<'_> {
    pub async fn forward(self, channel_manager_channel: &ChannelManagerChannel) {
        match self {
            RouteMessageTo::Downstream((downstream_id, message)) => {
                _ = channel_manager_channel
                    .downstream_sender
                    .send((downstream_id, message.into_static()));
            }
            RouteMessageTo::TemplateProvider(message) => {
                _ = channel_manager_channel
                    .tp_sender
                    .send(message.into_static())
                    .await;
            }
        }
    }
}
