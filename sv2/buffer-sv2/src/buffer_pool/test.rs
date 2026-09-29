use super::{InnerMemory, PoolBack, POOL_CAPACITY};

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
