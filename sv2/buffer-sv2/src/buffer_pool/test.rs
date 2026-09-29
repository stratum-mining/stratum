extern crate std;

use super::{InnerMemory, PoolBack, POOL_CAPACITY};
use crate::{buffer::BufferFromSystemMemory, Buffer};
use std::panic::{catch_unwind, AssertUnwindSafe};

#[test]
fn failed_tail_clear_keeps_the_back_length() {
    let mut memory = InnerMemory::new(40);
    for i in 0..POOL_CAPACITY {
        memory.slots[i] = (5 * i, 5);
    }
    memory.len = POOL_CAPACITY;

    let mut back = PoolBack::new();
    back.set_len_from_inner_memory(POOL_CAPACITY);

    // Three free tail slots, but a request no compaction can fit.
    let cleared = back.try_clear_tail_unchecked(&mut memory, 0b1111_1000, 1000);

    assert!(!cleared);
    assert_eq!(back.len(), POOL_CAPACITY);
}

#[test]
fn oversized_request_does_not_fit() {
    let mut memory = InnerMemory::new(8);
    memory.raw_offset = 1;

    assert!(!memory.has_tail_capacity(usize::MAX));
    assert!(!memory.has_capacity_until_offset(usize::MAX, 8));
}

#[test]
fn rejected_writable_range_leaves_the_length_untouched() {
    let mut memory = InnerMemory::new(8);
    memory.raw_offset = 1;

    let rejected = catch_unwind(AssertUnwindSafe(|| {
        memory.reserve_raw(usize::MAX);
    }));

    assert!(rejected.is_err());
    assert_eq!(memory.raw_len, 0);
}

#[test]
fn rejected_system_memory_request_leaves_the_length_untouched() {
    let mut memory = BufferFromSystemMemory::new(0);
    memory.reserve(1)[0] = 1;
    memory.commit(1);

    let rejected = catch_unwind(AssertUnwindSafe(|| {
        memory.reserve(usize::MAX);
    }));

    assert!(rejected.is_err());
    assert_eq!(Buffer::len(&memory), 1);
}
