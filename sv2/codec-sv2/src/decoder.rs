// # Decoder
//
// Provides utilities for decoding messages held by Sv2 frames, with or without Noise protocol
// support.
//
// It includes primitives to both decode encoded standard Sv2 frames and to decrypt and decode
// Noise-encrypted encoded Sv2 frames, ensuring secure communication when required.
//
// ## Usage
// All messages passed between Sv2 roles are encoded as Sv2 frames. These frames are decoded using
// primitives in this module. There are two types of decoders for reading these frames: one for
// regular Sv2 frames [`StandardDecoder`], and another for Noise-encrypted frames
// [`StandardNoiseDecoder`]. Both decoders manage the deserialization of incoming data and, when
// applicable, the decryption of the data upon receiving the transmitted message.
//
// ### Buffer Management
//
// The decoders rely on buffers to hold intermediate data during the decoding process.
//
// - When the `with_buffer_pool` feature is enabled, the internal `Buffer` type is backed by a
//   pool-allocated buffer [`binary_sv2::BufferPool`], providing more efficient memory usage,
//   particularly in high-throughput scenarios.
// - If this feature is not enabled, a system memory buffer [`binary_sv2::BufferFromSystemMemory`]
//   is used for simpler applications where memory efficiency is less critical.

#[cfg(feature = "noise_sv2")]
use binary_sv2::Deserialize;
#[cfg(feature = "noise_sv2")]
use binary_sv2::GetSize;
use binary_sv2::Serialize;
pub use buffer_sv2::AeadBuffer;
use core::marker::PhantomData;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing::HandShakeFrame;
use framing_sv2::{
    framing::{Frame, Sv2Frame},
    header::Header,
};
#[cfg(feature = "noise_sv2")]
use framing_sv2::{ENCRYPTED_SV2_FRAME_HEADER_SIZE, SV2_FRAME_CHUNK_SIZE, SV2_FRAME_HEADER_SIZE};
#[cfg(feature = "noise_sv2")]
use noise_sv2::NoiseCodec;
#[cfg(feature = "noise_sv2")]
use noise_sv2::NOISE_FRAME_HEADER_SIZE;

#[cfg(feature = "noise_sv2")]
use crate::error::Error;
use crate::error::Result;

use crate::Error::MissingBytes;
#[cfg(feature = "noise_sv2")]
use crate::State;

#[cfg(not(feature = "with_buffer_pool"))]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory as Buffer};

#[cfg(feature = "with_buffer_pool")]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory, BufferPool};

// The buffer type for holding intermediate data during decoding.
//
// When the `with_buffer_pool` feature is enabled, `Buffer` is a pool-allocated buffer type
// [`BufferPool`], which allows for more efficient memory management. Otherwise, it defaults to
// [`BufferFromSystemMemory`].
//
// `Buffer` is used for storing both serialized Sv2 frames and encrypted Noise data.
#[cfg(feature = "with_buffer_pool")]
type Buffer = BufferPool<BufferFromSystemMemory>;

/// An encoded or decoded Sv2 frame containing either a regular or Noise-protected message.
///
/// A wrapper around the [`Frame`] enum that represents either a regular or Noise-protected Sv2
/// frame containing the generic message type (`T`).
pub type StandardEitherFrame<T> = Frame<T, <Buffer as IsBuffer>::Slice>;

/// An encoded or decoded Sv2 frame.
///
/// A wrapper around the [`Sv2Frame`] that represents a regular Sv2 frame containing the generic
/// message type (`T`).
pub type StandardSv2Frame<T> = Sv2Frame<T, <Buffer as IsBuffer>::Slice>;

/// Standard Sv2 decoder with Noise protocol support.
///
/// Used for decoding and decrypting generic message types (`T`) encoded in Sv2 frames and
/// encrypted via the Noise protocol.
#[cfg(feature = "noise_sv2")]
pub type StandardNoiseDecoder<T> = WithNoise<Buffer, T>;

/// Standard Sv2 decoder without Noise protocol support.
///
/// Used for decoding generic message types (`T`) encoded in Sv2 frames.
pub type StandardDecoder<T> = WithoutNoise<Buffer, T>;

/// Decoder for Sv2 frames with Noise protocol support.
///
/// Accumulates the encrypted data into a dedicated buffer until the entire encrypted frame is
/// received. The Noise protocol is then used to decrypt the accumulated data into another
/// dedicated buffer, converting it back into its original serialized form. This decrypted data is
/// then deserialized into the original Sv2 frame and message format.
#[cfg(feature = "noise_sv2")]
#[derive(Debug)]
pub struct WithNoise<B: IsBuffer, T: Serialize + binary_sv2::GetSize> {
    // Marker for the type of frame being decoded.
    //
    // Used to maintain the generic type (`T`) information of the message payload held by the
    // frame. `T` refers to a type that implements the necessary traits for serialization
    // [`binary_sv2::Serialize`] and size calculation [`binary_sv2::GetSize`].
    frame: PhantomData<T>,

    // Tracks the number of bytes remaining until the full frame is received.
    //
    // Ensures that the full encrypted Noise frame has been received by keeping track of the
    // remaining bytes. Once the complete frame is received, decoding can proceed.
    missing_noise_b: usize,

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
}

#[cfg(feature = "noise_sv2")]
impl<'a, T: Serialize + GetSize + Deserialize<'a>, B: IsBuffer + AeadBuffer> WithNoise<B, T> {
    /// Attempts to decode the next Noise encrypted frame.
    ///
    /// On success, the decoded and decrypted frame is returned. Otherwise, an error indicating the
    /// number of missing bytes required to complete the encoded frame, an error on a badly
    /// formatted message header, or an error on decryption failure is returned.
    ///
    /// In this case of the `Error::MissingBytes`, the user should resize the decoder buffer using
    /// `writable`, read another chunk from the incoming message stream, and then call `next_frame`
    /// again. This process should be repeated until `next_frame` returns `Ok`, indicating that the
    /// full message has been received, and the decoding and decryption of the frame can proceed.
    #[inline]
    pub fn next_frame(&mut self, state: &mut State) -> Result<Frame<T, B::Slice>> {
        match state {
            State::HandShake(_) => unreachable!(),
            State::NotInitialized(msg_len) => {
                let hint = *msg_len - self.noise_buffer.as_ref().len();
                match hint {
                    0 => {
                        self.missing_noise_b = NOISE_FRAME_HEADER_SIZE;
                        Ok(self.while_handshaking())
                    }
                    _ => {
                        self.missing_noise_b = hint;
                        Err(Error::MissingBytes(hint))
                    }
                }
            }
            State::Transport(noise_codec) => {
                let hint = if IsBuffer::len(&self.sv2_buffer) < SV2_FRAME_HEADER_SIZE {
                    let len = IsBuffer::len(&self.noise_buffer);
                    let src = self.noise_buffer.get_data_by_ref(len);
                    if src.len() < ENCRYPTED_SV2_FRAME_HEADER_SIZE {
                        ENCRYPTED_SV2_FRAME_HEADER_SIZE - src.len()
                    } else {
                        0
                    }
                } else {
                    let src = self.sv2_buffer.get_data_by_ref(SV2_FRAME_HEADER_SIZE);
                    let header = Header::from_bytes(src)?;
                    header.encrypted_len() - IsBuffer::len(&self.noise_buffer)
                };

                match hint {
                    0 => {
                        self.missing_noise_b = ENCRYPTED_SV2_FRAME_HEADER_SIZE;
                        self.decode_noise_frame(noise_codec)
                    }
                    _ => {
                        self.missing_noise_b = hint;
                        Err(Error::MissingBytes(hint))
                    }
                }
            }
        }
    }

    /// Returns the number of bytes expected in the next read operation.
    ///
    /// This value indicates how many more bytes are required to complete the
    /// current Noise-encrypted frame. It is used to determine the exact size
    /// of the writable buffer that should be passed to the underlying stream
    /// during reading.
    ///
    /// The returned length dynamically updates as data is received and processed,
    /// and ensures that we only read as much as needed to complete the frame.
    pub fn writable_len(&self) -> usize {
        self.missing_noise_b
    }

    /// Provides a writable buffer for receiving incoming Noise-encrypted Sv2 data.
    ///
    /// This buffer is used to store incoming data, and its size is adjusted based on the number
    /// of missing bytes. As new data is read, it is written into this buffer until enough data has
    /// been received to fully decode a frame. The buffer must have the correct number of bytes
    /// available to progress to the decoding process.
    #[inline]
    pub fn writable(&mut self) -> &mut [u8] {
        self.noise_buffer.get_writable(self.missing_noise_b)
    }

    /// Determines whether the decoder's internal buffers can be safely dropped.
    ///
    /// For more information, refer to the [`buffer_sv2`
    /// crate](https://docs.rs/buffer_sv2/latest/buffer_sv2/).
    pub fn droppable(&self) -> bool {
        self.noise_buffer.is_droppable() && self.sv2_buffer.is_droppable()
    }

    // Processes and decodes a Sv2 frame during the Noise protocol handshake phase.
    //
    // Handles the decoding of a handshake frame from the `noise_buffer`. It converts the received
    // data into a `HandShakeFrame` and encapsulates it into a `Frame` for further processing by
    // the codec.
    //
    // This is used exclusively during the initial handshake phase of the Noise protocol, before
    // transitioning to regular frame encryption and decryption.
    fn while_handshaking(&mut self) -> Frame<T, B::Slice> {
        let src = self.noise_buffer.get_data_owned().as_mut().to_vec();

        // Since the frame length is already validated during the handshake process, this
        // operation is infallible
        let frame = HandShakeFrame::from_bytes_unchecked(src.into());

        frame.into()
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
    // complete the frame, the function will return an `Error::MissingBytes` with the number of
    // additional bytes required to fully decrypt the frame. Once all bytes are available, the
    // decryption process completes and the frame can be successfully decoded.
    #[inline]
    fn decode_noise_frame(&mut self, noise_codec: &mut NoiseCodec) -> Result<Frame<T, B::Slice>> {
        match (
            IsBuffer::len(&self.noise_buffer),
            IsBuffer::len(&self.sv2_buffer),
        ) {
            // HERE THE SV2 HEADER IS READY TO BE DECRYPTED
            (ENCRYPTED_SV2_FRAME_HEADER_SIZE, 0) => {
                let src = self.noise_buffer.get_data_owned();
                let decrypted_header = self
                    .sv2_buffer
                    .get_writable(ENCRYPTED_SV2_FRAME_HEADER_SIZE);
                decrypted_header.copy_from_slice(src.as_ref());
                self.sv2_buffer.as_ref();
                noise_codec.decrypt(&mut self.sv2_buffer)?;
                let header =
                    Header::from_bytes(self.sv2_buffer.get_data_by_ref(SV2_FRAME_HEADER_SIZE))?;
                self.missing_noise_b = header.encrypted_len();
                Err(Error::MissingBytes(header.encrypted_len()))
            }
            // HERE THE SV2 PAYLOAD IS READY TO BE DECRYPTED
            _ => {
                // DECRYPT THE PAYLOAD IN CHUNKS
                let encrypted_payload = self.noise_buffer.get_data_owned();
                let encrypted_payload_len = encrypted_payload.as_ref().len();
                let mut start = 0;
                let mut end = if encrypted_payload_len < SV2_FRAME_CHUNK_SIZE {
                    encrypted_payload_len
                } else {
                    SV2_FRAME_CHUNK_SIZE
                };
                // Do not try to decrypt the header cause it is already decrypted
                let mut decrypted_len = SV2_FRAME_HEADER_SIZE;

                while start < encrypted_payload_len {
                    let decrypted_payload = self.sv2_buffer.get_writable(end - start);
                    decrypted_payload.copy_from_slice(&encrypted_payload.as_ref()[start..end]);
                    self.sv2_buffer.danger_set_start(decrypted_len);
                    noise_codec.decrypt(&mut self.sv2_buffer)?;
                    start = end;
                    end = (start + SV2_FRAME_CHUNK_SIZE).min(encrypted_payload_len);
                    decrypted_len += self.sv2_buffer.as_ref().len();
                }
                self.sv2_buffer.danger_set_start(0);
                let src = self.sv2_buffer.get_data_owned();
                let frame = Sv2Frame::<T, B::Slice>::from_bytes_unchecked(src);
                Ok(frame.into())
            }
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + binary_sv2::GetSize> WithNoise<Buffer, T> {
    /// Crates a new [`WithNoise`] decoder with default buffer sizes.
    ///
    /// Initializes the decoder with default buffer sizes and sets the number of missing bytes to
    /// 0.
    pub fn new() -> Self {
        Self {
            frame: PhantomData,
            missing_noise_b: 0,
            noise_buffer: Buffer::new(2_usize.pow(16) * 5),
            sv2_buffer: Buffer::new(2_usize.pow(16) * 5),
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + binary_sv2::GetSize> Default for WithNoise<Buffer, T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Decoder for standard Sv2 frames.
///
/// Accumulates the data into a dedicated buffer until the entire Sv2 frame is received. This data
/// is then deserialized into the original Sv2 frame and message format.
#[derive(Debug)]
pub struct WithoutNoise<B: IsBuffer, T: Serialize + binary_sv2::GetSize> {
    // Marker for the type of frame being decoded.
    //
    // Used to maintain the generic type (`T`) information of the message payload held by the
    // frame. `T` refers to a type that implements the necessary traits for serialization
    // [`binary_sv2::Serialize`] and size calculation [`binary_sv2::GetSize`].
    frame: PhantomData<T>,

    // Tracks the number of bytes remaining until the full frame is received.
    //
    // Ensures that the full Sv2 frame has been received by keeping track of the remaining bytes.
    // Once the complete frame is received, decoding can proceed.
    missing_b: usize,

    // Buffer for holding incoming data to be decoded into a Sv2 frame.
    //
    // This buffer stores incoming data as it is received, allowing the decoder to accumulate the
    // necessary bytes until a full frame is available. Once the full encoded frame has been
    // received, the buffer's contents are processed and decoded into an Sv2 frame.
    buffer: B,
}

impl<T: Serialize + binary_sv2::GetSize, B: IsBuffer> WithoutNoise<B, T> {
    /// Attempts to decode the next frame, returning either a frame or an error indicating how many
    /// bytes are missing.
    ///
    /// Attempts to decode the next Sv2 frame.
    ///
    /// On success, the decoded frame is returned. Otherwise, an error indicating the number of
    /// missing bytes required to complete the frame is returned.
    ///
    /// In the case of `Error::MissingBytes`, the user should resize the decoder buffer using
    /// `writable`, read another chunk from the incoming message stream, and then call `next_frame`
    /// again. This process should be repeated until `next_frame` returns `Ok`, indicating that the
    /// full message has been received, and the frame can be fully decoded.
    #[inline]
    pub fn next_frame(&mut self) -> Result<Sv2Frame<T, B::Slice>> {
        let len = self.buffer.len();
        let src = self.buffer.get_data_by_ref(len);
        let hint = Sv2Frame::<T, B::Slice>::size_hint(src) as usize;

        match hint {
            0 => {
                self.missing_b = Header::SIZE;
                let src = self.buffer.get_data_owned();
                let frame = Sv2Frame::<T, B::Slice>::from_bytes_unchecked(src);
                Ok(frame)
            }
            _ => {
                self.missing_b = hint;
                Err(MissingBytes(self.missing_b))
            }
        }
    }

    /// Provides a writable buffer for receiving incoming Sv2 data.
    ///
    /// This buffer is used to store incoming data, and its size is adjusted based on the number of
    /// missing bytes. As new data is read, it is written into this buffer until enough data has
    /// been received to fully decode a frame. The buffer must have the correct number of bytes
    /// available to progress to the decoding process.
    pub fn writable(&mut self) -> &mut [u8] {
        self.buffer.get_writable(self.missing_b)
    }
}

impl<T: Serialize + binary_sv2::GetSize> WithoutNoise<Buffer, T> {
    /// Creates a new [`WithoutNoise`] with a buffer of default size.
    ///
    /// Initializes the decoder with a default buffer size and sets the number of missing bytes to
    /// the size of the header.
    pub fn new() -> Self {
        Self {
            frame: PhantomData,
            missing_b: Header::SIZE,
            buffer: Buffer::new(2_usize.pow(16) * 5),
        }
    }
}

impl<T: Serialize + binary_sv2::GetSize> Default for WithoutNoise<Buffer, T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binary_sv2::{binary_codec_sv2, Serialize};

    #[derive(Serialize)]
    pub struct TestMessage {}

    #[test]
    fn unencrypted_writable_with_missing_b_initialized_as_header_size() {
        let mut decoder = StandardDecoder::<TestMessage>::new();
        let actual = decoder.writable();
        let expect = [0u8; Header::SIZE];
        assert_eq!(actual, expect);
    }
}
