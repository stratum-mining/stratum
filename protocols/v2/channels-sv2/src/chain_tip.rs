//! # Chain Tip
use binary_sv2::U256;
use mining_sv2::SetNewPrevHash as SetNewPrevHashMp;
use template_distribution_sv2::SetNewPrevHash as SetNewPrevHashTdp;

/// An abstraction over the chain tip, carrying information from `SetNewPrevHash` messages.
///
/// Used for:
/// - creating non-future jobs
/// - validating shares.
#[derive(Debug, Clone)]
pub struct ChainTip {
    prev_hash: U256<'static>,
    nbits: u32,
    min_ntime: u32,
}

impl ChainTip {
    /// Constructs a new `ChainTip` instance.
    pub fn new(prev_hash: U256<'static>, nbits: u32, min_ntime: u32) -> Self {
        Self {
            prev_hash,
            nbits,
            min_ntime,
        }
    }

    /// Retrieves the hash of the previous block
    pub fn prev_hash(&self) -> U256<'static> {
        self.prev_hash.clone()
    }

    /// Retrieves the network difficulty for the current block
    pub fn nbits(&self) -> u32 {
        self.nbits
    }

    /// Retrieves the smallest nTime value available for hashing
    pub fn min_ntime(&self) -> u32 {
        self.min_ntime
    }
}

impl From<SetNewPrevHashTdp<'_>> for ChainTip {
    fn from(set_new_prev_hash: SetNewPrevHashTdp) -> Self {
        let set_new_prev_hash_static = set_new_prev_hash.into_static();
        let prev_hash = set_new_prev_hash_static.prev_hash;
        let nbits = set_new_prev_hash_static.n_bits;
        let min_ntime = set_new_prev_hash_static.header_timestamp;
        Self::new(prev_hash, nbits, min_ntime)
    }
}

impl From<SetNewPrevHashMp<'_>> for ChainTip {
    fn from(set_new_prev_hash: SetNewPrevHashMp) -> Self {
        let set_new_prev_hash_static = set_new_prev_hash.into_static();
        let prev_hash = set_new_prev_hash_static.prev_hash;
        let nbits = set_new_prev_hash_static.nbits;
        let min_ntime = set_new_prev_hash_static.min_ntime;
        Self::new(prev_hash, nbits, min_ntime)
    }
}
