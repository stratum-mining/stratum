use crate::{JdsError, JdsResult};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::{utils::CoinbaseOutput as CoinbaseOutput_, Error as RolesLogicSv2Error};
use serde::Deserialize;
use std::{
    convert::{TryFrom, TryInto},
    time::Duration,
};
use stratum_common::bitcoin::{Script, TxOut};

pub fn get_coinbase_output(config: &JdsConfig) -> JdsResult<Vec<TxOut>> {
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
        true => Err(JdsError::RolesLogicSv2(
            RolesLogicSv2Error::EmptyCoinbaseOutputs,
        )),
        _ => Ok(result),
    }
}

impl TryFrom<&CoinbaseOutput> for CoinbaseOutput_ {
    type Error = RolesLogicSv2Error;

    fn try_from(pool_output: &CoinbaseOutput) -> Result<Self, RolesLogicSv2Error> {
        match pool_output.output_script_type.as_str() {
            "P2PK" | "P2PKH" | "P2WPKH" | "P2SH" | "P2WSH" | "P2TR" => Ok(CoinbaseOutput_ {
                output_script_type: pool_output.clone().output_script_type,
                output_script_value: pool_output.clone().output_script_value,
            }),
            _ => Err(RolesLogicSv2Error::UnknownOutputScriptType),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CoinbaseOutput {
    output_script_type: String,
    output_script_value: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JdsConfig {
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
