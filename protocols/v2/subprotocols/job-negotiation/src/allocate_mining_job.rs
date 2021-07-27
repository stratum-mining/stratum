#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str0255, B0255};

/// # AllocateMiningJobToken(Client->Server)
///
/// A request to get an identifier for a future-submitted mining job. Ratelimited to a rather slow
/// rate and only available on connections where this has been negotiated. Otherwise, only
/// mining_job_token(s) from CreateMiningJob.Success are valid.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AllocateMiningJobToken<'decoder> {
    /// Unconstrained sequence of bytes. Whatever is needed by the pool to
    /// identify/authenticate the client, e.g. “braiinstest”. Additional restrictions
    /// can be imposed by the pool. It is highly recommended that UTF-8
    /// encoding is used.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub user_identifier: Str0255<'decoder>,
    /// Unique identifier for pairing the response.
    pub request_id: u32,
}

/// # AllocateMiningJobToken.Success(Server -> Client)
///
/// The Server MUST NOT change the value of coinbase_output_max_additional_size in
/// AllocateMiningJobToken.Success messages unless required for changes to the pool’s
/// configuration. Notably, if the pool intends to change the space it requires for coinbase
/// transaction outputs regularly, it should simply prefer to use the maximum of all such output
/// sizes as the coinbase_output_max_additional_size value.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AllocateMiningJobTokenSuccess<'decoder> {
    /// Unique identifier for pairing the response.
    pub request_id: u32,
    /// Token that makes the client eligible for committing a mining job for
    /// approval/transaction negotiation or for identifying custom mining job
    /// on mining connection.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub mining_job_token: B0255<'decoder>,
    /// The maximum additional serialized bytes which the pool will add in
    /// coinbase transaction outputs. See discussion in the Template
    /// Distribution Protocol’s CoinbaseOutputDataSize message for more
    /// details.
    pub coinbase_output_max_additional_size: u32,
    /// If true, the mining_job_token can be used immediately on a mining
    /// connection in the SetCustomMiningJob message, even before
    /// CommitMiningJob and CommitMiningJob.Success messages have
    /// been sent and received.
    /// If false, Job Negotiator MUST use this token for CommitMiningJob
    /// only.
    /// This MUST be true when SetupConnection.flags had
    /// REQUIRES_ASYNC_JOB_MINING set.
    pub async_mining_allowed: bool,
}
