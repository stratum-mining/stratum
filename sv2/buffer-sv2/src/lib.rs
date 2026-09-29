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
//! [`Vec<u8>`](alloc::vec::Vec). In high-performance scenarios, [`Slice`] can reference externally managed memory
//! from the [`BufferPool`], reducing dynamic memory allocations and increasing performance.
//!
//! ## Slices are scratch space
//!
//! A [`Slice`] handed out by a [`BufferPool`] holds one of its slots until it is dropped. The pool
//! is meant for frames that are decoded and then dropped: a slice kept alive longer keeps its
//! slot, and once every slot is held, each new frame falls back to system memory. Bytes that have
//! to outlive decoding are copied out: cloning a [`Slice`] copies it into memory the clone owns,
//! and the slot is freed as soon as the original is dropped.
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

pub use crate::buffer::BufferFromSystemMemory;
pub use buffer_pool::BufferPool;
pub use slice::Slice;

/// Interface for working with memory buffers.
///
/// An abstraction for buffer management, allowing implementors to handle either owned memory
/// ([`Slice`] with [`Vec<u8>`](alloc::vec::Vec)). Utilities are provided to borrow writable memory, retrieve data
/// from the buffer, and manage memory slices.
///
/// This trait is used during the serialization and deserialization
/// of message types in the [`binary_sv2` crate](https://crates.io/crates/binary_sv2).
pub trait Buffer {
    /// The type of slice that the buffer uses.
    type Slice: AsMut<[u8]> + AsRef<[u8]> + Into<Slice>;

    /// Makes room for `len` more bytes after the data written so far and returns that space,
    /// without counting any of it as written.
    ///
    /// Only [`Buffer::commit`] counts bytes as written. Reserving again before committing
    /// replaces the reservation, and bytes written into it are not kept.
    fn reserve(&mut self, len: usize) -> &mut [u8];

    /// Counts the first `len` bytes of the last reservation as written, and ends that
    /// reservation.
    ///
    /// Panics if `len` is larger than the last reservation.
    fn commit(&mut self, len: usize);

    /// Provides ownership of a slice in the buffer pool to the caller and updates the buffer
    /// pool's state by modifying the position in `shared_state` that the slice occupies. The pool
    /// now points to the next set of uninitialized space.
    fn get_data_owned(&mut self) -> Self::Slice;

    /// Returns the committed bytes of the frame being written, without transferring ownership of
    /// the buffer.
    fn frame(&self) -> &[u8];

    /// Returns the committed bytes of the frame being written, mutably, without transferring
    /// ownership of the buffer.
    fn frame_mut(&mut self) -> &mut [u8];

    /// Returns the size of the written portion of the buffer. This is useful for tracking how much
    /// of the buffer has been filled with data. The number of bytes currently written in the
    /// buffer is returned.
    fn len(&self) -> usize;

    /// Drops the committed bytes past `len`, like [`Vec::truncate`](alloc::vec::Vec::truncate): a `len` at or past the
    /// committed length changes nothing, so the frame can only shrink.
    fn truncate(&mut self, len: usize);

    /// Returns `true` if the buffer is empty, `false` otherwise.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
