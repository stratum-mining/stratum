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

// A special index value used to mark `Slice` as ignored in certain operations, such as memory pool
// tracking or state management.
//
// It can be used as a sentinel value for slices that should not be processed or tracked, helping
// differentiate valid slices from those that need to be skipped. When a `Slice`'s `index` is set
// to `INGORE_INDEX`, it is flagged to be ignored and by any logic that processes or tracks slice
// indices.
pub const INGORE_INDEX: u8 = 59;

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
#[derive(Debug, Clone)]
pub struct Slice {
    // Raw pointer to the start of the memory block.
    //
    // Allows for efficient access to the underlying memory. Care should be taken when working with
    // raw pointers to avoid memory safety issues. The pointer must be valid and must point to a
    // properly allocated and initialized memory region.
    pub(crate) offset: *mut u8,

    // Length of the memory block in bytes.
    //
    // Represents how much memory is being used. This is critical for safe memory access, as it
    // prevents reading or writing outside the bounds of the buffer.
    pub(crate) len: usize,

    /// Unique identifier (index) of the slice in the shared memory pool.
    ///
    /// When in back or front mode, tracks the slice within the pool and manages memory reuse. It
    /// allows for quick identification of slices when freeing or reassigning memory. If in alloc
    /// mode, it is set to `IGNORE_INDEX`.
    pub index: u8,

    /// Shared state of the memory pool.
    ///
    /// When in back or front mode, tracks how many slices are currently in use and ensures proper
    /// synchronization of memory access across multiple contexts.
    pub shared_state: SharedState,

    /// Optional dynamically allocated buffer.
    ///
    /// If present, the slice owns the memory and is responsible for managing its lifecycle. If
    /// [`None`], the buffer pool is in back or front mode and the slice points to memory managed
    /// by the memory pool. Is `Some(Vec<u8>)` when in alloc mode.
    pub owned: Option<Vec<u8>>,

    // Mode flag to track the state of the slice during development.
    //
    // Useful for identifying whether the slice is being used correctly in different modes (e.g.,
    // whether is is currently being written to or read from). Typically used for logging and
    // debugging.
    #[cfg(feature = "debug")]
    pub mode: u8,

    /// Timestamp to track when the slice was created.
    ///
    /// Useful for diagnosing time-related issues and tracking the lifespan of memory slices during
    /// development and debugging.
    #[cfg(feature = "debug")]
    pub time: SystemTime,
}

impl Slice {
    /// Returns the length of the slice in bytes.
    ///
    /// If the slice owns its memory (`owned`), it returns the length of the owned buffer. If the
    /// slice does not own the memory, it returns `0`.
    pub fn len(&self) -> usize {
        if let Some(owned) = &self.owned {
            owned.len()
        } else {
            0
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
    /// Returns the owned buffer if present, otherwise converts the raw pointer and length into a
    /// mutable slice.
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        match self.owned.as_mut() {
            None => unsafe { core::slice::from_raw_parts_mut(self.offset, self.len) },
            Some(x) => x,
        }
    }
}

impl AsRef<[u8]> for Slice {
    /// Converts the [`Slice`] into an immutable slice of bytes (`&[u8]`).
    ///
    /// Returns the owned buffer if present, otherwise converts the raw pointer and length into an
    /// immutable slice.
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        match self.owned.as_ref() {
            None => unsafe { core::slice::from_raw_parts_mut(self.offset, self.len) },
            Some(x) => x,
        }
    }
}

impl Drop for Slice {
    /// Toggles the shared state when the slice is dropped, allowing the memory to be reused.
    ///
    /// In debug mode, it also tracks the `mode` of the slice when it is dropped.
    fn drop(&mut self) {
        #[cfg(feature = "debug")]
        self.shared_state.toogle(self.index, self.mode);

        #[cfg(not(feature = "debug"))]
        self.shared_state.toogle(self.index);
    }
}

impl From<Vec<u8>> for Slice {
    /// Creates a [`Slice`] from a [`Vec<u8>`], taking ownership of the vector.
    ///
    /// Initializes the [`Slice`] with the vector's pointer and sets the length to `0`.
    fn from(mut v: Vec<u8>) -> Self {
        let offset = v[0..].as_mut_ptr();
        Slice {
            offset,
            len: 0,
            // The slice's memory is owned by a `Vec<u8>`, so the slice does not need an `index` in
            // the pool to manage the memory
            index: crate::slice::INGORE_INDEX,
            shared_state: SharedState::new(),
            owned: Some(v),
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
// safe to clear. Each time a memory slice is used or released, the corresponding bit in the shared
// state is toggled. When no slices are in use (all bits are zero), the buffer pool can safely
// reclaim or reuse the memory.
//
// This system ensures that no memory is prematurely cleared while it is still being referenced.
// The buffer pool checks whether any slice is still in use before clearing, and only when the
// shared state indicates that all references have been dropped (i.e., no unprocessed messages
// remain) can the buffer pool safely clear or reuse the memory.
#[derive(Clone, Debug)]
pub struct SharedState(Arc<AtomicU8>);

impl Default for SharedState {
    // Creates a new `SharedState` with an internal `AtomicU8` initialized to `0`, indicating no
    // memory slots are in use.
    fn default() -> Self {
        Self::new()
    }
}

impl SharedState {
    // Creates a new `SharedState` with an internal `AtomicU8` initialized to `0`, indicating no
    // memory slots are in use.
    pub fn new() -> Self {
        Self(Arc::new(AtomicU8::new(0)))
    }

    // Atomically loads and returns the current value of the `SharedState` using the specified
    // memory ordering.
    //
    // Returns the current state of the memory slots as an 8-bit value.
    #[inline(always)]
    pub fn load(&self, ordering: Ordering) -> u8 {
        self.0.load(ordering)
    }

    // Toggles the bit at the specified `position` in the `SharedState`, including logs regarding
    // the shared state of the memory after toggling. The `mode` parameter is used to differentiate
    // between different states or operations (e.g., reading or writing) for debugging purposes.
    //
    // After a message held by a buffer slice has been processed, the corresponding bit in the
    // shared state is toggled (flipped). When the shared state for a given region reaches zero
    // (i.e., all bits are cleared), the buffer pool knows it can safely reclaim or reuse that
    // memory slice.
    //
    // Uses atomic bitwise operations to ensure thread-safe toggling without locks. It manipulates
    // the shared state in-place using the `AtomicU8::fetch_update` method, which atomically
    // applies a bitwise XOR (`^`) to toggle the bit at the specified `position`.
    //
    // Panics if the `position` is outside the range of 1-8, as this refers to an invalid bit.
    #[cfg(feature = "debug")]
    pub fn toogle(&self, position: u8, mode: u8) {
        let mask: u8 = match position {
            1 => 0b10000000,
            2 => 0b01000000,
            3 => 0b00100000,
            4 => 0b00010000,
            5 => 0b00001000,
            6 => 0b00000100,
            7 => 0b00000010,
            8 => 0b00000001,
            INGORE_INDEX => return,
            _ => panic!("{}", position),
        };
        //if position == 2 {
        //    let bt = Backtrace::force_capture();
        //    println!("{:#?}", bt);
        //};
        self.0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |mut shared_state| {
                let pre = shared_state;
                shared_state ^= mask;
                println!("TOOGLE:: {} {:b} {:b}", mode, pre, shared_state);
                Some(shared_state)
            })
            .unwrap();
    }

    // Toggles the bit at the specified `position` in the `SharedState`.
    //
    // After a message held by a buffer slice has been processed, the corresponding bit in the
    // shared state is toggled (flipped). When the shared state for a given region reaches zero
    // (i.e., all bits are cleared), the buffer pool knows it can safely reclaim or reuse that
    // memory slice.
    //
    // Uses atomic bitwise operations to ensure thread-safe toggling without locks. It manipulates
    // the shared state in-place using the `AtomicU8::fetch_update` method, which atomically
    // applies a bitwise XOR (`^`) to toggle the bit at the specified `position`.
    //
    // Panics if the `position` is outside the range of 1-8, as this refers to an invalid bit.
    #[cfg(not(feature = "debug"))]
    pub fn toogle(&self, position: u8) {
        let mask: u8 = match position {
            1 => 0b10000000,
            2 => 0b01000000,
            3 => 0b00100000,
            4 => 0b00010000,
            5 => 0b00001000,
            6 => 0b00000100,
            7 => 0b00000010,
            8 => 0b00000001,
            INGORE_INDEX => return,
            _ => panic!("{}", position),
        };
        self.0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |mut shared_state| {
                shared_state ^= mask;
                Some(shared_state)
            })
            .unwrap();
    }
}
