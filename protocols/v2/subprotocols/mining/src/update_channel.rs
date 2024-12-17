#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str0255, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// Message used by downstream to notify an upstream about changes on a specified channel.
///
/// If a downstream performs device/connection aggregation (i.e. it is a proxy), it must send this
/// message when downstream channels change.
///
/// Only relevant for Extended Channels.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateChannel<'decoder> {
    /// Channel identification.
    pub channel_id: u32,
    /// Expected hash rate of the device (or cumulative hashrate on the channel if multiple devices
    /// are connected downstream) in h/s.
    ///
    /// Depending on upstream’s target setting policy, this value can be used for setting a
    /// reasonable target for the channel.
    ///
    /// Proxy must send 0.0f when there are no mining devices connected yet.
    pub nominal_hash_rate: f32,
    /// As there can be some delay between [`UpdateChannel`] and corresponding [`SetTarget`]
    /// messages, based on new job readiness on the server, this field is understood as
    /// downstream’s request.
    ///
    /// When maximum target is smaller than currently used maximum target for the channel,
    /// upstream node must reflect the downstreams’s request (and send appropriate [`SetTarget`]
    /// message).
    ///
    /// Upstream can change maximum target by sending [`SetTarget`] message.
    ///
    /// [`SetTarget`]: crate::SetTarget
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub maximum_target: U256<'decoder>,
}

/// Message used by upstream to notify downstream about an error in the [`UpdateChannel`] message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateChannelError<'decoder> {
    /// Channel identification.
    pub channel_id: u32,
    /// Reason for channel update error.
    ///
    /// Possible error codes:
    /// - max-target-out-of-range
    /// - invalid-channel-id
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub error_code: Str0255<'decoder>,
}
#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for UpdateChannel<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size() + 4 + self.maximum_target.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for UpdateChannelError<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size() + self.error_code.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'a> UpdateChannel<'a> {
    pub fn into_static(self) -> UpdateChannel<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
    pub fn as_static(&self) -> UpdateChannel<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
}
#[cfg(feature = "with_serde")]
impl<'a> UpdateChannelError<'a> {
    pub fn into_static(self) -> UpdateChannelError<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
    pub fn as_static(&self) -> UpdateChannelError<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
}
