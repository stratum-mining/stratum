use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize, Str0255};
use core::convert::TryInto;

/// Message used by a downstream to close a mining channel.
///
/// If you are sending this message through a proxy on behalf of multiple downstreams, you must send
/// it for each open channel separately.
///
/// Upon receiving this message, upstream **must** stop sending messages for the channel.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CloseChannel<'decoder> {
    /// Channel id of the channel to be closed.
    pub channel_id: u32,
    /// Reason for closing the channel.
    pub reason_code: Str0255<'decoder>,
}

impl fmt::Display for CloseChannel<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CloseChannel(channel_id: {}, reason_code: {})",
            self.channel_id,
            self.reason_code.as_utf8_or_hex()
        )
    }
}
