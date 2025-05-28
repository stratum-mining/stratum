#[derive(Debug)]
pub enum VardiffError {
    HashrateToTargetError(String),
    TargetToHashrateError(String),
    TimeError(std::time::SystemTimeError),
}

impl From<std::time::SystemTimeError> for VardiffError {
    fn from(value: std::time::SystemTimeError) -> Self {
        VardiffError::TimeError(value)
    }
}
