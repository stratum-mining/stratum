#![allow(dead_code)]
use crate::{
    header::{Header, NOISE_HEADER_SIZE},
    Error,
};
use alloc::vec::Vec;
use binary_sv2::{to_writer, GetSize, Serialize};
use core::convert::TryFrom;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

/// Represents different types of frames that can be sent over the wire.
#[derive(Debug)]
pub enum Frame<T, B> {
    /// Abstraction for a Noise Handshake Frame Contains only a `Slice` payload with a fixed length
    /// Only used during Noise Handshake process
    HandShake(HandShakeFrame),
    /// Abstraction for a SV2 Frame.
    /// `T` represents the deserialized payload, `B` the serialized payload.
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

impl<T, B> TryFrom<Frame<T, B>> for HandShakeFrame {
    type Error = Error;

    fn try_from(v: Frame<T, B>) -> Result<Self, Error> {
        match v {
            Frame::HandShake(frame) => Ok(frame),
            Frame::Sv2(_) => Err(Error::ExpectedHandshakeFrame),
        }
    }
}

impl<T, B> TryFrom<Frame<T, B>> for Sv2Frame<T, B> {
    type Error = Error;

    fn try_from(v: Frame<T, B>) -> Result<Self, Error> {
        match v {
            Frame::Sv2(frame) => Ok(frame),
            Frame::HandShake(_) => Err(Error::ExpectedSv2Frame),
        }
    }
}

impl<T, B> From<HandShakeFrame> for Frame<T, B> {
    fn from(v: HandShakeFrame) -> Self {
        Self::HandShake(v)
    }
}

impl<T, B> From<Sv2Frame<T, B>> for Frame<T, B> {
    fn from(v: Sv2Frame<T, B>) -> Self {
        Self::Sv2(v)
    }
}

/// Abstraction for a SV2 Frame.
#[derive(Debug, Clone)]
pub struct Sv2Frame<T, B> {
    /// Frame header
    header: Header,
    /// Deserialized payload
    payload: Option<T>,
    /// Serialized header + payload
    serialized: Option<B>,
}

impl<T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> Sv2Frame<T, B> {
    /// Tries to build a `Sv2Frame` from a non-serialized payload.
    /// Returns a `Sv2Frame` if the size of the payload fits in the frame, `None` otherwise.
    pub fn from_message(
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

    pub fn from_bytes(mut bytes: B) -> Result<Self, Error> {
        let header = Header::from_bytes(bytes.as_mut())?;
        Ok(Self {
            header,
            payload: None,
            serialized: Some(bytes),
        })
    }

    /// Write the serialized `Sv2Frame` into `dst`.
    /// This operation when called on an already serialized frame is very cheap.
    /// When called on a non serialized frame, it is not so cheap (because it serializes it).
    #[inline]
    pub fn serialize(self, dst: &mut [u8]) -> Result<(), Error> {
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

    /// Returns the payload of the `Sv2Frame` as a byte slice.
    ///
    /// Will return None if the `Sv2Frame` is not yet serialized.
    ///
    /// You can serialize the frame by calling [`Sv2Frame::serialize`].
    ///
    /// [`Sv2Frame::serialize`]: crate::framing2::Sv2Frame::serialize
    pub fn payload(&self) -> Option<&[u8]> {
        if let Some(serialized) = self.serialized.as_ref() {
            Some(serialized.as_ref()[Header::SIZE..].as_ref())
        } else {
            None // Sv2Frame is not yet serialized.
        }
    }

    /// Returns the header of the `Sv2Frame`.
    pub fn header(&self) -> crate::header::Header {
        self.header
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

    /// If `Sv2Frame` is serialized, returns the length of `self.serialized`,
    /// otherwise, returns the length of `self.payload`.
    #[inline]
    pub fn encoded_length(&self) -> usize {
        if let Some(serialized) = self.serialized.as_ref() {
            serialized.as_ref().len()
        } else if let Some(payload) = self.payload.as_ref() {
            payload.get_size() + Header::SIZE
        } else {
            // Sv2Frame always has a payload or a serialized payload
            panic!("Impossible state")
        }
    }
}

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

/// Abstraction for a Noise Handshake Frame.
///
/// Contains only a `Slice` payload with a fixed length.
///
/// Only used during Noise Handshake process.
#[derive(Debug)]
pub struct HandShakeFrame {
    payload: Slice,
}

impl HandShakeFrame {
    /// Builds a `HandShakeFrame` from raw bytes.
    ///
    /// Nothing is assumed or checked about the correctness of the payload.
    pub fn from_bytes(bytes: Slice) -> Self {
        Self { payload: bytes }
    }

    /// Get the Noise Frame payload
    pub fn payload(&self) -> &[u8] {
        &self.payload[NOISE_HEADER_SIZE..]
    }

    /// Returns the size of the `HandShakeFrame` payload.
    fn encoded_length(&self) -> usize {
        self.payload.len()
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

#[cfg(test)]
mod tests {
    use crate::framing2::Sv2Frame;
    use alloc::vec::Vec;
    use binary_sv2::{binary_codec_sv2, Serialize};

    #[cfg(test)]
    #[derive(Serialize)]
    struct T {}

    #[test]
    fn test_size_hint() {
        let h = Sv2Frame::<T, Vec<u8>>::size_hint(&[0, 128, 30, 46, 0, 0][..]);
        assert!(h == 46);
    }
}
