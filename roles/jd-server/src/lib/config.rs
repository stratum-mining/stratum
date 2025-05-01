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

use config_helpers::CoinbaseOutput;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::utils::CoinbaseOutput as CoinbaseOutput_;
use serde::Deserialize;
use std::{convert::TryInto, time::Duration};
use stratum_common::bitcoin::{Amount, TxOut};

#[derive(Debug, serde::Deserialize, Clone)]
pub struct JobDeclaratorServerConfig {
    #[serde(default = "default_true")]
    full_template_mode_required: bool,
    listen_jd_address: String,
    authority_public_key: Secp256k1PublicKey,
    authority_secret_key: Secp256k1SecretKey,
    cert_validity_sec: u64,
    coinbase_outputs: Vec<CoinbaseOutput>,
    core_rpc_url: String,
    core_rpc_port: u16,
    core_rpc_user: String,
    core_rpc_pass: String,
    #[serde(deserialize_with = "config_helpers::duration_from_toml")]
    mempool_update_interval: Duration,
}

impl JobDeclaratorServerConfig {
    /// Creates a new instance of [`JobDeclaratorServerConfig`].
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
            full_template_mode_required: true,
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
    pub fn coinbase_outputs(&self) -> &Vec<CoinbaseOutput> {
        &self.coinbase_outputs
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
    pub fn set_coinbase_outputs(&mut self, outputs: Vec<CoinbaseOutput>) {
        self.coinbase_outputs = outputs;
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
    use ext_config::{Config, File, FileFormat};
    use roles_logic_sv2::utils::CoinbaseOutput as CoinbaseOutput_;
    use std::{convert::TryInto, path::PathBuf};
    use stratum_common::bitcoin::{Amount, ScriptBuf, TxOut};

    use crate::config::JobDeclaratorServerConfig;

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

    #[tokio::test]
    async fn test_offline_rpc_url() {
        let mut config = load_config("config-examples/jds-config-hosted-example.toml");
        config.set_core_rpc_url("http://127.0.0.1".to_string());
        let jd = JobDeclaratorServer::new(config);
        assert!(jd.start().await.is_err());
    }

    #[test]
    fn test_get_txout_non_empty() {
        let config = load_config("config-examples/jds-config-hosted-example.toml");
        let outputs = config.get_txout().expect("Failed to get coinbase output");

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
    fn test_get_txout_empty() {
        let mut config = load_config("config-examples/jds-config-hosted-example.toml");
        config.set_coinbase_outputs(Vec::new());

        let result = &config.get_txout();
        assert!(
            matches!(result, Err(roles_logic_sv2::Error::EmptyCoinbaseOutputs)),
            "Expected an error for empty coinbase outputs"
        );
    }

    #[test]
    fn test_try_from_valid_input() {
        let input = config_helpers::CoinbaseOutput::new(
            "P2PKH".to_string(),
            "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
        );
        let result: Result<CoinbaseOutput_, _> = (&input).try_into();
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_from_invalid_input() {
        let input = config_helpers::CoinbaseOutput::new(
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
    fn get_txout_invalid_output_script_type() {
        let mut config = load_config("config-examples/jds-config-hosted-example.toml");
        config.set_coinbase_outputs(vec![config_helpers::CoinbaseOutput::new(
            "INVALID".to_string(),
            "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
        )]);
        let outputs = config.get_txout();
        assert!(
            matches!(
                outputs,
                Err(roles_logic_sv2::Error::UnknownOutputScriptType)
            ),
            "Expected an error for unknown output script type"
        );
    }

    #[test]
    fn get_txout_supported_output_script_types() {
        let config = load_config("config-examples/jds-config-hosted-example.toml");
        let outputs = config.get_txout().expect("Failed to get coinbase output");
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
