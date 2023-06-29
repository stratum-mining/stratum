#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str0255, B0255, B064K};
use core::convert::TryInto;

/// TODO: comment
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IdentifyTransactions<'decoder> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub user_identifier: Str0255<'decoder>,
    pub request_id: u32,
}

/// TODO: comment
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IdentifyTransactionsSuccess<'decoder> {
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
impl<'d> GetSize for IdentifyTransactions<'d> {
    fn get_size(&self) -> usize {
        self.user_identifier.get_size() + self.request_id.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for IdentifyTransactionsSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size()
            + self.mining_job_token.get_size()
            + self.coinbase_output_max_additional_size.get_size()
            + self.coinbase_output.get_size()
            + self.async_mining_allowed.get_size()
    }
}
