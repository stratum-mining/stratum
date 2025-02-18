use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::utils::CoinbaseOutput as CoinbaseOutput_;
use std::convert::TryFrom;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct PoolConfig {
    listen_address: String,
    tp_address: String,
    tp_authority_public_key: Option<Secp256k1PublicKey>,
    authority_public_key: Secp256k1PublicKey,
    authority_secret_key: Secp256k1SecretKey,
    cert_validity_sec: u64,
    coinbase_outputs: Vec<CoinbaseOutput>,
    pool_signature: String,
}

impl PoolConfig {
    pub fn new(
        pool_connection: ConnectionConfig,
        template_provider: TemplateProviderConfig,
        authority_config: AuthorityConfig,
        coinbase_outputs: Vec<CoinbaseOutput>,
    ) -> Self {
        Self {
            listen_address: pool_connection.listen_address,
            tp_address: template_provider.address,
            tp_authority_public_key: template_provider.authority_public_key,
            authority_public_key: authority_config.public_key,
            authority_secret_key: authority_config.secret_key,
            cert_validity_sec: pool_connection.cert_validity_sec,
            coinbase_outputs,
            pool_signature: pool_connection.signature,
        }
    }

    pub fn coinbase_outputs(&self) -> &Vec<CoinbaseOutput> {
        self.coinbase_outputs.as_ref()
    }

    pub fn listen_address(&self) -> &String {
        &self.listen_address
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

    pub fn pool_signature(&self) -> &String {
        &self.pool_signature
    }

    pub fn tp_authority_public_key(&self) -> Option<&Secp256k1PublicKey> {
        self.tp_authority_public_key.as_ref()
    }

    pub fn tp_address(&self) -> &String {
        &self.tp_address
    }

    pub fn set_coinbase_outputs(&mut self, coinbase_outputs: Vec<CoinbaseOutput>) {
        self.coinbase_outputs = coinbase_outputs;
    }
}

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

#[derive(Clone, Debug, serde::Deserialize)]
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
    type Error = roles_logic_sv2::errors::Error;

    fn try_from(pool_output: &CoinbaseOutput) -> Result<Self, Self::Error> {
        match pool_output.output_script_type.as_str() {
            "TEST" | "P2PK" | "P2PKH" | "P2WPKH" | "P2SH" | "P2WSH" | "P2TR" => {
                Ok(CoinbaseOutput_ {
                    output_script_type: pool_output.clone().output_script_type,
                    output_script_value: pool_output.clone().output_script_value,
                })
            }
            _ => Err(roles_logic_sv2::Error::UnknownOutputScriptType),
        }
    }
}
