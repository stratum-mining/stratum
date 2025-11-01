use core::fmt;

/// Errors that can occur during TLV field encoding and decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlvError {
    /// Worker identity exceeds the maximum allowed length of 32 bytes.
    ///
    /// Contains the actual length in bytes.
    WorkerIdTooLong(usize),

    /// TLV data buffer is too short to contain a valid TLV header.
    ///
    /// Contains (actual_length, required_length).
    BufferTooShort(usize, usize),

    /// Extension type in TLV does not match expected Worker Hashrate Tracking (0x0002).
    ///
    /// Contains the actual extension_type found.
    InvalidExtensionType(u16),

    /// Field type in TLV does not match expected UserIdentity (0x01).
    ///
    /// Contains the actual field_type found.
    InvalidFieldType(u8),

    /// Failed to deserialize the UserIdentity structure from TLV value bytes.
    ///
    /// Contains the underlying binary_sv2::Error.
    DeserializationFailed(binary_sv2::Error),

    /// Failed to serialize the UserIdentity structure to TLV value bytes.
    ///
    /// Contains the underlying binary_sv2::Error.
    SerializationFailed(binary_sv2::Error),

    /// Failed to encode TLV data using binary_sv2.
    EncodingError,

    /// Failed to decode TLV data using binary_sv2.
    DecodingError,
}

impl fmt::Display for TlvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TlvError::WorkerIdTooLong(len) => write!(
                f,
                "Worker identity length {} bytes exceeds maximum of 32 bytes",
                len
            ),
            TlvError::BufferTooShort(actual, required) => write!(
                f,
                "TLV buffer too short: {} bytes available, {} bytes required",
                actual, required
            ),
            TlvError::InvalidExtensionType(ext_type) => write!(
                f,
                "Invalid extension type 0x{:04x}, expected 0x0002 (Worker Hashrate Tracking)",
                ext_type
            ),
            TlvError::InvalidFieldType(field_type) => write!(
                f,
                "Invalid field type 0x{:02x}, expected 0x01 (UserIdentity)",
                field_type
            ),
            TlvError::DeserializationFailed(err) => {
                write!(f, "Failed to deserialize UserIdentity: {:?}", err)
            }
            TlvError::SerializationFailed(err) => {
                write!(f, "Failed to serialize UserIdentity: {:?}", err)
            }
            TlvError::EncodingError => write!(f, "Failed to encode TLV data"),
            TlvError::DecodingError => write!(f, "Failed to decode TLV data"),
        }
    }
}

impl From<binary_sv2::Error> for TlvError {
    fn from(err: binary_sv2::Error) -> Self {
        TlvError::DeserializationFailed(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_error_display() {
        let err = TlvError::WorkerIdTooLong(50);
        assert!(err.to_string().contains("50"));
        assert!(err.to_string().contains("32"));

        let err = TlvError::InvalidExtensionType(0x0099);
        assert!(err.to_string().contains("0x0099"));
        assert!(err.to_string().contains("0x0002"));

        let err = TlvError::InvalidFieldType(0x05);
        assert!(err.to_string().contains("0x05"));
        assert!(err.to_string().contains("0x01"));
    }
}
