#![allow(special_module_name)]
use async_channel::{bounded, unbounded};
use codec_sv2::{
    noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey},
    StandardEitherFrame, StandardSv2Frame,
};
use error::OutputScriptError;
use roles_logic_sv2::parsers::PoolMessages;
use serde::Deserialize;
use std::{
    convert::{TryFrom, TryInto},
    str::FromStr,
};

use stratum_common::bitcoin::{
    secp256k1::{All, Secp256k1},
    PublicKey, Script, TxOut,
};

use tracing::{error, info, warn};
mod error;
mod lib;
mod status;

use lib::{mining_pool::Pool, template_receiver::TemplateRx};

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub fn get_coinbase_output(config: &Configuration) -> Result<Vec<TxOut>, OutputScriptError> {
    let result = config
        .coinbase_outputs
        .iter()
        .map(|coinbase_output| {
            coinbase_output.try_into().map(|output_script| TxOut {
                value: 0, //setting value to 0 in order to check that it is correctly updated by TP value_remaining
                script_pubkey: output_script,
            })
        })
        .collect::<Result<Vec<TxOut>, OutputScriptError>>();

    match result.as_deref() {
        Ok([]) => Err(OutputScriptError::EmptyCoinbaseOutputs(
            "Empty coinbase outputs".to_string(),
        )),
        _ => result,
    }
}

impl TryFrom<&CoinbaseOutput> for Script {
    type Error = OutputScriptError;

    fn try_from(value: &CoinbaseOutput) -> Result<Self, Self::Error> {
        match value.output_script_type.as_str() {
            "P2PK" => {
                let pub_key = PublicKey::from_str(value.output_script_value.as_str())
                    .expect("Invalid output_script_value for P2PK. It must be a valid public key.");
                Ok(Script::new_p2pk(&pub_key))
            }
            "P2PKH" => {
                let pub_key_hash = PublicKey::from_str(value.output_script_value.as_str())
                    .expect("Invalid output_script_value for P2PKH. It must be a valid public key.")
                    .pubkey_hash();
                Ok(Script::new_p2pkh(&pub_key_hash))
            }
            "P2WPKH" => {
                let w_pub_key_hash = PublicKey::from_str(value.output_script_value.as_str())
                    .expect(
                        "Invalid output_script_value for P2WPKH. It must be a valid public key.",
                    )
                    .wpubkey_hash()
                    .unwrap();
                Ok(Script::new_v0_p2wpkh(&w_pub_key_hash))
            }
            "P2SH" => {
                let script_hashed = Script::from_str(&value.output_script_value)
                    .expect("Invalid output_script_value for P2SH. It must be a valid script.")
                    .script_hash();
                Ok(Script::new_p2sh(&script_hashed))
            }
            "P2WSH" => {
                let w_script_hashed = Script::from_str(&value.output_script_value)
                    .expect("Invalid output_script_value for P2WSH. It must be a valid script.")
                    .wscript_hash();
                Ok(Script::new_v0_p2wsh(&w_script_hashed))
            }
            "P2TR" => {
                // From the bip
                //
                // Conceptually, every Taproot output corresponds to a combination of
                // a single public key condition (the internal key),
                // and zero or more general conditions encoded in scripts organized in a tree.
                let pub_key = PublicKey::from_str(value.output_script_value.as_str())
                    .expect("Invalid output_script_value for P2TR. It must be a valid public key.");
                Ok({
                    let (pubkey_only, _) = pub_key.inner.x_only_public_key();
                    Script::new_v1_p2tr::<All>(&Secp256k1::<All>::new(), pubkey_only, None)
                })
            }
            _ => Err(OutputScriptError::UnknownScriptType(
                value.output_script_type.clone(),
            )),
        }
    }
}

use tokio::select;

use crate::status::Status;

#[derive(Debug, Deserialize, Clone)]
pub struct CoinbaseOutput {
    output_script_type: String,
    output_script_value: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Configuration {
    pub listen_address: String,
    pub tp_address: String,
    pub authority_public_key: EncodedEd25519PublicKey,
    pub authority_secret_key: EncodedEd25519SecretKey,
    pub cert_validity_sec: u64,
    pub coinbase_outputs: Vec<CoinbaseOutput>,
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
    let coinbase_output_result = get_coinbase_output(&config);
    let coinbase_output_len = match coinbase_output_result {
        Ok(coinbase_output) => coinbase_output.len() as u32,
        Err(err) => {
            error!("Failed to get coinbase output: {:?}", err);
            return;
        }
    };
    let template_rx_res = TemplateRx::connect(
        config.tp_address.parse().unwrap(),
        s_new_t,
        s_prev_hash,
        r_solution,
        r_message_recv_signal,
        status::Sender::Upstream(status_tx.clone()),
        coinbase_output_len,
    )
    .await;

    if let Err(e) = template_rx_res {
        error!("Could not connect to Template Provider: {}", e);
        return;
    }

    let pool = Pool::start(
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
            status::State::DownstreamInstanceDropped(downstream_id) => {
                warn!("Dropping downstream instance {} from pool", downstream_id);
                if pool
                    .safe_lock(|p| p.remove_downstream(downstream_id))
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}
