//! TLV (Type-Length-Value) encoding and decoding implementation.
//!
//! This module provides the core functionality for working with TLV fields in Stratum V2 extensions.

use super::TlvError;
use crate::binary_sv2::{self, Deserialize, Serialize};
use alloc::{string::String, vec::Vec};
use core::fmt::{self, Write};

extern crate alloc;

/// TLV header size in bytes (3 bytes Type + 2 bytes Length)
pub const TLV_HEADER_SIZE: usize = 5;

/// TLV Type field (3 bytes total).
///
/// Combines extension_type (U16) and field_type (U8) to uniquely identify
/// a TLV field within the extension system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Type {
    /// Extension type identifier (2 bytes, little-endian)
    pub extension_type: u16,
    /// Field type within the extension (1 byte)
    pub field_type: u8,
}

impl Type {
    /// Creates a new TLV Type.
    pub fn new(extension_type: u16, field_type: u8) -> Self {
        Self {
            extension_type,
            field_type,
        }
    }
}

/// A single TLV (Type-Length-Value) field.
///
/// # Structure
/// - Type (3 bytes): extension_type (U16) + field_type (U8)
/// - Length (2 bytes): U16 length of value in bytes
/// - Value (variable): Raw byte data
///
/// Total size: 5 bytes (TLV header) + value length
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tlv {
    /// TLV type (extension_type + field_type)
    pub r#type: Type,
    /// Length of value in bytes
    pub length: u16,
    /// Raw value bytes
    pub value: Vec<u8>,
}

impl Tlv {
    /// Creates a new TLV field.
    ///
    /// # Arguments
    /// * `extension_type` - The extension identifier
    /// * `field_type` - The field type within the extension
    /// * `value` - The raw value bytes
    pub fn new(extension_type: u16, field_type: u8, value: Vec<u8>) -> Self {
        let length = value.len() as u16;
        Self {
            r#type: Type::new(extension_type, field_type),
            length,
            value,
        }
    }

    /// Decodes a TLV from a byte buffer.
    ///
    /// Expects the buffer to start with a complete TLV (Type-Length-Value).
    /// Returns an error if the buffer is too short or decoding fails.
    pub fn decode(data: &[u8]) -> Result<Self, TlvError> {
        if data.len() < TLV_HEADER_SIZE {
            return Err(TlvError::BufferTooShort(data.len(), TLV_HEADER_SIZE));
        }

        // Decode Type (3 bytes)
        let extension_type = u16::from_le_bytes([data[0], data[1]]);
        let field_type = data[2];
        let r#type = Type::new(extension_type, field_type);

        // Decode Length (2 bytes at offset 3)
        let length = u16::from_le_bytes([data[3], data[4]]);

        // Check if we have enough data for the value
        let total_size = TLV_HEADER_SIZE + length as usize;
        if data.len() < total_size {
            return Err(TlvError::BufferTooShort(data.len(), total_size));
        }

        // Extract value
        let value = data[TLV_HEADER_SIZE..total_size].to_vec();

        Ok(Self {
            r#type,
            length,
            value,
        })
    }

    /// Encodes this TLV into bytes.
    ///
    /// Returns the complete TLV bytes (Type + Length + Value).
    pub fn encode(&self) -> Result<Vec<u8>, TlvError> {
        let mut bytes = Vec::with_capacity(TLV_HEADER_SIZE + self.value.len());

        // Encode Type (3 bytes)
        let type_bytes = binary_sv2::to_bytes(self.r#type).map_err(TlvError::EncodingError)?;
        bytes.extend_from_slice(&type_bytes);

        // Encode Length (2 bytes, little-endian)
        bytes.extend_from_slice(&self.length.to_le_bytes());

        // Append Value
        bytes.extend_from_slice(&self.value);

        Ok(bytes)
    }

    /// Returns the total encoded size of this TLV in bytes.
    #[inline]
    pub fn encoded_size(&self) -> usize {
        TLV_HEADER_SIZE + self.value.len()
    }

    /// Validates that the length field matches the actual value length.
    ///
    /// This is useful for ensuring TLV integrity after deserialization.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.length as usize == self.value.len()
    }
}

impl fmt::Display for Tlv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Tlv(ext=0x{:04x}, field=0x{:02x}, len={}, value={})",
            self.r#type.extension_type,
            self.r#type.field_type,
            self.length,
            value_to_string(&self.value)
        )
    }
}

/// Helper function to convert TLV value bytes to a human-readable string.
fn value_to_string(bytes: &[u8]) -> String {
    match core::str::from_utf8(bytes) {
        Ok(s) => {
            // Check if the string is printable ASCII/UTF-8
            if s.chars().all(|c| !c.is_control() || c == '\n' || c == '\t') {
                alloc::format!("\"{}\"", s)
            } else {
                bytes_to_hex(bytes)
            }
        }
        Err(_) => bytes_to_hex(bytes),
    }
}

/// Helper function to convert bytes to hex string representation.
fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut hex = String::from("0x");
    for byte in bytes {
        let _ = write!(&mut hex, "{:02x}", byte);
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_tlv_struct_encode_decode() {
        let value = b"Worker_001".to_vec();
        let tlv = Tlv::new(0x0002, 0x01, value.clone());

        assert_eq!(tlv.r#type.extension_type, 0x0002);
        assert_eq!(tlv.r#type.field_type, 0x01);
        assert_eq!(tlv.length, 10);
        assert_eq!(tlv.value, value);

        // Encode and decode
        let encoded = tlv.encode().unwrap();
        let decoded = Tlv::decode(&encoded).unwrap();

        assert_eq!(decoded, tlv);
    }

    #[test]
    fn test_tlv_encode_format() {
        let tlv = Tlv::new(0x0002, 0x01, b"test".to_vec());
        let encoded = tlv.encode().unwrap();

        // Check encoding format (little-endian)
        assert_eq!(encoded[0], 0x02); // extension_type low byte
        assert_eq!(encoded[1], 0x00); // extension_type high byte
        assert_eq!(encoded[2], 0x01); // field_type
        assert_eq!(encoded[3], 0x04); // length low byte (4)
        assert_eq!(encoded[4], 0x00); // length high byte
        assert_eq!(&encoded[5..], b"test");
    }

    #[test]
    fn test_tlv_decode_buffer_too_short() {
        let short_buffer = vec![0x02, 0x00, 0x01]; // Only 3 bytes, need at least 5
        let result = Tlv::decode(&short_buffer);
        assert!(matches!(result, Err(TlvError::BufferTooShort(3, 5))));
    }

    #[test]
    fn test_tlv_decode_value_too_short() {
        let mut buffer = vec![0x02, 0x00, 0x01]; // Type: 0x0002, field: 0x01
        buffer.extend_from_slice(&[0x0a, 0x00]); // Length: 10
        buffer.extend_from_slice(b"short"); // Only 5 bytes, but length says 10

        let result = Tlv::decode(&buffer);
        assert!(matches!(result, Err(TlvError::BufferTooShort(10, 15))));
    }

    #[test]
    fn test_tlv_is_valid() {
        let valid_tlv = Tlv::new(0x0002, 0x01, b"test".to_vec());
        assert!(valid_tlv.is_valid());

        let mut invalid_tlv = valid_tlv.clone();
        invalid_tlv.length = 99; // Mismatch with actual value length
        assert!(!invalid_tlv.is_valid());
    }
}
