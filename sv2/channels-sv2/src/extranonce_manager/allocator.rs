extern crate alloc;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use super::prefix::AllocatedExtranoncePrefix;
use super::{bitvector::BitVector, bytes_needed, MAX_EXTRANONCE_LEN};

/// Manages extranonce prefix allocation for downstream channels.
///
/// A single allocator handles both standard and extended channels. Each call to
/// [`allocate_standard`](Self::allocate_standard) or [`allocate_extended`](Self::allocate_extended)
/// returns a unique [`AllocatedExtranoncePrefix`] whose bytes can be passed
/// directly to channel constructors. Because both channel types draw from the
/// same `local_index` pool (a single shared bitmap), their extranonces are
/// guaranteed never to overlap.
///
/// # Layout
///
/// ```text
/// [ upstream_prefix ][ local_prefix ][ local_index ][ rollable ]
///      upstream          caller         allocator      miner
/// ```
///
/// - `upstream_prefix`: Bytes assigned by an upstream node
///   (empty for a pool / root allocator).
/// - `local_prefix`: Caller-owned static bytes placed inside the allocator's
///   region, typically used as a node identifier or to shrink `rollable` to a
///   specific target size.
/// - `local_index`: Per-channel dynamic bytes managed by the allocator; width
///   is derived from `max_channels` via [`bytes_needed`].
/// - `rollable`: Bytes left for downstream rolling (extended channels) or
///   zero-padded (standard channels).
///
/// # Lifecycle
///
/// Allocation lifetime is managed by ownership: each
/// [`AllocatedExtranoncePrefix`] releases its allocation automatically when
/// dropped. There is no explicit release API. Store the prefix alongside
/// the channel state and the allocation is returned to the allocator when
/// the channel state is dropped.
///
/// # Memory
///
/// The allocator uses an internal bitmap of `max_channels` bits
/// (`max_channels / 8` bytes). See the module-level documentation for a
/// reference table.
pub struct ExtranonceAllocator {
    upstream_prefix: Vec<u8>,
    local_prefix_bytes: Vec<u8>,
    total_extranonce_len: u8,
    /// `Arc` so each outstanding [`AllocatedExtranoncePrefix`] can hold a
    /// `Weak` reference and clear its own bit from `Drop` without any locking.
    allocation_bitmap: Arc<BitVector>,
    last_allocated_index: Option<usize>,
}

impl core::fmt::Debug for ExtranonceAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExtranonceAllocator")
            .field("upstream_prefix_len", &self.upstream_prefix_len())
            .field("local_prefix_len", &self.local_prefix_len())
            .field("local_index_len", &self.local_index_len())
            .field("rollable_extranonce_size", &self.rollable_extranonce_size())
            .field("total_extranonce_len", &self.total_extranonce_len)
            .field("max_channels", &self.max_channels())
            .field("allocated_count", &self.allocation_bitmap.count_ones())
            .finish()
    }
}

impl ExtranonceAllocator {
    /// Create a fresh allocator (for pool / root nodes with no upstream).
    ///
    /// - `local_prefix_bytes`: Caller-owned static bytes placed at the start
    ///   of the allocator's region. Can be empty. Typically used as a server
    ///   identifier.
    /// - `total_extranonce_len`: Total extranonce length in bytes (≤ 32).
    /// - `max_channels`: Maximum concurrent channels. Determines the size of
    ///   the internal allocation bitmap (`max_channels / 8` bytes) and the
    ///   width of the `local_index` region. See the
    ///   [Memory](#memory) section on [`ExtranonceAllocator`] for a reference
    ///   table.
    pub fn new(
        local_prefix_bytes: Vec<u8>,
        total_extranonce_len: u8,
        max_channels: u32,
    ) -> Result<Self, ExtranonceAllocatorError> {
        Self::validate_and_create(
            Vec::new(),
            local_prefix_bytes,
            total_extranonce_len,
            max_channels,
        )
    }

    /// Create an allocator seeded with an upstream-assigned prefix (for
    /// proxies such as JDC, Translator, and any node that sub-allocates an
    /// upstream-assigned prefix).
    ///
    /// Use this when the node receives an `extranonce_prefix` from the
    /// upstream node (via `OpenExtendedMiningChannelSuccess` or
    /// `SetExtranoncePrefix`) and needs to subdivide the remaining space for
    /// its own downstream channels.
    ///
    /// - `upstream_prefix_bytes`: The prefix bytes received from the upstream
    ///   node.
    /// - `local_prefix_bytes`: Caller-owned static bytes placed immediately
    ///   after the upstream prefix. Can be empty. Useful for shrinking
    ///   `rollable` to a specific target size, or for embedding a tag/identifier.
    /// - `total_extranonce_len`: Total extranonce length in bytes (≤ 32).
    /// - `max_channels`: Maximum concurrent channels. Determines the width of
    ///   `local_index` and the size of the internal allocation bitmap. See
    ///   the [Memory](#memory) section on [`ExtranonceAllocator`] for a
    ///   reference table.
    pub fn from_upstream_prefix(
        upstream_prefix_bytes: Vec<u8>,
        local_prefix_bytes: Vec<u8>,
        total_extranonce_len: u8,
        max_channels: u32,
    ) -> Result<Self, ExtranonceAllocatorError> {
        Self::validate_and_create(
            upstream_prefix_bytes,
            local_prefix_bytes,
            total_extranonce_len,
            max_channels,
        )
    }

    /// Allocate a prefix for an extended channel.
    ///
    /// `min_rollable_size` is the downstream's requested minimum extranonce
    /// space for rolling (from the `OpenExtendedMiningChannel` message). It
    /// must be ≤ [`rollable_extranonce_size`](Self::rollable_extranonce_size).
    ///
    /// Returns a prefix of length [`full_prefix_len`](Self::full_prefix_len)
    /// (= `upstream_prefix_len + local_prefix_len + local_index_len`).
    ///
    /// # Lifecycle
    ///
    /// The allocation is released automatically when the returned
    /// [`AllocatedExtranoncePrefix`] is dropped. Store the prefix on the
    /// channel state and let ownership handle cleanup — no explicit
    /// release call is needed.
    pub fn allocate_extended(
        &mut self,
        min_rollable_size: usize,
    ) -> Result<AllocatedExtranoncePrefix, ExtranonceAllocatorError> {
        if min_rollable_size > self.rollable_extranonce_size() as usize {
            return Err(ExtranonceAllocatorError::InvalidRollableSize);
        }
        let idx = self
            .find_free_index()
            .ok_or(ExtranonceAllocatorError::CapacityExhausted)?;
        self.mark_allocated(idx);
        Ok(AllocatedExtranoncePrefix::from_allocation(
            idx as u32,
            self.upstream_prefix_len(),
            self.build_extended_prefix(idx),
            Arc::downgrade(&self.allocation_bitmap),
        ))
    }

    /// Allocate a prefix for a standard channel.
    ///
    /// Returns a prefix of length `total_extranonce_len` (the full extranonce).
    /// The `rollable` portion is zero-padded since standard channels
    /// don't roll.
    ///
    /// # Lifecycle
    ///
    /// The allocation is released automatically when the returned
    /// [`AllocatedExtranoncePrefix`] is dropped. Store the prefix on the
    /// channel state and let ownership handle cleanup — no explicit
    /// release call is needed.
    pub fn allocate_standard(
        &mut self,
    ) -> Result<AllocatedExtranoncePrefix, ExtranonceAllocatorError> {
        let idx = self
            .find_free_index()
            .ok_or(ExtranonceAllocatorError::CapacityExhausted)?;
        self.mark_allocated(idx);
        Ok(AllocatedExtranoncePrefix::from_allocation(
            idx as u32,
            self.upstream_prefix_len(),
            self.build_standard_prefix(idx),
            Arc::downgrade(&self.allocation_bitmap),
        ))
    }

    /// The `upstream_prefix` region bytes.
    #[inline]
    pub fn upstream_prefix(&self) -> &[u8] {
        &self.upstream_prefix
    }

    /// Length of the `upstream_prefix` region.
    ///
    /// Bounded by `total_extranonce_len` (≤ 32), hence `u8`.
    #[inline]
    pub fn upstream_prefix_len(&self) -> u8 {
        self.upstream_prefix.len() as u8
    }

    /// The `local_prefix` region bytes (caller-owned static bytes only).
    ///
    /// Does **not** include the `local_index` region; use
    /// [`local_index_len`](Self::local_index_len) for that.
    #[inline]
    pub fn local_prefix(&self) -> &[u8] {
        &self.local_prefix_bytes
    }

    /// Length of the `local_prefix` region (caller-owned static bytes only).
    ///
    /// Does **not** include the `local_index` region; use
    /// [`local_index_len`](Self::local_index_len) for that. Bounded by
    /// `total_extranonce_len` (≤ 32), hence `u8`.
    #[inline]
    pub fn local_prefix_len(&self) -> u8 {
        self.local_prefix_bytes.len() as u8
    }

    /// Length of the `local_index` region (allocator-managed per-channel bytes).
    ///
    /// Derived from `max_channels` via [`bytes_needed`]. Bounded by
    /// `total_extranonce_len` (≤ 32), hence `u8`.
    #[inline]
    pub fn local_index_len(&self) -> u8 {
        bytes_needed(self.max_channels())
    }

    /// Bytes available for downstream rolling (extended channels).
    ///
    /// Equal to
    /// `total_extranonce_len − upstream_prefix_len − local_prefix_len − local_index_len`.
    /// Bounded by `total_extranonce_len` (≤ 32), hence `u8`.
    #[inline]
    pub fn rollable_extranonce_size(&self) -> u8 {
        self.total_extranonce_len
            - self.upstream_prefix_len()
            - self.local_prefix_len()
            - self.local_index_len()
    }

    /// Length of the full prefix produced by this allocator:
    /// `upstream_prefix_len + local_prefix_len + local_index_len`.
    ///
    /// This is the length of every [`AllocatedExtranoncePrefix`] returned by
    /// [`allocate_extended`](Self::allocate_extended). Standard prefixes are
    /// longer by `rollable_extranonce_size` (zero-padded).
    #[inline]
    pub fn full_prefix_len(&self) -> u8 {
        self.upstream_prefix_len() + self.local_prefix_len() + self.local_index_len()
    }

    /// Total extranonce length.
    #[inline]
    pub fn total_extranonce_len(&self) -> u8 {
        self.total_extranonce_len
    }

    /// Number of currently allocated channels.
    ///
    /// Bounded by `max_channels` (`u32`).
    #[inline]
    pub fn allocated_count(&self) -> u32 {
        self.allocation_bitmap.count_ones()
    }

    /// Maximum number of concurrent channels.
    #[inline]
    pub fn max_channels(&self) -> u32 {
        // The bitmap was constructed from a `u32` passed to `new`/`from_upstream_prefix`,
        // so this cast is lossless.
        self.allocation_bitmap.capacity() as u32
    }

    /// Validate configuration and create the allocator. Shared by `new()` and `from_upstream_prefix()`.
    fn validate_and_create(
        upstream_prefix: Vec<u8>,
        local_prefix_bytes: Vec<u8>,
        total_extranonce_len: u8,
        max_channels: u32,
    ) -> Result<Self, ExtranonceAllocatorError> {
        if total_extranonce_len > MAX_EXTRANONCE_LEN {
            return Err(ExtranonceAllocatorError::ExceedsMaxLength);
        }
        if max_channels == 0 {
            return Err(ExtranonceAllocatorError::ZeroMaxChannels);
        }

        let local_index_len = bytes_needed(max_channels) as usize;
        let needed = upstream_prefix.len() + local_prefix_bytes.len() + local_index_len;
        if needed > total_extranonce_len as usize {
            return Err(ExtranonceAllocatorError::PrefixExceedsTotalLength);
        }

        let allocation_bitmap = Arc::new(BitVector::new(max_channels as usize));

        Ok(Self {
            upstream_prefix,
            local_prefix_bytes,
            total_extranonce_len,
            allocation_bitmap,
            last_allocated_index: None,
        })
    }

    /// Mark a local index as in-use and record it as the latest allocation.
    #[inline]
    fn mark_allocated(&mut self, index: usize) {
        self.allocation_bitmap.set(index, true);
        self.last_allocated_index = Some(index);
    }

    /// Find the next available local index.
    ///
    /// Scans forward from the last allocation point, wrapping around.
    fn find_free_index(&self) -> Option<usize> {
        let max = self.max_channels() as usize;
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
    /// Layout: `[ upstream_prefix | local_prefix | local_index ]`
    fn build_extended_prefix(&self, local_index: usize) -> Vec<u8> {
        let mut prefix = Vec::with_capacity(self.full_prefix_len() as usize);
        prefix.extend_from_slice(&self.upstream_prefix);
        prefix.extend_from_slice(&self.local_prefix_bytes);
        prefix.extend_from_slice(&Self::local_index_to_bytes(
            local_index,
            self.local_index_len() as usize,
        ));
        prefix
    }

    /// Build the prefix bytes for a standard channel.
    ///
    /// Same as [`build_extended_prefix`](Self::build_extended_prefix), plus
    /// zero-padded `rollable` bytes at the end (standard channels don't roll).
    fn build_standard_prefix(&self, local_index: usize) -> Vec<u8> {
        let mut prefix = self.build_extended_prefix(local_index);
        prefix.resize(self.total_extranonce_len as usize, 0);
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
    /// `total_extranonce_len` exceeds [`MAX_EXTRANONCE_LEN`].
    ExceedsMaxLength,
    /// `max_channels` must be greater than zero.
    ZeroMaxChannels,
    /// `upstream_prefix_len + local_prefix_len + local_index_len` exceeds `total_extranonce_len`.
    PrefixExceedsTotalLength,
    /// All channels are allocated — no more capacity.
    CapacityExhausted,
    /// The requested rollable extranonce size exceeds `rollable_extranonce_size`.
    InvalidRollableSize,
}

impl core::fmt::Display for ExtranonceAllocatorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ExceedsMaxLength => {
                write!(f, "total_extranonce_len exceeds {MAX_EXTRANONCE_LEN} bytes")
            }
            Self::ZeroMaxChannels => write!(f, "max_channels must be greater than zero"),
            Self::PrefixExceedsTotalLength => {
                write!(
                    f,
                    "upstream_prefix + local_prefix + local_index exceeds total_extranonce_len"
                )
            }
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
        // local_prefix(2) + local_index(2) = 4, rollable = 16
        let mut alloc = ExtranonceAllocator::new(vec![0x00, 0x01], 20, 65_536).unwrap();

        assert_eq!(alloc.total_extranonce_len(), 20);
        assert_eq!(alloc.upstream_prefix_len(), 0);
        assert_eq!(alloc.local_prefix(), &[0x00, 0x01]);
        assert_eq!(alloc.local_prefix_len(), 2);
        assert_eq!(alloc.local_index_len(), 2);
        assert_eq!(alloc.full_prefix_len(), 4);
        assert_eq!(alloc.rollable_extranonce_size(), 16);
        assert_eq!(alloc.max_channels(), 65_536);
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
    fn proxy_no_local_prefix() {
        // upstream(4) + local_prefix(0) + local_index(2) = 6, rollable = 14
        let upstream_prefix = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let mut alloc = ExtranonceAllocator::from_upstream_prefix(
            upstream_prefix.clone(),
            Vec::new(),
            20,
            65_536,
        )
        .unwrap();

        assert_eq!(alloc.upstream_prefix(), &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(alloc.upstream_prefix_len(), 4);
        assert_eq!(alloc.local_prefix(), &[] as &[u8]);
        assert_eq!(alloc.local_prefix_len(), 0);
        assert_eq!(alloc.local_index_len(), 2);
        assert_eq!(alloc.full_prefix_len(), 6);
        assert_eq!(alloc.rollable_extranonce_size(), 14);

        let ext = alloc.allocate_extended(14).unwrap();
        assert_eq!(ext.as_bytes().len(), 6);
        assert_eq!(&ext.as_bytes()[0..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn proxy_with_local_prefix_padding() {
        // Translator-style: upstream grants 16 bytes of rollable, but the
        // downstream miner only wants 4 rollable. Use `local_prefix_bytes`
        // to absorb the slack (12 − local_index bytes) into caller-owned
        // static padding.
        //
        // total=20, upstream=4, local_index=1 (max_channels=256), rollable=4
        // => local_prefix must be 20 − 4 − 1 − 4 = 11
        let upstream_prefix = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let local_prefix_bytes = vec![0x42; 11];
        let mut alloc = ExtranonceAllocator::from_upstream_prefix(
            upstream_prefix.clone(),
            local_prefix_bytes.clone(),
            20,
            256,
        )
        .unwrap();

        assert_eq!(alloc.upstream_prefix_len(), 4);
        assert_eq!(alloc.local_prefix_len(), 11);
        assert_eq!(alloc.local_index_len(), 1);
        assert_eq!(alloc.full_prefix_len(), 16);
        assert_eq!(alloc.rollable_extranonce_size(), 4);

        let ext = alloc.allocate_extended(4).unwrap();
        // Prefix bytes must be [upstream | local_prefix | local_index].
        assert_eq!(ext.as_bytes().len(), 16);
        assert_eq!(&ext.as_bytes()[0..4], upstream_prefix.as_slice());
        assert_eq!(&ext.as_bytes()[4..15], local_prefix_bytes.as_slice());
        // local_index is 1 byte; first allocation index is 0.
        assert_eq!(ext.as_bytes()[15], 0);
    }

    #[test]
    fn uniqueness() {
        // local_prefix(0) + local_index(1) = 1, rollable = 19
        let mut alloc = ExtranonceAllocator::new(Vec::new(), 20, 256).unwrap();
        let mut seen = HashSet::new();
        // Keep prefixes alive so they don't auto-release via Drop mid-test.
        let mut prefixes = Vec::with_capacity(256);

        for _ in 0..256 {
            let p = alloc.allocate_extended(19).unwrap();
            assert!(seen.insert(p.as_bytes().to_vec()), "duplicate prefix");
            prefixes.push(p);
        }

        assert_eq!(alloc.allocated_count(), 256);
        drop(prefixes);
        assert_eq!(alloc.allocated_count(), 0);
    }

    #[test]
    fn exhaustion_and_reuse() {
        // local_index(1), rollable = 5
        let mut alloc = ExtranonceAllocator::new(Vec::new(), 6, 256).unwrap();

        let mut prefixes = Vec::new();
        for _ in 0..256 {
            prefixes.push(alloc.allocate_extended(5).unwrap());
        }

        assert!(alloc.allocate_extended(5).is_err());
        assert_eq!(alloc.allocated_count(), 256);

        let freed_bytes = prefixes[42].as_bytes().to_vec();
        // Remove from the Vec and drop the prefix -> slot freed by Drop.
        drop(prefixes.remove(42));
        assert_eq!(alloc.allocated_count(), 255);

        let reused = alloc.allocate_extended(5).unwrap();
        assert_eq!(reused.as_bytes(), freed_bytes.as_slice());
        assert_eq!(alloc.allocated_count(), 256);

        drop(prefixes);
        drop(reused);
        assert_eq!(alloc.allocated_count(), 0);
    }

    #[test]
    fn drop_frees_and_allows_reuse() {
        // local_index(1), rollable = 5 (max_channels = 4 -> 1 byte)
        let mut alloc = ExtranonceAllocator::new(Vec::new(), 6, 4).unwrap();

        let p0 = alloc.allocate_extended(5).unwrap();
        let p1 = alloc.allocate_extended(5).unwrap();
        let p2 = alloc.allocate_extended(5).unwrap();
        let p3 = alloc.allocate_extended(5).unwrap();
        assert_eq!(alloc.allocated_count(), 4);
        assert!(alloc.allocate_extended(5).is_err());

        let freed_bytes = p1.as_bytes().to_vec();
        drop(p1);
        assert_eq!(alloc.allocated_count(), 3);

        let reused = alloc.allocate_extended(5).unwrap();
        assert_eq!(reused.as_bytes(), freed_bytes.as_slice());

        drop(p0);
        drop(p2);
        drop(p3);
        drop(reused);
        assert_eq!(alloc.allocated_count(), 0);
    }

    #[test]
    fn drop_reuses_first_available_index() {
        let mut alloc = ExtranonceAllocator::new(Vec::new(), 6, 256).unwrap();

        let mut prefixes = Vec::new();
        for _ in 0..256 {
            prefixes.push(alloc.allocate_extended(5).unwrap());
        }

        let freed_bytes = prefixes[10].as_bytes().to_vec();

        // `remove` shifts indices, so pull the higher one first.
        drop(prefixes.remove(20));
        drop(prefixes.remove(10));

        // First-fit reuse: the lower freed slot is reused first.
        let reused = alloc.allocate_extended(5).unwrap();
        assert_eq!(reused.as_bytes(), freed_bytes.as_slice());

        drop(prefixes);
        drop(reused);
        assert_eq!(alloc.allocated_count(), 0);
    }

    #[test]
    fn prefix_drop_frees_slot_on_scope_exit() {
        // Verify that a prefix dropping at end-of-scope (without an explicit
        // `drop` call) still releases its slot.
        let mut alloc = ExtranonceAllocator::new(Vec::new(), 6, 4).unwrap();
        {
            let _p = alloc.allocate_extended(5).unwrap();
            assert_eq!(alloc.allocated_count(), 1);
        }
        assert_eq!(alloc.allocated_count(), 0);
    }

    #[test]
    fn prefix_outliving_allocator_is_safe() {
        // If the allocator is dropped first, the prefix's Drop upgrades a
        // dead `Weak` and is a silent no-op. This test exists primarily to
        // demonstrate that the pattern does not panic or UB.
        let alloc = ExtranonceAllocator::new(Vec::new(), 6, 4)
            .unwrap()
            .allocate_extended(5)
            .unwrap();
        // `alloc` here is actually the `AllocatedExtranoncePrefix` — the
        // allocator was a temporary and has been dropped already. Dropping
        // `alloc` (the prefix) should be a no-op.
        drop(alloc);
    }

    #[test]
    fn standard_prefix_includes_rollable_zeros() {
        // local_prefix(1) + local_index(1) = 2, rollable = 18 (zero-padded)
        let mut alloc = ExtranonceAllocator::new(vec![0x01], 20, 256).unwrap();

        let std = alloc.allocate_standard().unwrap();
        assert_eq!(std.as_bytes().len(), 20);
        assert!(std.as_bytes()[2..].iter().all(|&b| b == 0));
    }

    #[test]
    fn invalid_rollable_size() {
        // local_index(1), rollable = 19
        let mut alloc = ExtranonceAllocator::new(Vec::new(), 20, 256).unwrap();
        let err = alloc.allocate_extended(20).unwrap_err();
        assert_eq!(err, ExtranonceAllocatorError::InvalidRollableSize);
    }

    #[test]
    fn validation_errors() {
        assert_eq!(
            ExtranonceAllocator::new(Vec::new(), 33, 256).unwrap_err(),
            ExtranonceAllocatorError::ExceedsMaxLength
        );

        assert_eq!(
            ExtranonceAllocator::new(Vec::new(), 20, 0).unwrap_err(),
            ExtranonceAllocatorError::ZeroMaxChannels
        );

        // local_prefix(2) + local_index(1) = 3, but total = 2
        assert_eq!(
            ExtranonceAllocator::new(vec![0x01, 0x02], 2, 256).unwrap_err(),
            ExtranonceAllocatorError::PrefixExceedsTotalLength
        );

        // upstream(20) + local_index(1) = 21 > total(20)
        assert_eq!(
            ExtranonceAllocator::from_upstream_prefix(vec![0; 20], Vec::new(), 20, 256)
                .unwrap_err(),
            ExtranonceAllocatorError::PrefixExceedsTotalLength
        );

        // upstream(4) + local_prefix(16) + local_index(1) = 21 > total(20)
        assert_eq!(
            ExtranonceAllocator::from_upstream_prefix(vec![0; 4], vec![0; 16], 20, 256)
                .unwrap_err(),
            ExtranonceAllocatorError::PrefixExceedsTotalLength
        );
    }

    #[test]
    fn no_overlap_standard_and_extended() {
        // local_index(1), rollable = 7
        let mut alloc = ExtranonceAllocator::new(Vec::new(), 8, 256).unwrap();

        let ext = alloc.allocate_extended(7).unwrap();
        let std = alloc.allocate_standard().unwrap();

        assert_ne!(&ext.as_bytes()[..1], &std.as_bytes()[..1]);
    }
}
