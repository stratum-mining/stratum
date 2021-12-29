/// GroupChannel is embedded in the DownstreamMiningNode but the UpstreamMiningNode has a reference
/// to all the paired DownstreamMiningNode so when recive meeages that change the channel state it
/// will change it from the DownstreamMiningNode references before relaying the message to
/// downstream
use super::downstream_mining::DownstreamMiningNode;
use crate::lib::upstream_mining::UpstreamMiningNodes;
use crate::Mutex;
use async_std::sync::Arc;
use messages_sv2::handlers::{common::Protocol, common::SetupConnectionSuccess};

#[derive(Debug, Copy, Clone)]
pub enum ChannelType {
    #[allow(dead_code)]
    Extended(ExtendedChannel),
    Group(GroupChannel),
    #[allow(dead_code)]
    Standard,
}

#[derive(Debug, Copy, Clone)]
pub struct Channel {
    pub upstream: ChannelType,
    pub downstream: ChannelType,
}

#[derive(Debug, Copy, Clone)]
pub struct ExtendedChannel {}

#[derive(Debug, Clone)]
struct GroupChannelIds {
    upstream_id: u32,
    // Header only Downstream can have only one downstream id
    // but non header only Downstream can have more than one downstream id
    downstream_id: Vec<u32>,
}

#[derive(Debug, Copy, Clone)]
pub enum GroupChannel {
    //UpAndDownOpened(GroupChannelIds),
    OnlyUpOpened(u32),
    Close,
}

//impl GroupChannel {
//    pub fn new() -> Self {
//        GroupChannel::Close
//    }
//}

impl GroupChannel {
    /// Nasty glue code that:
    /// It try to pair a downstream new connected downstream with an upstream.
    /// It check downstream version, flags and upstream version, flags.
    /// If upstream already have an open group channel with proxy it will copy it.
    /// If not it will create a new not initialized group channel and it will open as soon as
    /// downstream require to open a channel.
    pub async fn new_group_channel(
        protocol: Protocol,
        min_v: u16,
        max_v: u16,
        flags: u32,
        upstream_nodes_mutex: Arc<Mutex<UpstreamMiningNodes>>,
        downstream: Arc<Mutex<DownstreamMiningNode>>,
        downstream_id: u32,
    ) -> Result<Channel, ()> {
        // Find an upstream that support downstream and if there is one pair them
        let mut upstream_nodes = upstream_nodes_mutex.lock().await;
        let (upstream_mutex, used_version) = upstream_nodes
            .pair_downstream(
                protocol,
                min_v,
                max_v,
                flags,
                downstream.clone(),
                downstream_id,
            )
            .await?;
        drop(upstream_nodes);

        // If upstream already have an opened group channel use it if not create new one
        let group_channel_id = upstream_mutex
            .safe_lock(|upstream| upstream.get_group_channel_id())
            .await;
        let group_channel = match group_channel_id {
            Some(id) => GroupChannel::OnlyUpOpened(id),
            None => GroupChannel::Close,
        };
        let channel = Channel {
            upstream: ChannelType::Group(group_channel),
            downstream: ChannelType::Group(group_channel),
        };

        // Send SetupConnectionSuccess to donwstream, start processing new messages coming from
        // downstream and pair the above channel with the downstream
        let setup_connection_success = SetupConnectionSuccess {
            used_version,
            flags,
        };
        DownstreamMiningNode::initialize(
            downstream.clone(),
            setup_connection_success,
            channel,
            upstream_mutex,
        )
        .await;

        Ok(channel)
    }
}
