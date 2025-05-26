pub enum ExtendedJobError {
    CoinbaseOutputsSumOverflow,
    InvalidCoinbaseOutputsSum,
}

pub enum StandardJobError {}

#[derive(Debug)]
pub enum ExtendedJobFactoryError {
    InvalidTemplate(String),
    CoinbaseTxPrefixError,
    CoinbaseTxSuffixError,
    CoinbaseOutputsSumOverflow,
    InvalidCoinbaseOutputsSum,
    ChainTipRequired,
}
