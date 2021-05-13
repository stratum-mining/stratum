#![no_std]

extern crate alloc;
use alloc::vec::Vec;

pub enum WriteError {
    WriteZero
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
        let (a, b) = core::mem::replace(self, &mut []).split_at_mut(amt);
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
