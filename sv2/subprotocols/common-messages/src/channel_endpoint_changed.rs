use alloc::{fmt, vec::Vec};
use binary_sv2::{Deserialize, Serialize};
use core::convert::TryInto;

/// Message used by an upstream role for announcing a mining channel endpoint change.
///
/// This message should be sent when a mining channel’s upstream or downstream endpoint changes and
/// that channel had previously exchanged message(s) with `channel_msg` bitset of unknown
/// `extension_type`.
///
/// When a downstream receives such a message, any extension state (including version and extension
/// support) must be reset and renegotiated.

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct ChannelEndpointChanged {
    /// Unique identifier of the channel that has changed its endpoint.
    pub channel_id: u32,
}

impl ChannelEndpointChanged {
    pub fn into_owned(self) -> ChannelEndpointChangedOwned {
        ChannelEndpointChangedOwned {
            channel_id: self.channel_id,
        }
    }

    pub fn as_owned(&self) -> ChannelEndpointChangedOwned {
        (*self).into_owned()
    }
}

/// Owned alias for [`ChannelEndpointChanged`] because the message has no borrowed fields.
pub type ChannelEndpointChangedOwned = ChannelEndpointChanged;

impl fmt::Display for ChannelEndpointChanged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChannelEndpointChanged(channel_id: {})", self.channel_id)
    }
}
