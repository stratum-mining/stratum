#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Seq064K, Serialize, B016M};

/// TODO: comment
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ProvideMissingTransactions<'decoder> {
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub request_id: u32,
    pub unknown_tx_position_list: Seq064K<'decoder, u16>,
}

/// TODO: comment
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ProvideMissingTransactionsSuccess<'decoder> {
    pub request_id: u32,
    pub transaction_list: Seq064K<'decoder, B016M<'decoder>>,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for ProvideMissingTransactions<'d> {
    fn get_size(&self) -> usize {
        self.user_identifier.get_size() + self.unknown_tx_position_list.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for ProvideMissingTransactionsSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() + self.transaction_list.get_size()
    }
}
