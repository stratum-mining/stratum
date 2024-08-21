#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq064K, Serialize, B016M};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

// In order to do block propagation, the JDserver must know all the transactions within the current
// block template. These transactions are provided by the JDclient to the JDserver as a sequence
// of short hashes (in the message DeclareMiningJob). The JDserver has a mempool, which uses to identify the transactions from this
// list. If there is some transaction that it is not in the JDserver memppol, the JDserver sends
// this message to ask for them. They are specified by their position in the original DeclareMiningJob message, 0-indexed not including the coinbase transaction transaction.

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ProvideMissingTransactions<'decoder> {
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub unknown_tx_position_list: Seq064K<'decoder, u16>,
}

// List of full transactions as requested by ProvideMissingTransactions, in the order they were requested in ProvideMissingTransactions

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ProvideMissingTransactionsSuccess<'decoder> {
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub transaction_list: Seq064K<'decoder, B016M<'decoder>>,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for ProvideMissingTransactions<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() + self.unknown_tx_position_list.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for ProvideMissingTransactionsSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() + self.transaction_list.get_size()
    }
}
