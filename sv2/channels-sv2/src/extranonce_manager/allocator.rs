extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use super::prefix::ExtranoncePrefix;
use super::{bitvector::BitVector, bytes_needed, MAX_EXTRANONCE_LEN};

/// Manages extranonce prefix allocation for downstream channels.
///
/// A single allocator handles both standard and extended channels. Each call to
/// [`allocate_standard`](Self::allocate_standard) or [`allocate_extended`](Self::allocate_extended)
/// returns a unique [`ExtranoncePrefix`] whose bytes can be passed directly to
/// channel constructors. Because both channel types draw from the same
/// `local_index` pool (a single shared bitmap), their extranonces are
/// guaranteed never to overlap. When a channel closes, call [`free`](Self::free)
/// with the prefix's [`local_index`](ExtranoncePrefix::local_index) to
/// make it available for reuse.
///
/// # Memory
///
/// The allocator uses an internal bitmap of `max_channels` bits (`max_channels / 8`
/// bytes). See the module-level documentation for a reference table.
pub struct ExtranonceAllocator {
    upstream_prefix: Vec<u8>,
    server_bytes: Vec<u8>,
    total_extranonce_len: usize,
    allocation_bitmap: BitVector,
    last_allocated_index: Option<usize>,
}

impl core::fmt::Debug for ExtranonceAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExtranonceAllocator")
            .field("upstream_prefix_len", &self.upstream_prefix.len())
            .field("local_prefix_len", &self.local_prefix_len())
            .field("rollable_extranonce_size", &self.rollable_extranonce_size())
            .field("total_extranonce_len", &self.total_extranonce_len)
            .field("max_channels", &self.max_channels())
            .field("allocated_count", &self.allocation_bitmap.count_ones())
            .finish()
    }
}

impl ExtranonceAllocator {
    /// Create a fresh allocator (for Pool server / root nodes).
    ///
    /// Use this when there is no upstream node — the pool is the root of the
    /// extranonce hierarchy.
    ///
    /// - `total_extranonce_len`: Total extranonce length in bytes (≤ 32).
    /// - `server_bytes`: Optional static server identifier (placed at the start of local_prefix).
    /// - `max_channels`: Maximum concurrent channels. Determines the size of the
    ///   internal allocation bitmap (`max_channels / 8` bytes). See the
    ///   [Memory](#memory) section on [`ExtranonceAllocator`] for a reference table.
    ///
    /// `local_prefix_len` is derived automatically:
    /// `server_bytes.len() + bytes_needed(max_channels)`.
    pub fn new(
        total_extranonce_len: usize,
        server_bytes: Option<Vec<u8>>,
        max_channels: usize,
    ) -> Result<Self, ExtranonceAllocatorError> {
        let server_bytes = server_bytes.unwrap_or_default();
        Self::validate_and_create(Vec::new(), server_bytes, total_extranonce_len, max_channels)
    }

    /// Create an allocator seeded with an upstream-assigned prefix (for proxies,
    /// JD clients, and translators).
    ///
    /// Use this when the node receives an `extranonce_prefix` from upstream via
    /// `OpenExtendedMiningChannelSuccess` or `SetExtranoncePrefix`, and needs to
    /// subdivide the remaining space for its own downstream channels.
    ///
    /// - `upstream_prefix`: The prefix bytes received from the upstream node.
    /// - `total_extranonce_len`: Total extranonce length.
    /// - `max_channels`: Maximum concurrent channels. Determines the size of the
    ///   internal allocation bitmap (`max_channels / 8` bytes). See the
    ///   [Memory](#memory) section on [`ExtranonceAllocator`] for a reference table.
    ///
    /// `local_prefix_len` is derived automatically: `bytes_needed(max_channels)`.
    pub fn from_upstream(
        upstream_prefix: Vec<u8>,
        total_extranonce_len: usize,
        max_channels: usize,
    ) -> Result<Self, ExtranonceAllocatorError> {
        Self::validate_and_create(
            upstream_prefix,
            Vec::new(),
            total_extranonce_len,
            max_channels,
        )
    }

    /// Allocate a prefix for an extended channel.
    ///
    /// `min_rollable_size` is the downstream's requested minimum extranonce space
    /// for rolling (from the `OpenExtendedMiningChannel` message). It must be
    /// ≤ `rollable_extranonce_size`.
    ///
    /// Returns a prefix of length `upstream_prefix_len + local_prefix_len`.
    pub fn allocate_extended(
        &mut self,
        min_rollable_size: usize,
    ) -> Result<ExtranoncePrefix, ExtranonceAllocatorError> {
        if min_rollable_size > self.rollable_extranonce_size() {
            return Err(ExtranonceAllocatorError::InvalidRollableSize);
        }
        let idx = self
            .find_free_index()
            .ok_or(ExtranonceAllocatorError::CapacityExhausted)?;
        self.mark_allocated(idx);
        Ok(ExtranoncePrefix::new(idx, self.build_extended_prefix(idx)))
    }

    /// Allocate a prefix for a standard channel.
    ///
    /// Returns a prefix of length `total_extranonce_len` (the full extranonce).
    /// The `rollable_extranonce_size` portion is zero-padded since standard channels
    /// don't roll.
    pub fn allocate_standard(&mut self) -> Result<ExtranoncePrefix, ExtranonceAllocatorError> {
        let idx = self
            .find_free_index()
            .ok_or(ExtranonceAllocatorError::CapacityExhausted)?;
        self.mark_allocated(idx);
        Ok(ExtranoncePrefix::new(idx, self.build_standard_prefix(idx)))
    }

    /// Free a previously allocated local index, making it available for reuse.
    pub fn free(&mut self, local_index: usize) {
        if local_index < self.max_channels() {
            self.allocation_bitmap.set(local_index, false);
        }
    }

    /// Bytes available for downstream rolling (extended channels).
    pub fn rollable_extranonce_size(&self) -> usize {
        self.total_extranonce_len - self.upstream_prefix.len() - self.local_prefix_len()
    }

    /// Length of the upstream prefix portion.
    pub fn upstream_prefix_len(&self) -> usize {
        self.upstream_prefix.len()
    }

    /// The upstream prefix bytes.
    pub fn upstream_prefix(&self) -> &[u8] {
        &self.upstream_prefix
    }

    /// Total extranonce length.
    pub fn total_extranonce_len(&self) -> usize {
        self.total_extranonce_len
    }

    /// Length of the local prefix portion (`server_bytes.len() + local_index_len`).
    pub fn local_prefix_len(&self) -> usize {
        self.server_bytes.len() + self.local_index_len()
    }

    /// Number of currently allocated channels.
    pub fn allocated_count(&self) -> usize {
        self.allocation_bitmap.count_ones()
    }

    /// Maximum number of concurrent channels.
    pub fn max_channels(&self) -> usize {
        self.allocation_bitmap.capacity()
    }

    /// Number of bytes used to encode each local index.
    fn local_index_len(&self) -> usize {
        bytes_needed(self.max_channels())
    }

    /// Validate configuration and create the allocator. Shared by `new()` and `from_upstream()`.
    fn validate_and_create(
        upstream_prefix: Vec<u8>,
        server_bytes: Vec<u8>,
        total_extranonce_len: usize,
        max_channels: usize,
    ) -> Result<Self, ExtranonceAllocatorError> {
        if total_extranonce_len > MAX_EXTRANONCE_LEN {
            return Err(ExtranonceAllocatorError::ExceedsMaxLength);
        }
        if max_channels == 0 {
            return Err(ExtranonceAllocatorError::InvalidLengths);
        }

        let local_index_len = bytes_needed(max_channels);
        let local_prefix_len = server_bytes.len() + local_index_len;

        if upstream_prefix.len() + local_prefix_len > total_extranonce_len {
            return Err(ExtranonceAllocatorError::InvalidLengths);
        }

        let allocation_bitmap = BitVector::new(max_channels);

        Ok(Self {
            upstream_prefix,
            server_bytes,
            total_extranonce_len,
            allocation_bitmap,
            last_allocated_index: None,
        })
    }

    /// Mark a local index as in-use and record it as the latest allocation.
    fn mark_allocated(&mut self, index: usize) {
        self.allocation_bitmap.set(index, true);
        self.last_allocated_index = Some(index);
    }

    /// Find the next available local index.
    ///
    /// Scans forward from the last allocation point, wrapping around.
    fn find_free_index(&self) -> Option<usize> {
        let max = self.max_channels();
        let start = self.last_allocated_index.map_or(0, |idx| (idx + 1) % max);

        self.allocation_bitmap
            .find_first_zero_in_range(start, max)
            .or_else(|| {
                if start > 0 {
                    self.allocation_bitmap.find_first_zero_in_range(0, start)
                } else {
                    None
                }
            })
    }

    /// Build the prefix bytes for an extended channel.
    ///
    /// Layout: `[upstream_prefix | server_bytes | local_index_bytes]`
    fn build_extended_prefix(&self, local_index: usize) -> Vec<u8> {
        let mut prefix = Vec::with_capacity(self.upstream_prefix.len() + self.local_prefix_len());
        prefix.extend_from_slice(&self.upstream_prefix);
        prefix.extend_from_slice(&self.server_bytes);
        prefix.extend_from_slice(&Self::local_index_to_bytes(
            local_index,
            self.local_index_len(),
        ));
        prefix
    }

    /// Build the prefix bytes for a standard channel.
    ///
    /// Same as the extended prefix, plus zero-padded rollable bytes at the end
    /// (standard channels don't roll, so we fill the rollable bytes with 0).
    fn build_standard_prefix(&self, local_index: usize) -> Vec<u8> {
        let mut prefix = self.build_extended_prefix(local_index);
        prefix.resize(self.total_extranonce_len, 0);
        prefix
    }

    /// Encode a `local_index` (usize) into `len` big-endian bytes.
    ///
    /// Example: `local_index_to_bytes(258, 2)` → `[0x01, 0x02]`
    fn local_index_to_bytes(local_index: usize, len: usize) -> Vec<u8> {
        let mut result = vec![0u8; len];
        let be_bytes = local_index.to_be_bytes();
        let copy_len = be_bytes.len().min(len);
        let src_start = be_bytes.len() - copy_len;
        let dst_start = len - copy_len;
        result[dst_start..].copy_from_slice(&be_bytes[src_start..]);
        result
    }
}

/// Errors returned by [`ExtranonceAllocator`] operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtranonceAllocatorError {
    /// `total_extranonce_len` exceeds [`MAX_EXTRANONCE_LEN`] (32).
    ExceedsMaxLength,
    /// The inputs don't fit within `total_extranonce_len`, or `max_channels` is zero.
    InvalidLengths,
    /// All channels are allocated — no more capacity.
    CapacityExhausted,
    /// The requested rollable extranonce size exceeds `rollable_extranonce_size`.
    InvalidRollableSize,
}

impl core::fmt::Display for ExtranonceAllocatorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ExceedsMaxLength => write!(f, "total extranonce length exceeds 32 bytes"),
            Self::InvalidLengths => write!(f, "invalid length configuration"),
            Self::CapacityExhausted => write!(f, "all channels are allocated — no more capacity"),
            Self::InvalidRollableSize => {
                write!(f, "requested rollable size exceeds available space")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn pool_basic_allocation() {
        // server_bytes(2) + bytes_needed(65536)(2) = 4 byte local prefix
        let mut alloc = ExtranonceAllocator::new(20, Some(vec![0x00, 0x01]), 65536).unwrap();

        assert_eq!(alloc.total_extranonce_len(), 20);
        assert_eq!(alloc.local_prefix_len(), 4);
        assert_eq!(alloc.upstream_prefix_len(), 0);
        assert_eq!(alloc.rollable_extranonce_size(), 16);
        assert_eq!(alloc.max_channels(), 65536);
        assert_eq!(alloc.allocated_count(), 0);

        let ext = alloc.allocate_extended(16).unwrap();
        assert_eq!(ext.as_bytes().len(), 4);
        assert_eq!(&ext.as_bytes()[0..2], &[0x00, 0x01]);
        assert_eq!(alloc.allocated_count(), 1);

        let std = alloc.allocate_standard().unwrap();
        assert_eq!(std.as_bytes().len(), 20);
        assert_eq!(&std.as_bytes()[0..2], &[0x00, 0x01]);
        assert_eq!(alloc.allocated_count(), 2);
    }

    #[test]
    fn proxy_from_upstream() {
        // bytes_needed(65536) = 2, no server_bytes -> local_prefix = 2
        let upstream_prefix = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let mut alloc =
            ExtranonceAllocator::from_upstream(upstream_prefix.clone(), 20, 65536).unwrap();

        assert_eq!(alloc.upstream_prefix(), &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(alloc.upstream_prefix_len(), 4);
        assert_eq!(alloc.local_prefix_len(), 2);
        assert_eq!(alloc.rollable_extranonce_size(), 14);

        let ext = alloc.allocate_extended(14).unwrap();
        assert_eq!(ext.as_bytes().len(), 6);
        assert_eq!(&ext.as_bytes()[0..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn uniqueness() {
        // bytes_needed(256) = 1, local_prefix = 1, rollable = 19
        let mut alloc = ExtranonceAllocator::new(20, None, 256).unwrap();
        let mut seen = HashSet::new();

        for _ in 0..256 {
            let p = alloc.allocate_extended(19).unwrap();
            assert!(seen.insert(p.as_bytes().to_vec()), "duplicate prefix");
        }

        assert_eq!(alloc.allocated_count(), 256);
    }

    #[test]
    fn exhaustion_and_reuse() {
        // bytes_needed(256) = 1, rollable = 5
        let mut alloc = ExtranonceAllocator::new(6, None, 256).unwrap();

        let mut ids = Vec::new();
        for _ in 0..256 {
            let p = alloc.allocate_extended(5).unwrap();
            ids.push(p.local_index());
        }

        assert!(alloc.allocate_extended(5).is_err());
        assert_eq!(alloc.allocated_count(), 256);

        alloc.free(ids[42]);
        assert_eq!(alloc.allocated_count(), 255);

        let reused = alloc.allocate_extended(5).unwrap();
        assert_eq!(reused.local_index(), ids[42]);
        assert_eq!(alloc.allocated_count(), 256);
    }

    #[test]
    fn free_reuses_first_available_index() {
        let mut alloc = ExtranonceAllocator::new(6, None, 256).unwrap();

        let mut ids = Vec::new();
        for _ in 0..256 {
            ids.push(alloc.allocate_extended(5).unwrap().local_index());
        }

        let id_a = ids[20];
        let id_b = ids[10];
        alloc.free(id_a);
        alloc.free(id_b);

        let reused = alloc.allocate_extended(5).unwrap();
        assert_eq!(reused.local_index(), id_b);
    }

    #[test]
    fn standard_prefix_includes_rollable_zeros() {
        // server_bytes(1) + bytes_needed(256)(1) = 2 byte local prefix
        let mut alloc = ExtranonceAllocator::new(20, Some(vec![0x01]), 256).unwrap();

        let std = alloc.allocate_standard().unwrap();
        assert_eq!(std.as_bytes().len(), 20);
        assert!(std.as_bytes()[2..].iter().all(|&b| b == 0));
    }

    #[test]
    fn invalid_rollable_size() {
        // bytes_needed(256) = 1, rollable = 19
        let mut alloc = ExtranonceAllocator::new(20, None, 256).unwrap();
        let err = alloc.allocate_extended(20);
        assert_eq!(err, Err(ExtranonceAllocatorError::InvalidRollableSize));
    }

    #[test]
    fn validation_errors() {
        assert_eq!(
            ExtranonceAllocator::new(33, None, 256).unwrap_err(),
            ExtranonceAllocatorError::ExceedsMaxLength
        );

        assert_eq!(
            ExtranonceAllocator::new(20, None, 0).unwrap_err(),
            ExtranonceAllocatorError::InvalidLengths
        );

        // server_bytes(2) + bytes_needed(256)(1) = 3, but total = 2
        assert_eq!(
            ExtranonceAllocator::new(2, Some(vec![0x01, 0x02]), 256).unwrap_err(),
            ExtranonceAllocatorError::InvalidLengths
        );

        // upstream(20) + bytes_needed(256)(1) = 21 > total(20)
        assert_eq!(
            ExtranonceAllocator::from_upstream(vec![0; 20], 20, 256).unwrap_err(),
            ExtranonceAllocatorError::InvalidLengths
        );
    }

    #[test]
    fn no_overlap_standard_and_extended() {
        // bytes_needed(256) = 1, rollable = 7
        let mut alloc = ExtranonceAllocator::new(8, None, 256).unwrap();

        let ext = alloc.allocate_extended(7).unwrap();
        let std = alloc.allocate_standard().unwrap();

        assert_ne!(ext.local_index(), std.local_index());
        assert_ne!(&ext.as_bytes()[..1], &std.as_bytes()[..1]);
    }
}
