#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq064K, Serialize, ShortTxId, Str0255, B0255, B064K, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

#[cfg(doc)]
use crate::AllocateMiningJobTokenSuccess;

/// A request sent by the Job Declarator that proposes a selected set of transactions to the upstream (pool) node.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJob<'decoder> {
    /// Unique identifier for pairing the response
    pub request_id: u32,
    /// Previously reserved mining job token received by [`AllocateMiningJobTokenSuccess`]
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub mining_job_token: B0255<'decoder>,
    /// Version header field. To be later modified by BIP320-consistent changes.
    pub version: u32,
    /// The coinbase transaction nVersion field
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_prefix: B064K<'decoder>,
    /// Up to 8 bytes (not including the length byte) which are to be placed at the beginning of the coinbase
    /// field in the coinbase transaction
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_suffix: B064K<'decoder>,
    /// A unique nonce used to ensure tx_short_hash collisions are uncorrelated across the network
    pub tx_short_hash_nonce: u64,
    /// Sequence of SHORT_TX_IDs. Inputs to the SipHash functions are transaction hashes from the mempool.
    /// Secret keys k0, k1 are derived from the first two little-endian 64-bit integers from the
    /// SHA256(tx_short_hash_nonce), respectively (see bip-0152 for more information). Upstream node
    /// checks the list against its mempool. Does not include the coinbase transaction (as there is
    /// no corresponding full data for it yet).
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_short_hash_list: Seq064K<'decoder, ShortTxId<'decoder>>,
    /// Hash of the full sequence of SHA256(transaction_data) contained in the transaction_hash_list
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_hash_list_hash: U256<'decoder>,
    /// Extra data which the Pool may require to validate the work (as defined in the Template Distribution Protocol)
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub excess_data: B064K<'decoder>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJobSuccess<'decoder> {
    /// Identifier of the original request
    pub request_id: u32,
    /// Unique identifier provided by the pool of the job that the Job Declarator has declared with the pool.
    /// It MAY be the same token as [`DeclareMiningJob`]::mining_job_token if the pool allows to start mining on
    /// not yet declared job. If the token is different from the one in the corresponding [`DeclareMiningJob`]
    /// message (irrespective of if the client is already mining using the original token), the client MUST
    /// send a [`SetCustomMiningJob`] message on each Mining Protocol client which wishes to mine using the declared job.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub new_mining_job_token: B0255<'decoder>,
}

/// Possible error codes values:
/// - `invalid-mining-job-token`
/// - `invalid-job-param-value-{}` - {} is replaced by a particular field name from DeclareMiningJob message
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJobError<'decoder> {
    /// Identifier of the original request
    pub request_id: u32,
    /// Human-readable error code(s).
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub error_code: Str0255<'decoder>,
    /// Optional data providing further details to given error
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub error_details: B064K<'decoder>,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for DeclareMiningJob<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size()
            + self.mining_job_token.get_size()
            + self.version.get_size()
            + self.coinbase_prefix.get_size()
            + self.coinbase_suffix.get_size()
            + self.tx_short_hash_nonce.get_size()
            + self.tx_short_hash_list.get_size()
            + self.tx_hash_list_hash.get_size()
            + self.excess_data.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for DeclareMiningJobSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() + self.new_mining_job_token.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for DeclareMiningJobError<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() + self.error_code.get_size() + self.error_details.get_size()
    }
}
