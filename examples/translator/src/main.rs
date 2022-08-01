use async_channel::{Receiver, Sender};
use codec_sv2::StandardEitherFrame;
use network_helpers::plain_connection_tokio::PlainConnection;
use once_cell::sync::Lazy;
use roles_logic_sv2::{
    parsers::MiningDeviceMessages,
    routing_logic::{CommonRoutingLogic, MiningProxyRoutingLogic, MiningRoutingLogic},
    selectors::{GeneralMiningSelector, UpstreamMiningSelctor},
    utils::{Id, Mutex},
};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};
use tokio::net::{TcpListener, TcpStream};
pub mod config;
pub mod downstream;
pub mod setup_connection_handler;
pub mod upstream;
use crate::config::Config;
use crate::upstream::UpstreamMiningNode;

pub type Message = MiningDeviceMessages<'static>;
pub type EitherFrame = StandardEitherFrame<Message>;
type RLogic = MiningProxyRoutingLogic<
    crate::downstream::DownstreamMiningNode,
    crate::upstream::UpstreamMiningNode,
    crate::upstream::ProxyRemoteSelector,
>;

/// Panic whene we are looking one of this 2 global mutex would force the proxy to go down as every
/// part of the program depend on them.
/// SAFTEY note: we use global mutable memory instead of a dedicated struct that use a dedicated
/// task to change the mutable state and communicate with the other parts of the program via
/// messages cause it is impossible for a task to panic while is using one of the two below Mutex.
/// So it make sense to use shared mutable memory to lower the complexity of the codebase and to
/// have some performance gain.
static ROUTING_LOGIC: Lazy<Mutex<RLogic>> = Lazy::new(|| Mutex::new(initialize_r_logic()));
static JOB_ID_TO_UPSTREAM_ID: Lazy<Mutex<HashMap<u32, u32>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Downstream client (typically the Mining Device) connection address + port
const DOWNSTREAM_ADDR: &str = "127.0.0.1:34255";

async fn initialize_upstreams() {
    let upstreams = ROUTING_LOGIC
        .safe_lock(|r_logic| r_logic.upstream_selector.upstreams.clone())
        .unwrap();
    crate::upstream::scan(upstreams).await;
}

pub fn initialize_r_logic() -> RLogic {
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
    //crate::lib::upstream_mining::scan(upstream_mining_nodes.clone()).await;
    let upstream_selector = GeneralMiningSelector::new(upstream_mining_nodes);
    MiningProxyRoutingLogic {
        upstream_selector,
        downstream_id_generator: Id::new(),
        downstream_to_upstream_map: std::collections::HashMap::new(),
    }
}

pub fn get_routing_logic() -> MiningRoutingLogic<
    crate::downstream::DownstreamMiningNode,
    crate::upstream::UpstreamMiningNode,
    crate::upstream::ProxyRemoteSelector,
    RLogic,
> {
    MiningRoutingLogic::Proxy(&ROUTING_LOGIC)
}

pub fn get_common_routing_logic() -> CommonRoutingLogic<RLogic> {
    CommonRoutingLogic::Proxy(&ROUTING_LOGIC)
}

pub fn upstream_from_job_id(job_id: u32) -> Option<Arc<Mutex<UpstreamMiningNode>>> {
    let upstream_id: u32;
    upstream_id = JOB_ID_TO_UPSTREAM_ID
        .safe_lock(|x| *x.get(&job_id).unwrap())
        .unwrap();
    ROUTING_LOGIC
        .safe_lock(|rlogic| rlogic.upstream_selector.get_upstream(upstream_id))
        .unwrap()
}

pub(crate) fn add_job_id(job_id: u32, up_id: u32, prev_job_id: Option<u32>) {
    if let Some(prev_job_id) = prev_job_id {
        JOB_ID_TO_UPSTREAM_ID
            .safe_lock(|x| x.remove(&prev_job_id))
            .unwrap();
    }
    JOB_ID_TO_UPSTREAM_ID
        .safe_lock(|x| x.insert(job_id, up_id))
        .unwrap();
}

/// Sv1 Upstream (Miner) <-> Sv1/Sv2 Proxy <-> Sv2 Upstream (Pool)
/// 1. Define the socket where the server will listen for the incoming connection
/// 2. Server binds to a socket and starts listening
/// 3. A Downstream client connects
/// 4. Server opens the connection and initializes it via a `PlainConnection` that returns a
/// `Receiver<EitherFrame>` and a `Sender<EitherFrame>`. Messages are sent to the downstream client
/// (most typically the Mining Device) via the `Sender`. Messages sent by the downstream client are
/// received by the proxy via the `Receiver`, then parsed.
#[tokio::main]
async fn main() {
    println!("Hello, sv1 to sv2 translator!");

    // 1. Define the socket where the server will listen for the incoming connection
    let config_file = std::fs::read_to_string("proxy-config.toml").unwrap();
    let config: Config = toml::from_str(&config_file).unwrap();
    let socket = SocketAddr::new(
        IpAddr::from_str(&config.listen_address).unwrap(),
        config.listen_mining_port,
    );
    // 2. Server binds to a socket and starts listening
    let listner = TcpListener::bind(DOWNSTREAM_ADDR).await.unwrap();
    println!("PROXY INITIALIZED");

    // Spawn downstream tasks
    tokio::task::spawn(async {
        // 3. A Downstream client connects
        let stream = TcpStream::connect(DOWNSTREAM_ADDR).await.unwrap();
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;
        let received = receiver.recv().await;
    });

    // 4. Server opens the connection and initializes it via a `PlainConnection` that returns a
    // `Receiver<EitherFrame>` and a `Sender<EitherFrame>`. Messages are sent to the downstream client
    // (most typically the Mining Device) via the `Sender`. Messages sent by the downstream client are
    // received by the proxy via the `Receiver`, then parsed.
    while let Ok((stream, _)) = listner.accept().await {
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;
        let received = receiver.recv().await;
    }
}
