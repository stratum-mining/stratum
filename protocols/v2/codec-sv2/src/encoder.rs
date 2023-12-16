use alloc::vec::Vec;
use binary_sv2::{GetSize, Serialize};
use const_sv2::AEAD_MAC_LEN;
#[cfg(feature = "noise_sv2")]
use const_sv2::{SV2_FRAME_CHUNK_SIZE, SV2_FRAME_HEADER_SIZE};
#[cfg(feature = "noise_sv2")]
use core::convert::TryInto;
use core::marker::PhantomData;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing2::{EitherFrame, HandShakeFrame};
use framing_sv2::{
    framing2::{Frame as F_, Sv2Frame},
    header::NoiseHeader,
};
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
                let mut encrypted_len = NoiseHeader::SIZE;

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
