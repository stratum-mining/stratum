#![allow(dead_code)]
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::{errors::Error, utils::CoinbaseOutput as CoinbaseOutput_};
use serde::Deserialize;
use std::time::Duration;
use stratum_common::bitcoin::TxOut;

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
pub struct ProxyConfig {
    pub downstream_address: String,
    pub downstream_port: u16,
    pub max_supported_version: u16,
    pub min_supported_version: u16,
    pub min_extranonce2_size: u16,
    pub withhold: bool,
    pub authority_public_key: Secp256k1PublicKey,
    pub authority_secret_key: Secp256k1SecretKey,
    pub cert_validity_sec: u64,
    pub tp_address: String,
    pub tp_authority_public_key: Option<Secp256k1PublicKey>,
    #[allow(dead_code)]
    pub retry: u32,
    pub upstreams: Vec<Upstream>,
    #[serde(deserialize_with = "duration_from_toml")]
    pub timeout: Duration,
    pub coinbase_outputs: Vec<CoinbaseOutput>,
    pub test_only_do_not_send_solution_to_tp: Option<bool>,
}

pub struct PoolConfig {
    authority_public_key: Secp256k1PublicKey,
    authority_secret_key: Secp256k1SecretKey,
}

impl PoolConfig {
    pub fn new(
        authority_public_key: Secp256k1PublicKey,
        authority_secret_key: Secp256k1SecretKey,
    ) -> Self {
        Self {
            authority_public_key,
            authority_secret_key,
        }
    }
}

pub struct TPConfig {
    cert_validity_sec: u64,
    tp_address: String,
    tp_authority_public_key: Option<Secp256k1PublicKey>,
}

impl TPConfig {
    pub fn new(
        cert_validity_sec: u64,
        tp_address: String,
        tp_authority_public_key: Option<Secp256k1PublicKey>,
    ) -> Self {
        Self {
            cert_validity_sec,
            tp_address,
            tp_authority_public_key,
        }
    }
}

pub struct ProtocolConfig {
    max_supported_version: u16,
    min_supported_version: u16,
    min_extranonce2_size: u16,
    coinbase_outputs: Vec<CoinbaseOutput>,
}

impl ProtocolConfig {
    pub fn new(
        max_supported_version: u16,
        min_supported_version: u16,
        min_extranonce2_size: u16,
        coinbase_outputs: Vec<CoinbaseOutput>,
    ) -> Self {
        Self {
            max_supported_version,
            min_supported_version,
            min_extranonce2_size,
            coinbase_outputs,
        }
    }
}

impl ProxyConfig {
    pub fn new(
        listening_address: std::net::SocketAddr,
        protocol_config: ProtocolConfig,
        withhold: bool,
        pool_config: PoolConfig,
        tp_config: TPConfig,
        upstreams: Vec<Upstream>,
        timeout: Duration,
    ) -> Self {
        Self {
            downstream_address: listening_address.ip().to_string(),
            downstream_port: listening_address.port(),
            max_supported_version: protocol_config.max_supported_version,
            min_supported_version: protocol_config.min_supported_version,
            min_extranonce2_size: protocol_config.min_extranonce2_size,
            withhold,
            authority_public_key: pool_config.authority_public_key,
            authority_secret_key: pool_config.authority_secret_key,
            cert_validity_sec: tp_config.cert_validity_sec,
            tp_address: tp_config.tp_address,
            tp_authority_public_key: tp_config.tp_authority_public_key,
            retry: 0,
            upstreams,
            timeout,
            coinbase_outputs: protocol_config.coinbase_outputs,
            test_only_do_not_send_solution_to_tp: None,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Upstream {
    pub authority_pubkey: Secp256k1PublicKey,
    pub pool_address: String,
    pub jd_address: String,
    pub pool_signature: String, // string be included in coinbase tx input scriptsig
}

impl Upstream {
    pub fn new(
        authority_pubkey: Secp256k1PublicKey,
        pool_address: String,
        jd_address: String,
        pool_signature: String,
    ) -> Self {
        Self {
            authority_pubkey,
            pool_address,
            jd_address,
            pool_signature,
        }
    }
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

pub fn get_coinbase_output(config: &ProxyConfig) -> Result<Vec<TxOut>, Error> {
    let mut result = Vec::new();
    for coinbase_output_pool in &config.coinbase_outputs {
        let coinbase_output: CoinbaseOutput_ = coinbase_output_pool.try_into()?;
        let output_script = coinbase_output.try_into()?;
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
