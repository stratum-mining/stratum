//! # `buffer_sv2`
//!
//! Handles memory management for Stratum V2 (Sv2) roles.
//!
//! Provides a memory-efficient buffer pool ([`BufferPool`]) that minimizes allocations and
//! deallocations for high-throughput message frame processing in Sv2 roles. [`Slice`] helps
//! minimize memory allocation overhead by reusing large buffers, improving performance and
//! reducing latency. The [`BufferPool`] tracks the usage of memory slices, using atomic operations
//! and shared state tracking to safely manage memory across multiple threads.
//!
//! ## Memory Structure
//!
//! The [`BufferPool`] manages a contiguous block of memory allocated on the heap, divided into
//! fixed-size slots. Memory allocation within this pool operates in three distinct modes:
//!
//! 1. **Back Mode**: By default, memory is allocated sequentially from the back (end) of the buffer
//!    pool. This mode continues until the back slots are fully occupied.
//! 2. **Front Mode**: Once the back slots are exhausted, the [`BufferPool`] checks if any slots at
//!    the front (beginning) have been freed. If available, it switches to front mode, allocating
//!    memory from the front slots.
//! 3. **Alloc Mode**: If both back and front slots are occupied, the [`BufferPool`] enters alloc
//!    mode, where it allocates additional memory directly from the system heap. This mode may
//!    introduce performance overhead due to dynamic memory allocation.
//!
//! [`BufferPool`] dynamically transitions between these modes based on slot availability,
//! optimizing memory usage and performance.
//!
//! ## Usage
//!
//! When an incoming Sv2 message is received, it is buffered for processing. The [`BufferPool`]
//! attempts to allocate memory from its internal slots, starting in back mode. If the back slots
//! are full, it checks for available front slots to switch to front mode. If no internal slots are
//! free, it resorts to alloc mode, allocating memory from the system heap.
//!
//! For operations requiring dedicated buffers, the [`Slice`] type manages its own memory using
//! [`Vec<u8>`]. In high-performance scenarios, [`Slice`] can reference externally managed memory
//! from the [`BufferPool`], reducing dynamic memory allocations and increasing performance.
//!
//! ### Debug Mode
//! Provides additional tracking for debugging memory management issues.

#![cfg_attr(not(feature = "debug"), no_std)]
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

/// Represents errors that can occur while writing data into a buffer.
pub enum WriteError {
    /// No data could be written.
    WriteZero,
}

/// Interface for writing data into a buffer.
///
/// An abstraction over different buffer types ([`Vec<u8>`] or [`BufferPool`]), it provides methods
/// for writing data from a byte slice into the buffer, with the option to either write a portion
/// of the data or attempt to write the entire byte slice at once.
pub trait Write {
    /// Writes data from a byte slice (`buf`) into the buffer, returning the number of bytes that
    /// were successfully written.
    fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError>;

    /// Attempts to write the entire byte slice (`buf`) into the buffer. If the buffer cannot
    /// accept the full length of the data, an error is returned.
    fn write_all(&mut self, buf: &[u8]) -> Result<(), WriteError>;
}

impl Write for Vec<u8> {
    /// Writes data from a byte slice into a [`Vec<u8>`] buffer by extending the vector with the
    /// contents of the provided slice.
    #[inline]
    fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }

    /// Attempts to write all the data from a byte slice into a [`Vec<u8>`] buffer by extending the
    /// vector. Since [`Vec<u8>`] can dynamically resize, this method will always succeed as long
    /// as there is available memory.
    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> Result<(), WriteError> {
        self.extend_from_slice(buf);
        Ok(())
    }
}

impl Write for &mut [u8] {
    /// Writes data from a byte slice into a mutable byte array (`&mut [u8]`), up to the length of
    /// the provided buffer.
    #[inline]
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        let amt = core::cmp::min(data.len(), self.len());
        let res = core::mem::take(self);
        let (a, b) = res.split_at_mut(amt);
        a.copy_from_slice(&data[..amt]);
        *self = b;
        Ok(amt)
    }

    /// Attempts to write all the data from a byte slice into a mutable byte array (`&mut [u8]`).
    /// If the buffer is not large enough to contain all the data, an error is returned.
    #[inline]
    fn write_all(&mut self, data: &[u8]) -> Result<(), WriteError> {
        if self.write(data)? == data.len() {
            Ok(())
        } else {
            Err(WriteError::WriteZero)
        }
    }
}

/// Interface for working with memory buffers.
///
/// An abstraction for buffer management, allowing implementors to handle either owned memory
/// ([`Slice`] with [`Vec<u8>`]). Utilities are provided to borrow writable memory, retrieve data
/// from the buffer, and manage memory slices.
///
/// This trait is used during the serialization and deserialization
/// of message types in the [`binary_sv2` crate](https://crates.io/crates/binary_sv2).
pub trait Buffer {
    /// The type of slice that the buffer uses.
    type Slice: AsMut<[u8]> + AsRef<[u8]> + Into<Slice>;

    /// Borrows a mutable slice of the buffer, allowing the caller to write data into it. The
    /// caller specifies the length of the data they need to write.
    fn get_writable(&mut self, len: usize) -> &mut [u8];

    /// Provides ownership of a slice in the buffer pool to the caller and updates the buffer
    /// pool's state by modifying the position in `shared_state` that the slice occupies. The pool
    /// now points to the next set of uninitialized space.
    fn get_data_owned(&mut self) -> Self::Slice;

    /// Provides a mutable reference to the written portion of the buffer, up to the specified
    /// length, without transferring ownership of the buffer. This allows the caller to modify the
    /// buffer’s contents directly without taking ownership.
    fn get_data_by_ref(&mut self, len: usize) -> &mut [u8];

    /// Provides an immutable reference to the written portion of the buffer, up to the specified
    /// length, without transferring ownership of the buffer. This allows the caller to inspect the
    /// buffer’s contents without modifying or taking ownership.
    fn get_data_by_ref_(&self, len: usize) -> &[u8];

    /// Returns the size of the written portion of the buffer. This is useful for tracking how much
    /// of the buffer has been filled with data. The number of bytes currently written in the
    /// buffer is returned.
    fn len(&self) -> usize;

    /// Modifies the starting point of the buffer, effectively discarding data up to the given
    /// `index`. This can be useful for performance optimizations in situations where older data
    /// is no longer needed, but its use can be unsafe unless you understand its implications.
    fn danger_set_start(&mut self, index: usize);

    /// Returns `true` if the buffer is empty, `false` otherwise.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Determines if the buffer is safe to drop. This typically checks if the buffer contains
    /// essential data that still needs to be processed.
    ///
    /// Returns `true` if the buffer can be safely dropped, `false` otherwise.
    fn is_droppable(&self) -> bool;
}
