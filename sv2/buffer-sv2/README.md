# `buffer_sv2`

[![crates.io](https://img.shields.io/crates/v/buffer_sv2.svg)](https://crates.io/crates/buffer_sv2)
[![docs.rs](https://docs.rs/buffer_sv2/badge.svg)](https://docs.rs/buffer_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=buffer_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)

`buffer_sv2` handles memory management for Stratum V2 (Sv2) roles. It provides a memory-efficient
buffer pool that minimizes allocations and deallocations for high-throughput message frame
processing in Sv2 roles. Memory allocation overhead is minimized by reusing large buffers,
improving performance and reducing latency. The buffer pool tracks the usage of memory slices,
using shared state tracking to safely manage memory across multiple threads.

## Main Components

- **Buffer Trait**: An interface for working with memory buffers. This trait has two implementations
  (`BufferPool` and `BufferFromSystemMemory`) that includes a `Write` trait to replace
  `std::io::Write` in `no_std` environments.
- **BufferPool**: A thread-safe pool of reusable memory buffers for high-throughput applications.
- **BufferFromSystemMemory**: Manages a dynamically growing buffer in system memory for applications
  where performance is not a concern.
- **Slice**: A contiguous block of memory, either preallocated or dynamically allocated.

## Usage

To include this crate in your project, run:

```bash
cargo add buffer_sv2
```

This crate can be built with the following feature flags:

- `debug`: Provides additional tracking for debugging memory management issues.
- `fuzz`: Enables support for fuzz testing.

### Unsafe Code
There are four unsafe code blocks instances:

- `buffer_pool/mod.rs`: `fn get_writable_(&mut self, len: usize, shared_state: u8, without_check: bool) -> &mut [u8] { .. }` in the `impl<T: Buffer> BufferPool<T>`
- `slice.rs`:
  - `unsafe impl Send for Slice {}`
  - `fn as_mut(&mut self) -> &mut [u8] { .. }` in the `impl AsMut<[u8]> for Slice`
  - `fn as_ref(&mut self) -> &mut [u8] { .. }` in the `impl AsMut<[u8]> for Slice`

### Examples

This crate provides three examples demonstrating how the memory is managed:

1. **[Basic Usage Example](https://github.com/stratum-mining/stratum/blob/main/utils/buffer/examples/basic_buffer_pool.rs)**:
   Creates a buffer pool, writes to it, and retrieves the data from it.

2. **[Buffer Pool Exhaustion Example](https://github.com/stratum-mining/stratum/blob/main/utils/buffer/examples/buffer_pool_exhaustion.rs)**:
   Demonstrates how data is added to a buffer pool and dynamically allocates directly to the heap
   once the buffer pool's capacity has been exhausted.

3. **[Variable Sized Messages Example](https://github.com/stratum-mining/stratum/blob/main/utils/buffer/examples/variable_sized_messages.rs)**:
   Writes messages of variable sizes to the buffer pool.

## `Buffer` Trait

The `Buffer` trait is designed to work with the
[`codec_sv2`](https://docs.rs/codec_sv2/1.3.0/codec_sv2/index.html) decoders, which operate by:

1. Filling a buffer with the size of the protocol header being decoded.
2. Parsing the filled bytes to compute the message length.
3. Filling a buffer with the size of the message.
4. Using the header and message to construct a
   [`framing_sv2::framing::Frame`](https://docs.rs/framing_sv2/2.0.0/framing_sv2/framing/enum.Frame.html).

To fill the buffer, the `codec_sv2` decoder must pass a reference of the buffer to a filler. To
construct a `Frame`, the decoder must pass ownership of the buffer to the `Frame`.

```rust
fn get_writable(&mut self, len: usize) -> &mut [u8];
```

This `get_writable` method returns a mutable reference to the buffer, starting at the current
length and ending at `len`, and sets the buffer length to the previous length plus `len`.

```rust
get_data_owned(&mut self) -> Slice;
```

This `get_data_owned` method returns a `Slice` that implements `AsMut<[u8]>` and `Send`.

The `Buffer` trait is implemented for `BufferFromSystemMemory` and `BufferPool`. It includes a
`Write` trait to replace `std::io::Write` in `no_std` environments.

## `BufferPoolFromSystemMemory`
`BufferFromSystemMemory` is a simple implementation of the `Buffer` trait. Each time a new buffer is
needed, it creates a new `Vec<u8>`.

- `get_writable(..)` returns mutable references to the inner vector.
- `get_data_owned(..)` returns the inner vector.

## `BufferPool`
While `BufferFromSystemMemory` is sufficient for many cases, `BufferPool` offers a more efficient
solution for high-performance applications, such as proxies and pools with thousands of connections.

When created, `BufferPool` preallocates a user-defined capacity of bytes in the heap using a
`Vec<u8>`. When `get_data_owned(..)` is called, it creates a `Slice` that contains a view into the
preallocated memory. `BufferPool` guarantees that slices never overlap and maintains unique
ownership of each `Slice`.

`Slice` implements the `Drop`, allowing the view into the preallocated memory to be reused upon
dropping.

### Buffer Management and Allocation

`BufferPool` is useful for working with sequentially processed buffers, such as filling a buffer,
retrieving it, and then reusing it as needed. `BufferPool` optimizes for memory reuse by providing
pre-allocated memory that can be used in one of three modes:

1. **Back Mode**: Default mode where allocations start from the back of the buffer.
2. **Front Mode**: Used when slots at the back are full but memory can still be reused by moving to
   the front.
3. **Alloc Mode**: Falls back to system memory allocation (`BufferFromSystemMemory`) when both back
   and front sections are full, providing additional capacity but with reduced performance.

`BufferPool` can only be fragmented between the front and back and between back and end.

#### Fragmentation, Overflow, and Optimization
`BufferPool` can allocate a maximum of `8` `Slice`s (as it uses an `AtomicU8` to track used and
freed slots) and up to the defined capacity in bytes. If all `8` slots are taken or there is no more
space in the preallocated memory, `BufferPool` falls back to `BufferFromSystemMemory`.

Typically, `BufferPool` is used to process messages sequentially (decode, respond, decode). It is
optimized to check for any freed slots starting from the beginning, then reuse these before
considering further allocation. It is also optimized to drop all the slices and to drop the last
slice. It also efficiently handles scenarios where all slices are dropped or when the last slice is
released, reducing memory fragmentation.

The following cases illustrate typical memory usage patterns within `BufferPool`:
1. Slots fill from back to front, switching as each area reaches capacity.
2. Pool resets upon full usage, then reuses back slots.
3. After filling the back, front slots are used when they become available.

Below is a graphical representation of the most optimized cases. A number means that the slot is
taken, the minus symbol (`-`) means the slot is free. There are `8` slots.

Case 1: Buffer pool exhaustion
```
--------  BACK MODE
1-------  BACK MODE
12------  BACK MODE
123-----  BACK MODE
1234----  BACK MODE
12345---  BACK MODE
123456--  BACK MODE
1234567-  BACK MODE
12345678  BACK MODE (buffer is now full)
12345678  ALLOC MODE (new bytes being allocated in a new space in the heap)
12345678  ALLOC MODE (new bytes being allocated in a new space in the heap)
..... and so on
```

Case 2: Buffer pool reset to remain in back mode
```
--------  BACK MODE
1-------  BACK MODE
12------  BACK MODE
123-----  BACK MODE
1234----  BACK MODE
12345---  BACK MODE
123456--  BACK MODE
1234567-  BACK MODE
12345678  BACK MODE (buffer is now full)
--------  RESET
9-------  BACK MODE
9a------  BACK MODE
```

Case 3: Buffer pool switches from back to front to back modes
```
--------  BACK MODE
1-------  BACK MODE
12------  BACK MODE
123-----  BACK MODE
1234----  BACK MODE
12345---  BACK MODE
123456--  BACK MODE
1234567-  BACK MODE
12345678  BACK MODE (buffer is now full)
--345678  Consume first two data bytes from the buffer
-9345678  SWITCH TO FRONT MODE
a9345678  FRONT MODE (buffer is now full)
a93456--  Consume last two data bytes from the buffer
a93456b-  SWITCH TO BACK MODE
a93456bc  BACK MODE (buffer is now full)
```

## Benchmarks and Performance

To run benchmarks, execute:

```
cargo bench --features criterion
```

## Benchmarks Comparisons

`BufferPool` is benchmarked against `BufferFromSystemMemory` and two additional structure for
reference: `PPool` (a hashmap-based pool) and `MaxEfficeincy` (a highly optimized but unrealistic
control implementation written such that the benchmarks do not panic and the compiler does not
complain). `BufferPool` generally provides better performance and lower latency than `PPool` and
`BufferFromSystemMemory`.

**Note**: Both `PPool` and `MaxEfficeincy` are completely broken and are only useful as references
for the benchmarks.

### `BENCHES.md` Benchmarks
The `BufferPool` always outperforms the `PPool` (hashmap-based pool) and the solution without a
pool.

Executed for 2,000 samples:

```
* single thread with  `BufferPool`: ---------------------------------- 7.5006 ms
* single thread with  `BufferFromSystemMemory`: ---------------------- 10.274 ms
* single thread with  `PPoll`: --------------------------------------- 32.593 ms
* single thread with  `MaxEfficeincy`: ------------------------------- 1.2618 ms
* multi-thread with   `BufferPool`: ---------------------------------- 34.660 ms
* multi-thread with   `BufferFromSystemMemory`: ---------------------- 142.23 ms
* multi-thread with   `PPoll`: --------------------------------------- 49.790 ms
* multi-thread with   `MaxEfficeincy`: ------------------------------- 18.201 ms
* multi-thread 2 with `BufferPool`: ---------------------------------- 80.869 ms
* multi-thread 2 with `BufferFromSystemMemory`: ---------------------- 192.24 ms
* multi-thread 2 with `PPoll`: --------------------------------------- 101.75 ms
* multi-thread 2 with `MaxEfficeincy`: ------------------------------- 66.972 ms
```

### Single Thread Benchmarks

If the buffer is not sent to another context `BufferPool`, it is 1.4 times faster than no pool, 4.3
time faster than the `PPool`, and 5.7 times slower than max efficiency.

Average times for 1,000 operations:

- `BufferPool`: 7.5 ms
- `BufferFromSystemMemory`: 10.27 ms
- `PPool`: 32.59 ms
- `MaxEfficiency`: 1.26 ms

```
for 0..1000:
  add random bytes to the buffer
  get the buffer
  add random bytes to the buffer
  get the buffer
  drop the 2 buffer
```

### Multi-Threaded Benchmarks (most similar to actual use case)

If the buffer is sent to other contexts, `BufferPool` is 4 times faster than no pool, 0.6 times
faster than `PPool`, and 1.8 times slower than max efficiency.

- `BufferPool`: 34.66 ms
- `BufferFromSystemMemory`: 142.23 ms
- `PPool`: 49.79 ms
- `MaxEfficiency`: 18.20 ms

```
for 0..1000:
  add random bytes to the buffer
  get the buffer
  send the buffer to another thread   -> wait 1 ms and then drop it
  add random bytes to the buffer
  get the buffer
  send the buffer to another thread   -> wait 1 ms and then drop it
```

### Multi threads 2
```
for 0..1000:
  add random bytes to the buffer
  get the buffer
  send the buffer to another thread   -> wait 1 ms and then drop it
  add random bytes to the buffer
  get the buffer
  send the buffer to another thread   -> wait 1 ms and then drop it
  wait for the 2 buffer to be dropped
```

## Fuzz Testing
Install `cargo-fuzz` with:

```bash
cargo install cargo-fuzz
```

Run the fuzz tests:

```bash
cd ./fuzz
cargo fuzz run slower -- -rss_limit_mb=5000000000
cargo fuzz run faster -- -rss_limit_mb=5000000000
```
The test must be run with `-rss_limit_mb=5000000000` as this flag checks `BufferPool` with
capacities from `0` to `2^32`.

`BufferPool` is fuzz-tested to ensure memory reliability across different scenarios, including
delayed memory release and cross-thread access. The tests checks if slices created by `BufferPool`
still contain the same bytes contained at creation time after a random amount of time and after it
has been sent to other threads.

There are 2 fuzzy test, the first (faster) it map a smaller input space to
Two main fuzz tests are provided:

1. Faster: Maps a smaller input space to test the most likely inputs
2. Slower: Has a bigger input space to explore "all" the edge case. It forces the buffer to be sent
   to different cores.

Both tests have been run for several hours without crashes.
