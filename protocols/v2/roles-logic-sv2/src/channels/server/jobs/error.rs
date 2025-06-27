#[derive(Debug)]
pub enum ExtendedJobError {
    FailedToDeserializeCoinbase,
    CoinbaseInputCountMismatch,
    FailedToSerializeCoinbaseOutputs,
    FailedToSerializeCoinbasePrefix,
    FutureJobNotAllowed,
    InvalidMinNTime,
}

pub enum StandardJobError {}

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
