#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq0255, Serialize, Str0255, B0255, B064K, U256};
#[cfg(not(feature = "with_serde"))]
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
    #[cfg_attr(feature = "with_serde", serde(borrow))]
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
    /// The value, in satoshis, available for spending in coinbase outputs added by the client.
    /// Includes both transaction fees and block subsidy.
    pub coinbase_tx_value_remaining: u64,
    /// All the outputs that will be included in the coinbase txs
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_tx_outputs: B064K<'decoder>,
    /// The `locktime` field in the coinbase transaction.
    pub coinbase_tx_locktime: u32,
    /// Merkle path hashes ordered from deepest.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub merkle_path: Seq0255<'decoder, U256<'decoder>>,
    /// Size of extranonce in bytes that will be provided by the downstream role.
    pub extranonce_size: u16,
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
            + self.token.get_size()
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
    }
}
#[cfg(feature = "with_serde")]
impl GetSize for SetCustomMiningJobSuccess {
    fn get_size(&self) -> usize {
        self.channel_id.get_size() + self.request_id.get_size() + self.job_id.get_size()
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
        panic!("This function shouldn't be called by the Message Generator");
    }
    pub fn as_static(&self) -> SetCustomMiningJob<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
}
#[cfg(feature = "with_serde")]
impl<'a> SetCustomMiningJobError<'a> {
    pub fn into_static(self) -> SetCustomMiningJobError<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
    pub fn as_static(&self) -> SetCustomMiningJobError<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
}
#[cfg(feature = "with_serde")]
impl SetCustomMiningJobSuccess {
    pub fn into_static(self) -> SetCustomMiningJobSuccess {
        panic!("This function shouldn't be called by the Message Generator");
    }
    pub fn as_static(&self) -> SetCustomMiningJobSuccess {
        panic!("This function shouldn't be called by the Message Generator");
    }
}
