use alloc::vec::Vec;
use binary_sv2::{GetSize, Serialize};
#[cfg(feature = "noise_sv2")]
use core::cmp::min;
#[cfg(feature = "noise_sv2")]
use core::convert::TryInto;
use core::marker::PhantomData;
#[cfg(feature = "noise_sv2")]
use framing_sv2::framing2::{build_noise_frame_header, EitherFrame, HandShakeFrame};
use framing_sv2::framing2::{Frame as F_, Sv2Frame};

#[cfg(feature = "noise_sv2")]
use crate::{Error, State, TransportMode};

#[cfg(feature = "noise_sv2")]
const TAGLEN: usize = const_sv2::SNOW_TAGLEN;
#[cfg(feature = "noise_sv2")]
const MAX_M_L: usize = const_sv2::NOISE_FRAME_MAX_SIZE;
#[cfg(feature = "noise_sv2")]
const M: usize = MAX_M_L - TAGLEN;

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
    pub fn encode(&mut self, item: Item<T>, state: &mut State) -> Result<Slice, Error> {
        match state {
            State::Transport(transport_mode) => {
                let len = item.encoded_length();
                let writable = self.sv2_buffer.get_writable(len);

                // ENCODE THE SV2 FRAME
                let i: Sv2Frame<T, Slice> = item.try_into().map_err(|_| Error::CodecTodo)?;
                i.serialize(writable)?;

                // IF THE MESSAGE FIT INTO A NOISE FRAME ENCODE IT HOT PATH
                if len <= M {
                    self.encode_single_frame(transport_mode)
                        .map_err(|_| Error::CodecTodo)?;

                // IF LEN IS BIGGER THAN NOISE PAYLOAD MAX SIZE MESSAGE IS ENCODED AS SEVERAL NOISE
                // MESSAGES COLD PATH
                } else {
                    self.encode_multiple_frame(transport_mode)
                        .map_err(|_| Error::CodecTodo)?;
                }
            }
            State::HandShake(_) => self.while_handshaking(item).map_err(|_| Error::CodecTodo)?,
            State::NotInitialized => self.while_handshaking(item).map_err(|_| Error::CodecTodo)?,
        };

        // Clear sv2_buffer
        self.sv2_buffer.get_data_owned();
        // Return noise_buffer
        Ok(self.noise_buffer.get_data_owned())
    }

    #[inline(always)]
    fn encode_single_frame(&mut self, transport_mode: &mut TransportMode) -> Result<(), ()> {
        // RESERVE ENAUGH SPACE TO ENCODE THE NOISE MESSAGE
        let len = TransportMode::size_hint_encrypt(self.sv2_buffer.len());

        // PREPEND THE NOISE FRAME HEADER
        build_noise_frame_header(self.noise_buffer.get_writable(2), len as u16);

        // ENCRYPT THE SV2 FRAME AND ENCODE THE NOISE FRAME
        transport_mode
            .write(
                self.sv2_buffer.get_data_by_ref(self.sv2_buffer.len()),
                self.noise_buffer.get_writable(len),
            )
            .map_err(|_| ())
    }

    #[inline(never)]
    fn encode_multiple_frame(&mut self, transport_mode: &mut TransportMode) -> Result<(), ()> {
        let buffer_len: usize = self.sv2_buffer.len();
        let mut start: usize = 0;
        let mut end: usize = M;

        loop {
            end = min(end, buffer_len);

            let buf = &self.sv2_buffer.get_data_by_ref(self.sv2_buffer.len())[start..end];

            // PREPEND THE NOISE FRAME HEADER
            let len = TransportMode::size_hint_encrypt(buf.len());
            build_noise_frame_header(self.noise_buffer.get_writable(2), len as u16);

            // ENCRYPT THE SV2 FRAGMENT
            transport_mode
                .write(buf, self.noise_buffer.get_writable(len))
                .map_err(|_| ())?;

            if end == buffer_len {
                break;
            }

            start += end;
            end += end;
        }
        Ok(())
    }

    #[inline(never)]
    fn while_handshaking(&mut self, item: Item<T>) -> Result<(), ()> {
        let len = item.encoded_length();
        // ENCODE THE SV2 FRAME
        let i: HandShakeFrame = item.try_into().map_err(|_| ())?;
        i.serialize(self.noise_buffer.get_writable(len))
            .map_err(|_| ())?;

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
    pub fn encode(&mut self, item: Sv2Frame<T, Slice>) -> Result<&[u8], crate::Error> {
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
