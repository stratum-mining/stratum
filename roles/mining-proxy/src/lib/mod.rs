pub mod downstream_mining;
pub mod error;
pub mod upstream_mining;

use once_cell::sync::OnceCell;
use roles_logic_sv2::{
    routing_logic::{CommonRoutingLogic, MiningProxyRoutingLogic, MiningRoutingLogic},
    selectors::GeneralMiningSelector,
    utils::{GroupId, Id, Mutex},
};
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::oneshot};
use tracing::info;
use upstream_mining::UpstreamMiningNode;

type RLogic = MiningProxyRoutingLogic<
    downstream_mining::DownstreamMiningNode,
    upstream_mining::UpstreamMiningNode,
    upstream_mining::ProxyRemoteSelector,
>;

/// Panic whene we are looking one of this 2 global mutex would force the proxy to go down as every
/// part of the program depend on them.
/// SAFTEY note: we use global mutable memory instead of a dedicated struct that use a dedicated
/// task to change the mutable state and communicate with the other parts of the program via
/// messages cause it is impossible for a task to panic while is using one of the two below Mutex.
/// So it make sense to use shared mutable memory to lower the complexity of the codebase and to
/// have some performance gain.
pub static ROUTING_LOGIC: OnceCell<Mutex<RLogic>> = OnceCell::new();
static MIN_EXTRANONCE_SIZE: u16 = 6;
static EXTRANONCE_RANGE_1_LENGTH: usize = 4;

pub async fn initialize_upstreams(min_version: u16, max_version: u16) {
    let upstreams = ROUTING_LOGIC
        .get()
        .expect("BUG: ROUTING_LOGIC has not been set yet")
        .safe_lock(|r_logic| r_logic.upstream_selector.upstreams.clone())
        .unwrap();
    let available_upstreams = upstream_mining::scan(upstreams, min_version, max_version).await;
    ROUTING_LOGIC
        .get()
        .unwrap()
        .safe_lock(|rl| rl.upstream_selector.update_upstreams(available_upstreams))
        .unwrap();
}

fn remove_upstream(id: u32) {
    let upstreams = ROUTING_LOGIC
        .get()
        .expect("BUG: ROUTING_LOGIC has not been set yet")
        .safe_lock(|r_logic| r_logic.upstream_selector.upstreams.clone())
        .unwrap();
    let mut updated_upstreams = vec![];
    for upstream in upstreams {
        if upstream.safe_lock(|s| s.get_id()).unwrap() != id {
            updated_upstreams.push(upstream)
        }
    }
    ROUTING_LOGIC
        .get()
        .unwrap()
        .safe_lock(|rl| rl.upstream_selector.update_upstreams(updated_upstreams))
        .unwrap();
}

pub fn get_routing_logic() -> MiningRoutingLogic<
    downstream_mining::DownstreamMiningNode,
    upstream_mining::UpstreamMiningNode,
    upstream_mining::ProxyRemoteSelector,
    RLogic,
> {
    MiningRoutingLogic::Proxy(
        ROUTING_LOGIC
            .get()
            .expect("BUG: ROUTING_LOGIC was not set yet"),
    )
}
pub fn get_common_routing_logic() -> CommonRoutingLogic<RLogic> {
    CommonRoutingLogic::Proxy(
        ROUTING_LOGIC
            .get()
            .expect("BUG: ROUTING_LOGIC was not set yet"),
    )
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpstreamMiningValues {
    pub address: String,
    pub port: u16,
    pub pub_key: key_utils::Secp256k1PublicKey,
    pub channel_kind: ChannelKind,
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub enum ChannelKind {
    Group,
    Extended,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Configuration {
    pub upstreams: Vec<UpstreamMiningValues>,
    pub listen_address: String,
    pub listen_mining_port: u16,
    pub max_supported_version: u16,
    pub min_supported_version: u16,
    pub downstream_share_per_minute: f32,
    pub expected_total_downstream_hr: f32,
    pub reconnect: bool,
}
pub async fn initialize_r_logic(
    upstreams: &[UpstreamMiningValues],
    group_id: Arc<Mutex<GroupId>>,
    config: Configuration,
) -> RLogic {
    let channel_ids = Arc::new(Mutex::new(Id::new()));
    let mut upstream_mining_nodes = Vec::with_capacity(upstreams.len());
    for (index, upstream_) in upstreams.iter().enumerate() {
        let socket = SocketAddr::new(upstream_.address.parse().unwrap(), upstream_.port);

        let upstream = Arc::new(Mutex::new(UpstreamMiningNode::new(
            index as u32,
            socket,
            upstream_.pub_key.into_bytes(),
            upstream_.channel_kind,
            group_id.clone(),
            channel_ids.clone(),
            config.downstream_share_per_minute,
            None,
            None,
            config.expected_total_downstream_hr,
            config.reconnect,
        )));

        match upstream_.channel_kind {
            ChannelKind::Group => (),
            ChannelKind::Extended => (),
        }

        upstream_mining_nodes.push(upstream);
    }
    let upstream_selector = GeneralMiningSelector::new(upstream_mining_nodes);
    MiningProxyRoutingLogic {
        upstream_selector,
        downstream_id_generator: Id::new(),
        downstream_to_upstream_map: std::collections::HashMap::new(),
    }
}

pub async fn start_mining_proxy(config: Configuration) {
    let group_id = Arc::new(Mutex::new(GroupId::new()));
    ROUTING_LOGIC
        .set(Mutex::new(
            initialize_r_logic(&config.upstreams, group_id, config.clone()).await,
        ))
        .expect("BUG: Failed to set ROUTING_LOGIC");

    info!("Initializing upstream scanner");
    initialize_upstreams(config.min_supported_version, config.max_supported_version).await;
    info!("Initializing downstream listener");

    let socket = SocketAddr::new(
        config.listen_address.parse().unwrap(),
        config.listen_mining_port,
    );
    let listener = TcpListener::bind(socket).await.unwrap();

    info!("Listening for downstream mining connections on {}", socket);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let (_, res) = tokio::join!(
        // Wait for downstream connection
        downstream_mining::listen_for_downstream_mining(listener, shutdown_rx),
        // handle SIGTERM/QUIT / ctrl+c
        tokio::spawn(async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen to signals");
            let _ = shutdown_tx.send(());
            info!("Interrupt received");
        })
    );

    if let Err(e) = res {
        panic!("Failed to wait for clean exit: {:?}", e);
    }

    info!("Shutdown done");
}
