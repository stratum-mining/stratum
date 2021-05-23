use crate::Buffer;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct BufferFromSystemMemory {
    inner: Vec<u8>,
    cursor: usize,
}

impl BufferFromSystemMemory {
    pub fn new() -> Self {
        Self {
            inner: Vec::new(),
            cursor: 0,
        }
    }
}

impl Default for BufferFromSystemMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer for BufferFromSystemMemory {
    type Slice = Vec<u8>;

    #[inline]
    fn get_writable(&mut self, len: usize) -> &mut [u8] {
        let cursor = self.cursor;
        let len = self.cursor + len;

        if len > self.inner.len() {
            self.inner.resize(len, 0)
        };

        self.cursor = len;

        &mut self.inner[cursor..len]
    }

    #[inline]
    fn get_data_owned(&mut self) -> Vec<u8> {
        let mut tail = self.inner.split_off(self.cursor);
        core::mem::swap(&mut tail, &mut self.inner);
        let head = tail;
        self.cursor = 0;
        head
    }

    #[inline]
    fn get_data_by_ref(&mut self, len: usize) -> &mut [u8] {
        &mut self.inner[..usize::min(len, self.cursor)]
    }

    #[inline]
    fn len(&self) -> usize {
        self.cursor
    }
}

#[cfg(test)]
// Used to test if BufferPool try to allocate from system memory
pub struct TestBufferFromMemory(pub Vec<u8>);

#[cfg(test)]
impl Buffer for TestBufferFromMemory {
    type Slice = Vec<u8>;

    fn get_writable(&mut self, _len: usize) -> &mut [u8] {
        panic!()
    }

    fn get_data_owned(&mut self) -> Self::Slice {
        panic!()
    }

    fn get_data_by_ref(&mut self, _len: usize) -> &mut [u8] {
        &mut self.0[0..0]
    }

    fn len(&self) -> usize {
        0
    }
}
