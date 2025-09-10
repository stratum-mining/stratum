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
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize, U24};
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

    /// Get the [`Header`[ extension type.
    pub fn ext_type(&self) -> u16 {
        self.extension_type
    }

    /// Check if [`Header`] represents a channel message.
    ///
    /// A header can represent a channel message if the MSB(Most Significant Bit) is set.
    pub fn channel_msg(&self) -> bool {
        const CHANNEL_MSG_MASK: u16 = 0b0000_0000_0000_0001;
        self.extension_type & CHANNEL_MSG_MASK == self.extension_type
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
    pub fn encrypted_len(&self) -> usize {
        let len = self.len();
        let mut chunks = len / (SV2_FRAME_CHUNK_SIZE - AEAD_MAC_LEN);
        if len % (SV2_FRAME_CHUNK_SIZE - AEAD_MAC_LEN) != 0 {
            chunks += 1;
        }
        let mac_len = chunks * AEAD_MAC_LEN;
        len + mac_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

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
}
