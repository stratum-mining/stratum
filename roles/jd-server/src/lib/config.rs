use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::utils::CoinbaseOutput as CoinbaseOutput_;
use serde::Deserialize;
use std::{
    convert::{TryFrom, TryInto},
    time::Duration,
};
use stratum_common::bitcoin::{Amount, ScriptBuf, TxOut};

#[derive(Debug, serde::Deserialize, Clone)]
pub struct JobDeclaratorServerConfig {
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

impl JobDeclaratorServerConfig {
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
    #[derive(serde::Deserialize)]
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

impl TryFrom<&CoinbaseOutput> for CoinbaseOutput_ {
    type Error = roles_logic_sv2::errors::Error;

    fn try_from(pool_output: &CoinbaseOutput) -> Result<Self, Self::Error> {
        match pool_output.output_script_type.as_str() {
            "P2PK" | "P2PKH" | "P2WPKH" | "P2SH" | "P2WSH" | "P2TR" => Ok(CoinbaseOutput_ {
                output_script_type: pool_output.clone().output_script_type,
                output_script_value: pool_output.clone().output_script_value,
            }),
            _ => Err(roles_logic_sv2::errors::Error::UnknownOutputScriptType),
        }
    }
}

pub fn get_coinbase_output(
    config: &JobDeclaratorServerConfig,
) -> Result<Vec<TxOut>, roles_logic_sv2::Error> {
    let mut result = Vec::new();
    for coinbase_output_pool in &config.coinbase_outputs {
        let coinbase_output: CoinbaseOutput_ = coinbase_output_pool.try_into()?;
        let output_script: ScriptBuf = coinbase_output.try_into()?;
        result.push(TxOut {
            value: Amount::from_sat(0),
            script_pubkey: output_script,
        });
    }
    match result.is_empty() {
        true => Err(roles_logic_sv2::Error::EmptyCoinbaseOutputs),
        _ => Ok(result),
    }
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

#[cfg(test)]
mod tests {
    use ext_config::{Config, File, FileFormat};
    use roles_logic_sv2::utils::CoinbaseOutput as CoinbaseOutput_;
    use std::{convert::TryInto, path::PathBuf};
    use stratum_common::bitcoin::{Amount, ScriptBuf, TxOut};

    use super::super::JobDeclaratorServer;

    use super::*;

    fn load_config(path: &str) -> JobDeclaratorServerConfig {
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

    #[tokio::test]
    async fn test_invalid_rpc_url() {
        let mut config = load_config("config-examples/jds-config-hosted-example.toml");
        config.core_rpc_url = "invalid".to_string();
        assert!(JobDeclaratorServer::new(config).is_err());
    }

    #[tokio::test]
    async fn test_offline_rpc_url() {
        let mut config = load_config("config-examples/jds-config-hosted-example.toml");
        config.core_rpc_url = "http://127.0.0.1".to_string();
        let jd = JobDeclaratorServer::new(config).unwrap();
        assert!(jd.start().await.is_err());
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
        let expected_script: ScriptBuf = expected_output.try_into().unwrap();
        let expected_transaction_output = TxOut {
            value: Amount::from_sat(0),
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
            matches!(result, Err(roles_logic_sv2::Error::EmptyCoinbaseOutputs)),
            "Expected an error for empty coinbase outputs"
        );
    }

    #[test]
    fn test_try_from_valid_input() {
        let input = CoinbaseOutput::new(
            "P2PKH".to_string(),
            "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
        );
        let result: Result<CoinbaseOutput_, _> = (&input).try_into();
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_from_invalid_input() {
        let input = CoinbaseOutput::new(
            "INVALID".to_string(),
            "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
        );
        let result: Result<CoinbaseOutput_, _> = (&input).try_into();
        assert!(matches!(
            result,
            Err(roles_logic_sv2::Error::UnknownOutputScriptType)
        ));
    }

    #[test]
    fn get_coinbase_output_invalid_output_script_type() {
        let mut config = load_config("config-examples/jds-config-hosted-example.toml");
        config.coinbase_outputs[0].output_script_type = "INVALID".to_string();
        let result = get_coinbase_output(&config);
        assert!(
            matches!(result, Err(roles_logic_sv2::Error::UnknownOutputScriptType)),
            "Expected an error for unknown output script type"
        );
    }

    #[test]
    fn get_coinbase_output_supported_output_script_types() {
        let config = load_config("config-examples/jds-config-hosted-example.toml");
        let outputs = get_coinbase_output(&config).expect("Failed to get coinbase output");
        let expected_output = CoinbaseOutput_ {
            output_script_type: "P2WPKH".to_string(),
            output_script_value:
                "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
        };
        let expected_script: ScriptBuf = expected_output.try_into().unwrap();
        let expected_transaction_output = TxOut {
            value: Amount::from_sat(0),
            script_pubkey: expected_script,
        };

        assert_eq!(outputs[0], expected_transaction_output);
    }
}
