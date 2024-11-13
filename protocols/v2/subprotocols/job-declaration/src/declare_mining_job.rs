#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq064K, Serialize, ShortTxId, Str0255, B0255, B064K, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// ## DeclareMiningJob (Client -> Server)
/// A request sent by the Job Declarator that proposes a selected set of transactions to the
/// upstream (pool) node.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJob<'decoder> {
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub mining_job_token: B0255<'decoder>,
    pub version: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_prefix: B064K<'decoder>,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_suffix: B064K<'decoder>,
    pub tx_short_hash_nonce: u64,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_short_hash_list: Seq064K<'decoder, ShortTxId<'decoder>>,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_hash_list_hash: U256<'decoder>,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub excess_data: B064K<'decoder>,
}

/// ## DeclareMiningJobSuccess (Server -> Client)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJobSuccess<'decoder> {
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub new_mining_job_token: B0255<'decoder>,
}

/// ## DeclareMiningJobError (Server -> Client)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJobError<'decoder> {
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub error_code: Str0255<'decoder>,
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
