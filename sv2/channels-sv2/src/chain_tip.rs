//! # Chain Tip
use binary_sv2::U256Owned;
use mining_sv2::{
    SetNewPrevHash as SetNewPrevHashMp, SetNewPrevHashOwned as SetNewPrevHashMpOwned,
};
use template_distribution_sv2::{
    SetNewPrevHash as SetNewPrevHashTdp, SetNewPrevHashOwned as SetNewPrevHashTdpOwned,
};

/// An abstraction over the chain tip, carrying information from `SetNewPrevHash` messages.
///
/// Used for:
/// - creating non-future jobs
/// - validating shares.
///
/// Only `prev_hash`, `nbits` and `ntime_start` are carried. The Template Distribution
/// `SetNewPrevHash.target`, which a Template Provider may set below the target `nbits` encodes
/// (weak-block propagation), is deliberately not: `channels_sv2` currently does not support
/// weak-block propagation, so the block-validity threshold is the one `nbits` encodes, and share
/// validation classifies `BlockFound` against that alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTip {
    prev_hash: U256Owned,
    nbits: u32,
    ntime_start: u32,
}

impl ChainTip {
    /// Constructs a new `ChainTip` instance.
    pub fn new(prev_hash: U256Owned, nbits: u32, ntime_start: u32) -> Self {
        Self {
            prev_hash,
            nbits,
            ntime_start,
        }
    }

    /// Retrieves the hash of the previous block
    pub fn prev_hash(&self) -> U256Owned {
        self.prev_hash.clone()
    }

    /// Retrieves the network difficulty for the current block
    pub fn nbits(&self) -> u32 {
        self.nbits
    }

    /// Retrieves the nTime value at which hashing starts
    pub fn ntime_start(&self) -> u32 {
        self.ntime_start
    }
}

/// Converts a Template Distribution `SetNewPrevHash`, dropping its `target` (see [`ChainTip`]).
impl From<SetNewPrevHashTdpOwned> for ChainTip {
    fn from(set_new_prev_hash: SetNewPrevHashTdpOwned) -> Self {
        Self::new(
            set_new_prev_hash.prev_hash,
            set_new_prev_hash.n_bits,
            set_new_prev_hash.ntime_start,
        )
    }
}

impl From<SetNewPrevHashMpOwned> for ChainTip {
    fn from(set_new_prev_hash: SetNewPrevHashMpOwned) -> Self {
        Self::new(
            set_new_prev_hash.prev_hash,
            set_new_prev_hash.nbits,
            set_new_prev_hash.ntime_start,
        )
    }
}

/// Converts a Template Distribution `SetNewPrevHash`, dropping its `target` (see [`ChainTip`]).
impl From<SetNewPrevHashTdp<'_>> for ChainTip {
    fn from(set_new_prev_hash: SetNewPrevHashTdp) -> Self {
        let set_new_prev_hash_static = set_new_prev_hash.into_owned();
        let prev_hash = set_new_prev_hash_static.prev_hash;
        let nbits = set_new_prev_hash_static.n_bits;
        let ntime_start = set_new_prev_hash_static.ntime_start;
        Self::new(prev_hash, nbits, ntime_start)
    }
}

impl From<SetNewPrevHashMp<'_>> for ChainTip {
    fn from(set_new_prev_hash: SetNewPrevHashMp) -> Self {
        let set_new_prev_hash_static = set_new_prev_hash.into_owned();
        let prev_hash = set_new_prev_hash_static.prev_hash;
        let nbits = set_new_prev_hash_static.nbits;
        let ntime_start = set_new_prev_hash_static.ntime_start;
        Self::new(prev_hash, nbits, ntime_start)
    }
}
