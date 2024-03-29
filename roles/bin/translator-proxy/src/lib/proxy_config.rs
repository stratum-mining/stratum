use key_utils::Secp256k1PublicKey;
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
    pub downstream_difficulty_config: DownstreamDifficultyConfig,
    pub upstream_difficulty_config: UpstreamDifficultyConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DownstreamDifficultyConfig {
    pub min_individual_miner_hashrate: f32,
    pub shares_per_minute: f32,
    #[serde(default = "u32::default")]
    pub submits_since_last_update: u32,
    #[serde(default = "u64::default")]
    pub timestamp_of_last_update: u64,
}

impl PartialEq for DownstreamDifficultyConfig {
    fn eq(&self, other: &Self) -> bool {
        other.min_individual_miner_hashrate.round() as u32
            == self.min_individual_miner_hashrate.round() as u32
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpstreamDifficultyConfig {
    pub channel_diff_update_interval: u32,
    pub channel_nominal_hashrate: f32,
    #[serde(default = "u64::default")]
    pub timestamp_of_last_update: u64,
    #[serde(default = "bool::default")]
    pub should_aggregate: bool,
}
