use alloc::vec::Vec;
use binary_sv2::{binary_codec_sv2, Deserialize, Seq064K, Serialize, U256};
use core::convert::TryInto;

/// Message used by JDS as a response to a [`crate::DeclareMiningJob`] message indicating it
/// detected a collision in the [`crate::DeclareMiningJob::tx_short_hash_list`], or was unable to
/// reconstruct the [`crate::DeclareMiningJob::tx_hash_list_hash`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IdentifyTransactions {
    /// Mining job unique identifier.
    ///
    /// This **must** be the same as the received [`crate::DeclareMiningJob::request_id`].
    pub request_id: u32,
}

/// Messaged used by JDC to accept [`IdentifyTransactions`] message and provide the full set
/// of transaction data hashes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IdentifyTransactionsSuccess<'decoder> {
    /// Unique identifier.
    ///
    /// This **must** be the same as the received [`IdentifyTransactions::request_id`].
    pub request_id: u32,
    /// The full list of transaction data hashes used to build the mining job in the corresponding
    /// DeclareMiningJob message
    pub tx_data_hashes: Seq064K<'decoder, U256<'decoder>>,
}
