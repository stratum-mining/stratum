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
mod lib;
use std::net::{IpAddr, SocketAddr};

use lib::upstream_mining::UpstreamMiningNode;
use serde::Deserialize;
use std::str::FromStr;

use roles_logic_sv2::{
    routing_logic::MiningProxyRoutingLogic,
    selectors::{GeneralMiningSelector, UpstreamMiningSelctor},
    utils::{Id, Mutex},
};
use std::{collections::HashMap, sync::Arc};

type RLogic = MiningProxyRoutingLogic<
    crate::lib::downstream_mining::DownstreamMiningNode,
    crate::lib::upstream_mining::UpstreamMiningNode,
    crate::lib::upstream_mining::ProxyRemoteSelector,
>;

static mut ROUTING_LOGIC: Option<Arc<Mutex<RLogic>>> = None;
static mut JOB_ID_TO_UPSTREAM_ID: Option<Arc<Mutex<HashMap<u32, u32>>>> = None;

pub fn max_supported_version() -> u16 {
    let config_file = std::fs::read_to_string("proxy-config.toml").unwrap();
    let config: Config = toml::from_str(&config_file).unwrap();
    config.max_supported_version
}
pub fn min_supported_version() -> u16 {
    let config_file = std::fs::read_to_string("proxy-config.toml").unwrap();
    let config: Config = toml::from_str(&config_file).unwrap();
    config.min_supported_version
}

pub async fn get_routing_logic() -> Arc<Mutex<RLogic>> {
    unsafe {
        loop {
            match ROUTING_LOGIC.clone() {
                Some(r_logic) => return r_logic,
                None => async_std::task::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
    }
}

pub fn get_routing_logic_sync() -> Arc<Mutex<RLogic>> {
    unsafe {
        let cloned = ROUTING_LOGIC.clone();
        cloned.unwrap()
    }
}

pub fn upstream_from_job_id(job_id: u32) -> Option<Arc<Mutex<UpstreamMiningNode>>> {
    let upstream_id: u32;
    unsafe {
        upstream_id = JOB_ID_TO_UPSTREAM_ID
            .clone()
            .unwrap()
            .safe_lock(|x| *x.get(&job_id).unwrap())
            .unwrap();
    }
    get_routing_logic_sync()
        .safe_lock(|r| r.upstream_selector.get_upstream(upstream_id))
        .unwrap()
}

pub fn add_job_id(job_id: u32, up_id: u32, prev_job_id: Option<u32>) {
    unsafe {
        if let Some(prev_job_id) = prev_job_id {
            JOB_ID_TO_UPSTREAM_ID
                .clone()
                .unwrap()
                .safe_lock(|x| x.remove(&prev_job_id))
                .unwrap();
        }
        JOB_ID_TO_UPSTREAM_ID
            .clone()
            .unwrap()
            .safe_lock(|x| x.insert(job_id, up_id))
            .unwrap();
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
    max_supported_version: u16,
    min_supported_version: u16,
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
    let job_ids = Arc::new(Mutex::new(Id::new()));
    let upstream_mining_nodes: Vec<Arc<Mutex<UpstreamMiningNode>>> = upstreams
        .iter()
        .enumerate()
        .map(|(index, upstream)| {
            let socket =
                SocketAddr::new(IpAddr::from_str(&upstream.address).unwrap(), upstream.port);
            Arc::new(Mutex::new(UpstreamMiningNode::new(
                index as u32,
                socket,
                upstream.pub_key,
                job_ids.clone(),
            )))
        })
        .collect();
    crate::lib::upstream_mining::scan(upstream_mining_nodes.clone()).await;
    let upstream_selector = GeneralMiningSelector::new(upstream_mining_nodes);
    let routing_logic = MiningProxyRoutingLogic {
        upstream_selector,
        downstream_id_generator: Id::new(),
        downstream_to_upstream_map: std::collections::HashMap::new(),
    };
    unsafe {
        ROUTING_LOGIC = Some(Arc::new(Mutex::new(routing_logic)));
        JOB_ID_TO_UPSTREAM_ID = Some(Arc::new(Mutex::new(HashMap::new())));
    }

    // Wait for downstream connection
    let socket = SocketAddr::new(
        IpAddr::from_str(&config.listen_address).unwrap(),
        config.listen_mining_port,
    );
    crate::lib::downstream_mining::listen_for_downstream_mining(socket).await
}
