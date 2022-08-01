use serde::Deserialize;

/// Upstream configuration values
#[derive(Debug, Deserialize)]
pub struct UpstreamValues {
    pub address: String,
    pub port: u16,
    pub pub_key: [u8; 32],
}

/// Upstream server connection configuration
#[derive(Debug, Deserialize)]
pub struct Config {
    pub upstreams: Vec<UpstreamValues>,
    pub listen_address: String,
    pub listen_mining_port: u16,
    pub max_supported_version: u16,
    pub min_supported_version: u16,
}

pub(crate) fn max_supported_version() -> u16 {
    let config_file = std::fs::read_to_string("proxy-config.toml").unwrap();
    let config: Config = toml::from_str(&config_file).unwrap();
    config.max_supported_version
}

pub(crate) fn min_supported_version() -> u16 {
    let config_file = std::fs::read_to_string("proxy-config.toml").unwrap();
    let config: Config = toml::from_str(&config_file).unwrap();
    config.min_supported_version
}
