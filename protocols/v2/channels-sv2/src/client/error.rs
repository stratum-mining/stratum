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
}

/// Errors that can occur within a **group channel** context.
///
/// Currently includes only job ID lookup failures.
#[derive(Debug)]
pub enum GroupChannelError {
    /// The specified job ID was not found in the group channel.
    JobIdNotFound,
}
