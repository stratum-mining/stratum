use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ProxyConfig {
    pub upstream_address: String,
    pub upstream_port: u16,
    pub upstream_authority_pubkey: [u8; 32],
    pub downstream_address: String,
    pub downstream_port: u16,
    pub max_supported_version: u16,
    pub min_supported_version: u16,
    pub min_extranonce2_size: u16,
}
