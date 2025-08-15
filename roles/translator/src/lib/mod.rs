//! ## Translator Sv2
//!
//! Provides the core logic and main struct (`TranslatorSv2`) for running a
//! Stratum V1 to Stratum V2 translation proxy.
//!
//! This module orchestrates the interaction between downstream SV1 miners and upstream SV2
//! applications (proxies or pool servers).
//!
//! The central component is the `TranslatorSv2` struct, which encapsulates the state and
//! provides the `start` method as the main entry point for running the translator service.
//! It relies on several sub-modules (`config`, `downstream_sv1`, `upstream_sv2`, `proxy`, `status`,
//! etc.) for specialized functionalities.
#![allow(clippy::module_inception)]
use async_channel::unbounded;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

pub use v1::server_to_client;

use config::TranslatorConfig;

use crate::{
    status::{State, Status},
    sv1::sv1_server::sv1_server::Sv1Server,
    sv2::{channel_manager::ChannelMode, ChannelManager, Upstream},
    task_manager::TaskManager,
    utils::ShutdownMessage,
};

pub mod config;
pub mod error;
pub mod status;
pub mod sv1;
pub mod sv2;
mod task_manager;
pub mod utils;

/// The main struct that manages the SV1/SV2 translator.
#[derive(Clone, Debug)]
pub struct TranslatorSv2 {
    config: TranslatorConfig,
}

impl TranslatorSv2 {
    /// Creates a new `TranslatorSv2`.
    ///
    /// Initializes the translator with the given configuration and sets up
    /// the reconnect wait time.
    pub fn new(config: TranslatorConfig) -> Self {
        Self { config }
    }

    /// Starts the translator.
    ///
    /// This method starts the main event loop, which handles connections,
    /// protocol translation, job management, and status reporting.
    pub async fn start(self) {
        info!("Starting Translator Proxy...");

        let (notify_shutdown, _) = tokio::sync::broadcast::channel::<ShutdownMessage>(1);
        let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel::<()>(1);
        let task_manager = Arc::new(TaskManager::new());

        let (status_sender, status_receiver) = async_channel::unbounded::<Status>();

        let (channel_manager_to_upstream_sender, channel_manager_to_upstream_receiver) =
            unbounded();
        let (upstream_to_channel_manager_sender, upstream_to_channel_manager_receiver) =
            unbounded();
        let (channel_manager_to_sv1_server_sender, channel_manager_to_sv1_server_receiver) =
            unbounded();
        let (sv1_server_to_channel_manager_sender, sv1_server_to_channel_manager_receiver) =
            unbounded();

        debug!("Channels initialized.");

        let upstream_addresses = self
            .config
            .upstreams
            .iter()
            .map(|upstream| {
                let upstream_addr =
                    SocketAddr::new(upstream.address.parse().unwrap(), upstream.port);
                (upstream_addr, upstream.authority_pubkey)
            })
            .collect::<Vec<_>>();

        let upstream = match Upstream::new(
            &upstream_addresses,
            upstream_to_channel_manager_sender.clone(),
            channel_manager_to_upstream_receiver.clone(),
            notify_shutdown.clone(),
            shutdown_complete_tx.clone(),
        )
        .await
        {
            Ok(upstream) => {
                debug!("Upstream initialized successfully.");
                upstream
            }
            Err(e) => {
                error!("Failed to initialize upstream connection: {e:?}");
                return;
            }
        };

        let channel_manager = Arc::new(ChannelManager::new(
            channel_manager_to_upstream_sender,
            upstream_to_channel_manager_receiver,
            channel_manager_to_sv1_server_sender.clone(),
            sv1_server_to_channel_manager_receiver,
            if self.config.aggregate_channels {
                ChannelMode::Aggregated
            } else {
                ChannelMode::NonAggregated
            },
        ));

        let downstream_addr = SocketAddr::new(
            self.config.downstream_address.parse().unwrap(),
            self.config.downstream_port,
        );

        let sv1_server = Arc::new(Sv1Server::new(
            downstream_addr,
            channel_manager_to_sv1_server_receiver,
            sv1_server_to_channel_manager_sender,
            self.config.clone(),
        ));

        ChannelManager::run_channel_manager_tasks(
            channel_manager.clone(),
            notify_shutdown.clone(),
            shutdown_complete_tx.clone(),
            status_sender.clone(),
            task_manager.clone(),
        )
        .await;

        if let Err(e) = upstream
            .start(
                notify_shutdown.clone(),
                shutdown_complete_tx.clone(),
                status_sender.clone(),
                task_manager.clone(),
            )
            .await
        {
            error!("Failed to start upstream listener: {e:?}");
            return;
        }

        let notify_shutdown_clone = notify_shutdown.clone();
        let shutdown_complete_tx_clone = shutdown_complete_tx.clone();
        let status_sender_clone = status_sender.clone();
        let task_manager_clone = task_manager.clone();
        task_manager.spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        info!("Ctrl+C received — initiating graceful shutdown...");
                        let _ = notify_shutdown_clone.send(ShutdownMessage::ShutdownAll);
                        break;
                    }
                    message = status_receiver.recv() => {
                        if let Ok(status) = message {
                            match status.state {
                                State::DownstreamShutdown{downstream_id,..} => {
                                    warn!("Downstream {downstream_id:?} disconnected — notifying SV1 server.");
                                    let _ = notify_shutdown_clone.send(ShutdownMessage::DownstreamShutdown(downstream_id));
                                }
                                State::Sv1ServerShutdown(_) => {
                                    warn!("SV1 Server shutdown requested — initiating full shutdown.");
                                    let _ = notify_shutdown_clone.send(ShutdownMessage::ShutdownAll);
                                    break;
                                }
                                State::ChannelManagerShutdown(_) => {
                                    warn!("Channel Manager shutdown requested — initiating full shutdown.");
                                    let _ = notify_shutdown_clone.send(ShutdownMessage::ShutdownAll);
                                    break;
                                }
                                State::UpstreamShutdown(msg) => {
                                    warn!("Upstream connection dropped: {msg:?} — attempting reconnection...");

                                    match Upstream::new(
                                        &upstream_addresses,
                                        upstream_to_channel_manager_sender.clone(),
                                        channel_manager_to_upstream_receiver.clone(),
                                        notify_shutdown_clone.clone(),
                                        shutdown_complete_tx_clone.clone(),
                                    ).await {
                                        Ok(upstream) => {
                                            if let Err(e) = upstream
                                                .start(
                                                    notify_shutdown_clone.clone(),
                                                    shutdown_complete_tx_clone.clone(),
                                                    status_sender_clone.clone(),
                                                    task_manager_clone.clone()
                                                )
                                                .await
                                            {
                                                error!("Restarted upstream failed to start: {e:?}");
                                                let _ = notify_shutdown_clone.send(ShutdownMessage::ShutdownAll);
                                                break;
                                            } else {
                                                info!("Upstream restarted successfully.");
                                                // Reset channel manager state and shutdown downstreams in one message
                                                let _ = notify_shutdown_clone.send(ShutdownMessage::UpstreamReconnectedResetAndShutdownDownstreams);
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to reinitialize upstream after disconnect: {e:?}");
                                            let _ = notify_shutdown_clone.send(ShutdownMessage::ShutdownAll);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        if let Err(e) = Sv1Server::start(
            sv1_server,
            notify_shutdown.clone(),
            shutdown_complete_tx.clone(),
            status_sender.clone(),
            task_manager.clone(),
        )
        .await
        {
            error!("SV1 server startup failed: {e:?}");
            notify_shutdown.send(ShutdownMessage::ShutdownAll).unwrap();
        }

        drop(shutdown_complete_tx);
        info!("Waiting for shutdown completion signals from subsystems...");
        let shutdown_timeout = tokio::time::Duration::from_secs(5);
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
        info!("TranslatorSv2 shutdown complete.");
    }
}
