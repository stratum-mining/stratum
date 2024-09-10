//! # Stratum V2 Codec Decoder
//!
//! This module provides functionality for decoding Stratum V2 messages, including support for
//! the Noise protocol for secure communication.
//!
//! ## Features
//!
//! * **Standard Decoder**: Decodes Stratum V2 frames without encryption.
//! * **Noise Decoder**: Decodes Stratum V2 frames with Noise protocol encryption.
//!
//! ## Types
//!
//! * `StandardEitherFrame`: Represents an encoded or decoded frame that could be either a regular
//!   or Noise-protected frame.
//! * `StandardSv2Frame`: Represents an encoded or decoded Stratum V2 frame.
//! * `StandardNoiseDecoder`: Decoder for Stratum V2 frames with Noise protocol support.
//! * `StandardDecoder`: Decoder for Stratum V2 frames without Noise protocol support.
//!
//! ## Usage
//!
//! This module is designed to be used to decode incoming Stratum V2 messages, potentially with
//! Noise protocol encryption for secure communication.
//!
//! ## Example
//!
//! ```ignore
//! use codec_sv2::decoder::{StandardDecoder, StandardNoiseDecoder};
//!
//! // Create a standard decoder
//! let mut decoder: StandardDecoder<MyFrameType> = StandardDecoder::new();
//!
//! // Create a noise decoder (requires the `noise_sv2` feature)
//! #[cfg(feature = "noise_sv2")]
//! let mut noise_decoder: StandardNoiseDecoder<MyFrameType> = StandardNoiseDecoder::new();
//! ```
//!

#[cfg(feature = "noise_sv2")]
use binary_sv2::Deserialize;
#[cfg(feature = "noise_sv2")]
use binary_sv2::GetSize;
use binary_sv2::Serialize;
pub use buffer_sv2::AeadBuffer;
#[allow(unused_imports)]
pub use const_sv2::{SV2_FRAME_CHUNK_SIZE, SV2_FRAME_HEADER_SIZE};
use core::marker::PhantomData;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing::HandShakeFrame;
#[cfg(feature = "noise_sv2")]
use framing_sv2::header::{NOISE_HEADER_ENCRYPTED_SIZE, NOISE_HEADER_SIZE};
use framing_sv2::{
    framing::{Frame, Sv2Frame},
    header::Header,
};
#[cfg(feature = "noise_sv2")]
use noise_sv2::NoiseCodec;

#[cfg(not(feature = "with_buffer_pool"))]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory as Buffer};

#[cfg(feature = "with_buffer_pool")]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory, BufferPool};
#[cfg(feature = "with_buffer_pool")]
type Buffer = BufferPool<BufferFromSystemMemory>;

#[cfg(feature = "noise_sv2")]
use crate::error::Error;
use crate::error::Result;

use crate::Error::MissingBytes;
#[cfg(feature = "noise_sv2")]
use crate::State;

/// An encoded or decoded frame that could either be a regular or Noise-protected frame.
pub type StandardEitherFrame<T> = Frame<T, <Buffer as IsBuffer>::Slice>;

/// An encoded or decoded Sv2 frame.
pub type StandardSv2Frame<T> = Sv2Frame<T, <Buffer as IsBuffer>::Slice>;

/// Standard decoder with Noise protocol support.
#[cfg(feature = "noise_sv2")]
pub type StandardNoiseDecoder<T> = WithNoise<Buffer, T>;

/// Standard Sv2 decoder without Noise protocol support.
pub type StandardDecoder<T> = WithoutNoise<Buffer, T>;

/// Decoder for Sv2 frames with Noise protocol support.
#[cfg(feature = "noise_sv2")]
pub struct WithNoise<B: IsBuffer, T: Serialize + binary_sv2::GetSize> {
    /// Used to maintain type information for the generic parameter `T`, which represents the type
    /// of frames being decoded.
    ///
    /// `T` refers to a type that implements the necessary traits for serialization
    /// (`binary_sv2::Serialize`), deserialization (`binary_sv2::Deserialize`), and size
    /// calculation (`binary_sv2::GetSize`).
    frame: PhantomData<T>,

    /// Number of missing bytes needed to complete the Noise header or payload.
    ///
    /// Keeps track of how many more bytes are required to fully decode the current Noise frame.
    missing_noise_b: usize,

    /// Buffer for holding encrypted Noise data.
    ///
    /// Stores the incoming encrypted data until it is ready to be decrypted and processed.
    noise_buffer: B,

    /// Buffer for holding decrypted Sv2 data.
    ///
    /// Stores the decrypted data from the Noise protocol until it is ready to be processed and
    /// concerted into a Sv2 frame.
    sv2_buffer: B,
}

#[cfg(feature = "noise_sv2")]
impl<'a, T: Serialize + GetSize + Deserialize<'a>, B: IsBuffer + AeadBuffer> WithNoise<B, T> {
    /// Attempts to decode the next frame, returning either a frame or an error indicating how many
    /// bytes are missing.
    #[inline]
    pub fn next_frame(&mut self, state: &mut State) -> Result<Frame<T, B::Slice>> {
        match state {
            State::HandShake(_) => unreachable!(),
            State::NotInitialized(msg_len) => {
                let hint = *msg_len - self.noise_buffer.as_ref().len();
                match hint {
                    0 => {
                        self.missing_noise_b = NOISE_HEADER_SIZE;
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
                    if src.len() < NOISE_HEADER_ENCRYPTED_SIZE {
                        NOISE_HEADER_ENCRYPTED_SIZE - src.len()
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
                        self.missing_noise_b = NOISE_HEADER_ENCRYPTED_SIZE;
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

    /// Decodes a Noise-encrypted frame.
    ///
    /// Handles the decryption of Noise-encrypted frames, including both the header and the
    /// payload. It processes the frame in chunks if necessary, ensuring that all encrypted data is
    /// properly decrypted and converted into a usable frame.
    #[inline]
    fn decode_noise_frame(&mut self, noise_codec: &mut NoiseCodec) -> Result<Frame<T, B::Slice>> {
        match (
            IsBuffer::len(&self.noise_buffer),
            IsBuffer::len(&self.sv2_buffer),
        ) {
            // HERE THE SV2 HEADER IS READY TO BE DECRYPTED
            (NOISE_HEADER_ENCRYPTED_SIZE, 0) => {
                let src = self.noise_buffer.get_data_owned();
                let decrypted_header = self.sv2_buffer.get_writable(NOISE_HEADER_ENCRYPTED_SIZE);
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

    /// Processes frames during the handshake phase.
    ///
    /// Used while the codec is in the handshake phase of the Noise protocol. It processes and
    /// returns a handshake frame that has been received and encapsulates it in an `Frame`,
    /// indicating the frame has been processed and is ready to be handled by the codec.
    fn while_handshaking(&mut self) -> Frame<T, B::Slice> {
        let src = self.noise_buffer.get_data_owned().as_mut().to_vec();

        // below is inffalible as noise frame length has been already checked
        let frame = HandShakeFrame::from_bytes_unchecked(src.into());

        frame.into()
    }

    /// Provides a writable buffer for incoming data.
    #[inline]
    pub fn writable(&mut self) -> &mut [u8] {
        self.noise_buffer.get_writable(self.missing_noise_b)
    }

    /// Checks if the buffers are droppable.
    pub fn droppable(&self) -> bool {
        self.noise_buffer.is_droppable() && self.sv2_buffer.is_droppable()
    }
}

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + binary_sv2::GetSize> WithNoise<Buffer, T> {
    /// Crates a new `WithNoise` decoder with default buffer sizes.
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

/// Decoder for Sv2 frames without Noise protocol support.
#[derive(Debug)]
pub struct WithoutNoise<B: IsBuffer, T: Serialize + binary_sv2::GetSize> {
    /// Marker for the type of frame being decoded.
    ///
    /// Used to maintain type information for the generic parameter `T` which represents the type
    /// of frames being decoded. `T` refers to a type that implements the necessary traits for
    /// serialization (`binary_sv2::Serialize`), deserialization (`binary_sv2::Deserialize`), and
    /// size calculation (`binary_sv2::GetSize`).
    frame: PhantomData<T>,

    /// Number of missing bytes needed to complete the frame.
    ///
    /// Keeps track of how many more bytes are required to fully decode the current frame.
    missing_b: usize,

    /// Buffer for holding data to be decoded.
    ///
    /// Stores incoming data until it is ready to be processed and converted into a Sv2 frame.
    buffer: B,
}

impl<T: Serialize + binary_sv2::GetSize, B: IsBuffer> WithoutNoise<B, T> {
    /// Attempts to decode the next frame, returning either a frame or an error indicating how many
    /// bytes are missing.
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

    /// Provides a writable buffer for incoming data.
    pub fn writable(&mut self) -> &mut [u8] {
        self.buffer.get_writable(self.missing_b)
    }
}

impl<T: Serialize + binary_sv2::GetSize> WithoutNoise<Buffer, T> {
    /// Creates a new `WithoutNoise` decoder with default buffer sizes.
    ///
    /// Initializes the decoder with default buffer sizes and sets the number of missing bytes to
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
