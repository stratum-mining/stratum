#![allow(special_module_name)]
use async_channel::unbounded;
use tracing::{error, info, warn};
use crate::lib::{mempool, Configuration, status};
use roles_logic_sv2::utils::Mutex;
use std::{sync::Arc, time::Duration};
use tokio::{select, task};
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
    let mempool = Arc::new(Mutex::new(mempool::JDsMempool::new(
        url.clone(),
        username,
        password,
    )));
    let mempool_cloned_ = mempool.clone();
    if url.contains("http") {
        task::spawn(async move {
            loop {
                let _ = mempool::JDsMempool::update_mempool(mempool_cloned_.clone()).await;
                // TODO this should be configurable by the user
                tokio::time::sleep(Duration::from_millis(10000)).await;
            }
        });
    };

    let (status_tx, status_rx) = unbounded();
    info!("Jds INITIALIZING with config: {:?}", &args.config_path);

    let cloned = config.clone();
    let sender = status::Sender::Downstream(status_tx.clone());
    let mempool_cloned = mempool.clone();
    task::spawn(async move { JobDeclarator::start(cloned, sender, mempool_cloned).await });

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
