use crate::buffer_pool::{InnerMemory, PoolFront, PoolMode, POOL_CAPACITY};

#[derive(Debug, Clone)]
pub struct PoolBack {
    back_start: usize,
    len: usize,
}

impl PoolBack {
    pub fn new() -> Self {
        Self {
            back_start: 0,
            len: 0,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn set_len_from_inner_memory(&mut self, len: usize) {
        let len = len - self.back_start;

        #[cfg(feature = "fuzz")]
        assert!(len + self.back_start <= POOL_CAPACITY);

        self.len = len;
    }

    #[inline(always)]
    pub fn back_start(&self) -> usize {
        self.back_start
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.back_start = 0;
        self.len = 0;
    }

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

    #[inline(always)]
    // Check if it is possible to clear the tail it must always be called before try_clear_tail
    pub fn tail_is_clearable(&self, shared_state: u8) -> bool {
        let element_in_back = POOL_CAPACITY - self.back_start;
        let element_to_drop = usize::min(element_in_back, shared_state.trailing_zeros() as usize);
        element_to_drop <= self.len && self.back_start + self.len >= POOL_CAPACITY
    }

    #[inline(always)]
    fn try_clear_head(
        &mut self,
        shared_state: u8,
        memory: &mut InnerMemory,
    ) -> Result<(), PoolMode> {
        // 0b00111110  2 leading zeros
        //
        // the first 2 elements have been dropped so back start at 2 and BufferPool can go in Front
        // mode
        //
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
