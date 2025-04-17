use crate::job_management::extended::ExtendedJobFactoryError;

#[derive(Debug)]
pub enum ExtendedChannelError {
    JobFactoryError(ExtendedJobFactoryError),
    ChainTipNotSet,
    TemplateIdNotFound,
}
