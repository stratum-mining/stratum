#![allow(special_module_name)]
use async_channel::{bounded, unbounded};
use codec_sv2::{
    noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey},
    StandardEitherFrame, StandardSv2Frame,
};
use roles_logic_sv2::{bitcoin::PublicKey, parsers::PoolMessages};
use serde::{de::Visitor, Deserialize};
use std::str::FromStr;

use tracing::{error, info};
mod error;
mod lib;
mod status;

use lib::{mining_pool::Pool, template_receiver::TemplateRx};

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

const BLOCK_REWARD: u64 = 5_000_000_000;

const COINBASE_ADD_SZIE: u32 = 100;

const COINBASE_PREFIX: Vec<u8> = vec![];
const COINBASE_SUFFIX: Vec<u8> = vec![];

use tokio::{select, task};

use crate::{lib::job_negotiator::JobNegotiator, status::Status};

/// used to deserialize a string repesentation of an uncompressed secp256k1
/// public key from the pool-config.toml
#[derive(Debug, Clone)]
pub struct PublicKeyWrapper {
    pub pub_key: PublicKey,
}

/// used by serde for deserialization
struct PublicKeyVisitor;

impl<'de> Visitor<'de> for PublicKeyVisitor {
    type Value = bitcoin::PublicKey;
    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a secp255k1 public key string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match PublicKey::from_str(v) {
            Ok(pub_key) => Ok(pub_key),
            Err(e) => Err(E::custom(format!(
                "Invalid coinbase output config public key: {:?}",
                e
            ))),
        }
    }
}

impl<'de> Deserialize<'de> for PublicKeyWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self {
            pub_key: deserializer.deserialize_str(PublicKeyVisitor)?,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Configuration {
    pub listen_address: String,
    pub tp_address: String,
    pub listen_jn_address: String,
    pub authority_public_key: EncodedEd25519PublicKey,
    pub authority_secret_key: EncodedEd25519SecretKey,
    pub cert_validity_sec: u64,
    pub coinbase_outputs: Vec<PublicKeyWrapper>,
    #[cfg(feature = "test_only_allow_unencrypted")]
    pub test_only_listen_adress_plain: String,
}

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
        const DEFAULT_CONFIG_PATH: &'static str = "pool-config.toml";

        pub fn from_args() -> Result<Self, String> {
            let cli_args = std::env::args();

            let config_path = cli_args
                .scan(ArgsState::Next, |state, item| {
                    match std::mem::replace(state, ArgsState::Done) {
                        ArgsState::Next => match item.as_str() {
                            "-c" | "--config" => {
                                *state = ArgsState::ExpectPath;
                                Some(ArgsResult::None)
                            }
                            "-h" | "--help" => Some(ArgsResult::Help(format!(
                                "Usage: -h/--help, -c/--config <path|default {}>",
                                Self::DEFAULT_CONFIG_PATH
                            ))),
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

    let (status_tx, status_rx) = unbounded();
    let (s_new_t, r_new_t) = bounded(10);
    let (s_prev_hash, r_prev_hash) = bounded(10);
    let (s_solution, r_solution) = bounded(10);
    let (s_message_recv_signal, r_message_recv_signal) = bounded(10);
    info!("Pool INITIALIZING with config: {:?}", &args.config_path);

    let template_rx_res = TemplateRx::connect(
        config.tp_address.parse().unwrap(),
        s_new_t,
        s_prev_hash,
        r_solution,
        r_message_recv_signal,
        status::Sender::Upstream(status_tx.clone()),
    )
    .await;
    if let Err(e) = template_rx_res {
        error!("Could not connect to Template Provider: {}", e);
        return;
    }

    let cloned = config.clone();
    task::spawn(async move { JobNegotiator::start(cloned).await });

    Pool::start(
        config.clone(),
        r_new_t,
        r_prev_hash,
        s_solution,
        s_message_recv_signal,
        status::Sender::DownstreamListener(status_tx),
    );

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
        let task_status: Status = task_status.unwrap();

        match task_status.state {
            // Should only be sent by the downstream listener
            status::State::DownstreamShutdown(err) => {
                error!(
                    "SHUTDOWN from Downstream: {}\nTry to restart the downstream listener",
                    err
                );
                break;
            }
            status::State::TemplateProviderShutdown(err) => {
                error!("SHUTDOWN from Upstream: {}\nTry to reconnecting or connecting to a new upstream", err);
                break;
            }
            status::State::Healthy(msg) => {
                info!("HEALTHY message: {}", msg);
            }
        }
    }
}
