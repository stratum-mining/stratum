use crate::job_management::extended::ExtendedJobFactoryError;

#[derive(Debug)]
pub enum StandardChannelError {
    TemplateIdNotFound,
}

#[derive(Debug)]
pub enum GroupChannelError {
    TemplateIdNotFound,
    JobFactoryError(ExtendedJobFactoryError),
}
