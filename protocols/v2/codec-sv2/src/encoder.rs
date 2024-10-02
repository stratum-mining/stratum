//! # Encoder
//!
//! Provides functionality for encoding Stratum V2 messages, including support for the
//! Noise protocol for secure communication.
//!
//! ## Usage
//!
//! This module is designed to be used to encode outgoing Sv2 messages, with optional Noise
//! protocol encryption support for secure communication.

use alloc::vec::Vec;
use binary_sv2::{GetSize, Serialize};
#[allow(unused_imports)]
pub use const_sv2::{AEAD_MAC_LEN, SV2_FRAME_CHUNK_SIZE, SV2_FRAME_HEADER_SIZE};
#[cfg(feature = "noise_sv2")]
use core::convert::TryInto;
use core::marker::PhantomData;
use framing_sv2::framing::Sv2Frame;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing::{Frame, HandShakeFrame};
#[allow(unused_imports)]
pub use framing_sv2::header::NOISE_HEADER_ENCRYPTED_SIZE;

#[cfg(feature = "noise_sv2")]
use tracing::error;

#[cfg(feature = "noise_sv2")]
use crate::{Error, Result, State};

#[cfg(feature = "noise_sv2")]
#[cfg(not(feature = "with_buffer_pool"))]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory as Buffer};

#[cfg(feature = "noise_sv2")]
#[cfg(feature = "with_buffer_pool")]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory, BufferPool};

/// The buffer type for holding intermediate data during encoding.
///
/// When the `with_buffer_pool` feature is enabled, `Buffer` is a pool-allocated buffer type
/// (`BufferPool`), which allows for more efficient memory management. Otherwise, it defaults to
/// `BufferFromSystemMemory`.
///
/// `Buffer` is used for storing both serialized Sv2 frames and encrypted Noise data.
#[cfg(feature = "noise_sv2")]
#[cfg(feature = "with_buffer_pool")]
type Buffer = BufferPool<BufferFromSystemMemory>;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

/// Holds the frame's serialized bytes before transmission.
#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

/// Encoder for Sv2 frames with Noise protocol support.
#[cfg(feature = "noise_sv2")]
pub struct NoiseEncoder<T: Serialize + binary_sv2::GetSize> {
    /// Buffer for holding encrypted Noise data.
    ///
    /// Stores the encrypted data from the Sv2 frame after it has been processed by the Noise
    /// protocol and is ready to be transmitted.
    noise_buffer: Buffer,

    /// Buffer for holding serialized Sv2 data.
    ///
    /// Stores the data to be encrypted by the Noise protocol after being encoded into a Sv2 frame.
    sv2_buffer: Buffer,

    /// Used to maintain type information for the generic parameter `T`, which represents the type
    /// of frames being encoded.
    ///
    /// `T` refers to a type that implements the necessary traits for serialization
    /// (`binary_sv2::Serialize`) and size calculation (`binary_sv2::GetSize`).
    frame: PhantomData<T>,
}

/// A Sv2 frame to be encoded and optionally encrypted using the Noise protocol.
///
/// `Item` is primarily used in the context of encoding frames before transmission. It is an alias
/// to a `Frame` that contains a payload of type `T` and uses a `Slice` as the underlying buffer.
/// This type is used during the encoding process to encapsulate the data being processed.
#[cfg(feature = "noise_sv2")]
type Item<T> = Frame<T, Slice>;

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + GetSize> NoiseEncoder<T> {
    /// Encodes a Noise-encrypted frame.
    ///
    /// Encodes the given `item` into a Sv2 frame and then encrypts the frame using the Noise
    /// protocol. The `state` of the Noise codec determines whether the encoder is in the handshake
    /// or transport phase. On success, an encrypted frame as a `Slice` is returned.
    #[inline]
    pub fn encode(&mut self, item: Item<T>, state: &mut State) -> Result<Slice> {
        match state {
            State::Transport(noise_codec) => {
                let len = item.encoded_length();
                let writable = self.sv2_buffer.get_writable(len);

                // ENCODE THE SV2 FRAME
                let i: Sv2Frame<T, Slice> = item.try_into().map_err(|e| {
                    error!("Error while encoding 1 frame: {:?}", e);
                    Error::FramingError(e)
                })?;
                i.serialize(writable)?;

                let sv2 = self.sv2_buffer.get_data_owned();
                let sv2: &[u8] = sv2.as_ref();

                // ENCRYPT THE HEADER
                let to_encrypt = self.noise_buffer.get_writable(SV2_FRAME_HEADER_SIZE);
                to_encrypt.copy_from_slice(&sv2[..SV2_FRAME_HEADER_SIZE]);
                noise_codec.encrypt(&mut self.noise_buffer)?;

                // ENCRYPT THE PAYLOAD IN CHUNKS
                let mut start = SV2_FRAME_HEADER_SIZE;
                let mut end = if sv2.len() - start < (SV2_FRAME_CHUNK_SIZE - AEAD_MAC_LEN) {
                    sv2.len()
                } else {
                    SV2_FRAME_CHUNK_SIZE + start - AEAD_MAC_LEN
                };
                let mut encrypted_len = NOISE_HEADER_ENCRYPTED_SIZE;

                while start < sv2.len() {
                    let to_encrypt = self.noise_buffer.get_writable(end - start);
                    to_encrypt.copy_from_slice(&sv2[start..end]);
                    self.noise_buffer.danger_set_start(encrypted_len);
                    noise_codec.encrypt(&mut self.noise_buffer)?;
                    encrypted_len += self.noise_buffer.as_ref().len();
                    start = end;
                    end = (start + SV2_FRAME_CHUNK_SIZE - AEAD_MAC_LEN).min(sv2.len());
                }
                self.noise_buffer.danger_set_start(0);
            }
            State::HandShake(_) => self.while_handshaking(item)?,
            State::NotInitialized(_) => self.while_handshaking(item)?,
        };

        // Clear sv2_buffer
        self.sv2_buffer.get_data_owned();
        // Return noise_buffer
        Ok(self.noise_buffer.get_data_owned())
    }

    /// Processes and encodes Sv2 frames during the handshake phase of the Noise protocol.
    ///
    /// Used when the codec is in the handshake phase. It encodes the provided `item` into a
    /// handshake frame and stores the payload in the `noise_buffer`. This is necessary to
    /// establish the initial secure communication channel before transitioning to the transport
    /// phase of the Noise protocol.
    #[inline(never)]
    fn while_handshaking(&mut self, item: Item<T>) -> Result<()> {
        // ENCODE THE SV2 FRAME
        let i: HandShakeFrame = item.try_into().map_err(|e| {
            error!("Error while encoding 2 frame - while_handshaking: {:?}", e);
            Error::FramingError(e)
        })?;
        let payload = i.get_payload_when_handshaking();
        let wrtbl = self.noise_buffer.get_writable(payload.len());
        for (i, b) in payload.iter().enumerate() {
            wrtbl[i] = *b;
        }
        Ok(())
    }

    /// Determines whether the encoder's internal buffers can be safely dropped.
    pub fn droppable(&self) -> bool {
        self.noise_buffer.is_droppable() && self.sv2_buffer.is_droppable()
    }
}

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + binary_sv2::GetSize> NoiseEncoder<T> {
    /// Creates a new `NoiseEncoder` with default buffer sizes.
    pub fn new() -> Self {
        #[cfg(not(feature = "with_buffer_pool"))]
        let size = 512;
        #[cfg(feature = "with_buffer_pool")]
        let size = 2_usize.pow(16) * 5;
        Self {
            sv2_buffer: Buffer::new(size),
            noise_buffer: Buffer::new(size),
            frame: core::marker::PhantomData,
        }
    }
}

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + GetSize> Default for NoiseEncoder<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Encoder for Sv2 frames without Noise protocol support.
#[derive(Debug)]
pub struct Encoder<T> {
    /// Buffer for holding data to be encoded.
    ///
    /// Stores data to be processed and converted into an Sv2 frame before it is transmitted.
    buffer: Vec<u8>,

    /// Marker for the type of frame being encoded.
    ///
    /// Used to maintain type information for the generic parameter `T`, which represents the type
    /// of frames being encoded. `T` refers to a type that implements the necessary traits for
    /// serialization (`binary_sv2::Serialize`) and size calculation (`binary_sv2::GetSize`).
    frame: PhantomData<T>,
}

impl<T: Serialize + GetSize> Encoder<T> {
    /// Encodes the provided `item` into a Sv2 frame, storing the result in the internal buffer.
    ///
    /// The frame of type `T` is serialized and placed in into the buffer, preparing it for
    /// transmission.
    pub fn encode(
        &mut self,
        item: Sv2Frame<T, Slice>,
    ) -> core::result::Result<&[u8], crate::Error> {
        let len = item.encoded_length();

        self.buffer.resize(len, 0);

        item.serialize(&mut self.buffer)?;

        Ok(&self.buffer[..])
    }

    /// Creates a new `Encoder` with a buffer of default size.
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(512),
            frame: core::marker::PhantomData,
        }
    }
}

impl<T: Serialize + GetSize> Default for Encoder<T> {
    fn default() -> Self {
        Self::new()
    }
}
