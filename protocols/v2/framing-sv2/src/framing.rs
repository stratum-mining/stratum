use crate::{header::Header, Error};
use alloc::vec::Vec;
use binary_sv2::{to_writer, GetSize, Serialize};
use core::convert::TryFrom;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

/// A wrapper to be used in a context we need a generic reference to a frame
/// but it doesn't matter which kind of frame it is (`Sv2Frame` or `HandShakeFrame`)
#[derive(Debug)]
pub enum Frame<T, B>
where
    T: Serialize + GetSize,
    B: AsMut<[u8]> + AsRef<[u8]>,
{
    HandShake(HandShakeFrame),
    Sv2(Sv2Frame<T, B>),
}

impl<T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> Frame<T, B> {
    pub fn encoded_length(&self) -> usize {
        match &self {
            Self::HandShake(frame) => frame.encoded_length(),
            Self::Sv2(frame) => frame.encoded_length(),
        }
    }
}

impl<T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> From<HandShakeFrame> for Frame<T, B> {
    fn from(v: HandShakeFrame) -> Self {
        Self::HandShake(v)
    }
}

impl<T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> From<Sv2Frame<T, B>> for Frame<T, B> {
    fn from(v: Sv2Frame<T, B>) -> Self {
        Self::Sv2(v)
    }
}

/// Abstraction for a SV2 Frame.
#[derive(Debug, Clone)]
pub enum Sv2Frame<T, B> {
    Payload { header: Header, payload: T },
    Raw { header: Header, serialized: B },
}

impl<T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> Sv2Frame<T, B> {
    /// Tries to build a `Sv2Frame` from raw bytes, assuming they represent a serialized `Sv2Frame`
    /// frame (`Self.serialized`).  Returns a `Sv2Frame` on success, or the number of the bytes
    /// needed to complete the frame as an error. `Self.serialized` is `Some`, but nothing is
    /// assumed or checked about the correctness of the payload.
    #[inline]
    pub fn from_bytes(mut bytes: B) -> Result<Self, isize> {
        let hint = Self::size_hint(bytes.as_mut());

        if hint == 0 {
            Ok(Self::from_bytes_unchecked(bytes))
        } else {
            Err(hint)
        }
    }

    #[inline]
    pub fn from_bytes_unchecked(mut bytes: B) -> Self {
        // Unchecked function caller is supposed to already know that the passed bytes are valid
        let header = Header::from_bytes(bytes.as_mut()).expect("Invalid header");
        Sv2Frame::Raw {
            header,
            serialized: bytes,
        }
    }

    /// Tries to build a `Sv2Frame` from a non-serialized payload.  Returns a `Sv2Frame` if the size
    /// of the payload fits in the frame, `None` otherwise.
    pub fn from_message(
        message: T,
        message_type: u8,
        extension_type: u16,
        channel_msg: bool,
    ) -> Option<Self> {
        let extension_type = update_extension_type(extension_type, channel_msg);
        let len = message.get_size() as u32;
        Header::from_len(len, message_type, extension_type).map(|header| Self::Payload {
            header,
            payload: message,
        })
    }

    /// Write the serialized `Sv2Frame` into `dst`.
    /// This operation when called on an already serialized frame is very cheap.
    /// When called on a non serialized frame, it is not so cheap (because it serializes it).
    #[inline]
    pub fn serialize(self, dst: &mut [u8]) -> Result<(), Error> {
        match self {
            Sv2Frame::Raw { mut serialized, .. } => {
                dst.swap_with_slice(serialized.as_mut());
                Ok(())
            }
            Sv2Frame::Payload { header, payload } => {
                #[cfg(not(feature = "with_serde"))]
                to_writer(header, dst).map_err(Error::BinarySv2Error)?;
                #[cfg(not(feature = "with_serde"))]
                to_writer(payload, &mut dst[Header::SIZE..]).map_err(Error::BinarySv2Error)?;
                #[cfg(feature = "with_serde")]
                to_writer(&header, dst.as_mut()).map_err(Error::BinarySv2Error)?;
                #[cfg(feature = "with_serde")]
                to_writer(&payload, &mut dst.as_mut()[Header::SIZE..])
                    .map_err(Error::BinarySv2Error)?;
                Ok(())
            }
        }
    }

    /// `self` can be either serialized (`self.serialized` is `Some()`) or deserialized
    /// (`self.serialized` is `None`, `self.payload` is `Some()`).  This function is only intended
    /// as a fast way to get a reference to an already serialized payload. If the frame has not yet
    /// been serialized, this function should never be used (it will panic).
    pub fn payload(&mut self) -> Option<&mut [u8]> {
        match self {
            Sv2Frame::Raw { serialized, .. } => Some(&mut serialized.as_mut()[Header::SIZE..]),
            Sv2Frame::Payload { .. } => None,
        }
    }

    pub fn header(&self) -> Header {
        match self {
            Self::Payload { header, .. } => *header,
            Self::Raw { header, .. } => *header,
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
    pub fn size_hint(bytes: &[u8]) -> isize {
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

    /// If `Sv2Frame` is serialized, returns the length of `self.serialized`, otherwise, returns the
    /// length of `self.payload`.
    #[inline]
    pub fn encoded_length(&self) -> usize {
        match self {
            Sv2Frame::Raw { serialized, .. } => serialized.as_ref().len(),
            Sv2Frame::Payload { payload, .. } => payload.get_size() + Header::SIZE,
        }
    }
}

impl<A: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> Sv2Frame<A, B> {
    /// Maps a `Sv2Frame<A, B>` to `Sv2Frame<C, B>` by applying `fun`,
    /// which is assumed to be a closure that converts `A` to `C`
    pub fn map<C>(self, fun: fn(A) -> C) -> Sv2Frame<C, B>
    where
        C: Serialize + GetSize,
    {
        match self {
            Sv2Frame::Raw { header, serialized } => Sv2Frame::Raw { header, serialized },
            Sv2Frame::Payload { header, payload } => Sv2Frame::Payload {
                header,
                payload: fun(payload),
            },
        }
    }
}

impl<T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> TryFrom<Frame<T, B>> for Sv2Frame<T, B> {
    type Error = Error;

    fn try_from(v: Frame<T, B>) -> Result<Self, Error> {
        match v {
            Frame::Sv2(frame) => Ok(frame),
            Frame::HandShake(_) => Err(Error::ExpectedSv2Frame),
        }
    }
}

/// Abstraction for a Noise Handshake Frame Contains only a `Slice` payload with a fixed length
/// Only used during Noise Handshake process
#[derive(Debug)]
pub struct HandShakeFrame {
    payload: Slice,
}

impl HandShakeFrame {
    /// Builds a `HandShakeFrame` from raw bytes. Nothing is assumed or checked about the correctness of the payload.
    pub fn from_bytes(bytes: Slice) -> Result<Self, isize> {
        Ok(Self::from_bytes_unchecked(bytes))
    }

    /// Returns a `HandShakeFrame` from a generic byte array
    #[allow(clippy::useless_conversion)]
    pub fn from_message<T: AsRef<[u8]>>(message: T) -> HandShakeFrame {
        let mut payload = Vec::new();
        payload.extend_from_slice(message.as_ref());
        HandShakeFrame {
            payload: payload.into(),
        }
    }

    #[inline]
    pub fn from_bytes_unchecked(bytes: Slice) -> Self {
        Self { payload: bytes }
    }

    /// Returns payload of `HandShakeFrame` as a `Vec<u8>`
    pub fn get_payload_when_handshaking(&self) -> Vec<u8> {
        self.payload[0..].to_vec()
    }

    /// Returns the size of the `HandShakeFrame` payload.
    #[inline]
    fn encoded_length(&self) -> usize {
        self.payload.len()
    }
}

impl<T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> TryFrom<Frame<T, B>> for HandShakeFrame {
    type Error = Error;

    fn try_from(v: Frame<T, B>) -> Result<Self, Error> {
        match v {
            Frame::HandShake(frame) => Ok(frame),
            Frame::Sv2(_) => Err(Error::ExpectedHandshakeFrame),
        }
    }
}

/// Basically a boolean bit filter for `extension_type`.  Takes an `extension_type` represented as
/// a `u16` and a boolean flag (`channel_msg`).  If `channel_msg` is true, it sets the most
/// significant bit of `extension_type` to 1, otherwise, it clears the most significant bit to 0.
fn update_extension_type(extension_type: u16, channel_msg: bool) -> u16 {
    if channel_msg {
        let mask = 0b1000_0000_0000_0000;
        extension_type | mask
    } else {
        let mask = 0b0111_1111_1111_1111;
        extension_type & mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_sv2_frame_from_bytes() {
        let slice: Slice = vec![].into();
        assert_eq!(
            Sv2Frame::<u32, Slice>::from_bytes(slice.clone()).unwrap_err(),
            6
        );
        let slice: Slice = vec![0; 6].into();
        assert!(Sv2Frame::<u32, Slice>::from_bytes(slice.clone()).is_ok());
        let slice: Slice = vec![0; 10].into();
        assert_eq!(
            Sv2Frame::<u32, Slice>::from_bytes(slice.clone()).unwrap_err(),
            4
        );
        let slice: Slice = vec![0; 8].into();
        assert_eq!(
            Sv2Frame::<u32, Slice>::from_bytes(slice.clone()).unwrap_err(),
            2
        );
        let slice: Slice = vec![0; 4].into();
        assert_eq!(
            Sv2Frame::<u32, Slice>::from_bytes(slice.clone()).unwrap_err(),
            2
        );
        let slice: Slice = vec![0; 2].into();
        assert_eq!(
            Sv2Frame::<u32, Slice>::from_bytes(slice.clone()).unwrap_err(),
            4
        );
    }

    #[test]
    fn test_sv2_frame_from_message() {
        let message = 0u32;
        let message_type = 0u8;
        let extension_type = 0u16;
        let ret =
            Sv2Frame::<u32, Slice>::from_message(message, message_type, extension_type, false)
                .unwrap();
        assert_eq!(ret.encoded_length(), 10);
    }

    #[test]
    fn test_sv2_frame_payload() {
        let message = 2u32;
        let message_type = 0u8;
        let extension_type = 0u16;
        let mut frame =
            Sv2Frame::<u32, Slice>::from_message(message, message_type, extension_type, true)
                .unwrap();
        assert!(frame.payload().is_none());
        let slice: Slice = vec![0; 6].into();
        let mut frame = Sv2Frame::<u32, Slice>::from_bytes(slice.clone()).unwrap();
        assert_eq!(frame.payload().unwrap().len(), 0);
    }

    #[test]
    fn test_handsahke_from_bytes() {
        let slice: Slice = vec![].into();
        let frame = HandShakeFrame::from_bytes(slice.clone()).unwrap();
        assert_eq!(frame.encoded_length(), 0);
        let slice: Slice = vec![0; 6].into();
        let frame = HandShakeFrame::from_bytes(slice.clone()).unwrap();
        assert_eq!(frame.encoded_length(), 6);
    }

    #[test]
    fn test_handshake_from_message() {
        let message = vec![0u8; 6];
        let frame = HandShakeFrame::from_message(message);
        assert_eq!(frame.encoded_length(), 6);
    }
}
