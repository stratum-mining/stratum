#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str0255};
#[cfg(not(feature = "with_serde"))]
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
        panic!("This function shouldn't be called by the Message Generator");
    }
    pub fn as_static(&self) -> CloseChannel<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
}
