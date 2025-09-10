use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Seq0255, Serialize, Str0255, B0255, B064K, U256};
use core::convert::TryInto;

/// Message used by downstream role to set a custom job to an upstream (Pool).
///
/// The [`SetCustomMiningJob::token`] should provide the information for the upstream to authorize
/// the custom job that has been or will be negotiated between the Job Declarator Client and Job
/// Declarator Server.
///
/// Can be sent only on extended channel.
///
/// Previously exchanged `SetupConnection::flags` must contain `REQUIRES_WORK_SELECTION` flag i.e.,
/// work selection feature was successfully negotiated.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SetCustomMiningJob<'decoder> {
    /// Extended mining channel identifier.
    pub channel_id: u32,
    /// Specified by downstream role.
    ///
    /// Used for matching responses from upstream.
    ///
    /// The value must be connection-wide unique and is not interpreted by the upstream.
    pub request_id: u32,
    /// Provide the information for the upstream to authorize the custom job that has been or will
    /// be negotiated between the Job Declarator Client and Job Declarator Server.
    pub token: B0255<'decoder>,
    /// Version field that reflects the current network consensus.
    ///
    /// The general purpose bits (as specified in BIP320) can be freely manipulated by the
    /// downstream role. The downstream role must not rely on the upstream role to set the BIP320
    /// bits to any particular value.
    pub version: u32,
    /// Previous block’s hash.
    pub prev_hash: U256<'decoder>,
    /// Smallest `nTime` value available for hashing.
    pub min_ntime: u32,
    /// Block header field.
    pub nbits: u32,
    /// The coinbase transaction `nVersion` field.
    pub coinbase_tx_version: u32,
    /// Up to 8 bytes (not including the length byte) which are to be placed at the beginning of
    /// the coinbase field in the coinbase transaction.
    pub coinbase_prefix: B0255<'decoder>,
    /// The coinbase transaction input’s nSequence field.
    pub coinbase_tx_input_n_sequence: u32,
    /// All the outputs that will be included in the coinbase txs
    pub coinbase_tx_outputs: B064K<'decoder>,
    /// The `locktime` field in the coinbase transaction.
    pub coinbase_tx_locktime: u32,
    /// Merkle path hashes ordered from deepest.
    pub merkle_path: Seq0255<'decoder, U256<'decoder>>,
}

impl fmt::Display for SetCustomMiningJob<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SetCustomMiningJob(channel_id={}, request_id={}, token={}, version=0x{:08x}, prev_hash={}, min_ntime={}, nbits=0x{:08x}, coinbase_tx_version=0x{:08x}, coinbase_prefix={}, coinbase_tx_input_n_sequence=0x{:08x}, coinbase_tx_outputs={}, coinbase_tx_locktime={}, merkle_path={})",
            self.channel_id,
            self.request_id,
            self.token.as_hex(),
            self.version,
            self.prev_hash,
            self.min_ntime,
            self.nbits,
            self.coinbase_tx_version,
            self.coinbase_prefix.as_hex(),
            self.coinbase_tx_input_n_sequence,
            self.coinbase_tx_outputs,
            self.coinbase_tx_locktime,
            self.merkle_path
        )
    }
}

/// Message used by upstream to accept [`SetCustomMiningJob`] request.
///
/// Upon receiving this message, downstream can start submitting shares for this job immediately (by
/// using the [`SetCustomMiningJobSuccess::job_id`] provided within this response).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetCustomMiningJobSuccess {
    /// Extended mining channel identifier.
    pub channel_id: u32,
    /// Request identifier set by the downstream role.
    pub request_id: u32,
    /// Upstream’s identification of the mining job.
    pub job_id: u32,
}

impl fmt::Display for SetCustomMiningJobSuccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SetCustomMiningJobSuccess(channel_id={}, request_id={}, job_id={})",
            self.channel_id, self.request_id, self.job_id
        )
    }
}

/// Message used by upstream to reject [`SetCustomMiningJob`] request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetCustomMiningJobError<'decoder> {
    /// Extended mining channel identifier.
    pub channel_id: u32,
    /// Request identifier set by the downstream role.
    pub request_id: u32,
    /// Rejection reason.
    ///
    /// Possible errors:
    /// - invalid-channel-id
    /// - invalid-mining-job-token
    /// - invalid-job-param-value-{field_name}
    pub error_code: Str0255<'decoder>,
}

impl fmt::Display for SetCustomMiningJobError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SetCustomMiningJobError(channel_id={}, request_id={}, error_code={})",
            self.channel_id,
            self.request_id,
            self.error_code.as_utf8_or_hex()
        )
    }
}
