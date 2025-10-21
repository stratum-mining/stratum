// # Back of Buffer Pool
//
// Manages the "back" section of the buffer pool (`BufferPool`).
//
// The `PoolBack` struct is responsible for allocating, clearing, and managing memory slices from
// the back of the pool. It tracks the number of allocated slices and attempts to free up memory
// that is no longer in use, preventing unnecessary memory growth.
//
// Key functions in this module handle:
// - Clearing unused slices from the back of the pool to reclaim memory.
// - Managing slice allocation and ensuring enough capacity for new operations.
// - Switching between different pool modes, such as front or back, depending on memory state.
//
// By default, memory is always first allocated from the back of the `BufferPool`. If the all of
// the initially allocated buffer memory is completely filled and a new memory request comes in,
// `BufferPool` checks whether any memory has been freed at the back or the front using
// `SharedState`. If, for example, a slice has been freed that corresponds to the head of
// `SharedState`, `BufferPool` will switch to front mode and start allocating incoming memory
// requests from there. However, if the entire `SharedState` is full, it will switch to alloc mode
// and begin allocating system memory to fulfill incoming requests at a performance reduction. For
// subsequent memory requests, it will continue to check whether the prefix or suffix of
// `SharedState` has been freed. If it has, `BufferPool` will switch modes and start consuming
// pre-allocated `BufferPool` memory; if not, it will remain in alloc mode.

use crate::buffer_pool::{InnerMemory, PoolFront, PoolMode, POOL_CAPACITY};

// Manages the "back" section of the `BufferPool`.
//
// Handles the allocation of memory slices at the back of the buffer pool. It tracks the number of
// slices in use and attempts to free unused slices when necessary to maximize available memory.
// The back of the buffer pool is used first, if it fills up, the front of the buffer pool is used.
#[derive(Debug, Clone)]
pub struct PoolBack {
    // Starting index of the back section of the buffer pool.
    back_start: usize,

    // Number of allocated slices in the back section of the buffer pool.
    len: usize,
}

impl PoolBack {
    // Initializes a new `PoolBack` with no allocated slices.
    pub fn new() -> Self {
        Self {
            back_start: 0,
            len: 0,
        }
    }

    // Returns the number of allocated slices in the back section of the buffer pool.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    // Updates the length of the back section based on the state of the inner memory.
    #[inline(always)]
    pub fn set_len_from_inner_memory(&mut self, len: usize) {
        let len = len - self.back_start;

        #[cfg(feature = "fuzz")]
        assert!(len + self.back_start <= POOL_CAPACITY);

        self.len = len;
    }

    // Returns the starting index of the back section in the buffer pool.
    #[inline(always)]
    pub fn back_start(&self) -> usize {
        self.back_start
    }

    // Resets the back section of the pool by clearing its start index and length.
    #[inline(always)]
    pub fn reset(&mut self) {
        self.back_start = 0;
        self.len = 0;
    }

    // From the back section, checks if there are any unset bits and adjusts the length if there
    // are.
    //
    // Assumes the caller has already ensured the safety of the operation (such as slice bounds and
    // memory validity). It skips internal safety checks, relying on the caller to manage the
    // state, making it faster but potentially unsafe if misused.
    //
    // Returns `true` if the tail was cleared successfully, otherwise `false`.
    #[inline(always)]
    pub fn try_clear_tail_unchecked(
        &mut self,
        memory: &mut InnerMemory,
        shared_state: u8,
        len: usize,
    ) -> bool {
        let (leading_0, trailing_0) = (shared_state.leading_zeros(), shared_state.trailing_zeros());
        let element_in_back = POOL_CAPACITY - self.back_start;

        // 0b11000000   6 trailing zeros
        //       |
        //       v      but back start at position 5
        //   back start
        //
        // Pool must drop only 4 elements
        //
        let element_to_drop = usize::min(element_in_back, shared_state.trailing_zeros() as usize);

        // The actual element to drop are element_to_drop - already_dropped
        let already_dropped = POOL_CAPACITY - (self.back_start + self.len);

        match (
            leading_0,
            trailing_0,
            element_in_back,
            element_to_drop < already_dropped,
        ) {
            (0, 0, _, _) => false,
            (0, _, 0, _) => false,
            (_, _, _, true) => false,
            (0, _, _, _) => {
                let element_to_drop = element_to_drop - already_dropped;

                self.len -= element_to_drop;

                #[cfg(feature = "fuzz")]
                assert!(
                    self.len + self.back_start <= POOL_CAPACITY
                        && self.len + element_to_drop + already_dropped + self.back_start
                            == POOL_CAPACITY
                        && self.len + self.back_start <= POOL_CAPACITY
                );

                memory.try_change_len(self.len + self.back_start, len)
            }
            // If leading_0 is > than 0 return and clear the head
            (_, _, _, _) => false,
        }
    }

    // Checks if the tail of the back section can be cleared.
    //
    // Should always be called before attempting to clear the tail. Returns `true` if the tail can
    // be cleared, otherwise `false`.
    #[inline(always)]
    pub fn tail_is_clearable(&self, shared_state: u8) -> bool {
        let element_in_back = POOL_CAPACITY - self.back_start;
        let element_to_drop = usize::min(element_in_back, shared_state.trailing_zeros() as usize);

        element_to_drop <= self.len && self.back_start + self.len >= POOL_CAPACITY
    }

    // Attempts to clear the head of the back section, transitioning to the buffer pool front
    // section if not possible.
    //
    // Returns `Ok` if the head was cleared successfully. Otherwise an `Err(PoolMode::Front)` if
    // the buffer pool should switch to front mode, or an `Err(PoolMode::Alloc)` if the buffer pool
    // should switch to allocation mode.
    #[inline(always)]
    fn try_clear_head(
        &mut self,
        shared_state: u8,
        memory: &mut InnerMemory,
    ) -> Result<(), PoolMode> {
        // 0b00111110  2 leading zeros
        //
        // The first 2 elements have been dropped so back start at 2 and `BufferPool` can go in
        // front mode
        self.back_start = shared_state.leading_zeros() as usize;

        if self.back_start >= 1 && memory.raw_len < memory.slots[self.back_start].0 {
            if self.back_start >= self.len {
                self.len = 0;
            } else {
                self.len -= self.back_start;
            }
            let pool_front =
                PoolFront::new(memory.get_front_capacity(self.back_start), self.back_start);
            Err(PoolMode::Front(pool_front))
        } else {
            Err(PoolMode::Alloc)
        }
    }

    // Clears the tail or head of the back section, transitioning pool modes if necessary.
    //
    // Called when the state of the `BufferPool`, along with both `raw_offset` and `raw_len`,
    // reaches its maximum limits. It is used to clear any available space remaining in the
    // buffer's suffix or prefix. Based on that, the `BufferPool`'s state is updated by changing
    // its pool mode.
    //
    // Returns `Ok` if the tail or head was cleared successfully. Otherwise an
    // `Err(PoolMode::Front)` if the buffer pool should switch to front mode, or an
    // `Err(PoolMode::Alloc)` if the buffer pool should switch to allocation mode.
    #[inline(always)]
    pub fn clear_unchecked(
        &mut self,
        memory: &mut InnerMemory,
        shared_state: u8,
        len: usize,
    ) -> Result<(), PoolMode> {
        let tail_cleared = self.try_clear_tail_unchecked(memory, shared_state, len);

        if tail_cleared {
            return Ok(());
        };

        self.try_clear_head(shared_state, memory)
    }

    // Returns a writable slice of memory from the back section or transitions to a new pool mode
    // if necessary.
    //
    // Returns `Ok(*mut u8)` if writable memory is available, otherwise an `Err(PoolMode)` if a
    // mode change is required.
    #[inline(always)]
    pub fn get_writable(
        &mut self,
        len: usize,
        memory: &mut InnerMemory,
        shared_state: u8,
    ) -> Result<*mut u8, PoolMode> {
        let pool_has_byte_capacity = memory.has_tail_capacity(len);
        let pool_has_slice_capacity = (self.len + self.back_start) < POOL_CAPACITY;

        if pool_has_byte_capacity && pool_has_slice_capacity {
            #[cfg(feature = "fuzz")]
            assert!(self.len + self.back_start < POOL_CAPACITY);
            return Ok(memory.get_writable_raw_unchecked(len));
        }

        if !self.tail_is_clearable(shared_state) {
            return Err(PoolMode::Alloc);
        }

        match self.clear_unchecked(memory, shared_state, len) {
            Ok(_) => {
                let pool_has_byte_capacity = memory.has_tail_capacity(len);
                let pool_has_slice_capacity = self.len < POOL_CAPACITY;

                if pool_has_byte_capacity && pool_has_slice_capacity {
                    #[cfg(feature = "fuzz")]
                    assert!(self.len + self.back_start < POOL_CAPACITY);
                    Ok(memory.get_writable_raw_unchecked(len))
                } else {
                    Err(PoolMode::Alloc)
                }
            }
            Err(pool_mode) => Err(pool_mode),
        }
    }
}
