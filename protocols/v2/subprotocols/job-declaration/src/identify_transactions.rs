#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq064K, Serialize, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// TODO: comment
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IdentifyTransactions {
    pub request_id: u32,
}

/// TODO: comment
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IdentifyTransactionsSuccess<'decoder> {
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_data_hashes: Seq064K<'decoder, U256<'decoder>>,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl GetSize for IdentifyTransactions {
    fn get_size(&self) -> usize {
        self.request_id.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for IdentifyTransactionsSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() + self.tx_data_hashes.get_size()
    }
}
