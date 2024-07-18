pub mod error;
pub mod job_declarator;
pub mod mempool;
pub mod status;

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
    use std::path::PathBuf;

    use super::*;

    fn load_config(path: &str) -> Configuration {
        let config_path = PathBuf::from(path);
        assert!(
            config_path.exists(),
            "No config file found at {:?}",
            config_path
        );

        let config_string =
            std::fs::read_to_string(config_path).expect("Failed to read the config file");
        toml::from_str(&config_string).expect("Failed to parse config")
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
