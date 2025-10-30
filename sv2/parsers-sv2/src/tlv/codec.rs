//! TLV (Type-Length-Value) encoding and decoding implementation.
//!
//! This module provides the core functionality for working with TLV fields in Stratum V2 extensions.

use super::TlvError;
use crate::binary_sv2::{self, Deserialize, GetSize, Serialize};
use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt::{self, Write};

extern crate alloc;

/// TLV header size in bytes (3 bytes Type + 2 bytes Length)
pub const TLV_HEADER_SIZE: usize = 5;

/// Trait for types that can be encoded/decoded as TLV fields.
///
/// This trait provides a common interface for all extension field types,
/// encapsulating the extension-specific logic for encoding and decoding.
///
/// All TLV field implementations use `ParserError` for unified error handling,
/// which can represent both generic TLV errors and extension-specific validation errors.
///
/// # Example
/// ```ignore
/// impl TlvField for UserIdentity {
///     const EXTENSION_TYPE: u16 = 0x0002;
///     const FIELD_TYPE: u8 = 0x01;
///     
///     fn from_bytes(bytes: &[u8]) -> Result<Tlv, ParserError> {
///         // Decode implementation - TlvError auto-converts to ParserError
///         Tlv::decode(bytes).map_err(Into::into)
///     }
///     
///     fn to_bytes(&self) -> Result<Vec<u8>, ParserError> {
///         // Encode implementation
///         let tlv = self.to_tlv()?;
///         tlv.encode().map_err(Into::into)
///     }
///     
///     fn from_tlv(tlv: &Tlv) -> Result<Self, ParserError> {
///         // Extension-specific validation - ExtensionError auto-converts to ParserError
///         if tlv.type_.extension_type != Self::EXTENSION_TYPE {
///             return Err(UserIdentityError::InvalidExtensionType(tlv.type_.extension_type).into());
///         }
///         // ...
///     }
///     
///     fn to_tlv(&self) -> Result<Tlv, ParserError> {
///         // Convert from Self to Tlv
///     }
/// }
/// ```
pub trait TlvField: Sized {
    /// The extension type this field belongs to.
    const EXTENSION_TYPE: u16;

    /// The field type within the extension.
    const FIELD_TYPE: u8;

    /// Decodes a TLV from raw bytes.
    ///
    /// This parses the TLV structure (Type-Length-Value) from the byte buffer.
    fn from_bytes(bytes: &[u8]) -> Result<Tlv, crate::ParserError>;

    /// Encodes this field as raw TLV bytes.
    ///
    /// Returns the complete TLV bytes including Type, Length, and Value fields.
    fn to_bytes(&self) -> Result<Vec<u8>, crate::ParserError>;

    /// Creates an instance of this field from a parsed TLV.
    ///
    /// This validates the TLV type matches this field's extension and field type,
    /// then extracts and parses the value.
    fn from_tlv(tlv: &Tlv) -> Result<Self, crate::ParserError>;

    /// Converts this field into a TLV structure.
    ///
    /// This creates a TLV with the correct extension_type and field_type for this field.
    fn to_tlv(&self) -> Result<Tlv, crate::ParserError>;
}

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
    pub type_: Type,
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
            type_: Type::new(extension_type, field_type),
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
        let type_ = Type::new(extension_type, field_type);

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
            type_,
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
        let type_bytes = binary_sv2::to_bytes(self.type_).map_err(TlvError::EncodingError)?;
        bytes.extend_from_slice(&type_bytes);

        // Encode Length (2 bytes, little-endian)
        bytes.extend_from_slice(&self.length.to_le_bytes());

        // Append Value
        bytes.extend_from_slice(&self.value);

        Ok(bytes)
    }

    /// Returns the total encoded size of this TLV in bytes.
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
            self.type_.extension_type,
            self.type_.field_type,
            self.length,
            value_to_string(&self.value)
        )
    }
}

/// Iterator over TLV fields in a byte buffer.
///
/// Lazily decodes TLV fields as the iterator is consumed. Stops iteration
/// on the first decode error or when no more complete TLV headers can be read.
pub struct TlvIter<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> TlvIter<'a> {
    /// Creates a new TLV iterator over the provided data.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }
}

impl<'a> Iterator for TlvIter<'a> {
    type Item = Result<Tlv, TlvError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if we have enough data for a TLV header
        if self.offset + TLV_HEADER_SIZE > self.data.len() {
            return None;
        }

        match Tlv::decode(&self.data[self.offset..]) {
            Ok(tlv) => {
                let size = tlv.encoded_size();
                self.offset += size;
                Some(Ok(tlv))
            }
            Err(e) => {
                // Stop iteration on error by advancing to end
                self.offset = self.data.len();
                Some(Err(e))
            }
        }
    }
}

/// A collection of TLV fields that can be used for both parsing and building.
///
/// `TlvList` supports two modes:
/// 1. **Parsing mode**: Created from a byte buffer, iterates over TLVs lazily
/// 2. **Building mode**: Created from a Vec of TLVs, used to build frame bytes
///
/// This provides a unified interface for working with TLV fields in both directions.
#[derive(Debug, Clone)]
pub enum TlvList<'a> {
    /// TLV list backed by raw bytes (for parsing)
    Bytes(&'a [u8]),
    /// TLV list backed by a vector of TLVs (for building)
    Owned(Vec<Tlv>),
}

impl<'a> TlvList<'a> {
    /// Creates a new TLV list from a byte buffer (parsing mode).
    pub fn from_bytes<T: AsRef<[u8]> + ?Sized>(data: &'a T) -> Self {
        Self::Bytes(data.as_ref())
    }

    /// Creates a new TLV list from a slice of TLVs (building mode).
    pub fn from_slice(tlvs: &[Tlv]) -> Self {
        Self::Owned(tlvs.to_vec())
    }

    /// Returns an iterator over the TLV fields in this list.
    ///
    /// For byte-backed lists, this lazily decodes TLVs.
    /// For owned lists, this iterates over the stored TLVs.
    pub fn iter(&self) -> Box<dyn Iterator<Item = Result<Tlv, TlvError>> + '_> {
        match self {
            TlvList::Bytes(data) => Box::new(TlvIter::new(data)),
            TlvList::Owned(tlvs) => Box::new(tlvs.iter().map(|t| Ok(t.clone()))),
        }
    }

    /// Collects all valid TLVs into a vector, skipping any decode errors.
    pub fn to_vec(&self) -> Vec<Tlv> {
        self.iter().filter_map(|r| r.ok()).collect()
    }

    /// Collects TLVs that match the negotiated extension types.
    ///
    /// Filters TLVs by extension_type, keeping only those present in the
    /// `negotiated` list. Skips any decode errors.
    pub fn for_extensions(&self, negotiated: &[u16]) -> Vec<Tlv> {
        self.iter()
            .filter_map(|r| match r {
                Ok(tlv) if negotiated.contains(&tlv.type_.extension_type) => Some(tlv),
                _ => None,
            })
            .collect()
    }

    /// Finds the first TLV matching the specified extension and field type.
    ///
    /// Returns `None` if no matching TLV is found or if decode errors are encountered.
    pub fn find(&self, extension_type: u16, field_type: u8) -> Option<Tlv> {
        self.iter().find_map(|r| match r {
            Ok(tlv)
                if tlv.type_.extension_type == extension_type
                    && tlv.type_.field_type == field_type =>
            {
                Some(tlv)
            }
            _ => None,
        })
    }

    /// Builds complete SV2 frame bytes from a message with TLV fields appended.
    ///
    /// This method serializes an SV2 message and appends the TLV fields from this list,
    /// producing complete frame bytes ready for transmission. It automatically extracts
    /// the message type and channel bit using the `IsSv2Message` trait.
    ///
    /// The result can be converted to a `StandardSv2Frame` using `StandardSv2Frame::from_bytes()`.
    /// # Example
    /// ```ignore
    /// use parsers_sv2::{TlvList, Tlv, Mining};
    ///
    /// let message = Mining::SubmitSharesExtended(/* ... */);
    /// let tlv = Tlv::new(0x0002, 0x01, b"Worker_001".to_vec());
    /// let tlv_list = TlvList::from_slice(&[tlv]);
    /// let frame_bytes = tlv_list.build_frame_bytes_with_tlvs(message).unwrap();
    /// ```
    pub fn build_frame_bytes_with_tlvs<T>(&self, message: T) -> Result<Vec<u8>, TlvError>
    where
        T: Serialize + GetSize + crate::IsSv2Message,
    {
        // Extract message metadata using IsSv2Message trait
        let extension_type: u16 = (message.channel_bit() as u16) << 15;
        let message_type = message.message_type();

        // Serialize the base message
        let mut payload = binary_sv2::to_bytes(message).map_err(TlvError::EncodingError)?;

        // Append TLV data using iterator with flatten
        for tlv in self.iter().flatten() {
            payload.extend_from_slice(&tlv.encode()?);
        }

        // Calculate payload length
        let msg_length = payload.len() as u32;

        // Construct complete frame bytes (header + payload)
        let header_size = 6; // 2 bytes ext_type + 1 byte msg_type + 3 bytes length
        let mut frame_bytes = Vec::with_capacity(header_size + payload.len());
        frame_bytes.extend_from_slice(&extension_type.to_le_bytes());
        frame_bytes.push(message_type);
        frame_bytes.extend_from_slice(&msg_length.to_le_bytes()[..3]);
        frame_bytes.extend_from_slice(&payload);

        Ok(frame_bytes)
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

        assert_eq!(tlv.type_.extension_type, 0x0002);
        assert_eq!(tlv.type_.field_type, 0x01);
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
    fn test_tlv_list_from_bytes() {
        // Create buffer with two TLVs
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec());
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec());

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&tlv1.encode().unwrap());
        buffer.extend_from_slice(&tlv2.encode().unwrap());

        let list = TlvList::from_bytes(&buffer);
        let tlvs = list.to_vec();

        assert_eq!(tlvs.len(), 2);
        assert_eq!(tlvs[0], tlv1);
        assert_eq!(tlvs[1], tlv2);
    }

    #[test]
    fn test_tlv_list_from_slice() {
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec());
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec());

        let list = TlvList::from_slice(&[tlv1.clone(), tlv2.clone()]);
        let tlvs = list.to_vec();

        assert_eq!(tlvs.len(), 2);
        assert_eq!(tlvs[0], tlv1);
        assert_eq!(tlvs[1], tlv2);
    }

    #[test]
    fn test_tlv_list_iter() {
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec());
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec());

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&tlv1.encode().unwrap());
        buffer.extend_from_slice(&tlv2.encode().unwrap());

        let list = TlvList::from_bytes(&buffer);
        let mut iter = list.iter();

        assert_eq!(iter.next().unwrap().unwrap(), tlv1);
        assert_eq!(iter.next().unwrap().unwrap(), tlv2);
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_tlv_list_for_extensions() {
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec());
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec());
        let tlv3 = Tlv::new(0x0002, 0x02, b"other".to_vec());

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&tlv1.encode().unwrap());
        buffer.extend_from_slice(&tlv2.encode().unwrap());
        buffer.extend_from_slice(&tlv3.encode().unwrap());

        let list = TlvList::from_bytes(&buffer);
        let negotiated = vec![0x0002];
        let tlvs = list.for_extensions(&negotiated);

        assert_eq!(tlvs.len(), 2);
        assert_eq!(tlvs[0], tlv1);
        assert_eq!(tlvs[1], tlv3);
    }

    #[test]
    fn test_tlv_list_find() {
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec());
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec());

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&tlv1.encode().unwrap());
        buffer.extend_from_slice(&tlv2.encode().unwrap());

        let list = TlvList::from_bytes(&buffer);

        let found = list.find(0x0003, 0x01);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), tlv2);

        let not_found = list.find(0x9999, 0x99);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_tlv_list_empty() {
        let list = TlvList::from_bytes(&[] as &[u8]);
        let tlvs = list.to_vec();
        assert_eq!(tlvs.len(), 0);
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
