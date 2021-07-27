#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq064K, Serialize, B016M};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProvideMissingTransactions<'decoder> {
    /// Identifier of the original CreateMiningJob request.
    pub request_id: u32,
    /// A list of unrecognized transactions that need to be supplied by
    /// the Job Negotiator in full. They are specified by their position inthe original CommitMiningJob message, 0-indexed not including
    /// the coinbase transaction.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub unknown_tx_position_list: Seq064K<'decoder, u16>,
}

/// # ProvideMissingTransactions.Success (Client->Server)
///
/// This is a message to push transactions that the server didn’t recognize and requested them to
/// be supplied in ProvideMissingTransactions.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProvideMissingTransactionsSuccess<'decoder> {
    /// Identifier of the original CreateMiningJob request.
    pub request_id: u32,
    /// List of full transactions as requested by
    /// ProvideMissingTransactions, in the order they were requested
    /// in ProvideMissingTransactions.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub transaction_list: Seq064K<'decoder, B016M<'decoder>>,
}
