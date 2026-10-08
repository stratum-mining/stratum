//! # Channel Error Types

use crate::server::jobs::error::{JobFactoryError, StandardJobError};

/// Errors that can occur while operating an extended channel on the server side.
///
/// Variants carrying `&'static str` are intended to be used as `error_code` values in protocol
/// error messages (for example
/// [`OpenMiningChannelError`](mining_sv2::OpenMiningChannelError) and
/// [`UpdateChannelError`](mining_sv2::UpdateChannelError)).
///
/// Variants without `&'static str` SHOULD lead to a client disconnection or application
/// shutdown.
#[derive(Debug)]
pub enum ExtendedChannelError {
    OpenChannelInvalidNominalHashrate(&'static str),
    UpdateChannelInvalidNominalHashrate(&'static str),
    /// The requested `max_target` is zero. No share can meet it (no hash is below zero), and its
    /// difficulty is not representable (`Target::difficulty_float` returns `INFINITY`), so a
    /// channel operating at it would poison the work sums and the wire-facing
    /// `SubmitShares.Success` accounting with the first block-valid share; it is refused at the
    /// channel boundary instead.
    OpenChannelInvalidMaxTarget(&'static str),
    /// See [`Self::OpenChannelInvalidMaxTarget`]; the channel is left unchanged.
    UpdateChannelInvalidMaxTarget(&'static str),
    /// A target handed to `set_target` is zero, see [`Self::OpenChannelInvalidMaxTarget`]; the
    /// channel is left unchanged.
    InvalidTarget,
    RequestedMinExtranonceSizeTooLarge(&'static str),
    JobFactoryError(JobFactoryError),
    ChainTipNotSet,
    TemplateIdNotFound,
    JobIdNotFound,
    ExtranoncePrefixTooLarge,
    ScriptSigSizeTooLarge,
    InvalidJobOrigin,
    /// An immediately-active job carried an `ntime_start` below the `ntime_start` of the chain tip
    /// it is mined against; the job is discarded and the channel left unchanged.
    JobNtimeStartBelowChainTip,
    /// A group job advertises version rolling while the channel's policy forbids it; the job is
    /// discarded and the channel left unchanged, see
    /// [`ExtendedChannel::on_group_channel_job`](super::extended::ExtendedChannel::on_group_channel_job).
    GroupJobVersionRollingNotAllowed,
}

#[derive(Debug)]
pub enum GroupChannelError {
    FullExtranonceSizeMismatch,
    ChainTipNotSet,
    TemplateIdNotFound,
    JobFactoryError(JobFactoryError),
    ScriptSigSizeTooLarge,
}

/// Errors that can occur while operating a standard channel on the server side.
///
/// Variants carrying `&'static str` are intended to be used as `error_code` values in protocol
/// error messages (for example
/// [`OpenMiningChannelError`](mining_sv2::OpenMiningChannelError) and
/// [`UpdateChannelError`](mining_sv2::UpdateChannelError)).
///
/// Variants without `&'static str` SHOULD lead to a client disconnection or application
/// shutdown.
#[derive(Debug)]
pub enum StandardChannelError {
    OpenChannelInvalidNominalHashrate(&'static str),
    UpdateChannelInvalidNominalHashrate(&'static str),
    /// The requested `max_target` is zero. No share can meet it (no hash is below zero), and its
    /// difficulty is not representable (`Target::difficulty_float` returns `INFINITY`), so a
    /// channel operating at it would poison the work sums and the wire-facing
    /// `SubmitShares.Success` accounting with the first block-valid share; it is refused at the
    /// channel boundary instead.
    OpenChannelInvalidMaxTarget(&'static str),
    /// See [`Self::OpenChannelInvalidMaxTarget`]; the channel is left unchanged.
    UpdateChannelInvalidMaxTarget(&'static str),
    /// A target handed to `set_target` is zero, see [`Self::OpenChannelInvalidMaxTarget`]; the
    /// channel is left unchanged.
    InvalidTarget,
    TemplateIdNotFound,
    ExtranoncePrefixTooLarge,
    JobFactoryError(JobFactoryError),
    ChainTipNotSet,
    /// A group job could not be turned into this channel's standard job, see
    /// [`StandardChannel::on_group_channel_job`](super::standard::StandardChannel::on_group_channel_job).
    StandardJobError(StandardJobError),
    ScriptSigSizeTooLarge,
    /// An immediately-active job carried an `ntime_start` below the `ntime_start` of the chain tip
    /// it is mined against; the job is discarded and the channel left unchanged.
    JobNtimeStartBelowChainTip,
}
