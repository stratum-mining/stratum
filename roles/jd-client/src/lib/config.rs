use config_helpers_sv2::CoinbaseRewardScript;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};
use stratum_common::roles_logic_sv2::bitcoin::{Amount, TxOut};

#[derive(Debug, Deserialize, Clone)]
pub struct JobDeclaratorClientConfig {
    // The address on which the JDC will listen for incoming connections when acting as an
    // upstream.
    listening_address: SocketAddr,
    // The maximum supported SV2 protocol version.
    max_supported_version: u16,
    // The minimum supported SV2 protocol version.
    min_supported_version: u16,
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
    /// This is only used during solo-mining.
    pub coinbase_reward_script: CoinbaseRewardScript,
    /// A signature string identifying this JDC instance.
    jdc_signature: String,
    /// The path to the log file where JDC will write logs.
    log_file: Option<PathBuf>,
    /// User Identity
    user_identity: String,
    /// Shares per minute
    shares_per_minute: f64,
    /// share batch size
    share_batch_size: u64,
    /// JDC mode: FullTemplate or CoinbaseOnly
    #[serde(deserialize_with = "deserialize_jdc_mode", default)]
    pub mode: ConfigJDCMode,
}

impl JobDeclaratorClientConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        listening_address: SocketAddr,
        protocol_config: ProtocolConfig,
        user_identity: String,
        shares_per_minute: f64,
        share_batch_size: u64,
        pool_config: PoolConfig,
        tp_config: TPConfig,
        upstreams: Vec<Upstream>,
        jdc_signature: String,
        jdc_mode: Option<String>,
    ) -> Self {
        Self {
            listening_address,
            max_supported_version: protocol_config.max_supported_version,
            min_supported_version: protocol_config.min_supported_version,
            authority_public_key: pool_config.authority_public_key,
            authority_secret_key: pool_config.authority_secret_key,
            cert_validity_sec: tp_config.cert_validity_sec,
            tp_address: tp_config.tp_address,
            tp_authority_public_key: tp_config.tp_authority_public_key,
            upstreams,
            coinbase_reward_script: protocol_config.coinbase_reward_script,
            jdc_signature,
            log_file: None,
            user_identity,
            shares_per_minute,
            share_batch_size,
            mode: jdc_mode
                .map(|s| s.parse::<ConfigJDCMode>().unwrap_or_default())
                .unwrap_or_default(),
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
    pub fn user_identity(&self) -> &str {
        &self.user_identity
    }

    pub fn shares_per_minute(&self) -> f64 {
        self.shares_per_minute
    }

    pub fn share_batch_size(&self) -> u64 {
        self.share_batch_size
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum ConfigJDCMode {
    #[default]
    FullTemplate,
    CoinbaseOnly,
}

impl std::str::FromStr for ConfigJDCMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "COINBASEONLY" => Ok(ConfigJDCMode::CoinbaseOnly),
            _ => Ok(ConfigJDCMode::FullTemplate),
        }
    }
}

fn deserialize_jdc_mode<'de, D>(deserializer: D) -> Result<ConfigJDCMode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    Ok(ConfigJDCMode::from_str(&s).unwrap_or_default())
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
    pub pool_port: u16,
    // The network address of the JDS.
    pub jds_address: String,
    pub jds_port: u16,
}

impl Upstream {
    /// Creates a new instance of [`Upstream`].
    pub fn new(
        authority_pubkey: Secp256k1PublicKey,
        pool_address: String,
        pool_port: u16,
        jds_address: String,
        jds_port: u16,
    ) -> Self {
        Self {
            authority_pubkey,
            pool_address,
            pool_port,
            jds_address,
            jds_port,
        }
    }
}
