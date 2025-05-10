//! # Chain Tip
use binary_sv2::U256;

/// An abstraction over the chain tip, carrying information from `SetNewPrevHash` messages.
///
/// Used while creating non-future jobs.
#[derive(Debug, Clone)]
pub struct ChainTip {
    prev_hash: U256<'static>,
    nbits: u32,
    min_ntime: u32,
}

impl ChainTip {
    pub fn new(prev_hash: U256<'static>, nbits: u32, min_ntime: u32) -> Self {
        Self {
            prev_hash,
            nbits,
            min_ntime,
        }
    }

    pub fn prev_hash(&self) -> U256<'static> {
        self.prev_hash.clone()
    }

    pub fn nbits(&self) -> u32 {
        self.nbits
    }

    pub fn min_ntime(&self) -> u32 {
        self.min_ntime
    }
}
