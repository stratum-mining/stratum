//! Configurable Sv2 it support extended and group channel
//! Upstream means another proxy or a pool
//! Downstream means another proxy or a mining device
//!
//! UpstreamMining is the trait that a proxy must implement in order to
//! understant Downstream mining messages.
//!
//! DownstreamMining is the trait that a proxy must implement in order to
//! understand Upstream mining messages
//!
//! Same thing for DownstreamCommon and UpstreamCommon but for common messages
//!
//! DownstreamMiningNode implement both UpstreamMining and UpstreamCommon
//!
//! UpstreamMiningNode implement both DownstreamMining and DownstreamCommon
//!
//! A Downstream that signal the capacity to handle group channels can open more than one channel.
//! A Downstream that signal the incapacity to handle group channels can open only one channel.
//!
#![allow(special_module_name)]
mod args;
mod lib;

use lib::{
    downstream_mining, get_common_routing_logic, get_routing_logic, initialize_r_logic,
    initialize_upstreams, remove_upstream, upstream_mining, ChannelKind, ProxyConfig, ProxyError,
    ProxyResult, UpstreamMiningValues, EXTRANONCE_RANGE_1_LENGTH, MIN_EXTRANONCE_SIZE,
    ROUTING_LOGIC,
};
use roles_logic_sv2::utils::{GroupId, Mutex};
use std::{net::SocketAddr, sync::Arc};
use tracing::info;

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
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let proxy_config = match args::process_cli_args() {
        Ok(p) => p,
        Err(_) => return,
    };

    let group_id = Arc::new(Mutex::new(GroupId::new()));
    ROUTING_LOGIC
        .set(Mutex::new(
            initialize_r_logic(&proxy_config.upstreams, group_id, proxy_config.clone()).await,
        ))
        .expect("BUG: Failed to set ROUTING_LOGIC");
    info!("PROXY INITIALIZING");
    initialize_upstreams(
        proxy_config.min_supported_version,
        proxy_config.max_supported_version,
    )
    .await;
    info!("PROXY INITIALIZED");

    // Wait for downstream connection
    let socket = SocketAddr::new(
        proxy_config.listen_address.parse().unwrap(),
        proxy_config.listen_mining_port,
    );

    info!("PROXY INITIALIZED");
    downstream_mining::listen_for_downstream_mining(socket).await
}
