// # Encoder
//
// Provides utilities for encoding messages into Sv2 frames, with or without Noise protocol
// support.
//
// ## Usage
//
// All messages passed between Sv2 roles are encoded as Sv2 frames using primitives in this module.
// There are two types of encoders for creating these frames: one for regular Sv2 frames
// [`Encoder`], and another for Noise-encrypted frames [`NoiseEncoder`]. Both encoders manage the
// serialization of outgoing data and, when applicable, the encryption of the data before
// transmission.
//
// ### Buffer Management
//
// The encoders rely on buffers to hold intermediate data during the encoding process.
//
// - When the `with_buffer_pool` feature is enabled, the internal `Buffer` type is backed by a
//   pool-allocated buffer [`binary_sv2::BufferPool`], providing more efficient memory usage,
//   particularly in high-throughput scenarios.
// - If the feature is not enabled, a system memory buffer [`binary_sv2::BufferFromSystemMemory`] is
//   used for simpler applications where memory efficiency is less critical.

use alloc::vec::Vec;
use binary_sv2::{GetSize, Serialize};
#[cfg(feature = "noise_sv2")]
use core::convert::TryInto;
use core::marker::PhantomData;
use framing_sv2::framing::Sv2Frame;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing::{Frame, HandShakeFrame};
#[cfg(feature = "noise_sv2")]
use framing_sv2::{ENCRYPTED_SV2_FRAME_HEADER_SIZE, SV2_FRAME_CHUNK_SIZE, SV2_FRAME_HEADER_SIZE};
#[cfg(feature = "noise_sv2")]
use noise_sv2::AEAD_MAC_LEN;

#[cfg(feature = "tracing")]
use tracing::error;

#[cfg(feature = "noise_sv2")]
use crate::{Error, Result, State};

#[cfg(feature = "noise_sv2")]
#[cfg(not(feature = "with_buffer_pool"))]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory as Buffer};

#[cfg(feature = "noise_sv2")]
#[cfg(feature = "with_buffer_pool")]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory, BufferPool};

// The buffer type for holding intermediate data during encoding.
//
// When the `with_buffer_pool` feature is enabled, `Buffer` uses a pool-allocated buffer
// [`BufferPool`], providing more efficient memory management, particularly in high-throughput
// environments. If the feature is not enabled, it defaults to [`BufferFromSystemMemory`], a
// simpler system memory buffer.
//
// `Buffer` is utilized for storing both serialized Sv2 frames and encrypted Noise data during the
// encoding process, ensuring that all frames are correctly handled before transmission.
#[cfg(feature = "noise_sv2")]
#[cfg(feature = "with_buffer_pool")]
type Buffer = BufferPool<BufferFromSystemMemory>;

// A simple buffer slice for holding serialized Sv2 frame data before transmission.
//
// When the `with_buffer_pool` feature is disabled, [`Slice`] defaults to a `Vec<u8>`, which serves
// as a dynamically allocated array to hold the serialized bytes of Sv2 frames. This provides
// flexibility in managing the encoded data during transmission or further processing, though it
// may not offer the same memory efficiency as the pool-allocated version [`BufferPool`].
#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

// A buffer slice used for holding serialized Sv2 frame data before transmission.
//
// When the `with_buffer_pool` feature is enabled, [`Slice`] defaults to a `buffer_sv2::Slice`,
// which serves as a slice of the `Buffer` that stores the encoded data. It holds the frame's
// serialized bytes temporarily, ensuring the data is ready for transmission or encryption,
// depending on whether Noise protocol support is enabled.
#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

/// Encoder for Sv2 frames with Noise protocol encryption.
///
/// Serializes the Sv2 frame into a dedicated buffer. Encrypts this serialized data using the Noise
/// protocol, storing it into another dedicated buffer. Encodes the serialized and encrypted data,
/// such that it is ready for transmission.
#[cfg(feature = "noise_sv2")]
pub struct NoiseEncoder<T: Serialize + binary_sv2::GetSize> {
    // Buffer for holding encrypted Noise data to be transmitted.
    //
    // Stores the encrypted data after the Sv2 frame has been processed by the Noise protocol
    // and is ready for transmission. This buffer holds the outgoing encrypted data, ensuring
    // that the full frame is correctly prepared before being sent.
    noise_buffer: Buffer,

    // Buffer for holding serialized Sv2 data before encryption.
    //
    // Stores the data after it has been serialized into an Sv2 frame but before it is encrypted
    // by the Noise protocol. The buffer accumulates the frame's serialized bytes before they are
    // encrypted and then encoded for transmission.
    sv2_buffer: Buffer,

    // Marker for the type of frame being encoded.
    //
    // Used to maintain the generic type information for `T`, which represents the message payload
    // contained within the Sv2 frame. `T` refers to a type that implements the necessary traits
    // for serialization [`binary_sv2::Serialize`] and size calculation [`binary_sv2::GetSize`],
    // ensuring that the encoder can handle different message types correctly during the encoding
    // process.
    frame: PhantomData<T>,
}

// A Sv2 frame that will be encoded and optionally encrypted using the Noise protocol.
//
// Represent a Sv2 frame during the encoding process. It encapsulates the frame's generic payload
// message type (`T`) and is stored in a [`Slice`] buffer. The `Item` is passed to the encoder,
// which either processes it for normal transmission or applies Noise encryption, depending on the
// codec's state.
#[cfg(feature = "noise_sv2")]
type Item<T> = Frame<T, Slice>;

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + GetSize> NoiseEncoder<T> {
    /// Encodes an Sv2 frame and encrypts it using the Noise protocol.
    ///
    /// Takes an `item`, which is an Sv2 frame containing a payload of type `T`, and encodes it for
    /// transmission. The frame is encrypted after being serialized. The `state` parameter
    /// determines whether the encoder is in the handshake or transport phase, guiding the
    /// appropriate encoding and encryption action.
    ///
    /// - In the handshake phase, the initial handshake messages are processed to establish secure
    ///   communication.
    /// - In the transport phase, the full frame is serialized, encrypted, and stored in a buffer
    ///   for transmission.
    ///
    /// On success, the method returns an encrypted (`Slice`) (buffer) ready for transmission.
    /// Otherwise, errors on an encryption or serialization failure.
    #[inline]
    pub fn encode(&mut self, item: Item<T>, state: &mut State) -> Result<Slice> {
        match state {
            State::Transport(noise_codec) => {
                let len = item.encoded_length();
                let writable = self.sv2_buffer.get_writable(len);

                // ENCODE THE SV2 FRAME
                let i: Sv2Frame<T, Slice> = item.try_into().map_err(|e| {
                    #[cfg(feature = "tracing")]
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
                let mut encrypted_len = ENCRYPTED_SV2_FRAME_HEADER_SIZE;

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

    // Encodes Sv2 frames during the handshake phase of the Noise protocol.
    //
    // Used when the encoder is in the handshake phase, before secure communication is fully
    // established. It encodes the provided `item` into a handshake frame, storing the resulting
    // data in the `noise_buffer`. The handshake phase is necessary to exchange initial messages
    // and set up the Noise encryption state before transitioning to the transport phase, where
    // full frames are encrypted and transmitted.
    #[inline(never)]
    fn while_handshaking(&mut self, item: Item<T>) -> Result<()> {
        // ENCODE THE SV2 FRAME
        let i: HandShakeFrame = item.try_into().map_err(|e| {
            #[cfg(feature = "tracing")]
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

/// Encoder for standard Sv2 frames.
///
/// Serializes the Sv2 frame into a dedicated buffer then encodes it, such that it is ready for
/// transmission.
#[derive(Debug)]
pub struct Encoder<T> {
    // Buffer for holding serialized Sv2 data.
    //
    // Stores the serialized bytes of the Sv2 frame after it has been encoded. Once the frame is
    // serialized, the resulting bytes are stored in this buffer to be transmitted. The buffer is
    // dynamically resized to accommodate the size of the encoded frame.
    buffer: Vec<u8>,

    // Marker for the type of frame being encoded.
    //
    // Used to maintain the generic type information for `T`, which represents the message payload
    // contained within the Sv2 frame. `T` refers to a type that implements the necessary traits
    // for serialization [`binary_sv2::Serialize`] and size calculation [`binary_sv2::GetSize`],
    // ensuring that the encoder can handle different message types correctly during the encoding
    // process.
    frame: PhantomData<T>,
}

impl<T: Serialize + GetSize> Encoder<T> {
    /// Encodes a standard Sv2 frame for transmission.
    ///
    /// Takes a standard Sv2 frame containing a payload of type `T` and serializes it into a byte
    /// stream. The resulting serialized bytes are stored in the internal `buffer`, preparing the
    /// frame for transmission. On success, the method returns a reference to the serialized bytes
    /// stored in the internal buffer. Otherwise, errors on a serialization failure.
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
