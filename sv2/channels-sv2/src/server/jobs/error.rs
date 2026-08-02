//! # Job Error Types

#[derive(Debug)]
pub enum ExtendedJobError {
    FailedToDeserializeCoinbase,
    FailedToDeserializeCoinbaseOutputs,
    CoinbaseInputCountMismatch,
    FailedToSerializeCoinbaseOutputs,
    FailedToSerializeCoinbasePrefix,
    FailedToConvertToStandardJob,
    FailedToCalculateMerkleRoot,
    FutureJobNotAllowed,
    InvalidMinNTime,
}

pub enum StandardJobError {
    FailedToDeserializeCoinbaseOutputs,
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
}
