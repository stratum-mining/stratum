use crate::{
    framing::{either_frame::EitherFrame, frame::Frame},
    header::Header,
    Error,
};
use binary_sv2::{to_writer, GetSize, Serialize};
use core::convert::TryFrom;

#[allow(unused_imports)]
use alloc::vec::Vec;

/// SV2 frame with generic payload `T` and buffer type `B`
#[derive(Debug, Clone)]
pub struct Sv2Frame<T, B> {
    header: Header,
    payload: Option<T>,
    /// Serializsed header + payload
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

impl<A, B> Sv2Frame<A, B> {
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

#[cfg(feature = "with_buffer_pool")]
impl<A> From<EitherFrame<A, Vec<u8>>> for Sv2Frame<A, buffer_sv2::Slice> {
    fn from(_: EitherFrame<A, Vec<u8>>) -> Self {
        unreachable!()
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

impl<'a, T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> Frame<'a, T> for Sv2Frame<T, B> {
    type Buffer = B;
    type Deserialized = B;

    /// Serialize the frame into dst if the frame is already serialized it just swap dst with
    /// itself
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

    // self can be either serialized (it cointain an AsMut<[u8]> with the serialized data or
    // deserialized it contain the rust type that represant the Sv2 message. If the type is
    // deserialized self.paylos.is_some() is true. To get the serialized payload the inner type
    // should be serialized and this function should never be used, cause is intended as a fast
    // function that return a reference to an already serialized payload. For that the function
    // panic.
    fn payload(&'a mut self) -> &'a mut [u8] {
        if let Some(serialized) = self.serialized.as_mut() {
            &mut serialized.as_mut()[Header::SIZE..]
        } else {
            // panic here is the expected behaviour
            panic!()
        }
    }

    /// If is an Sv2 frame return the Some(header) if it is a noise frame return None
    fn get_header(&self) -> Option<crate::header::Header> {
        Some(self.header)
    }

    /// Try to build a Frame frame from raw bytes.
    /// It return the frame or the number of the bytes needed to complete the frame
    /// The resulting frame is just a header plus a payload with the right number of bytes nothing
    /// is said about the correctness of the payload
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

    #[inline]
    fn size_hint(bytes: &[u8]) -> isize {
        match Header::from_bytes(bytes) {
            Err(_) => {
                // Return incorrect header length
                (Header::SIZE - bytes.len()) as isize
            }
            Ok(header) => {
                if bytes.len() - Header::SIZE == header.len() {
                    0
                } else {
                    (bytes.len() - Header::SIZE) as isize + header.len() as isize
                }
            }
        }
    }

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

    /// Try to build an Frame frame from a serializable payload.
    /// It returns a Frame if the size of the payload fits in the frame, if not it returns None
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
use binary_sv2::binary_codec_sv2;

#[cfg(test)]
#[derive(Serialize)]
struct T {}

#[test]
fn test_size_hint() {
    let h = Sv2Frame::<T, Vec<u8>>::size_hint(&[0, 128, 30, 46, 0, 0][..]);
    assert!(h == 46);
}
