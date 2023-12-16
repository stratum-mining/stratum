#[cfg(feature = "noise_sv2")]
use binary_sv2::Deserialize;
#[cfg(feature = "noise_sv2")]
use binary_sv2::GetSize;
use binary_sv2::Serialize;
use buffer_sv2::AeadBuffer;
use const_sv2::{SV2_FRAME_CHUNK_SIZE, SV2_FRAME_HEADER_SIZE};
use core::marker::PhantomData;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing2::HandShakeFrame;
#[cfg(feature = "noise_sv2")]
use framing_sv2::header::NoiseHeader;
use framing_sv2::{
    framing2::{EitherFrame, Frame as F_, Sv2Frame},
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

#[cfg(feature = "noise_sv2")]
pub type StandardNoiseDecoder<T> = WithNoise<Buffer, T>;
pub type StandardEitherFrame<T> = EitherFrame<T, <Buffer as IsBuffer>::Slice>;
pub type StandardSv2Frame<T> = Sv2Frame<T, <Buffer as IsBuffer>::Slice>;
pub type StandardDecoder<T> = WithoutNoise<Buffer, T>;

#[cfg(feature = "noise_sv2")]
pub struct WithNoise<B: IsBuffer, T: Serialize + binary_sv2::GetSize> {
    frame: PhantomData<T>,
    missing_noise_b: usize,
    noise_buffer: B,
    sv2_buffer: B,
}

#[cfg(feature = "noise_sv2")]
impl<'a, T: Serialize + GetSize + Deserialize<'a>, B: IsBuffer + AeadBuffer> WithNoise<B, T> {
    #[inline]
    pub fn next_frame(&mut self, state: &mut State) -> Result<EitherFrame<T, B::Slice>> {
        match state {
            State::HandShake(_) => unreachable!(),
            State::NotInitialized(msg_len) => {
                let hint = *msg_len - self.noise_buffer.as_ref().len();
                match hint {
                    0 => {
                        self.missing_noise_b = NoiseHeader::HEADER_SIZE;
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
                    if src.len() < NoiseHeader::SIZE {
                        NoiseHeader::SIZE - src.len()
                    } else {
                        0
                    }
                } else {
                    let src = self.sv2_buffer.get_data_by_ref_(SV2_FRAME_HEADER_SIZE);
                    let header = Header::from_bytes(src)?;
                    header.encrypted_len() - IsBuffer::len(&self.noise_buffer)
                };

                match hint {
                    0 => {
                        self.missing_noise_b = NoiseHeader::SIZE;
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

    #[inline]
    fn decode_noise_frame(
        &mut self,
        noise_codec: &mut NoiseCodec,
    ) -> Result<EitherFrame<T, B::Slice>> {
        match (
            IsBuffer::len(&self.noise_buffer),
            IsBuffer::len(&self.sv2_buffer),
        ) {
            // HERE THE SV2 HEADER IS READY TO BE DECRYPTED
            (NoiseHeader::SIZE, 0) => {
                let src = self.noise_buffer.get_data_owned();
                let decrypted_header = self.sv2_buffer.get_writable(NoiseHeader::SIZE);
                decrypted_header.copy_from_slice(src.as_ref());
                self.sv2_buffer.as_ref();
                noise_codec.decrypt(&mut self.sv2_buffer)?;
                let header =
                    Header::from_bytes(self.sv2_buffer.get_data_by_ref_(SV2_FRAME_HEADER_SIZE))?;
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
                    noise_codec.decrypt(&mut self.sv2_buffer).unwrap();
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

    fn while_handshaking(&mut self) -> EitherFrame<T, B::Slice> {
        let src = self.noise_buffer.get_data_owned().as_mut().to_vec();

        // below is inffalible as noise frame length has been already checked
        let frame = HandShakeFrame::from_bytes_unchecked(src.into());

        frame.into()
    }

    #[inline]
    pub fn writable(&mut self) -> &mut [u8] {
        self.noise_buffer.get_writable(self.missing_noise_b)
    }
}

#[cfg(feature = "noise_sv2")]
impl<T: Serialize + binary_sv2::GetSize> WithNoise<Buffer, T> {
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

#[derive(Debug)]
pub struct WithoutNoise<B: IsBuffer, T: Serialize + binary_sv2::GetSize> {
    frame: PhantomData<T>,
    missing_b: usize,
    buffer: B,
}

impl<T: Serialize + binary_sv2::GetSize, B: IsBuffer> WithoutNoise<B, T> {
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

    pub fn writable(&mut self) -> &mut [u8] {
        self.buffer.get_writable(self.missing_b)
    }
}

impl<T: Serialize + binary_sv2::GetSize> WithoutNoise<Buffer, T> {
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
