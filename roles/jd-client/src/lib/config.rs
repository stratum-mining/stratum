#![allow(dead_code)]
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::{errors::Error, utils::CoinbaseOutput as CoinbaseOutput_};
use serde::Deserialize;
use std::time::Duration;
use stratum_common::bitcoin::{Amount, TxOut};

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
pub struct JobDeclaratorClientConfig {
    downstream_address: String,
    downstream_port: u16,
    max_supported_version: u16,
    min_supported_version: u16,
    min_extranonce2_size: u16,
    withhold: bool,
    authority_public_key: Secp256k1PublicKey,
    authority_secret_key: Secp256k1SecretKey,
    cert_validity_sec: u64,
    tp_address: String,
    tp_authority_public_key: Option<Secp256k1PublicKey>,
    #[allow(dead_code)]
    retry: u32,
    upstreams: Vec<Upstream>,
    #[serde(deserialize_with = "duration_from_toml")]
    timeout: Duration,
    coinbase_outputs: Vec<CoinbaseOutput>,
}

impl JobDeclaratorClientConfig {
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
        }
    }

    pub fn downstream_address(&self) -> &str {
        &self.downstream_address
    }

    pub fn downstream_port(&self) -> u16 {
        self.downstream_port
    }

    pub fn min_extranonce2_size(&self) -> u16 {
        self.min_extranonce2_size
    }

    pub fn upstreams(&self) -> &Vec<Upstream> {
        &self.upstreams
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn withhold(&self) -> bool {
        self.withhold
    }

    pub fn authority_public_key(&self) -> &Secp256k1PublicKey {
        &self.authority_public_key
    }

    pub fn authority_secret_key(&self) -> &Secp256k1SecretKey {
        &self.authority_secret_key
    }

    pub fn cert_validity_sec(&self) -> u64 {
        self.cert_validity_sec
    }

    pub fn tp_address(&self) -> &str {
        &self.tp_address
    }

    pub fn tp_authority_public_key(&self) -> Option<&Secp256k1PublicKey> {
        self.tp_authority_public_key.as_ref()
    }

    pub fn min_supported_version(&self) -> u16 {
        self.min_supported_version
    }

    pub fn max_supported_version(&self) -> u16 {
        self.max_supported_version
    }
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

pub fn get_coinbase_output(config: &JobDeclaratorClientConfig) -> Result<Vec<TxOut>, Error> {
    let mut result = Vec::new();
    for coinbase_output_pool in &config.coinbase_outputs {
        let coinbase_output: CoinbaseOutput_ = coinbase_output_pool.try_into()?;
        let output_script = coinbase_output.try_into()?;
        result.push(TxOut {
            value: Amount::from_sat(0),
            script_pubkey: output_script,
        });
    }
    match result.is_empty() {
        true => Err(Error::EmptyCoinbaseOutputs),
        _ => Ok(result),
    }
}
