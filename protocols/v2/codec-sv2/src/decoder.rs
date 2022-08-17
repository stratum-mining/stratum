#[cfg(feature = "noise_sv2")]
use binary_sv2::Deserialize;
#[cfg(feature = "noise_sv2")]
use binary_sv2::GetSize;
use binary_sv2::Serialize;
use core::marker::PhantomData;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing2::{HandShakeFrame, NoiseFrame};
#[cfg(feature = "noise_sv2")]
use framing_sv2::header::NoiseHeader;
use framing_sv2::{
    framing2::{EitherFrame, Frame as F_, Sv2Frame},
    header::Header,
};

#[cfg(not(feature = "with_buffer_pool"))]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory as Buffer};

#[cfg(feature = "with_buffer_pool")]
use buffer_sv2::{Buffer as IsBuffer, BufferFromSystemMemory, BufferPool};
#[cfg(feature = "with_buffer_pool")]
type Buffer = BufferPool<BufferFromSystemMemory>;

use crate::error::{Error, Result};

#[cfg(feature = "noise_sv2")]
use crate::{State, TransportMode};

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
    sv2_frame_size: usize,
}

#[cfg(feature = "noise_sv2")]
impl<'a, T: Serialize + GetSize + Deserialize<'a>, B: IsBuffer> WithNoise<B, T> {
    #[inline]
    pub fn next_frame(&mut self, state: &mut State) -> Result<EitherFrame<T, B::Slice>> {
        let len = self.noise_buffer.len();
        let src = self.noise_buffer.get_data_by_ref(len);
        let hint = NoiseFrame::size_hint(src) as usize;

        match hint {
            0 => {
                self.missing_noise_b = NoiseHeader::SIZE;
                self.decode_noise_frame(state)
            }
            _ => {
                self.missing_noise_b = hint;
                Err(Error::MissingBytes(hint))
            }
        }
    }

    #[inline]
    fn decode_noise_frame(&mut self, state: &mut State) -> Result<EitherFrame<T, B::Slice>> {
        match state {
            State::Transport(transport_mode) => {
                // STRIP THE HEADER FROM THE FRAME AND GET THE ENCRYPTED PAYLOAD
                // everything here can not fail as the size has been already checked
                #[cfg(feature = "with_buffer_pool")]
                let src = self.noise_buffer.get_data_owned();
                #[cfg(not(feature = "with_buffer_pool"))]
                let src = self.noise_buffer.get_data_owned().as_mut().to_vec();
                let mut noise_frame = NoiseFrame::from_bytes_unchecked(src.into());
                let src = noise_frame.payload();

                // DECRYPT THE ENCRYPTED PAYLOAD
                let len = TransportMode::size_hint_decrypt(src.len()).ok_or(())?;
                let decrypted = self.sv2_buffer.get_writable(len);
                transport_mode.read(src, decrypted)?;

                // IF THE DECODER IS RECEIVING A FRAGMENTED FRAME ADD THE DECRYPTED DATA TO THE
                // PARTIAL FRAME AND CHECK IF READY
                if self.sv2_frame_size > 0 {
                    return Ok(self.handle_fragmented().ok_or(())?);
                };

                let len = self.sv2_buffer.len();
                let src = self.sv2_buffer.get_data_by_ref(len);
                let hint = Sv2Frame::<T, B::Slice>::size_hint(src);

                // IF HINT IS 0 A COMPLETE SV2 FRAME IS AVAIABLE THIS IS THE HOT PATH AS USUALLY
                // THE SIZE OF AN SV2 MESSAGE IS SMALLER THE THE MAX SIZE OF A NOISE FRAME
                if hint == 0 {
                    let src = self.sv2_buffer.get_data_owned();
                    let frame = Sv2Frame::<T, B::Slice>::from_bytes_unchecked(src);
                    return Ok(frame.into());
                }

                // IF HINT IS NOT 0 AND MISSING BYTES IS 0 IT MEANs THAT THE FIRST FRAGMENT OF AN
                // SV2 HAS BEEN RECEIVED
                self.handle_fragmented().ok_or(())?;
                Err(Error::MissingBytes(self.missing_noise_b))
            }
            State::HandShake(_) => Ok(self.while_handshaking()),
            State::NotInitialized => Ok(self.while_handshaking()),
        }
    }

    #[inline(never)]
    fn handle_fragmented(&mut self) -> Option<EitherFrame<T, B::Slice>> {
        // IF IS NOT THE FIRST FRAGMENT CHECK IF A COMPLETE FRAME IS AVAIBLE IF YES RETURN THE
        // FRAME IF NOT SET MISSING NOISE BYTES TO NOISE HEADER SIZE SO THE DECODER CAN START TO
        // DECODE THE NEXT NOISE FRAME
        let len = self.sv2_buffer.len();
        let src = self.sv2_buffer.get_data_by_ref(len);
        let hint = Sv2Frame::<T, B::Slice>::size_hint(src);
        if self.sv2_frame_size != 0 {
            if hint == 0 {
                let src = self.sv2_buffer.get_data_owned();
                let frame = Sv2Frame::<T, B::Slice>::from_bytes_unchecked(src);
                Some(frame.into())
            } else {
                self.missing_noise_b = NoiseHeader::SIZE;
                None
            }

        // IF IS THE FIRST FRAGMETN JUST SET THE MISSING SV2 AND NOISE BYTES
        } else {
            self.sv2_frame_size = hint as usize;
            self.missing_noise_b = NoiseHeader::SIZE;

            None
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
            sv2_frame_size: 0,
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
                Err(Error::MissingBytes(self.missing_b))
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
