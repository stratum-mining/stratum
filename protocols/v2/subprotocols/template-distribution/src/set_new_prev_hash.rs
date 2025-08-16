use alloc::vec::Vec;
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize, U256};
use core::{convert::TryInto, fmt};

/// Message used by an upstream(Template Provider) to indicate the latest block header hash
/// to mine on.
///
/// Upon validating a new best block, the upstream **must** immediately send this message.
///
/// If a [`crate::NewTemplate`] message has previously been sent with the
/// [`crate::NewTemplate::future_template`] flag set, the [`SetNewPrevHash::template_id`] field
/// **should** be set to the [`crate::NewTemplate::template_id`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SetNewPrevHash<'decoder> {
    /// Identifier of the template to mine on.
    ///
    /// This must be identical to previously sent [`crate::NewTemplate`] message.
    pub template_id: u64,
    /// Previous block’s hash, as it must appear in the next block’s header.
    pub prev_hash: U256<'decoder>,
    /// `nTime` field in the block header at which the client should start (usually current time).
    ///
    /// This is **not** the minimum valid `nTime` value.
    pub header_timestamp: u32,
    /// Block header field.
    pub n_bits: u32,
    /// The maximum double-SHA256 hash value which would represent a valid block. Note that this
    /// may be lower than the target implied by nBits in several cases, including weak-block based
    /// block propagation.
    pub target: U256<'decoder>,
}

impl fmt::Display for SetNewPrevHash<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SetNewPrevHash {{ template_id: {}, prev_hash: {}, header_timestamp: {}, n_bits: {}, target: {} }}",
            self.template_id,
            self.prev_hash,
            self.header_timestamp,
            self.n_bits,
            self.target
        )
    }
}
