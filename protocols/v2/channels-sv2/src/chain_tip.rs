//! # Chain Tip
use binary_sv2::U256;

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
