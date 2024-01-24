#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str0255};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

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
    pub reason_code: Str0255<'decoder>,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for CloseChannel<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size() + self.reason_code.get_size()
    }
}

#[cfg(feature = "with_serde")]
impl<'a> CloseChannel<'a> {
    pub fn into_static(self) -> CloseChannel<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> CloseChannel<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
