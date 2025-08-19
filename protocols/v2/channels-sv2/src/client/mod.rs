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
