// # Simple `BufferPool` Usage
//
// This example showcases how to:
// 1. Creating a `BufferPool`.
// 2. Obtaining a writable buffer.
// 3. Writing data into the buffer.
// 4. Retrieving the data as a referenced slice.
// 5. Retrieving the data as an owned slice.
//
// # Run
//
// ```
// cargo run --example basic_buffer_pool
// ```

use buffer_sv2::{Buffer, BufferPool};

fn main() {
    // Create a new BufferPool with a capacity of 32 bytes
    let mut buffer_pool = BufferPool::new(32);

    // Get a writable buffer from the pool
    let data_to_write = b"Ciao, mundo!"; // 12 bytes
    let writable = buffer_pool.get_writable(data_to_write.len());

    // Write data (12 bytes) into the buffer.
    writable.copy_from_slice(data_to_write);
    assert_eq!(buffer_pool.len(), 12);

    // Retrieve the data as a referenced slice
    let _data_slice = buffer_pool.get_data_by_ref(12);
    assert_eq!(buffer_pool.len(), 12);

    // Retrieve the data as an owned slice
    let data_slice = buffer_pool.get_data_owned();
    assert_eq!(buffer_pool.len(), 0);

    let expect = [67, 105, 97, 111, 44, 32, 109, 117, 110, 100, 111, 33]; // "Ciao, mundo!" ASCII
    assert_eq!(data_slice.as_ref(), expect);
}
