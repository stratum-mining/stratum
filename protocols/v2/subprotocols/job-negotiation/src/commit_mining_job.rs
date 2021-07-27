#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq064K, Serialize, Str0255, B0255, B064K, U256};

/// # CommitMiningJob (Client -> Server)
///
/// A request sent by the Job Negotiator that proposes a selected set of transactions to the
/// upstream (pool) node.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommitMiningJob<'decoder> {
    /// Unique identifier for pairing the response.
    pub request_id: u32,
    /// Previously reserved mining job token received by
    /// AllocateMiningJobToken.Success.
    pub mining_job_token: u32,
    /// Version header field. To be later modified by
    /// BIP320-consistent changes.
    pub version: u32,
    /// The coinbase transaction nVersion field.
    pub coinbase_tx_version: u32,
    /// Up to 8 bytes (not including the length byte) which are to be
    /// placed at the beginning of the coinbase field in the coinbase
    /// transaction.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_prefix: B0255<'decoder>,
    /// The coinbase transaction input’s nSequence field.
    pub coinbase_tx_input_n_sequence: u32,
    /// The value, in satoshis, available for spending in coinbase
    /// outputs added by the client. Includes both transaction fees
    /// and block subsidy.
    pub coinbase_tx_value_remaining: u64,
    /// Bitcoin transaction outputs to be included as the last outputs
    /// in the coinbase transaction.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_tx_outputs: Seq064K<'decoder, B064K<'decoder>>,
    /// The locktime field in the coinbase transaction.
    pub coinbase_tx_locktime: u32,
    /// Extranonce size requested to be always available for the
    /// mining channel when this job is used on a mining connection.
    pub min_extranonce_size: u16,
    /// A unique nonce used to ensure tx_short_hash collisions are
    /// uncorrelated across the network.
    pub tx_short_hash_nonce: u64,
    /// TODO in the spec is defined as SEQ0_64K[B8] check which is the correct format.
    /// Sequence of SipHash-2-4(SHA256(transaction_data),
    /// tx_short_hash_nonce)) upstream node to check against its
    /// mempool. Does not include the coinbase transaction (as there
    /// is no corresponding full data for it yet)
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_short_hash_list: Seq064K<'decoder, u64>,
    /// Hash of the full sequence of SHA256(transaction_data)
    /// contained in the transaction_hash_list.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub tx_hash_list_hash: U256<'decoder>,
    /// Extra data which the Pool may require to validate the work (as
    /// defined in the Template Distribution Protocol).
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub excess_data: B064K<'decoder>,
}

/// # CommitMiningJob.Success (Server->Client)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommitMiningJobSuccess<'decoder> {
    /// Identifier of the original request.
    pub request_id: u32,
    /// Unique identifier provided by the pool of the job that the Job Negotiator
    /// has negotiated with the pool. It MAY be the same token as
    /// CommitMiningJob::mining_job_token if the pool allows to start mining
    /// on not yet negotiated job.
    /// If the token is different from the one in the corresponding
    /// CommitMiningJob message (irrespective of if the client is already mining
    /// using the original token), the client MUST send a SetCustomMiningJob
    /// message on each Mining Protocol client which wishes to mine using the
    /// negotiated job.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub new_mining_job_token: B0255<'decoder>,
}

/// # CommitMiningJob.Error (Server->Client)
/// Possible error codes:
/// * ‘invalid-mining-job-token’
/// * ‘invalid-job-param-value-{}’ - {} is replaced by a particular field name from CommitMiningJob message
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommitMiningJobError<'decoder> {
    /// Identifier of the original request.
    pub request_id: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub error_code: Str0255<'decoder>,
    /// Optional data providing further details to given error.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub error_details: B064K<'decoder>,
}
