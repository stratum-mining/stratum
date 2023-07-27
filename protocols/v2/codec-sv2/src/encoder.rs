use alloc::vec::Vec;
use binary_sv2::{GetSize, Serialize};
#[cfg(feature = "noise_sv2")]
use core::convert::TryInto;
use core::marker::PhantomData;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing2::{build_noise_frame_header, EitherFrame, HandShakeFrame};
use framing_sv2::framing2::{Frame as F_, Sv2Frame};
#[cfg(feature = "noise_sv2")]
use noise_sv2::NoiseCodec;
#[cfg(feature = "noise_sv2")]
use tracing::error;

#[cfg(feature = "noise_sv2")]
use crate::{Error, Result, State};

#[cfg(feature = "noise_sv2")]
const MAC_LEN: usize = const_sv2::AEAD_MAC_LEN;

#[cfg(feature = "noise_sv2")]
#[cfg(not(feature = "with_buffer_pool"))]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory as Buffer};

#[cfg(feature = "noise_sv2")]
#[cfg(feature = "with_buffer_pool")]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory, BufferPool};

#[cfg(feature = "noise_sv2")]
#[cfg(feature = "with_buffer_pool")]
type Buffer = BufferPool<BufferFromSystemMemory>;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

#[cfg(feature = "noise_sv2")]
pub struct NoiseEncoder<T: Serialize + binary_sv2::GetSize> {
    noise_buffer: Buffer,
    sv2_buffer: Buffer,
    frame: PhantomData<T>,
}

#[cfg(feature = "noise_sv2")]
type Item<T> = EitherFrame<T, Slice>;

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + GetSize> NoiseEncoder<T> {
    #[inline]
    pub fn encode(&mut self, item: Item<T>, state: &mut State) -> Result<Slice> {
        match state {
            State::Transport(transport_mode) => {
                let len = item.encoded_length();
                let writable = self.sv2_buffer.get_writable(len);

                // ENCODE THE SV2 FRAME
                let i: Sv2Frame<T, Slice> = item.try_into().map_err(|e| {
                    error!("Error while encoding 1 frame: {:?}", e);
                    Error::FramingError(e)
                })?;
                i.serialize(writable)?;
                self.encode_single_frame(transport_mode)?;
            }
            State::HandShake(_) => self.while_handshaking(item)?,
            State::NotInitialized => self.while_handshaking(item)?,
        };

        // Clear sv2_buffer
        self.sv2_buffer.get_data_owned();
        // Return noise_buffer
        Ok(self.noise_buffer.get_data_owned())
    }

    /// Encode a single noise message frame.
    #[inline(always)]
    fn encode_single_frame(&mut self, noise_codec: &mut NoiseCodec) -> Result<()> {
        // Reserve enough space to encode the noise message
        //let len = TransportMode::size_hint_encrypt(self.sv2_buffer.len());
        let len = self.sv2_buffer.len() + MAC_LEN;

        // Prepend the noise frame header
        build_noise_frame_header(self.noise_buffer.get_writable(2), len as u16);

        // Encrypt the SV2 frame and encode the noise frame
        noise_codec.encrypt(&mut self.sv2_buffer)?;
        let noise_frame = self.noise_buffer.get_writable(self.sv2_buffer.len());
        let encrypted = self.sv2_buffer.get_data_owned();
        for i in 0..len {
            noise_frame[i] = encrypted[i];
        }

        Ok(())
    }

    #[inline(never)]
    fn while_handshaking(&mut self, item: Item<T>) -> Result<()> {
        let len = item.encoded_length();
        // ENCODE THE SV2 FRAME
        let i: HandShakeFrame = item.try_into().map_err(|e| {
            error!("Error while encoding 2 frame - while_handshaking: {:?}", e);
            Error::FramingError(e)
        })?;
        i.serialize(self.noise_buffer.get_writable(len))?;

        Ok(())
    }
}

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + binary_sv2::GetSize> NoiseEncoder<T> {
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

#[derive(Debug)]
pub struct Encoder<T> {
    buffer: Vec<u8>,
    frame: PhantomData<T>,
}

impl<T: Serialize + GetSize> Encoder<T> {
    pub fn encode(
        &mut self,
        item: Sv2Frame<T, Slice>,
    ) -> core::result::Result<&[u8], crate::Error> {
        let len = item.encoded_length();

        self.buffer.resize(len, 0);

        item.serialize(&mut self.buffer)?;

        Ok(&self.buffer[..])
    }

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
