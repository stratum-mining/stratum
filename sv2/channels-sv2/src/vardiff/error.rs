/// Defines errors encountered during Vardiff operations.
#[derive(Debug)]
pub enum VardiffError {
    /// Failed to convert hashrate to a target.
    HashrateToTargetError(String),
    /// Failed to convert target to a hashrate.
    TargetToHashrateError(String),
    /// **Unreachable after the Clock-injection refactor.**
    ///
    /// The algorithm previously called `SystemTime::now().duration_since(...)`
    /// which could fail if the system clock was before the UNIX epoch — this
    /// variant carried that error. Time access now goes through
    /// [`crate::vardiff::Clock::now_secs`], which is infallible, so no path in
    /// the algorithm constructs this variant anymore.
    ///
    /// Retained for backward compatibility (removing an enum variant would
    /// break downstream exhaustive matches). Scheduled for removal at the
    /// next major version bump.
    TimeError(std::time::SystemTimeError),
}

/// Conversion preserved alongside [`VardiffError::TimeError`] for the same
/// backward-compatibility reason. No code path in this crate exercises it.
impl From<std::time::SystemTimeError> for VardiffError {
    fn from(value: std::time::SystemTimeError) -> Self {
        VardiffError::TimeError(value)
    }
}
