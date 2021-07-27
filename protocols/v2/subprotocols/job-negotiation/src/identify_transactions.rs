#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq064K, Serialize, U256};

/// # IdentifyTransactions (Server->Client)
///
/// Sent by the Server in response to a CommitMiningJob message indicating it detected a
/// collision in the tx_short_hash_list, or was unable to reconstruct the tx_hash_list_hash.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentifyTransactions {
    /// Unique identifier for pairing the response to the CommitMiningJob message.
    pub request_id: u32,
}

/// # IdentifyTransactions.Success (Client->Server)
///
/// Sent by the Client in response to an IdentifyTransactions message to provide the full set of
/// transaction data hashes.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentifyTransactionsSuccess<'decoder> {
    /// Unique identifier for pairing the response to the
    /// CommitMiningJob/IdentifyTransactions message.
    pub request_id: u32,
    /// The full list of transaction data hashes used to build the mining job in
    /// the corresponding CommitMiningJob message.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_hash_list: Seq064K<'decoder, U256<'decoder>>,
}
