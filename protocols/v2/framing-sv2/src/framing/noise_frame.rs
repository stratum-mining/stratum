use crate::{framing::frame::Frame, header::NoiseHeader, Error};

use alloc::vec::Vec;

const NOISE_MAX_LEN: usize = const_sv2::NOISE_FRAME_MAX_SIZE;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;
#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

#[derive(Debug)]
pub struct NoiseFrame {
    payload: Slice,
}

impl NoiseFrame {
    pub fn get_payload_when_handshaking(&self) -> Vec<u8> {
        self.payload[0..].to_vec()
    }
}

impl<'a> Frame<'a, Slice> for NoiseFrame {
    type Buffer = Slice;
    type Deserialized = &'a mut [u8];

    /// Serialize the frame into dst if the frame is already serialized it just swap dst with
    /// itself
    #[inline]
    fn serialize(mut self, dst: &mut [u8]) -> Result<(), Error> {
        dst.swap_with_slice(self.payload.as_mut());
        Ok(())
    }

    #[inline]
    fn payload(&'a mut self) -> &'a mut [u8] {
        &mut self.payload[NoiseHeader::HEADER_SIZE..]
    }

    /// If is an Sv2 frame return the Some(header) if it is a noise frame return None
    fn get_header(&self) -> Option<crate::header::Header> {
        None
    }

    // For a NoiseFrame from_bytes is the same of from_bytes_unchecked
    fn from_bytes(bytes: Self::Buffer) -> Result<Self, isize> {
        Ok(Self::from_bytes_unchecked(bytes))
    }

    #[inline]
    fn from_bytes_unchecked(bytes: Self::Buffer) -> Self {
        Self { payload: bytes }
    }

    #[inline]
    fn size_hint(bytes: &[u8]) -> isize {
        if bytes.len() < NoiseHeader::HEADER_SIZE {
            return (NoiseHeader::HEADER_SIZE - bytes.len()) as isize;
        };

        let len_b = &bytes[NoiseHeader::LEN_OFFSET..NoiseHeader::HEADER_SIZE];
        let expected_len = u16::from_le_bytes([len_b[0], len_b[1]]) as usize;

        if bytes.len() - NoiseHeader::HEADER_SIZE == expected_len {
            0
        } else {
            expected_len as isize - (bytes.len() - NoiseHeader::HEADER_SIZE) as isize
        }
    }

    #[inline]
    fn encoded_length(&self) -> usize {
        self.payload.len()
    }

    /// Try to build a `Frame` frame from a serializable payload.
    /// It returns a Frame if the size of the payload fits in the frame, if not it returns None
    /// Inneficient should be used only to build `HandShakeFrames`
    /// TODO check if is used only to build `HandShakeFrames`
    #[allow(clippy::useless_conversion)]
    fn from_message(
        message: Slice,
        _message_type: u8,
        _extension_type: u16,
        _channel_msg: bool,
    ) -> Option<Self> {
        if message.len() <= NOISE_MAX_LEN {
            Some(Self {
                payload: message.into(),
            })
        } else {
            None
        }
    }
}

#[allow(clippy::useless_conversion)]
pub fn handshake_message_to_frame<T: AsRef<[u8]>>(message: T) -> NoiseFrame {
    let mut payload = Vec::new();
    payload.extend_from_slice(message.as_ref());
    NoiseFrame {
        payload: payload.into(),
    }
}
