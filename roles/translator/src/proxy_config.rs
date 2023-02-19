use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ProxyConfig {
    pub upstream_address: String,
    pub upstream_port: u16,
    pub upstream_authority_pubkey: codec_sv2::noise_sv2::formats::EncodedEd25519PublicKey,
    pub downstream_address: String,
    pub downstream_port: u16,
    pub max_supported_version: u16,
    pub min_supported_version: u16,
    pub min_extranonce2_size: u16,
    pub jn_config: Option<JnConfig>,
    pub coinbase_reward_sat: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JnConfig {
    pub jn_address: String,
    pub tp_address: String,
}
