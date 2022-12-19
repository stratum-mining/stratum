#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq0255, Seq064K, Serialize, Str0255, B0255, B064K, U256};
use core::convert::TryInto;

/// # SetCustomMiningJob (Client -> Server)
///
/// Can be sent only on extended channel. SetupConnection.flags MUST contain
/// *REQUIRES_WORK_SELECTION* flag (work selection feature successfully negotiated).
/// The downstream node has a custom job negotiated by a trusted external Job Negotiator. The
/// mining_job_token provides the information for the pool to authorize the custom job that has
/// been or will be negotiated between the Job Negotiator and Pool.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetCustomMiningJob<'decoder> {
    /// Extended channel identifier.
    pub channel_id: u32,
    /// Client-specified identifier for pairing responses.
    pub request_id: u32,
    /// Token provided by the pool which uniquely identifies
    /// the job that the Job Negotiator has negotiated with the
    /// pool. See the Job Negotiation Protocol for more
    /// details.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub mining_job_token: B0255<'decoder>,
    /// Valid version field that reflects the current network
    /// consensus. The general purpose bits (as specified in
    /// BIP320) can be freely manipulated by the downstream
    /// node.
    pub version: u32,
    /// Previous block’s hash, found in the block header field.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub prev_hash: U256<'decoder>,
    /// Smallest nTime value available for hashing.
    pub min_ntime: u32,
    /// Block header field.
    pub nbits: u32,
    /// The coinbase transaction nVersion field.
    pub coinbase_tx_version: u32,
    /// Up to 8 bytes (not including the length byte) which are
    /// to be placed at the beginning of the coinbase field in
    /// the coinbase transaction.
    pub coinbase_prefix: u32,
    /// The coinbase transaction input’s nSequence field.
    pub coinbase_tx_input_n_sequence: u32,
    /// The value, in satoshis, available for spending in
    /// coinbase outputs added by the client. Includes both
    /// transaction fees and block subsidy.
    pub coinbase_tx_value_remaining: u64,
    /// Bitcoin transaction outputs to be included as the last
    /// outputs in the coinbase transaction.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_tx_outputs: Seq064K<'decoder, B064K<'decoder>>,
    /// The locktime field in the coinbase transaction.
    pub coinbase_tx_locktime: u32,
    /// Merkle path hashes ordered from deepest.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub merkle_path: Seq0255<'decoder, U256<'decoder>>,
    /// Size of extranonce in bytes that will be provided by the
    /// downstream node.
    pub extranonce_size: u16,
    /// TBD: Can be custom job ever future?
    pub future_job: bool,
}

/// # SetCustomMiningJob.Success (Server -> Client)
///
/// Response from the server when it accepts the custom mining job. Client can start to mine on
/// the job immediately (by using the job_id provided within this response).
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetCustomMiningJobSuccess<'decoder> {
    /// Extended channel identifier.
    pub channel_id: u32,
    /// Client-specified identifier for pairing responses. Value from the request
    /// MUST be provided by upstream in the response message.
    pub request_id: u32,
    /// Server’s identification of the mining job.
    pub job_id: u32,
    /// Prefix part of the coinbase transaction*.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_tx_prefix: B064K<'decoder>,
    /// Suffix part of the coinbase transaction.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_tx_suffix: B064K<'decoder>,
}

/// # SetCustomMiningJob.Error (Server -> Client)
///
/// Possible errors:
/// * ‘invalid-channel-id’
/// * ‘invalid-mining-job-token’
/// * ‘invalid-job-param-value-{}’ - {} is replaced by a particular field name from SetCustomMiningJob message
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetCustomMiningJobError<'decoder> {
    /// Extended channel identifier.
    pub channel_id: u32,
    /// Client-specified identifier for pairing responses. Value from the request
    /// MUST be provided by upstream in the response message.
    pub request_id: u32,
    /// Reason why the custom job has been rejected.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub error_code: Str0255<'decoder>,
}
#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for SetCustomMiningJob<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size()
            + self.request_id.get_size()
            + self.mining_job_token.get_size()
            + self.version.get_size()
            + self.prev_hash.get_size()
            + self.min_ntime.get_size()
            + self.nbits.get_size()
            + self.coinbase_tx_version.get_size()
            + self.coinbase_prefix.get_size()
            + self.coinbase_tx_input_n_sequence.get_size()
            + self.coinbase_tx_value_remaining.get_size()
            + self.coinbase_tx_outputs.get_size()
            + self.coinbase_tx_locktime.get_size()
            + self.merkle_path.get_size()
            + self.extranonce_size.get_size()
            + self.future_job.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for SetCustomMiningJobSuccess<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size()
            + self.request_id.get_size()
            + self.job_id.get_size()
            + self.coinbase_tx_prefix.get_size()
            + self.coinbase_tx_suffix.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for SetCustomMiningJobError<'d> {
    fn get_size(&self) -> usize {
        self.channel_id.get_size() + self.request_id.get_size() + self.error_code.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'a> SetCustomMiningJob<'a> {
    pub fn into_static(self) -> SetCustomMiningJob<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> SetCustomMiningJob<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
#[cfg(feature = "with_serde")]
impl<'a> SetCustomMiningJobError<'a> {
    pub fn into_static(self) -> SetCustomMiningJobError<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> SetCustomMiningJobError<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
#[cfg(feature = "with_serde")]
impl<'a> SetCustomMiningJobSuccess<'a> {
    pub fn into_static(self) -> SetCustomMiningJobSuccess<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> SetCustomMiningJobSuccess<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
