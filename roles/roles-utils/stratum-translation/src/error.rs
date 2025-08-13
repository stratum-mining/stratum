#[derive(Debug)]
pub enum StratumTranslationError {
    // SV1 -> SV2
    InvalidJobId,
    IncompatibleVersionRollingMask,
    InvalidExtranonceLength,
    InvalidUserIdentity(String),
}

pub type Result<T> = core::result::Result<T, StratumTranslationError>;
