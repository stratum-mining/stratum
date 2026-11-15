//! TLV list for managing collections of TLV fields.

use super::{iter::TlvIter, Tlv, TlvError};
use crate::binary_sv2::{GetSize, Serialize};
use alloc::vec::Vec;
use framing_sv2::{SV2_FRAME_HEADER_SIZE, SV2_FRAME_MAX_PAYLOAD_SIZE};

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
    /// the extension type, message type and channel bit using the `IsSv2Message` trait.
    ///
    /// The result can be converted to a `SerializedFrame` using
    /// `SerializedFrame::from_bytes()`.
    /// # Example
    /// ```ignore
    /// use parsers_sv2::{TlvList, Tlv, Mining};
    ///
    /// let message = Mining::SubmitSharesExtended(/* ... */);
    /// let tlv = Tlv::new(0x0002, 0x01, b"Worker_001".to_vec()).unwrap();
    /// let tlv_list = TlvList::from_slice(&[tlv]);
    /// let frame_bytes = tlv_list.build_frame_bytes_with_tlvs(message).unwrap();
    /// ```
    pub fn build_frame_bytes_with_tlvs<T>(&self, message: T) -> Result<Vec<u8>, TlvError>
    where
        T: Serialize + GetSize + crate::IsSv2Message,
    {
        // Extract message metadata using IsSv2Message trait. Bit 15 of the
        // header's extension_type is the channel_msg flag, so it is taken from
        // channel_bit() only and cleared from extension_type().
        let extension_type =
            (message.extension_type() & 0x7fff) | ((message.channel_bit() as u16) << 15);
        let message_type = message.message_type();

        // Serialize the base message
        let mut payload = crate::binary_sv2::to_bytes(message).map_err(TlvError::EncodingError)?;

        // Append TLV data directly from bytes
        let tlv_bytes: &[u8] = match self {
            TlvList::Bytes(data) => data,
            TlvList::Owned(bytes) => bytes.as_slice(),
        };
        payload.extend_from_slice(tlv_bytes);

        // Reject frames whose payload does not fit the 24-bit length field.
        let payload_len = payload.len();
        if payload_len > SV2_FRAME_MAX_PAYLOAD_SIZE {
            return Err(TlvError::FramePayloadTooLong(payload_len));
        }

        // Calculate payload length
        let msg_length = payload_len as u32;

        // Construct complete frame bytes (header + payload)
        let mut frame_bytes = Vec::with_capacity(SV2_FRAME_HEADER_SIZE + payload.len());
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
    use crate::{Extensions, ExtensionsNegotiation, IsSv2Message, Mining, SubmitSharesStandard};
    use alloc::vec;
    use binary_sv2::Seq064K;
    use extensions_sv2::{
        RequestExtensions, EXTENSION_TYPE_EXTENSIONS_NEGOTIATION, MESSAGE_TYPE_REQUEST_EXTENSIONS,
    };
    use framing_sv2::header::Header;

    #[derive(Serialize)]
    struct ChannelBitInExtensionType {
        request_id: u16,
    }

    impl IsSv2Message for ChannelBitInExtensionType {
        fn message_type(&self) -> u8 {
            0
        }
        fn channel_bit(&self) -> bool {
            false
        }
        fn extension_type(&self) -> u16 {
            0x8001
        }
    }

    #[test]
    fn test_build_frame_bytes_with_tlvs_preserves_extension_type() {
        let message = Extensions::ExtensionsNegotiation(ExtensionsNegotiation::RequestExtensions(
            RequestExtensions {
                request_id: 0,
                requested_extensions: Seq064K::new(vec![2]).unwrap(),
            },
        ));
        let tlv = Tlv::new(0x0002, 0x01, b"worker1".to_vec()).unwrap();
        let list = TlvList::from_slice(&[tlv.clone()]).unwrap();

        let frame = list.build_frame_bytes_with_tlvs(message).unwrap();
        let header = Header::from_bytes(&frame).unwrap();

        assert_eq!(header.ext_type(), EXTENSION_TYPE_EXTENSIONS_NEGOTIATION);
        assert_eq!(header.msg_type(), MESSAGE_TYPE_REQUEST_EXTENSIONS);
        assert_eq!(header.payload_length(), frame.len() - Header::SIZE);
        assert!(frame.ends_with(&tlv.encode().unwrap()));
    }

    #[test]
    fn test_build_frame_bytes_with_tlvs_sets_channel_bit() {
        let message = Mining::SubmitSharesStandard(SubmitSharesStandard {
            channel_id: 1,
            sequence_number: 0,
            job_id: 0,
            nonce: 0,
            ntime: 0,
            version: 0,
        });
        let list = TlvList::from_bytes(&[] as &[u8]);

        let frame = list.build_frame_bytes_with_tlvs(message).unwrap();
        let header = Header::from_bytes(&frame).unwrap();

        assert!(header.channel_msg());
        assert_eq!(header.ext_type_without_channel_msg(), 0);
    }

    #[test]
    fn test_build_frame_bytes_with_tlvs_takes_channel_bit_from_channel_bit_only() {
        let list = TlvList::from_bytes(&[] as &[u8]);

        let frame = list
            .build_frame_bytes_with_tlvs(ChannelBitInExtensionType { request_id: 0 })
            .unwrap();
        let header = Header::from_bytes(&frame).unwrap();

        assert_eq!(header.ext_type(), 0x0001);
    }

    #[test]
    fn test_tlv_list_from_bytes() {
        // Create buffer with two TLVs
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec()).unwrap();
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec()).unwrap();

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
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec()).unwrap();
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec()).unwrap();

        let list = TlvList::from_slice(&[tlv1.clone(), tlv2.clone()]).unwrap();
        let tlvs = list.to_vec();

        assert_eq!(tlvs.len(), 2);
        assert_eq!(tlvs[0], tlv1);
        assert_eq!(tlvs[1], tlv2);
    }

    #[test]
    fn test_tlv_list_iter() {
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec()).unwrap();
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec()).unwrap();

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
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec()).unwrap();
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec()).unwrap();
        let tlv3 = Tlv::new(0x0002, 0x02, b"other".to_vec()).unwrap();

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
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec()).unwrap();
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec()).unwrap();

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
    fn test_build_frame_bytes_with_tlvs_enforces_max_frame_payload_size() {
        use crate::{ChannelEndpointChanged, CommonMessagesOwned};

        // size is 4 byte (u32)
        let message =
            CommonMessagesOwned::ChannelEndpointChanged(ChannelEndpointChanged { channel_id: 0 });

        // Exactly the maximum payload size should succeed.
        let tlv_bytes = vec![0u8; SV2_FRAME_MAX_PAYLOAD_SIZE - message.get_size()];
        let tlv_list = TlvList::from_bytes(&tlv_bytes);

        let frame = tlv_list
            .build_frame_bytes_with_tlvs(message.clone())
            .expect("maximum frame payload should be accepted");

        let declared_length = u32::from_le_bytes([frame[3], frame[4], frame[5], 0]) as usize;

        assert_eq!(declared_length, SV2_FRAME_MAX_PAYLOAD_SIZE);
        assert_eq!(
            frame.len(),
            SV2_FRAME_HEADER_SIZE + SV2_FRAME_MAX_PAYLOAD_SIZE
        );

        // Exceeding the maximum by one byte should return an error.
        let tlv_bytes = vec![0u8; SV2_FRAME_MAX_PAYLOAD_SIZE - message.get_size() + 1];
        let tlv_list = TlvList::from_bytes(&tlv_bytes);

        assert!(matches!(
            tlv_list.build_frame_bytes_with_tlvs(message),
            Err(TlvError::FramePayloadTooLong(len))
                if len == SV2_FRAME_MAX_PAYLOAD_SIZE + 1
        ));
    }
}
