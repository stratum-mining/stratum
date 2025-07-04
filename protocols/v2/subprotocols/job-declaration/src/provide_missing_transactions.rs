use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Seq064K, Serialize, B016M};
use core::convert::TryInto;

/// Message used by the JDS to ask for transactions that it did not recognize from
/// [`crate::DeclareMiningJob`] message.
///
/// In order to do block propagation, JDS must know all the transactions within the current block
/// template. These transactions are provided by the JDC to JDS as a sequence of transaction ids in
/// the [`crate::DeclareMiningJob`] message. If JDS is unable to recognize any of the transactions
/// through its mempool, it sends this message to ask for them. They are specified by their
/// position in the original [`crate::DeclareMiningJob`] message, 0-indexed not including the
/// coinbase transaction.
///
/// Used only under [`Full Template`] mode.
///
/// [`Full Template`]: https://github.com/stratum-mining/sv2-spec/blob/main/06-Job-Declaration-Protocol.md#632-full-template-mode
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ProvideMissingTransactions<'decoder> {
    /// Unique Identifier.
    ///
    /// Must be the same as the received [`crate::DeclareMiningJob::request_id`].
    pub request_id: u32,
    /// A list of unrecognized transactions that need to be supplied by the JDC in full. They are
    /// specified by their position in the original [`crate::DeclareMiningJob`] message, 0-indexed
    /// not including the coinbase transaction transaction.
    pub unknown_tx_position_list: Seq064K<'decoder, u16>,
}

impl fmt::Display for ProvideMissingTransactions<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ProvideMissingTransactions(request_id: {}, unknown_tx_position_list: {})",
            self.request_id, self.unknown_tx_position_list
        )
    }
}

/// Message used by JDC to accept [`ProvideMissingTransactions`] message and provide the full
/// list of transactions in the order they were requested by [`ProvideMissingTransactions`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ProvideMissingTransactionsSuccess<'decoder> {
    /// Unique Identifier.
    ///
    /// Must be the same as the received [`ProvideMissingTransactions::request_id`].
    pub request_id: u32,
    /// List of full transactions as requested by [`ProvideMissingTransactions`].
    pub transaction_list: Seq064K<'decoder, B016M<'decoder>>,
}
impl fmt::Display for ProvideMissingTransactionsSuccess<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ProvideMissingTransactionsSuccess(request_id: {}, transaction_list: {})",
            self.request_id, self.transaction_list
        )
    }
}
