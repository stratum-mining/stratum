/* #![allow(special_module_name)]
use crate::lib::{mempool, status, Configuration};
use async_channel::unbounded;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::{
    errors::Error, parsers::PoolMessages as JdsMessages, utils::CoinbaseOutput as CoinbaseOutput_,
};
use serde::Deserialize;
use std::convert::{TryFrom, TryInto};

use error_handling::handle_result;
use stratum_common::bitcoin::{Script, TxOut};
use tracing::{error, info, warn};

//use crate::lib::mempool;
use roles_logic_sv2::utils::Mutex;
use std::{sync::Arc, time::Duration};
use tokio::{select, task};

use crate::{lib::job_declarator::JobDeclarator, status::Status};

#[derive(Debug, Deserialize, Clone)]
pub struct CoinbaseOutput {
    output_script_type: String,
    output_script_value: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Configuration {
    pub listen_jd_address: String,
    pub authority_public_key: Secp256k1PublicKey,
    pub authority_secret_key: Secp256k1SecretKey,
    pub cert_validity_sec: u64,
    pub coinbase_outputs: Vec<CoinbaseOutput>,
    pub core_rpc_url: String,
    pub core_rpc_port: u16,
    pub core_rpc_user: String,
    pub core_rpc_pass: String,
    #[serde(deserialize_with = "duration_from_toml")]
    pub mempool_update_timeout: Duration,
} */
#![allow(special_module_name)]
use crate::lib::{mempool, status, Configuration};
use async_channel::unbounded;
use error_handling::handle_result;
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
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
    let mempool = Arc::new(Mutex::new(mempool::JDsMempool::new(
        url.clone(),
        username,
        password,
    )));
    let mempool_update_timeout = config.mempool_update_timeout;
    let mempool_cloned_ = mempool.clone();
    let (status_tx, status_rx) = unbounded();
    let sender = status::Sender::Downstream(status_tx.clone());
    if url.contains("http") {
        let sender_clone = sender.clone();
        task::spawn(async move {
            loop {
                let updated_mempool =
                    mempool::JDsMempool::update_mempool(mempool_cloned_.clone()).await;
                if let Err(err) = updated_mempool {
                    error!("{:?}", err);
                    error!("Unable to connect to Template Provider (possible reasons: not fully synced, down)");
                    handle_result!(sender_clone, Err(err));
                }
                tokio::time::sleep(mempool_update_timeout).await;
            }
        });
    };

    //let (status_tx, status_rx) = unbounded();
    info!("Jds INITIALIZING with config: {:?}", &args.config_path);

    let cloned = config.clone();

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
