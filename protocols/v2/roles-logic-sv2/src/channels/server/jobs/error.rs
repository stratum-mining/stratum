#[derive(Debug)]
pub enum ExtendedJobError {
    FailedToDeserializeCoinbase,
    FailedToDeserializeCoinbaseOutputs,
    CoinbaseInputCountMismatch,
    FailedToSerializeCoinbaseOutputs,
    FailedToSerializeCoinbasePrefix,
    FutureJobNotAllowed,
    InvalidMinNTime,
}

pub enum StandardJobError {
    FailedToDeserializeCoinbaseOutputs,
}

#[derive(Debug)]
pub enum JobFactoryError {
    InvalidTemplate(String),
    DeserializeCoinbaseOutputsError,
    CoinbaseTxPrefixError,
    CoinbaseTxSuffixError,
    CoinbaseOutputsSumOverflow,
    InvalidCoinbaseOutputsSum,
    ChainTipRequired,
}
