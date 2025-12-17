//! # Channel Error Types

use crate::server::jobs::error::JobFactoryError;

#[derive(Debug)]
pub enum ExtendedChannelError {
    JobFactoryError(JobFactoryError),
    InvalidNominalHashrate,
    RequestedMaxTargetOutOfRange,
    ChainTipNotSet,
    TemplateIdNotFound,
    JobIdNotFound,
    RequestedMinExtranonceSizeTooLarge,
    ExtranoncePrefixTooLarge,
    ScriptSigSizeTooLarge,
    InvalidJobOrigin,
}

#[derive(Debug)]
pub enum GroupChannelError {
    ChainTipNotSet,
    TemplateIdNotFound,
    JobFactoryError(JobFactoryError),
    ScriptSigSizeTooLarge,
}

#[derive(Debug)]
pub enum StandardChannelError {
    TemplateIdNotFound,
    InvalidNominalHashrate,
    RequestedMaxTargetOutOfRange,
    ExtranoncePrefixTooLarge,
    JobFactoryError(JobFactoryError),
    ChainTipNotSet,
    FailedToConvertToStandardJob,
    ScriptSigSizeTooLarge,
}
