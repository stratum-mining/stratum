use core::fmt;

/// Generic errors that can occur during TLV field encoding and decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Failed to construct a StandardSv2Frame from bytes.
    ///
    /// Contains the error code returned by `Sv2Frame::from_bytes`.
    FrameConstructionFailed(isize),
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
            TlvError::FrameConstructionFailed(code) => {
                write!(
                    f,
                    "Failed to construct Sv2Frame from bytes (error code: {})",
                    code
                )
            }
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

        let err = TlvError::FrameConstructionFailed(-1);
        assert!(err.to_string().contains("-1"));
    }
}
