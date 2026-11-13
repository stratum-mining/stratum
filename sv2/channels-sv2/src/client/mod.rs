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

/// Maximum number of past jobs a client channel retains under the current chain tip.
///
/// Past jobs serve late-share validation, so the cap must stay nonzero. On overflow the oldest is
/// evicted and a share against it is rejected as
/// [`InvalidJobId`](crate::client::share_accounting::ShareValidationError::InvalidJobId) — a
/// bounded loss of creditable work, the price of bounding memory under a hostile upstream, which
/// controls the job stream and can otherwise force one retained past job per active-job message.
///
/// Matches the server-side default, so a proxy never evicts a job its upstream still accepts.
/// Retaining more than the upstream gains nothing: the proxy credits the share locally and the
/// upstream rejects it anyway.
///
/// The cap is a retention *window* — `cap / job rate` — and here the upstream sets the rate.
/// Measurement bounded the requirement at ~16 s (PR #2307), so 16 covers the fastest configurable
/// rate of one job per second. Operators who know their upstream's interval `T` should set
/// `ceil(16 s / T)` and reclaim the memory, at ~4.5 kB per retained job per channel (PR #2290).
/// That matters most on a translator, where every downstream miner holds its own client channel;
/// on a job-declaration client the interval is its own `SetCustomMiningJob` rate.
///
/// Constructors take a `max_past_jobs` override, falling back here on `None`/`Some(0)`.
pub const MAX_PAST_JOBS: usize = 16;

/// Maximum number of accepted-share hashes a client channel retains for duplicate detection.
///
/// 4 096 hashes is one 128 KB allocation, which keeps the `no_std`/embedded use case viable:
/// the bound has to be affordable on the smallest supported device, since an adversarial
/// upstream advertising a trivial target can drive the cache to it at message speed.
///
/// A client cache does not need to hold a whole chain tip's worth of shares. It exists to catch
/// a share source re-submitting work it already sent — a retransmit or a buggy loop, which
/// arrives within seconds, or a replay once the tip returns to a `prev_hash` it just left, which
/// follows a short reorg. The cache is kept across tip changes for the latter. 4 096 covers
/// ~11 hours of history for a typical 6 shares/min channel and ~7 minutes for a very busy
/// 600 shares/min proxy channel, far beyond any realistic duplicate window in both cases. Overflow evicts oldest-first, and
/// an evicted-then-replayed hash costs one double-counted local statistic: an accepted replay
/// window, see [`ShareAccounting`](share_accounting::ShareAccounting) for why clients do not
/// fail hard at the bound.
pub const MAX_SEEN_SHARES: usize = 4_096;

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
