use serde::{Deserialize, Serialize};
use serde_sv2::{from_bytes, to_writer, Bytes as Sv2Bytes, GetLen, U16, U24, U8};
use std::convert::{TryFrom, TryInto};
use std::io::Write;

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub struct Header {
    extesion_type: U16, // TODO use specific type?
    msg_type: U8,       // TODO use specific type?
    msg_length: U24,
}

impl Header {
    pub const LEN_OFFSET: usize = const_sv2::SV2_FRAME_HEADER_LEN_OFFSET;
    pub const LEN_SIZE: usize = const_sv2::SV2_FRAME_HEADER_LEN_END;
    pub const LEN_END: usize = Self::LEN_OFFSET + Self::LEN_SIZE;

    pub const SIZE: usize = const_sv2::SV2_FRAME_HEADER_SIZE;

    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, isize> {
        if bytes.len() < Self::SIZE {
            return Err((Self::SIZE - bytes.len()) as isize);
        };

        // TODO remove hardcoded
        let extesion_type = u16::from_le_bytes([bytes[0], bytes[1]]);
        let msg_type = bytes[2];
        let msg_length = u32::from_le_bytes([bytes[3], bytes[4], bytes[5], 0]);

        Ok(Self {
            extesion_type,
            msg_type,
            // TODO
            msg_length: msg_length.try_into().unwrap(),
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        let inner: u32 = self.msg_length.into();
        inner as usize
    }

    #[inline]
    pub fn from_len(len: u32) -> Option<Header> {
        Some(Self {
            extesion_type: 7, //TODO
            msg_type: 9,      // TODO
            // TODO
            msg_length: len.try_into().unwrap(),
        })
    }

}

pub struct NoiseHeader {}

impl NoiseHeader {
    pub const SIZE: usize = const_sv2::NOISE_FRAME_HEADER_SIZE;
    pub const LEN_OFFSET: usize = const_sv2::NOISE_FRAME_HEADER_LEN_OFFSET;
    pub const LEN_END: usize = const_sv2::NOISE_FRAME_HEADER_LEN_END;
}
