/// Defines errors encountered during Vardiff operations.
#[derive(Debug)]
pub enum VardiffError {
    /// Failed to convert hashrate to a target.
    HashrateToTargetError(String),
    /// Failed to convert target to a hashrate.
    TargetToHashrateError(String),
    /// System time error occurred.
    TimeError(std::time::SystemTimeError),
}

impl From<std::time::SystemTimeError> for VardiffError {
    fn from(value: std::time::SystemTimeError) -> Self {
        VardiffError::TimeError(value)
    }
}
