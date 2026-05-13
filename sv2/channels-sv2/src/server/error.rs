//! # Channel Error Types

use crate::server::jobs::error::JobFactoryError;

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
    RequestedMinExtranonceSizeTooLarge(&'static str),
    JobFactoryError(JobFactoryError),
    ChainTipNotSet,
    TemplateIdNotFound,
    JobIdNotFound,
    ExtranoncePrefixTooLarge,
    ScriptSigSizeTooLarge,
    InvalidJobOrigin,
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
    TemplateIdNotFound,
    ExtranoncePrefixTooLarge,
    JobFactoryError(JobFactoryError),
    ChainTipNotSet,
    FailedToConvertToStandardJob,
    ScriptSigSizeTooLarge,
}
