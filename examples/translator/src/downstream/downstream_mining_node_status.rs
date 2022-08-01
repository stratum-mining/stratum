use roles_logic_sv2::common_properties::{CommonDownstreamData, DownstreamChannel};
use std::collections::HashMap;

#[derive(Debug)]
pub enum DownstreamMiningNodeStatus {
    Initializing,
    Paired((CommonDownstreamData, HashMap<u32, Vec<DownstreamChannel>>)),
}

impl DownstreamMiningNodeStatus {
    pub(crate) fn is_paired(&self) -> bool {
        match self {
            DownstreamMiningNodeStatus::Initializing => false,
            DownstreamMiningNodeStatus::Paired(_) => true,
        }
    }

    pub(crate) fn pair(&mut self, data: CommonDownstreamData) {
        match self {
            DownstreamMiningNodeStatus::Initializing => {
                let self_ = Self::Paired((data, HashMap::new()));
                let _ = std::mem::replace(self, self_);
            }
            DownstreamMiningNodeStatus::Paired(_) => panic!(),
        }
    }

    pub fn get_channels(&mut self) -> &mut HashMap<u32, Vec<DownstreamChannel>> {
        match self {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired((_, channels)) => channels,
        }
    }

    pub(crate) fn add_channel(&mut self, channel: DownstreamChannel) {
        match self {
            DownstreamMiningNodeStatus::Initializing => panic!(),
            DownstreamMiningNodeStatus::Paired((_, channels)) => {
                match channels.get_mut(&channel.group_id()) {
                    Some(g) => g.push(channel),
                    None => {
                        channels.insert(channel.group_id(), vec![channel]);
                    }
                };
            }
        }
    }
}
