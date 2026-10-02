//! # Stratum V2 Channels
//!
//! `channels_sv2` provides primitives and abstractions for Stratum V2 (Sv2) Channels.
//!
//! This crate implements the core channel management functionality for both mining clients and
//! servers, including standard, extended, and group channels, and share accounting mechanisms.
//!
//! ## Features
//!
//! - Channel primitives for SV2 mining protocol
//! - Channel management for mining servers and clients
//! - Standard, extended, and group channel support
//! - Share accounting
//! - Server-side job lifecycle management
//! - [`client`] module is `no_std` compatible. To enable it build the crate with `no_std` feature.
#![cfg_attr(feature = "no_std", no_std)]

pub mod extranonce_manager;
pub use extranonce_manager::MAX_EXTRANONCE_LEN;

/// Version rolling mask as per [BIP323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki):
/// only these general-purpose bits may differ between a job's advertised version and a share's
/// version. All other bits of a share's version must match the job's version exactly.
pub const VERSION_ROLLING_MASK: u32 = 0x1fffffe0;

/// Mirrors Bitcoin Core's `MAX_FUTURE_BLOCK_TIME` (`src/chain.h`): a block timestamp more than
/// 2 hours in the future is consensus-invalid.
///
/// Share validation enforces `share.ntime <= ntime_start + MAX_FUTURE_BLOCK_TIME`, where
/// `ntime_start` is the referenced job's (the chain tip's for jobs built or activated under it),
/// anchoring the consensus allowance at the receipt of the message that supplied it
/// (`ntime_start` ≈ wall time when that message arrived, since this crate is `no_std`-compatible
/// and has no clock). This is deliberately looser than the Sv2 spec's elapsed-time window
/// (`ntime <= ntime_start + seconds elapsed since receipt of the message that supplied it`, which
/// is stricter than consensus): embedding applications that have a time source can additionally
/// enforce the spec-exact window. The bound equals the consensus limit at receipt and becomes
/// conservative as the job ages; a false rejection would require a >2h-old job *and* a miner
/// stamping wall time instead of rolling from the job's `ntime_start` — a known, negligible edge.
pub const MAX_FUTURE_BLOCK_TIME: u32 = 2 * 60 * 60;

/// Worst-case chain-tip lifetime, in minutes, assumed when bounding the accepted-share dedup
/// cache (`seen_shares`).
///
/// `seen_shares` only needs to hold shares for the lifetime of one chain tip (it is flushed
/// whenever the tip's `prev_hash` changes), and 10 hours is far beyond any realistic block
/// interval, so the resulting cap is unreachable by a well-behaved channel.
pub const WORST_CASE_TIP_MINUTES: u64 = 600;

/// Safety margin applied on top of the expected share rate when bounding the accepted-share
/// dedup cache (`seen_shares`).
///
/// Poisson variance over thousands of expected shares is ~√N, so a 2× factor is already
/// generous; its real job is covering the window before vardiff converges.
pub const SEEN_SHARES_MARGIN: u64 = 2;

/// Lower clamp applied to the seen-shares budget derived by [`seen_shares_budget`].
///
/// Keeps duplicate detection meaningful when the configured expected rate is absurdly low:
/// 4 096 hashes ≈ 0.3 MB measured (the backing `HashSet` rounds up to 8 192 slots). Without
/// it, a tiny derived budget would close a legitimate server channel after a handful of
/// accepted shares.
pub const MIN_SEEN_SHARES_CAP: usize = 4_096;

/// Upper clamp on the seen-shares budget derived by [`seen_shares_budget`], in hashes.
///
/// Backstops absurd expected-rate configuration, so that no single server channel's dedup cache
/// can be sized past a few tens of MB. Client channels do not use this: they bound their cache
/// at the far smaller [`client::MAX_SEEN_SHARES`], see there for why.
pub const MAX_SEEN_SHARES_CAP: usize = 1 << 20;

/// Returns the maximum number of accepted-share hashes retained for duplicate detection, given
/// the channel's expected share rate:
///
/// ```text
/// budget = clamp(expected_shares_per_minute × WORST_CASE_TIP_MINUTES × SEEN_SHARES_MARGIN,
///                MIN_SEEN_SHARES_CAP, MAX_SEEN_SHARES_CAP)
/// ```
///
/// Sustaining more than [`SEEN_SHARES_MARGIN`]× the expected rate for
/// [`WORST_CASE_TIP_MINUTES`] straight means the share rate has far outrun the channel's
/// configured expectation with no tip transition in between — not a state to accommodate.
/// This assumes mainnet-like block cadence: on networks where a tip can stay pinned past ~20
/// hours (regtest, an idle CI chain), an honest channel can walk into the budget, so such
/// setups should raise the configured rate or treat the resulting channel closure as expected.
///
/// Worst-case per-channel memory (measured, not payload-only: the backing `HashSet` rounds its
/// capacity to a power of two at ~7/8 load and transiently doubles while rehashing): at the
/// default-ish 6 shares/min the budget is 7 200 hashes ≈ 0.5 MB per channel; at 100 shares/min,
/// 120 000 hashes ≈ 9 MB steady and ~15 MB peak through the last rehash. The result is clamped
/// into [[`MIN_SEEN_SHARES_CAP`], [`MAX_SEEN_SHARES_CAP`]] so that a degenerate expected rate
/// can neither disable dedup nor unbound memory.
///
/// Only server channels derive a budget: they have a pool-configured expected rate, and the
/// budget is load-bearing there (validation fails once it is hit, signalling the embedding
/// application to close the channel), so it must be unreachable for any honest rate. Client
/// channels bound `seen_shares` at the flat [`client::MAX_SEEN_SHARES`] and evict oldest-first;
/// see [`client::share_accounting::ShareAccounting`] for why they need no derivation.
pub fn seen_shares_budget(expected_shares_per_minute: f64) -> usize {
    // `as usize` saturates on overflow/NaN, and the margin factor makes truncation irrelevant
    ((expected_shares_per_minute * (WORST_CASE_TIP_MINUTES * SEEN_SHARES_MARGIN) as f64) as usize)
        .clamp(MIN_SEEN_SHARES_CAP, MAX_SEEN_SHARES_CAP)
}

#[cfg(not(feature = "no_std"))]
pub mod server;

#[cfg(not(feature = "no_std"))]
pub mod outputs;

pub mod bip141;
pub mod chain_tip;
pub mod client;
pub mod merkle_root;
pub mod target;

#[cfg(not(feature = "no_std"))]
pub mod vardiff;

#[cfg(not(feature = "no_std"))]
pub use vardiff::{classic::VardiffState, Vardiff};
