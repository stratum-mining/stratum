/// GroupChannel is embedded in the DownstreamMiningNode but the UpstreamMiningNode has a reference
/// to all the paired DownstreamMiningNode so when recive meeages that change the channel state it
/// will change it from the DownstreamMiningNode references before relaying the message to
/// downstream
use super::downstream_mining::DownstreamMiningNode;
use crate::lib::upstream_mining::UpstreamMiningNodes;
use async_std::sync::{Arc, Mutex};
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

#[derive(Debug, Copy, Clone)]
struct GroupChannelIds {
    upstream_id: u32,
    downstream_id: u32,
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
        upstream_nodes: Arc<Mutex<UpstreamMiningNodes>>,
        downstream: Arc<Mutex<DownstreamMiningNode>>,
        downstream_id: u32,
    ) -> Result<Channel, ()> {
        let mut upstream_nodes = upstream_nodes.lock().await;

        // Find an upstream that support downstream and if there is one pair them
        let (upstream, used_version) = upstream_nodes
            .pair_downstream(
                protocol,
                min_v,
                max_v,
                flags,
                downstream.clone(),
                downstream_id,
            )
            .await?;

        // If upstream already have an opened group channel use it if not create new one
        let upstream_ = upstream.clone();
        let upstream_ = upstream_.lock().await;
        let group_channel = match upstream_.get_group_channel_id() {
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
            upstream,
        )
        .await;

        Ok(channel)
    }

    //pub async fn open(&mut self, upstream: Arc<Mutex<UpstreamMiningNodes>>) -> Result<u32,()> {
    //}
}
