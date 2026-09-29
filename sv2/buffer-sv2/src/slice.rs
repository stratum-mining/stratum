// # Slice
//
// Provides efficient memory management for the Sv2 protocol by allowing memory reuse, either
// through owned memory (`Vec<u8>`) or externally managed memory in a buffer pool (`BufferPool`).
//
// `Slice` helps minimize memory allocation overhead by reusing large buffers, improving
// performance and reducing latency in high-throughput environments. Tracks the usage of memory
// slices, ensuring safe reuse across multiple threads via `SharedState`.
//
// ## Key Features
// - **Memory Reuse**: Divides large buffers into smaller slices, reducing the need for frequent
//   allocations.
// - **Shared Access**: Allows safe concurrent access using atomic state tracking (`Arc<AtomicU8>`).
// - **Flexible Management**: Supports both owned memory and externally managed memory.
//
// ## Usage
// 1. **Owned Memory**: For isolated operations, `Slice` manages its own memory (`Vec<u8>`).
// 2. **Buffer Pool**: In high-performance systems, `Slice` references externally managed memory
//    from a buffer pool (`BufferPool`), reducing dynamic memory allocation.
//
// ### Debug Mode
// Provides additional tracking for debugging memory management issues.

use alloc::{sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU8, Ordering};
#[cfg(feature = "debug")]
use std::time::SystemTime;

/// Allows [`Slice`] to be safely transferred between threads.
///
/// [`Slice`] contains a raw pointer (`*mut u8`), so Rust cannot automatically implement [`Send`].
/// The `unsafe` block asserts that memory access is thread-safe, relaying on `SharedState` and
/// atomic operations to prevent data races.
unsafe impl Send for Slice {}

/// A contiguous block of memory, either preallocated or dynamically allocated.
///
/// It serves as a lightweight handle to a memory buffer, allowing for direct manipulation and
/// shared access. It can either hold a reference to a preallocated memory block or own a
/// dynamically allocated buffer (via [`Vec<u8>`]).
#[derive(Debug)]
pub struct Slice {
    // Where the bytes live: a region of a buffer pool, or memory the slice owns.
    repr: Repr,

    // Mode flag to track the state of the slice during development.
    //
    // Useful for identifying whether the slice is being used correctly in different modes (e.g.,
    // whether is is currently being written to or read from). Typically used for logging and
    // debugging.
    #[cfg(feature = "debug")]
    mode: u8,

    // Timestamp to track when the slice was created.
    //
    // Useful for diagnosing time-related issues and tracking the lifespan of memory slices during
    // development and debugging.
    #[cfg(feature = "debug")]
    #[allow(dead_code)]
    time: SystemTime,
}

#[derive(Debug)]
enum Repr {
    // A region of a buffer pool's memory, whose slot stays claimed until the slice is dropped.
    //
    // `ptr` and `len` bound the region; `slot` is the bit that tracks it in `state`.
    Pooled {
        state: SharedState,
        ptr: *mut u8,
        len: usize,
        slot: u8,
    },

    // Memory the slice owns, used when the pool falls back to system memory.
    Heap(Vec<u8>),
}

impl Slice {
    // Hands out the `len` bytes at `ptr` in a buffer pool's memory, claiming `slot` in `state`
    // until the slice is dropped.
    pub(crate) fn pooled(
        state: SharedState,
        ptr: *mut u8,
        len: usize,
        slot: u8,
        #[cfg(feature = "debug")] mode: u8,
    ) -> Self {
        state.claim(
            slot,
            #[cfg(feature = "debug")]
            mode,
        );
        Slice {
            repr: Repr::Pooled {
                state,
                ptr,
                len,
                slot,
            },
            #[cfg(feature = "debug")]
            mode,
            #[cfg(feature = "debug")]
            time: SystemTime::now(),
        }
    }

    /// Returns the length of the slice in bytes.
    pub fn len(&self) -> usize {
        match &self.repr {
            Repr::Pooled { len, .. } => *len,
            Repr::Heap(owned) => owned.len(),
        }
    }

    /// Checks if the slice is empty.
    ///
    /// Returns `true` if the slice is empty, i.e., it has no data. Otherwise, returns `false`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl core::ops::Index<usize> for Slice {
    type Output = u8;

    /// Provides immutable indexing access to the [`Slice`] at the specified position.
    ///
    /// Uses `as_ref` to get a reference to the underlying buffer and returns the byte at the
    /// `index`.
    fn index(&self, index: usize) -> &Self::Output {
        self.as_ref().index(index)
    }
}

impl core::ops::IndexMut<usize> for Slice {
    /// Provides mutable indexing access to the [`Slice`] at the specified position.
    ///
    /// Uses `as_mut` to get a mutable reference to the underlying buffer and returns the byte at
    /// the `index`.
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.as_mut().index_mut(index)
    }
}

impl core::ops::Index<core::ops::RangeFrom<usize>> for Slice {
    type Output = [u8];

    /// Provides immutable slicing access to a range starting from the given `index`.
    ///
    /// Uses `as_ref` to get a reference to the underlying buffer and returns the range.
    fn index(&self, index: core::ops::RangeFrom<usize>) -> &Self::Output {
        self.as_ref().index(index)
    }
}

impl core::ops::IndexMut<core::ops::RangeFrom<usize>> for Slice {
    /// Provides mutable slicing access to a range starting from the given `index`.
    ///
    /// Uses `as_mut` to get a mutable reference to the underlying buffer and returns the range.
    fn index_mut(&mut self, index: core::ops::RangeFrom<usize>) -> &mut Self::Output {
        self.as_mut().index_mut(index)
    }
}

impl core::ops::Index<core::ops::Range<usize>> for Slice {
    type Output = [u8];

    /// Provides immutable slicing access to the specified range within the `Slice`.
    ///
    /// Uses `as_ref` to get a reference to the underlying buffer and returns the specified range.
    fn index(&self, index: core::ops::Range<usize>) -> &Self::Output {
        self.as_ref().index(index)
    }
}

impl core::ops::IndexMut<core::ops::Range<usize>> for Slice {
    /// Provides mutable slicing access to the specified range within the `Slice`.
    ///
    /// Uses `as_mut` to get a mutable reference to the underlying buffer and returns the specified
    /// range.
    fn index_mut(&mut self, index: core::ops::Range<usize>) -> &mut Self::Output {
        self.as_mut().index_mut(index)
    }
}

impl core::ops::Index<core::ops::RangeFull> for Slice {
    type Output = [u8];

    /// Provides immutable access to the entire range of the [`Slice`].
    ///
    /// Uses `as_ref` to get a reference to the entire underlying buffer.
    fn index(&self, index: core::ops::RangeFull) -> &Self::Output {
        self.as_ref().index(index)
    }
}

impl AsMut<[u8]> for Slice {
    /// Converts the [`Slice`] into a mutable slice of bytes (`&mut [u8]`).
    ///
    /// Returns the owned buffer, or converts the pool region's pointer and length into a mutable
    /// slice.
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        match &mut self.repr {
            Repr::Pooled { ptr, len, .. } => unsafe { core::slice::from_raw_parts_mut(*ptr, *len) },
            Repr::Heap(owned) => owned,
        }
    }
}

impl AsRef<[u8]> for Slice {
    /// Converts the [`Slice`] into an immutable slice of bytes (`&[u8]`).
    ///
    /// Returns the owned buffer, or converts the pool region's pointer and length into an
    /// immutable slice.
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        match &self.repr {
            Repr::Pooled { ptr, len, .. } => unsafe { core::slice::from_raw_parts(*ptr, *len) },
            Repr::Heap(owned) => owned,
        }
    }
}

impl Clone for Slice {
    /// Copies the bytes into a new [`Slice`] that owns its memory, independent of the pool.
    fn clone(&self) -> Self {
        Slice::from(self.as_ref().to_vec())
    }
}

impl Drop for Slice {
    /// Releases the slice's slot in the shared state, allowing the memory to be reused.
    ///
    /// In debug mode, it also tracks the `mode` of the slice when it is dropped.
    fn drop(&mut self) {
        if let Repr::Pooled { state, slot, .. } = &self.repr {
            state.release(
                *slot,
                #[cfg(feature = "debug")]
                self.mode,
            );
        }
    }
}

impl From<Vec<u8>> for Slice {
    /// Creates a [`Slice`] from a [`Vec<u8>`], taking ownership of the vector.
    ///
    /// The slice owns the vector's memory, so it takes no slot in any buffer pool.
    fn from(v: Vec<u8>) -> Self {
        Slice {
            repr: Repr::Heap(v),
            #[cfg(feature = "debug")]
            mode: 2,
            #[cfg(feature = "debug")]
            time: SystemTime::now(),
        }
    }
}

// The shared state of the buffer pool.
//
// Encapsulates an atomic 8-bit value (`AtomicU8`) to track the shared state of memory slices in a
// thread-safe manner. It uses atomic operations to ensure that memory tracking can be done
// concurrently without locks.
//
// Each bit in the `AtomicU8` represents the state of a memory slot (e.g., whether it is allocated
// or free) in the buffer pool, allowing the system to manage and synchronize memory usage across
// multiple slices.
//
// `SharedState` acts like a reference counter, helping the buffer pool know when a buffer slice is
// safe to clear. The corresponding bit in the shared state is set when a memory slice is handed
// out and cleared when it is dropped. When no slices are in use (all bits are zero), the buffer
// pool can safely reclaim or reuse the memory.
//
// This system ensures that no memory is prematurely cleared while it is still being referenced.
// The buffer pool checks whether any slice is still in use before clearing, and only when the
// shared state indicates that all references have been dropped (i.e., no unprocessed messages
// remain) can the buffer pool safely clear or reuse the memory.
#[derive(Clone, Debug)]
pub(crate) struct SharedState(Arc<AtomicU8>);

impl SharedState {
    // Creates a new `SharedState` with an internal `AtomicU8` initialized to `0`, indicating no
    // memory slots are in use.
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicU8::new(0)))
    }

    // Atomically loads and returns the current state of the memory slots as an 8-bit value.
    //
    // Acquires what dropped slices released, so the memory they freed is safe to reuse.
    #[inline(always)]
    pub(crate) fn load(&self) -> u8 {
        self.0.load(Ordering::Acquire)
    }

    // Returns the bit that tracks slot `position`.
    //
    // Panics if the `position` is outside the range of 1-8, as this refers to an invalid bit.
    #[inline(always)]
    fn mask(position: u8) -> u8 {
        match position {
            1..=8 => 0b1000_0000 >> (position - 1),
            _ => panic!("{}", position),
        }
    }

    // Marks slot `position` as taken by the slice the pool is handing out.
    //
    // Only the pool sets bits, on the thread that hands the slice out, so no ordering is needed.
    #[inline(always)]
    pub(crate) fn claim(&self, position: u8, #[cfg(feature = "debug")] mode: u8) {
        let mask = Self::mask(position);
        let pre = self.0.fetch_or(mask, Ordering::Relaxed);
        assert_eq!(pre & mask, 0, "slot {position} is held by a live slice");

        #[cfg(feature = "debug")]
        println!("CLAIM:: {} {:b} {:b}", mode, pre, pre | mask);
    }

    // Marks slot `position` as free once the slice holding it is dropped.
    //
    // The release ordering publishes the slice's accesses before the pool reuses that memory.
    #[inline(always)]
    pub(crate) fn release(&self, position: u8, #[cfg(feature = "debug")] mode: u8) {
        let mask = Self::mask(position);
        let pre = self.0.fetch_and(!mask, Ordering::Release);
        debug_assert_ne!(pre & mask, 0);

        #[cfg(feature = "debug")]
        println!("RELEASE:: {} {:b} {:b}", mode, pre, pre & !mask);
    }
}
