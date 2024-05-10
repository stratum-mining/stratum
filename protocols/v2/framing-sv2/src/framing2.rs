use crate::{
    header::{Header, NoiseHeader},
    Error,
};
use alloc::vec::Vec;
use binary_sv2::{to_writer, GetSize, Serialize};
use core::convert::TryFrom;

const NOISE_MAX_LEN: usize = const_sv2::NOISE_FRAME_MAX_SIZE;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

impl<A, B> Sv2Frame<A, B> {
    /// Maps a `Sv2Frame<A, B>` to `Sv2Frame<C, B>` by applying `fun`,
    /// which is assumed to be a closure that converts `A` to `C`
    pub fn map<C>(self, fun: fn(A) -> C) -> Sv2Frame<C, B> {
        let serialized = self.serialized;
        let header = self.header;
        let payload = self.payload.map(fun);
        Sv2Frame {
            header,
            payload,
            serialized,
        }
    }
}

pub trait Frame<'a, T: Serialize + GetSize>: Sized {
    type Buffer: AsMut<[u8]>;
    type Deserialized;

    /// Write the serialized `Frame` into `dst`.
    fn serialize(self, dst: &mut [u8]) -> Result<(), Error>;

    /// Get the payload
    fn payload(&'a mut self) -> &'a mut [u8];

    /// Returns `Some(self.header)` when the frame has a header (`Sv2Frame`), returns `None` where it doesn't (`HandShakeFrame`).
    fn get_header(&self) -> Option<crate::header::Header>;

    /// Try to build a `Frame` from raw bytes.
    /// Checks if the payload has the correct size (as stated in the `Header`).
    /// Returns `Self` on success, or the number of the bytes needed to complete the frame
    /// as an error. Nothing is assumed or checked about the correctness of the payload.
    fn from_bytes(bytes: Self::Buffer) -> Result<Self, isize>;

    /// Builds a `Frame` from raw bytes.
    /// Does not check if the payload has the correct size (as stated in the `Header`).
    /// Nothing is assumed or checked about the correctness of the payload.
    fn from_bytes_unchecked(bytes: Self::Buffer) -> Self;

    /// Helps to determine if the frame size encoded in a byte array correctly representing the size of the frame.
    /// - Returns `0` if the byte slice is of the expected size according to the header.
    /// - Returns a negative value if the byte slice is smaller than a Noise Frame header; this value
    ///   represents how many bytes are missing.
    /// - Returns a positive value if the byte slice is longer than expected; this value
    ///   indicates the surplus of bytes beyond the expected size.
    fn size_hint(bytes: &[u8]) -> isize;

    /// Returns the size of the `Frame` payload.
    fn encoded_length(&self) -> usize;

    /// Try to build a `Frame` from a serializable payload.
    /// Returns `Some(Self)` if the size of the payload fits in the frame, `None` otherwise.
    fn from_message(
        message: T,
        message_type: u8,
        extension_type: u16,
        channel_msg: bool,
    ) -> Option<Self>;
}

/// Abstraction for a SV2 Frame.
#[derive(Debug, Clone)]
pub struct Sv2Frame<T, B> {
    header: Header,
    payload: Option<T>,
    /// Serialized header + payload
    serialized: Option<B>,
}

impl<T, B> Default for Sv2Frame<T, B> {
    fn default() -> Self {
        Sv2Frame {
            header: Header::default(),
            payload: None,
            serialized: None,
        }
    }
}

/// Abstraction for a Noise Handshake Frame
/// Contains only a `Slice` payload with a fixed length
/// Only used during Noise Handshake process
#[derive(Debug)]
pub struct HandShakeFrame {
    payload: Slice,
}

impl HandShakeFrame {
    /// Returns payload of `HandShakeFrame` as a `Vec<u8>`
    pub fn get_payload_when_handshaking(&self) -> Vec<u8> {
        self.payload[0..].to_vec()
    }
}

impl<'a, T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> Frame<'a, T> for Sv2Frame<T, B> {
    type Buffer = B;
    type Deserialized = B;

    /// Write the serialized `Sv2Frame` into `dst`.
    /// This operation when called on an already serialized frame is very cheap.
    /// When called on a non serialized frame, it is not so cheap (because it serializes it).
    #[inline]
    fn serialize(self, dst: &mut [u8]) -> Result<(), Error> {
        if let Some(mut serialized) = self.serialized {
            dst.swap_with_slice(serialized.as_mut());
            Ok(())
        } else if let Some(payload) = self.payload {
            #[cfg(not(feature = "with_serde"))]
            to_writer(self.header, dst).map_err(Error::BinarySv2Error)?;
            #[cfg(not(feature = "with_serde"))]
            to_writer(payload, &mut dst[Header::SIZE..]).map_err(Error::BinarySv2Error)?;
            #[cfg(feature = "with_serde")]
            to_writer(&self.header, dst.as_mut()).map_err(Error::BinarySv2Error)?;
            #[cfg(feature = "with_serde")]
            to_writer(&payload, &mut dst.as_mut()[Header::SIZE..])
                .map_err(Error::BinarySv2Error)?;
            Ok(())
        } else {
            // Sv2Frame always has a payload or a serialized payload
            panic!("Impossible state")
        }
    }

    /// `self` can be either serialized (`self.serialized` is `Some()`) or
    /// deserialized (`self.serialized` is `None`, `self.payload` is `Some()`).
    /// This function is only intended as a fast way to get a reference to an
    /// already serialized payload. If the frame has not yet been
    /// serialized, this function should never be used (it will panic).
    fn payload(&'a mut self) -> &'a mut [u8] {
        if let Some(serialized) = self.serialized.as_mut() {
            &mut serialized.as_mut()[Header::SIZE..]
        } else {
            // panic here is the expected behaviour
            panic!("Sv2Frame is not yet serialized.")
        }
    }

    /// `Sv2Frame` always returns `Some(self.header)`.
    fn get_header(&self) -> Option<crate::header::Header> {
        Some(self.header)
    }

    /// Tries to build a `Sv2Frame` from raw bytes, assuming they represent a serialized `Sv2Frame` frame (`Self.serialized`).
    /// Returns a `Sv2Frame` on success, or the number of the bytes needed to complete the frame
    /// as an error. `Self.serialized` is `Some`, but nothing is assumed or checked about the correctness of the payload.
    #[inline]
    fn from_bytes(mut bytes: Self::Buffer) -> Result<Self, isize> {
        let hint = Self::size_hint(bytes.as_mut());

        if hint == 0 {
            Ok(Self::from_bytes_unchecked(bytes))
        } else {
            Err(hint)
        }
    }

    #[inline]
    fn from_bytes_unchecked(mut bytes: Self::Buffer) -> Self {
        // Unchecked function caller is supposed to already know that the passed bytes are valid
        let header = Header::from_bytes(bytes.as_mut()).expect("Invalid header");
        Self {
            header,
            payload: None,
            serialized: Some(bytes),
        }
    }

    /// After parsing `bytes` into a `Header`, this function helps to determine if the `msg_length`
    /// field is correctly representing the size of the frame.
    /// - Returns `0` if the byte slice is of the expected size according to the header.
    /// - Returns a negative value if the byte slice is shorter than expected; this value
    ///   represents how many bytes are missing.
    /// - Returns a positive value if the byte slice is longer than expected; this value
    ///   indicates the surplus of bytes beyond the expected size.
    #[inline]
    fn size_hint(bytes: &[u8]) -> isize {
        match Header::from_bytes(bytes) {
            Err(_) => {
                // Returns how many bytes are missing from the expected frame size
                (Header::SIZE - bytes.len()) as isize
            }
            Ok(header) => {
                if bytes.len() - Header::SIZE == header.len() {
                    // expected frame size confirmed
                    0
                } else {
                    // Returns how many excess bytes are beyond the expected frame size
                    (bytes.len() - Header::SIZE) as isize + header.len() as isize
                }
            }
        }
    }

    /// If `Sv2Frame` is serialized, returns the length of `self.serialized`,
    /// otherwise, returns the length of `self.payload`.
    #[inline]
    fn encoded_length(&self) -> usize {
        if let Some(serialized) = self.serialized.as_ref() {
            serialized.as_ref().len()
        } else if let Some(payload) = self.payload.as_ref() {
            payload.get_size() + Header::SIZE
        } else {
            // Sv2Frame always has a payload or a serialized payload
            panic!("Impossible state")
        }
    }

    /// Tries to build a `Sv2Frame` from a non-serialized payload.
    /// Returns a `Sv2Frame` if the size of the payload fits in the frame, `None` otherwise.
    fn from_message(
        message: T,
        message_type: u8,
        extension_type: u16,
        channel_msg: bool,
    ) -> Option<Self> {
        let extension_type = update_extension_type(extension_type, channel_msg);
        let len = message.get_size() as u32;
        Header::from_len(len, message_type, extension_type).map(|header| Self {
            header,
            payload: Some(message),
            serialized: None,
        })
    }
}

impl<'a> Frame<'a, Slice> for HandShakeFrame {
    type Buffer = Slice;
    type Deserialized = &'a mut [u8];

    /// Put the Noise Frame payload into `dst`
    #[inline]
    fn serialize(mut self, dst: &mut [u8]) -> Result<(), Error> {
        dst.swap_with_slice(self.payload.as_mut());
        Ok(())
    }

    /// Get the Noise Frame payload
    #[inline]
    fn payload(&'a mut self) -> &'a mut [u8] {
        &mut self.payload[NoiseHeader::HEADER_SIZE..]
    }

    /// `HandShakeFrame` always returns `None`.
    fn get_header(&self) -> Option<crate::header::Header> {
        None
    }

    /// Builds a `HandShakeFrame` from raw bytes. Nothing is assumed or checked about the correctness of the payload.
    fn from_bytes(bytes: Self::Buffer) -> Result<Self, isize> {
        Ok(Self::from_bytes_unchecked(bytes))
    }

    #[inline]
    fn from_bytes_unchecked(bytes: Self::Buffer) -> Self {
        Self { payload: bytes }
    }

    /// After parsing the expected `HandShakeFrame` size from `bytes`, this function helps to determine if this value
    /// correctly representing the size of the frame.
    /// - Returns `0` if the byte slice is of the expected size according to the header.
    /// - Returns a negative value if the byte slice is smaller than a Noise Frame header; this value
    ///   represents how many bytes are missing.
    /// - Returns a positive value if the byte slice is longer than expected; this value
    ///   indicates the surplus of bytes beyond the expected size.
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

    /// Returns the size of the `HandShakeFrame` payload.
    #[inline]
    fn encoded_length(&self) -> usize {
        self.payload.len()
    }

    /// Tries to build a `HandShakeFrame` frame from a byte slice.
    /// Returns a `HandShakeFrame` if the size of the payload fits in the frame, `None` otherwise.
    /// This is quite inefficient, and should be used only to build `HandShakeFrames`
    // TODO check if is used only to build `HandShakeFrames`
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

/// Returns a `HandShakeFrame` from a generic byte array
#[allow(clippy::useless_conversion)]
pub fn handshake_message_to_frame<T: AsRef<[u8]>>(message: T) -> HandShakeFrame {
    let mut payload = Vec::new();
    payload.extend_from_slice(message.as_ref());
    HandShakeFrame {
        payload: payload.into(),
    }
}

/// Basically a boolean bit filter for `extension_type`.
/// Takes an `extension_type` represented as a `u16` and a boolean flag (`channel_msg`).
/// If `channel_msg` is true, it sets the most significant bit of `extension_type` to 1,
/// otherwise, it clears the most significant bit to 0.
fn update_extension_type(extension_type: u16, channel_msg: bool) -> u16 {
    if channel_msg {
        let mask = 0b1000_0000_0000_0000;
        extension_type | mask
    } else {
        let mask = 0b0111_1111_1111_1111;
        extension_type & mask
    }
}

/// A wrapper to be used in a context we need a generic reference to a frame
/// but it doesn't matter which kind of frame it is (`Sv2Frame` or `HandShakeFrame`)
#[derive(Debug)]
pub enum EitherFrame<T, B> {
    HandShake(HandShakeFrame),
    Sv2(Sv2Frame<T, B>),
}

impl<T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> EitherFrame<T, B> {
    pub fn encoded_length(&self) -> usize {
        match &self {
            Self::HandShake(frame) => frame.encoded_length(),
            Self::Sv2(frame) => frame.encoded_length(),
        }
    }
}

impl<T, B> TryFrom<EitherFrame<T, B>> for HandShakeFrame {
    type Error = Error;

    fn try_from(v: EitherFrame<T, B>) -> Result<Self, Error> {
        match v {
            EitherFrame::HandShake(frame) => Ok(frame),
            EitherFrame::Sv2(_) => Err(Error::ExpectedHandshakeFrame),
        }
    }
}

impl<T, B> TryFrom<EitherFrame<T, B>> for Sv2Frame<T, B> {
    type Error = Error;

    fn try_from(v: EitherFrame<T, B>) -> Result<Self, Error> {
        match v {
            EitherFrame::Sv2(frame) => Ok(frame),
            EitherFrame::HandShake(_) => Err(Error::ExpectedSv2Frame),
        }
    }
}

impl<T, B> From<HandShakeFrame> for EitherFrame<T, B> {
    fn from(v: HandShakeFrame) -> Self {
        Self::HandShake(v)
    }
}

impl<T, B> From<Sv2Frame<T, B>> for EitherFrame<T, B> {
    fn from(v: Sv2Frame<T, B>) -> Self {
        Self::Sv2(v)
    }
}

#[cfg(test)]
use binary_sv2::binary_codec_sv2;

#[cfg(test)]
#[derive(Serialize)]
struct T {}

#[test]
fn test_size_hint() {
    let h = Sv2Frame::<T, Vec<u8>>::size_hint(&[0, 128, 30, 46, 0, 0][..]);
    assert!(h == 46);
}
