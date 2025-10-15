use channels_sv2::bip141::StripBip141Error;

#[derive(Debug)]
pub enum StratumTranslationError {
    // SV1 -> SV2
    InvalidJobId,
    IncompatibleVersionRollingMask,
    InvalidExtranonceLength,
    InvalidUserIdentity(String),
    // SV2 -> SV1
    FailedToTryToStripBip141(StripBip141Error),
    FailedToSerializeToB064K,
}

pub type Result<T> = core::result::Result<T, StratumTranslationError>;
