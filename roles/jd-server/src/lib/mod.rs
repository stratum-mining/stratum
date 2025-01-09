pub mod error;
pub mod job_declarator;
pub mod mempool;
pub mod status;

use async_channel::{bounded, unbounded, Receiver, Sender};
use error_handling::handle_result;
use job_declarator::JobDeclarator;
use mempool::error::JdsMempoolError;
use roles_logic_sv2::utils::Mutex;
use std::{ops::Sub, sync::Arc};
use tokio::{select, task};
use tracing::{error, info, warn};

use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::{
    errors::Error, parsers::PoolMessages as JdsMessages, utils::CoinbaseOutput as CoinbaseOutput_,
};
use serde::Deserialize;
use std::{
    convert::{TryFrom, TryInto},
    time::Duration,
};
use stratum_common::bitcoin::{Script, TxOut};

pub type Message = JdsMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[derive(Debug, Clone)]
pub struct JobDeclaratorServer {
    config: Configuration,
}

impl JobDeclaratorServer {
    pub fn new(config: Configuration) -> Self {
        Self { config }
    }
    pub async fn start(&self) {
        let config = self.config.clone();
        let url = config.core_rpc_url.clone() + ":" + &config.core_rpc_port.clone().to_string();
        let username = config.core_rpc_user.clone();
        let password = config.core_rpc_pass.clone();
        // TODO should we manage what to do when the limit is reaced?
        let (new_block_sender, new_block_receiver): (Sender<String>, Receiver<String>) =
            bounded(10);
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
        // reaching the channel bound. The new_block_sender is given as input to
        // JobDeclarator::start()
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
        };

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
    }
}

pub fn get_coinbase_output(config: &Configuration) -> Result<Vec<TxOut>, Error> {
    let mut result = Vec::new();
    for coinbase_output_pool in &config.coinbase_outputs {
        let coinbase_output: CoinbaseOutput_ = coinbase_output_pool.try_into()?;
        let output_script: Script = coinbase_output.try_into()?;
        result.push(TxOut {
            value: 0,
            script_pubkey: output_script,
        });
    }
    match result.is_empty() {
        true => Err(Error::EmptyCoinbaseOutputs),
        _ => Ok(result),
    }
}

impl TryFrom<&CoinbaseOutput> for CoinbaseOutput_ {
    type Error = Error;

    fn try_from(pool_output: &CoinbaseOutput) -> Result<Self, Self::Error> {
        match pool_output.output_script_type.as_str() {
            "P2PK" | "P2PKH" | "P2WPKH" | "P2SH" | "P2WSH" | "P2TR" => Ok(CoinbaseOutput_ {
                output_script_type: pool_output.clone().output_script_type,
                output_script_value: pool_output.clone().output_script_value,
            }),
            _ => Err(Error::UnknownOutputScriptType),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CoinbaseOutput {
    output_script_type: String,
    output_script_value: String,
}

impl CoinbaseOutput {
    pub fn new(output_script_type: String, output_script_value: String) -> Self {
        Self {
            output_script_type,
            output_script_value,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Configuration {
    #[serde(default = "default_true")]
    pub async_mining_allowed: bool,
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
    pub mempool_update_interval: Duration,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CoreRpc {
    url: String,
    port: u16,
    user: String,
    pass: String,
}

impl CoreRpc {
    pub fn new(url: String, port: u16, user: String, pass: String) -> Self {
        Self {
            url,
            port,
            user,
            pass,
        }
    }
}

impl Configuration {
    pub fn new(
        listen_jd_address: String,
        authority_public_key: Secp256k1PublicKey,
        authority_secret_key: Secp256k1SecretKey,
        cert_validity_sec: u64,
        coinbase_outputs: Vec<CoinbaseOutput>,
        core_rpc: CoreRpc,
        mempool_update_interval: Duration,
    ) -> Self {
        Self {
            async_mining_allowed: true,
            listen_jd_address,
            authority_public_key,
            authority_secret_key,
            cert_validity_sec,
            coinbase_outputs,
            core_rpc_url: core_rpc.url,
            core_rpc_port: core_rpc.port,
            core_rpc_user: core_rpc.user,
            core_rpc_pass: core_rpc.pass,
            mempool_update_interval,
        }
    }
}

fn default_true() -> bool {
    true
}

fn duration_from_toml<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Helper {
        unit: String,
        value: u64,
    }

    let helper = Helper::deserialize(deserializer)?;
    match helper.unit.as_str() {
        "seconds" => Ok(Duration::from_secs(helper.value)),
        "secs" => Ok(Duration::from_secs(helper.value)),
        "s" => Ok(Duration::from_secs(helper.value)),
        "milliseconds" => Ok(Duration::from_millis(helper.value)),
        "millis" => Ok(Duration::from_millis(helper.value)),
        "ms" => Ok(Duration::from_millis(helper.value)),
        "microseconds" => Ok(Duration::from_micros(helper.value)),
        "micros" => Ok(Duration::from_micros(helper.value)),
        "us" => Ok(Duration::from_micros(helper.value)),
        "nanoseconds" => Ok(Duration::from_nanos(helper.value)),
        "nanos" => Ok(Duration::from_nanos(helper.value)),
        "ns" => Ok(Duration::from_nanos(helper.value)),
        // ... add other units as needed
        _ => Err(serde::de::Error::custom("Unsupported duration unit")),
    }
}
#[cfg(test)]
mod tests {
    use ext_config::{Config, File, FileFormat};
    use std::path::PathBuf;

    use super::*;

    fn load_config(path: &str) -> Configuration {
        let config_path = PathBuf::from(path);
        assert!(
            config_path.exists(),
            "No config file found at {:?}",
            config_path
        );

        let config_path = config_path.to_str().unwrap();

        let settings = Config::builder()
            .add_source(File::new(&config_path, FileFormat::Toml))
            .build()
            .expect("Failed to build config");

        settings.try_deserialize().expect("Failed to parse config")
    }

    #[test]
    fn test_get_coinbase_output_non_empty() {
        let config = load_config("config-examples/jds-config-hosted-example.toml");
        let outputs = get_coinbase_output(&config).expect("Failed to get coinbase output");

        let expected_output = CoinbaseOutput_ {
            output_script_type: "P2WPKH".to_string(),
            output_script_value:
                "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
        };
        let expected_script: Script = expected_output.try_into().unwrap();
        let expected_transaction_output = TxOut {
            value: 0,
            script_pubkey: expected_script,
        };

        assert_eq!(outputs[0], expected_transaction_output);
    }

    #[test]
    fn test_get_coinbase_output_empty() {
        let mut config = load_config("config-examples/jds-config-hosted-example.toml");
        config.coinbase_outputs.clear();

        let result = get_coinbase_output(&config);
        assert!(
            matches!(result, Err(Error::EmptyCoinbaseOutputs)),
            "Expected an error for empty coinbase outputs"
        );
    }

    #[test]
    fn test_try_from_valid_input() {
        let input = CoinbaseOutput {
            output_script_type: "P2PKH".to_string(),
            output_script_value:
                "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
        };
        let result: Result<CoinbaseOutput_, _> = (&input).try_into();
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_from_invalid_input() {
        let input = CoinbaseOutput {
            output_script_type: "INVALID".to_string(),
            output_script_value:
                "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
        };
        let result: Result<CoinbaseOutput_, _> = (&input).try_into();
        assert!(matches!(result, Err(Error::UnknownOutputScriptType)));
    }
}
