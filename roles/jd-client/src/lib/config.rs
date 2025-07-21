//! ## JDC Configuration Module
//!
//! The main configuration struct is [`JobDeclaratorClientConfig`], which is typically
//! loaded from a configuration file (e.g., TOML). Helper structs like [`PoolConfig`],
//! [`TPConfig`], [`ProtocolConfig`], and [`Upstream`] are used during the construction
//! of the main configuration.

#![allow(dead_code)]
use config_helpers::CoinbaseRewardScript;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};
use stratum_common::roles_logic_sv2::bitcoin::{Amount, TxOut};

/// Represents the configuration of a Job Declarator Client (JDC).
///
/// This struct holds all the necessary configuration parameters for a JDC instance.
/// JDC can operate in two modes:
///
/// 1. Downstream: Connects to a mining pool (specifically Pool and JDS) and a Template Provider
///    (TP) to receive job templates. The pool and jds connection details are specified in the
///    `upstreams` field, and the TP connection details are in `tp_address`.
/// 2. Upstream: Listens for incoming connections from other downstreams on the address specified in
///    `listening_address`.

#[derive(Debug, Deserialize, Clone)]
pub struct JobDeclaratorClientConfig {
    // The address on which the JDC will listen for incoming connections when acting as an
    // upstream.
    listening_address: SocketAddr,
    // The maximum supported SV2 protocol version.
    max_supported_version: u16,
    // The minimum supported SV2 protocol version.
    min_supported_version: u16,
    // Needs more discussion..
    withhold: bool,
    // The public key used by this JDC for noise encryption.
    authority_public_key: Secp256k1PublicKey,
    /// The secret key used by this JDC for noise encryption.
    authority_secret_key: Secp256k1SecretKey,
    /// The validity period (in seconds) for the certificate used in noise.
    cert_validity_sec: u64,
    /// The address of the TP that this JDC will connect to.
    tp_address: String,
    /// The expected public key of the TP's authority for authentication (optional).
    tp_authority_public_key: Option<Secp256k1PublicKey>,
    /// A list of upstream Job Declarator Servers (JDS) that this JDC can connect to.
    /// JDC can fallover between these upstreams.
    upstreams: Vec<Upstream>,
    /// The timeout duration for network operations.
    #[serde(deserialize_with = "config_helpers::duration_from_toml")]
    timeout: Duration,
    /// This is only used during solo-mining.
    coinbase_reward_script: CoinbaseRewardScript,
    /// A signature string identifying this JDC instance.
    jdc_signature: String,
    /// The path to the log file where JDC will write logs.
    log_file: Option<PathBuf>,
}

impl JobDeclaratorClientConfig {
    /// Creates a new instance of [`JobDeclaratorClientConfig`].
    ///
    /// # Panics
    ///
    /// Panics if `protocol_config.coinbase_reward_script` is empty.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        listening_address: SocketAddr,
        protocol_config: ProtocolConfig,
        withhold: bool,
        pool_config: PoolConfig,
        tp_config: TPConfig,
        upstreams: Vec<Upstream>,
        timeout: Duration,
        jdc_signature: String,
    ) -> Self {
        Self {
            listening_address,
            max_supported_version: protocol_config.max_supported_version,
            min_supported_version: protocol_config.min_supported_version,
            withhold,
            authority_public_key: pool_config.authority_public_key,
            authority_secret_key: pool_config.authority_secret_key,
            cert_validity_sec: tp_config.cert_validity_sec,
            tp_address: tp_config.tp_address,
            tp_authority_public_key: tp_config.tp_authority_public_key,
            upstreams,
            timeout,
            coinbase_reward_script: protocol_config.coinbase_reward_script,
            jdc_signature,
            log_file: None,
        }
    }

    /// Returns the listening address of the Job Declartor Client.
    pub fn listening_address(&self) -> &SocketAddr {
        &self.listening_address
    }

    /// Returns the list of upstreams.
    ///
    /// JDC will try to fallback to the next upstream in case of failure of the current one.
    pub fn upstreams(&self) -> &Vec<Upstream> {
        &self.upstreams
    }

    /// Returns the timeout duration.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Returns the withhold flag.
    pub fn withhold(&self) -> bool {
        self.withhold
    }

    /// Returns the authority public key.
    pub fn authority_public_key(&self) -> &Secp256k1PublicKey {
        &self.authority_public_key
    }

    /// Returns the authority secret key.
    pub fn authority_secret_key(&self) -> &Secp256k1SecretKey {
        &self.authority_secret_key
    }

    /// Returns the certificate validity in seconds.
    pub fn cert_validity_sec(&self) -> u64 {
        self.cert_validity_sec
    }

    /// Returns Template Provider address.
    pub fn tp_address(&self) -> &str {
        &self.tp_address
    }

    /// Returns Template Provider authority public key.
    pub fn tp_authority_public_key(&self) -> Option<&Secp256k1PublicKey> {
        self.tp_authority_public_key.as_ref()
    }

    /// Returns the minimum supported version.
    pub fn min_supported_version(&self) -> u16 {
        self.min_supported_version
    }

    /// Returns the maximum supported version.
    pub fn max_supported_version(&self) -> u16 {
        self.max_supported_version
    }

    /// Returns the JDC signature.
    pub fn jdc_signature(&self) -> &str {
        &self.jdc_signature
    }

    pub fn get_txout(&self) -> TxOut {
        TxOut {
            value: Amount::from_sat(0),
            script_pubkey: self.coinbase_reward_script.script_pubkey().to_owned(),
        }
    }

    pub fn log_file(&self) -> Option<&Path> {
        self.log_file.as_deref()
    }
    pub fn set_log_file(&mut self, log_file: Option<PathBuf>) {
        if let Some(log_file) = log_file {
            self.log_file = Some(log_file);
        }
    }
}

/// Represents pool specific encryption keys.
pub struct PoolConfig {
    authority_public_key: Secp256k1PublicKey,
    authority_secret_key: Secp256k1SecretKey,
}

impl PoolConfig {
    /// Creates a new instance of [`PoolConfig`].
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

/// Represent template provider config for JDC to connect.
pub struct TPConfig {
    // The validity period (in seconds) expected for the Template Provider's certificate.
    cert_validity_sec: u64,
    // The network address of the Template Provider.
    tp_address: String,
    // The expected public key of the Template Provider's authority (optional).
    tp_authority_public_key: Option<Secp256k1PublicKey>,
}

impl TPConfig {
    // Creates a new instance of [`TPConfig`].
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

/// Represent protocol versioning the JDC supports.
pub struct ProtocolConfig {
    // The maximum supported SV2 protocol version.
    max_supported_version: u16,
    // The minimum supported SV2 protocol version.
    min_supported_version: u16,
    // A coinbase output to be included in block templates.
    coinbase_reward_script: CoinbaseRewardScript,
}

impl ProtocolConfig {
    // Creates a new instance of [`ProtocolConfig`].
    pub fn new(
        max_supported_version: u16,
        min_supported_version: u16,
        coinbase_reward_script: CoinbaseRewardScript,
    ) -> Self {
        Self {
            max_supported_version,
            min_supported_version,
            coinbase_reward_script,
        }
    }
}

/// Represents necessary fields required to connect to JDS
#[derive(Debug, Deserialize, Clone)]
pub struct Upstream {
    // The public key of the upstream pool's authority for authentication.
    pub authority_pubkey: Secp256k1PublicKey,
    // The address of the upstream pool's main server.
    pub pool_address: String,
    // The network address of the JDS.
    pub jd_address: String,
}

impl Upstream {
    /// Creates a new instance of [`Upstream`].
    pub fn new(
        authority_pubkey: Secp256k1PublicKey,
        pool_address: String,
        jd_address: String,
    ) -> Self {
        Self {
            authority_pubkey,
            pool_address,
            jd_address,
        }
    }
}
