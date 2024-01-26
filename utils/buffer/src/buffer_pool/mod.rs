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

pub const POOL_CAPACITY: usize = 8;
mod pool_back;
pub use pool_back::PoolBack;

#[derive(Debug, Clone)]
/// Used by BufferPool to allocate in the front of the inner vector
pub struct PoolFront {
    back_start: usize,
    byte_capacity: usize,
    len: usize,
}

impl PoolFront {
    #[inline(always)]
    fn new(byte_capacity: usize, back_start: usize) -> Self {
        Self {
            byte_capacity,
            back_start,
            len: 0,
        }
    }

    #[inline(always)]
    // try to clear the tail of the head :D
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

#[derive(Debug, Clone)]
/// Internal memory of the BufferPool
pub struct InnerMemory {
    pool: Vec<u8>,
    pub(crate) raw_offset: usize,
    pub(crate) raw_len: usize,
    slots: [(usize, usize); POOL_CAPACITY],
    len: usize,
}

impl InnerMemory {
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

    #[inline(always)]
    fn reset(&mut self) {
        self.raw_len = 0;
        self.raw_offset = 0;
        self.slots = [(0_usize, 0_usize); POOL_CAPACITY];
    }

    #[inline(always)]
    fn reset_raw(&mut self) {
        self.raw_len = 0;
        self.raw_offset = 0;
    }

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

    #[inline(never)]
    fn prepend_raw_data(&mut self, raw_data: &[u8]) {
        self.raw_offset = 0;
        self.raw_len = raw_data.len();

        let dest = &mut self.pool[0..self.raw_len];

        dest.copy_from_slice(raw_data);
    }

    #[inline(never)]
    fn copy_into_buffer(&mut self, buffer: &mut impl Buffer) {
        let writable = buffer.get_writable(self.raw_len);
        writable.copy_from_slice(&self.pool[self.raw_offset..self.raw_offset + self.raw_len]);
    }

    #[inline(always)]
    fn has_tail_capacity(&self, len: usize) -> bool {
        let end = self.raw_offset + self.raw_len;
        end + len <= self.pool.capacity()
    }

    #[inline(always)]
    fn has_capacity_until_offset(&self, len: usize, offset: usize) -> bool {
        self.raw_offset + self.raw_len + len <= offset
    }

    #[inline(always)]
    fn get_writable_raw_unchecked(&mut self, len: usize) -> *mut u8 {
        let writable_offset = self.raw_offset + self.raw_len;
        self.raw_len += len;
        self.pool[writable_offset..writable_offset + len].as_mut_ptr()
    }

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
                let mut res = self.inner_memory.get_data_owned(shared_state, mode);
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
}

#[cfg(not(test))]
impl<T: Buffer> Drop for BufferPool<T> {
    fn drop(&mut self) {
        while self.shared_state.load(Ordering::Relaxed) != 0 {}
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
