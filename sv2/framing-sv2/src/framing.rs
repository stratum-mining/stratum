//! # Sv2 Frame
//!
//! Handles the serializing and deserializing of both Sv2 and Noise handshake messages into frames.
//!
//! It handles the serialization and deserialization of frames, ensuring that messages can be
//! correctly encoded and transmitted and then received and decoded between Sv2 roles.
//!
//! # Usage
//!
//! Frames come in two kinds. Almost all messages passed between Sv2 roles travel in an Sv2 frame,
//! a [`crate::header::Header`] followed by the serialized message payload:
//! [`crate::framing::MessageFrame`] on the way out, holding a message still to be serialized, and
//! [`crate::framing::SerializedFrame`] on the way in, holding the bytes that were read. The
//! [`crate::framing::HandshakeMessage`] is used exclusively during the Noise handshake process,
//! performed between Sv2 roles at the beginning of their communication. It is used until the
//! handshake state progresses to transport mode. After that, all subsequent messages use
//! [`crate::framing::MessageFrame`]. No header is included in the handshake message.

use crate::{header::Header, Error};
use alloc::vec::Vec;
use binary_sv2::{to_writer, GetSize, Serialize};
use core::cmp::Ordering;

/// Size of the `channel_id` that opens the payload of a channel message.
const CHANNEL_ID_SIZE: usize = 4;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

/// Describes how the length of a byte slice misses the frame size declared by its [`Header`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeHint {
    /// The slice does not hold a complete frame yet, and the given number of bytes are still
    /// missing, either from the [`Header`] or from the payload it declares.
    Missing(usize),

    /// The slice holds a complete frame followed by the given number of surplus bytes.
    Surplus(usize),
}

impl core::fmt::Display for SizeHint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Missing(n) => write!(f, "missing {n} bytes to complete the frame"),
            Self::Surplus(n) => write!(f, "{n} bytes buffered past the end of the frame"),
        }
    }
}

/// A frame an encoder can write out, whichever side of the split it comes from.
///
/// Implemented by [`MessageFrame`], which serializes its message on the way out, and by
/// [`SerializedFrame`], which already holds the bytes to write. A caller that sometimes has one
/// and sometimes the other can implement this for its own type and hand that to the encoder.
pub trait EncodableFrame {
    /// Returns the length the frame takes once encoded, which includes the
    /// [`crate::header::Header`] and so is never below [`crate::SV2_FRAME_HEADER_SIZE`]. An
    /// encoder rejects a frame that reports less than that.
    fn encoded_length(&self) -> usize;

    /// Writes the frame into `dst`, erroring out unless `dst` is exactly [`Self::encoded_length`]
    /// long. A longer `dst` is refused rather than left with whatever it held past the frame,
    /// which a caller that sends the whole buffer would send along.
    fn encode_into(self, dst: &mut [u8]) -> Result<(), Error>;
}

impl<T: Serialize + GetSize> EncodableFrame for MessageFrame<T> {
    fn encoded_length(&self) -> usize {
        MessageFrame::encoded_length(self)
    }

    fn encode_into(self, dst: &mut [u8]) -> Result<(), Error> {
        let expected = MessageFrame::encoded_length(&self);
        if dst.len() != expected {
            return Err(Error::UnexpectedDestinationLength {
                expected,
                actual: dst.len(),
            });
        }
        self.header.write_into(dst)?;
        to_writer(self.message, &mut dst[Header::SIZE..]).map_err(Error::BinarySv2Error)?;
        Ok(())
    }
}

impl<B: AsMut<[u8]> + AsRef<[u8]>> EncodableFrame for SerializedFrame<B> {
    fn encoded_length(&self) -> usize {
        SerializedFrame::encoded_length(self)
    }

    fn encode_into(self, dst: &mut [u8]) -> Result<(), Error> {
        let expected = SerializedFrame::encoded_length(&self);
        if dst.len() != expected {
            return Err(Error::UnexpectedDestinationLength {
                expected,
                actual: dst.len(),
            });
        }
        dst.copy_from_slice(self.as_bytes());
        Ok(())
    }
}

/// A frame carrying a message that has not been serialized yet.
///
/// A message plus the [`Header`] that describes it, built with [`MessageFrame::from_message`] and
/// written out with [`EncodableFrame::encode_into`]. A frame read off the wire is a
/// [`SerializedFrame`] instead.
#[derive(Debug, Clone)]
pub struct MessageFrame<T> {
    header: Header,
    message: T,
}

impl<T: Serialize + GetSize> MessageFrame<T> {
    /// Tries to build a [`MessageFrame`] from a message.
    ///
    /// Errors out with [`Error::PayloadTooLong`] if the serialized size of the message does not
    /// fit in a frame.
    pub fn from_message(
        message: T,
        message_type: u8,
        extension_type: u16,
        channel_msg: bool,
    ) -> Result<Self, Error> {
        let extension_type = update_extension_type(extension_type, channel_msg);
        let header = Header::from_len(message.get_size(), message_type, extension_type)?;
        Ok(Self { header, message })
    }

    /// Returns the [`Header`] of the frame.
    pub fn header(&self) -> Header {
        self.header
    }

    /// Returns the length the frame takes once serialized: the message plus [`Header::SIZE`].
    #[inline]
    pub fn encoded_length(&self) -> usize {
        self.header.payload_length() + Header::SIZE
    }
}

/// A frame carrying the serialized bytes of its header and payload.
///
/// What a decoder hands back once it has read a whole frame off the wire, and what a role
/// forwarding one hands an encoder. Because it is built from those bytes, [`Self::payload`]
/// always has them.
#[derive(Debug, Clone)]
pub struct SerializedFrame<B> {
    header: Header,
    bytes: B,
}

impl<B: AsMut<[u8]> + AsRef<[u8]>> SerializedFrame<B> {
    /// Tries to build a [`SerializedFrame`] from raw bytes, erroring out with the [`SizeHint`]
    /// that describes the mismatch if they do not hold exactly one frame.
    ///
    /// Nothing is assumed or checked about the correctness of the payload.
    #[inline]
    pub fn from_bytes(bytes: B) -> Result<Self, SizeHint> {
        let header = Self::parse_header(bytes.as_ref())?;
        Ok(Self::from_parts(header, bytes))
    }

    /// Builds a [`SerializedFrame`] from a [`Header`] already parsed out of `bytes`, so that a
    /// caller that had to parse one to decide the bytes were complete does not parse it twice.
    #[inline]
    pub fn from_parts(header: Header, bytes: B) -> Self {
        debug_assert_eq!(Header::SIZE + header.payload_length(), bytes.as_ref().len());
        Self { header, bytes }
    }

    /// Returns the [`Header`] of the frame.
    pub fn header(&self) -> Header {
        self.header
    }

    /// Returns the serialized payload, i.e. everything the frame holds after its [`Header`].
    pub fn payload(&mut self) -> &mut [u8] {
        &mut self.bytes.as_mut()[Header::SIZE..]
    }

    /// Returns the length the frame takes once encoded.
    #[inline]
    pub fn encoded_length(&self) -> usize {
        self.bytes.as_ref().len()
    }

    /// Returns the `channel_id` the message is destined for, if it has one.
    pub fn channel_id(&self) -> Option<u32> {
        if !self.header.channel_msg() {
            return None;
        }
        let payload = self.bytes.as_ref().get(Header::SIZE..)?;
        let id = payload.get(..CHANNEL_ID_SIZE)?;
        Some(u32::from_le_bytes(id.try_into().ok()?))
    }

    /// Returns the whole frame, header included, as the bytes it was built from.
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    /// Consumes the frame and returns the bytes it was built from.
    pub fn into_bytes(self) -> B {
        self.bytes
    }

    /// Copies the frame into bytes it owns, for a frame that has to be kept or passed on after it
    /// is decoded. A frame backed by a decoder's buffer pool releases its slot as it is consumed.
    pub fn into_owned(self) -> Self
    where
        B: From<Vec<u8>>,
    {
        Self {
            header: self.header,
            bytes: B::from(self.bytes.as_ref().to_vec()),
        }
    }

    /// Parses the [`Header`] and checks it against the length of `bytes`, returning it only when
    /// they hold exactly one complete frame and the [`SizeHint`] that describes the mismatch
    /// otherwise.
    ///
    /// If `bytes` is too short to contain a full [`Header`], the returned [`SizeHint::Missing`]
    /// only accounts for the bytes needed to complete the header.
    #[inline]
    pub fn parse_header(bytes: &[u8]) -> Result<Header, SizeHint> {
        let Ok(header) = Header::from_bytes(bytes) else {
            return Err(SizeHint::Missing(Header::SIZE.saturating_sub(bytes.len())));
        };
        let expected = Header::SIZE + header.payload_length();
        match bytes.len().cmp(&expected) {
            Ordering::Less => Err(SizeHint::Missing(expected - bytes.len())),
            Ordering::Equal => Ok(header),
            Ordering::Greater => Err(SizeHint::Surplus(bytes.len() - expected)),
        }
    }
}

/// A Noise handshake message.
///
/// Carries only the serialized payload, of a length fixed by the handshake step that produced it,
/// and is used only during the Noise handshake: no [`Header`] describes it, so it is not a frame.
/// Once the handshake completes, communication switches to [`MessageFrame`] and
/// [`SerializedFrame`].
#[derive(Debug)]
pub struct HandshakeMessage {
    payload: Slice,
}

impl HandshakeMessage {
    /// Builds a [`HandshakeMessage`] from raw bytes. Nothing is assumed or checked about the
    /// correctness of the payload.
    #[inline]
    pub fn from_bytes(bytes: Slice) -> Self {
        Self { payload: bytes }
    }

    /// Builds a [`HandshakeMessage`] that carries a copy of `message`, the bytes a Noise handshake
    /// step produced.
    #[allow(clippy::useless_conversion)]
    pub fn from_message<T: AsRef<[u8]>>(message: T) -> Self {
        let mut payload = Vec::new();
        payload.extend_from_slice(message.as_ref());
        Self {
            payload: payload.into(),
        }
    }

    /// Returns the payload of the [`HandshakeMessage`].
    #[inline]
    pub fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }
}

// Basically a Boolean bit filter for `extension_type`.
//
// Takes an `extension_type` represented as a `u16` and a Boolean flag (`channel_msg`). If
// `channel_msg` is true, it sets the most significant bit of `extension_type` to `1`, otherwise,
// it clears the most significant bit to `0`.
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
    use binary_sv2::{encodable::EncodableField, B064KOwned, Serialize};
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;

    #[derive(Serialize)]
    struct T {}

    #[test]
    fn test_parse_header() {
        let hint =
            SerializedFrame::<Vec<u8>>::parse_header(&[0, 128, 30, 46, 0, 0][..]).unwrap_err();
        assert_eq!(hint, SizeHint::Missing(46));
    }

    #[test]
    fn test_parse_header_empty_payload() {
        let header = SerializedFrame::<Vec<u8>>::parse_header(&[0, 0, 1, 0, 0, 0][..]).unwrap();
        assert_eq!(header.payload_length(), 0);
        let hint =
            SerializedFrame::<Vec<u8>>::parse_header(&[0, 0, 1, 0, 0, 0, 9, 9, 9][..]).unwrap_err();
        assert_eq!(hint, SizeHint::Surplus(3));
    }

    struct HugeMsg(usize);

    impl From<HugeMsg> for EncodableField<'_> {
        fn from(_: HugeMsg) -> Self {
            EncodableField::Struct(Vec::new())
        }
    }

    impl GetSize for HugeMsg {
        fn get_size(&self) -> usize {
            self.0
        }
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn from_message_rejects_a_size_that_does_not_fit_in_a_u32() {
        let len = (u32::MAX as usize) + 2;
        assert_eq!(
            MessageFrame::<HugeMsg>::from_message(HugeMsg(len), 0x01, 0x0000, false).err(),
            Some(Error::PayloadTooLong(len))
        );
    }

    #[test]
    fn from_message_rejects_a_size_that_does_not_fit_in_a_u24() {
        const U24_MAX: usize = 16_777_215;

        assert_eq!(
            MessageFrame::<HugeMsg>::from_message(HugeMsg(U24_MAX + 1), 0x01, 0x0000, false).err(),
            Some(Error::PayloadTooLong(U24_MAX + 1))
        );

        let frame = MessageFrame::<HugeMsg>::from_message(HugeMsg(U24_MAX), 0x01, 0x0000, false)
            .expect("the largest length the U24 holds is accepted");
        assert_eq!(frame.header().payload_length(), U24_MAX);
    }

    #[test]
    fn serialized_frame_encode_into_writes_the_frame_it_was_built_from() {
        let bytes = vec![0, 0, 1, 3, 0, 0, 0xaa, 0xbb, 0xcc];
        let frame = SerializedFrame::<Vec<u8>>::from_bytes(bytes.clone()).unwrap();
        assert_eq!(frame.encoded_length(), bytes.len());

        let mut dst = vec![0u8; bytes.len()];
        frame.encode_into(&mut dst).unwrap();
        assert_eq!(dst, bytes);
    }

    #[test]
    fn serialized_frame_encode_into_rejects_a_destination_of_another_length() {
        let bytes = vec![0, 0, 1, 3, 0, 0, 0xaa, 0xbb, 0xcc];

        for actual in [bytes.len() - 1, bytes.len() + 1] {
            let frame = SerializedFrame::<Vec<u8>>::from_bytes(bytes.clone()).unwrap();
            let mut dst = vec![0u8; actual];
            assert_eq!(
                frame.encode_into(&mut dst),
                Err(Error::UnexpectedDestinationLength {
                    expected: bytes.len(),
                    actual,
                })
            );
            assert!(dst.iter().all(|b| *b == 0));
        }
    }

    #[derive(Debug, Clone)]
    struct ValidU24(u32);

    impl Arbitrary for ValidU24 {
        fn arbitrary(g: &mut Gen) -> Self {
            ValidU24(u32::arbitrary(g) % 16_777_216)
        }
    }

    #[derive(Debug, Clone, PartialEq, Serialize)]
    struct TestMessage {
        data: B064KOwned,
    }

    impl Arbitrary for TestMessage {
        fn arbitrary(g: &mut Gen) -> Self {
            let size = usize::arbitrary(g) % 256;
            let data: Vec<u8> = (0..size).map(|_| u8::arbitrary(g)).collect();
            TestMessage {
                data: data.try_into().unwrap(),
            }
        }
    }

    #[quickcheck]
    fn prop_sv2frame_from_message_size_limit(msg: TestMessage) {
        let msg_type = 0x01u8;
        let extension_type = 0x0000u16;

        let frame =
            MessageFrame::<TestMessage>::from_message(msg.clone(), msg_type, extension_type, false);

        if msg.get_size() < 16_777_216 {
            assert!(
                frame.is_ok(),
                "Frame creation should succeed for message size {} < U24_MAX",
                msg.get_size()
            );
        } else {
            assert!(
                frame.is_err(),
                "Frame creation should fail for message size {} >= U24_MAX",
                msg.get_size()
            );
        }
    }

    /// Both encoders size their buffer with `encoded_length` and then call `serialize`, which
    /// checks the same length again. Derived `get_size` walks every element of a `Seq0255` or
    /// `Seq064K`, so each of those must read the length the header already holds instead.
    #[test]
    fn encoding_a_frame_walks_the_message_once() {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);

        struct CountingMsg;

        impl From<CountingMsg> for EncodableField<'_> {
            fn from(_: CountingMsg) -> Self {
                EncodableField::Struct(Vec::new())
            }
        }

        impl GetSize for CountingMsg {
            fn get_size(&self) -> usize {
                CALLS.fetch_add(1, Ordering::Relaxed);
                0
            }
        }

        let frame =
            MessageFrame::<CountingMsg>::from_message(CountingMsg, 0x01, 0x0000, false).unwrap();
        assert_eq!(
            CALLS.load(Ordering::Relaxed),
            1,
            "from_message sizes the header"
        );

        let len = frame.encoded_length();
        assert_eq!(len, Header::SIZE);
        assert_eq!(
            CALLS.load(Ordering::Relaxed),
            1,
            "encoded_length should read the header, not walk the message"
        );

        let mut dst = vec![0u8; len];
        frame.encode_into(&mut dst).unwrap();
        assert_eq!(
            CALLS.load(Ordering::Relaxed),
            1,
            "serialize should not walk the message again to check the destination length"
        );
    }

    /// The hand-written header serializer must agree byte for byte with the derived one it
    /// replaced, and round-trip through `Header::from_bytes`.
    #[quickcheck]
    fn prop_header_write_into_matches_the_derived_encoding(
        msg_length: ValidU24,
        msg_type: u8,
        extension_type: u16,
    ) {
        let header = Header::from_len(msg_length.0 as usize, msg_type, extension_type).unwrap();

        let mut hand = [0u8; Header::SIZE];
        header.write_into(&mut hand).unwrap();

        let mut derived = [0u8; Header::SIZE];
        binary_sv2::to_writer(header, &mut derived).unwrap();
        assert_eq!(hand, derived);

        let parsed = Header::from_bytes(&hand).unwrap();
        assert_eq!(parsed.payload_length(), msg_length.0 as usize);
        assert_eq!(parsed.msg_type(), msg_type);
    }

    #[test]
    fn header_write_into_rejects_a_short_destination() {
        let header = Header::from_len(0, 0, 0).unwrap();
        let mut dst = [0u8; Header::SIZE - 1];
        assert_eq!(
            header.write_into(&mut dst[..]),
            Err(Error::UnexpectedHeaderLength(Header::SIZE - 1))
        );
    }

    #[quickcheck]
    fn prop_sv2frame_encoded_length_consistency(msg: TestMessage) {
        let msg_type = 0x01u8;
        let extension_type = 0x0000u16;

        let frame =
            MessageFrame::<TestMessage>::from_message(msg.clone(), msg_type, extension_type, false)
                .unwrap();

        let encoded_len = frame.encoded_length();
        let expected_len = msg.get_size() + Header::SIZE;

        assert_eq!(
            encoded_len,
            expected_len,
            "Frame encoded_length() should be msg_size({}) + header_size({}), got {}",
            msg.get_size(),
            Header::SIZE,
            encoded_len
        );
    }

    #[quickcheck]
    fn prop_sv2frame_serialization_roundtrip_small(data: Vec<u8>) {
        let data: Vec<u8> = data.iter().take(1000).copied().collect();
        let msg = TestMessage {
            data: data.try_into().unwrap(),
        };
        let msg_type = 0x01u8;
        let extension_type = 0x0000u16;

        let frame =
            MessageFrame::<TestMessage>::from_message(msg.clone(), msg_type, extension_type, false)
                .unwrap();

        let mut buffer = vec![0u8; frame.encoded_length()];
        frame
            .encode_into(&mut buffer)
            .expect("Serialization should succeed");

        let deserialized =
            SerializedFrame::<Vec<u8>>::from_bytes(buffer).expect("Deserialization should succeed");

        let header = deserialized.header();
        assert_eq!(
            header.msg_type(),
            msg_type,
            "Message type should match after roundtrip"
        );
        assert_eq!(
            header.ext_type_without_channel_msg(),
            extension_type,
            "Extension type should match after roundtrip"
        );
        assert_eq!(
            header.payload_length(),
            msg.get_size(),
            "Payload length should match after roundtrip"
        );
    }

    #[quickcheck]
    fn prop_sv2frame_parse_header_exact_match(msg_length: ValidU24) {
        let msg_type = 0x01u8;
        let extension_type = 0x0000u16;

        let header = Header::from_len(msg_length.0 as usize, msg_type, extension_type).unwrap();

        let mut bytes = vec![0u8; Header::SIZE + msg_length.0 as usize];
        binary_sv2::to_writer(header, &mut bytes[..Header::SIZE]).unwrap();

        let parsed = SerializedFrame::<Vec<u8>>::parse_header(&bytes)
            .expect("parse_header should return the header when bytes hold exactly one frame");
        assert_eq!(parsed.payload_length(), msg_length.0 as usize);
        assert_eq!(parsed.msg_type(), msg_type);
    }

    #[quickcheck]
    fn prop_sv2frame_parse_header_insufficient_header(bytes: Vec<u8>) {
        let bytes: Vec<u8> = bytes.iter().take(Header::SIZE - 1).copied().collect();

        let hint = SerializedFrame::<Vec<u8>>::parse_header(&bytes).unwrap_err();
        assert_eq!(
            hint,
            SizeHint::Missing(Header::SIZE - bytes.len()),
            "parse_header should return the bytes missing to complete the header"
        );
    }

    #[quickcheck]
    fn prop_sv2frame_channel_msg_flag(msg: TestMessage, channel_msg: bool) {
        let msg_type = 0x01u8;
        let extension_type = 0x0ABCu16;

        // Only test with messages that fit in U24
        if msg.get_size() >= 16_777_216 {
            return;
        }

        let frame =
            MessageFrame::<TestMessage>::from_message(msg, msg_type, extension_type, channel_msg)
                .unwrap();

        let header = frame.header();
        assert_eq!(
            header.channel_msg(),
            channel_msg,
            "Frame channel_msg flag should be {} as specified in from_message",
            channel_msg
        );
    }

    #[quickcheck]
    fn prop_serialized_frame_payload_roundtrip(msg: TestMessage) {
        let msg_type = 0x01u8;
        let extension_type = 0x0000u16;
        let mut expected_payload = {
            let mut bytes = vec![0u8; msg.get_size()];
            binary_sv2::to_writer(msg.clone(), &mut bytes).unwrap();
            bytes
        };

        let frame = MessageFrame::<TestMessage>::from_message(msg, msg_type, extension_type, false)
            .unwrap();
        let mut buffer = vec![0u8; frame.encoded_length()];
        frame.encode_into(&mut buffer).unwrap();

        let mut frame = SerializedFrame::<Vec<u8>>::from_bytes(buffer).unwrap();
        assert_eq!(frame.payload(), expected_payload.as_mut_slice());
    }

    #[quickcheck]
    fn prop_message_frame_encode_into_destination_length(msg: TestMessage, delta: u8) {
        let delta = (delta % 8) as usize + 1;

        let frame = MessageFrame::<TestMessage>::from_message(msg, 0x01, 0x0000, false).unwrap();
        let required = frame.encoded_length();

        let mut too_short = vec![0u8; required - delta.min(required)];
        let actual = too_short.len();
        assert_eq!(
            frame.clone().encode_into(&mut too_short),
            Err(Error::UnexpectedDestinationLength {
                expected: required,
                actual
            })
        );

        let mut oversized = vec![0u8; required + delta];
        assert_eq!(
            frame.clone().encode_into(&mut oversized),
            Err(Error::UnexpectedDestinationLength {
                expected: required,
                actual: required + delta
            })
        );
        assert!(oversized.iter().all(|b| *b == 0));

        let mut exact = vec![0u8; required];
        assert!(frame.encode_into(&mut exact).is_ok());
        assert_eq!(
            SerializedFrame::<Vec<u8>>::from_bytes(exact)
                .unwrap()
                .encoded_length(),
            required
        );
    }

    /// The `channel_msg` bit promises a `U32` `channel_id` at the head of the payload.
    #[test]
    fn channel_id_is_read_only_when_the_channel_msg_bit_is_set() {
        let mut bytes = vec![0, 0x80, 1, 4, 0, 0, 0x2a, 0x00, 0x00, 0x00];
        let frame = SerializedFrame::<Vec<u8>>::from_bytes(bytes.clone()).unwrap();
        assert!(frame.header().channel_msg());
        assert_eq!(frame.channel_id(), Some(42));

        bytes[1] = 0;
        let frame = SerializedFrame::<Vec<u8>>::from_bytes(bytes).unwrap();
        assert!(!frame.header().channel_msg());
        assert_eq!(frame.channel_id(), None);

        let short = vec![0, 0x80, 1, 3, 0, 0, 0x2a, 0x00, 0x00];
        let frame = SerializedFrame::<Vec<u8>>::from_bytes(short).unwrap();
        assert_eq!(frame.channel_id(), None);

        let empty = vec![0, 0x80, 1, 0, 0, 0];
        let frame = SerializedFrame::<Vec<u8>>::from_bytes(empty).unwrap();
        assert_eq!(frame.channel_id(), None);
    }

    #[test]
    fn from_bytes_rejects_a_short_header() {
        assert_eq!(
            SerializedFrame::<Vec<u8>>::from_bytes(vec![0, 0, 1, 0, 0]).err(),
            Some(SizeHint::Missing(1))
        );
    }

    #[quickcheck]
    fn prop_handshake_message_roundtrip(payload: Vec<u8>) {
        let payload: Vec<u8> = payload.iter().take(1000).copied().collect();

        let frame = HandshakeMessage::from_message(&payload);
        let recovered = frame.payload();

        assert_eq!(
            recovered,
            payload,
            "HandshakeMessage roundtrip should preserve payload exactly (size: {})",
            payload.len()
        );
    }

    #[quickcheck]
    fn prop_handshake_message_from_bytes(payload: Vec<u8>) {
        let payload: Vec<u8> = payload.iter().take(1000).copied().collect();

        let frame = HandshakeMessage::from_bytes(payload.clone().into());

        let recovered = frame.payload();
        assert_eq!(
            recovered,
            payload,
            "Payload should be preserved through from_bytes (size: {})",
            payload.len()
        );
    }

    #[quickcheck]
    fn prop_update_extension_type_channel_msg_set(extension_type: u16) {
        let result = update_extension_type(extension_type, true);
        assert_ne!(
            result & 0b1000_0000_0000_0000,
            0,
            "update_extension_type with channel_msg=true should set MSB: input=0x{:04X}, output=0x{:04X}",
            extension_type,
            result
        );
    }

    #[quickcheck]
    fn prop_update_extension_type_channel_msg_unset(extension_type: u16) {
        let result = update_extension_type(extension_type, false);
        assert_eq!(
            result & 0b1000_0000_0000_0000,
            0,
            "update_extension_type with channel_msg=false should clear MSB: input=0x{:04X}, output=0x{:04X}",
            extension_type,
            result
        );
    }

    #[quickcheck]
    fn prop_update_extension_type_preserves_lower_bits_when_set(extension_type: u16) {
        let result = update_extension_type(extension_type, true);
        let lower_bits = extension_type & 0b0111_1111_1111_1111;
        let result_lower_bits = result & 0b0111_1111_1111_1111;

        assert_eq!(
            lower_bits, result_lower_bits,
            "update_extension_type should preserve lower 15 bits when setting MSB: input=0x{:04X}, expected_lower=0x{:04X}, got_lower=0x{:04X}",
            extension_type, lower_bits, result_lower_bits
        );
    }

    #[quickcheck]
    fn prop_update_extension_type_preserves_lower_bits_when_unset(extension_type: u16) {
        let result = update_extension_type(extension_type, false);
        let lower_bits = extension_type & 0b0111_1111_1111_1111;

        assert_eq!(
            result, lower_bits,
            "update_extension_type with channel_msg=false should return only lower 15 bits: input=0x{:04X}, expected=0x{:04X}, got=0x{:04X}",
            extension_type, lower_bits, result
        );
    }

    #[quickcheck]
    fn prop_parse_header_truncated_payload(msg_length: ValidU24, cut: u16) {
        let msg_type = 0x01u8;
        let ext = 0u16;

        let header = Header::from_len(msg_length.0 as usize, msg_type, ext).unwrap();

        let payload_len = msg_length.0 as usize;
        if payload_len == 0 {
            return;
        }

        let missing = (cut as usize % payload_len) + 1;
        let actual_payload = payload_len - missing;

        let mut bytes = vec![0u8; Header::SIZE + actual_payload];
        binary_sv2::to_writer(header, &mut bytes[..Header::SIZE]).unwrap();

        let hint = SerializedFrame::<Vec<u8>>::parse_header(&bytes).unwrap_err();

        assert_eq!(
            hint,
            SizeHint::Missing(missing),
            "parse_header should report the missing bytes"
        );
    }

    #[quickcheck]
    fn prop_parse_header_extra_bytes(msg_length: ValidU24, extra: u16) {
        let msg_type = 0x01u8;
        let ext = 0u16;

        let header = Header::from_len(msg_length.0 as usize, msg_type, ext).unwrap();

        let extra = (extra % 64 + 1) as usize;

        let mut bytes = vec![0u8; Header::SIZE + msg_length.0 as usize + extra];
        binary_sv2::to_writer(header, &mut bytes[..Header::SIZE]).unwrap();

        let hint = SerializedFrame::<Vec<u8>>::parse_header(&bytes).unwrap_err();

        assert_eq!(
            hint,
            SizeHint::Surplus(extra),
            "parse_header should report the number of extra bytes"
        );
    }

    #[quickcheck]
    fn prop_parse_header_incremental_arrival(msg_length: ValidU24) {
        let payload_len = (msg_length.0 % 4096) as usize;
        let header = Header::from_len(payload_len, 1, 0).unwrap();
        let total = Header::SIZE + payload_len;

        let mut full = vec![0u8; total];
        binary_sv2::to_writer(header, &mut full[..Header::SIZE]).unwrap();

        for i in 0..total {
            let hint = SerializedFrame::<Vec<u8>>::parse_header(&full[..i]).unwrap_err();
            let expected = if i < Header::SIZE {
                SizeHint::Missing(Header::SIZE - i)
            } else {
                SizeHint::Missing(total - i)
            };
            assert_eq!(hint, expected, "hint mismatch with {i} of {total} bytes");
        }

        let parsed = SerializedFrame::<Vec<u8>>::parse_header(&full).unwrap();
        assert_eq!(parsed.payload_length(), payload_len);
    }

    #[cfg(feature = "with_buffer_pool")]
    #[test]
    fn an_owned_copy_of_a_frame_frees_its_pool_slot() {
        use buffer_sv2::{Buffer, BufferPool};

        let bytes = [0, 0, 1, 2, 0, 0, 7, 8];
        let mut pool = BufferPool::new(64);
        pool.reserve(bytes.len()).copy_from_slice(&bytes);
        pool.commit(bytes.len());
        let frame = SerializedFrame::<Slice>::from_bytes(pool.get_data_owned()).unwrap();
        let pooled_at = frame.as_bytes().as_ptr();

        let owned = frame.into_owned();

        pool.reserve(bytes.len()).fill(0xaa);
        pool.commit(bytes.len());
        let reused = pool.get_data_owned();
        assert_eq!(reused.as_ref().as_ptr(), pooled_at);
        assert_eq!(owned.as_bytes(), &bytes);
    }
}
