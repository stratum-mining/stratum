#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq0255, Seq064K, Serialize, ShortTxId, B0255, B064K, U256, Str0255};
use core::convert::TryInto;

/// ## CommitMiningJob (Client -> Server)
/// A request sent by the Job Declarator that proposes a selected set of transactions to the upstream (pool) node.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct CommitMiningJob<'decoder> {
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub mining_job_token: B0255<'decoder>,
    pub version: u32,
    pub coinbase_tx_version: u32,
    pub coinbase_prefix: B0255<'decoder>,
    pub coinbase_tx_input_n_sequence: u32,
    pub coinbase_tx_value_remaining: u64,
    pub coinbase_tx_outputs: B064K<'decoder>,
    pub coinbase_tx_locktime: u32,
    pub min_extranonce_size: u16,
    pub tx_short_hash_nonce: u64,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_short_hash_list: Seq064K<'decoder, ShortTxId<'decoder>>,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_hash_list_hash: U256<'decoder>,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub excess_data: B064K<'decoder>,
    /// Merkle path hashes ordered from deepest.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub merkle_path: Seq0255<'decoder, U256<'decoder>>,
}

/// ## CommitMiningJobSuccess (Server -> Client)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct CommitMiningJobSuccess<'decoder> {
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub new_mining_job_token: B0255<'decoder>,
}

/// ## CommitMiningJobError (Server -> Client)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct CommitMiningJobError<'decoder> {
    pub request_id: u32,
    pub error_code: Str0255<'decoder>,
    pub error_details: B064K<'decoder>,
}

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for CommitMiningJob<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size()
            + self.mining_job_token.get_size()
            + self.version.get_size()
            + self.coninbase_tx_version.get_size()
            + self.coninbase_prefix.get_size()
            + self.coninbase_tx_input_nsequence.get_size()
            + self.coninbase_tx_value_remaining.get_size()
            + self.coinbase_tx_outputs.get_size()
            + self.coinbase_tx_locktime.get_size()
            + self.min_extranonce_size.get_size()
            + self.tx_short_hash_nonce.get_size()
            + self.tx_short_hash_list.get_size()
            + self.tx_hash_list_hash.get_size()
            + self.excess_data.get_size()
            + self.merkle_path.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for CommitMiningJobSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() + self.new_mining_job_token.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for CommitMiningJobError<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() 
            + self.error_code.get_size() 
            + self.error_details.get_size()
    }
}
