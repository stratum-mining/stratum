use core::fmt;

/// Generic errors that can occur during TLV field encoding and decoding.
#[derive(Debug, PartialEq, Eq)]
pub enum TlvError {
    /// TLV data buffer is too short to contain a valid TLV header or value.
    ///
    /// Contains (actual_length, required_length).
    BufferTooShort(usize, usize),

    /// Failed to encode/serialize TLV data using binary_sv2.
    ///
    /// Contains the underlying binary_sv2::Error.
    EncodingError(binary_sv2::Error),

    /// Failed to decode/deserialize TLV data using binary_sv2.
    ///
    /// Contains the underlying binary_sv2::Error.
    DecodingError(binary_sv2::Error),

    /// The value is longer than the 2-byte TLV length field can declare.
    ///
    /// Contains the value length.
    ValueTooLong(usize),

    /// The TLV's `length` field does not match the length of its value.
    LengthMismatch {
        /// Length the `length` field declares.
        declared: u16,
        /// Actual length of the value.
        actual: usize,
    },
}

impl fmt::Display for TlvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TlvError::BufferTooShort(actual, required) => write!(
                f,
                "TLV buffer too short: {} bytes available, {} bytes required",
                actual, required
            ),
            TlvError::EncodingError(err) => write!(f, "Failed to encode TLV data: {:?}", err),
            TlvError::DecodingError(err) => write!(f, "Failed to decode TLV data: {:?}", err),
            TlvError::ValueTooLong(len) => write!(
                f,
                "TLV value is {} bytes long, the length field holds at most {}",
                len,
                u16::MAX
            ),
            TlvError::LengthMismatch { declared, actual } => write!(
                f,
                "TLV length field declares {} bytes, the value is {} bytes long",
                declared, actual
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_error_display() {
        let err = TlvError::BufferTooShort(5, 10);
        assert!(err.to_string().contains("5"));
        assert!(err.to_string().contains("10"));
    }
}
