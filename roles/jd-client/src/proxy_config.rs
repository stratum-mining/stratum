use codec_sv2::noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ProxyConfig {
    pub upstream_address: String,
    pub upstream_port: u16,
    pub upstream_authority_pubkey: EncodedEd25519PublicKey,
    pub downstream_address: String,
    pub downstream_port: u16,
    pub max_supported_version: u16,
    pub min_supported_version: u16,
    pub min_extranonce2_size: u16,
    pub withhold: bool,
    pub authority_public_key: EncodedEd25519PublicKey,
    pub authority_secret_key: EncodedEd25519SecretKey,
    pub jd_config: JdConfig,
    pub cert_validity_sec: u64,
    pub retry: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JdConfig {
    pub jd_address: String,
    pub tp_address: String,
    pub pool_signature: String, // string be included in coinbase tx input scriptsig
}
