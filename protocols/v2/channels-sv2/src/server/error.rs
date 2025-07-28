use crate::server::jobs::error::JobFactoryError;

/// Error variants for Extended Channel operations.
///
/// Represents all possible failure reasons when operating on an Extended Channel,
/// including job creation failures, invalid configuration, and state errors.
#[derive(Debug)]
pub enum ExtendedChannelError {
    /// Error occurred during job creation via the job factory.
    JobFactoryError(JobFactoryError),
    /// The nominal hashrate provided is invalid.
    InvalidNominalHashrate,
    /// The requested maximum target is out of the allowed range.
    RequestedMaxTargetOutOfRange,
    /// The chain tip is not set when required for the operation.
    ChainTipNotSet,
    /// The specified template ID was not found among future jobs.
    TemplateIdNotFound,
    /// The specified job ID was not found among future jobs.
    JobIdNotFound,
    /// The requested minimum rollable extranonce size exceeds available extranonce length.
    RequestedMinExtranonceSizeTooLarge,
    /// The new extranonce prefix is too large and violates channel constraints.
    NewExtranoncePrefixTooLarge,
}

/// Error variants for Group Channel operations.
///
/// Represents all possible failure reasons when operating on a Group Channel,
/// including missing chain tip, missing template, and job factory errors.
#[derive(Debug)]
pub enum GroupChannelError {
    /// The chain tip is not set when required for the operation.
    ChainTipNotSet,
    /// The specified template ID was not found among future jobs.
    TemplateIdNotFound,
    /// Error occurred during job creation via the job factory.
    JobFactoryError(JobFactoryError),
}

/// Error variants for Standard Channel operations.
///
/// Represents all possible failure reasons when operating on a Standard Channel,
/// including job creation failures, invalid configuration, and state errors.
#[derive(Debug)]
pub enum StandardChannelError {
    /// The specified template ID was not found among future jobs.
    TemplateIdNotFound,
    /// The nominal hashrate provided is invalid.
    InvalidNominalHashrate,
    /// The requested maximum target is out of the allowed range.
    RequestedMaxTargetOutOfRange,
    /// The new extranonce prefix is too large and violates channel constraints.
    NewExtranoncePrefixTooLarge,
    /// Error occurred during job creation via the job factory.
    JobFactoryError(JobFactoryError),
    /// The chain tip is not set when required for the operation.
    ChainTipNotSet,
}
