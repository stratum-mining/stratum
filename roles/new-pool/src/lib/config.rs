//! ## Configuration Module
//!
//! Defines [`PoolConfig`], the configuration structure for the Pool, along with its supporting
//! types.
//!
//! This module handles:
//! - Initializing [`PoolConfig`]
//! - Managing [`TemplateProviderConfig`], [`AuthorityConfig`], [`CoinbaseOutput`], and
//!   [`ConnectionConfig`]
//! - Validating and converting coinbase outputs
use std::path::{Path, PathBuf};

use config_helpers_sv2::CoinbaseRewardScript;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};

/// Configuration for the Pool, including connection, authority, and coinbase settings.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct PoolConfig {
    listen_address: String,
    tp_address: String,
    tp_authority_public_key: Option<Secp256k1PublicKey>,
    authority_public_key: Secp256k1PublicKey,
    authority_secret_key: Secp256k1SecretKey,
    cert_validity_sec: u64,
    coinbase_reward_script: CoinbaseRewardScript,
    pool_signature: String,
    shares_per_minute: f32,
    share_batch_size: usize,
    log_file: Option<PathBuf>,
    server_id: u16,
}

impl PoolConfig {
    /// Creates a new instance of the [`PoolConfig`].
    ///
    /// # Panics
    ///
    /// Panics if `coinbase_reward_script` is empty.
    pub fn new(
        pool_connection: ConnectionConfig,
        template_provider: TemplateProviderConfig,
        authority_config: AuthorityConfig,
        coinbase_reward_script: CoinbaseRewardScript,
        shares_per_minute: f32,
        share_batch_size: usize,
        server_id: u16,
    ) -> Self {
        Self {
            listen_address: pool_connection.listen_address,
            tp_address: template_provider.address,
            tp_authority_public_key: template_provider.authority_public_key,
            authority_public_key: authority_config.public_key,
            authority_secret_key: authority_config.secret_key,
            cert_validity_sec: pool_connection.cert_validity_sec,
            coinbase_reward_script,
            pool_signature: pool_connection.signature,
            shares_per_minute,
            share_batch_size,
            log_file: None,
            server_id,
        }
    }

    /// Returns the coinbase output.
    pub fn coinbase_reward_script(&self) -> &CoinbaseRewardScript {
        &self.coinbase_reward_script
    }

    /// Returns Pool listenining address.
    pub fn listen_address(&self) -> &String {
        &self.listen_address
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

    /// Returns the Pool signature.
    pub fn pool_signature(&self) -> &String {
        &self.pool_signature
    }

    /// Return the Template Provider authority public key.
    pub fn tp_authority_public_key(&self) -> Option<&Secp256k1PublicKey> {
        self.tp_authority_public_key.as_ref()
    }

    /// Returns the Template Provider address.
    pub fn tp_address(&self) -> &String {
        &self.tp_address
    }

    /// Returns the share batch size.
    pub fn share_batch_size(&self) -> usize {
        self.share_batch_size
    }

    /// Sets the coinbase output.
    pub fn set_coinbase_reward_script(&mut self, coinbase_output: CoinbaseRewardScript) {
        self.coinbase_reward_script = coinbase_output;
    }

    /// Returns the shares per minute.
    pub fn shares_per_minute(&self) -> f32 {
        self.shares_per_minute
    }

    /// Change TP address.
    pub fn set_tp_address(&mut self, tp_address: String) {
        self.tp_address = tp_address;
    }

    /// Sets the log directory.
    pub fn set_log_dir(&mut self, log_dir: Option<PathBuf>) {
        if let Some(dir) = log_dir {
            self.log_file = Some(dir);
        }
    }
    /// Returns the log directory.
    pub fn log_dir(&self) -> Option<&Path> {
        self.log_file.as_deref()
    }

    /// Returns the server id.
    pub fn server_id(&self) -> u16 {
        self.server_id
    }
}

/// Configuration for connecting to a Template Provider.
pub struct TemplateProviderConfig {
    address: String,
    authority_public_key: Option<Secp256k1PublicKey>,
}

impl TemplateProviderConfig {
    pub fn new(address: String, authority_public_key: Option<Secp256k1PublicKey>) -> Self {
        Self {
            address,
            authority_public_key,
        }
    }
}

/// Pool's authority public and secret keys.
pub struct AuthorityConfig {
    pub public_key: Secp256k1PublicKey,
    pub secret_key: Secp256k1SecretKey,
}

impl AuthorityConfig {
    pub fn new(public_key: Secp256k1PublicKey, secret_key: Secp256k1SecretKey) -> Self {
        Self {
            public_key,
            secret_key,
        }
    }
}

/// Connection settings for the Pool listener.
pub struct ConnectionConfig {
    listen_address: String,
    cert_validity_sec: u64,
    signature: String,
}

impl ConnectionConfig {
    pub fn new(listen_address: String, cert_validity_sec: u64, signature: String) -> Self {
        Self {
            listen_address,
            cert_validity_sec,
            signature,
        }
    }
}
