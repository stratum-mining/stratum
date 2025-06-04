use crate::channels::server::jobs::error::ExtendedJobFactoryError;

#[derive(Debug)]
pub enum ExtendedChannelError {
    JobFactoryError(ExtendedJobFactoryError),
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
    JobFactoryError(ExtendedJobFactoryError),
}

#[derive(Debug)]
pub enum StandardChannelError {
    TemplateIdNotFound,
    InvalidNominalHashrate,
    RequestedMaxTargetOutOfRange,
    NewExtranoncePrefixTooLarge,
}
