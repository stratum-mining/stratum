//! # Sv2 Frame Header
//!
//! Defines the [`crate::header::Header`] structure used in the framing of Sv2 messages.
//!
//! Each [`crate::framing::Sv2Frame`] starts with a 6-byte header with information about the
//! message payload, including its extension type, if it is associated with a specific mining
//! channel, the type of message (e.g. `SetupConnection`, `NewMiningJob`, etc.) and the payload
//! length.
//!
//! ## Header Structure
//!
//! The Sv2 header includes the following fields:
//!
//! - `extension_type`: A `16`-bit field that describes the protocol extension associated with the
//!   message. It also contains a special bit (the `channel_msg` bit) to indicate if the message is
//!   tied to a specific channel.
//! - `msg_type`: An `8`-bit field representing the specific message type within the given
//!   extension.
//! - `msg_length`: A `24`-bit field that indicates the length of the message payload, excluding the
//!   header itself.

use crate::Error;
use alloc::vec::Vec;
use binary_sv2::{self, Deserialize, Serialize, U24};
use core::convert::TryInto;

use crate::{SV2_FRAME_CHUNK_SIZE, SV2_FRAME_HEADER_SIZE};
use noise_sv2::AEAD_MAC_LEN;

/// Abstraction for a Sv2 Frame Header.
#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub struct Header {
    // Unique identifier of the extension describing this protocol message.  Most significant bit
    // (i.e.bit 15, 0-indexed, aka channel_msg) indicates a message which is specific to a channel,
    // whereas if the most significant bit is unset, the message is to be interpreted by the
    // immediate receiving device.  Note that the channel_msg bit is ignored in the extension
    // lookup, i.e.an extension_type of 0x8ABC is for the same "extension" as 0x0ABC.  If the
    // channel_msg bit is set, the first four bytes of the payload field is a U32 representing the
    // channel_id this message is destined for. Note that for the Job Declaration and Template
    // Distribution Protocols the channel_msg bit is always unset.
    extension_type: u16, // fix: use U16 type
    //
    // Unique identifier of the extension describing this protocol message
    msg_type: u8, // fix: use specific type?

    // Length of the protocol message, not including this header
    msg_length: U24,
}

impl Header {
    pub const SIZE: usize = SV2_FRAME_HEADER_SIZE;
    const CHANNEL_MSG_MASK: u16 = 0b1000_0000_0000_0000;

    /// Construct a [`Header`] from raw bytes
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < Self::SIZE {
            return Err(Error::UnexpectedHeaderLength(bytes.len() as isize));
        };
        let extension_type = u16::from_le_bytes([bytes[0], bytes[1]]);
        let msg_type = bytes[2];
        let msg_length: U24 = u32::from_le_bytes([bytes[3], bytes[4], bytes[5], 0]).try_into()?;
        Ok(Self {
            extension_type,
            msg_type,
            msg_length,
        })
    }

    // Get the payload length
    #[allow(clippy::len_without_is_empty)]
    #[inline]
    pub(crate) fn len(&self) -> usize {
        let inner: u32 = self.msg_length.into();
        inner as usize
    }

    // Construct a [`Header`] from payload length, type and extension type.
    #[inline]
    pub(crate) fn from_len(msg_length: u32, msg_type: u8, extension_type: u16) -> Option<Header> {
        Some(Self {
            extension_type,
            msg_type,
            msg_length: msg_length.try_into().ok()?,
        })
    }

    /// Get the [`Header`] message type.
    pub fn msg_type(&self) -> u8 {
        self.msg_type
    }

    /// Get the [`Header`] extension type.
    pub fn ext_type(&self) -> u16 {
        self.extension_type
    }

    /// Get the [`Header`] extension type without the channel message bit (i.e. bit 15).
    pub fn ext_type_without_channel_msg(&self) -> u16 {
        self.extension_type & !Self::CHANNEL_MSG_MASK
    }

    /// Check if [`Header`] represents a channel message.
    ///
    /// A header can represent a channel message if the MSB(Most Significant Bit) is set.
    pub fn channel_msg(&self) -> bool {
        self.extension_type & Self::CHANNEL_MSG_MASK != 0
    }

    /// Calculates the total length of a chunked message, accounting for MAC overhead.
    ///
    /// Determines the total length of the message frame, including the overhead introduced by
    /// MACs. If the message is split into multiple chunks (due to its size exceeding the maximum
    /// frame chunk size), each chunk requires a MAC for integrity verification.
    ///
    /// This method is particularly relevant when the message is being encrypted using the Noise
    /// protocol, where the payload is divided into encrypted chunks, and each chunk is appended
    /// with a MAC. However, it can also be applied to non-encrypted chunked messages to calculate
    /// their total length.
    ///
    /// The calculated length includes the full payload length and any additional space required
    /// for the MACs.
    #[allow(clippy::manual_div_ceil)]
    pub fn encrypted_len(&self) -> usize {
        let len = self.len();
        let payload_per_chunk = SV2_FRAME_CHUNK_SIZE - AEAD_MAC_LEN;

        let chunks = (len + payload_per_chunk - 1) / payload_per_chunk;
        len + chunks * AEAD_MAC_LEN
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;

    #[test]
    fn test_header_from_bytes() {
        let bytes = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let header = Header::from_bytes(&bytes).unwrap();
        assert_eq!(header.extension_type, 0x0201);
        assert_eq!(header.msg_type, 0x03);
        assert_eq!(header.msg_length, 0x060504_u32.try_into().unwrap());
    }

    #[test]
    fn test_header_from_len() {
        let header = Header::from_len(0x1234, 0x56, 0x789a).unwrap();
        assert_eq!(header.extension_type, 0x789a);
        assert_eq!(header.msg_type, 0x56);
        assert_eq!(header.msg_length, 0x1234_u32.try_into().unwrap());

        let extension_type = 0;
        let msg_type = 0x1;
        let msg_length = 0x1234_u32;
        let header = Header::from_len(msg_length, msg_type, extension_type).unwrap();
        assert_eq!(header.extension_type, 0);
        assert_eq!(header.msg_type, 0x1);
        assert_eq!(header.msg_length, 0x1234_u32.try_into().unwrap());
    }

    #[derive(Debug, Clone)]
    struct ValidU24(u32);

    impl Arbitrary for ValidU24 {
        fn arbitrary(g: &mut Gen) -> Self {
            ValidU24(u32::arbitrary(g) % 16_777_216)
        }
    }

    #[quickcheck]
    fn prop_header_serialization_roundtrip(
        msg_length: ValidU24,
        msg_type: u8,
        extension_type: u16,
    ) {
        let header = Header::from_len(msg_length.0, msg_type, extension_type).unwrap();
        let mut bytes = vec![0u8; SV2_FRAME_HEADER_SIZE];
        if binary_sv2::to_writer(header, &mut bytes[..]).is_err() {
            return;
        }

        let deserialized = Header::from_bytes(&bytes).expect("Failed to deserialize header");
        assert_eq!(
            deserialized.msg_type(),
            msg_type,
            "Message type mismatch after roundtrip"
        );
        assert_eq!(
            deserialized.ext_type(),
            extension_type,
            "Extension type mismatch after roundtrip"
        );

        let len: u32 = deserialized.msg_length.into();
        assert_eq!(len, msg_length.0, "Message length mismatch after roundtrip");
    }

    #[quickcheck]
    fn prop_header_from_bytes_size_requirement(bytes: Vec<u8>) {
        let result = Header::from_bytes(&bytes);

        if bytes.len() < SV2_FRAME_HEADER_SIZE {
            assert!(
                matches!(result, Err(Error::UnexpectedHeaderLength(_))),
                "Expected UnexpectedHeaderLength error for buffer with {} bytes, got {:?}",
                bytes.len(),
                result
            );
        } else {
            assert!(
                result.is_ok() || !matches!(result, Err(Error::UnexpectedHeaderLength(_))),
                "Got unexpected UnexpectedHeaderLength error for sufficient buffer size"
            );
        }
    }

    #[quickcheck]
    fn prop_header_channel_msg_bit(extension_type: u16, channel_msg: bool) {
        let msg_length = 100u32;
        let msg_type = 0x01u8;

        let adjusted_extension_type = if channel_msg {
            extension_type | 0b1000_0000_0000_0000
        } else {
            extension_type & 0b0111_1111_1111_1111
        };

        let header = Header::from_len(msg_length, msg_type, adjusted_extension_type).unwrap();

        assert_eq!(
            header.channel_msg(),
            channel_msg,
            "channel_msg() should return {} for extension_type 0x{:04X} with MSB {}",
            channel_msg,
            adjusted_extension_type,
            if channel_msg { "set" } else { "unset" }
        );
    }

    #[quickcheck]
    fn prop_header_ext_type_without_channel_msg(extension_type: u16) {
        let msg_length = 100u32;
        let msg_type = 0x01u8;

        let header = Header::from_len(msg_length, msg_type, extension_type).unwrap();

        let without_channel = header.ext_type_without_channel_msg();
        let expected = extension_type & 0b0111_1111_1111_1111;
        assert_eq!(
            without_channel, expected,
            "ext_type_without_channel_msg() should clear MSB: got 0x{:04X}, expected 0x{:04X}",
            without_channel, expected
        );
    }

    #[quickcheck]
    fn prop_header_encrypted_len_calculation(msg_length: ValidU24) {
        let header = Header::from_len(msg_length.0, 0x01, 0x0000).unwrap();

        let encrypted_len = header.encrypted_len();
        let aead_mac_len = AEAD_MAC_LEN;
        let payload_per_chunk = SV2_FRAME_CHUNK_SIZE - aead_mac_len;
        let chunks = (msg_length.0 as usize + payload_per_chunk - 1) / payload_per_chunk;
        let expected_len = msg_length.0 as usize + chunks * aead_mac_len;

        assert_eq!(
            encrypted_len, expected_len,
            "encrypted_len() mismatch for msg_length={}: {} chunks, expected {} bytes, got {} bytes",
            msg_length.0, chunks, expected_len, encrypted_len
        );
    }

    #[quickcheck]
    fn prop_header_len_consistency(msg_length: ValidU24) {
        let header = Header::from_len(msg_length.0, 0x01, 0x0000).unwrap();

        assert_eq!(
            header.len(),
            msg_length.0 as usize,
            "Header len() should match the msg_length used to create it"
        );
    }

    #[quickcheck]
    fn prop_header_size_is_constant() {
        assert_eq!(
            Header::SIZE,
            SV2_FRAME_HEADER_SIZE,
            "Header::SIZE should equal SV2_FRAME_HEADER_SIZE"
        );
        assert_eq!(
            SV2_FRAME_HEADER_SIZE, 6,
            "SV2_FRAME_HEADER_SIZE should be 6 bytes"
        );
    }
}
