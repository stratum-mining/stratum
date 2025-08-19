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
    NewExtranoncePrefixTooLarge,
}

#[derive(Debug)]
pub enum GroupChannelError {
    ChainTipNotSet,
    TemplateIdNotFound,
    JobFactoryError(JobFactoryError),
}

#[derive(Debug)]
pub enum StandardChannelError {
    TemplateIdNotFound,
    InvalidNominalHashrate,
    RequestedMaxTargetOutOfRange,
    NewExtranoncePrefixTooLarge,
    JobFactoryError(JobFactoryError),
    ChainTipNotSet,
    FailedToConvertToStandardJob,
}
