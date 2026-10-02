use alloc::vec::Vec;
use binary_sv2::{Deserialize, Serialize, U256};
use core::{convert::TryInto, fmt};

/// Message used by an upstream(Template Provider) to indicate the latest block header hash
/// to mine on.
///
/// Upon validating a new best block, the upstream **must** immediately send this message.
///
/// Prior to that, the upstream **must** have sent at least one, but potentially multiple,
/// [`crate::NewTemplate`] messages with the [`crate::NewTemplate::future_template`] flag set. A
/// downstream should keep track of all of them, and convert them into `NewMiningJob` or
/// `NewExtendedMiningJob` messages with an empty `ntime_start`, in case it is also acting as a
/// server under the Mining Protocol.
///
/// [`SetNewPrevHash::template_id`] identifies which of those future templates is now valid to
/// mine on, given the [`SetNewPrevHash::prev_hash`] carried here. Once it has been activated,
/// the remaining future templates can be discarded, leaving room for the future templates
/// relative to the next `SetNewPrevHash`.
///
/// Note the ordering requirement is on the upstream: receiving this message with no future
/// template queued means the peer is not conforming to the protocol, not that the
/// [`SetNewPrevHash::template_id`] is optional.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SetNewPrevHash<'decoder> {
    /// Identifier of the template to mine on.
    ///
    /// References a [`crate::NewTemplate`] previously sent with the
    /// [`crate::NewTemplate::future_template`] flag set: the one that is now valid for
    /// [`SetNewPrevHash::prev_hash`].
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
            "SetNewPrevHash {{ template_id: {}, prev_hash: {}, header_timestamp: {}, n_bits: 0x{:08x}, target: {} }}",
            self.template_id,
            self.prev_hash,
            self.header_timestamp,
            self.n_bits,
            self.target
        )
    }
}

impl fmt::Display for SetNewPrevHashOwned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SetNewPrevHash {{ template_id: {}, prev_hash: {}, header_timestamp: {}, n_bits: 0x{:08x}, target: {} }}",
            self.template_id,
            self.prev_hash,
            self.header_timestamp,
            self.n_bits,
            self.target
        )
    }
}
