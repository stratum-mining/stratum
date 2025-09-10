// # Handling Buffer Pool Exhaustion and Heap Allocation
//
// This example demonstrates how a buffer pool is filled. The back slots of the buffer pool are
// exhausted first, followed by the front of the buffer pool. Once both the back and front are
// exhausted, data is allocated on the heap at a performance decrease.
//
// 1. Fills up the back slots of the buffer pool until they’re exhausted.
// 2. Releases one slot to allow the buffer pool to switch to front mode.
// 3. Fully fills the front slots of the buffer pool.
// 4. Switches to alloc mode for direct heap allocation when both the buffer pool's back and front
//    slots are at capacity.
//
// Below is a visual representation of how the buffer pool evolves as the example progresses:
//
// --------  BACK MODE
// a-------  BACK MODE (add "a" via loop)
// aa------  BACK MODE (add "a" via loop)
// aaa-----  BACK MODE (add "a" via loop)
// aaaa----  BACK MODE (add "a" via loop)
// -aaa----  BACK MODE (pop front)
// -aaab---  BACK MODE (add "b")
// -aaabc--  BACK MODE (add "c" via loop)
// -aaabcc-  BACK MODE (add "c" via loop)
// -aaabccc  BACK MODE (add "c" via loop)
// caaabccc  BACK MODE (add "c" via loop, which gets added via front mode)
// caaabccc  ALLOC MODE (add "d", allocated in a new space in the heap)
//
// # Run
//
// ```
// cargo run --example buffer_pool_exhaustion
// ```

use buffer_sv2::{Buffer, BufferPool};
use std::collections::VecDeque;

fn main() {
    // 8 byte capacity
    let mut buffer_pool = BufferPool::new(8);
    let mut slices = VecDeque::new();

    // Write data to fill back slots
    for _ in 0..4 {
        let data_bytes = b"a"; // 1 byte
        let writable = buffer_pool.get_writable(data_bytes.len()); // Mutable slice to internal
                                                                   // buffer
        writable.copy_from_slice(data_bytes);
        let data_slice = buffer_pool.get_data_owned(); // Take ownership of allocated segment
        slices.push_back(data_slice);
    }
    assert!(buffer_pool.is_back_mode());

    // Release one slice and add another in the back (one slice in back mode must be free to switch
    // to front mode)
    slices.pop_front(); // Free the slice's associated segment in the buffer pool
    let data_bytes = b"b"; // 1 byte
    let writable = buffer_pool.get_writable(data_bytes.len());
    writable.copy_from_slice(data_bytes);
    let data_slice = buffer_pool.get_data_owned();
    slices.push_back(data_slice);
    assert!(buffer_pool.is_back_mode()); // Still in back mode

    // Write data to switch to front mode
    for _ in 0..4 {
        let data_bytes = b"c"; // 1 byte
        let writable = buffer_pool.get_writable(data_bytes.len());
        writable.copy_from_slice(data_bytes);
        let data_slice = buffer_pool.get_data_owned();
        slices.push_back(data_slice);
    }
    assert!(buffer_pool.is_front_mode()); // Confirm front mode

    // Add another slice, causing a switch to alloc mode
    let data_bytes = b"d"; // 1 byte
    let writable = buffer_pool.get_writable(data_bytes.len());
    writable.copy_from_slice(data_bytes);
    let data_slice = buffer_pool.get_data_owned();
    slices.push_back(data_slice);
    assert!(buffer_pool.is_alloc_mode());
}
