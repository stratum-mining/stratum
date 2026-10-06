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
use crate::{state::Chunk, Result, TransportEncryptState, SV2_FRAME_PLAINTEXT_CHUNK_SIZE};
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing::HandshakeMessage;
use framing_sv2::SV2_FRAME_HEADER_SIZE;

use crate::Buffer;
use buffer_sv2::Buffer as IsBuffer;

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
/// Serializes the Sv2 frame into a dedicated buffer. Encrypts this serialized data using the Noise
/// protocol, storing it into another dedicated buffer. Encodes the serialized and encrypted data,
/// such that it is ready for transmission.
#[cfg(feature = "noise_sv2")]
pub struct WithNoise<B: IsBuffer> {
    // Buffer for holding encrypted Noise data to be transmitted.
    //
    // Stores the encrypted data after the Sv2 frame has been processed by the Noise protocol
    // and is ready for transmission. This buffer holds the outgoing encrypted data, ensuring
    // that the full frame is correctly prepared before being sent.
    noise_buffer: B,

    // Buffer for holding serialized Sv2 data before encryption.
    //
    // Stores the data after it has been serialized into an Sv2 frame but before it is encrypted
    // by the Noise protocol. The buffer accumulates the frame's serialized bytes before they are
    // encrypted and then encoded for transmission.
    sv2_buffer: B,
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
    /// `encoded_length` is too short to hold a header.
    #[inline]
    pub fn encode_transport<F: EncodableFrame>(
        &mut self,
        frame: F,
        state: &mut TransportEncryptState,
    ) -> Result<B::Slice> {
        self.encrypt_frame(frame, |buf| state.encrypt(buf))?;

        Ok(self.noise_buffer.get_data_owned())
    }

    #[inline]
    fn encrypt_frame<F: EncodableFrame>(
        &mut self,
        frame: F,
        encrypt: impl FnMut(&mut Chunk<'_, B>) -> Result<()>,
    ) -> Result<()> {
        let result = self.try_encrypt_frame(frame, encrypt);

        if result.is_err() {
            self.noise_buffer.get_data_owned();
            self.sv2_buffer.get_data_owned();
        }

        result
    }

    #[inline]
    fn try_encrypt_frame<F: EncodableFrame>(
        &mut self,
        frame: F,
        mut encrypt: impl FnMut(&mut Chunk<'_, B>) -> Result<()>,
    ) -> Result<()> {
        let len = frame.encoded_length();
        if len < SV2_FRAME_HEADER_SIZE {
            return Err(framing_sv2::Error::UnexpectedHeaderLength(len).into());
        }
        // ENCODE THE SV2 FRAME
        frame.encode_into(self.sv2_buffer.reserve(len))?;
        self.sv2_buffer.commit(len);

        let sv2 = self.sv2_buffer.get_data_owned();
        let sv2: &[u8] = sv2.as_ref();

        // ENCRYPT THE HEADER
        self.noise_buffer
            .reserve(SV2_FRAME_HEADER_SIZE)
            .copy_from_slice(&sv2[..SV2_FRAME_HEADER_SIZE]);
        self.noise_buffer.commit(SV2_FRAME_HEADER_SIZE);
        encrypt(&mut Chunk::new(&mut self.noise_buffer, 0))?;

        // ENCRYPT THE PAYLOAD IN CHUNKS
        let mut start = SV2_FRAME_HEADER_SIZE;
        while start < sv2.len() {
            let end = (start + SV2_FRAME_PLAINTEXT_CHUNK_SIZE).min(sv2.len());
            let chunk_start = IsBuffer::len(&self.noise_buffer);
            self.noise_buffer
                .reserve(end - start)
                .copy_from_slice(&sv2[start..end]);
            self.noise_buffer.commit(end - start);
            encrypt(&mut Chunk::new(&mut self.noise_buffer, chunk_start))?;
            start = end;
        }
        Ok(())
    }
}

#[cfg(feature = "noise_sv2")]
impl WithNoise<Buffer> {
    /// Creates a new `NoiseEncoder` with default buffer sizes.
    pub fn new() -> Self {
        Self {
            sv2_buffer: Buffer::new(crate::DEFAULT_POOL_BUFFER_SIZE),
            noise_buffer: Buffer::new(crate::DEFAULT_POOL_BUFFER_SIZE),
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
    /// serialization failure, or with [`framing_sv2::Error::UnexpectedHeaderLength`] on a frame
    /// whose `encoded_length` is too short to hold a header.
    pub fn encode<F: EncodableFrame>(&mut self, item: F) -> crate::Result<B::Slice> {
        let len = item.encoded_length();
        if len < SV2_FRAME_HEADER_SIZE {
            return Err(framing_sv2::Error::UnexpectedHeaderLength(len).into());
        }
        item.encode_into(self.buffer.reserve(len))?;
        self.buffer.commit(len);

        Ok(self.buffer.get_data_owned())
    }
}

impl WithoutNoise<Buffer> {
    /// Creates a new `Encoder` with a buffer of default size.
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(crate::DEFAULT_POOL_BUFFER_SIZE),
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
