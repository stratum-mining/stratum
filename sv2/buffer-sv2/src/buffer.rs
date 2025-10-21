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
use aes_gcm::aead::Buffer as AeadBuffer;
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

    // Starting index for the buffer. Useful for scenarios where part of the buffer is skipped or
    // invalid.
    start: usize,
}

impl BufferFromSystemMemory {
    /// Creates a new buffer with no initial data.
    pub fn new(_: usize) -> Self {
        Self {
            inner: Vec::new(),
            cursor: 0,
            start: 0,
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
    // writing.
    #[inline]
    fn get_writable(&mut self, len: usize) -> &mut [u8] {
        let cursor = self.cursor;

        // Reserve space in the buffer for writing based on the requested `len`
        let len = self.cursor + len;

        // If the internal buffer is not large enough to hold the new data, resize it
        if len > self.inner.len() {
            self.inner.resize(len, 0)
        };

        self.cursor = len;

        // Portion of the buffer where data can be written
        &mut self.inner[cursor..len]
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
        head
    }

    // Returns a mutable reference to the written portion of the internal buffer that has been
    // filled up with data, up to the specified length (`len`).
    #[inline]
    fn get_data_by_ref(&mut self, len: usize) -> &mut [u8] {
        &mut self.inner[..usize::min(len, self.cursor)]
    }

    // Returns an immutable reference to the written portion of the internal buffer that has been
    // filled up with data, up to the specified length (`len`).
    #[inline]
    fn get_data_by_ref_(&self, len: usize) -> &[u8] {
        &self.inner[..usize::min(len, self.cursor)]
    }

    // Returns the current write position (cursor) in the buffer, representing how much of the
    // internal buffer has been filled with data.
    #[inline]
    fn len(&self) -> usize {
        self.cursor
    }

    // Sets the start index for the buffer, adjusting where reads and writes begin. Used to discard
    // part of the buffer by adjusting the starting point for future operations.
    #[inline]
    fn danger_set_start(&mut self, index: usize) {
        self.start = index;
    }

    // Indicates that the buffer is always safe to drop, as `Vec<u8>` manages memory internally.
    #[inline]
    fn is_droppable(&self) -> bool {
        true
    }
}

// Used to test if `BufferPool` tries to allocate from system memory.
#[cfg(test)]
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

    fn is_droppable(&self) -> bool {
        true
    }
}

impl AsRef<[u8]> for BufferFromSystemMemory {
    /// Returns a reference to the internal buffer as a byte slice, starting from the specified
    /// `start` index. Provides an immutable view into the buffer's contents, allowing it to be
    /// used as a regular slice for reading.
    fn as_ref(&self) -> &[u8] {
        let start = self.start;
        &self.get_data_by_ref_(Buffer::len(self))[start..]
    }
}

impl AsMut<[u8]> for BufferFromSystemMemory {
    /// Returns a mutable reference to the internal buffer as a byte slice, starting from the
    /// specified `start` index. Allows direct modification of the buffer's contents, while
    /// restricting access to the data after the `start` index.
    fn as_mut(&mut self) -> &mut [u8] {
        let start = self.start;
        self.get_data_by_ref(Buffer::len(self))[start..].as_mut()
    }
}

impl AeadBuffer for BufferFromSystemMemory {
    /// Extends the internal buffer by appending the given byte slice. Dynamically resizes the
    /// internal buffer to accommodate the new data and copies the contents of `other` into it.
    fn extend_from_slice(&mut self, other: &[u8]) -> aes_gcm::aead::Result<()> {
        self.get_writable(other.len()).copy_from_slice(other);
        Ok(())
    }

    /// Truncates the internal buffer to the specified length, adjusting for the `start` index.
    /// Resets the buffer cursor to reflect the new size, effectively discarding any data beyond
    /// the truncated length.
    fn truncate(&mut self, len: usize) {
        let len = len + self.start;
        self.cursor = len;
    }
}
