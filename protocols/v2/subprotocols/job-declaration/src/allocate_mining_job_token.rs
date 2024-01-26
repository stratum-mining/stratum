#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str0255, B0255, B064K};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// ## AllocateMiningJobToken (Client -> Server)
/// A request to get an identifier for a future-submitted mining job.
/// Rate limited to a rather slow rate and only available on connections where this has been
/// negotiated. Otherwise, only `mining_job_token(s)` from `CreateMiningJob.Success` are valid.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct AllocateMiningJobToken<'decoder> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub user_identifier: Str0255<'decoder>,
    pub request_id: u32,
}

/// ## AllocateMiningJobTokenSuccess (Server -> Clien)
/// The Server MUST NOT change the value of `coinbase_output_max_additional_size` in
/// `AllocateMiningJobToken.Success` messages unless required for changes to the pool’
/// configuration.
/// Notably, if the pool intends to change the space it requires for coinbase transaction outputs
/// regularly, it should simply prefer to use the maximum of all such output sizes as the
/// `coinbase_output_max_additional_size` value.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct AllocateMiningJobTokenSuccess<'decoder> {
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub mining_job_token: B0255<'decoder>,
    pub coinbase_output_max_additional_size: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_output: B064K<'decoder>,
    pub async_mining_allowed: bool,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for AllocateMiningJobToken<'d> {
    fn get_size(&self) -> usize {
        self.user_identifier.get_size() + self.request_id.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for AllocateMiningJobTokenSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size()
            + self.mining_job_token.get_size()
            + self.coinbase_output_max_additional_size.get_size()
            + self.coinbase_output.get_size()
            + self.async_mining_allowed.get_size()
    }
}
