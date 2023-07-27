use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ProxyConfig {
    pub upstream_address: String,
    pub upstream_port: u16,
    pub upstream_authority_pubkey: Secp256k1PublicKey,
    pub downstream_address: String,
    pub downstream_port: u16,
    pub max_supported_version: u16,
    pub min_supported_version: u16,
    pub min_extranonce2_size: u16,
    pub withhold: bool,
    pub authority_public_key: Secp256k1PublicKey,
    pub authority_secret_key: Secp256k1SecretKey,
    pub jd_config: JdConfig,
    pub cert_validity_sec: u64,
    pub retry: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JdConfig {
    pub jd_address: String,
    pub tp_address: String,
}
