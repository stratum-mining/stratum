use codec_sv2::noise_sv2::formats::EncodedEd25519PublicKey;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ProxyConfig {
    pub upstream_address: String,
    pub upstream_port: u16,
    pub upstream_authority_pubkey: EncodedEd25519PublicKey,
    pub downstream_address: String,
    pub downstream_port: u16,
    pub max_supported_version: u16,
    pub min_supported_version: u16,
}
