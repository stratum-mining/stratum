#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str032};

/// # CloseChannel (Client -> Server, Server -> Client)
///
/// Client sends this message when it ends its operation. The server MUST stop sending messages
/// for the channel. A proxy MUST send this message on behalf of all opened channels from a
/// downstream connection in case of downstream connection closure.
///
/// If a proxy is operating in channel aggregating mode (translating downstream channels into
/// aggregated extended upstream channels), it MUST send an UpdateChannel message when it
/// receives CloseChannel or connection closure from a downstream connection. In general, proxy
/// servers MUST keep the upstream node notified about the real state of the downstream
/// channels.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CloseChannel<'decoder> {
    /// Channel identification.
    pub channel_id: u32,
    /// Reason for closing the channel.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub reason_code: Str032<'decoder>,
}
