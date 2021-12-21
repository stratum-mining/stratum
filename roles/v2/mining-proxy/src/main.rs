//! Configurable Sv2 it support extended and group channel
//! Upstream means another proxy or a pool
//! Downstream means another proxy or a mining device
//!
//! ## From messages_sv2
//! UpstreamMining is the (sub)protocol that a proxy must implement in order to
//! understant Downstream mining messages.
//!
//! DownstreamMining is the (sub)protocol that a proxy must implement in order to
//! understand Upstream mining messages
//!
//! Same thing for DownstreamCommon and UpstreamCommon
//!
//! ## Internal
//! DownstreamMiningNode rapresent the Downstream as defined above as the proxy need to understand
//! some message (TODO which one?) from downstream it DownstreamMiningNode it implement
//! UpstreamMining. DownstreamMiningNode implement UpstreamCommon in order to setup a connection
//! with the downstream node.
//!
//! UpstreamMiningNode rapresent the upstream as defined above as the proxy only need to relay
//! downstream messages coming from downstream UpstreamMiningNode do not (for now) implement
//! DownstreamMining. UpstreamMiningNode implement DownstreamCommon (TODO) in order to setup a
//! connection with with the upstream node.
//!
//! A Downstream that signal the capacity to handle group channels can open more than one channel.
//! A Downstream that signal the incapacity to handle group channels can open only one channel.
//!
mod lib;
use std::net::{IpAddr, SocketAddr};

use async_std::sync::{Arc, Mutex};
use lib::upstream_mining::{UpstreamMiningNode, UpstreamMiningNodes};
use serde::Deserialize;
use std::str::FromStr;

// TODO make them configurable via flags or config file
pub const MAX_SUPPORTED_VERSION: u16 = 2;
pub const MIN_SUPPORTED_VERSION: u16 = 2;

#[derive(Debug)]
pub struct Id {
    state: u32,
}

impl Id {
    pub fn new() -> Self {
        Self { state: 0 }
    }
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.state += 1;
        self.state
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
pub struct UpstreamValues {
    address: String,
    port: u16,
    pub_key: [u8; 32],
}

#[derive(Debug, Deserialize)]
pub struct Config {
    upstreams: Vec<UpstreamValues>,
    listen_address: String,
    listen_mining_port: u16,
}

/// 1. the proxy scan all the upstreams and map them
/// 2. donwstream open a connetcion with proxy
/// 3. downstream send SetupConnection
/// 4. a mining_channle::Upstream is created
/// 5. upstream_mining::UpstreamMiningNodes is used to pair this downstream with the most suitable
///    upstream
/// 6. mining_channle::Upstream create a new downstream_mining::DownstreamMiningNode embedding
///    itself in it
/// 7. normal operation between the paired downstream_mining::DownstreamMiningNode and
///    upstream_mining::UpstreamMiningNode begin
#[async_std::main]
async fn main() {
    // Scan all the upstreams and map them
    let config_file = std::fs::read_to_string("proxy-config.toml").unwrap();
    let config: Config = toml::from_str(&config_file).unwrap();
    let upstreams = config.upstreams;
    let upstream_mining_nodes = upstreams
        .iter()
        .map(|upstream| {
            let socket =
                SocketAddr::new(IpAddr::from_str(&upstream.address).unwrap(), upstream.port);
            Arc::new(Mutex::new(UpstreamMiningNode::new(
                socket,
                upstream.pub_key,
            )))
        })
        .collect();
    let mut upsteam_mining_nodes = UpstreamMiningNodes {
        nodes: upstream_mining_nodes,
    };
    upsteam_mining_nodes.scan().await;

    // Wait for downstream connection
    let socket = SocketAddr::new(
        IpAddr::from_str(&config.listen_address).unwrap(),
        config.listen_mining_port,
    );
    crate::lib::downstream_mining::listen_for_downstream_mining(socket, upsteam_mining_nodes).await
}
