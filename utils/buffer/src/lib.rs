//#![cfg_attr(not(feature = "debug"), no_std)]
//#![feature(backtrace)]

mod buffer;
mod buffer_pool;
mod slice;
#[cfg(test)]
mod test;

extern crate alloc;
use alloc::vec::Vec;

pub use crate::buffer::BufferFromSystemMemory;
pub use aes_gcm::aead::Buffer as AeadBuffer;
pub use buffer_pool::BufferPool;
pub use slice::Slice;

pub enum WriteError {
    WriteZero,
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError>;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), WriteError>;
}

impl Write for Vec<u8> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> Result<(), WriteError> {
        self.extend_from_slice(buf);
        Ok(())
    }
}

impl Write for &mut [u8] {
    #[inline]
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        let amt = core::cmp::min(data.len(), self.len());
        let res = core::mem::take(self);
        let (a, b) = res.split_at_mut(amt);
        a.copy_from_slice(&data[..amt]);
        *self = b;
        Ok(amt)
    }

    #[inline]
    fn write_all(&mut self, data: &[u8]) -> Result<(), WriteError> {
        if self.write(data)? == data.len() {
            Ok(())
        } else {
            Err(WriteError::WriteZero)
        }
    }
}

pub trait Buffer {
    type Slice: AsMut<[u8]> + AsRef<[u8]> + Into<Slice>;

    // Caller need to borrow a buffer to write some date
    fn get_writable(&mut self, len: usize) -> &mut [u8];

    // Caller need to get the previously written buffer and should own it
    fn get_data_owned(&mut self) -> Self::Slice;

    // Caller need a view in the written part of the buffer
    fn get_data_by_ref(&mut self, len: usize) -> &mut [u8];

    // Caller need a view in the written part of the buffer
    fn get_data_by_ref_(&self, len: usize) -> &[u8];

    // Return the size of the written part of the buffer that is still owned by the Buffer
    fn len(&self) -> usize;

    // Set the first element of the buffer to the element at the given index (here only for
    // perfomnce do not use unless you are really sure about what it do)
    fn danger_set_start(&mut self, index: usize);

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
