use alloc::{vec, vec::Vec};
use core::sync::atomic::Ordering;

#[cfg(test)]
use crate::buffer::TestBufferFromMemory;
use crate::{
    buffer::BufferFromSystemMemory,
    slice::{SharedState, Slice},
    Buffer,
};
#[cfg(feature = "debug")]
use std::time::SystemTime;

use aes_gcm::aead::Buffer as AeadBuffer;

mod pool_back;
pub use pool_back::PoolBack;

/// Maximum number of memory slices the buffer pool can concurrently manage.
///
/// This value limits the number of slices the [`BufferPool`] can track and manage at once. Once
/// the pool reaches its capacity of `8` slices, it may need to free memory or switch modes (e.g.,
/// to system memory). The choice of `8` ensures a balance between performance and memory
/// management. The use of `usize` allows for compatibility with platform-dependent memory
/// operations.
pub const POOL_CAPACITY: usize = 8;

/// Manages the "front" section of the [`BufferPool`].
///
/// Handles the allocation of memory slices at the front of the buffer pool. It tracks the number
/// of slices in use and attempts to free unused slices when necessary to maximize available
/// memory. The front of the buffer pool is used if the back of the buffer pool is filled up.
#[derive(Debug, Clone)]
pub struct PoolFront {
    // Starting index of the front section of the buffer pool.
    back_start: usize,

    // Maximum number of bytes that can be allocated in the front section of the buffer pool.
    //
    // Helps manage how much memory can be used before triggering memory clearing or switching
    // [`PoolMode`].
    byte_capacity: usize,

    // Number of allocated slices in the front section of the buffer pool.
    len: usize,
}

impl PoolFront {
    /// Initializes a new [`PoolFront`] with the specified byte capacity and back start position.
    #[inline(always)]
    fn new(byte_capacity: usize, back_start: usize) -> Self {
        Self {
            byte_capacity,
            back_start,
            len: 0,
        }
    }

    /// Attempts to clear unused memory slices at the tail of the front section.
    ///
    /// Returns `true` if slices were successfully cleared, otherwise `false` if no slices could be
    /// cleared or memory conditions prevent clearing.
    #[inline(always)]
    fn try_clear_tail(&mut self, memory: &mut InnerMemory, mut shared_state: u8) -> bool {
        #[cfg(feature = "fuzz")]
        assert!(self.len > 0);
        // 1001_1100   bs=3 len=1
        // 0000_0001   >> 7
        shared_state >>= POOL_CAPACITY - self.len;

        let element_to_drop = shared_state.trailing_zeros();

        match element_to_drop {
            0 => false,
            8 => {
                self.len = 0;

                let raw_offset = memory.raw_offset();
                memory.move_raw_at_offset_unchecked(raw_offset);

                true
            }
            _ => {
                #[cfg(feature = "fuzz")]
                assert!(
                    element_to_drop <= self.back_start as u32 && element_to_drop <= self.len as u32
                );

                self.len -= element_to_drop as usize;

                memory.len = self.len;
                let raw_offset = memory.raw_offset();
                memory.move_raw_at_offset_unchecked(raw_offset);

                true
            }
        }
    }

    /// Clears the front memory slices if conditions allow and checks if the memory pool has
    /// capacity to allocate `len` bytes in the buffer.
    ///
    /// Returns `Ok` if memory was successfully cleared and there is sufficient capacity, otherwise
    /// an `Err(PoolMode::Back)` if the memory cannot be cleared or lacks capacity. This error
    /// indicates the [`BufferPool`] should attempt a transition to use the back of the buffer
    /// pool.
    #[inline(always)]
    fn clear(
        &mut self,
        memory: &mut InnerMemory,
        shared_state: u8,
        len: usize,
    ) -> Result<(), PoolMode> {
        if self.len > 0 && self.try_clear_tail(memory, shared_state) {
            if memory.has_capacity_until_offset(len, self.byte_capacity) {
                Ok(())
            } else {
                Err(PoolMode::Back)
            }
        } else {
            Err(PoolMode::Back)
        }
    }

    /// Attempts to allocate a writable memory region in the front section of the buffer pool,
    /// returning a writable slice if successful, or transitioning to a new pool mode if necessary.
    ///
    /// Returns a pointer to the writable memory (`Ok(*mut u8)`) if successful, otherwise an
    /// `Err(PoolMode::Back)` if the memory cannot be cleared or lacks capacity. This error
    /// indicates the [`BufferPool`] should attempt a transition to use the back of the buffer
    /// pool.
    #[inline(always)]
    fn get_writable(
        &mut self,
        len: usize,
        memory: &mut InnerMemory,
        shared_state: u8,
    ) -> Result<*mut u8, PoolMode> {
        let pool_has_slice_capacity = self.len < self.back_start;
        let pool_has_head_capacity = memory.has_capacity_until_offset(len, self.byte_capacity);

        if pool_has_slice_capacity && pool_has_head_capacity {
            return Ok(memory.get_writable_raw_unchecked(len));
        };

        self.clear(memory, shared_state, len)
            .map(|_| memory.get_writable_raw_unchecked(len))
    }
}

#[derive(Debug, Clone)]
/// Internal state of the BufferPool
pub enum PoolMode {
    Back,
    Front(PoolFront),
    Alloc,
}

/// Internal memory management for the [`BufferPool`].
///
/// Handles allocating, tracking, and managing memory slices for manipulating memory offsets,
/// copying data, and managing capacity. It uses a contiguous block of memory ([`Vec<u8>`]),
/// tracking its usage through offsets (`raw_offset`, `raw_length`), and manages slice allocations
/// through `slots`. Used by [`BufferPool`] to optimize memory reused and minimize heap
/// allocations.
#[derive(Debug, Clone)]
pub struct InnerMemory {
    // Underlying contiguous block of memory to be managed.
    pool: Vec<u8>,

    // Current offset into the contiguous block of memory where the next write will occur.
    pub(crate) raw_offset: usize,

    // Length of the valid data within the contiguous block of memory, starting from `raw_offset`.
    pub(crate) raw_len: usize,

    // Tracks individual chunks of memory being used in the buffer pool by tracking the start and
    // length of each allocated memory slice.
    slots: [(usize, usize); POOL_CAPACITY],

    // Number of active slices in use, represented by how many slots are currently occupied.
    len: usize,
}

impl InnerMemory {
    // Initializes a new `InnerMemory` with a specified size of the internal memory buffer
    // (`capacity`), in bytes.
    fn new(capacity: usize) -> Self {
        let pool = vec![0; capacity];
        Self {
            pool,
            raw_offset: 0,
            raw_len: 0,
            slots: [(0_usize, 0_usize); POOL_CAPACITY],
            len: 0,
        }
    }

    // Resets the internal memory pool, clearing all used memory and resetting the slot tracking.
    #[inline(always)]
    fn reset(&mut self) {
        self.raw_len = 0;
        self.raw_offset = 0;
        self.slots = [(0_usize, 0_usize); POOL_CAPACITY];
    }

    // Resets only the raw memory state, without affecting the slot tracking.
    #[inline(always)]
    fn reset_raw(&mut self) {
        self.raw_len = 0;
        self.raw_offset = 0;
    }

    // Returns the capacity of the front portion of the buffer.
    //
    // Used to determine how much space is available in the front of the buffer pool, based on the
    // `back_start` position.
    #[inline(always)]
    fn get_front_capacity(&self, back_start: usize) -> usize {
        #[cfg(feature = "fuzz")]
        assert!(
            back_start >= 1
                && back_start < POOL_CAPACITY
                && self.slots[back_start].0 != 0_usize
                && self.slots[back_start].1 != 0_usize
                && self.slots[back_start].0 + self.slots[back_start].1 <= self.pool.len()
        );

        self.slots[back_start].0
    }

    // Calculates the offset for the next writable section of memory. Returns the offset based on
    // the current memory length and slot usage.
    #[inline(always)]
    fn raw_offset(&self) -> usize {
        match self.len {
            0 => 0,
            _ => {
                let index = self.len - 1;
                #[cfg(feature = "fuzz")]
                assert!(
                    index < POOL_CAPACITY
                        && self.slots[index].1 != 0_usize
                        && self.slots[index].0 + self.slots[index].1 <= self.pool.len()
                );

                let (index, len) = self.slots[index];

                index + len
            }
        }
    }

    // Calculates the offset for a specific length of memory. Returns the offset based on the
    // provided length and the current state of the memory slots.
    #[inline(always)]
    fn raw_offset_from_len(&self, len: usize) -> usize {
        match len {
            0 => 0,
            _ => {
                let index = len - 1;
                #[cfg(feature = "fuzz")]
                assert!(
                    index < POOL_CAPACITY
                        && self.slots[index].1 != 0_usize
                        && self.slots[index].0 + self.slots[index].1 <= self.pool.len()
                );

                let (index, len) = self.slots[index];

                index + len
            }
        }
    }

    // Moves the raw data to the front of the memory pool to avoid fragmentation, if necessary.
    //
    // Used to compact the raw data by moving all the active slices to the front of the memory
    // pool, making the pool contiguous again. This process is only performed when needed to free
    // up space for new allocations without increasing the total memory footprint.
    #[inline(always)]
    fn move_raw_at_front(&mut self) {
        match self.raw_len {
            0 => self.raw_offset = 0,
            _ => {
                self.pool
                    .copy_within(self.raw_offset..self.raw_offset + self.raw_len, 0);
                self.raw_offset = 0;
            }
        }
    }

    // Tries to update the length and offset of the memory pool and moves the raw offset if there
    // is enough capacity to accommodate new memory. Returns `true` if successful, otherwise false.
    #[inline(always)]
    fn try_change_len(&mut self, slot_len: usize, raw_len: usize) -> bool {
        let raw_offset = self.raw_offset_from_len(slot_len);

        let end = raw_offset + self.raw_len;
        if end + raw_len <= self.pool.capacity() {
            self.len = slot_len;
            self.move_raw_at_offset_unchecked(raw_offset);
            true
        } else {
            false
        }
    }

    // Moves the raw data to a specific offset within the memory pool to avoid fragmentation, if
    // necessary.
    //
    // Misuse of this function can lead to undefined behavior, such as memory corruption or
    // crashes, if it operates on out-of-bounds or misaligned memory.
    #[inline(always)]
    fn move_raw_at_offset_unchecked(&mut self, offset: usize) {
        match self.raw_len {
            0 => self.raw_offset = offset,
            _ => {
                self.pool
                    .copy_within(self.raw_offset..self.raw_offset + self.raw_len, offset);
                self.raw_offset = offset;
            }
        }
    }

    // Inserts raw data at the front of the memory pool, adjusting the raw offset and length.
    #[inline(never)]
    fn prepend_raw_data(&mut self, raw_data: &[u8]) {
        self.raw_offset = 0;
        self.raw_len = raw_data.len();

        let dest = &mut self.pool[0..self.raw_len];

        dest.copy_from_slice(raw_data);
    }

    // Copies the internal raw memory into another buffer. Used when transitioning memory between
    // different pool modes.
    #[inline(never)]
    fn copy_into_buffer(&mut self, buffer: &mut impl Buffer) {
        let writable = buffer.get_writable(self.raw_len);
        writable.copy_from_slice(&self.pool[self.raw_offset..self.raw_offset + self.raw_len]);
    }

    // Checks if there is enough capacity at the tail of the memory pool to accommodate `len`
    // bytes.
    #[inline(always)]
    fn has_tail_capacity(&self, len: usize) -> bool {
        let end = self.raw_offset + self.raw_len;
        end + len <= self.pool.capacity()
    }

    // Checks if there is enough capacity in the memory pool up to the specified offset to
    // accommodate `len` bytes.
    #[inline(always)]
    fn has_capacity_until_offset(&self, len: usize, offset: usize) -> bool {
        self.raw_offset + self.raw_len + len <= offset
    }

    // Returns a raw pointer to the writable memory region of the memory pool, marking the section
    // as used.
    #[inline(always)]
    fn get_writable_raw_unchecked(&mut self, len: usize) -> *mut u8 {
        let writable_offset = self.raw_offset + self.raw_len;
        self.raw_len += len;
        self.pool[writable_offset..writable_offset + len].as_mut_ptr()
    }

    // Transfers ownership of the raw memory slice that has been written into the buffer, returning
    // it as a `Slice`. This is the main mechanism for passing processed data from the buffer to
    // the rest of the system.
    //
    // After this call, the buffer resets internally, allowing it to write new data into a fresh
    // portion of memory. This prevents memory duplication and ensures efficient reuse of the
    // buffer. This method is used when the data in the buffer is ready for processing, sending, or
    // further manipulation. The returned `Slice` now owns the memory and the buffer no longer has
    // responsibility for it, allowing the buffer to handle new incoming data.
    #[inline(always)]
    fn get_data_owned(
        &mut self,
        shared_state: &mut SharedState,
        #[cfg(feature = "debug")] mode: u8,
    ) -> Slice {
        let slice = &mut self.pool[self.raw_offset..self.raw_offset + self.raw_len];

        let mut index: u8 = crate::slice::INGORE_INDEX;

        if self.raw_len > 0 {
            self.slots[self.len] = (self.raw_offset, self.raw_len);

            self.len += 1;
            index = self.len as u8;

            #[cfg(feature = "debug")]
            shared_state.toogle(index, mode);

            #[cfg(not(feature = "debug"))]
            shared_state.toogle(index);
        }

        let offset = slice.as_mut_ptr();

        self.raw_offset += self.raw_len;
        self.raw_len = 0;

        Slice {
            offset,
            len: slice.len(),
            index,
            shared_state: shared_state.clone(),
            owned: None,
            #[cfg(feature = "debug")]
            mode,
            #[cfg(feature = "debug")]
            time: SystemTime::now(),
        }
    }
}

#[derive(Debug)]
pub struct BufferPool<T: Buffer> {
    pool_back: PoolBack,
    pub mode: PoolMode,
    shared_state: SharedState,
    inner_memory: InnerMemory,
    system_memory: T,
    // Used only when we need as_ref or as_mut, set the first element to the one with index equal
    // to start
    start: usize,
}

impl BufferPool<BufferFromSystemMemory> {
    pub fn new(capacity: usize) -> Self {
        Self {
            pool_back: PoolBack::new(),
            mode: PoolMode::Back,
            shared_state: SharedState::new(),
            inner_memory: InnerMemory::new(capacity),
            system_memory: BufferFromSystemMemory::default(),
            start: 0,
        }
    }
}

#[cfg(test)]
impl BufferPool<TestBufferFromMemory> {
    #[cfg(test)]
    pub fn new_fail_system_memory(capacity: usize) -> Self {
        Self {
            pool_back: PoolBack::new(),
            mode: PoolMode::Back,
            shared_state: SharedState::new(),
            inner_memory: InnerMemory::new(capacity),
            system_memory: TestBufferFromMemory(Vec::new()),
            start: 0,
        }
    }
}

impl<T: Buffer> BufferPool<T> {
    pub fn is_front_mode(&self) -> bool {
        match self.mode {
            PoolMode::Back => false,
            PoolMode::Front(_) => true,
            PoolMode::Alloc => false,
        }
    }

    pub fn is_back_mode(&self) -> bool {
        match self.mode {
            PoolMode::Back => true,
            PoolMode::Front(_) => false,
            PoolMode::Alloc => false,
        }
    }

    pub fn is_alloc_mode(&self) -> bool {
        match self.mode {
            PoolMode::Back => false,
            PoolMode::Front(_) => false,
            PoolMode::Alloc => true,
        }
    }

    #[inline(always)]
    fn reset(&mut self) {
        #[cfg(feature = "debug")]
        println!("RESET");

        self.inner_memory.len = 0;

        match self.mode {
            PoolMode::Back => {
                self.inner_memory.move_raw_at_front();
                self.pool_back.reset();
            }
            PoolMode::Front(_) => {
                self.inner_memory.move_raw_at_front();
                self.mode = PoolMode::Back;
                self.pool_back.reset();
            }
            PoolMode::Alloc => {
                if self.system_memory.len() < self.inner_memory.pool.capacity() {
                    let raw_len = self.system_memory.len();
                    if raw_len > 0 {
                        self.inner_memory
                            .prepend_raw_data(self.system_memory.get_data_by_ref(raw_len));
                        self.system_memory.get_data_owned();
                    } else {
                        self.inner_memory.reset();
                    }
                    self.mode = PoolMode::Back;
                    self.pool_back.reset();
                }
            }
        }
    }

    #[inline(never)]
    fn get_writable_from_system_memory(
        &mut self,
        len: usize,
        shared_state: u8,
        without_check: bool,
    ) -> &mut [u8] {
        if self.system_memory.len() > 0
            || without_check
            || self.pool_back.len() == 0
            || !self.pool_back.tail_is_clearable(shared_state)
        {
            self.system_memory.get_writable(len)
        } else {
            #[cfg(feature = "fuzz")]
            assert!(self.inner_memory.raw_len == 0 && self.inner_memory.raw_offset == 0);

            match self
                .pool_back
                .clear_unchecked(&mut self.inner_memory, shared_state, len)
            {
                Ok(_) => {
                    self.change_mode(PoolMode::Back, len, shared_state);
                    self.get_writable_(len, shared_state, false)
                }
                Err(PoolMode::Front(f)) => {
                    self.change_mode(PoolMode::Front(f), len, shared_state);
                    self.get_writable_(len, shared_state, false)
                }
                Err(PoolMode::Alloc) => {
                    self.inner_memory.reset_raw();
                    self.system_memory.get_writable(len)
                }
                Err(_) => panic!(),
            }
        }
    }

    #[inline(never)]
    fn get_data_owned_from_sytem_memory(&mut self) -> Slice {
        self.system_memory.get_data_owned().into()
    }

    #[inline(always)]
    fn change_mode(&mut self, mode: PoolMode, len: usize, shared_state: u8) {
        match (&mut self.mode, &mode) {
            (PoolMode::Back, PoolMode::Alloc) => {
                #[cfg(feature = "debug")]
                println!(
                    "BACK => ALLOc {} {:?}",
                    self.pool_back.len(),
                    SystemTime::now()
                );

                self.inner_memory.copy_into_buffer(&mut self.system_memory);
                self.inner_memory.reset_raw();
                self.mode = mode;
            }
            (PoolMode::Back, PoolMode::Front(_)) => {
                #[cfg(feature = "fuzz")]
                assert!(shared_state.leading_zeros() > 0);
                #[cfg(feature = "debug")]
                println!("BACK => FRONT");

                self.inner_memory.len = 0;
                self.inner_memory.move_raw_at_front();
                self.mode = mode;
            }
            (PoolMode::Front(_), PoolMode::Back) => {
                #[cfg(feature = "debug")]
                println!("FRONT +> BACL");

                if !self.pool_back.tail_is_clearable(shared_state) {
                    self.inner_memory.copy_into_buffer(&mut self.system_memory);
                    self.inner_memory.reset_raw();
                    self.mode = PoolMode::Alloc;
                } else if self.pool_back.try_clear_tail_unchecked(
                    &mut self.inner_memory,
                    shared_state,
                    len,
                ) {
                    self.mode = mode;
                } else {
                    #[cfg(feature = "debug")]
                    println!("ALLOC 2");

                    self.inner_memory.copy_into_buffer(&mut self.system_memory);
                    self.inner_memory.reset_raw();
                    self.mode = PoolMode::Alloc;
                }
            }
            (PoolMode::Alloc, PoolMode::Back) => {
                #[cfg(feature = "debug")]
                println!("ALLOC => BACK {:?}", SystemTime::now());

                self.inner_memory.len = self.pool_back.len() + self.pool_back.back_start();
                self.mode = mode;
            }
            (PoolMode::Alloc, PoolMode::Front(_)) => {
                #[cfg(feature = "fuzz")]
                assert!(shared_state.leading_zeros() > 0);
                #[cfg(feature = "debug")]
                println!("ALLOC +> FORNT {:?} {:b}", SystemTime::now(), shared_state);

                self.inner_memory.reset_raw();
                self.inner_memory.len = 0;
                self.mode = mode;
            }
            (PoolMode::Front(_), PoolMode::Alloc) => {
                panic!();
            }
            (PoolMode::Back, PoolMode::Back) => {
                panic!();
            }
            (PoolMode::Front(_), PoolMode::Front(_)) => {
                panic!();
            }
            (PoolMode::Alloc, PoolMode::Alloc) => {
                panic!();
            }
        }
    }

    #[inline(always)]
    fn get_writable_(&mut self, len: usize, shared_state: u8, without_check: bool) -> &mut [u8] {
        let writable = match &mut self.mode {
            PoolMode::Back => {
                self.pool_back
                    .get_writable(len, &mut self.inner_memory, shared_state)
            }
            PoolMode::Front(front) => front.get_writable(len, &mut self.inner_memory, shared_state),
            PoolMode::Alloc => {
                return self.get_writable_from_system_memory(len, shared_state, without_check)
            }
        };

        match writable {
            Ok(offset) => {
                let writable: &mut [u8];
                unsafe {
                    writable = core::slice::from_raw_parts_mut(offset, len);
                }
                writable
            }
            Err(mode) => {
                self.change_mode(mode, len, shared_state);
                let without_check = self.is_alloc_mode();
                self.get_writable_(len, shared_state, without_check)
            }
        }
    }
}

impl<T: Buffer> Buffer for BufferPool<T> {
    type Slice = Slice;

    #[inline(always)]
    fn get_writable(&mut self, len: usize) -> &mut [u8] {
        let shared_state = self.shared_state.load(Ordering::Relaxed);

        // If all the slices have been dropped just reset the pool
        if shared_state == 0 && self.pool_back.len() != 0 {
            self.reset();
        }

        self.get_writable_(len, shared_state, false)
    }

    #[inline(always)]
    fn get_data_owned(&mut self) -> Self::Slice {
        let shared_state = &mut self.shared_state;

        #[cfg(feature = "debug")]
        let mode: u8 = match self.mode {
            PoolMode::Back => 0,
            PoolMode::Front(_) => 1,
            PoolMode::Alloc => 8,
        };

        #[cfg(feature = "debug")]
        match &mut self.mode {
            PoolMode::Back => {
                println!(
                    "{} {} {}",
                    self.inner_memory.raw_offset, self.inner_memory.raw_len, self.inner_memory.len
                );
                let res = self.inner_memory.get_data_owned(shared_state, mode);
                self.pool_back
                    .set_len_from_inner_memory(self.inner_memory.len);
                println!(
                    "{} {} {}",
                    self.inner_memory.raw_offset, self.inner_memory.raw_len, self.inner_memory.len
                );
                println!("GET DATA BACK {:?}", self.inner_memory.slots);
                res
            }
            PoolMode::Front(f) => {
                let res = self.inner_memory.get_data_owned(shared_state, mode);
                f.len = self.inner_memory.len;
                println!("GET DATA FRONT {:?}", self.inner_memory.slots);
                res
            }
            PoolMode::Alloc => self.get_data_owned_from_sytem_memory(),
        }

        #[cfg(not(feature = "debug"))]
        match &mut self.mode {
            PoolMode::Back => {
                let res = self.inner_memory.get_data_owned(shared_state);
                self.pool_back
                    .set_len_from_inner_memory(self.inner_memory.len);
                res
            }
            PoolMode::Front(f) => {
                let res = self.inner_memory.get_data_owned(shared_state);
                f.len = self.inner_memory.len;
                res
            }
            PoolMode::Alloc => self.get_data_owned_from_sytem_memory(),
        }
    }

    fn get_data_by_ref(&mut self, len: usize) -> &mut [u8] {
        match self.mode {
            PoolMode::Alloc => self.system_memory.get_data_by_ref(len),
            _ => {
                &mut self.inner_memory.pool[self.inner_memory.raw_offset
                    ..self.inner_memory.raw_offset + self.inner_memory.raw_len]
            }
        }
    }

    fn get_data_by_ref_(&self, len: usize) -> &[u8] {
        match self.mode {
            PoolMode::Alloc => self.system_memory.get_data_by_ref_(len),
            _ => {
                &self.inner_memory.pool[self.inner_memory.raw_offset
                    ..self.inner_memory.raw_offset + self.inner_memory.raw_len]
            }
        }
    }

    fn len(&self) -> usize {
        match self.mode {
            PoolMode::Back => self.inner_memory.raw_len,
            PoolMode::Front(_) => self.inner_memory.raw_len,
            PoolMode::Alloc => self.system_memory.len(),
        }
    }

    fn danger_set_start(&mut self, index: usize) {
        self.start = index;
    }

    #[inline(always)]
    fn is_droppable(&self) -> bool {
        self.shared_state.load(Ordering::Relaxed) == 0
    }
}

#[cfg(not(test))]
impl<T: Buffer> Drop for BufferPool<T> {
    fn drop(&mut self) {
        while self.shared_state.load(Ordering::Relaxed) != 0 {
            core::hint::spin_loop();
        }
    }
}

impl<T: Buffer> BufferPool<T> {
    pub fn droppable(&self) -> bool {
        self.shared_state.load(Ordering::Relaxed) == 0
    }
}

impl<T: Buffer> AsRef<[u8]> for BufferPool<T> {
    fn as_ref(&self) -> &[u8] {
        &self.get_data_by_ref_(Buffer::len(self))[self.start..]
    }
}
impl<T: Buffer> AsMut<[u8]> for BufferPool<T> {
    fn as_mut(&mut self) -> &mut [u8] {
        let start = self.start;
        self.get_data_by_ref(Buffer::len(self))[start..].as_mut()
    }
}
impl<T: Buffer + AeadBuffer> AeadBuffer for BufferPool<T> {
    fn extend_from_slice(&mut self, other: &[u8]) -> aes_gcm::aead::Result<()> {
        self.get_writable(other.len()).copy_from_slice(other);
        Ok(())
    }

    fn truncate(&mut self, len: usize) {
        let len = len + self.start;
        match self.mode {
            PoolMode::Back => self.inner_memory.raw_len = len,
            PoolMode::Front(_) => self.inner_memory.raw_len = len,
            PoolMode::Alloc => self.system_memory.truncate(len),
        }
    }
}
