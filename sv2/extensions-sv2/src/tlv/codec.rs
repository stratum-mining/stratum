use alloc::vec;
use alloc::vec::Vec;
use binary_sv2::{Deserialize, Encodable};

use super::error::TlvError;

/// Generic TLV (Type-Length-Value) encoder and decoder.
///
/// TLV fields are appended to the end of base protocol messages when extensions
/// are negotiated. The format is:
/// - Type (U16 | U8): Extension type (U16) + Field type (U8) = 3 bytes
/// - Length (U16): Length of the value in bytes = 2 bytes  
/// - Value: The actual field data = N bytes
///
/// Total overhead: 5 bytes + value length
///
/// TLV header size in bytes (3 bytes Type + 2 bytes Length)
pub const TLV_HEADER_SIZE: usize = 5;

/// A Type-Length-Value (TLV) field.
///
/// TLV fields are used to extend Stratum V2 messages with optional data
/// when extensions are negotiated between client and server.
///
/// Structure matches the Stratum V2 TLV specification:
/// - Type (3 bytes): extension_type (U16) + field_type (U8)
/// - Length (2 bytes): U16 indicating value size
/// - Value (N bytes): The actual data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tlv {
    /// The extension type (U16) - identifies which extension this TLV belongs to
    pub extension_type: u16,
    /// The field type (U8) - identifies the specific field within the extension
    pub field_type: u8,
    /// Length (U16) - size of the value field in bytes
    pub length: u16,
    /// The raw value bytes (length must match the length field)
    pub value: Vec<u8>,
}

impl Tlv {
    /// Creates a new TLV field.
    pub fn new(extension_type: u16, field_type: u8, value: Vec<u8>) -> Self {
        let length = value.len() as u16;
        Self {
            extension_type,
            field_type,
            length,
            value,
        }
    }

    /// Encodes the TLV into bytes using SV2 binary encoding.
    ///
    /// Returns the complete TLV bytes according to SV2 encoding.
    pub fn encode(&self) -> Result<Vec<u8>, TlvError> {
        let total_size = TLV_HEADER_SIZE + self.value.len();
        let mut tlv = vec![0u8; total_size];

        let mut offset = 0;

        self.extension_type
            .to_bytes(&mut tlv[offset..offset + 2])
            .map_err(|_| TlvError::EncodingError)?;
        offset += 2;

        self.field_type
            .to_bytes(&mut tlv[offset..offset + 1])
            .map_err(|_| TlvError::EncodingError)?;
        offset += 1;

        self.length
            .to_bytes(&mut tlv[offset..offset + 2])
            .map_err(|_| TlvError::EncodingError)?;
        offset += 2;

        tlv[offset..].copy_from_slice(&self.value);

        Ok(tlv)
    }

    /// Decodes a TLV from bytes using SV2 binary decoding.
    ///
    /// Accepts any type that can be converted to a byte slice via `AsRef<[u8]>`,
    /// including `&[u8]`, `Vec<u8>`, and `&Vec<u8>`.
    ///
    /// Returns `Ok(Tlv)` on success, or `Err(TlvError)` if the data is invalid.
    pub fn decode<T: AsRef<[u8]>>(data: T) -> Result<Self, TlvError> {
        let data = data.as_ref();

        // Check minimum TLV header size
        if data.len() < TLV_HEADER_SIZE {
            return Err(TlvError::BufferTooShort(data.len(), TLV_HEADER_SIZE));
        }

        let mut offset = 0;

        let mut ext_type_bytes = [data[offset], data[offset + 1]];
        let extension_type =
            u16::from_bytes(&mut ext_type_bytes).map_err(|_| TlvError::DecodingError)?;
        offset += 2;

        let mut field_type_bytes = [data[offset]];
        let field_type =
            u8::from_bytes(&mut field_type_bytes).map_err(|_| TlvError::DecodingError)?;
        offset += 1;

        let mut length_bytes = [data[offset], data[offset + 1]];
        let length = u16::from_bytes(&mut length_bytes).map_err(|_| TlvError::DecodingError)?;
        offset += 2;

        // Check if we have enough data for the complete TLV
        let total_size = TLV_HEADER_SIZE + length as usize;
        if data.len() < total_size {
            return Err(TlvError::BufferTooShort(data.len(), total_size));
        }

        // Parse Value - raw bytes
        let value = data[offset..offset + length as usize].to_vec();

        Ok(Self {
            extension_type,
            field_type,
            length,
            value,
        })
    }

    /// Returns the total size of this TLV when encoded (header + value).
    pub fn encoded_size(&self) -> usize {
        TLV_HEADER_SIZE + self.value.len()
    }

    /// Validates that the length field matches the actual value length.
    ///
    /// This is useful for ensuring TLV integrity after deserialization.
    pub fn is_valid(&self) -> bool {
        self.length as usize == self.value.len()
    }

    /// Parses multiple TLV fields from a buffer.
    ///
    /// Scans through the buffer and decodes all valid TLV fields into a vector.
    /// Stops at the first decoding error or end of buffer.
    pub fn parse_all<T: AsRef<[u8]>>(data: T) -> Vec<Self> {
        let data = data.as_ref();
        let mut tlvs = Vec::new();
        let mut offset = 0;

        while offset + TLV_HEADER_SIZE <= data.len() {
            match Self::decode(&data[offset..]) {
                Ok(tlv) => {
                    offset += tlv.encoded_size();
                    tlvs.push(tlv);
                }
                Err(_) => break, // Stop on first decode error
            }
        }

        tlvs
    }
}

/// Checks if data contains a TLV field related to the specified extension type.
pub fn has_tlv_for_extension<T: AsRef<[u8]>>(data: T, expected_extension_type: u16) -> bool {
    let data = data.as_ref();
    if data.len() < TLV_HEADER_SIZE {
        return false;
    }

    let mut ext_type_bytes = [data[0], data[1]];
    if let Ok(extension_type) = u16::from_bytes(&mut ext_type_bytes) {
        extension_type == expected_extension_type
    } else {
        false
    }
}

/// Validates if data contains TLV fields for any of the specified extension types.
///
/// This function scans through the TLV data and checks if any TLV field matches
/// one of the negotiated extension types. It's useful for handlers to validate
/// that TLV data is actually valid and contains known extensions.
pub fn has_valid_tlv_data<T: AsRef<[u8]>>(data: T, negotiated_extensions: &[u16]) -> bool {
    let data = data.as_ref();
    if data.len() < TLV_HEADER_SIZE {
        return false;
    }

    let mut offset = 0;

    // Scan through all TLV fields
    while offset + TLV_HEADER_SIZE <= data.len() {
        let mut ext_type_bytes = [data[offset], data[offset + 1]];
        let extension_type = match u16::from_bytes(&mut ext_type_bytes) {
            Ok(t) => t,
            Err(_) => return false,
        };

        let mut length_bytes = [data[offset + 3], data[offset + 4]];
        let length = match u16::from_bytes(&mut length_bytes) {
            Ok(l) => l as usize,
            Err(_) => return false,
        };

        let tlv_size = TLV_HEADER_SIZE + length;
        if offset + tlv_size > data.len() {
            // Malformed TLV (length extends beyond buffer)
            return false;
        }

        // Check if this is a negotiated extension
        if negotiated_extensions.contains(&extension_type) {
            return true;
        }

        offset += tlv_size;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_tlv_struct_encode_decode() {
        let value = b"Worker_001".to_vec();
        let extension_type = 0x0002;
        let field_type = 0x01;

        // Create and encode
        let tlv = Tlv::new(extension_type, field_type, value.clone());
        let encoded = tlv.encode().unwrap();

        // Verify structure (little-endian via binary_sv2)
        assert_eq!(encoded[0], 0x02); // extension_type low byte
        assert_eq!(encoded[1], 0x00); // extension_type high byte
        assert_eq!(encoded[2], 0x01); // field_type
        assert_eq!(encoded[3], 0x0a); // length low byte (10 bytes)
        assert_eq!(encoded[4], 0x00); // length high byte
        assert_eq!(&encoded[5..], &value[..]);

        // Decode
        let decoded_tlv = Tlv::decode(&encoded).unwrap();
        assert_eq!(decoded_tlv.extension_type, extension_type);
        assert_eq!(decoded_tlv.field_type, field_type);
        assert_eq!(decoded_tlv.value, value);

        // Test encoded_size
        assert_eq!(tlv.encoded_size(), TLV_HEADER_SIZE + value.len());

        // Test is_valid
        assert!(tlv.is_valid());
        assert!(decoded_tlv.is_valid());
    }

    #[test]
    fn test_encode_empty_value() {
        let tlv = Tlv::new(0x0001, 0x00, vec![]);
        let encoded = tlv.encode().unwrap();

        assert_eq!(encoded.len(), TLV_HEADER_SIZE);
        assert_eq!(encoded[3], 0x00); // length low byte
        assert_eq!(encoded[4], 0x00); // length high byte
    }

    #[test]
    fn test_decode_buffer_too_short() {
        let short_data = &[0x00, 0x02, 0x01]; // Only 3 bytes

        let result = Tlv::decode(short_data);
        assert!(matches!(result, Err(TlvError::BufferTooShort(3, 5))));
    }

    #[test]
    fn test_decode_incomplete_value() {
        let incomplete = &[0x02, 0x00, 0x01, 0x0a, 0x00, 0x01, 0x02]; // Says 10 bytes but only 2 (little-endian)

        let result = Tlv::decode(incomplete);
        assert!(matches!(result, Err(TlvError::BufferTooShort(7, 15))));
    }

    #[test]
    fn test_has_tlv_for_extension() {
        let tlv = Tlv::new(0x0002, 0x01, b"test".to_vec());
        let encoded = tlv.encode().unwrap();

        assert!(has_tlv_for_extension(&encoded, 0x0002));
        assert!(!has_tlv_for_extension(&encoded, 0x0001));
        assert!(!has_tlv_for_extension(&[], 0x0002));
        assert!(!has_tlv_for_extension(&[0x00], 0x0002));
    }

    #[test]
    fn test_has_valid_tlv_data() {
        // Single TLV for extension 0x0002
        let tlv1 = Tlv::new(0x0002, 0x01, b"test".to_vec());
        let encoded1 = tlv1.encode().unwrap();
        assert!(has_valid_tlv_data(&encoded1, &[0x0002]));
        assert!(!has_valid_tlv_data(&encoded1, &[0x0001]));
        assert!(!has_valid_tlv_data(&encoded1, &[]));

        // Multiple TLVs
        let tlv2 = Tlv::new(0x0002, 0x01, b"worker1".to_vec());
        let tlv3 = Tlv::new(0x0003, 0x01, b"data".to_vec());
        let mut multiple_tlvs = tlv2.encode().unwrap();
        multiple_tlvs.extend_from_slice(&tlv3.encode().unwrap());

        assert!(has_valid_tlv_data(&multiple_tlvs, &[0x0002]));
        assert!(has_valid_tlv_data(&multiple_tlvs, &[0x0003]));
        assert!(has_valid_tlv_data(&multiple_tlvs, &[0x0002, 0x0003]));
        assert!(!has_valid_tlv_data(&multiple_tlvs, &[0x0001]));

        // Malformed TLV (length exceeds buffer) - little-endian 0xFFFF
        let malformed = vec![0x02, 0x00, 0x01, 0xFF, 0xFF]; // Says 65535 bytes but no data
        assert!(!has_valid_tlv_data(&malformed, &[0x0002]));

        // Too short
        assert!(!has_valid_tlv_data(&[0x02, 0x00], &[0x0002]));
    }
}
