//! ## JDS Core Runtime Module
//!
//! This module serves as the central coordination layer of the Job Declarator Server (JDS).
//!
//! It connects all core components:
//! - `mempool`: a local cache of Bitcoin transactions, synchronized via RPC.
//! - `job_declarator`: protocol logic for handling downstream job declaration clients.
//! - `status`: a simple health/error propagation mechanism.
//! - `config`: configuration loader and accessor.
//!
//! The [`JobDeclaratorServer`] struct represents the entrypoint to the system's async runtime.
//! It is launched from `main.rs` and responsible for:
//! - validating config
//! - initializing the mempool
//! - spawning all background tasks
//! - handling graceful shutdowns and task health reporting
//!
//! All components communicate asynchronously using `async_channel`.

pub mod config;
pub mod error;
pub mod job_declarator;
pub mod mempool;
pub mod status;
use async_channel::{bounded, unbounded, Receiver, Sender};
use config::JobDeclaratorServerConfig;
use error::JdsError;
use error_handling::handle_result;
use job_declarator::JobDeclarator;
use mempool::error::JdsMempoolError;
pub use rpc_sv2::Uri;
use std::{ops::Sub, str::FromStr, sync::Arc};
use stratum_common::roles_logic_sv2::{
    codec_sv2::{StandardEitherFrame, StandardSv2Frame},
    parsers_sv2::AnyMessage as JdsMessages,
    utils::Mutex,
};
use tokio::{select, task};
use tracing::{error, info, warn};

/// Type alias for incoming SV2 messages.
pub type Message = JdsMessages<'static>;

/// SV2 frame carrying a parsed JDS message.
pub type StdFrame = StandardSv2Frame<Message>;

/// SV2 frame that can be either a standard message or handshake frame.
pub type EitherFrame = StandardEitherFrame<Message>;

/// The core runtime orchestrator for the JDS system.
///
/// Starts all essential services (mempool polling, block submission, job declaration protocol)
/// and monitors for shutdown conditions or task failures via a `status` channel.
#[derive(Debug, Clone)]
pub struct JobDeclaratorServer {
    config: JobDeclaratorServerConfig,
}

impl JobDeclaratorServer {
    /// Constructs a new instance using the given TOML configuration.
    pub fn new(config: JobDeclaratorServerConfig) -> Self {
        Self { config }
    }

    /// Starts the Job Declarator Server runtime.
    ///
    /// This method spawns the following:
    /// - a task for polling the Bitcoin Core mempool
    /// - a task for processing new block submissions from downstream clients
    /// - a task for listening to incoming downstream connections
    /// - a task for integrating transaction data into the local mempool
    ///
    /// It concludes with a `select!` loop that reacts to:
    /// - SIGINT (`tokio::signal::ctrl_c()`)
    /// - messages from the `status` channel
    ///
    /// When a critical error or interrupt is received, the server shuts down cleanly.
    pub async fn start(&self) -> Result<(), JdsError> {
        let mut config = self.config.clone();
        // Normalize URL to avoid trailing slashes.
        if config.core_rpc_url().ends_with('/') {
            config.set_core_rpc_url(config.core_rpc_url().trim_end_matches('/').to_string());
        }
        let url = config.core_rpc_url().to_string() + ":" + &config.core_rpc_port().to_string();
        let username = config.core_rpc_user();
        let password = config.core_rpc_pass();
        // Channel for sending new blocks to the Bitcoin node
        let (new_block_sender, new_block_receiver): (Sender<String>, Receiver<String>) =
            bounded(10);
        let url = Uri::from_str(&url.clone()).expect("Invalid core rpc url");
        // Shared mempool instance
        let mempool = Arc::new(Mutex::new(mempool::JDsMempool::new(
            url,
            username.to_string(),
            password.to_string(),
            new_block_receiver,
        )));
        let mempool_update_interval = config.mempool_update_interval();
        let mempool_cloned_ = mempool.clone();
        let mempool_cloned_1 = mempool.clone();
        // Pre-flight check: can we reach the RPC node
        if let Err(e) = mempool::JDsMempool::health(mempool_cloned_1.clone()).await {
            error!("JDS Connection with bitcoin core failed {:?}", e);
            return Err(JdsError::MempoolError(e));
        }
        let (status_tx, status_rx) = unbounded();
        let sender = status::Sender::Downstream(status_tx.clone());
        let mut last_empty_mempool_warning =
            std::time::Instant::now().sub(std::time::Duration::from_secs(60));

        let sender_update_mempool = sender.clone();
        // ========== Task: Periodically update the mempool via RPC ========== //
        task::spawn(async move {
            loop {
                let update_mempool_result: Result<(), mempool::error::JdsMempoolError> =
                    mempool::JDsMempool::update_mempool(mempool_cloned_.clone()).await;
                if let Err(err) = update_mempool_result {
                    match err {
                        JdsMempoolError::EmptyMempool => {
                            if last_empty_mempool_warning.elapsed().as_secs() >= 60 {
                                warn!("{:?}", err);
                                warn!("Template Provider is running, but its mempool is empty (possible reasons: you're testing in testnet, signet, or regtest)");
                                last_empty_mempool_warning = std::time::Instant::now();
                            }
                        }
                        JdsMempoolError::NoClient => {
                            mempool::error::handle_error(&err);
                            handle_result!(sender_update_mempool, Err(err));
                        }
                        JdsMempoolError::Rpc(_) => {
                            mempool::error::handle_error(&err);
                            handle_result!(sender_update_mempool, Err(err));
                        }
                        JdsMempoolError::PoisonLock(_) => {
                            mempool::error::handle_error(&err);
                            handle_result!(sender_update_mempool, Err(err));
                        }
                    }
                }
                tokio::time::sleep(mempool_update_interval).await;
                // DO NOT REMOVE THIS LINE
                //let _transactions =
                // mempool::JDsMempool::_get_transaction_list(mempool_cloned_.clone());
            }
        });

        // ========== Task: Listen for SubmitSolution events ========== //
        let mempool_cloned = mempool.clone();
        let sender_submit_solution = sender.clone();
        task::spawn(async move {
            loop {
                let result = mempool::JDsMempool::on_submit(mempool_cloned.clone()).await;
                if let Err(err) = result {
                    match err {
                        JdsMempoolError::EmptyMempool => {
                            if last_empty_mempool_warning.elapsed().as_secs() >= 60 {
                                warn!("{:?}", err);
                                warn!("Template Provider is running, but its mempool is empty (possible reasons: you're testing in testnet, signet, or regtest)");
                                last_empty_mempool_warning = std::time::Instant::now();
                            }
                        }
                        _ => {
                            // TODO here there should be a better error managmenet
                            mempool::error::handle_error(&err);
                            handle_result!(sender_submit_solution, Err(err));
                        }
                    }
                }
            }
        });

        // ========== Task: Launch Job Declarator server ========== //
        let cloned = config.clone();
        let mempool_cloned = mempool.clone();
        let (sender_add_txs_to_mempool, receiver_add_txs_to_mempool) = unbounded();
        task::spawn(async move {
            JobDeclarator::start(
                cloned,
                sender,
                mempool_cloned,
                new_block_sender,
                sender_add_txs_to_mempool,
            )
            .await
        });

        // ========== Task: Add transactions to mempool when received ========== //
        task::spawn(async move {
            loop {
                if let Ok(add_transactions_to_mempool) = receiver_add_txs_to_mempool.recv().await {
                    let mempool_cloned = mempool.clone();
                    task::spawn(async move {
                        match mempool::JDsMempool::add_tx_data_to_mempool(
                            mempool_cloned,
                            add_transactions_to_mempool,
                        )
                        .await
                        {
                            Ok(_) => (),
                            Err(err) => {
                                // TODO
                                // here there should be a better error management
                                mempool::error::handle_error(&err);
                            }
                        }
                    });
                }
            }
        });

        // ========== Central Runtime Loop: Shutdown and Error Reactions ========== //
        loop {
            let task_status = select! {
                task_status = status_rx.recv() => task_status,
                interrupt_signal = tokio::signal::ctrl_c() => {
                    match interrupt_signal {
                        Ok(()) => {
                            info!("Interrupt received");
                        },
                        Err(err) => {
                            error!("Unable to listen for interrupt signal: {}", err);
                            // we also shut down in case of error
                        },
                    }
                    break;
                }
            };
            let task_status: status::Status = task_status.unwrap();

            match task_status.state {
                // Should only be sent by the downstream listener
                status::State::DownstreamShutdown(err) => {
                    error!(
                        "SHUTDOWN from Downstream: {}\nTry to restart the downstream listener",
                        err
                    );
                }
                status::State::TemplateProviderShutdown(err) => {
                    error!("SHUTDOWN from Upstream: {}\nTry to reconnecting or connecting to a new upstream", err);
                    break;
                }
                status::State::Healthy(msg) => {
                    info!("HEALTHY message: {}", msg);
                }
                status::State::DownstreamInstanceDropped(downstream_id) => {
                    warn!("Dropping downstream instance {} from jds", downstream_id);
                }
            }
        }
        Ok(())
    }
}
