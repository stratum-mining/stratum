//! # Job Error Types

#[derive(Debug)]
pub enum ExtendedJobError {
    FailedToDeserializeCoinbase,
    FailedToDeserializeCoinbaseOutputs,
    CoinbaseInputCountMismatch,
    FailedToSerializeCoinbaseOutputs,
    FailedToSerializeCoinbasePrefix,
    FutureJobNotAllowed,
    InvalidNtimeStart,
}

#[derive(Debug)]
pub enum StandardJobError {
    FailedToDeserializeCoinbaseOutputs,
    InvalidJobOrigin,
    FailedToCalculateMerkleRoot,
}

#[derive(Debug)]
pub enum JobFactoryError {
    FailedToStripBip141,
    FailedToSerializeCoinbaseOutputs,
    FailedToSerializeCoinbasePrefix,
    InvalidTemplate(String),
    DeserializeCoinbaseOutputsError,
    CoinbaseTxPrefixError,
    CoinbaseTxSuffixError,
    CoinbaseOutputsSumOverflow,
    InvalidCoinbaseOutputsSum,
    ChainTipRequired,
    /// The assembled coinbase `scriptSig` would exceed the size mandated by Bitcoin consensus
    /// rules (see `MAX_SCRIPT_SIG_SIZE`).
    ScriptSigSizeTooLarge,
    /// The bytes of a [`PrebuiltJob`](super::PrebuiltJob) around a zero-filled extranonce slot
    /// do not deserialize as a coinbase transaction, so no merkle root can be computed from them.
    InvalidPrebuiltCoinbase,
}
