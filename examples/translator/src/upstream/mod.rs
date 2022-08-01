use crate::downstream::downstream_mining_node::DownstreamMiningNode;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::{
    parsers::PoolMessages, selectors::ProxyDownstreamMiningSelector as Prs, utils::Mutex,
};
use std::sync::Arc;
use tokio::task;

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
pub type ProxyRemoteSelector = Prs<DownstreamMiningNode>;

pub mod upstream_mining_connection;
pub mod upstream_mining_node;
pub use upstream_mining_connection::UpstreamMiningConnection;
pub use upstream_mining_node::{JobDispatcher, UpstreamMiningNode};

pub async fn scan(nodes: Vec<Arc<Mutex<UpstreamMiningNode>>>) {
    let spawn_tasks: Vec<task::JoinHandle<()>> = nodes
        .iter()
        .map(|node| {
            let node = node.clone();
            task::spawn(async move {
                UpstreamMiningNode::setup_flag_and_version(node, None)
                    .await
                    .unwrap();
            })
        })
        .collect();
    for task in spawn_tasks {
        task.await.unwrap();
    }
}
