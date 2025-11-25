//! TLV list for managing collections of TLV fields.

use super::{iter::TlvIter, Tlv, TlvError};
use crate::binary_sv2::{GetSize, Serialize};
use alloc::vec::Vec;

extern crate alloc;

/// A collection of TLV fields that can be used for both parsing and building.
///
/// `TlvList` supports two modes:
/// 1. **Parsing mode**: Created from a byte buffer, iterates over TLVs lazily
/// 2. **Building mode**: Created from a Vec of TLVs, encoded to bytes for building
///
/// Both variants represent raw TLV-encoded bytes. The iterator performs lazy decoding,
/// operating over the byte slice rather than over `Tlv` values.
#[derive(Debug, Clone)]
pub enum TlvList<'a> {
    /// TLV list backed by borrowed raw bytes
    Bytes(&'a [u8]),
    /// TLV list backed by owned raw bytes
    Owned(Vec<u8>),
}

impl<'a> TlvList<'a> {
    /// Creates a new TLV list from a byte buffer (parsing mode).
    pub fn from_bytes<T: AsRef<[u8]> + ?Sized>(data: &'a T) -> Self {
        Self::Bytes(data.as_ref())
    }

    /// Creates a new TLV list from a slice of TLVs (building mode).
    ///
    /// Encodes the TLVs to raw bytes and stores them in an owned vector.
    pub fn from_slice(tlvs: &[Tlv]) -> Result<Self, TlvError> {
        let mut bytes = Vec::new();
        for tlv in tlvs {
            bytes.extend_from_slice(&tlv.encode()?);
        }
        Ok(Self::Owned(bytes))
    }

    /// Returns an iterator over the TLV fields in this list.
    ///
    /// This lazily decodes TLVs from the raw byte representation.
    pub fn iter(&self) -> impl Iterator<Item = Result<Tlv, TlvError>> + '_ {
        match self {
            TlvList::Bytes(data) => TlvIter::new(data),
            TlvList::Owned(bytes) => TlvIter::new(bytes),
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
                Ok(tlv) if negotiated.contains(&tlv.r#type.extension_type) => Some(tlv),
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
                if tlv.r#type.extension_type == extension_type
                    && tlv.r#type.field_type == field_type =>
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
        let mut payload = crate::binary_sv2::to_bytes(message).map_err(TlvError::EncodingError)?;

        // Append TLV data directly from bytes
        let tlv_bytes: &[u8] = match self {
            TlvList::Bytes(data) => data,
            TlvList::Owned(bytes) => bytes.as_slice(),
        };
        payload.extend_from_slice(tlv_bytes);

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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

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

        let list = TlvList::from_slice(&[tlv1.clone(), tlv2.clone()]).unwrap();
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
}
