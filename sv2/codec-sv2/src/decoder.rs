//! # Decoder
//!
//! Provides utilities for decoding messages held by Sv2 frames, with or without Noise protocol
//! support.
//!
//! It includes primitives to both decode encoded standard Sv2 frames and to decrypt and decode
//! Noise-encrypted encoded Sv2 frames, ensuring secure communication when required.
//!
//! ## Usage
//! All messages passed between Sv2 roles are encoded as Sv2 frames. These frames are decoded using
//! primitives in this module. There are two types of decoders for reading these frames: one for
//! regular Sv2 frames [`Decoder`], and another for Noise-encrypted frames `NoiseDecoder` (under
//! the `noise_sv2` feature). Both decoders manage the deserialization of incoming data and, when
//! applicable, the decryption of the data upon receiving the transmitted message.
//!
//! ### Buffer Management
//!
//! The decoders rely on buffers to hold intermediate data during the decoding process.
//!
//! - When the `with_buffer_pool` feature is enabled, the internal `Buffer` type is backed by a
//!   pool-allocated buffer [`buffer_sv2::BufferPool`], providing more efficient memory usage,
//!   particularly in high-throughput scenarios.
//! - If this feature is not enabled, a system memory buffer [`buffer_sv2::BufferFromSystemMemory`]
//!   is used for simpler applications where memory efficiency is less critical.

#[cfg(feature = "noise_sv2")]
use buffer_sv2::AeadBuffer;
use buffer_sv2::Buffer as IsBuffer;
#[cfg(feature = "noise_sv2")]
use framing_sv2::{framing::HandshakeMessage, SV2_FRAME_HEADER_SIZE};
use framing_sv2::{
    framing::{SerializedFrame, SizeHint},
    header::Header,
    SV2_FRAME_CHUNK_SIZE,
};

use crate::{
    error::{Error, Result},
    Buffer,
};
#[cfg(feature = "noise_sv2")]
use crate::{
    state::ExpectsHandshakeMessage, TransportDecryptState, ENCRYPTED_SV2_FRAME_HEADER_SIZE,
};

/// Sv2 decoder with Noise protocol support.
///
/// Used for decoding Sv2 frames encrypted via the Noise protocol.
#[cfg(feature = "noise_sv2")]
pub type NoiseDecoder = WithNoise<Buffer>;

/// Sv2 decoder without Noise protocol support.
///
/// Used for decoding plain Sv2 frames.
pub type Decoder = WithoutNoise<Buffer>;

/// Decoder for Sv2 frames with Noise protocol support.
///
/// Accumulates the encrypted data into a dedicated buffer until the entire encrypted frame is
/// received. The Noise protocol is then used to decrypt the accumulated data into another
/// dedicated buffer, converting it back into its original serialized form. This decrypted data is
/// then deserialized into the original Sv2 frame and message format.
#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
pub struct WithNoise<B: IsBuffer> {
    // Buffer for holding incoming encrypted Noise data to be decrypted.
    //
    // Stores the incoming encrypted data, allowing the decoder to accumulate the necessary bytes
    // for full decryption. Once the entire encrypted frame is received, the decoder processes the
    // buffer to extract the underlying frame.
    noise_buffer: B,

    // Buffer for holding decrypted data to be decoded.
    //
    // Stores the decrypted data until it is ready to be processed and converted into a Sv2 frame.
    sv2_buffer: B,

    // Number of encrypted bytes still missing before the current frame can progress.
    //
    // Set from the header once it is decrypted, and to the size of the next expected message
    // or header otherwise; [`Self::read_len`] caps it at one chunk.
    missing_noise_b: usize,

    // Size of the window the last `read_buf` returned, `0` once it is advanced or a frame step
    // ends it.
    window: usize,
}

/// The outcome of a decode round.
#[derive(Debug)]
pub enum Decoded<F> {
    /// A complete frame.
    Frame(F),
    /// The frame is not fully buffered yet.
    ///
    /// Carries the number of bytes the decoder will accept on the next read, which is always
    /// `read_len` and never more than one chunk, not the number of bytes left in the frame.
    /// Zero means the bytes are already buffered: call again without reading.
    Incomplete(usize),
}

/// The outcome of a transport-mode decode round.
///
/// Both variants hand the decrypting state back, since a round that did not fail leaves it usable
/// for the next one. A failed round returns an `Err` instead and the state is gone with it.
#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
pub enum Decrypted<F> {
    /// A complete frame, and the state to decrypt the next one with.
    Frame(F, TransportDecryptState),
    /// The frame is not fully buffered yet.
    ///
    /// Carries the number of bytes the decoder will accept on the next read, which is always
    /// [`WithNoise::read_len`] and never more than one chunk, not the number of bytes left in
    /// the frame, along with the state to call again with. Zero means the bytes are already
    /// buffered: call again without reading.
    Incomplete(usize, TransportDecryptState),
}

#[cfg(feature = "noise_sv2")]
impl<B: IsBuffer + AeadBuffer> WithNoise<B> {
    /// Attempts to decode the next handshake frame.
    ///
    /// Handshake messages have a fixed size that depends on the role `state` plays, so no header
    /// is read: the decoder buffers exactly that many bytes.
    ///
    /// On [`Decoded::Incomplete`], read another chunk from the stream into [`Self::read_buf`],
    /// report it with [`Self::advance`], and call this method again until it returns a
    /// [`Decoded::Frame`]. The count it carries is what the decoder will accept on the next read,
    /// which is always [`Self::read_len`] and never more than one chunk, not the number of bytes
    /// left in the message.
    ///
    /// Bytes buffered past the end of the message are kept as the start of the next one, or of
    /// the first encrypted header.
    ///
    /// Only a state that is waiting on its counterpart can be read for. An [`crate::Handshake`]
    /// in the [`noise_sv2::Initiator`] role has sent nothing yet, so nothing is coming back:
    ///
    /// ```compile_fail,E0277
    /// use codec_sv2::NoiseDecoder;
    /// use noise_sv2::Initiator;
    ///
    /// let mut decoder = NoiseDecoder::new();
    /// let _ = decoder.next_handshake_frame::<Initiator>();
    /// ```
    #[inline]
    pub fn next_handshake_frame<R: ExpectsHandshakeMessage>(
        &mut self,
    ) -> Result<Decoded<HandshakeMessage>> {
        self.window = 0;
        if let Some(missing) = self.missing(R::EXPECTED_MESSAGE_SIZE) {
            return Ok(Decoded::Incomplete(missing));
        }
        let message = self.take(R::EXPECTED_MESSAGE_SIZE);
        self.expect(ENCRYPTED_SV2_FRAME_HEADER_SIZE);
        Ok(Decoded::Frame(HandshakeMessage::from_message(
            &message.as_ref()[..R::EXPECTED_MESSAGE_SIZE],
        )))
    }

    fn missing(&mut self, expected: usize) -> Option<usize> {
        match expected.saturating_sub(IsBuffer::len(&self.noise_buffer)) {
            0 => None,
            missing => {
                self.missing_noise_b = missing;
                Some(self.read_len())
            }
        }
    }

    fn take(&mut self, expected: usize) -> B::Slice {
        let bytes = self.noise_buffer.get_data_owned();
        let carried = &bytes.as_ref()[expected..];
        if !carried.is_empty() {
            self.noise_buffer
                .reserve(carried.len())
                .copy_from_slice(carried);
            self.noise_buffer.commit(carried.len());
        }
        bytes
    }

    fn expect(&mut self, size: usize) {
        self.missing_noise_b = size.saturating_sub(IsBuffer::len(&self.noise_buffer));
    }

    /// Attempts to decode the next encrypted frame with the decrypting half of a completed
    /// handshake.
    ///
    /// On [`Decrypted::Incomplete`], read another chunk from the stream into [`Self::read_buf`],
    /// report it with [`Self::advance`], and call this method again with the state it carries
    /// until it returns a [`Decrypted::Frame`]. The count it carries is what the decoder will
    /// accept on the next read, which is always [`Self::read_len`] and never more than one
    /// chunk, not the number of bytes left in the frame.
    ///
    /// Bytes buffered past the end of the frame are kept as the start of the next one.
    ///
    /// An `Err` consumes the state. After a failed decryption the cipher's nonce no longer
    /// follows the bytes the peer keeps sending: a retransmission of the ciphertext that just
    /// failed still authenticates under the nonce the cipher stayed at, and the decoder, back to
    /// waiting for a header, would read it as one. The connection has to be torn down and a new
    /// [`crate::Handshake`] run, which the types enforce by never handing the same state back:
    ///
    /// ```compile_fail,E0382
    /// use codec_sv2::{NoiseDecoder, TransportDecryptState};
    ///
    /// fn state() -> TransportDecryptState { unimplemented!() }
    ///
    /// let mut decoder = NoiseDecoder::new();
    /// let state = state();
    /// if decoder.next_transport_frame(state).is_err() {
    ///     let _retry = decoder.next_transport_frame(state);
    /// }
    /// ```
    #[inline]
    pub fn next_transport_frame(
        &mut self,
        mut state: TransportDecryptState,
    ) -> Result<Decrypted<SerializedFrame<B::Slice>>> {
        match self.next_transport(|buf| state.decrypt(buf))? {
            Decoded::Frame(frame) => Ok(Decrypted::Frame(frame, state)),
            Decoded::Incomplete(n) => Ok(Decrypted::Incomplete(n, state)),
        }
    }

    // Decodes a transport-mode frame, decrypting through `decrypt`.
    #[inline]
    fn next_transport(
        &mut self,
        decrypt: impl FnMut(&mut B) -> Result<()>,
    ) -> Result<Decoded<SerializedFrame<B::Slice>>> {
        self.window = 0;
        let expected = if IsBuffer::len(&self.sv2_buffer) < SV2_FRAME_HEADER_SIZE {
            ENCRYPTED_SV2_FRAME_HEADER_SIZE
        } else {
            crate::encrypted_payload_length(&Header::from_bytes(self.sv2_buffer.frame())?)
        };
        if let Some(missing) = self.missing(expected) {
            return Ok(Decoded::Incomplete(missing));
        }
        self.decode_noise_frame(expected, decrypt)
    }

    /// Returns the number of bytes to read next for the current Noise-encrypted frame.
    ///
    /// This is how many bytes are still missing from the frame, capped at one chunk
    /// ([`framing_sv2::SV2_FRAME_CHUNK_SIZE`]), which is the unit the payload is encrypted in. A
    /// peer declares the payload length in a header it sends before any of that payload, so
    /// buffering the whole declared length up front would let it reserve megabytes with a
    /// 22-byte write; the frame is instead read a chunk at a time, and the buffer grows with the
    /// data that actually arrives.
    ///
    /// The returned length dynamically updates as data is received and processed.
    pub fn read_len(&self) -> usize {
        self.missing_noise_b.min(SV2_FRAME_CHUNK_SIZE)
    }

    /// Returns the window to read incoming Noise-encrypted Sv2 data into, [`Self::read_len`]
    /// bytes long.
    ///
    /// None of it counts as received until [`Self::advance`] reports how many bytes the read
    /// actually filled. Calling this again first returns a new window, and bytes left in the old
    /// one are not kept.
    #[inline]
    pub fn read_buf(&mut self) -> &mut [u8] {
        self.window = self.read_len();
        self.noise_buffer.reserve(self.window)
    }

    /// Counts the first `n` bytes of the window from [`Self::read_buf`] as received.
    ///
    /// Errors with [`Error::ReadBeyondWindow`] if `n` is larger than that window. The window also
    /// ends once a `next_` call runs.
    #[inline]
    pub fn advance(&mut self, n: usize) -> Result<()> {
        if n > self.window {
            return Err(Error::ReadBeyondWindow {
                read: n,
                window: self.window,
            });
        }
        self.noise_buffer.commit(n);
        self.window = 0;
        self.missing_noise_b = self.missing_noise_b.saturating_sub(n);
        Ok(())
    }

    /// Determines whether the decoder's internal buffers can be safely dropped.
    ///
    /// For more information, refer to the [`buffer_sv2`
    /// crate](https://docs.rs/buffer_sv2/latest/buffer_sv2/).
    pub fn droppable(&self) -> bool {
        self.noise_buffer.is_droppable() && self.sv2_buffer.is_droppable()
    }

    // Decodes a Noise-encrypted Sv2 frame, handling both the message header and payload
    // decryption.
    //
    // Processes Noise-encrypted Sv2 frames by first decrypting the header, followed by the
    // payload. If the frame's data is received in chunks, it ensures that decryption occurs
    // incrementally as more encrypted data becomes available. The decrypted data is then stored in
    // the `sv2_buffer`, from which the resulting Sv2 frame is extracted and returned.
    //
    // On success, the decoded frame is returned. Otherwise, an error indicating the number of
    // missing bytes required to complete the encoded frame, an error on a badly formatted message
    // header, or a decryption failure error is returned. If there are still bytes missing to
    // complete the frame, the function will return `Decoded::Incomplete` with the number of
    // additional bytes required to fully decrypt the frame. Once all bytes are available, the
    // decryption process completes and the frame can be successfully decoded.
    #[inline]
    fn decode_noise_frame(
        &mut self,
        expected: usize,
        decrypt: impl FnMut(&mut B) -> Result<()>,
    ) -> Result<Decoded<SerializedFrame<B::Slice>>> {
        let result = self.try_decode_noise_frame(expected, decrypt);

        if result.is_err() {
            self.sv2_buffer.danger_set_start(0);
            self.sv2_buffer.get_data_owned();
            self.noise_buffer.get_data_owned();
            self.expect(ENCRYPTED_SV2_FRAME_HEADER_SIZE);
        }

        result
    }

    #[inline]
    fn try_decode_noise_frame(
        &mut self,
        expected: usize,
        mut decrypt: impl FnMut(&mut B) -> Result<()>,
    ) -> Result<Decoded<SerializedFrame<B::Slice>>> {
        if IsBuffer::len(&self.sv2_buffer) < SV2_FRAME_HEADER_SIZE {
            // HERE THE SV2 HEADER IS READY TO BE DECRYPTED
            let src = self.take(expected);
            self.sv2_buffer
                .reserve(expected)
                .copy_from_slice(&src.as_ref()[..expected]);
            self.sv2_buffer.commit(expected);
            decrypt(&mut self.sv2_buffer)?;
            let header = Header::from_bytes(self.sv2_buffer.frame())?;
            let payload = crate::encrypted_payload_length(&header);
            if payload > 0 {
                self.expect(payload);
                return Ok(Decoded::Incomplete(self.read_len()));
            }
            // A frame that declares no payload is already whole, so return it in this same round
            // rather than handing the caller a zero-length read window to come back through.
            self.decrypt_payload(0, decrypt)
        } else {
            self.decrypt_payload(expected, decrypt)
        }
    }

    // Decrypts `expected` bytes of payload, chunk by chunk, onto the header already decrypted in
    // `sv2_buffer`, and returns the frame the two make up.
    #[inline]
    fn decrypt_payload(
        &mut self,
        expected: usize,
        mut decrypt: impl FnMut(&mut B) -> Result<()>,
    ) -> Result<Decoded<SerializedFrame<B::Slice>>> {
        let encrypted_payload = self.take(expected);
        self.expect(ENCRYPTED_SV2_FRAME_HEADER_SIZE);
        let encrypted_payload = &encrypted_payload.as_ref()[..expected];
        let mut start = 0;
        // Do not try to decrypt the header cause it is already decrypted
        let mut decrypted_len = SV2_FRAME_HEADER_SIZE;
        while start < expected {
            let end = (start + SV2_FRAME_CHUNK_SIZE).min(expected);
            self.sv2_buffer
                .reserve(end - start)
                .copy_from_slice(&encrypted_payload[start..end]);
            self.sv2_buffer.commit(end - start);
            self.sv2_buffer.danger_set_start(decrypted_len);
            decrypt(&mut self.sv2_buffer)?;
            start = end;
            decrypted_len += self.sv2_buffer.as_ref().len();
        }
        self.sv2_buffer.danger_set_start(0);
        let src = self.sv2_buffer.get_data_owned();
        Ok(Decoded::Frame(SerializedFrame::<B::Slice>::from_bytes(
            src,
        )?))
    }
}

#[cfg(feature = "noise_sv2")]
impl WithNoise<Buffer> {
    /// Crates a new [`WithNoise`] decoder with default buffer sizes.
    ///
    /// It starts waiting for an encrypted Sv2 header, so [`Self::read_buf`] is that wide before
    /// the first `next_` call. A handshake message is longer; the first `next_handshake_frame`
    /// moves the decoder into the handshake phase and asks for the rest.
    pub fn new() -> Self {
        Self {
            noise_buffer: Buffer::new(crate::DEFAULT_POOL_BUFFER_SIZE),
            sv2_buffer: Buffer::new(crate::DEFAULT_POOL_BUFFER_SIZE),
            missing_noise_b: ENCRYPTED_SV2_FRAME_HEADER_SIZE,
            window: 0,
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl Default for WithNoise<Buffer> {
    fn default() -> Self {
        Self::new()
    }
}

/// Decoder for standard Sv2 frames.
///
/// Accumulates the data into a dedicated buffer until the entire Sv2 frame is received. This data
/// is then deserialized into the original Sv2 frame and message format.
#[derive(Debug)]
pub struct WithoutNoise<B: IsBuffer> {
    // Buffer for holding incoming data to be decoded into a Sv2 frame.
    //
    // This buffer stores incoming data as it is received, allowing the decoder to accumulate the
    // necessary bytes until a full frame is available. Once the full encoded frame has been
    // received, the buffer's contents are processed and decoded into an Sv2 frame.
    buffer: B,

    // Tracks the number of bytes remaining until the full frame is received.
    //
    // Ensures that the full Sv2 frame has been received by keeping track of the remaining bytes.
    // Once the complete frame is received, decoding can proceed.
    missing_b: usize,

    // Size of the window the last `read_buf` returned, `0` once it is advanced or `next_frame`
    // ends it.
    window: usize,
}

impl<B: IsBuffer> WithoutNoise<B> {
    /// Attempts to decode the next frame.
    ///
    /// [`Decoded::Incomplete`] carries the number of bytes the decoder will accept on the next
    /// read: read up to that many bytes from the stream into [`Self::read_buf`], report them with
    /// [`Self::advance`], and call `next_frame` again until it returns a [`Decoded::Frame`]. The
    /// count always equals [`Self::read_len`], so it is capped at one chunk and is not the number
    /// of bytes left in the frame — a frame longer than that takes several rounds.
    ///
    /// Bytes buffered past the end of the frame are kept as the start of the next one.
    #[inline]
    pub fn next_frame(&mut self) -> Result<Decoded<SerializedFrame<B::Slice>>> {
        self.window = 0;
        let len = self.buffer.len();
        let src = self.buffer.frame();

        match SerializedFrame::<B::Slice>::parse_header(src) {
            Ok(header) => {
                self.missing_b = Header::SIZE;
                let src = self.buffer.get_data_owned();
                Ok(Decoded::Frame(SerializedFrame::<B::Slice>::from_parts(
                    header, src,
                )))
            }
            Err(SizeHint::Missing(missing)) => {
                self.missing_b = missing;
                Ok(Decoded::Incomplete(self.read_len()))
            }
            Err(SizeHint::Surplus(surplus)) => {
                let bytes = self.buffer.get_data_owned();
                let (frame, carried) = bytes.as_ref().split_at(len - surplus);
                self.buffer.reserve(frame.len()).copy_from_slice(frame);
                self.buffer.commit(frame.len());
                let frame = self.buffer.get_data_owned();
                self.buffer.reserve(carried.len()).copy_from_slice(carried);
                self.buffer.commit(carried.len());
                self.missing_b = Header::SIZE.saturating_sub(carried.len());
                Ok(Decoded::Frame(SerializedFrame::<B::Slice>::from_bytes(
                    frame,
                )?))
            }
        }
    }

    /// Returns the number of bytes to read next for the current frame.
    ///
    /// This is how many bytes are still missing from the frame, capped at
    /// [`framing_sv2::SV2_FRAME_CHUNK_SIZE`]. A peer declares the payload length in the header it
    /// sends before any of that payload, so buffering the whole declared length up front would
    /// let it reserve close to 16 MiB with a six-byte write; the frame is instead read a chunk at
    /// a time, and the buffer grows with the data that actually arrives.
    pub fn read_len(&self) -> usize {
        self.missing_b.min(SV2_FRAME_CHUNK_SIZE)
    }

    /// Returns the window to read incoming Sv2 data into, [`Self::read_len`] bytes long.
    ///
    /// None of it counts as received until [`Self::advance`] reports how many bytes the read
    /// actually filled. Calling this again first returns a new window, and bytes left in the old
    /// one are not kept.
    pub fn read_buf(&mut self) -> &mut [u8] {
        self.window = self.read_len();
        self.buffer.reserve(self.window)
    }

    /// Counts the first `n` bytes of the window from [`Self::read_buf`] as received.
    ///
    /// Errors with [`Error::ReadBeyondWindow`] if `n` is larger than that window. The window also
    /// ends once [`Self::next_frame`] runs.
    pub fn advance(&mut self, n: usize) -> Result<()> {
        if n > self.window {
            return Err(Error::ReadBeyondWindow {
                read: n,
                window: self.window,
            });
        }
        self.buffer.commit(n);
        self.window = 0;
        self.missing_b = self.missing_b.saturating_sub(n);
        Ok(())
    }
}

impl WithoutNoise<Buffer> {
    /// Creates a new [`WithoutNoise`] with a buffer of default size.
    ///
    /// Initializes the decoder with a default buffer size and sets the number of missing bytes to
    /// the size of the header.
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(crate::DEFAULT_POOL_BUFFER_SIZE),
            missing_b: Header::SIZE,
            window: 0,
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
    use binary_sv2::{Deserialize, Serialize};
    // Not redundant: the glob import above brings in `crate::Result`, whose single type parameter
    // the code generated by the `Deserialize` derive cannot use.
    use core::result::Result;

    #[derive(Serialize, Deserialize)]
    pub struct TestMessage {}

    #[test]
    fn unencrypted_read_buf_with_missing_b_initialized_as_header_size() {
        let mut decoder = Decoder::new();
        let actual = decoder.read_buf();
        let expect = [0u8; Header::SIZE];
        assert_eq!(actual, expect);
    }

    #[cfg(feature = "noise_sv2")]
    #[test]
    fn noise_handshake_frame_waits_for_the_size_the_role_expects() {
        use crate::{ExpectsHandshakeMessage, InitiatorSent};
        use noise_sv2::Responder;

        let mut decoder = NoiseDecoder::new();
        assert!(matches!(
            decoder.next_handshake_frame::<Responder>(),
            Ok(Decoded::Incomplete(n)) if n == Responder::EXPECTED_MESSAGE_SIZE
        ));
        assert_eq!(decoder.read_len(), Responder::EXPECTED_MESSAGE_SIZE);

        let mut decoder = NoiseDecoder::new();
        assert!(matches!(
            decoder.next_handshake_frame::<InitiatorSent>(),
            Ok(Decoded::Incomplete(n)) if n == InitiatorSent::EXPECTED_MESSAGE_SIZE
        ));
    }
}

#[cfg(test)]
mod prop_tests {
    use crate::{decoder::Buffer, encoder::Encoder, Decoded, Decoder};
    #[cfg(feature = "noise_sv2")]
    use crate::{Decrypted, NoiseDecoder, NoiseEncoder};
    use binary_sv2::{Deserialize, Serialize};
    use buffer_sv2::Buffer as IsBuffer;
    use framing_sv2::{
        framing::{MessageFrame, SerializedFrame},
        header::Header,
        SV2_FRAME_CHUNK_SIZE,
    };
    #[cfg(feature = "noise_sv2")]
    use noise_sv2::Responder;
    #[cfg(feature = "noise_sv2")]
    use noise_sv2::ELLSWIFT_ENCODING_SIZE;
    use quickcheck::{Arbitrary, Gen, TestResult};
    use quickcheck_macros::quickcheck;
    #[cfg(feature = "noise_sv2")]
    use std::convert::TryInto;

    #[cfg(feature = "noise_sv2")]
    use crate::test_utils::{decode_noise_frame, make_handshake_pair, make_transport_state_pair};

    type Slice = <Buffer as IsBuffer>::Slice;

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
    struct TestMessage {
        value: u16,
    }

    #[cfg(feature = "noise_sv2")]
    #[derive(Serialize)]
    struct EmptyMessage {}

    impl Arbitrary for TestMessage {
        fn arbitrary(g: &mut Gen) -> Self {
            TestMessage {
                value: u16::arbitrary(g),
            }
        }
    }

    fn decode_frame(
        decoder: &mut Decoder,
        encoded_bytes: &[u8],
        chunk_size: Option<usize>,
    ) -> Option<SerializedFrame<Slice>> {
        let mut offset = 0;
        while offset < encoded_bytes.len() {
            let writable = decoder.read_buf();
            let available = encoded_bytes.len() - offset;
            let to_copy = match chunk_size {
                Some(c) => core::cmp::min(core::cmp::min(writable.len(), c), available),
                None => core::cmp::min(writable.len(), available),
            };
            writable[..to_copy].copy_from_slice(&encoded_bytes[offset..offset + to_copy]);
            decoder.advance(to_copy).unwrap();
            offset += to_copy;

            match decoder.next_frame() {
                Ok(Decoded::Frame(frame)) => return Some(frame),
                Ok(Decoded::Incomplete(_)) => continue,
                Err(_) => return None,
            }
        }
        None
    }

    /// Verifies that encoding then decoding a frame over the standard (unencrypted) codec
    /// recovers the original message, msg_type, and ext_type exactly.
    #[quickcheck]
    fn prop_encode_decode_roundtrip(msg: TestMessage, msg_type: u8, ext_type: u16) -> TestResult {
        let original_msg = msg.clone();

        let frame = match MessageFrame::<TestMessage>::from_message(msg, msg_type, ext_type, false)
        {
            Ok(f) => f,
            Err(_) => return TestResult::discard(),
        };

        let expected_ext_type = frame.header().ext_type();

        let mut encoder = Encoder::new();
        let encoded = match encoder.encode(frame) {
            Ok(e) => e,
            Err(_) => return TestResult::failed(),
        };

        let mut decoder = Decoder::new();
        match decode_frame(&mut decoder, encoded.as_ref(), None) {
            Some(mut decoded_frame) => {
                let header = decoded_frame.header();
                let actual_msg_type = header.msg_type();
                let actual_ext_type = header.ext_type();
                let decoded_msg: TestMessage = match binary_sv2::from_bytes(decoded_frame.payload())
                {
                    Ok(m) => m,
                    Err(_) => return TestResult::failed(),
                };
                TestResult::from_bool(
                    decoded_msg == original_msg
                        && actual_msg_type == msg_type
                        && actual_ext_type == expected_ext_type,
                )
            }
            None => TestResult::failed(),
        }
    }

    /// Verifies that the decoder correctly accumulates partial input, emitting `Incomplete`
    /// on each incomplete delivery before returning the frame once all bytes arrive.
    #[quickcheck]
    fn prop_decoder_handles_partial_data(
        msg: TestMessage,
        msg_type: u8,
        chunk_size: u8,
    ) -> TestResult {
        if chunk_size == 0 {
            return TestResult::discard();
        }

        let frame = match MessageFrame::<TestMessage>::from_message(msg, msg_type, 0, false) {
            Ok(f) => f,
            Err(_) => return TestResult::discard(),
        };

        let mut encoder = Encoder::new();
        let encoded = match encoder.encode(frame) {
            Ok(e) => e,
            Err(_) => return TestResult::failed(),
        };

        let mut decoder = Decoder::new();
        let encoded_bytes: &[u8] = encoded.as_ref();
        let chunk_size = (chunk_size as usize).max(1);

        let mut offset = 0;
        let mut missing_bytes_count = 0;
        while offset < encoded_bytes.len() {
            let writable = decoder.read_buf();
            let to_copy = core::cmp::min(
                core::cmp::min(writable.len(), chunk_size),
                encoded_bytes.len() - offset,
            );
            writable[..to_copy].copy_from_slice(&encoded_bytes[offset..offset + to_copy]);
            decoder.advance(to_copy).unwrap();
            offset += to_copy;

            match decoder.next_frame() {
                Ok(Decoded::Frame(_)) => return TestResult::passed(),
                Ok(Decoded::Incomplete(n)) => {
                    missing_bytes_count += 1;
                    assert!(n > 0);
                }
                Err(_) => return TestResult::failed(),
            }
        }

        TestResult::from_bool(missing_bytes_count > 0)
    }

    #[test]
    fn bytes_read_past_a_plain_frame_open_the_next_one() {
        const SURPLUS: usize = 4;

        let mut encoder = Encoder::new();
        let mut encode = |value: u16| -> alloc::vec::Vec<u8> {
            let frame =
                MessageFrame::<TestMessage>::from_message(TestMessage { value }, 0, 0, false)
                    .unwrap();
            let encoded = encoder.encode(frame).unwrap();
            let encoded: &[u8] = encoded.as_ref();
            encoded.to_vec()
        };
        let first = encode(1);
        let second = encode(2);

        let mut decoder = Decoder::new();
        decoder.read_buf().copy_from_slice(&first[..Header::SIZE]);
        decoder.advance(Header::SIZE).unwrap();
        assert!(matches!(decoder.next_frame(), Ok(Decoded::Incomplete(_))));
        decoder.read_buf().copy_from_slice(&first[Header::SIZE..]);
        decoder.advance(first.len() - Header::SIZE).unwrap();
        decoder
            .buffer
            .reserve(SURPLUS)
            .copy_from_slice(&second[..SURPLUS]);
        decoder.buffer.commit(SURPLUS);

        let Ok(Decoded::Frame(mut frame)) = decoder.next_frame() else {
            panic!("expected the first frame");
        };
        assert_eq!(
            binary_sv2::from_bytes::<TestMessage>(frame.payload()).unwrap(),
            TestMessage { value: 1 }
        );
        assert_eq!(decoder.read_len(), Header::SIZE - SURPLUS);

        let mut frame = decode_frame(&mut decoder, &second[SURPLUS..], None).unwrap();
        assert_eq!(
            binary_sv2::from_bytes::<TestMessage>(frame.payload()).unwrap(),
            TestMessage { value: 2 }
        );
    }

    #[cfg(feature = "noise_sv2")]
    #[test]
    fn bytes_read_past_the_handshake_open_the_first_header() {
        use crate::ENCRYPTED_SV2_FRAME_HEADER_SIZE;

        const SURPLUS: usize = 4;

        let mut decoder = NoiseDecoder::new();
        assert!(matches!(
            decoder.next_handshake_frame::<Responder>(),
            Ok(Decoded::Incomplete(ELLSWIFT_ENCODING_SIZE))
        ));
        decoder.read_buf().fill(0);
        decoder.advance(decoder.read_len()).unwrap();
        decoder
            .noise_buffer
            .reserve(SURPLUS)
            .copy_from_slice(&[0xff; SURPLUS]);
        decoder.noise_buffer.commit(SURPLUS);

        let Ok(Decoded::Frame(frame)) = decoder.next_handshake_frame::<Responder>() else {
            panic!("expected the handshake message");
        };
        assert_eq!(frame.payload().len(), ELLSWIFT_ENCODING_SIZE);
        assert_eq!(
            decoder.read_len(),
            ENCRYPTED_SV2_FRAME_HEADER_SIZE - SURPLUS
        );
        assert_eq!(decoder.noise_buffer.as_ref(), &[0xff; SURPLUS]);
    }

    #[cfg(feature = "noise_sv2")]
    #[test]
    fn bytes_read_past_a_frame_open_the_next_one() {
        use crate::ENCRYPTED_SV2_FRAME_HEADER_SIZE;

        const SURPLUS: usize = 3;

        let (mut sender, receiver) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        let mut encrypt = |value: u16| -> alloc::vec::Vec<u8> {
            let frame =
                MessageFrame::<TestMessage>::from_message(TestMessage { value }, 0, 0, false)
                    .unwrap();
            let encrypted = encoder.encode_transport(frame, &mut sender).unwrap();
            let encrypted: &[u8] = encrypted.as_ref();
            encrypted.to_vec()
        };
        let first = encrypt(1);
        let second = encrypt(2);
        let (first_header, first_payload) = first.split_at(ENCRYPTED_SV2_FRAME_HEADER_SIZE);

        let mut decoder = NoiseDecoder::new();
        decoder.read_buf().copy_from_slice(first_header);
        decoder.advance(first_header.len()).unwrap();
        decoder
            .noise_buffer
            .reserve(SURPLUS)
            .copy_from_slice(&first_payload[..SURPLUS]);
        decoder.noise_buffer.commit(SURPLUS);
        let Ok(Decrypted::Incomplete(_, receiver)) = decoder.next_transport_frame(receiver) else {
            panic!("expected the decoder to want the payload");
        };
        assert_eq!(decoder.read_len(), first_payload.len() - SURPLUS);

        decoder
            .read_buf()
            .copy_from_slice(&first_payload[SURPLUS..]);
        decoder.advance(first_payload.len() - SURPLUS).unwrap();
        decoder
            .noise_buffer
            .reserve(second.len())
            .copy_from_slice(&second);
        decoder.noise_buffer.commit(second.len());
        let Ok(Decrypted::Frame(mut frame, receiver)) = decoder.next_transport_frame(receiver)
        else {
            panic!("expected the first frame");
        };
        assert_eq!(
            binary_sv2::from_bytes::<TestMessage>(frame.payload()).unwrap(),
            TestMessage { value: 1 }
        );
        assert_eq!(decoder.read_len(), 0);

        let Ok(Decrypted::Incomplete(0, receiver)) = decoder.next_transport_frame(receiver) else {
            panic!("expected an empty read window");
        };
        let Ok(Decrypted::Frame(mut frame, _)) = decoder.next_transport_frame(receiver) else {
            panic!("expected the second frame");
        };
        assert_eq!(
            binary_sv2::from_bytes::<TestMessage>(frame.payload()).unwrap(),
            TestMessage { value: 2 }
        );
        assert_eq!(decoder.read_len(), ENCRYPTED_SV2_FRAME_HEADER_SIZE);
    }
    /// A frame that declares no payload is whole as soon as its header is decrypted, so it comes
    /// back in that same round rather than through a zero-length read window a caller reading
    /// into `read_buf` would take for EOF.
    #[cfg(feature = "noise_sv2")]
    #[test]
    fn a_frame_with_no_payload_is_returned_without_a_further_read() {
        let (mut sender, receiver) = make_transport_state_pair();
        let frame =
            MessageFrame::<EmptyMessage>::from_message(EmptyMessage {}, 0xaa, 0, false).unwrap();
        assert_eq!(frame.header().payload_length(), 0);

        let mut encoder = crate::NoiseEncoder::new();
        let encoded = encoder.encode_transport(frame, &mut sender).unwrap();
        let encoded: Vec<u8> = AsRef::<[u8]>::as_ref(&encoded).to_vec();
        assert_eq!(encoded.len(), crate::ENCRYPTED_SV2_FRAME_HEADER_SIZE);

        let mut decoder = NoiseDecoder::new();
        decoder.read_buf().copy_from_slice(&encoded);
        decoder.advance(encoded.len()).unwrap();
        let Ok(Decrypted::Frame(frame, _)) = decoder.next_transport_frame(receiver) else {
            panic!("expected the frame in the same round as its header");
        };
        assert_eq!(frame.header().payload_length(), 0);
        assert_eq!(frame.as_bytes().len(), Header::SIZE);
        assert_eq!(decoder.read_len(), crate::ENCRYPTED_SV2_FRAME_HEADER_SIZE);
    }

    /// A caller that sizes its read from `read_buf` before calling `next_` — the shape both
    /// examples use — must never be handed a zero-length window, and the handshake must complete
    /// straight into the encrypted Sv2 header rather than into a short read of its own.
    #[cfg(feature = "noise_sv2")]
    #[test]
    fn the_read_window_is_never_empty_across_the_handshake() {
        use crate::ENCRYPTED_SV2_FRAME_HEADER_SIZE;

        let (initiator, _responder) = make_handshake_pair();

        let mut decoder = NoiseDecoder::new();
        assert_ne!(decoder.read_len(), 0, "a fresh decoder");

        let mut first = initiator.step_0().unwrap().0.payload().to_vec();
        let mut offset = 0;
        loop {
            let w = decoder.read_buf();
            assert_ne!(w.len(), 0, "while reading the handshake message");
            let n = w.len().min(first.len() - offset);
            w[..n].copy_from_slice(&first[offset..offset + n]);
            decoder.advance(n).unwrap();
            offset += n;
            match decoder.next_handshake_frame::<Responder>() {
                Ok(Decoded::Frame(_)) => break,
                Ok(Decoded::Incomplete(_)) => continue,
                Err(e) => panic!("failed to read the handshake message: {e:?}"),
            }
        }
        first.clear();

        assert_eq!(decoder.read_len(), ENCRYPTED_SV2_FRAME_HEADER_SIZE);
    }

    /// The first transport frame after a handshake takes one read for its header, not a short
    /// read left over from the removed two-byte noise framing followed by the rest.
    #[cfg(feature = "noise_sv2")]
    #[test]
    fn the_first_transport_frame_reads_its_header_in_one_round() {
        use crate::ENCRYPTED_SV2_FRAME_HEADER_SIZE;

        let mut decoder = NoiseDecoder::new();

        let initiator_first = {
            let (initiator, _) = make_handshake_pair();
            initiator.step_0().unwrap().0.payload().to_vec()
        };
        let mut offset = 0;
        loop {
            let w = decoder.read_buf();
            let n = w.len().min(initiator_first.len() - offset);
            w[..n].copy_from_slice(&initiator_first[offset..offset + n]);
            decoder.advance(n).unwrap();
            offset += n;
            match decoder.next_handshake_frame::<Responder>() {
                Ok(Decoded::Frame(_)) => break,
                Ok(Decoded::Incomplete(_)) => continue,
                Err(e) => panic!("{e:?}"),
            }
        }

        let (mut sender, mut receiver) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        let frame =
            MessageFrame::<TestMessage>::from_message(TestMessage { value: 42 }, 0, 0, false)
                .unwrap();
        let encrypted = encoder.encode_transport(frame, &mut sender).unwrap();
        let encrypted: &[u8] = encrypted.as_ref();

        let mut sizes = alloc::vec::Vec::new();
        let mut offset = 0;
        loop {
            let w = decoder.read_buf();
            sizes.push(w.len());
            let n = w.len().min(encrypted.len() - offset);
            w[..n].copy_from_slice(&encrypted[offset..offset + n]);
            decoder.advance(n).unwrap();
            offset += n;
            match decoder.next_transport_frame(receiver) {
                Ok(Decrypted::Frame(..)) => break,
                Ok(Decrypted::Incomplete(_, state)) => receiver = state,
                Err(e) => panic!("failed to decode the first transport frame: {e:?}"),
            }
        }

        assert_eq!(sizes[0], ENCRYPTED_SV2_FRAME_HEADER_SIZE);
        assert_eq!(sizes.len(), 2, "header then payload, got {sizes:?}");
    }

    /// A chunk past the first failing must leave the decrypt offset where the next frame can
    /// use it: `get_data_owned` clears the cursor but not the offset `danger_set_start` moved.
    #[cfg(feature = "noise_sv2")]
    #[test]
    fn a_failed_later_chunk_does_not_strand_the_decrypt_offset() {
        use crate::ENCRYPTED_SV2_FRAME_HEADER_SIZE;

        const PAYLOAD: usize = 2 * SV2_FRAME_CHUNK_SIZE;

        let mut plain = vec![0u8; Header::SIZE + PAYLOAD];
        plain[2] = 1;
        plain[3..Header::SIZE].copy_from_slice(&(PAYLOAD as u32).to_le_bytes()[..3]);

        let (mut sender, mut receiver) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        let frame = SerializedFrame::<Vec<u8>>::from_bytes(plain).unwrap();
        let encrypted = encoder.encode_transport(frame, &mut sender).unwrap();
        let encrypted_bytes: &[u8] = encrypted.as_ref();
        let mut encrypted = encrypted_bytes.to_vec();

        let second_chunk = ENCRYPTED_SV2_FRAME_HEADER_SIZE + SV2_FRAME_CHUNK_SIZE + 8;
        encrypted[second_chunk] ^= 0x01;

        let mut decoder = NoiseDecoder::new();
        let mut offset = 0;
        loop {
            match decoder.next_transport_frame(receiver) {
                Ok(Decrypted::Frame(..)) => panic!("the tampered frame should not decode"),
                Ok(Decrypted::Incomplete(_, state)) => {
                    receiver = state;
                    let w = decoder.read_buf();
                    let n = w.len().min(encrypted.len() - offset);
                    w[..n].copy_from_slice(&encrypted[offset..offset + n]);
                    decoder.advance(n).unwrap();
                    offset += n;
                }
                Err(_) => break,
            }
        }

        let (_, receiver) = make_transport_state_pair();
        let Ok(Decrypted::Incomplete(_, receiver)) = decoder.next_transport_frame(receiver) else {
            panic!("expected the decoder to want a header");
        };
        decoder.read_buf().fill(0);
        decoder.advance(decoder.read_len()).unwrap();
        let _ = decoder.next_transport_frame(receiver);
    }

    /// A peer declares the payload length in the header it sends first, so the decoder must not
    /// size its read window from that declaration: six bytes would otherwise reserve 16 MiB.
    #[test]
    fn a_declared_frame_length_does_not_widen_the_read_window() {
        let mut decoder = Decoder::new();
        decoder
            .read_buf()
            .copy_from_slice(&[0, 0, 0, 0xff, 0xff, 0xff]);
        decoder.advance(Header::SIZE).unwrap();

        assert!(matches!(
            decoder.next_frame(),
            Ok(Decoded::Incomplete(SV2_FRAME_CHUNK_SIZE))
        ));
        assert_eq!(decoder.read_len(), SV2_FRAME_CHUNK_SIZE);
        assert_eq!(decoder.read_buf().len(), SV2_FRAME_CHUNK_SIZE);
    }

    /// A caller that sizes its read from `Incomplete` and one that sizes it from `read_buf`
    /// must agree, on every round of a frame that takes more than one.
    #[test]
    fn incomplete_always_reports_the_next_read_window() {
        const PAYLOAD: usize = 2 * SV2_FRAME_CHUNK_SIZE + 7;

        let mut encoded = vec![0u8; Header::SIZE + PAYLOAD];
        encoded[2] = 1;
        encoded[3..Header::SIZE].copy_from_slice(&(PAYLOAD as u32).to_le_bytes()[..3]);

        let mut decoder = Decoder::new();
        let mut offset = 0;
        loop {
            let missing = match decoder.next_frame() {
                Ok(Decoded::Frame(_)) => break,
                Ok(Decoded::Incomplete(n)) => n,
                Err(e) => panic!("failed to decode a multi-chunk frame: {e:?}"),
            };
            let writable = decoder.read_buf();
            assert_eq!(missing, writable.len());
            writable[..missing].copy_from_slice(&encoded[offset..offset + missing]);
            decoder.advance(missing).unwrap();
            offset += missing;
        }
        assert_eq!(offset, encoded.len());
    }

    /// The counterpart of the above: a frame longer than the read window is still decoded, over
    /// as many reads as it takes.
    #[test]
    fn a_frame_longer_than_the_read_window_is_read_over_several_rounds() {
        const PAYLOAD: usize = 3 * SV2_FRAME_CHUNK_SIZE + 7;

        let mut encoded = vec![0u8; Header::SIZE + PAYLOAD];
        encoded[2] = 1;
        encoded[3..Header::SIZE].copy_from_slice(&(PAYLOAD as u32).to_le_bytes()[..3]);
        for (i, byte) in encoded[Header::SIZE..].iter_mut().enumerate() {
            *byte = i as u8;
        }

        let mut decoder = Decoder::new();
        let mut offset = 0;
        let mut rounds = 0;
        let frame = loop {
            let writable = decoder.read_buf();
            let n = writable.len().min(encoded.len() - offset);
            writable[..n].copy_from_slice(&encoded[offset..offset + n]);
            decoder.advance(n).unwrap();
            offset += n;
            rounds += 1;

            match decoder.next_frame() {
                Ok(Decoded::Frame(frame)) => break frame,
                Ok(Decoded::Incomplete(_)) => continue,
                Err(e) => panic!("failed to decode a multi-chunk frame: {e:?}"),
            }
        };

        assert!(rounds > 2, "the frame should not fit in a single read");
        assert_eq!(frame.header().payload_length(), PAYLOAD);
        assert_eq!(frame.as_bytes(), &encoded[..]);
    }

    /// The Noise decoder learns the payload length from a header it has authenticated, but the
    /// peer still has not sent any of that payload, so the same cap applies.
    #[cfg(feature = "noise_sv2")]
    #[test]
    fn a_declared_noise_payload_length_does_not_widen_the_read_window() {
        use crate::ENCRYPTED_SV2_FRAME_HEADER_SIZE;
        use framing_sv2::SV2_FRAME_HEADER_SIZE;

        let (mut sender, receiver) = make_transport_state_pair();

        let mut header = Buffer::new(crate::DEFAULT_POOL_BUFFER_SIZE);
        header
            .reserve(SV2_FRAME_HEADER_SIZE)
            .copy_from_slice(&[0, 0, 0, 0xff, 0xff, 0xff]);
        header.commit(SV2_FRAME_HEADER_SIZE);
        sender.encrypt(&mut header).unwrap();
        let header = header.get_data_owned();

        let mut decoder = NoiseDecoder::new();
        let Ok(Decrypted::Incomplete(ENCRYPTED_SV2_FRAME_HEADER_SIZE, receiver)) =
            decoder.next_transport_frame(receiver)
        else {
            panic!("expected the decoder to want a header");
        };
        let header: &[u8] = header.as_ref();
        decoder.read_buf().copy_from_slice(header);
        decoder.advance(header.len()).unwrap();

        assert!(matches!(
            decoder.next_transport_frame(receiver),
            Ok(Decrypted::Incomplete(..))
        ));
        assert_eq!(decoder.read_len(), SV2_FRAME_CHUNK_SIZE);
        assert_eq!(decoder.read_buf().len(), SV2_FRAME_CHUNK_SIZE);
    }

    /// A Noise frame whose payload spans several chunks survives the round trip, and every round
    /// of it agrees on how many bytes the decoder wants next.
    #[cfg(feature = "noise_sv2")]
    #[test]
    fn a_noise_frame_longer_than_the_read_window_is_read_over_several_rounds() {
        const PAYLOAD: usize = 2 * SV2_FRAME_CHUNK_SIZE + 7;

        let mut plain = vec![0u8; Header::SIZE + PAYLOAD];
        plain[2] = 1;
        plain[3..Header::SIZE].copy_from_slice(&(PAYLOAD as u32).to_le_bytes()[..3]);
        for (i, byte) in plain[Header::SIZE..].iter_mut().enumerate() {
            *byte = i as u8;
        }

        let (mut sender, mut receiver) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        let frame = SerializedFrame::<Vec<u8>>::from_bytes(plain.clone()).unwrap();
        let encrypted = encoder.encode_transport(frame, &mut sender).unwrap();
        let encrypted: &[u8] = encrypted.as_ref();

        let mut decoder = NoiseDecoder::new();
        let mut offset = 0;
        let mut rounds = 0;
        let decoded = loop {
            match decoder.next_transport_frame(receiver) {
                Ok(Decrypted::Frame(frame, _)) => break frame,
                Ok(Decrypted::Incomplete(n, state)) => {
                    receiver = state;
                    let writable = decoder.read_buf();
                    assert_eq!(n, writable.len());
                    writable.copy_from_slice(&encrypted[offset..offset + n]);
                    decoder.advance(n).unwrap();
                    offset += n;
                    rounds += 1;
                }
                Err(e) => panic!("failed to decode a multi-chunk noise frame: {e:?}"),
            }
        };

        assert!(rounds > 2, "the frame should not fit in a single read");
        assert_eq!(offset, encrypted.len());
        assert_eq!(decoded.as_bytes(), &plain[..]);
    }

    /// Verifies that a single decoder instance correctly decodes two consecutive independent
    /// frames in sequence, confirming that internal state resets between frames.
    #[quickcheck]
    fn prop_decoder_multiple_frames(
        msg1: TestMessage,
        msg2: TestMessage,
        msg_type: u8,
    ) -> TestResult {
        let frame1 =
            match MessageFrame::<TestMessage>::from_message(msg1.clone(), msg_type, 0, false) {
                Ok(f) => f,
                Err(_) => return TestResult::discard(),
            };
        let frame2 =
            match MessageFrame::<TestMessage>::from_message(msg2.clone(), msg_type, 0, false) {
                Ok(f) => f,
                Err(_) => return TestResult::discard(),
            };

        let mut encoder = Encoder::new();
        let encoded1 = match encoder.encode(frame1) {
            Ok(e) => e,
            Err(_) => return TestResult::failed(),
        };
        let encoded2 = match encoder.encode(frame2) {
            Ok(e) => e,
            Err(_) => return TestResult::failed(),
        };

        let mut decoder = Decoder::new();

        let decoded_msg1 = match decode_frame(&mut decoder, encoded1.as_ref(), None) {
            Some(mut f) => match binary_sv2::from_bytes::<TestMessage>(f.payload()) {
                Ok(m) => m,
                Err(_) => return TestResult::failed(),
            },
            None => return TestResult::failed(),
        };
        let decoded_msg2 = match decode_frame(&mut decoder, encoded2.as_ref(), None) {
            Some(mut f) => match binary_sv2::from_bytes::<TestMessage>(f.payload()) {
                Ok(m) => m,
                Err(_) => return TestResult::failed(),
            },
            None => return TestResult::failed(),
        };

        TestResult::from_bool(decoded_msg1 == msg1 && decoded_msg2 == msg2)
    }

    /// Verifies that encrypting then decrypting a frame via `NoiseEncoder`
    /// recovers the original message, msg_type, and ext_type exactly.
    #[cfg(feature = "noise_sv2")]
    #[quickcheck]
    fn prop_noise_encode_decode_roundtrip(
        msg: TestMessage,
        msg_type: u8,
        ext_type: u16,
    ) -> TestResult {
        let (mut sender_state, receiver_state) = make_transport_state_pair();
        let original = msg.clone();

        let sv2_frame =
            match MessageFrame::<TestMessage>::from_message(msg, msg_type, ext_type, false) {
                Ok(f) => f,
                Err(_) => return TestResult::discard(),
            };
        let expected_ext = sv2_frame.header().ext_type();

        let mut encoder = NoiseEncoder::new();
        let encrypted = match encoder.encode_transport(sv2_frame, &mut sender_state) {
            Ok(e) => e,
            Err(_) => return TestResult::failed(),
        };

        let mut decoder = NoiseDecoder::new();
        let encrypted_bytes: &[u8] = encrypted.as_ref();
        match decode_noise_frame(&mut decoder, receiver_state, encrypted_bytes) {
            Some((mut decoded, _)) => {
                let header = decoded.header();
                let decoded_msg: TestMessage = match binary_sv2::from_bytes(decoded.payload()) {
                    Ok(m) => m,
                    Err(_) => return TestResult::failed(),
                };
                TestResult::from_bool(
                    decoded_msg == original
                        && header.msg_type() == msg_type
                        && header.ext_type() == expected_ext,
                )
            }
            None => TestResult::failed(),
        }
    }

    /// Verifies that `NoiseDecoder` correctly handles data arriving in multiple rounds —
    /// one round per encrypted segment (header, then payload) — emitting `Incomplete`
    /// between rounds before returning the fully decrypted frame.
    #[cfg(feature = "noise_sv2")]
    #[quickcheck]
    fn prop_noise_decoder_handles_partial_data(msg: TestMessage, msg_type: u8) -> TestResult {
        let frame = match MessageFrame::<TestMessage>::from_message(msg, msg_type, 0, false) {
            Ok(f) => f,
            Err(_) => return TestResult::discard(),
        };

        let (mut sender_state, mut receiver_state) = make_transport_state_pair();
        let mut encoder = NoiseEncoder::new();
        let encrypted = match encoder.encode_transport(frame, &mut sender_state) {
            Ok(e) => e,
            Err(_) => return TestResult::failed(),
        };

        let mut decoder = NoiseDecoder::new();
        let encoded_bytes: &[u8] = encrypted.as_ref();
        let mut offset = 0;
        let mut missing_bytes_count = 0;

        loop {
            let writable = decoder.read_buf();
            let n = writable
                .len()
                .min(encoded_bytes.len().saturating_sub(offset));
            writable[..n].copy_from_slice(&encoded_bytes[offset..offset + n]);
            decoder.advance(n).unwrap();
            offset += n;

            match decoder.next_transport_frame(receiver_state) {
                Ok(Decrypted::Frame(..)) => return TestResult::from_bool(missing_bytes_count > 0),
                Ok(Decrypted::Incomplete(n, state)) => {
                    receiver_state = state;
                    missing_bytes_count += 1;
                    assert!(n > 0);
                }
                Err(_) => return TestResult::failed(),
            }
        }
    }

    #[cfg(feature = "noise_sv2")]
    #[test]
    fn noise_decoder_recovers_from_a_failed_decryption() {
        let (mut sender_state, receiver_state) = make_transport_state_pair();
        let frame =
            MessageFrame::<TestMessage>::from_message(TestMessage { value: 7 }, 0, 0, false)
                .unwrap();
        let mut encoder = NoiseEncoder::new();
        let encrypted = encoder.encode_transport(frame, &mut sender_state).unwrap();
        let encrypted: &[u8] = encrypted.as_ref();

        let mut decoder = NoiseDecoder::new();

        // Fail on the encrypted header. The closure never touches `receiver_state`, so its nonce
        // stays where it was and the same bytes can be replayed below.
        assert!(matches!(
            decoder.next_transport(|_| Err(crate::Error::AeadError(noise_sv2::AeadError))),
            Ok(Decoded::Incomplete(_))
        ));
        let writable = decoder.read_buf();
        let len = writable.len();
        writable.copy_from_slice(&encrypted[..len]);
        decoder.advance(len).unwrap();
        let failed = decoder
            .next_transport(|_| Err(crate::Error::AeadError(noise_sv2::AeadError)))
            .unwrap_err();
        assert!(matches!(failed, crate::Error::AeadError(_)));
        assert_eq!(IsBuffer::len(&decoder.sv2_buffer), 0);
        assert!(decoder.sv2_buffer.as_ref().is_empty());

        // The same decoder must now decode the frame from the start.
        let decoded = decode_noise_frame(&mut decoder, receiver_state, encrypted);
        match decoded {
            Some((mut f, _)) => assert_eq!(
                binary_sv2::from_bytes::<TestMessage>(f.payload()).unwrap(),
                TestMessage { value: 7 }
            ),
            None => panic!("failed to decode the frame after a failed decryption"),
        }
    }

    /// Verifies that a single `NoiseDecoder` instance correctly
    /// decodes two consecutive noise-encrypted frames in sequence using
    /// the same shared transport state.
    #[cfg(feature = "noise_sv2")]
    #[quickcheck]
    fn prop_noise_decoder_multiple_frames(
        msg1: TestMessage,
        msg2: TestMessage,
        msg_type: u8,
    ) -> TestResult {
        let (mut sender_state, receiver_state) = make_transport_state_pair();

        let frame1 =
            match MessageFrame::<TestMessage>::from_message(msg1.clone(), msg_type, 0, false) {
                Ok(f) => f,
                Err(_) => return TestResult::discard(),
            };
        let frame2 =
            match MessageFrame::<TestMessage>::from_message(msg2.clone(), msg_type, 0, false) {
                Ok(f) => f,
                Err(_) => return TestResult::discard(),
            };

        let mut encoder = NoiseEncoder::new();

        let enc1 = match encoder.encode_transport(frame1, &mut sender_state) {
            Ok(e) => e,
            Err(_) => return TestResult::failed(),
        };
        let enc2 = match encoder.encode_transport(frame2, &mut sender_state) {
            Ok(e) => e,
            Err(_) => return TestResult::failed(),
        };

        let mut decoder = NoiseDecoder::new();

        let (decoded_msg1, receiver_state) =
            match decode_noise_frame(&mut decoder, receiver_state, enc1.as_ref()) {
                Some((mut f, state)) => match binary_sv2::from_bytes::<TestMessage>(f.payload()) {
                    Ok(m) => (m, state),
                    Err(_) => return TestResult::failed(),
                },
                None => return TestResult::failed(),
            };

        let decoded_msg2 = match decode_noise_frame(&mut decoder, receiver_state, enc2.as_ref()) {
            Some((mut f, _)) => match binary_sv2::from_bytes::<TestMessage>(f.payload()) {
                Ok(m) => m,
                Err(_) => return TestResult::failed(),
            },
            None => return TestResult::failed(),
        };

        TestResult::from_bool(decoded_msg1 == msg1 && decoded_msg2 == msg2)
    }

    #[test]
    fn a_short_read_and_a_repeated_read_buf_are_not_counted_as_received() {
        let mut encoder = Encoder::new();
        let frame =
            MessageFrame::<TestMessage>::from_message(TestMessage { value: 7 }, 0, 0, false)
                .unwrap();
        let encoded = encoder.encode(frame).unwrap();
        let encoded: &[u8] = encoded.as_ref();

        let mut decoder = Decoder::new();
        decoder.read_buf().fill(0xff);
        decoder.read_buf()[..2].copy_from_slice(&encoded[..2]);
        decoder.advance(2).unwrap();
        assert_eq!(decoder.read_len(), Header::SIZE - 2);

        let mut frame = decode_frame(&mut decoder, &encoded[2..], Some(1)).unwrap();
        assert_eq!(
            binary_sv2::from_bytes::<TestMessage>(frame.payload()).unwrap(),
            TestMessage { value: 7 }
        );
    }

    #[test]
    fn advancing_past_the_read_window_is_an_error() {
        let mut decoder = Decoder::new();
        let window = decoder.read_buf().len();
        assert_eq!(
            decoder.advance(window + 1),
            Err(crate::Error::ReadBeyondWindow {
                read: window + 1,
                window,
            })
        );

        decoder.read_buf();
        let _ = decoder.next_frame();
        assert_eq!(
            decoder.advance(1),
            Err(crate::Error::ReadBeyondWindow { read: 1, window: 0 })
        );
    }

    #[cfg(feature = "noise_sv2")]
    #[test]
    fn a_short_read_is_not_counted_by_the_noise_decoder() {
        let (initiator, _) = make_handshake_pair();
        let first = initiator.step_0().unwrap().0.payload().to_vec();

        let mut decoder = NoiseDecoder::new();
        let _ = decoder.next_handshake_frame::<Responder>();
        decoder.read_buf().fill(0xff);
        decoder.read_buf()[..10].copy_from_slice(&first[..10]);
        decoder.advance(10).unwrap();
        assert!(matches!(
            decoder.next_handshake_frame::<Responder>(),
            Ok(Decoded::Incomplete(n)) if n == first.len() - 10
        ));

        decoder.read_buf().copy_from_slice(&first[10..]);
        decoder.advance(first.len() - 10).unwrap();
        let Ok(Decoded::Frame(message)) = decoder.next_handshake_frame::<Responder>() else {
            panic!("expected the handshake message");
        };
        assert_eq!(message.payload(), &first[..]);
    }
}
