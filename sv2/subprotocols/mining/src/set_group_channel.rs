use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Seq064K, Serialize};
use core::convert::TryInto;

/// Message used by upstream to associate a set of Standard Channel(s) to a Group Channel.
///
/// A channel becomes a group channel when it is used by this message as
/// [`SetGroupChannel::group_channel_id`].
///
/// Every standard channel is a member of a group of standard channels, addressed by the upstream
/// server’s provided identifier. The group channel is used mainly for efficient job distribution
/// to multiple standard channels at once.
///
/// The upstream must ensure that a group channel has a unique channel ID within one connection.
///
/// Channel reinterpretation is not allowed.
///
/// This message can be sent only to connections that didnt set `REQUIRES_STANDARD_JOBS` flag in
/// `SetupConnection` message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetGroupChannel<'decoder> {
    /// Identifier of the group where the standard channel belongs.
    pub group_channel_id: u32,
    /// A sequence of opened standard channel IDs, for which the group channel is being redefined.
    pub channel_ids: Seq064K<'decoder, u32>,
}

impl fmt::Display for SetGroupChannel<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SetGroupChannel(group_channel_id={}, channel_ids={})",
            self.group_channel_id, self.channel_ids
        )
    }
}
