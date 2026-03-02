//! Extranonce prefix allocation for downstream SV2 channels.
//!
//! [`ExtranonceAllocator`] manages unique extranonce prefixes for both standard and
//! extended mining channels.
//!
//! # Extranonce layout
//!
//! | upstream_prefix | server_bytes | local_index_bytes | rollable bytes |
//! |:---------------:|:------------:|:-----------------:|:--------------:|
//! | (fixed) | (optional) | (per-channel) | (rolling / downstream) |
//!
//! - **upstream_prefix**: Fixed bytes assigned by the upstream node (empty for a Pool).
//! - **local_prefix**: Per-channel bytes controlled by *this* node, composed of:
//!   - **server_bytes** (optional): Static identifier for this server instance.
//!   - **local_index_bytes**: Dynamic bytes — each channel gets a unique value.
//!     The byte-width is derived automatically from `max_channels`.
//! - **rollable bytes**: Remaining space for downstream rolling (extended channels)
//!   or further allocation layers. Standard channels zero-pad this portion.
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
//! # Usage
//!
//! ## Pool (root node, no upstream)
//!
//! A pool creates the allocator with [`ExtranonceAllocator::new`], providing an optional
//! `server_bytes` to distinguish multiple pool server instances. It then calls
//! [`allocate_standard`](ExtranonceAllocator::allocate_standard) or
//! [`allocate_extended`](ExtranonceAllocator::allocate_extended) each time a downstream
//! opens a channel.
//!
//! ```
//! use channels_sv2::extranonce_manager::ExtranonceAllocator;
//!
//! // Pool: 20-byte extranonce, 2-byte server identifier, up to 65 536 channels.
//! // local_prefix_len is derived: server_bytes(2) + bytes_needed(65536)(2) = 4.
//! // Rollable space: 20 − 4 = 16 bytes.
//! let mut allocator = ExtranonceAllocator::new(
//!     20,                         // total_extranonce_len
//!     Some(vec![0x00, 0x01]),     // server_bytes
//!     65_536,                     // max_channels
//! ).unwrap();
//!
//! // Allocate a standard channel:
//! let prefix = allocator.allocate_standard().unwrap();
//! let prefix_bytes = prefix.as_bytes();
//! let idx = prefix.local_index();    // store for later freeing
//!
//! // When the channel closes:
//! allocator.free(idx);
//! ```
//!
//! ## JDC / Translator / Proxies (receives upstream extranonce prefix)
//!
//! Proxies (JDC and Translator included) receive an `extranonce_prefix` from their upstream node
//! (via `OpenExtendedMiningChannelSuccess` or `SetExtranoncePrefix`). They create
//! the allocator with [`ExtranonceAllocator::from_upstream`] and then subdivide
//! the remaining space for their own downstream channels.
//!
//! ```
//! use channels_sv2::extranonce_manager::ExtranonceAllocator;
//!
//! let upstream_prefix = vec![0xAA, 0xBB, 0xCC, 0xDD];
//! let mut allocator = ExtranonceAllocator::from_upstream(
//!     upstream_prefix,
//!     20,     // total_extranonce_len
//!     65_536, // max_channels
//! ).unwrap();
//!
//! // local_prefix_len = bytes_needed(65536) = 2 (no server_bytes for proxies).
//! // Rollable: 20 − 4 (upstream) − 2 (local) = 14 bytes.
//! let prefix = allocator.allocate_extended(14).unwrap();
//! let prefix_bytes = prefix.as_bytes();
//! ```

pub mod allocator;
mod bitvector;
pub mod prefix;

pub use allocator::{ExtranonceAllocator, ExtranonceAllocatorError};
pub use prefix::ExtranoncePrefix;

/// Maximum extranonce length in bytes (Sv2 spec).
pub const MAX_EXTRANONCE_LEN: usize = 32;

/// Minimum bytes needed to represent `n` distinct values (0..n-1).
///
/// Always returns at least 1.
pub(crate) fn bytes_needed(n: usize) -> usize {
    if n <= 1 {
        return 1;
    }
    let bits = usize::BITS - (n - 1).leading_zeros();
    bits.div_ceil(8) as usize
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
