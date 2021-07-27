#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq0255, Serialize, B032, B064K, U256};

/// # NewMiningJob (Server -> Client)
///
/// The server provides an updated mining job to the client through a standard channel.
///
/// If the future_job field is set to *False*, the client MUST start to mine on the new job as soon as
/// possible after receiving this message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewMiningJob<'decoder> {
    /// Channel identifier, this must be a standard channel.
    pub channel_id: u32,
    /// Server’s identification of the mining job. This identifier must be provided
    /// to the server when shares are submitted later in the mining process.
    pub job_id: u32,
    /// True if the job is intended for a future *SetNewPrevHash* message sent on
    /// this channel. If False, the job relates to the last sent *SetNewPrevHash*
    /// message on the channel and the miner should start to work on the job
    /// immediately.
    pub future_job: bool,
    /// Valid version field that reflects the current network consensus. The
    /// general purpose bits (as specified in BIP320) can be freely manipulated
    /// by the downstream node. The downstream node MUST NOT rely on the
    /// upstream node to set the BIP320 bits to any particular value.
    pub version: u32,
    /// Merkle root field as used in the bitcoin block header.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub merkle_root: B032<'decoder>,
}

/// NewExtendedMiningJob (Server -> Client)
///
/// (Extended and group channels only)
/// For an *extended channel*: The whole search space of the job is owned by the specified
/// channel. If the future_job field is set to *False,* the client MUST start to mine on the new job as
/// soon as possible after receiving this message.
/// For a *group channel*: This is a broadcast variant of NewMiningJob message with the
/// merkle_root field replaced by merkle_path and coinbase TX prefix and suffix, for further traffic
/// optimization. The Merkle root is then defined deterministically for each channel by the
/// common merkle_path and unique extranonce_prefix serialized into the coinbase. The full
/// coinbase is then constructed as follows: *coinbase_tx_prefix* + *extranonce_prefix* + *coinbase_tx_suffix*.
/// The proxy MAY transform this multicast variant for downstream standard channels into
/// NewMiningJob messages by computing the derived Merkle root for them. A proxy MUST
/// translate the message for all downstream channels belonging to the group which don’t signal
/// that they accept extended mining jobs in the SetupConnection message (intended and
/// expected behaviour for end mining devices).
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewExtendedMiningJob<'decoder> {
    /// For a group channel, the message is broadcasted to all standard
    /// channels belonging to the group. Otherwise, it is addressed to
    /// the specified extended channel.
    pub channel_id: u32,
    /// Server’s identification of the mining job.
    pub job_id: u32,
    /// True if the job is intended for a future *SetNewPrevHash* message
    /// sent on the channel. If False, the job relates to the last sent
    /// *SetNewPrevHash* message on the channel and the miner should
    /// start to work on the job immediately.
    pub future_job: bool,
    /// Valid version field that reflects the current network consensus.
    pub version: u32,
    /// If set to True, the general purpose bits of version (as specified in
    /// BIP320) can be freely manipulated by the downstream node.
    /// The downstream node MUST NOT rely on the upstream node to
    /// set the BIP320 bits to any particular value.
    /// If set to False, the downstream node MUST use version as it is
    /// defined by this message.
    pub version_rolling_allowed: bool,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    /// Merkle path hashes ordered from deepest.
    pub merkle_path: Seq0255<'decoder, U256<'decoder>>,
    /// Prefix part of the coinbase transaction.
    /// The full coinbase is constructed by inserting one of the following:
    /// * For a *standard channel*: extranonce_prefix
    /// * For an *extended channel*: extranonce_prefix + extranonce (=N bytes), where N is the
    ///   negotiated extranonce space for the channel (OpenMiningChannel.Success.extranonce_size)
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub coinbase_tx_prefix: B064K<'decoder>,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    /// Suffix part of the coinbase transaction.
    pub coinbase_tx_suffix: B064K<'decoder>,
}
