#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// # SetTarget (Server -> Client)
///
/// The server controls the submission rate by adjusting the difficulty target on a specified
/// channel. All submits leading to hashes higher than the specified target will be rejected by the
/// server.
/// Maximum target is valid until the next *SetTarget* message is sent and is applicable for all jobs
/// received on the channel in the future or already received with flag *future_job=True*. The
/// message is not applicable for alrea
///
/// When SetTarget is sent to a group channel, the maximum target is applicable to all channels in
/// the group.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetTarget<'decoder> {
    /// Channel identifier.
    pub channel_id: u32,
    /// Maximum value of produced hash that will be accepted by a server to
    /// accept shares.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub maximum_target: U256<'decoder>,
}
#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for SetTarget<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size() + self.maximum_target.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'a> SetTarget<'a> {
    pub fn into_static(self) -> SetTarget<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> SetTarget<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
