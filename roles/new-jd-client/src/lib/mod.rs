#![allow(warnings)]
use std::{
    net::SocketAddr,
    sync::{atomic::AtomicU8, Arc},
};

use async_channel::{unbounded, Receiver, Sender};
use key_utils::Secp256k1PublicKey;
use stratum_common::roles_logic_sv2::bitcoin::consensus::Encodable;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

use crate::{
    channel_manager::ChannelManager,
    config::JobDeclaratorClientConfig,
    error::JDCError,
    job_declarator::JobDeclarator,
    status::{State, Status},
    task_manager::TaskManager,
    template_receiver::TemplateReceiver,
    upstream::Upstream,
    utils::{EitherFrame, ShutdownMessage},
};

mod channel_manager;
pub mod config;
mod downstream;
pub mod error;
pub mod jd_mode;
mod job_declarator;
mod status;
mod task_manager;
mod template_receiver;
mod upstream;
pub mod utils;

pub struct JobDeclaratorClient {
    config: JobDeclaratorClientConfig,
}

impl JobDeclaratorClient {
    pub fn new(config: JobDeclaratorClientConfig) -> Self {
        Self { config }
    }

    pub async fn start(&self) {
        info!(
            "Job declarator client starting... setting up subsystems, User Identity: {}",
            self.config.user_identity()
        );

        let miner_coinbase_outputs = vec![self.config.get_txout()];
        let mut encoded_outputs = vec![];

        miner_coinbase_outputs
            .consensus_encode(&mut encoded_outputs)
            .expect("Invalid coinbase output in config");

        let (notify_shutdown, _) = tokio::sync::broadcast::channel::<ShutdownMessage>(1);
        let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel::<()>(1);
        let task_manager = Arc::new(TaskManager::new());

        let (status_sender, status_receiver) = async_channel::unbounded::<Status>();

        let (channel_manager_to_upstream_sender, channel_manager_to_upstream_receiver) =
            unbounded::<EitherFrame>();
        let (upstream_to_channel_manager_sender, upstream_to_channel_manager_receiver) =
            unbounded::<EitherFrame>();

        let (channel_manager_to_jd_sender, channel_manager_to_jd_receiver) =
            unbounded::<EitherFrame>();
        let (jd_to_channel_manager_sender, jd_to_channel_manager_receiver) =
            unbounded::<EitherFrame>();

        let (channel_manager_to_downstream_sender, channel_manager_to_downstream_receiver) =
            broadcast::channel(10);
        let (downstream_to_channel_manager_sender, downstream_to_channel_manager_receiver) =
            unbounded();

        let (channel_manager_to_tp_sender, channel_manager_to_tp_receiver) =
            unbounded::<EitherFrame>();
        let (tp_to_channel_manager_sender, tp_to_channel_manager_receiver) =
            unbounded::<EitherFrame>();

        debug!("Channels initialized.");

        let listening_address = self.config.listening_address().clone();

        let channel_manager = ChannelManager::new(
            self.config.clone(),
            task_manager.clone(),
            channel_manager_to_upstream_sender.clone(),
            upstream_to_channel_manager_receiver.clone(),
            channel_manager_to_jd_sender.clone(),
            jd_to_channel_manager_receiver.clone(),
            channel_manager_to_tp_sender.clone(),
            tp_to_channel_manager_receiver.clone(),
            channel_manager_to_downstream_sender.clone(),
            downstream_to_channel_manager_receiver,
            status_sender.clone(),
            encoded_outputs.clone(),
        )
        .await
        .unwrap();

        let channel_manager_clone = channel_manager.clone();

        // Initialize the template Receiver
        let tp_address = self.config.tp_address().to_string();
        let tp_pubkey = self.config.tp_authority_public_key().copied();

        let mut template_receiver = TemplateReceiver::new(
            tp_address.clone(),
            tp_pubkey,
            channel_manager_to_tp_receiver,
            tp_to_channel_manager_sender,
            notify_shutdown.clone(),
            shutdown_complete_tx.clone(),
            task_manager.clone(),
            status_sender.clone(),
        )
        .await
        .unwrap();

        info!("Template provider setup done");

        let notify_shutdown_cl = notify_shutdown.clone();
        let shutdown_complete_tx_cl = shutdown_complete_tx.clone();
        let status_sender_cl = status_sender.clone();
        let task_manager_cl = task_manager.clone();

        template_receiver
            .start(
                tp_address,
                notify_shutdown_cl,
                shutdown_complete_tx_cl,
                status_sender_cl,
                task_manager_cl,
                encoded_outputs.clone(),
            )
            .await;

        let upstream_addresses: Vec<_> = self
            .config
            .upstreams()
            .iter()
            .map(|u| {
                let pool_addr = SocketAddr::new(
                    u.pool_address.parse().expect("Invalid pool address"),
                    u.pool_port,
                );
                let jd_addr =
                    SocketAddr::new(u.jd_address.parse().expect("Invalid JD address"), u.jd_port);
                (pool_addr, jd_addr, u.authority_pubkey)
            })
            .collect();

        info!("Attempting to initialize upstream...");

        let (upstream, job_declarator) = match self
            .initialize_jd(
                &upstream_addresses,
                channel_manager_to_upstream_receiver,
                upstream_to_channel_manager_sender,
                channel_manager_to_jd_receiver,
                jd_to_channel_manager_sender,
                notify_shutdown.clone(),
                shutdown_complete_tx.clone(),
                status_sender.clone(),
                task_manager.clone(),
            )
            .await
        {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!("Failed to initialize upstream: {:?}", e);
                return;
            }
        };

        channel_manager
            .start(
                notify_shutdown.clone(),
                shutdown_complete_tx.clone(),
                status_sender.clone(),
                task_manager.clone(),
            )
            .await;

        upstream
            .start(
                self.config.min_supported_version(),
                self.config.max_supported_version(),
                notify_shutdown.clone(),
                shutdown_complete_tx.clone(),
                status_sender.clone(),
                task_manager.clone(),
            )
            .await;

        job_declarator
            .start(
                notify_shutdown.clone(),
                shutdown_complete_tx.clone(),
                status_sender.clone(),
                task_manager.clone(),
            )
            .await;

        channel_manager_clone.allocate_tokens(1).await;

        channel_manager_clone
            .start_downstream_server(
                *self.config.authority_public_key(),
                *self.config.authority_secret_key(),
                self.config.cert_validity_sec(),
                *self.config.listening_address(),
                task_manager.clone(),
                notify_shutdown.clone(),
                shutdown_complete_tx.clone(),
                status_sender.clone(),
                downstream_to_channel_manager_sender.clone(),
                channel_manager_to_downstream_sender.clone(),
            )
            .await;

        info!("Spawning status listener task...");
        let notify_shutdown_clone = notify_shutdown.clone();
        let shutdown_complete_tx_clone = shutdown_complete_tx.clone();
        let status_sender_clone = status_sender.clone();
        let task_manager_clone = task_manager.clone();

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Ctrl+C received — initiating graceful shutdown...");
                    notify_shutdown_clone.send(ShutdownMessage::ShutdownAll).unwrap();
                    break;
                }
                message = status_receiver.recv() => {
                    if let Ok(status) = message {
                        match status.state {
                            State::DownstreamShutdown{downstream_id,..} => {
                                warn!("Downstream {downstream_id:?} disconnected — Channel manager.");
                                notify_shutdown_clone.send(ShutdownMessage::DownstreamShutdown(downstream_id)).unwrap();
                            }
                            State::TemplateReceiverShutdown(_) => {
                                warn!("Template Receiver shutdown requested — initiating full shutdown.");
                                notify_shutdown_clone.send(ShutdownMessage::ShutdownAll).unwrap();
                                break;
                            }
                            State::ChannelManagerShutdown(_) => {
                                warn!("Channel Manager shutdown requested — initiating full shutdown.");
                                notify_shutdown_clone.send(ShutdownMessage::ShutdownAll).unwrap();
                                break;
                            }
                            State::UpstreamShutdown(msg) => {
                                warn!("Upstream connection dropped: {msg:?} — attempting reconnection...");
                                // In these cases we gonna fallback
                            }
                            State::JobDeclaratorShutdown(_) => {
                                warn!("Template Receiver shutdown requested — initiating full shutdown.");
                                // In these cases we gonna fallback
                            }
                        }
                    }
                }
            }
        }

        drop(shutdown_complete_tx);
        info!("Waiting for shutdown completion signals from subsystems...");
        let shutdown_timeout = tokio::time::Duration::from_secs(1);
        tokio::select! {
            _ = shutdown_complete_rx.recv() => {
                info!("All subsystems reported shutdown complete.");
            }
            _ = tokio::time::sleep(shutdown_timeout) => {
                warn!("Graceful shutdown timed out after {shutdown_timeout:?} — forcing shutdown.");
                task_manager.abort_all().await;
            }
        }
        info!("Joining remaining tasks...");
        task_manager.join_all().await;
        info!("JD Client shutdown complete.");
    }

    pub async fn initialize_jd(
        &self,
        upstreams: &[(SocketAddr, SocketAddr, Secp256k1PublicKey)],
        channel_manager_to_upstream_receiver: Receiver<EitherFrame>,
        upstream_to_channel_manager_sender: Sender<EitherFrame>,
        channel_manager_to_jd_receiver: Receiver<EitherFrame>,
        jd_to_channel_manager_sender: Sender<EitherFrame>,
        notify_shutdown: broadcast::Sender<ShutdownMessage>,
        shutdown_complete_tx: tokio::sync::mpsc::Sender<()>,
        status_sender: Sender<Status>,
        task_manager: Arc<TaskManager>,
    ) -> Result<(Upstream, JobDeclarator), JDCError> {
        const MAX_RETRIES: usize = 3;

        for (i, upstream_addr) in upstreams.iter().enumerate() {
            info!(
                "Trying upstream {} of {}: {:?}",
                i + 1,
                upstreams.len(),
                upstream_addr
            );

            for attempt in 1..=MAX_RETRIES {
                info!("Connection attempt {}/{}...", attempt, MAX_RETRIES);

                match try_initialize_single(
                    upstream_addr,
                    self.config.min_supported_version(),
                    self.config.max_supported_version(),
                    upstream_to_channel_manager_sender.clone(),
                    channel_manager_to_upstream_receiver.clone(),
                    jd_to_channel_manager_sender.clone(),
                    channel_manager_to_jd_receiver.clone(),
                    notify_shutdown.clone(),
                    shutdown_complete_tx.clone(),
                    status_sender.clone(),
                    task_manager.clone(),
                )
                .await
                {
                    Ok(pair) => return Ok(pair),
                    Err(e) => {
                        warn!(
                            "Attempt {}/{} failed for {:?}: {:?}",
                            attempt, MAX_RETRIES, upstream_addr, e
                        );
                        if attempt == MAX_RETRIES {
                            warn!(
                                "Max retries reached for {:?}, moving to next upstream",
                                upstream_addr
                            );
                        }
                    }
                }
            }
        }

        tracing::error!("All upstreams failed after {} retries each", MAX_RETRIES);
        Err(JDCError::Shutdown)
    }
}

async fn try_initialize_single(
    upstream_addr: &(SocketAddr, SocketAddr, Secp256k1PublicKey),
    min_version: u16,
    max_version: u16,
    upstream_to_channel_manager_sender: Sender<EitherFrame>,
    channel_manager_to_upstream_receiver: Receiver<EitherFrame>,
    jd_to_channel_manager_sender: Sender<EitherFrame>,
    channel_manager_to_jd_receiver: Receiver<EitherFrame>,
    notify_shutdown: broadcast::Sender<ShutdownMessage>,
    shutdown_complete_tx: tokio::sync::mpsc::Sender<()>,
    status_sender: Sender<Status>,
    task_manager: Arc<TaskManager>,
) -> Result<(Upstream, JobDeclarator), JDCError> {
    info!("Upstream connection in-progress at initialize single");
    let upstream = Upstream::init(
        upstream_addr,
        upstream_to_channel_manager_sender,
        channel_manager_to_upstream_receiver,
        notify_shutdown.clone(),
        shutdown_complete_tx.clone(),
        task_manager.clone(),
        status_sender.clone(),
    )
    .await?;

    info!("Upstream connection done at initialize single");

    let job_declarator = JobDeclarator::init(
        upstream_addr,
        jd_to_channel_manager_sender,
        channel_manager_to_jd_receiver,
        notify_shutdown,
        shutdown_complete_tx,
        task_manager.clone(),
        status_sender.clone(),
    )
    .await?;

    Ok((upstream, job_declarator))
}
