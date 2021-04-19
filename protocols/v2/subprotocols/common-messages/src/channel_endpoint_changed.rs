use serde::{Deserialize, Serialize};
use serde_sv2::U32;

/// ## ChannelEndpointChanged (Server -> Client)
/// When a channel’s upstream or downstream endpoint changes and that channel had previously
/// sent messages with [channel_msg](TODO) bitset of unknown extension_type, the intermediate proxy
/// MUST send a [`ChannelEndpointChanged`] message. Upon receipt thereof, any extension state
/// (including version negotiation and the presence of support for a given extension) MUST be
/// reset and version/presence negotiation must begin again.
///
#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct ChannelEndpointChanged {
    /// The channel which has changed endpoint.
    channel_id: U32,
}
