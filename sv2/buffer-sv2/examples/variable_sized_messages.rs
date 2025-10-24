// # Handling Variable-Sized Messages
//
// This example demonstrates how to the `BufferPool` handles messages of varying sizes.
//
// # Run
//
// ```
// cargo run --example variable_sized_messages
// ```

use buffer_sv2::{Buffer, BufferPool};
use std::collections::VecDeque;

fn main() {
    // Initialize a BufferPool with a capacity of 32 bytes
    let mut buffer_pool = BufferPool::new(32);
    let mut slices = VecDeque::new();

    // Function to write data to the buffer pool and store the slice
    let write_data = |pool: &mut BufferPool<_>, data: &[u8], slices: &mut VecDeque<_>| {
        let writable = pool.get_writable(data.len());
        writable.copy_from_slice(data);
        let data_slice = pool.get_data_owned();
        slices.push_back(data_slice);
        println!("{:?}", &pool);
        println!();
    };

    // Write a small message to the first slot
    let small_message = b"Hello";
    write_data(&mut buffer_pool, small_message, &mut slices);
    assert!(buffer_pool.is_back_mode());
    assert_eq!(slices.back().unwrap().as_ref(), small_message);

    // Write a medium-sized message to the second slot
    let medium_message = b"Rust programming";
    write_data(&mut buffer_pool, medium_message, &mut slices);
    assert!(buffer_pool.is_back_mode());
    assert_eq!(slices.back().unwrap().as_ref(), medium_message);

    // Write a large message that exceeds the remaining pool capacity
    let large_message = b"This message is larger than the remaining buffer pool capacity.";
    write_data(&mut buffer_pool, large_message, &mut slices);
    assert!(buffer_pool.is_alloc_mode());
    assert_eq!(slices.back().unwrap().as_ref(), large_message);

    while let Some(slice) = slices.pop_front() {
        drop(slice);
    }

    // Write another small message
    let another_small_message = b"Hi";
    write_data(&mut buffer_pool, another_small_message, &mut slices);
    assert_eq!(slices.back().unwrap().as_ref(), another_small_message);

    // Verify that the buffer pool has returned to back mode for the last write
    assert!(buffer_pool.is_back_mode());
}
