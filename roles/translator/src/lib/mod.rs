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
use async_channel::{bounded, unbounded};
use futures::FutureExt;
use rand::Rng;
use status::Status;
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};
pub use stratum_common::roles_logic_sv2::utils::Mutex;

use tokio::{
    select,
    sync::{broadcast, Notify},
    task::{self, AbortHandle},
};
use tracing::{debug, error, info, warn};
pub use v1::server_to_client;

use config::TranslatorConfig;

use crate::status::State;

pub mod config;
pub mod downstream_sv1;
pub mod error;
pub mod proxy;
pub mod status;
pub mod upstream_sv2;
pub mod utils;

/// The main struct that manages the SV1/SV2 translator.
#[derive(Clone, Debug)]
pub struct TranslatorSv2 {
    config: TranslatorConfig,
    reconnect_wait_time: u64,
    shutdown: Arc<Notify>,
}

impl TranslatorSv2 {
    /// Creates a new `TranslatorSv2`.
    ///
    /// Initializes the translator with the given configuration and sets up
    /// the reconnect wait time.
    pub fn new(config: TranslatorConfig) -> Self {
        let mut rng = rand::thread_rng();
        let wait_time = rng.gen_range(0..=3000);
        Self {
            config,
            reconnect_wait_time: wait_time,
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Starts the translator.
    ///
    /// This method starts the main event loop, which handles connections,
    /// protocol translation, job management, and status reporting.
    pub async fn start(self) {
        // Status channel for components to signal errors or state changes.
        let (tx_status, rx_status) = unbounded();

        // Shared mutable state for the current mining target.
        let target = Arc::new(Mutex::new(vec![0; 32]));

        // Broadcast channel to send SV1 `mining.notify` messages from the Bridge
        // to all connected Downstream (SV1) clients.
        let (tx_sv1_notify, _rx_sv1_notify): (
            broadcast::Sender<server_to_client::Notify>,
            broadcast::Receiver<server_to_client::Notify>,
        ) = broadcast::channel(10);

        // FIXME: Remove this task collector mechanism.
        // Collector for holding handles to spawned tasks for potential abortion.
        let task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>> =
            Arc::new(Mutex::new(Vec::new()));

        // Delegate initial setup and connection logic to internal_start.
        Self::internal_start(
            self.config.clone(),
            tx_sv1_notify.clone(),
            target.clone(),
            tx_status.clone(),
            task_collector.clone(),
        )
        .await;

        debug!("Starting up signal listener");
        let task_collector_ = task_collector.clone();

        debug!("Starting up status listener");
        let wait_time = self.reconnect_wait_time;
        // Check all tasks if is_finished() is true, if so exit
        // Spawn a task to listen for Ctrl+C signal.
        tokio::spawn({
            let shutdown_signal = self.shutdown.clone();
            async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    info!("Interrupt received");
                    // Notify the main loop to begin shutdown.
                    shutdown_signal.notify_one();
                }
            }
        });

        // Main status loop.
        loop {
            select! {
                // Listen for status updates from components.
                task_status = rx_status.recv().fuse() => {
                    if let Ok(task_status_) = task_status {
                        match task_status_.state {
                            // If any critical component shuts down due to error, shut down the whole translator.
                            // Logic needs to be improved, maybe respawn rather than a total shutdown.
                            State::DownstreamShutdown(err) | State::BridgeShutdown(err) | State::UpstreamShutdown(err) => {
                                error!("SHUTDOWN from: {}", err);
                                self.shutdown();
                            }
                            // If the upstream signals a need to reconnect.
                            State::UpstreamTryReconnect(err) => {
                                error!("Trying to reconnect the Upstream because of: {}", err);
                                let task_collector1 = task_collector_.clone();
                                let tx_sv1_notify1 = tx_sv1_notify.clone();
                                let target = target.clone();
                                let tx_status = tx_status.clone();
                                let proxy_config = self.config.clone();
                                // Spawn a new task to handle the reconnection process.
                                tokio::spawn (async move {
                                    // Wait for the randomized delay to avoid thundering herd issues.
                                    tokio::time::sleep(std::time::Duration::from_millis(wait_time)).await;

                                    // Abort all existing tasks before restarting.
                                    let task_collector_aborting = task_collector1.clone();
                                    kill_tasks(task_collector_aborting.clone());

                                    warn!("Trying reconnecting to upstream");
                                    // Restart the internal components.
                                    Self::internal_start(
                                        proxy_config,
                                        tx_sv1_notify1,
                                        target.clone(),
                                        tx_status.clone(),
                                        task_collector1,
                                    )
                                    .await;
                                });
                            }
                            // Log healthy status messages.
                            State::Healthy(msg) => {
                                info!("HEALTHY message: {}", msg);
                            }
                        }
                    } else {
                        info!("Channel closed");
                        kill_tasks(task_collector.clone());
                        break; // Channel closed
                    }
                }
                // Listen for the shutdown signal (from Ctrl+C or explicit call).
                _ = self.shutdown.notified() => {
                    info!("Shutting down gracefully...");
                    kill_tasks(task_collector.clone());
                    break;
                }
            }
        }
    }

    /// Internal helper function to initialize and start the core components.
    ///
    /// Sets up communication channels between the Bridge, Upstream, and Downstream.
    /// Creates, connects, and starts the Upstream (SV2) handler.
    /// Waits for initial data (extranonce, target) from the Upstream.
    /// Creates and starts the Bridge (protocol translation logic).
    /// Starts the Downstream (SV1) listener to accept miner connections.
    /// Collects task handles for graceful shutdown management.
    async fn internal_start(
        proxy_config: TranslatorConfig,
        tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
        target: Arc<Mutex<Vec<u8>>>,
        tx_status: async_channel::Sender<Status<'static>>,
        task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
    ) {
        // Channel: Bridge -> Upstream (SV2 SubmitSharesExtended)
        let (tx_sv2_submit_shares_ext, rx_sv2_submit_shares_ext) = bounded(10);

        // Channel: Downstream -> Bridge (SV1 Messages)
        let (tx_sv1_bridge, rx_sv1_downstream) = unbounded();

        // Channel: Upstream -> Bridge (SV2 NewExtendedMiningJob)
        let (tx_sv2_new_ext_mining_job, rx_sv2_new_ext_mining_job) = bounded(10);

        // Channel: Upstream -> internal_start -> Bridge (Initial Extranonce)
        let (tx_sv2_extranonce, rx_sv2_extranonce) = bounded(1);

        // Channel: Upstream -> Bridge (SV2 SetNewPrevHash)
        let (tx_sv2_set_new_prev_hash, rx_sv2_set_new_prev_hash) = bounded(10);

        // Prepare upstream connection address.
        let upstream_addr = SocketAddr::new(
            IpAddr::from_str(&proxy_config.upstream_address)
                .expect("Failed to parse upstream address!"),
            proxy_config.upstream_port,
        );

        // Shared difficulty configuration
        let diff_config = Arc::new(Mutex::new(proxy_config.upstream_difficulty_config.clone()));
        let task_collector_upstream = task_collector.clone();
        // Instantiate the Upstream (SV2) component.
        let upstream = match upstream_sv2::Upstream::new(
            upstream_addr,
            proxy_config.upstream_authority_pubkey,
            rx_sv2_submit_shares_ext,  // Receives shares from Bridge
            tx_sv2_set_new_prev_hash,  // Sends prev hash updates to Bridge
            tx_sv2_new_ext_mining_job, // Sends new jobs to Bridge
            proxy_config.min_extranonce2_size,
            tx_sv2_extranonce,                           // Sends initial extranonce
            status::Sender::Upstream(tx_status.clone()), // Sends status updates
            target.clone(),                              // Shares target state
            diff_config.clone(),                         // Shares difficulty config
            task_collector_upstream,
        )
        .await
        {
            Ok(upstream) => upstream,
            Err(e) => {
                // FIXME: Send error to status main loop, and then exit.
                error!("Failed to create upstream: {}", e);
                return;
            }
        };
        let task_collector_init_task = task_collector.clone();

        // Spawn the core initialization logic in a separate task.
        // This allows the main `start` loop to remain responsive to shutdown signals
        // even during potentially long-running connection attempts.
        let task = task::spawn(async move {
            // Connect to the SV2 Upstream role
            match upstream_sv2::Upstream::connect(
                upstream.clone(),
                proxy_config.min_supported_version,
                proxy_config.max_supported_version,
            )
            .await
            {
                Ok(_) => info!("Connected to Upstream!"),
                Err(e) => {
                    // FIXME: Send error to status main loop, and then exit.
                    error!("Failed to connect to Upstream EXITING! : {}", e);
                    return;
                }
            }

            // Start the task to parse incoming messages from the Upstream.
            if let Err(e) = upstream_sv2::Upstream::parse_incoming(upstream.clone()) {
                error!("failed to create sv2 parser: {}", e);
                return;
            }

            debug!("Finished starting upstream listener");
            // Start the task handler to process share submissions received from the Bridge.
            if let Err(e) = upstream_sv2::Upstream::handle_submit(upstream.clone()) {
                error!("Failed to create submit handler: {}", e);
                return;
            }

            // Wait to receive the initial extranonce information from the Upstream.
            // This is needed before the Bridge can be fully initialized.
            let (extended_extranonce, up_id) = rx_sv2_extranonce.recv().await.unwrap();
            loop {
                let target: [u8; 32] = target.safe_lock(|t| t.clone()).unwrap().try_into().unwrap();
                if target != [0; 32] {
                    break;
                };
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            let task_collector_bridge = task_collector_init_task.clone();
            // Instantiate the Bridge component.
            let b = proxy::Bridge::new(
                rx_sv1_downstream,
                tx_sv2_submit_shares_ext,
                rx_sv2_set_new_prev_hash,
                rx_sv2_new_ext_mining_job,
                tx_sv1_notify.clone(),
                status::Sender::Bridge(tx_status.clone()),
                extended_extranonce,
                target,
                up_id,
                task_collector_bridge,
            );
            // Start the Bridge's main processing loop.
            proxy::Bridge::start(b.clone());

            // Prepare downstream listening address.
            let downstream_addr = SocketAddr::new(
                IpAddr::from_str(&proxy_config.downstream_address).unwrap(),
                proxy_config.downstream_port,
            );

            let task_collector_downstream = task_collector_init_task.clone();
            // Start accepting connections from Downstream (SV1) miners.
            downstream_sv1::Downstream::accept_connections(
                downstream_addr,
                tx_sv1_bridge,
                tx_sv1_notify,
                status::Sender::DownstreamListener(tx_status.clone()),
                b,
                proxy_config.downstream_difficulty_config,
                diff_config,
                task_collector_downstream,
            );
        }); // End of init task
        let _ =
            task_collector.safe_lock(|t| t.push((task.abort_handle(), "init task".to_string())));
    }

    /// Closes Translator role and any open connection associated with it.
    ///
    /// Note that this method will result in a full exit of the  running
    /// Translator and any open connection most be re-initiated upon new
    /// start.
    pub fn shutdown(&self) {
        self.shutdown.notify_one();
    }
}

// Helper function to iterate through the collected task handles and abort them
fn kill_tasks(task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>) {
    let _ = task_collector.safe_lock(|t| {
        while let Some(handle) = t.pop() {
            handle.0.abort();
            warn!("Killed task: {:?}", handle.1);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::TranslatorSv2;
    use ext_config::{Config, File, FileFormat};

    use crate::*;

    #[tokio::test]
    async fn test_shutdown() {
        let config_path = "config-examples/tproxy-config-hosted-pool-example.toml";
        let config: TranslatorConfig = match Config::builder()
            .add_source(File::new(config_path, FileFormat::Toml))
            .build()
        {
            Ok(settings) => match settings.try_deserialize::<TranslatorConfig>() {
                Ok(c) => c,
                Err(e) => {
                    dbg!(&e);
                    return;
                }
            },
            Err(e) => {
                dbg!(&e);
                return;
            }
        };
        let translator = TranslatorSv2::new(config.clone());
        let cloned = translator.clone();
        tokio::spawn(async move {
            cloned.start().await;
        });
        translator.shutdown();
        let ip = config.downstream_address.clone();
        let port = config.downstream_port;
        let translator_addr = format!("{}:{}", ip, port);
        assert!(std::net::TcpListener::bind(translator_addr).is_ok());
    }
}
