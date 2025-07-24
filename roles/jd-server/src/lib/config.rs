//! ## Configuration Module
//!
//! Defines [`JobDeclaratorServerConfig`], the configuration structure for the Job Declarator Server
//! (JDS).
//!
//! This module handles:
//! - Parsing TOML files via `serde`
//! - Accessing Bitcoin Core RPC parameters
//! - Managing cryptographic keys for Noise authentication
//! - Setting networking and coinbase logic
//!
//! Also defines a helper struct [`CoreRpc`] to group RPC parameters.

use config_helpers_sv2::CoinbaseRewardScript;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, serde::Deserialize, Clone)]
pub struct JobDeclaratorServerConfig {
    #[serde(default = "default_true")]
    full_template_mode_required: bool,
    listen_jd_address: String,
    authority_public_key: Secp256k1PublicKey,
    authority_secret_key: Secp256k1SecretKey,
    cert_validity_sec: u64,
    coinbase_reward_script: CoinbaseRewardScript,
    core_rpc_url: String,
    core_rpc_port: u16,
    core_rpc_user: String,
    core_rpc_pass: String,
    #[serde(deserialize_with = "config_helpers_sv2::duration_from_toml")]
    mempool_update_interval: Duration,
    log_file: Option<PathBuf>,
}

impl JobDeclaratorServerConfig {
    /// Creates a new instance of [`JobDeclaratorServerConfig`].
    ///
    /// # Panics
    ///
    /// Panics if `coinbase_reward_scripts` is empty.
    pub fn new(
        listen_jd_address: String,
        authority_public_key: Secp256k1PublicKey,
        authority_secret_key: Secp256k1SecretKey,
        cert_validity_sec: u64,
        coinbase_reward_script: CoinbaseRewardScript,
        core_rpc: CoreRpc,
        mempool_update_interval: Duration,
    ) -> Self {
        Self {
            full_template_mode_required: true,
            listen_jd_address,
            authority_public_key,
            authority_secret_key,
            cert_validity_sec,
            coinbase_reward_script,
            core_rpc_url: core_rpc.url,
            core_rpc_port: core_rpc.port,
            core_rpc_user: core_rpc.user,
            core_rpc_pass: core_rpc.pass,
            mempool_update_interval,
            log_file: None,
        }
    }

    /// Returns the listening address of the Job Declarator Server.
    pub fn listen_jd_address(&self) -> &str {
        &self.listen_jd_address
    }

    /// Returns the public key of the authority.
    pub fn authority_public_key(&self) -> &Secp256k1PublicKey {
        &self.authority_public_key
    }

    /// Returns the secret key of the authority.
    pub fn authority_secret_key(&self) -> &Secp256k1SecretKey {
        &self.authority_secret_key
    }

    /// Returns the URL of the core RPC.
    pub fn core_rpc_url(&self) -> &str {
        &self.core_rpc_url
    }

    /// Returns the port of the core RPC.
    pub fn core_rpc_port(&self) -> u16 {
        self.core_rpc_port
    }

    /// Returns the user of the core RPC.
    pub fn core_rpc_user(&self) -> &str {
        &self.core_rpc_user
    }

    /// Returns the password of the core RPC.
    pub fn core_rpc_pass(&self) -> &str {
        &self.core_rpc_pass
    }

    /// Returns the coinbase outputs.
    pub fn coinbase_reward_scripts(&self) -> &CoinbaseRewardScript {
        &self.coinbase_reward_script
    }

    /// Returns the certificate validity in seconds.
    pub fn cert_validity_sec(&self) -> u64 {
        self.cert_validity_sec
    }

    /// Returns whether [`Full Template`] is required. Otherwise, [`Coinbase Only`] mode will be
    /// used.
    ///
    /// [`Full Template`]: https://github.com/stratum-mining/sv2-spec/blob/main/06-Job-Declaration-Protocol.md#632-full-template-mode
    /// [`Coinbase Only`]: https://github.com/stratum-mining/sv2-spec/blob/main/06-Job-Declaration-Protocol.md#631-coinbase-only-mode
    pub fn full_template_mode_required(&self) -> bool {
        self.full_template_mode_required
    }

    /// Returns the mempool update interval.
    pub fn mempool_update_interval(&self) -> Duration {
        self.mempool_update_interval
    }

    /// Sets the listening address of Bitcoin core RPC.
    pub fn set_core_rpc_url(&mut self, url: String) {
        self.core_rpc_url = url;
    }

    /// Sets coinbase outputs.
    pub fn set_coinbase_reward_scripts(&mut self, output: CoinbaseRewardScript) {
        self.coinbase_reward_script = output;
    }

    pub fn log_file(&self) -> Option<&Path> {
        self.log_file.as_deref()
    }
    pub fn set_log_file(&mut self, log_file: Option<PathBuf>) {
        if let Some(path) = log_file {
            self.log_file = Some(path);
        }
    }
}

fn default_true() -> bool {
    true
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
    use super::super::JobDeclaratorServer;
    use ext_config::{Config, ConfigError, File, FileFormat};
    use std::path::PathBuf;
    use stratum_common::roles_logic_sv2::bitcoin::{self, Amount, ScriptBuf, TxOut};

    use crate::config::JobDeclaratorServerConfig;

    const COINBASE_CONFIG_TEMPLATE: &'static str = r#"
        full_template_mode_required = true
        authority_public_key = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
        authority_secret_key = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n"
        cert_validity_sec = 3600

        coinbase_reward_script = %COINBASE_REWARD_SCRIPT%

        listen_jd_address = "127.0.0.1:34264"
        core_rpc_url =  "http://127.0.0.1"
        core_rpc_port = 48332
        core_rpc_user =  "username"
        core_rpc_pass =  "password"
        [mempool_update_interval]
        unit = "secs"
        value = 1
    "#;
    const TEST_PK_HEX: &'static str =
        "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075";
    const TEST_INVALID_PK_HEX: &'static str =
        "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7ffffff";

    fn load_config(path: &str) -> JobDeclaratorServerConfig {
        let config_path = PathBuf::from(path);
        assert!(
            config_path.exists(),
            "No config file found at {:?}",
            config_path
        );

        let config_path = config_path.to_str().unwrap();

        let settings = Config::builder()
            .add_source(File::new(config_path, FileFormat::Toml))
            .build()
            .expect("Failed to build config");

        settings.try_deserialize().expect("Failed to parse config")
    }

    fn load_coinbase_config_str(path: &str) -> Result<JobDeclaratorServerConfig, ConfigError> {
        let s = COINBASE_CONFIG_TEMPLATE.replace("%COINBASE_REWARD_SCRIPT%", path);
        let settings = Config::builder()
            .add_source(File::from_str(&s, FileFormat::Toml))
            .build()
            .expect("Failed to build config");

        settings.try_deserialize()
    }

    #[tokio::test]
    async fn test_offline_rpc_url() {
        let mut config = load_config("config-examples/jds-config-hosted-example.toml");
        config.set_core_rpc_url("http://127.0.0.1".to_string());
        let jd = JobDeclaratorServer::new(config);
        assert!(jd.start().await.is_err());
    }

    #[test]
    fn test_get_non_empty_coinbase_reward_script() {
        let pk = TEST_PK_HEX
            .parse::<bitcoin::PublicKey>()
            .expect("Failed to parse public key");
        let config =
            load_coinbase_config_str(&format!("\"wpkh({pk})\"")).expect("Failed to parse config");

        let output = TxOut {
            value: Amount::from_sat(0),
            script_pubkey: config.coinbase_reward_scripts().script_pubkey(),
        };
        let expected_script = ScriptBuf::from_hex(&format!(
            "0014{}",
            pk.wpubkey_hash().expect("compressed key")
        ))
        .expect("hex");
        let expected_transaction_output = TxOut {
            value: Amount::from_sat(0),
            script_pubkey: expected_script,
        };

        assert_eq!(output, expected_transaction_output);
    }

    #[test]
    fn test_get_coinbase_reward_script_empty() {
        let error =
            load_coinbase_config_str("\"\"").expect_err("cannot parse config with empty txout");
        assert_eq!(
            error.to_string(),
            "Miniscript: unexpected «(0 args) while parsing Miniscript»",
        );
    }

    #[test]
    fn test_get_invalid_miniscript_in_coinbase_reward_script() {
        let error = load_coinbase_config_str(&format!("\"INVALID\""))
            .expect_err("Cannot parse config with bad miniscript");
        assert_eq!(
            error.to_string(),
            "Miniscript: unexpected «INVALID(0 args) while parsing Miniscript»",
        );
    }

    #[test]
    fn test_get_invalid_value_in_coinbase_reward_script() {
        let error = load_coinbase_config_str(&format!("\"wpkh({TEST_INVALID_PK_HEX})\""))
            .expect_err("Cannot parse config with bad pubkeys");
        assert_eq!(
            error.to_string(),
            "Miniscript: unexpected «Error while parsing simple public key»",
        );
    }
}
