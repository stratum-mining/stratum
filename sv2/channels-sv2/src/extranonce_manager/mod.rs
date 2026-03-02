//! Extranonce prefix allocation for downstream SV2 channels.
//!
//! [`ExtranonceAllocator`] manages unique extranonce prefixes for both
//! standard and extended mining channels.
//!
//! # Extranonce layout
//!
//! | upstream_prefix | local_prefix | local_index | rollable |
//! |:---------------:|:------------:|:-----------:|:--------:|
//! | upstream node | caller (this node) | allocator | miner |
//!
//! - **upstream_prefix**: Bytes assigned by an upstream node. Empty for a
//!   Pool / root allocator built with [`ExtranonceAllocator::new`].
//! - **local_prefix**: Caller-owned static bytes placed inside the
//!   allocator's region. Typically used as a server/node identifier
//!   (pool case) or as padding to shrink `rollable` to a specific target
//!   size, or for embedding a tag/identifier. Can be empty.
//! - **local_index**: Per-channel dynamic bytes managed by the allocator —
//!   each channel gets a unique value. The byte width is derived
//!   automatically from `max_channels` via [`bytes_needed`].
//! - **rollable**: Remaining space for downstream rolling (extended
//!   channels) or further allocation layers. Standard channels zero-pad
//!   this portion.
//!
//! Both constructors, [`ExtranonceAllocator::new`] and
//! [`ExtranonceAllocator::from_upstream_prefix`], take `local_prefix_bytes`, so the
//! four-region layout is symmetric across pool and proxy/client cases.
//!
//! # Memory
//!
//! The allocator uses an internal bitmap of `max_channels` bits. Reference:
//!
//! | `max_channels` | `local_index` bytes | Bitmap memory |
//! |----------------|---------------------|---------------|
//! | 256            | 1                   | 32 B          |
//! | 65,536         | 2                   | 8 KB          |
//! | 16,777,216     | 3                   | 2 MB          |
//!
//! # Lifecycle
//!
//! Each call to [`ExtranonceAllocator::allocate_standard`] or
//! [`ExtranonceAllocator::allocate_extended`] returns an owning
//! [`AllocatedExtranoncePrefix`] — a type-level guarantee that the prefix
//! holds a reservation in the allocator's bitmap.
//!
//! **The allocation is released automatically when the
//! [`AllocatedExtranoncePrefix`] is dropped.** Server-side channel
//! constructors (e.g.
//! [`server::extended::ExtendedChannel::new_for_pool`](crate::server::extended::ExtendedChannel::new_for_pool))
//! take [`AllocatedExtranoncePrefix`] directly, so the allocator's
//! accounting always reflects the set of live server channels. Client-side
//! constructors take the wider [`ExtranoncePrefix`] (which an
//! [`AllocatedExtranoncePrefix`] converts into via [`Into`]), since
//! client-held prefixes legitimately come from both wire and allocator
//! sources. There is no manual release API — ownership enforces the
//! lifecycle.
//!
//! Internally this is implemented by having the allocator hold its bitmap
//! in an `Arc` and giving each outstanding prefix a `Weak` reference to it.
//! The prefix's `Drop` upgrades the `Weak`, clears its bit via an atomic
//! operation, and returns. If the allocator has already been dropped, the
//! upgrade fails and `Drop` becomes a silent no-op (safe: nothing to update).
//!
//! # Usage
//!
//! ## Pool (root node, no upstream)
//!
//! A pool creates the allocator with [`ExtranonceAllocator::new`], optionally
//! providing a `local_prefix_bytes` server identifier. It then calls
//! [`allocate_standard`](ExtranonceAllocator::allocate_standard) or
//! [`allocate_extended`](ExtranonceAllocator::allocate_extended) each time a
//! downstream opens a channel.
//!
//! ```
//! use channels_sv2::extranonce_manager::ExtranonceAllocator;
//!
//! // Pool: 20-byte extranonce, 2-byte server identifier, up to 65 536 channels.
//! // Layout: upstream(0) + local_prefix(2) + local_index(2) = 4;
//! // rollable = 20 − 4 = 16.
//! let mut allocator = ExtranonceAllocator::new(
//!     vec![0x00, 0x01],           // local_prefix_bytes (server identifier)
//!     20,                         // total_extranonce_len
//!     65_536,                     // max_channels
//! ).unwrap();
//!
//! let prefix = allocator.allocate_standard().unwrap();
//! // Pass `prefix` directly to the channel constructor — the channel
//! // takes ownership, and the allocation is released automatically when
//! // the channel (and thus the prefix) is dropped.
//! ```
//!
//! ## JDC / Translator / Proxies (receives upstream extranonce prefix)
//!
//! Proxies (JDC and Translator included) receive an `extranonce_prefix` from
//! their upstream node (via `OpenExtendedMiningChannelSuccess` or
//! `SetExtranoncePrefix`). They create the allocator with
//! [`ExtranonceAllocator::from_upstream_prefix`] and then subdivide the remaining
//! space for their own downstream channels. `local_prefix_bytes` can be used
//! to embed a tag or to absorb slack so `rollable` matches a specific target
//! size.
//!
//! ```
//! use channels_sv2::extranonce_manager::ExtranonceAllocator;
//!
//! let upstream_prefix = vec![0xAA, 0xBB, 0xCC, 0xDD];
//! let mut allocator = ExtranonceAllocator::from_upstream_prefix(
//!     upstream_prefix,
//!     Vec::new(), // local_prefix_bytes: none in this example
//!     20,         // total_extranonce_len
//!     65_536,     // max_channels
//! ).unwrap();
//!
//! // Layout: upstream(4) + local_prefix(0) + local_index(2) = 6;
//! // rollable = 20 − 6 = 14.
//! let prefix = allocator.allocate_extended(14).unwrap();
//! ```
//!
//! ## Translator: pinning `rollable` to a downstream-chosen size
//!
//! When a translator wants to grant its SV1 miner exactly `N` rollable bytes
//! regardless of how much space upstream left, it passes the slack as
//! `local_prefix_bytes`:
//!
//! ```
//! use channels_sv2::extranonce_manager::ExtranonceAllocator;
//!
//! let upstream_prefix = vec![0xAA, 0xBB, 0xCC, 0xDD];
//! let downstream_rollable: u8 = 4;
//! let total: u8 = 20;
//! let max_channels: u32 = 256;
//!
//! // upstream(4) + local_index(1) + rollable(4) = 9; slack = 11.
//! let local_prefix_bytes = vec![0x42; 11];
//! let mut allocator = ExtranonceAllocator::from_upstream_prefix(
//!     upstream_prefix,
//!     local_prefix_bytes,
//!     total,
//!     max_channels,
//! ).unwrap();
//!
//! assert_eq!(allocator.rollable_extranonce_size(), downstream_rollable);
//! ```

pub mod allocator;
mod bitvector;
pub mod prefix;

pub use allocator::{ExtranonceAllocator, ExtranonceAllocatorError};
pub use prefix::{AllocatedExtranoncePrefix, ExtranoncePrefix, ExtranoncePrefixError};

/// Maximum extranonce length in bytes (Sv2 spec).
pub const MAX_EXTRANONCE_LEN: u8 = 32;

/// Minimum bytes needed to represent `n` distinct values (`0..n`).
///
/// Always returns at least 1. Exposed so consumers can derive byte counts
/// from their `max_channels` value without hardcoding (and risking drift
/// from what [`ExtranonceAllocator`] uses internally).
pub const fn bytes_needed(n: u32) -> u8 {
    if n <= 1 {
        return 1;
    }
    let bits = u32::BITS - (n - 1).leading_zeros();
    bits.div_ceil(8) as u8
}

#[cfg(test)]
mod tests {
    use super::bytes_needed;

    #[test]
    fn bytes_needed_values() {
        assert_eq!(bytes_needed(1), 1);
        assert_eq!(bytes_needed(2), 1);
        assert_eq!(bytes_needed(255), 1);
        assert_eq!(bytes_needed(256), 1);
        assert_eq!(bytes_needed(257), 2);
        assert_eq!(bytes_needed(65_536), 2);
        assert_eq!(bytes_needed(65_537), 3);
        assert_eq!(bytes_needed(16_777_216), 3);
        assert_eq!(bytes_needed(16_777_217), 4);
    }
}
