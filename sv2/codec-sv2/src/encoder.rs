//! # Encoder
//!
//! Provides utilities for encoding messages into Sv2 frames, with or without Noise protocol
//! support.
//!
//! ## Usage
//!
//! All messages passed between Sv2 roles are encoded as Sv2 frames using primitives in this module.
//! There are two types of encoders for creating these frames: one for regular Sv2 frames
//! [`Encoder`], and another for Noise-encrypted frames (`NoiseEncoder`, under the `noise_sv2`
//! feature). Both encoders manage the serialization of outgoing data and, when applicable, the
//! encryption of the data before transmission.
//!
//! ### Buffer Management
//!
//! The encoders rely on buffers to hold intermediate data during the encoding process.
//!
//! - When the `with_buffer_pool` feature is enabled, the internal `Buffer` type is backed by a
//!   pool-allocated buffer [`buffer_sv2::BufferPool`], providing more efficient memory usage,
//!   particularly in high-throughput scenarios.
//! - If the feature is not enabled, a system memory buffer [`buffer_sv2::BufferFromSystemMemory`] is
//!   used for simpler applications where memory efficiency is less critical.

use framing_sv2::framing::EncodableFrame;

#[cfg(feature = "noise_sv2")]
use crate::{state::Segment, Result, TransportEncryptState, SV2_FRAME_PLAINTEXT_CHUNK_SIZE};
#[cfg(feature = "noise_sv2")]
use alloc::vec::Vec;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing::HandshakeMessage;
use framing_sv2::{SV2_FRAME_HEADER_SIZE, SV2_FRAME_MAX_PAYLOAD_SIZE};
#[cfg(feature = "noise_sv2")]
use noise_sv2::AEAD_MAC_LEN;

use crate::Buffer;
use buffer_sv2::Buffer as IsBuffer;

// Checks the length an `EncodableFrame` reports before anything is reserved for it: the trait is
// public, so a downstream impl can report less than a header or more than a header can declare.
fn check_encoded_length(len: usize) -> core::result::Result<(), framing_sv2::Error> {
    if len < SV2_FRAME_HEADER_SIZE {
        return Err(framing_sv2::Error::UnexpectedHeaderLength(len));
    }
    if len - SV2_FRAME_HEADER_SIZE > SV2_FRAME_MAX_PAYLOAD_SIZE {
        return Err(framing_sv2::Error::PayloadTooLong(
            len - SV2_FRAME_HEADER_SIZE,
        ));
    }
    Ok(())
}

// The number of payload chunks a frame of `len` encoded bytes is encrypted in. A frame with an
// empty payload has none: only its header is encrypted.
#[cfg(feature = "noise_sv2")]
fn payload_chunks(len: usize) -> usize {
    (len - SV2_FRAME_HEADER_SIZE).div_ceil(SV2_FRAME_PLAINTEXT_CHUNK_SIZE)
}

// The size of a frame of `len` encoded bytes once encrypted: one tag after the header, and one
// after each payload chunk.
#[cfg(feature = "noise_sv2")]
fn encrypted_length(len: usize) -> usize {
    len + AEAD_MAC_LEN * (1 + payload_chunks(len))
}

// Encodes `frame` into `dst` and encrypts it there, without any other buffer.
//
// `dst` must be exactly `encrypted_length(len)` bytes, where `len` is the frame's (already
// checked) encoded length. The plaintext frame is written one tag's length into `dst`, which puts
// the first payload chunk exactly where it sits in the encrypted frame: only the 6-byte header has
// to move to the front (the bytes it leaves behind fall where its tag goes), and each later chunk
// moves back by one tag per chunk before it (last first, since every chunk moves further than the
// one before it). A frame whose payload fits in one chunk never moves its payload. The header and
// each chunk are then encrypted in order, each tag landing in the room left behind it.
#[cfg(feature = "noise_sv2")]
fn encrypt_frame_in_place<F: EncodableFrame>(
    frame: F,
    len: usize,
    dst: &mut [u8],
    mut encrypt: impl FnMut(&mut Segment<'_>) -> Result<()>,
) -> Result<()> {
    debug_assert_eq!(dst.len(), encrypted_length(len));
    frame.encode_into(&mut dst[AEAD_MAC_LEN..AEAD_MAC_LEN + len])?;
    dst.copy_within(AEAD_MAC_LEN..AEAD_MAC_LEN + SV2_FRAME_HEADER_SIZE, 0);

    let payload_len = len - SV2_FRAME_HEADER_SIZE;
    let chunks = payload_chunks(len);
    let chunk_len = |i: usize| {
        (payload_len - i * SV2_FRAME_PLAINTEXT_CHUNK_SIZE).min(SV2_FRAME_PLAINTEXT_CHUNK_SIZE)
    };
    let encrypted_start = |i: usize| {
        SV2_FRAME_HEADER_SIZE + AEAD_MAC_LEN + i * (SV2_FRAME_PLAINTEXT_CHUNK_SIZE + AEAD_MAC_LEN)
    };

    for i in (1..chunks).rev() {
        let start = SV2_FRAME_HEADER_SIZE + AEAD_MAC_LEN + i * SV2_FRAME_PLAINTEXT_CHUNK_SIZE;
        dst.copy_within(start..start + chunk_len(i), encrypted_start(i));
    }

    let header_end = SV2_FRAME_HEADER_SIZE + AEAD_MAC_LEN;
    encrypt(&mut Segment::new(
        &mut dst[..header_end],
        SV2_FRAME_HEADER_SIZE,
    ))?;
    for i in 0..chunks {
        let start = encrypted_start(i);
        let end = start + chunk_len(i) + AEAD_MAC_LEN;
        encrypt(&mut Segment::new(&mut dst[start..end], chunk_len(i)))?;
    }
    Ok(())
}

/// Standard Sv2 encoder with Noise protocol support.
///
/// Used for encoding Sv2 frames encrypted via the Noise protocol.
#[cfg(feature = "noise_sv2")]
pub type NoiseEncoder = WithNoise<Buffer>;

/// Standard Sv2 encoder without Noise protocol support.
///
/// Used for encoding plain Sv2 frames.
pub type Encoder = WithoutNoise<Buffer>;

/// Encoder for Sv2 frames with Noise protocol encryption.
///
/// Serializes each Sv2 frame straight into the buffer it is sent from and encrypts it there, so a
/// frame is written once and never staged in a second buffer.
#[cfg(feature = "noise_sv2")]
pub struct WithNoise<B: IsBuffer> {
    // Buffer for holding encrypted Noise data to be transmitted.
    //
    // Each frame is serialized into it and encrypted in place, then handed out ready for
    // transmission. Not used by `encode_transport_into`, which writes to the caller's buffer.
    noise_buffer: B,
}

#[cfg(feature = "noise_sv2")]
impl<B: IsBuffer> WithNoise<B> {
    /// Encodes a handshake message, which is written out as it is: the handshake that produces
    /// these messages is not done setting up encryption yet.
    ///
    /// Nothing about that can fail, so unlike [`Self::encode_transport`] this hands the buffer
    /// straight back.
    #[inline]
    pub fn encode_handshake(&mut self, message: HandshakeMessage) -> B::Slice {
        let payload = message.payload();
        self.noise_buffer
            .reserve(payload.len())
            .copy_from_slice(payload);
        self.noise_buffer.commit(payload.len());

        self.noise_buffer.get_data_owned()
    }

    /// Serializes an Sv2 frame and encrypts it with the encrypting half of a completed handshake,
    /// returning the (`Slice`) (buffer) ready for transmission.
    ///
    /// Errors with [`framing_sv2::Error::UnexpectedHeaderLength`] on a frame whose
    /// `encoded_length` is too short to hold a header, and with
    /// [`framing_sv2::Error::PayloadTooLong`] on one whose payload a header cannot declare.
    #[inline]
    pub fn encode_transport<F: EncodableFrame>(
        &mut self,
        frame: F,
        state: &mut TransportEncryptState,
    ) -> Result<B::Slice> {
        self.encrypt_frame(frame, |segment| state.encrypt(segment))?;

        Ok(self.noise_buffer.get_data_owned())
    }

    // Encodes and encrypts `frame` into `noise_buffer`. Nothing is committed unless the whole
    // frame encrypts, so a failure leaves nothing behind for the next frame: its reservation
    // starts at the same place.
    #[inline]
    fn encrypt_frame<F: EncodableFrame>(
        &mut self,
        frame: F,
        encrypt: impl FnMut(&mut Segment<'_>) -> Result<()>,
    ) -> Result<()> {
        let len = frame.encoded_length();
        check_encoded_length(len)?;
        let encrypted_len = encrypted_length(len);
        encrypt_frame_in_place(
            frame,
            len,
            self.noise_buffer.reserve(encrypted_len),
            encrypt,
        )?;
        self.noise_buffer.commit(encrypted_len);
        Ok(())
    }

    /// Serializes an Sv2 frame and encrypts it like [`Self::encode_transport`], appending the
    /// result to `dst` instead of handing out a buffer.
    ///
    /// The frame is written and encrypted in `dst` itself, which is only ever grown, so a caller
    /// that keeps one buffer for its writes (clearing it between them) stops allocating once that
    /// buffer has room for its largest write. Several frames can be appended back to back to go
    /// out in one write.
    ///
    /// On error, `dst` is left as it was. Errors like [`Self::encode_transport`].
    #[inline]
    pub fn encode_transport_into<F: EncodableFrame>(
        &mut self,
        frame: F,
        state: &mut TransportEncryptState,
        dst: &mut Vec<u8>,
    ) -> Result<()> {
        append_encrypted_frame(frame, dst, |segment| state.encrypt(segment))
    }
}

// Encodes and encrypts `frame` onto the end of `dst`, leaving `dst` as it was on error.
#[cfg(feature = "noise_sv2")]
#[inline]
fn append_encrypted_frame<F: EncodableFrame>(
    frame: F,
    dst: &mut Vec<u8>,
    encrypt: impl FnMut(&mut Segment<'_>) -> Result<()>,
) -> Result<()> {
    let len = frame.encoded_length();
    check_encoded_length(len)?;
    let start = dst.len();
    dst.resize(start + encrypted_length(len), 0);
    let result = encrypt_frame_in_place(frame, len, &mut dst[start..], encrypt);
    if result.is_err() {
        dst.truncate(start);
    }
    result
}

#[cfg(feature = "noise_sv2")]
impl WithNoise<Buffer> {
    /// Creates a new `NoiseEncoder` with default buffer sizes.
    pub fn new() -> Self {
        Self::with_capacity(crate::DEFAULT_POOL_BUFFER_SIZE)
    }

    /// Creates a new `NoiseEncoder` like [`Self::new`], with its buffer holding `capacity` bytes.
    ///
    /// Only the `with_buffer_pool` feature preallocates that memory: a frame that does not fit
    /// spills onto the heap. Without it the buffer grows on the heap and `capacity` is unused.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            noise_buffer: Buffer::new(capacity),
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl Default for WithNoise<Buffer> {
    fn default() -> Self {
        Self::new()
    }
}

/// Encoder for standard Sv2 frames.
///
/// Serializes the Sv2 frame into a dedicated buffer then encodes it, such that it is ready for
/// transmission.
#[derive(Debug)]
pub struct WithoutNoise<B: IsBuffer> {
    // Buffer for holding serialized Sv2 data.
    //
    // Stores the serialized bytes of the Sv2 frame after it has been encoded. Once the frame is
    // serialized, the resulting bytes are stored in this buffer to be transmitted. The buffer is
    // dynamically resized to accommodate the size of the encoded frame.
    buffer: B,
}

impl<B: IsBuffer> WithoutNoise<B> {
    /// Encodes a standard Sv2 frame for transmission.
    ///
    /// Serializes `item` into a byte stream. The resulting bytes are stored in the internal
    /// `buffer`, preparing the frame for transmission. On success, the method returns a reference
    /// to the serialized bytes stored in the internal buffer. Otherwise, errors on a
    /// serialization failure, with [`framing_sv2::Error::UnexpectedHeaderLength`] on a frame
    /// whose `encoded_length` is too short to hold a header, or with
    /// [`framing_sv2::Error::PayloadTooLong`] on one whose payload a header cannot declare.
    pub fn encode<F: EncodableFrame>(&mut self, item: F) -> crate::Result<B::Slice> {
        let len = item.encoded_length();
        check_encoded_length(len)?;
        item.encode_into(self.buffer.reserve(len))?;
        self.buffer.commit(len);

        Ok(self.buffer.get_data_owned())
    }
}

impl WithoutNoise<Buffer> {
    /// Creates a new `Encoder` with a buffer of default size.
    pub fn new() -> Self {
        Self::with_capacity(crate::DEFAULT_POOL_BUFFER_SIZE)
    }

    /// Creates a new `Encoder` like [`Self::new`], with its buffer holding `capacity` bytes.
    ///
    /// Only the `with_buffer_pool` feature preallocates that memory: a frame that does not fit
    /// spills onto the heap. Without it the buffer grows on the heap and `capacity` is unused.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Buffer::new(capacity),
        }
    }
}

impl Default for WithoutNoise<Buffer> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use framing_sv2::framing::SerializedFrame;

    struct FailingFrame;

    impl EncodableFrame for FailingFrame {
        fn encoded_length(&self) -> usize {
            10
        }

        fn encode_into(self, _dst: &mut [u8]) -> core::result::Result<(), framing_sv2::Error> {
            Err(framing_sv2::Error::UnexpectedDestinationLength {
                expected: 10,
                actual: 0,
            })
        }
    }

    /// A frame that fails to encode must not leave its reserved bytes in front of the next frame.
    #[test]
    fn failed_encode_does_not_leak_reserved_bytes() {
        let mut encoder = Encoder::new();

        assert!(encoder.encode(FailingFrame).is_err());

        let bytes = vec![0, 0, 1, 0, 0, 0];
        let frame = SerializedFrame::<Vec<u8>>::from_bytes(bytes.clone()).unwrap();
        let encoded = encoder.encode(frame).unwrap();
        let encoded: &[u8] = encoded.as_ref();
        assert_eq!(encoded, &bytes[..]);
    }

    struct Raw(Vec<u8>);

    impl EncodableFrame for Raw {
        fn encoded_length(&self) -> usize {
            self.0.len()
        }

        fn encode_into(self, dst: &mut [u8]) -> core::result::Result<(), framing_sv2::Error> {
            dst[..self.0.len()].copy_from_slice(&self.0);
            Ok(())
        }
    }

    /// `EncodableFrame` is public and user-implementable, so an impl reporting a length too short
    /// to hold a header must be rejected rather than sent out as something no peer can frame.
    #[test]
    fn the_plain_encoder_rejects_a_frame_shorter_than_a_header() {
        let mut encoder = Encoder::new();

        for len in 0..SV2_FRAME_HEADER_SIZE {
            let err = encoder
                .encode(Raw(vec![0; len]))
                .err()
                .unwrap_or_else(|| panic!("a {len}-byte frame should be rejected"));
            assert_eq!(err, framing_sv2::Error::UnexpectedHeaderLength(len).into());
        }

        let bytes = vec![0, 0, 1, 0, 0, 0];
        let encoded = encoder.encode(Raw(bytes.clone())).unwrap();
        let encoded: &[u8] = encoded.as_ref();
        assert_eq!(encoded, &bytes[..]);
    }

    /// `EncodableFrame` is public and user-implementable, so an impl reporting a length too short
    /// to hold a header must be rejected rather than indexed past the end of.
    #[cfg(feature = "noise_sv2")]
    #[test]
    fn a_frame_shorter_than_a_header_is_rejected_not_panicked_on() {
        let (mut sender, _) = crate::test_utils::make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();

        for len in 0..SV2_FRAME_HEADER_SIZE {
            let err = encoder
                .encode_transport(Raw(vec![0; len]), &mut sender)
                .err()
                .unwrap_or_else(|| panic!("a {len}-byte frame should be rejected"));
            assert_eq!(err, framing_sv2::Error::UnexpectedHeaderLength(len).into());
        }

        let bytes = vec![0, 0, 1, 0, 0, 0];
        assert!(encoder.encode_transport(Raw(bytes), &mut sender).is_ok());
    }

    // Reports a length without holding the bytes, and must be rejected before it is written.
    struct ClaimsLength(usize);

    impl EncodableFrame for ClaimsLength {
        fn encoded_length(&self) -> usize {
            self.0
        }

        fn encode_into(self, _dst: &mut [u8]) -> core::result::Result<(), framing_sv2::Error> {
            unreachable!("a frame longer than a header can declare must not be written")
        }
    }

    #[test]
    fn the_plain_encoder_rejects_a_frame_longer_than_a_header_can_declare() {
        let mut encoder = Encoder::new();

        let len = SV2_FRAME_HEADER_SIZE + SV2_FRAME_MAX_PAYLOAD_SIZE + 1;
        assert_eq!(
            encoder.encode(ClaimsLength(len)).err(),
            Some(framing_sv2::Error::PayloadTooLong(SV2_FRAME_MAX_PAYLOAD_SIZE + 1).into())
        );
        assert_eq!(
            encoder.encode(ClaimsLength(usize::MAX)).err(),
            Some(framing_sv2::Error::PayloadTooLong(usize::MAX - SV2_FRAME_HEADER_SIZE).into())
        );

        let bytes = vec![0, 0, 1, 0, 0, 0];
        let encoded = encoder.encode(Raw(bytes.clone())).unwrap();
        let encoded: &[u8] = encoded.as_ref();
        assert_eq!(encoded, &bytes[..]);
    }

    #[cfg(feature = "noise_sv2")]
    #[test]
    fn the_noise_encoder_rejects_a_frame_longer_than_a_header_can_declare() {
        let (mut sender, _) = crate::test_utils::make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();

        let len = SV2_FRAME_HEADER_SIZE + SV2_FRAME_MAX_PAYLOAD_SIZE + 1;
        assert_eq!(
            encoder
                .encode_transport(ClaimsLength(len), &mut sender)
                .err(),
            Some(framing_sv2::Error::PayloadTooLong(SV2_FRAME_MAX_PAYLOAD_SIZE + 1).into())
        );

        let bytes = vec![0, 0, 1, 0, 0, 0];
        assert!(encoder.encode_transport(Raw(bytes), &mut sender).is_ok());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use binary_sv2::{Deserialize, Serialize};
    // Not redundant: the glob import above brings in `crate::Result`, whose single type parameter
    // the code generated by the `Deserialize` derive cannot use.
    use core::result::Result;
    use framing_sv2::framing::MessageFrame;
    use quickcheck::{Arbitrary, Gen, TestResult};
    use quickcheck_macros::quickcheck;
    #[cfg(feature = "noise_sv2")]
    use {
        crate::test_utils::{decode_noise_frame, make_transport_state_pair},
        noise_sv2::AEAD_MAC_LEN,
    };

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct TestMessage {
        value: u16,
    }

    impl Arbitrary for TestMessage {
        fn arbitrary(g: &mut Gen) -> Self {
            TestMessage {
                value: u16::arbitrary(g),
            }
        }
    }

    /// Verifies that encoding any valid Sv2 frame produces non-empty bytes within the
    /// frame's declared encoded length.
    #[quickcheck]
    fn prop_encoder_encode(
        msg: TestMessage,
        msg_type: u8,
        ext_type: u16,
        channel_msg: bool,
    ) -> TestResult {
        let frame =
            match MessageFrame::<TestMessage>::from_message(msg, msg_type, ext_type, channel_msg) {
                Ok(f) => f,
                Err(_) => return TestResult::discard(),
            };

        let mut encoder = Encoder::new();
        match encoder.encode(frame.clone()) {
            Ok(data) => {
                let bytes: &[u8] = data.as_ref();
                TestResult::from_bool(!bytes.is_empty() && bytes.len() <= frame.encoded_length())
            }
            Err(_) => TestResult::failed(),
        }
    }

    /// Verifies that the encoder can encode multiple messages sequentially without errors.
    #[quickcheck]
    fn prop_encoder_reusable(msg1: TestMessage, msg2: TestMessage, msg_type: u8) -> TestResult {
        let frame1 = match MessageFrame::<TestMessage>::from_message(msg1, msg_type, 0, false) {
            Ok(f) => f,
            Err(_) => return TestResult::discard(),
        };
        let frame2 = match MessageFrame::<TestMessage>::from_message(msg2, msg_type, 0, false) {
            Ok(f) => f,
            Err(_) => return TestResult::discard(),
        };

        let mut encoder = Encoder::new();
        let ok1 = encoder.encode(frame1).is_ok();
        let ok2 = encoder.encode(frame2).is_ok();
        TestResult::from_bool(ok1 && ok2)
    }

    /// Verifies that encrypting any valid frame with `NoiseEncoder`
    /// in transport mode produces non-empty output.
    #[cfg(feature = "noise_sv2")]
    #[quickcheck]
    fn prop_noise_encoder_encode(
        msg: TestMessage,
        msg_type: u8,
        ext_type: u16,
        channel_msg: bool,
    ) -> TestResult {
        let frame =
            match MessageFrame::<TestMessage>::from_message(msg, msg_type, ext_type, channel_msg) {
                Ok(f) => f,
                Err(_) => return TestResult::discard(),
            };

        let (mut sender_enc, _) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        match encoder.encode_transport(frame, &mut sender_enc) {
            Ok(data) => {
                let bytes: &[u8] = data.as_ref();
                TestResult::from_bool(!bytes.is_empty())
            }
            Err(_) => TestResult::failed(),
        }
    }

    #[cfg(feature = "noise_sv2")]
    #[test]
    fn noise_encoder_recovers_from_a_failed_chunk_encryption() {
        let frame =
            MessageFrame::<TestMessage>::from_message(TestMessage { value: 1 }, 0, 0, false)
                .unwrap();

        let mut encoder = NoiseEncoder::new();

        // Let the header through and fail on the first payload chunk: that is the point where the
        // buffer already holds the encrypted header.
        let mut calls = 0;
        let result = encoder.encrypt_frame(frame, |buf| {
            calls += 1;
            if calls == 1 {
                // Stand in for the header encryption, which grows the buffer by a MAC.
                noise_sv2::AeadBuffer::extend_from_slice(buf, &[0; AEAD_MAC_LEN]).unwrap();
                Ok(())
            } else {
                Err(crate::Error::AeadError(noise_sv2::AeadError))
            }
        });
        assert!(result.is_err());
        assert_eq!(IsBuffer::len(&encoder.noise_buffer), 0);
        assert!(encoder.noise_buffer.frame().is_empty());

        // The next frame must be one the peer can open, with nothing of the failed one in it.
        let (mut sender_enc, receiver_dec) = make_transport_state_pair();
        let next = MessageFrame::<TestMessage>::from_message(TestMessage { value: 2 }, 0, 0, false)
            .unwrap();
        let encrypted = encoder.encode_transport(next, &mut sender_enc).unwrap();

        let mut decoder = crate::NoiseDecoder::new();
        let (mut decoded, _) = decode_noise_frame(&mut decoder, receiver_dec, encrypted.as_ref())
            .expect("failed to decode the frame after a failed encode");
        assert_eq!(
            binary_sv2::from_bytes::<TestMessage>(decoded.payload()).unwrap(),
            TestMessage { value: 2 }
        );
    }

    /// Verifies that `NoiseEncoder` can encrypt multiple frames
    /// sequentially with the same transport state without errors.
    #[cfg(feature = "noise_sv2")]
    #[quickcheck]
    fn prop_noise_encoder_reusable(
        msg1: TestMessage,
        msg2: TestMessage,
        msg_type: u8,
    ) -> TestResult {
        let frame1 = match MessageFrame::<TestMessage>::from_message(msg1, msg_type, 0, false) {
            Ok(f) => f,
            Err(_) => return TestResult::discard(),
        };
        let frame2 = match MessageFrame::<TestMessage>::from_message(msg2, msg_type, 0, false) {
            Ok(f) => f,
            Err(_) => return TestResult::discard(),
        };

        let (mut sender_enc, _) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        let ok1 = encoder.encode_transport(frame1, &mut sender_enc).is_ok();
        let ok2 = encoder.encode_transport(frame2, &mut sender_enc).is_ok();
        TestResult::from_bool(ok1 && ok2)
    }
}

#[cfg(all(test, feature = "noise_sv2"))]
mod in_place_tests {
    use super::*;
    use crate::test_utils::{decode_noise_frame, make_transport_state_pair};
    use crate::NoiseDecoder;
    use alloc::vec;
    use framing_sv2::framing::SerializedFrame;

    const MSG_TYPE: u8 = 0x15;
    const CHUNK: usize = SV2_FRAME_PLAINTEXT_CHUNK_SIZE;

    // Every boundary the in-place layout has: no payload, a partial chunk, exactly one chunk, one
    // byte into a second chunk, and several chunks with a partial last one.
    const PAYLOAD_LENS: [usize; 7] = [0, 1, 100, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK + 7];

    // A serialized frame whose payload is `len` bytes of a pattern that differs between chunks,
    // so a chunk written to the wrong place cannot decode to the right payload.
    fn frame(len: usize) -> SerializedFrame<Vec<u8>> {
        let mut bytes = vec![0, 0, MSG_TYPE];
        bytes.extend_from_slice(&(len as u32).to_le_bytes()[..3]);
        bytes.extend((0..len).map(|i| (i % 251) as u8));
        SerializedFrame::from_bytes(bytes).expect("a well-formed frame")
    }

    fn expected_payload(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn frames_appended_into_one_buffer_decrypt_back_in_order() {
        let (mut sender, mut receiver) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();

        let mut dst = Vec::new();
        let mut ends = Vec::new();
        for len in PAYLOAD_LENS {
            encoder
                .encode_transport_into(frame(len), &mut sender, &mut dst)
                .unwrap();
            ends.push(dst.len());
        }

        let mut decoder = NoiseDecoder::new();
        let mut start = 0;
        for (len, end) in PAYLOAD_LENS.into_iter().zip(ends) {
            assert_eq!(end - start, encrypted_length(SV2_FRAME_HEADER_SIZE + len));
            let (mut decoded, next) = decode_noise_frame(&mut decoder, receiver, &dst[start..end])
                .unwrap_or_else(|| panic!("a {len}-byte payload failed to decrypt"));
            assert_eq!(decoded.header().msg_type(), MSG_TYPE);
            assert_eq!(decoded.payload(), &expected_payload(len)[..]);
            receiver = next;
            start = end;
        }
    }

    #[test]
    fn encode_transport_frames_of_every_size_decrypt_back() {
        let (mut sender, mut receiver) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        let mut decoder = NoiseDecoder::new();

        for len in PAYLOAD_LENS {
            let encrypted = encoder.encode_transport(frame(len), &mut sender).unwrap();
            let encrypted: &[u8] = encrypted.as_ref();
            assert_eq!(
                encrypted.len(),
                encrypted_length(SV2_FRAME_HEADER_SIZE + len)
            );
            let (mut decoded, next) = decode_noise_frame(&mut decoder, receiver, encrypted)
                .unwrap_or_else(|| panic!("a {len}-byte payload failed to decrypt"));
            assert_eq!(decoded.payload(), &expected_payload(len)[..]);
            receiver = next;
        }
    }

    #[test]
    fn a_kept_buffer_is_written_without_reallocating() {
        let (mut sender, mut receiver) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        let mut decoder = NoiseDecoder::new();

        let mut dst = Vec::with_capacity(4096);
        let allocation = dst.as_ptr();
        for _ in 0..3 {
            dst.clear();
            encoder
                .encode_transport_into(frame(1000), &mut sender, &mut dst)
                .unwrap();
            assert_eq!(dst.as_ptr(), allocation);
            assert_eq!(dst.capacity(), 4096);

            let (mut decoded, next) = decode_noise_frame(&mut decoder, receiver, &dst).unwrap();
            assert_eq!(decoded.payload(), &expected_payload(1000)[..]);
            receiver = next;
        }
    }

    // Reports a length without holding the bytes, and fails when asked to write them.
    struct FailsToEncode(usize);

    impl EncodableFrame for FailsToEncode {
        fn encoded_length(&self) -> usize {
            self.0
        }

        fn encode_into(self, dst: &mut [u8]) -> core::result::Result<(), framing_sv2::Error> {
            Err(framing_sv2::Error::UnexpectedDestinationLength {
                expected: self.0,
                actual: dst.len(),
            })
        }
    }

    #[test]
    fn a_failed_frame_leaves_the_buffer_as_it_was() {
        let (mut sender, receiver) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        let mut dst = vec![1, 2, 3];

        // Rejected before anything is written.
        let too_long = SV2_FRAME_HEADER_SIZE + SV2_FRAME_MAX_PAYLOAD_SIZE + 1;
        assert!(encoder
            .encode_transport_into(FailsToEncode(too_long), &mut sender, &mut dst)
            .is_err());
        assert_eq!(dst, [1, 2, 3]);

        // Fails while being written.
        assert!(encoder
            .encode_transport_into(FailsToEncode(10), &mut sender, &mut dst)
            .is_err());
        assert_eq!(dst, [1, 2, 3]);

        // Fails while being encrypted, after the header went through.
        let mut calls = 0;
        let result = append_encrypted_frame(frame(CHUNK + 1), &mut dst, |segment| {
            calls += 1;
            if calls == 1 {
                noise_sv2::AeadBuffer::extend_from_slice(segment, &[0; AEAD_MAC_LEN])
            } else {
                Err(noise_sv2::AeadError)
            }
            .map_err(crate::Error::AeadError)
        });
        assert!(result.is_err());
        assert_eq!(dst, [1, 2, 3]);

        // Neither failure touched the transport state, so the next frame still opens.
        encoder
            .encode_transport_into(frame(5), &mut sender, &mut dst)
            .unwrap();
        let mut decoder = NoiseDecoder::new();
        let (mut decoded, _) = decode_noise_frame(&mut decoder, receiver, &dst[3..]).unwrap();
        assert_eq!(decoded.payload(), &expected_payload(5)[..]);
    }
}
