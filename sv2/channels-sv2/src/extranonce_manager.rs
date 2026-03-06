//! Extranonce prefix allocation for downstream SV2 channels.
//!
//! [`ExtranonceAllocator`] manages unique extranonce prefixes for both standard and
//! extended mining channels. It replaces the previous `ExtendedExtranonce` factory
//! from `mining_sv2`, adding support for prefix reuse when channels close.
//!
//! # Extranonce layout
//!
//! | upstream_prefix | server_id | local_prefix_id | rollable_extranonce_size |
//! |:---------------:|:---------:|:---------------:|:-----------------------:|
//! | (fixed) | (optional static) | (dynamic, unique per channel) | (for rolling or downstream allocation) |
//!
//! - **upstream_prefix**: Fixed bytes assigned by the upstream node (empty for a Pool server).
//! - **local_prefix**: Per-channel bytes controlled by *this* node, composed of:
//!   - **server_id** (optional): Static identifier for this server instance.
//!   - **local_prefix_id**: Dynamic bytes tracked by a bit vector — each channel gets a unique value.
//! - **rollable_extranonce_size**: Bytes left for the downstream miner to roll (extended channels), or do further nested allocation layers.
//!   Standard channels zero-pad this portion in the prefix since they don't roll.
//!
//! # Sizing `local_prefix_len` and `max_channels`
//!
//! `local_prefix_len` controls how many bytes are reserved in the extranonce for
//! this node's per-channel identification. It is split into `server_id` (optional,
//! static) and `local_prefix_id` (dynamic, one unique value per channel):
//!
//! ```text
//! local_prefix_id_len = local_prefix_len - server_id.len()
//! ```
//!
//! `max_channels` can be **at most** `2^(local_prefix_id_len * 8)`, but it can also
//! be smaller. When it is smaller, the unused high bytes of `local_prefix_id` will
//! always be zero — effectively wasting extranonce space that could have gone to
//! `rollable_extranonce_size`.
//!
//! **Rule of thumb:** match `local_prefix_id_len` to `max_channels` so that no
//! bytes are permanently zero.
//!
//! | `local_prefix_id_len` | Max addressable   | Bitmap memory |
//! |-----------------------|-------------------|---------------|
//! | 1 byte                | 256               | 32 B          |
//! | 2 bytes               | 65,536            | 8 KB          |
//! | 3 bytes               | 16,777,216        | 2 MB          |
//! | 4 bytes               | 4,294,967,296     | 512 MB        |
//!
//! **Example — Pool with `server_id`:**
//!
//! ```text
//! local_prefix_len = 4, server_id = 2 bytes  →  local_prefix_id_len = 2
//! max_channels = 65,536  →  uses full 2-byte range, no wasted bytes ✓
//! ```
//!
//! **Counter-example — no `server_id`, oversized `local_prefix_len`:**
//!
//! ```text
//! local_prefix_len = 4, server_id = none  →  local_prefix_id_len = 4
//! max_channels = 65,536  →  ids encoded in 4 bytes, top 2 always zero ✗
//! Better: local_prefix_len = 2, giving 2 extra bytes to rollable_extranonce_size
//! ```
//!
//! # Usage
//!
//! ## Pool (root node, no upstream)
//!
//! A pool creates the allocator with [`ExtranonceAllocator::new`], providing an optional
//! `server_id` to distinguish multiple pool server instances. It then calls
//! [`allocate_standard`](ExtranonceAllocator::allocate_standard) or
//! [`allocate_extended`](ExtranonceAllocator::allocate_extended) each time a downstream
//! opens a channel.
//!
//! ```
//! use channels_sv2::extranonce_manager::ExtranonceAllocator;
//!
//! // Pool: 20-byte extranonce, 4-byte local prefix (2 for server_id, 2 for channel id),
//! // leaving 16 bytes for downstream rolling.
//! let mut allocator = ExtranonceAllocator::new(
//!     20,                         // total_extranonce_len
//!     4,                          // local_prefix_len
//!     Some(vec![0x00, 0x01]),     // server_id (2 bytes)
//!     65_536,                     // max_channels (bitmap: 65_536 / 8 = 8 KB)
//! ).unwrap();
//!
//! // When a downstream opens a standard channel:
//! let prefix = allocator.allocate_standard().unwrap();
//! let prefix_bytes = prefix.as_bytes(); // pass to StandardChannel constructor
//! let id = prefix.local_prefix_id();    // store for later freeing
//!
//! // When the channel closes:
//! allocator.free(id);
//! ```
//!
//! ## JDC/ Translator / Proxies (receives upstream extranonce prefix)
//!
//! Proxies (JDC and Translator included) receive an `extranonce_prefix` from their upstream node
//! (via `OpenExtendedMiningChannelSuccess` or `SetExtranoncePrefix`). They create
//! the allocator with [`ExtranonceAllocator::from_upstream`] and then subdivide
//! the remaining space for their own downstream channels.
//!
//! ```
//! use channels_sv2::extranonce_manager::ExtranonceAllocator;
//!
//! // Upstream assigned prefix [0xAA, 0xBB, 0xCC, 0xDD] with total extranonce of 20 bytes.
//! let upstream_prefix = vec![0xAA, 0xBB, 0xCC, 0xDD];
//! let mut allocator = ExtranonceAllocator::from_upstream(
//!     upstream_prefix,
//!     4,      // local_prefix_len (bytes this node reserves for channel identification)
//!     20,     // total_extranonce_len
//!     65_536, // max_channels (bitmap: 65_536 / 8 = 8 KB)
//! ).unwrap();
//!
//! // When a downstream opens an extended channel with min_extranonce_size from the request:
//! let prefix = allocator.allocate_extended(12).unwrap();
//! let prefix_bytes = prefix.as_bytes(); // pass to ExtendedChannel constructor
//! ```

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

/// Maximum extranonce length in bytes (Sv2 spec).
pub const MAX_EXTRANONCE_LEN: usize = 32;

// ---------------------------------------------------------------------------
// ExtranonceAllocator
// ---------------------------------------------------------------------------

/// Manages extranonce prefix allocation for downstream channels.
///
/// A single allocator handles both standard and extended channels. Each call to
/// [`allocate_standard`](Self::allocate_standard) or [`allocate_extended`](Self::allocate_extended)
/// returns a unique [`ExtranoncePrefix`] whose bytes can be passed directly to
/// channel constructors. Because both channel types draw from the same
/// `local_prefix_id` pool (a single shared bitmap), their extranonces are
/// guaranteed never to overlap. When a channel closes, call [`free`](Self::free)
/// with the prefix's [`local_prefix_id`](ExtranoncePrefix::local_prefix_id) to
/// make it available for reuse.
///
/// # Memory
///
/// The allocator uses an internal bitmap of `max_channels` bits (`max_channels / 8`
/// bytes). See the module-level
/// [Sizing `local_prefix_len` and `max_channels`](index.html#sizing-local_prefix_len-and-max_channels)
/// section for a reference table.
pub struct ExtranonceAllocator {
    upstream_prefix: Vec<u8>,
    server_id: Vec<u8>,
    local_prefix_id_len: usize,
    local_prefix_len: usize,
    rollable_extranonce_size: usize,
    total_extranonce_len: usize,
    allocation_bitmap: BitVector,
    max_channels: usize,
    last_allocated_id: Option<usize>,
    last_freed_id: Option<usize>,
}

impl core::fmt::Debug for ExtranonceAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExtranonceAllocator")
            .field("upstream_prefix_len", &self.upstream_prefix.len())
            .field("local_prefix_len", &self.local_prefix_len)
            .field("rollable_extranonce_size", &self.rollable_extranonce_size)
            .field("total_extranonce_len", &self.total_extranonce_len)
            .field("max_channels", &self.max_channels)
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
    /// - `local_prefix_len`: Bytes reserved for this node's per-channel prefix.
    /// - `server_id`: Optional static server identifier (placed at the start of local_prefix).
    /// - `max_channels`: Maximum concurrent channels. Determines the size of the
    ///   internal allocation bitmap (`max_channels / 8` bytes). See the
    ///   [Memory](#memory) section on [`ExtranonceAllocator`] for a reference table.
    pub fn new(
        total_extranonce_len: usize,
        local_prefix_len: usize,
        server_id: Option<Vec<u8>>,
        max_channels: usize,
    ) -> Result<Self, ExtranonceAllocatorError> {
        let server_id = server_id.unwrap_or_default();
        Self::validate_and_create(
            Vec::new(),
            server_id,
            local_prefix_len,
            total_extranonce_len,
            max_channels,
        )
    }

    /// Create an allocator seeded with an upstream-assigned prefix (for proxies,
    /// JD clients, and translators).
    ///
    /// Use this when the node receives an `extranonce_prefix` from upstream via
    /// `OpenExtendedMiningChannelSuccess` or `SetExtranoncePrefix`, and needs to
    /// subdivide the remaining space for its own downstream channels.
    ///
    /// - `upstream_prefix`: The prefix bytes received from the upstream node.
    /// - `local_prefix_len`: Bytes this node reserves for per-channel identification.
    /// - `total_extranonce_len`: Total extranonce length.
    /// - `max_channels`: Maximum concurrent channels. Determines the size of the
    ///   internal allocation bitmap (`max_channels / 8` bytes). See the
    ///   [Memory](#memory) section on [`ExtranonceAllocator`] for a reference table.
    pub fn from_upstream(
        upstream_prefix: Vec<u8>,
        local_prefix_len: usize,
        total_extranonce_len: usize,
        max_channels: usize,
    ) -> Result<Self, ExtranonceAllocatorError> {
        Self::validate_and_create(
            upstream_prefix,
            Vec::new(),
            local_prefix_len,
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
        if min_rollable_size > self.rollable_extranonce_size {
            return Err(ExtranonceAllocatorError::InvalidRollableSize);
        }
        let id = self
            .find_free_id()
            .ok_or(ExtranonceAllocatorError::CapacityExhausted)?;
        self.mark_allocated(id);
        Ok(ExtranoncePrefix::new(id, self.build_extended_prefix(id)))
    }

    /// Allocate a prefix for a standard channel.
    ///
    /// Returns a prefix of length `total_extranonce_len` (the full extranonce).
    /// The `rollable_extranonce_size` portion is zero-padded since standard channels
    /// don't roll.
    pub fn allocate_standard(&mut self) -> Result<ExtranoncePrefix, ExtranonceAllocatorError> {
        let id = self
            .find_free_id()
            .ok_or(ExtranonceAllocatorError::CapacityExhausted)?;
        self.mark_allocated(id);
        Ok(ExtranoncePrefix::new(id, self.build_standard_prefix(id)))
    }

    /// Free a previously allocated local prefix id, making it available for reuse.
    pub fn free(&mut self, local_prefix_id: usize) {
        if local_prefix_id < self.max_channels {
            self.allocation_bitmap.set(local_prefix_id, false);
            self.last_freed_id = Some(local_prefix_id);
        }
    }

    /// Bytes available for downstream rolling (extended channels).
    pub fn rollable_extranonce_size(&self) -> usize {
        self.rollable_extranonce_size
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

    /// Length of the local prefix portion (server_id + local_prefix_id).
    pub fn local_prefix_len(&self) -> usize {
        self.local_prefix_len
    }

    /// Number of currently allocated channels.
    pub fn allocated_count(&self) -> usize {
        self.allocation_bitmap.count_ones()
    }

    /// Maximum number of concurrent channels.
    pub fn max_channels(&self) -> usize {
        self.max_channels
    }

    /// Validate configuration and create the allocator. Shared by `new()` and `from_upstream()`.
    fn validate_and_create(
        upstream_prefix: Vec<u8>,
        server_id: Vec<u8>,
        local_prefix_len: usize,
        total_extranonce_len: usize,
        max_channels: usize,
    ) -> Result<Self, ExtranonceAllocatorError> {
        if total_extranonce_len > MAX_EXTRANONCE_LEN {
            return Err(ExtranonceAllocatorError::ExceedsMaxLength);
        }
        if upstream_prefix.len() + local_prefix_len > total_extranonce_len {
            return Err(ExtranonceAllocatorError::InvalidLengths);
        }
        if max_channels == 0 {
            return Err(ExtranonceAllocatorError::InvalidLengths);
        }
        if server_id.len() >= local_prefix_len {
            return Err(ExtranonceAllocatorError::ServerIdTooLong);
        }

        let local_prefix_id_len = local_prefix_len - server_id.len();
        // Each `local_prefix_id` is encoded as `local_prefix_id_len` bytes, so the
        // largest representable id is `2^(local_prefix_id_len * 8) - 1`. If that
        // already saturates `usize` we skip the shift to avoid overflow.
        let max_addressable = if local_prefix_id_len >= core::mem::size_of::<usize>() {
            usize::MAX
        } else {
            1usize << (local_prefix_id_len * 8)
        };
        if max_channels > max_addressable {
            return Err(ExtranonceAllocatorError::MaxChannelsTooLarge);
        }

        let rollable_extranonce_size =
            total_extranonce_len - upstream_prefix.len() - local_prefix_len;
        let allocation_bitmap = BitVector::new(max_channels);

        Ok(Self {
            upstream_prefix,
            server_id,
            local_prefix_id_len,
            local_prefix_len,
            rollable_extranonce_size,
            total_extranonce_len,
            allocation_bitmap,
            max_channels,
            last_allocated_id: None,
            last_freed_id: None,
        })
    }

    /// Mark a local_prefix_id as in-use and record it as the latest allocation.
    fn mark_allocated(&mut self, id: usize) {
        self.allocation_bitmap.set(id, true);
        self.last_allocated_id = Some(id);
    }

    /// Find the next available local_prefix_id.
    ///
    /// Scans forward from the last allocation point, wrapping around.
    /// Prefers any free id over `last_freed_id` to avoid immediately reusing
    /// a prefix whose previous channel may still have in-flight shares.
    /// Falls back to `last_freed_id` if it is the only one available.
    fn find_free_id(&self) -> Option<usize> {
        let start = self
            .last_allocated_id
            .map_or(0, |id| (id + 1) % self.max_channels);

        // Try to find a free id that isn't the one we just freed.
        if let Some(id) = self.first_free_id_wrapping(start, self.last_freed_id) {
            return Some(id);
        }

        // Fall back to last_freed_id if it's the only one left.
        self.last_freed_id
            .filter(|&id| id < self.max_channels && !self.allocation_bitmap.get(id))
    }

    /// Scan the bitmap starting at `from`, wrapping around to cover all ids.
    /// Skips `skip` (if provided) so the caller can avoid a specific id.
    fn first_free_id_wrapping(&self, from: usize, skip: Option<usize>) -> Option<usize> {
        self.first_free_id_in_range(from, self.max_channels, skip)
            .or_else(|| {
                if from > 0 {
                    self.first_free_id_in_range(0, from, skip)
                } else {
                    None
                }
            })
    }

    /// Scan `[from, to)` for the first free id, skipping `skip` if provided.
    fn first_free_id_in_range(&self, from: usize, to: usize, skip: Option<usize>) -> Option<usize> {
        let mut cursor = from;
        while cursor < to {
            match self.allocation_bitmap.find_first_zero_in_range(cursor, to) {
                Some(id) if Some(id) == skip => cursor = id + 1,
                result => return result,
            }
        }
        None
    }

    /// Build the prefix bytes for an extended channel.
    ///
    /// Layout: `[upstream_prefix | server_id | encoded local_prefix_id]`
    fn build_extended_prefix(&self, local_prefix_id: usize) -> Vec<u8> {
        let mut prefix = Vec::with_capacity(self.upstream_prefix.len() + self.local_prefix_len);
        prefix.extend_from_slice(&self.upstream_prefix);
        prefix.extend_from_slice(&self.server_id);
        prefix.extend_from_slice(&Self::local_prefix_id_to_bytes(
            local_prefix_id,
            self.local_prefix_id_len,
        ));
        prefix
    }

    /// Build the prefix bytes for a standard channel.
    ///
    /// Same as the extended prefix, plus zero-padded rollable bytes at the end
    /// (standard channels don't roll, so the pool fills the full extranonce).
    fn build_standard_prefix(&self, local_prefix_id: usize) -> Vec<u8> {
        let mut prefix = self.build_extended_prefix(local_prefix_id);
        prefix.resize(self.total_extranonce_len, 0);
        prefix
    }

    /// Encode a `local_prefix_id` (usize) into `len` big-endian bytes.
    ///
    /// Example: `local_prefix_id_to_bytes(258, 2)` → `[0x01, 0x02]`
    fn local_prefix_id_to_bytes(local_prefix_id: usize, len: usize) -> Vec<u8> {
        let mut result = vec![0u8; len];
        let be_bytes = local_prefix_id.to_be_bytes();
        let copy_len = be_bytes.len().min(len);
        let src_start = be_bytes.len() - copy_len;
        let dst_start = len - copy_len;
        result[dst_start..].copy_from_slice(&be_bytes[src_start..]);
        result
    }
}

// ---------------------------------------------------------------------------
// ExtranoncePrefix
// ---------------------------------------------------------------------------

/// An allocated extranonce prefix returned by the allocator.
///
/// Stores both the raw prefix bytes (to pass to channel constructors) and the
/// `local_prefix_id` (to pass back to [`ExtranonceAllocator::free`] when the channel closes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtranoncePrefix {
    local_prefix_id: usize,
    prefix: Vec<u8>,
}

impl ExtranoncePrefix {
    fn new(local_prefix_id: usize, prefix: Vec<u8>) -> Self {
        Self {
            local_prefix_id,
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

    /// The assigned local prefix id — store this and pass to [`ExtranonceAllocator::free`]
    /// when the channel closes.
    pub fn local_prefix_id(&self) -> usize {
        self.local_prefix_id
    }
}

// ---------------------------------------------------------------------------
// ExtranonceAllocatorError
// ---------------------------------------------------------------------------

/// Errors returned by [`ExtranonceAllocator`] operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtranonceAllocatorError {
    /// `total_extranonce_len` exceeds [`MAX_EXTRANONCE_LEN`] (32).
    ExceedsMaxLength,
    /// The combination of upstream_prefix, local_prefix_len, and total_extranonce_len is invalid.
    InvalidLengths,
    /// `server_id` is too long — it must be strictly shorter than `local_prefix_len`.
    ServerIdTooLong,
    /// `max_channels` exceeds the range addressable by `local_prefix_id` bytes.
    MaxChannelsTooLarge,
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
            Self::ServerIdTooLong => {
                write!(
                    f,
                    "server_id must be strictly shorter than local_prefix_len"
                )
            }
            Self::MaxChannelsTooLarge => {
                write!(
                    f,
                    "max_channels exceeds addressable range of local_prefix_id bytes"
                )
            }
            Self::CapacityExhausted => write!(f, "all channels are allocated — no more capacity"),
            Self::InvalidRollableSize => {
                write!(f, "requested rollable size exceeds available space")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// BitVector (internal)
// ---------------------------------------------------------------------------

/// A compact bit vector that tracks which `local_prefix_id` values are in use.
///
/// Bits are packed into `u64` words — each word holds the allocation state for
/// 64 consecutive ids. A set bit means the id is allocated; a clear bit means
/// it is free.
///
/// ```text
///   words[0]               words[1]               words[2]  ...
///  ┌────────────────────┐ ┌────────────────────┐ ┌──────────────
///  │ ids 0..63          │ │ ids 64..127        │ │ ids 128..191
///  │ (64 bits per word) │ │                    │ │
///  └────────────────────┘ └────────────────────┘ └──────────────
/// ```
///
/// `capacity` is the actual number of valid ids (i.e. `max_channels`).
/// Because the storage is rounded up to the nearest multiple of 64, the
/// last word may contain trailing bits beyond `capacity` — these are
/// never set and are excluded from search results.
///
/// Word-level operations allow bulk checks:
/// - A word equal to `u64::MAX` means all 64 ids in that range are taken —
///   the scanner skips the entire word in one comparison.
/// - `trailing_zeros()` on the inverted word finds the first free id within
///   a word using a single CPU instruction (`TZCNT`/`BSF` on x86).
/// - `count_ones()` compiles to the hardware `POPCNT` instruction.
struct BitVector {
    /// Packed allocation bits. `words[i]` covers ids `i*64 .. (i+1)*64`.
    words: Vec<u64>,
    /// Number of valid ids tracked (= `max_channels`). May be less than
    /// `words.len() * 64` when `max_channels` is not a multiple of 64.
    capacity: usize,
}

impl BitVector {
    fn new(capacity: usize) -> Self {
        let num_words = capacity.div_ceil(64);
        Self {
            words: vec![0u64; num_words],
            capacity,
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.capacity
    }

    fn get(&self, index: usize) -> bool {
        debug_assert!(index < self.capacity);
        let word = index / 64;
        let bit = index % 64;
        (self.words[word] >> bit) & 1 == 1
    }

    fn set(&mut self, index: usize, value: bool) {
        debug_assert!(index < self.capacity);
        let word = index / 64;
        let bit = index % 64;
        if value {
            self.words[word] |= 1u64 << bit;
        } else {
            self.words[word] &= !(1u64 << bit);
        }
    }

    fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Find the first zero bit in `[from, to)`, skipping fully-set u64 words.
    /// Returns `None` if every bit in the range is set.
    fn find_first_zero_in_range(&self, from: usize, to: usize) -> Option<usize> {
        if from >= to {
            return None;
        }

        let mut idx = from;
        while idx < to {
            let word_idx = idx / 64;
            let word = self.words[word_idx];

            if word == u64::MAX {
                idx = (word_idx + 1) * 64;
                continue;
            }

            let bit_offset = idx % 64;
            let mask = !0u64 << bit_offset;
            let zeros_masked = !word & mask;

            if zeros_masked != 0 {
                let first_zero = word_idx * 64 + zeros_masked.trailing_zeros() as usize;
                if first_zero < to && first_zero < self.capacity {
                    return Some(first_zero);
                }
            }

            idx = (word_idx + 1) * 64;
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // -- BitVector unit tests -----------------------------------------------

    #[test]
    fn bitvector_basic() {
        let mut bv = BitVector::new(128);
        assert_eq!(bv.len(), 128);
        assert!(!bv.get(0));

        bv.set(0, true);
        assert!(bv.get(0));

        bv.set(0, false);
        assert!(!bv.get(0));
    }

    #[test]
    fn bitvector_count_ones() {
        let mut bv = BitVector::new(256);
        assert_eq!(bv.count_ones(), 0);

        for i in (0..256).step_by(2) {
            bv.set(i, true);
        }
        assert_eq!(bv.count_ones(), 128);
    }

    #[test]
    fn bitvector_find_zero_skips_full_words() {
        let mut bv = BitVector::new(192);
        for i in 0..128 {
            bv.set(i, true);
        }
        let found = bv.find_first_zero_in_range(0, 192);
        assert_eq!(found, Some(128));
    }

    #[test]
    fn bitvector_find_zero_partial_range() {
        let mut bv = BitVector::new(64);
        bv.set(3, true);
        bv.set(4, true);

        assert_eq!(bv.find_first_zero_in_range(3, 10), Some(5));
        assert_eq!(bv.find_first_zero_in_range(0, 3), Some(0));
    }

    #[test]
    fn bitvector_find_zero_all_set() {
        let mut bv = BitVector::new(64);
        for i in 0..64 {
            bv.set(i, true);
        }
        assert_eq!(bv.find_first_zero_in_range(0, 64), None);
    }

    // -- ExtranonceAllocator tests ------------------------------------------

    #[test]
    fn pool_basic_allocation() {
        let mut alloc = ExtranonceAllocator::new(20, 4, Some(vec![0x00, 0x01]), 65536).unwrap();

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
        let upstream_prefix = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let mut alloc =
            ExtranonceAllocator::from_upstream(upstream_prefix.clone(), 4, 20, 65536).unwrap();

        assert_eq!(alloc.upstream_prefix(), &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(alloc.upstream_prefix_len(), 4);
        assert_eq!(alloc.rollable_extranonce_size(), 12);

        let ext = alloc.allocate_extended(12).unwrap();
        assert_eq!(ext.as_bytes().len(), 8);
        assert_eq!(&ext.as_bytes()[0..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn uniqueness() {
        let mut alloc = ExtranonceAllocator::new(20, 4, None, 256).unwrap();
        let mut seen = HashSet::new();

        for _ in 0..256 {
            let p = alloc.allocate_extended(16).unwrap();
            assert!(seen.insert(p.as_bytes().to_vec()), "duplicate prefix");
        }

        assert_eq!(alloc.allocated_count(), 256);
    }

    #[test]
    fn exhaustion_and_reuse() {
        let mut alloc = ExtranonceAllocator::new(6, 2, None, 256).unwrap();

        let mut ids = Vec::new();
        for _ in 0..256 {
            let p = alloc.allocate_extended(4).unwrap();
            ids.push(p.local_prefix_id());
        }

        assert!(alloc.allocate_extended(4).is_err());
        assert_eq!(alloc.allocated_count(), 256);

        alloc.free(ids[42]);
        assert_eq!(alloc.allocated_count(), 255);

        let reused = alloc.allocate_extended(4).unwrap();
        assert_eq!(reused.local_prefix_id(), ids[42]);
        assert_eq!(alloc.allocated_count(), 256);
    }

    #[test]
    fn free_and_skip_last_freed() {
        let mut alloc = ExtranonceAllocator::new(6, 2, None, 256).unwrap();

        let mut ids = Vec::new();
        for _ in 0..256 {
            ids.push(alloc.allocate_extended(4).unwrap().local_prefix_id());
        }

        let id_a = ids[10];
        let id_b = ids[11];
        alloc.free(id_a);
        alloc.free(id_b);

        let reused = alloc.allocate_extended(4).unwrap();
        assert_ne!(
            reused.local_prefix_id(),
            id_b,
            "should skip the last-freed id"
        );
        assert_eq!(reused.local_prefix_id(), id_a);
    }

    #[test]
    fn standard_prefix_includes_rollable_zeros() {
        let mut alloc = ExtranonceAllocator::new(20, 4, Some(vec![0x01]), 256).unwrap();

        let std = alloc.allocate_standard().unwrap();
        assert_eq!(std.as_bytes().len(), 20);
        assert!(std.as_bytes()[4..].iter().all(|&b| b == 0));
    }

    #[test]
    fn invalid_rollable_size() {
        let mut alloc = ExtranonceAllocator::new(20, 4, None, 256).unwrap();
        let err = alloc.allocate_extended(17);
        assert_eq!(err, Err(ExtranonceAllocatorError::InvalidRollableSize));
    }

    #[test]
    fn validation_errors() {
        assert_eq!(
            ExtranonceAllocator::new(33, 4, None, 256).unwrap_err(),
            ExtranonceAllocatorError::ExceedsMaxLength
        );
        assert_eq!(
            ExtranonceAllocator::new(20, 2, Some(vec![1, 2]), 256).unwrap_err(),
            ExtranonceAllocatorError::ServerIdTooLong
        );
        assert_eq!(
            ExtranonceAllocator::new(20, 2, Some(vec![1]), 257).unwrap_err(),
            ExtranonceAllocatorError::MaxChannelsTooLarge
        );
        assert_eq!(
            ExtranonceAllocator::new(4, 5, None, 256).unwrap_err(),
            ExtranonceAllocatorError::InvalidLengths
        );
        assert_eq!(
            ExtranonceAllocator::new(20, 4, None, 0).unwrap_err(),
            ExtranonceAllocatorError::InvalidLengths
        );
    }

    #[test]
    fn no_overlap_standard_and_extended() {
        let mut alloc = ExtranonceAllocator::new(8, 2, None, 256).unwrap();

        let ext = alloc.allocate_extended(6).unwrap();
        let std = alloc.allocate_standard().unwrap();

        assert_ne!(ext.local_prefix_id(), std.local_prefix_id());
        assert_ne!(&ext.as_bytes()[..2], &std.as_bytes()[..2]);
    }
}
