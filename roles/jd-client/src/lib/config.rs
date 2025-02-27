#![allow(dead_code)]
use config_helpers::CoinbaseOutput;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::utils::CoinbaseOutput as CoinbaseOutput_;
use serde::Deserialize;
use std::time::Duration;
use stratum_common::bitcoin::{Amount, TxOut};

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
    #[serde(deserialize_with = "config_helpers::duration_from_toml")]
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

    pub fn get_txout(&self) -> Result<Vec<TxOut>, roles_logic_sv2::Error> {
        let mut result = Vec::new();
        for coinbase_output_pool in &self.coinbase_outputs {
            let coinbase_output: CoinbaseOutput_ = coinbase_output_pool.try_into()?;
            let output_script = coinbase_output.try_into()?;
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
