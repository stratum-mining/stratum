#![allow(special_module_name)]
use crate::lib::{
    mempool::{self, error::JdsMempoolError},
    status, Configuration,
};
use async_channel::{bounded, unbounded, Receiver, Sender};
use error_handling::handle_result;
use roles_logic_sv2::utils::Mutex;
use std::{ops::Sub, sync::Arc};
use tokio::{select, task};
use tracing::{error, info, warn};
mod lib;

use lib::job_declarator::JobDeclarator;

mod args {
    use std::path::PathBuf;

    #[derive(Debug)]
    pub struct Args {
        pub config_path: PathBuf,
    }

    enum ArgsState {
        Next,
        ExpectPath,
        Done,
    }

    enum ArgsResult {
        Config(PathBuf),
        None,
        Help(String),
    }

    impl Args {
        const DEFAULT_CONFIG_PATH: &'static str = "jds-config.toml";
        const HELP_MSG: &'static str =
            "Usage: -h/--help, -c/--config <path|default jds-config.toml>";

        pub fn from_args() -> Result<Self, String> {
            let cli_args = std::env::args();

            if cli_args.len() == 1 {
                println!("Using default config path: {}", Self::DEFAULT_CONFIG_PATH);
                println!("{}\n", Self::HELP_MSG);
            }

            let config_path = cli_args
                .scan(ArgsState::Next, |state, item| {
                    match std::mem::replace(state, ArgsState::Done) {
                        ArgsState::Next => match item.as_str() {
                            "-c" | "--config" => {
                                *state = ArgsState::ExpectPath;
                                Some(ArgsResult::None)
                            }
                            "-h" | "--help" => Some(ArgsResult::Help(Self::HELP_MSG.to_string())),
                            _ => {
                                *state = ArgsState::Next;

                                Some(ArgsResult::None)
                            }
                        },
                        ArgsState::ExpectPath => Some(ArgsResult::Config(PathBuf::from(item))),
                        ArgsState::Done => None,
                    }
                })
                .last();
            let config_path = match config_path {
                Some(ArgsResult::Config(p)) => p,
                Some(ArgsResult::Help(h)) => return Err(h),
                _ => PathBuf::from(Self::DEFAULT_CONFIG_PATH),
            };
            Ok(Self { config_path })
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = match args::Args::from_args() {
        Ok(cfg) => cfg,
        Err(help) => {
            error!("{}", help);
            return;
        }
    };

    // Load config
    let config: Configuration = match std::fs::read_to_string(&args.config_path) {
        Ok(c) => match toml::from_str(&c) {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to parse config: {}", e);
                return;
            }
        },
        Err(e) => {
            error!("Failed to read config: {}", e);
            return;
        }
    };

    let url = config.core_rpc_url.clone() + ":" + &config.core_rpc_port.clone().to_string();
    let username = config.core_rpc_user.clone();
    let password = config.core_rpc_pass.clone();
    // TODO should we manage what to do when the limit is reaced?
    let (new_block_sender, new_block_receiver): (Sender<String>, Receiver<String>) = bounded(10);
    let mempool = Arc::new(Mutex::new(mempool::JDsMempool::new(
        url.clone(),
        username,
        password,
        new_block_receiver,
    )));
    let mempool_update_interval = config.mempool_update_interval;
    let mempool_cloned_ = mempool.clone();
    let (status_tx, status_rx) = unbounded();
    let sender = status::Sender::Downstream(status_tx.clone());
    let mut last_empty_mempool_warning =
        std::time::Instant::now().sub(std::time::Duration::from_secs(60));

    // TODO if the jd-server is launched with core_rpc_url empty, the following flow is never
    // taken. Consequentally new_block_receiver in JDsMempool::on_submit is never read, possibly
    // reaching the channel bound. The new_block_sender is given as input to JobDeclarator::start()
    if url.contains("http") {
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
                //let _transactions = mempool::JDsMempool::_get_transaction_list(mempool_cloned_.clone());
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
    };

    info!("Jds INITIALIZING with config: {:?}", &args.config_path);

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
                    match lib::mempool::JDsMempool::add_tx_data_to_mempool(
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
}
