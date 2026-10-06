// # Buffer from System Memory
//
// Provides memory management for encoding and transmitting message frames between Sv2 roles when
// buffer pools have been exhausted.
//
// `BufferFromSystemMemory` serves as a fallback when a `BufferPool` is full or unable to allocate
// memory fast enough. Instead of relying on pre-allocated memory, it dynamically allocates memory
// on the heap using a `Vec<u8>`, ensuring that message frames can still be processed.
//
// This fallback mechanism allows the buffer to resize dynamically based on data needs, making it
// suitable for scenarios where message sizes vary. However, it introduces performance trade-offs
// such as slower allocation, increased memory fragmentation, and higher system overhead compared
// to using pre-allocated buffers.

use crate::Buffer;
use alloc::vec::Vec;

/// Manages a dynamically growing buffer in system memory using an internal [`Vec<u8>`].
///
/// Operates on a dynamically sized buffer and provides utilities for writing, reading, and
/// manipulating data. It tracks the current position where data is written, and resizes the buffer
/// as needed.
#[derive(Debug)]
pub struct BufferFromSystemMemory {
    // Underlying buffer storing the data.
    inner: Vec<u8>,

    // Current cursor indicating where the next byte should be written.
    cursor: usize,

    // Length of the last reservation, `0` once it is committed.
    reserved: usize,
}

impl BufferFromSystemMemory {
    /// Creates a new buffer with no initial data.
    pub fn new(_: usize) -> Self {
        Self {
            inner: Vec::new(),
            cursor: 0,
            reserved: 0,
        }
    }
}

impl Default for BufferFromSystemMemory {
    // Creates a new buffer with no initial data.
    fn default() -> Self {
        Self::new(0)
    }
}

impl Buffer for BufferFromSystemMemory {
    type Slice = Vec<u8>;

    // Dynamically allocates or resizes the internal `Vec<u8>` to ensure there is enough space for
    // writing, without moving the cursor.
    #[inline]
    fn reserve(&mut self, len: usize) -> &mut [u8] {
        let end = self
            .cursor
            .checked_add(len)
            .expect("writable length overflows usize");

        // If the internal buffer is not large enough to hold the new data, resize it
        if end > self.inner.len() {
            self.inner.resize(end, 0)
        };

        self.reserved = len;

        // Portion of the buffer where data can be written
        &mut self.inner[self.cursor..end]
    }

    // Moves the cursor past the first `len` bytes of the last reservation.
    #[inline]
    fn commit(&mut self, len: usize) {
        assert!(len <= self.reserved, "commit exceeds the last reservation");
        self.cursor += len;
        self.reserved = 0;
    }

    // Splits off the written portion of the buffer, returning it as a new `Vec<u8>`. Swaps the
    // internal buffer with a newly allocated empty one, effectively returning ownership of the
    // written data while resetting the internal buffer for future use.
    #[inline]
    fn get_data_owned(&mut self) -> Vec<u8> {
        // Split the internal buffer at the cursor position
        let mut tail = self.inner.split_off(self.cursor);

        // Swap the data after the cursor (tail) with the remaining buffer
        core::mem::swap(&mut tail, &mut self.inner);

        // Move ownership of the buffer content up to the cursor, resetting the internal buffer
        // state for future writes
        let head = tail;
        self.cursor = 0;
        self.reserved = 0;
        head
    }

    // Returns the portion of the internal buffer that has been committed, up to the cursor.
    #[inline]
    fn frame(&self) -> &[u8] {
        &self.inner[..self.cursor]
    }

    // Returns the portion of the internal buffer that has been committed, up to the cursor.
    #[inline]
    fn frame_mut(&mut self) -> &mut [u8] {
        &mut self.inner[..self.cursor]
    }

    // Returns the current write position (cursor) in the buffer, representing how much of the
    // internal buffer has been filled with data.
    #[inline]
    fn len(&self) -> usize {
        self.cursor
    }

    // Moves the cursor back to `len`, if it is past it.
    #[inline]
    fn truncate(&mut self, len: usize) {
        self.cursor = self.cursor.min(len);
        self.reserved = 0;
    }
}

// Used to test if `BufferPool` tries to allocate from system memory.
#[cfg(test)]
pub struct TestBufferFromMemory(pub Vec<u8>);

#[cfg(test)]
impl Buffer for TestBufferFromMemory {
    type Slice = Vec<u8>;

    fn reserve(&mut self, _len: usize) -> &mut [u8] {
        panic!()
    }

    fn commit(&mut self, _len: usize) {
        panic!()
    }

    fn get_data_owned(&mut self) -> Self::Slice {
        panic!()
    }

    fn frame(&self) -> &[u8] {
        &self.0[0..0]
    }

    fn frame_mut(&mut self) -> &mut [u8] {
        &mut self.0[0..0]
    }

    fn len(&self) -> usize {
        0
    }

    fn truncate(&mut self, _len: usize) {
        panic!()
    }
}
