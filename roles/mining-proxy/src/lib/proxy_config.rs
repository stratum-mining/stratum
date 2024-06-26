use crate::UpstreamMiningValues;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ProxyConfig {
    pub upstreams: Vec<UpstreamMiningValues>,
    pub listen_address: String,
    pub listen_mining_port: u16,
    pub max_supported_version: u16,
    pub min_supported_version: u16,
    pub downstream_share_per_minute: f32,
    pub expected_total_downstream_hr: f32,
    pub reconnect: bool,
}
