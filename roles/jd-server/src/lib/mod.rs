pub mod config;
pub mod error;
pub mod job_declarator;
pub mod mempool;
pub mod status;
use async_channel::{bounded, unbounded, Receiver, Sender};
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use config::JobDeclaratorServerConfig;
use error::JdsError;
use error_handling::handle_result;
use job_declarator::JobDeclarator;
use mempool::error::JdsMempoolError;
use roles_logic_sv2::{parsers::AnyMessage as JdsMessages, utils::Mutex};
pub use rpc_sv2::Uri;
use std::{ops::Sub, str::FromStr, sync::Arc};
use tokio::{select, task};
use tracing::{error, info, warn};

pub type Message = JdsMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[derive(Debug, Clone)]
pub struct JobDeclaratorServer {
    config: JobDeclaratorServerConfig,
}

impl JobDeclaratorServer {
    pub fn new(config: JobDeclaratorServerConfig) -> Self {
        Self { config }
    }
    pub async fn start(&self) -> Result<(), JdsError> {
        let mut config = self.config.clone();
        // In case the url came with a trailing slash, we remove it to make sure we end up with
        // `{scheme}://{host}:{port}` format.
        if config.core_rpc_url().ends_with('/') {
            config.set_core_rpc_url(config.core_rpc_url().trim_end_matches('/').to_string());
        }
        let url = config.core_rpc_url().to_string() + ":" + &config.core_rpc_port().to_string();
        let username = config.core_rpc_user();
        let password = config.core_rpc_pass();
        // TODO should we manage what to do when the limit is reaced?
        let (new_block_sender, new_block_receiver): (Sender<String>, Receiver<String>) =
            bounded(10);
        let url = Uri::from_str(&url.clone()).expect("Invalid core rpc url");
        let mempool = Arc::new(Mutex::new(mempool::JDsMempool::new(
            url,
            username.to_string(),
            password.to_string(),
            new_block_receiver,
        )));
        let mempool_update_interval = config.mempool_update_interval();
        let mempool_cloned_ = mempool.clone();
        let mempool_cloned_1 = mempool.clone();
        if let Err(e) = mempool::JDsMempool::health(mempool_cloned_1.clone()).await {
            error!("JDS Connection with bitcoin core failed {:?}", e);
            return Err(JdsError::MempoolError(e));
        }
        let (status_tx, status_rx) = unbounded();
        let sender = status::Sender::Downstream(status_tx.clone());
        let mut last_empty_mempool_warning =
            std::time::Instant::now().sub(std::time::Duration::from_secs(60));

        let sender_update_mempool = sender.clone();
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

        // Start the error handling loop
        // See `./status.rs` and `utils/error_handling` for information on how this operates
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
