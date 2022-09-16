# BufferPool

This crate provides a `Write` trait used to replace `std::io::Write` in a non_std environment a `Buffer`
trait and two implementations of `Buffer`: `BufferFromSystemMemory` and `BufferPool`.

## Intro
`BufferPool` is useful whenever we need to work with buffers sequentially (fill a buffer, get
the filled buffer, fill a new buffer, get the filled buffer, and so on).

To fill a buffer `BufferPool` returns an `&mut [u8]` with the requested len (the filling part).
When the buffer is filled and the owner needs to be changed, `BufferPool` returns a `Slice` that
implements `Send` and `AsMut<u8>` (the get part).

`BufferPool` pre-allocates a user-defined capacity in the heap and use it to allocate the buffers,
when a `Slice` is dropped `BufferPool` acknowledge it and reuse the freed space in the pre-allocated
memory.

## Implementation
The crate is `[no_std]` and lock-free, so, to synchronize the state of the pre-allocated memory
(taken or free) between different contexts an `AtomicU8` is used.
Each bit of the `u8` represent a memory slot, if the bit is 0 the memory slot is free if is
1 is taken. Whenever `BufferPool` creates a `Slice` a bit is set to 1 and whenever a `Slice` is
dropped a bit is set to 0.

## Use case
`BufferPool` has been developed to be used in proxies with thousand of connection, each connection
must parse a particular data format via a `Decoder` each decoder use 1 or 2 `Buffer` for each received
message. With `BufferPool` each connection can be instantiated with its own `BufferPool` and reuse
the space freed by old messages for new ones.

## Unsafe
There are 5 unsafes:
buffer_pool/mod.rs 550
slice.rs 8
slice.rs 27

## Write
Waiting for `Write` in `core` a compatible trait is used so that it can be replaced.

## Buffer
The `Buffer` trait has been written to work with `codec_sv2::Decoder`.

`codec_sv2::Decoder` works by:
1. fill a buffer of the size of the header of the protocol that is decoding
2. parse the filled bytes and compute the message length
3. fill a buffer of the size of the message
4. use the header and the message to construct a `frame_sv2::Frame`

To fill the buffer `Decoder` must pass a reference of the buffer to a filler. In order
to construct a `Frame` the `Decoder` must pass the ownership of the buffer to `Frame`.

```rust
get_writable(&mut self, len: usize) -> &mut [u8]
```
Return a mutable reference to the buffer, starting at buffer length and ending at buffer length + `len`.
and set buffer len at previous len + len.

```rust
get_data_owned(&mut self) -> Slice {
```
It returns `Slice`: something that implements `AsMut[u8]` and `Send`, and sets the buffer len to 0.

## BufferFromSystemMemory
Is the simplest implementation of a `Buffer`: each time that a new buffer is needed it create a new
`Vec<u8>`.

`get_writable(..)` returns mutable references to the inner vector.

`get_data_owned(..)` returns the inner vector.


## BufferPool
Usually `BufferFromSystemMemory` should be enough, but sometimes it is better to use something faster.

For each Sv2 connection, there is a `Decoder` and for each decoder, there are 1 or 2 buffers.

Proxies and pools with thousands of connections should use `Decoder<BufferPool>` rather than
`Decoder<BufferFromSystemMemory>`

`BufferPool` when created preallocate a user-defined capacity of bytes in the heap using a
`Vec<u8>`, then when `get_data_owned(..)` is called it create a `Slice` that contains a view into
the preallocated memory. `BufferPool` guarantees that slices never overlap and the uniqueness of
the `Slice` ownership.

`Slice` implements `Drop` so that the view into the preallocated memory can be reused.

### Fragmentation overflow and optimization
`BufferPool` can allocate a maximum of 8 `Slices` (cause it uses an `AtomicU8` to keep track of the
used and freed slots) and at maximum `capacity` bytes. Whenever all the 8 slots are tacked or there
is no more space on the preallocated memory `BufferPool` failover to a `BufferFromSystemMemory`.

Usually, a message is decoded then a response is sent then a new message is decoded, etc.
So `BufferPool` is optimized for use all the slots then check if the first slot has been dropped
If so use it, then check if the second slot has been dropped, and so on.
`BufferPool` is also optimized to drop all the slices.
`BufferPool` is also optimized to drop the last slice.

Below a graphical representation of the most optimized cases:
```
A number [0f] means that the slot is taken, the minus symbol (-) means the slot is free
There are 8 slot

CASE 1
--------  BACK MODE
1-------  BACK MODE
12------  BACK MODE
123-----  BACK MODE
1234----  BACK MODE
12345---  BACK MODE
123456--  BACK MODE
1234567-  BACK MODE
12345678  BACK MODE
12345698  BACK MODE
1234569a  BACK MODE
123456ba  BACK MODE
123456ca  BACK MODE
..... and so on

CASE 2
--------  BACK MODE
1-------  BACK MODE
12------  BACK MODE
123-----  BACK MODE
1234----  BACK MODE
12345---  BACK MODE
123456--  BACK MODE
1234567-  BACK MODE
12345678  BACK MODE
--------  RESET
9a------  BACK MODE
9ab-----  BACK MODE

CASE 3
-------- BACK MODE
1------- BACK MODE
12------ BACK MODE
123----- BACK MODE
1234---- BACK MODE
12345--- BACK MODE
123456-- BACK MODE
1234567- BACK MODE
12345678 BACK MODE
12345678 BACK MODE
--345678 SWITCH TO FRONT MODE
92345678 FRONT MODE
9a345678 FRONT MODE
9a3456-- SWITCH TO BACK MODE
9a3456b- BACK MODE
9a3456bc BACK MODE

```

`BufferPool` can operate in three modalities:
1. Back: it allocates in the back of the inner vector
2. Front: it allocates in the front of the inner vector
3. Alloc: failover to `BufferFromSystemMemory`

`BufferPool` can be fragmented only between front and back and between back and end.

### Performance

To run the benchmarks `cargo bench --features criterion`.

To have an idea of the performance gains, `BufferPool` is benchmarked against
`BufferFromSystemMemory` and two control structures `PPool` and `MaxEfficeincy`.

`PPool` is a buffer pool implemented with a hashmap and `MaxEfficeincy` is a `Buffer` implemented in the
fastest possible way so that the benches do not panic and the compiler does not complain. Btw they are
both completely broken, useful only as references for the benchmarks.

The benchmarks are:

#### Single thread
```
for 0..1000:
  add random bytes to the buffer
  get the buffer
  add random bytes to the buffer
  get the buffer
  drop the 2 buffer
  ```

#### Multi threads (this is the most similar to the actual use case IMHO)
```
for 0..1000:
  add random bytes to the buffer
  get the buffer
  send the buffer to another thread   -> wait 1 ms and then drop it
  add random bytes to the buffer
  get the buffer
  send the buffer to another thread   -> wait 1 ms and then drop it
  ```

#### Multi threads 2
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

#### Test
Some failing cases from fuzz.

#### From the benchmark in BENCHES.md executed for 2000 samples:
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

From the above numbers, it results that `BufferPool` always outperform the hashmap buffer pool and
the solution without a pool:

#### Single thread
If the buffer is not sent to another context `BufferPool` is 1.4 times faster than no pool and 4.3 time
faster than the hashmap pool and 5.7 times slower than max efficiency.

#### Multi threads
If the buffer is sent to other contexts  `BufferPool` is 4 times faster than no pool, 0.6 times faster
than the hashmap pool and 1.8 times slower than max efficiency.

### Fuzzy tests
Install cargo fuzz with `cargo install cargo-fuzz`

Then do `cd ./fuzz`

Run them with `cargo fuzz run slower -- -rss_limit_mb=5000000000` and
`cargo fuzz run faster -- -rss_limit_mb=5000000000`

`BufferPool` is fuzzy tested with `cargo fuzzy`. The test checks if slices created by `BufferPool`
still contain the same bytes contained at creation time after a random amount of time and after been
sent to other threads. There are 2 fuzzy test, the first (faster) it map a smaller input space to
test the most likely inputs, the second (slower) it have a bigger input space to pick "all" the
corner case. The slower also forces the buffer to be sent to different cores. I run both for several
hours without crashes.

The test must be run with `-rss_limit_mb=5000000000` cause they check `BufferPool` with capacities
from 0 to 2^32.

(1) TODO check if is always true
