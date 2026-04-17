extern crate alloc;

use alloc::vec::Vec;

/// An allocated extranonce prefix returned by the allocator.
///
/// Stores both the raw prefix bytes (to pass to channel constructors) and the
/// `local_index` (to pass back to [`super::ExtranonceAllocator::free`] when the channel closes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtranoncePrefix {
    local_index: usize,
    prefix: Vec<u8>,
}

impl ExtranoncePrefix {
    pub(crate) fn new(local_index: usize, prefix: Vec<u8>) -> Self {
        Self {
            local_index,
            prefix,
        }
    }

    /// The raw prefix bytes — pass these to channel constructors.
    pub fn as_bytes(&self) -> &[u8] {
        &self.prefix
    }

    /// Consume and return the prefix bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.prefix
    }

    /// The assigned local index — store this and pass to [`super::ExtranonceAllocator::free`]
    /// when the channel closes.
    pub fn local_index(&self) -> usize {
        self.local_index
    }
}
