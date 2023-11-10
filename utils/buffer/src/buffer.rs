use crate::Buffer;
use aes_gcm::aead::Buffer as AeadBuffer;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct BufferFromSystemMemory {
    inner: Vec<u8>,
    cursor: usize,
    start: usize,
}

impl BufferFromSystemMemory {
    pub fn new(_: usize) -> Self {
        Self {
            inner: Vec::new(),
            cursor: 0,
            start: 0,
        }
    }
}

impl Default for BufferFromSystemMemory {
    fn default() -> Self {
        Self::new(0)
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
    fn get_data_by_ref_(&self, len: usize) -> &[u8] {
        &self.inner[..usize::min(len, self.cursor)]
    }

    #[inline]
    fn len(&self) -> usize {
        self.cursor
    }

    #[inline]
    fn danger_set_start(&mut self, index: usize) {
        self.start = index;
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
    fn get_data_by_ref_(&self, _len: usize) -> &[u8] {
        &self.0[0..0]
    }

    fn len(&self) -> usize {
        0
    }
    fn danger_set_start(&mut self, _index: usize) {
        todo!()
    }
}

impl AsRef<[u8]> for BufferFromSystemMemory {
    fn as_ref(&self) -> &[u8] {
        let start = self.start;
        &self.get_data_by_ref_(Buffer::len(self))[start..]
    }
}
impl AsMut<[u8]> for BufferFromSystemMemory {
    fn as_mut(&mut self) -> &mut [u8] {
        let start = self.start;
        self.get_data_by_ref(Buffer::len(self))[start..].as_mut()
    }
}
impl AeadBuffer for BufferFromSystemMemory {
    fn extend_from_slice(&mut self, other: &[u8]) -> aes_gcm::aead::Result<()> {
        self.get_writable(other.len()).copy_from_slice(other);
        Ok(())
    }

    fn truncate(&mut self, len: usize) {
        let len = len + self.start;
        self.cursor = len;
    }
}
