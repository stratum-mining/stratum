#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str032, B032};

/// # SubmitSharesStandard (Client -> Server)
///
/// Client sends result of its hashing work to the server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SubmitSharesStandard {
    /// Channel identification.
    pub channel_id: u32,
    /// Unique sequential identifier of the submit within the channel.
    pub sequence_number: u32,
    /// Identifier of the job as provided by *NewMiningJob* or
    /// *NewExtendedMiningJob* message.
    pub job_id: u32,
    /// Nonce leading to the hash being submitted.
    pub nonce: u32,
    /// The nTime field in the block header. This MUST be greater than or equal
    /// to the header_timestamp field in the latest SetNewPrevHash message
    /// and lower than or equal to that value plus the number of seconds since
    /// the receipt of that message.
    pub ntime: u32,
    /// Full nVersion field.
    pub version: u32,
}
/// # SubmitSharesExtended (Client -> Server)
/// Only relevant for extended channels. The message is the same as SubmitShares, with the
/// following additional field:
/// * extranonce
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SubmitSharesExtended<'decoder> {
    /// Channel identification.
    pub channel_id: u32,
    /// Unique sequential identifier of the submit within the channel.
    pub sequence_number: u32,
    /// Identifier of the job as provided by *NewMiningJob* or
    /// *NewExtendedMiningJob* message.
    pub job_id: u32,
    /// Nonce leading to the hash being submitted.
    pub nonce: u32,
    /// The nTime field in the block header. This MUST be greater than or equal
    /// to the header_timestamp field in the latest SetNewPrevHash message
    /// and lower than or equal to that value plus the number of seconds since
    /// the receipt of that message.
    pub ntime: u32,
    /// Full nVersion field.
    pub version: u32,
    /// Extranonce bytes which need to be added to coinbase to form a fully
    /// valid submission (full coinbase = coinbase_tx_prefix +
    /// extranonce_prefix + extranonce + coinbase_tx_suffix). The size of the
    /// provided extranonce MUST be equal to the negotiated extranonce size
    /// from channel opening.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub extranonce: B032<'decoder>,
}

/// # SubmitShares.Success (Server -> Client)
///
/// Response to SubmitShares or SubmitSharesExtended, accepting results from the miner.
/// Because it is a common case that shares submission is successful, this response can be
/// provided for multiple SubmitShare messages aggregated together.
///
/// The server doesn’t have to double check that the sequence numbers sent by a client are
/// actually increasing. It can simply use the last one received when sending a response. It is the
/// client’s responsibility to keep the sequence numbers correct/useful.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SubmitSharesSuccess {
    /// Channel identifier.
    pub channel_id: u32,
    /// Most recent sequence number with a correct result.
    pub last_sequence_number: u32,
    /// Count of new submits acknowledged within this batch.
    pub new_submits_accepted_count: u32,
    /// Sum of shares acknowledged within this batch.
    pub new_shares_sum: u64,
}

/// # SubmitShares.Error (Server -> Client)
///
/// An error is immediately submitted for every incorrect submit attempt. In case the server is not
/// able to immediately validate the submission, the error is sent as soon as the result is known.
/// This delayed validation can occur when a miner gets faster updates about a new prevhash than
/// the server does (see NewPrevHash message for details).
///
/// Possible error codes:
/// * ‘invalid-channel-id’
/// * ‘stale-share’
/// * ‘difficulty-too-low’
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SubmitSharesError<'decoder> {
    pub channel_id: u32,
    pub sequence_number: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub error_code: Str032<'decoder>,
}
