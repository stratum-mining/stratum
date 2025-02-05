use alloc::vec::Vec;
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize};
use core::convert::TryInto;

/// Message used by an upstream role for announcing a mining channel endpoint change.
///
/// This message should be sent when a mining channel’s upstream or downstream endpoint changes and
/// that channel had previously exchanged message(s) with `channel_msg` bitset of unknown
/// `extension_type`.
///
/// When a downstream receives such a message, any extension state (including version and extension
/// support) must be reset and renegotiated.
#[repr(C)]
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct ChannelEndpointChanged {
    /// Unique identifier of the channel that has changed its endpoint.
    pub channel_id: u32,
}
