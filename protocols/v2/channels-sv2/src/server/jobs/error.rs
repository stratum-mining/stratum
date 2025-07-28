/// Error variants for Extended Job operations.
///
/// Represents possible failures when constructing or converting extended mining jobs,
/// including deserialization issues, invalid parameters, and protocol constraints.
#[derive(Debug)]
pub enum ExtendedJobError {
    /// Failed to deserialize the coinbase transaction.
    FailedToDeserializeCoinbase,
    /// Failed to deserialize coinbase outputs.
    FailedToDeserializeCoinbaseOutputs,
    /// The coinbase transaction does not have the expected number of inputs.
    CoinbaseInputCountMismatch,
    /// Failed to serialize coinbase outputs.
    FailedToSerializeCoinbaseOutputs,
    /// Failed to serialize the coinbase prefix.
    FailedToSerializeCoinbasePrefix,
    /// Attempted to convert a future job into a custom mining job (not allowed).
    FutureJobNotAllowed,
    /// The minimum ntime provided is invalid.
    InvalidMinNTime,
}

/// Error variants for Standard Job operations.
///
/// Represents possible failures when constructing or handling standard mining jobs.
pub enum StandardJobError {
    /// Failed to deserialize coinbase outputs.
    FailedToDeserializeCoinbaseOutputs,
}

/// Error variants for Job Factory operations.
///
/// Represents possible failures when creating jobs using the job factory,
/// including template errors, serialization issues, and invalid coinbase outputs.
#[derive(Debug)]
pub enum JobFactoryError {
    /// The provided template is invalid. Includes a descriptive error message.
    InvalidTemplate(String),
    /// Failed to deserialize coinbase outputs.
    DeserializeCoinbaseOutputsError,
    /// Failed to serialize the coinbase transaction prefix.
    CoinbaseTxPrefixError,
    /// Failed to serialize the coinbase transaction suffix.
    CoinbaseTxSuffixError,
    /// Overflow occurred when summing coinbase outputs.
    CoinbaseOutputsSumOverflow,
    /// The sum of coinbase outputs is invalid for the template.
    InvalidCoinbaseOutputsSum,
    /// A chain tip is required but was not provided.
    ChainTipRequired,
}
