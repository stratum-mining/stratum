//! Sv2 channels - Mining Clients Abstractions.
//!
//! The `client` module is compatible with `no_std` environments. To enable this mode, build the
//! crate with the `no_std` feature. In this configuration, standard library collections are
//! replaced with the `hashbrown` crate, together with `core` and `alloc`, allowing the module to be
//! used in embedded or constrained contexts.

pub mod error;
pub mod extended;
pub mod group;
pub mod share_accounting;
pub mod standard;

/// Maximum number of future jobs a client channel retains while waiting for a
/// [`SetNewPrevHash`](mining_sv2::SetNewPrevHash) (or chain tip update).
///
/// Upstream servers control `job_id`, so future jobs are stored under an upstream-controlled key.
/// Bounding this map prevents a malicious or buggy server from exhausting client memory by
/// streaming future jobs while withholding [`SetNewPrevHash`](mining_sv2::SetNewPrevHash). On
/// overflow, the oldest future job is evicted.
pub const MAX_FUTURE_JOBS: usize = 16;

// Type aliases that switch between `std::collections` and `hashbrown`
// depending on whether the `no_std` feature is enabled.
#[cfg(not(feature = "no_std"))]
type HashMap<K, V> = std::collections::HashMap<K, V>;
#[cfg(not(feature = "no_std"))]
type HashSet<T> = std::collections::HashSet<T>;
#[cfg(feature = "no_std")]
type HashMap<K, V> = hashbrown::HashMap<K, V>;
#[cfg(feature = "no_std")]
type HashSet<T> = hashbrown::HashSet<T>;
