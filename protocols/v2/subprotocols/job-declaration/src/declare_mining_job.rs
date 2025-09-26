use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Seq064K, Serialize, Str0255, B0255, B064K, U256};
use core::convert::TryInto;

/// Message used by JDC to proposes a selected set of transactions to JDS they wish to
/// mine on.
///
/// Used only under [`Full Template`] mode.
///
/// [`Full Template`]: https://github.com/stratum-mining/sv2-spec/blob/main/06-Job-Declaration-Protocol.md#632-full-template-mode
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJob<'decoder> {
    /// A unique identifier for this request.
    ///
    /// Used for pairing request/response.
    pub request_id: u32,
    /// Token received previously through [`crate::AllocateMiningJobTokenSuccess`] message.
    pub mining_job_token: B0255<'decoder>,
    /// Header version field.
    pub version: u32,
    /// Serialized bytes representing the initial part of the coinbase transaction (not including
    /// extranonce)
    pub coinbase_tx_prefix: B064K<'decoder>,
    /// Serialized bytes representing the final part of the coinbase transaction (after extranonce)
    pub coinbase_tx_suffix: B064K<'decoder>,
    /// List of the transaction ids contained in the template. JDS checks the list against its
    /// mempool and requests missing txs via [`crate::ProvideMissingTransactions`].
    ///
    /// This list Does not include the coinbase transaction (as there is no corresponding full data
    /// for it yet).
    pub tx_ids_list: Seq064K<'decoder, U256<'decoder>>,
    /// Extra data which the JDS may require to validate the work.
    pub excess_data: B064K<'decoder>,
}

impl fmt::Display for DeclareMiningJob<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DeclareMiningJob(request_id: {}, mining_job_token: {}, version: 0x{:08x}, coinbase_tx_prefix: {}, coinbase_tx_suffix: {}, tx_ids_list: {}, excess_data: {})",
            self.request_id,
            self.mining_job_token.as_hex(),
            self.version,
            self.coinbase_tx_prefix,
            self.coinbase_tx_suffix,
            self.tx_ids_list,
            self.excess_data
        )
    }
}

/// Messaged used by JDS to accept [`DeclareMiningJob`] message.
///
/// If [`Full Template`] mode is used, JDS MAY request txdata via `ProvideMissingTransactions`
/// before making this commitment.
///
/// [`Full Template`]: https://github.com/stratum-mining/sv2-spec/blob/main/06-Job-Declaration-Protocol.md#632-full-template-mode
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJobSuccess<'decoder> {
    /// A unique identifier for this request.
    ///
    /// Must be the same as the received [`DeclareMiningJob::request_id`].
    pub request_id: u32,
    /// This **may** be the same token as [DeclareMiningJob::mining_job_token] if the pool allows
    /// to start mining on a non declared job. If the token is different (irrespective of if the
    /// downstream is already mining using it), the downstream **must** send a `SetCustomMiningJob`
    /// message on each connection which wishes to mine using the declared job.
    pub new_mining_job_token: B0255<'decoder>,
}

impl fmt::Display for DeclareMiningJobSuccess<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DeclareMiningJobSuccess(request_id: {}, new_mining_job_token: {})",
            self.request_id,
            self.new_mining_job_token.as_hex()
        )
    }
}

/// Messaged used by JDS to reject [`DeclareMiningJob`] message.
///
/// Downstream should consider this as a trigger to fallback into some other Pool/JDS or solo
/// mining.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct DeclareMiningJobError<'decoder> {
    /// The unique identifier of the request.
    ///
    /// Must be the same as the received [`DeclareMiningJob::request_id`].
    pub request_id: u32,
    /// Possible values:
    ///
    /// - invalid-mining-job-token
    /// - invalid-job-param-value-{DeclareMiningJob::field}
    pub error_code: Str0255<'decoder>,
    /// Optional details about the error.
    pub error_details: B064K<'decoder>,
}

impl fmt::Display for DeclareMiningJobError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DeclareMiningJobError(request_id: {}, error_code: {}, error_details: {})",
            self.request_id,
            self.error_code.as_utf8_or_hex(),
            self.error_details
        )
    }
}
