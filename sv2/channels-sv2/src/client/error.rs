//! # Channel Error Types
//!
//! This module defines error types for different channel contexts: extended, standard,
//! and group channels. Each error type represents specific categories of failures that
//! can occur during channel operations.

use crate::bip141::StripBip141Error;

/// Errors that can occur within an **extended channel** context.
///
/// These include conditions where the extranonce prefix exceeds allowed limits
/// or a referenced job ID is not recognized by the channel.
#[derive(Debug)]
pub enum ExtendedChannelError {
    /// The provided extranonce prefix exceeds the maximum allowed size.
    NewExtranoncePrefixTooLarge,

    /// The provided target is zero. No share can meet it (no hash is below zero), and its
    /// difficulty is not representable (`Target::difficulty_float` returns `INFINITY`), so
    /// accepting it would let the first block-valid share poison the validated-work statistics.
    /// The target is upstream-controlled, and the channel boundary is where an unattainable one
    /// is refused.
    InvalidTarget,

    /// The specified job ID was not found in the extended channel.
    JobIdNotFound,
    FailedToTryToStripBip141(StripBip141Error),
    FailedToStripBip141,
    FailedToSerializeToB064K,
    FailedToDeserializeCoinbaseOutputs,
    ChannelIdMismatch,
    RequestIdMismatch,
    NoChainTip,
    ChainTipMismatch,
    /// An immediately-active job carried an `ntime_start` below the `ntime_start` of the chain tip
    /// it is mined against; the job is discarded and the channel left unchanged.
    JobNtimeStartBelowChainTip,
}

/// Errors that can occur within a **standard channel** context.
///
/// These cover scenarios such as missing job IDs or an oversized extranonce prefix.
#[derive(Debug)]
pub enum StandardChannelError {
    /// The specified job ID was not found in the standard channel.
    JobIdNotFound,

    /// The provided extranonce prefix exceeds the maximum allowed size.
    NewExtranoncePrefixTooLarge,

    /// The provided target is zero. No share can meet it (no hash is below zero), and its
    /// difficulty is not representable (`Target::difficulty_float` returns `INFINITY`), so
    /// accepting it would let the first block-valid share poison the validated-work statistics.
    /// The target is upstream-controlled, and the channel boundary is where an unattainable one
    /// is refused.
    InvalidTarget,

    /// The coinbase transaction of a group channel job is malformed, so no merkle root
    /// could be derived from it.
    InvalidCoinbase,
    /// An immediately-active job carried an `ntime_start` below the `ntime_start` of the chain tip
    /// it is mined against; the job is discarded and the channel left unchanged.
    JobNtimeStartBelowChainTip,
}

/// Errors that can occur within a **group channel** context.
///
/// Currently includes only job ID lookup failures.
#[derive(Debug)]
pub enum GroupChannelError {
    /// The specified job ID was not found in the group channel.
    JobIdNotFound,
    /// The full extranonce size for the group channel does not match the full extranonce size for the channel.
    FullExtranonceSizeMismatch,
}
