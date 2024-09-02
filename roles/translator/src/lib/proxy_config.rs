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

pub struct UpstreamConfig {
    address: String,
    port: u16,
    authority_pubkey: Secp256k1PublicKey,
    difficulty_config: UpstreamDifficultyConfig,
}

impl UpstreamConfig {
    pub fn new(
        address: String,
        port: u16,
        authority_pubkey: Secp256k1PublicKey,
        difficulty_config: UpstreamDifficultyConfig,
    ) -> Self {
        Self {
            address,
            port,
            authority_pubkey,
            difficulty_config,
        }
    }
}

pub struct DownstreamConfig {
    address: String,
    port: u16,
    difficulty_config: DownstreamDifficultyConfig,
}

impl DownstreamConfig {
    pub fn new(address: String, port: u16, difficulty_config: DownstreamDifficultyConfig) -> Self {
        Self {
            address,
            port,
            difficulty_config,
        }
    }
}

impl ProxyConfig {
    pub fn new(
        upstream: UpstreamConfig,
        downstream: DownstreamConfig,
        max_supported_version: u16,
        min_supported_version: u16,
        min_extranonce2_size: u16,
    ) -> Self {
        Self {
            upstream_address: upstream.address,
            upstream_port: upstream.port,
            upstream_authority_pubkey: upstream.authority_pubkey,
            downstream_address: downstream.address,
            downstream_port: downstream.port,
            max_supported_version,
            min_supported_version,
            min_extranonce2_size,
            downstream_difficulty_config: downstream.difficulty_config,
            upstream_difficulty_config: upstream.difficulty_config,
        }
    }
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

impl DownstreamDifficultyConfig {
    pub fn new(
        min_individual_miner_hashrate: f32,
        shares_per_minute: f32,
        submits_since_last_update: u32,
        timestamp_of_last_update: u64,
    ) -> Self {
        Self {
            min_individual_miner_hashrate,
            shares_per_minute,
            submits_since_last_update,
            timestamp_of_last_update,
        }
    }
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

impl UpstreamDifficultyConfig {
    pub fn new(
        channel_diff_update_interval: u32,
        channel_nominal_hashrate: f32,
        timestamp_of_last_update: u64,
        should_aggregate: bool,
    ) -> Self {
        Self {
            channel_diff_update_interval,
            channel_nominal_hashrate,
            timestamp_of_last_update,
            should_aggregate,
        }
    }
}
