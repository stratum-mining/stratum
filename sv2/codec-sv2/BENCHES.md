# codec-sv2 Benchmarks

Benchmarking suite for the codec-sv2 crate, covering encoding, decoding, serialization, Noise
protocol encryption/decryption, and buffer pool exhaustion behavior.

## Running Benchmarks

```bash
# All benchmarks (no optional features)
cargo bench

# With Noise protocol support
cargo bench --features noise_sv2

# With buffer pool allocator
cargo bench --features with_buffer_pool

# With all features (recommended for complete picture)
cargo bench --all-features

# Specific suite
cargo bench --bench encoder
cargo bench --bench decoder
cargo bench --bench noise_roundtrip --features noise_sv2
cargo bench --bench serialization
cargo bench --bench buffer_exhaustion
cargo bench --bench pool_lifecycle
```

## Benchmark Suites

### 1. Encoder (`encoder.rs`)

- **`encoder/plain`** — Single-frame encode with a plain `Encoder<T>`
- **`encoder/creation/plain`** — `Encoder::new()` overhead

With `noise_sv2` feature:
- **`encoder/noise/transport`** — Noise-encrypted frame encoding in transport mode (reuses an established session)
- **`encoder/creation/noise`** — `NoiseEncoder::new()` overhead
- **`encoder/noise/handshake/complete`** — Full 3-step Noise handshake

### 2. Decoder (`decoder.rs`)

- **`decoder/plain`** — Full decode loop: fill writable buffer, call `next_frame()` until complete
- **`decoder/creation/plain`** — `StandardDecoder::new()` overhead

### 3. Noise Roundtrip (`noise_roundtrip.rs`)

Requires `noise_sv2` feature.

- **`noise/roundtrip`** — Complete encode → encrypt → decrypt → decode cycle per iteration (fresh session each time to avoid state reuse)
- **`noise/encode_only`** — Noise encode in isolation with a persistent transport session
- **`noise/handshake/step_0`** — Initiator generates the first EllSwift key-exchange message
- **`noise/handshake/step_1`** — Responder processes step-0 and generates its response

### 4. Serialization (`serialization.rs`)

- **`serialization/frame_from_message`** — `Sv2Frame::from_message()`: stores message as `Option<T>`, no serialization yet

### 5. Buffer Pool Exhaustion (`buffer_exhaustion.rs`)

Measures latency at each stage of the `BufferPool` state machine
(`Back → Front → Alloc`) and shows where the cost cliff occurs.

### 6. Pool Lifecycle (`pool_lifecycle.rs`)

Measures how the decoder's buffer pool behaves under realistic message-processing
patterns — specifically the cost difference between **holding decoded frames**
(zero-copy, pool slots stay pinned) vs. **copying and releasing** (pool slot freed
immediately after deserialization).

Two variants appear in every group:
- **`zc_hold`** — decoded `Sv2Frame` is kept alive; pool slot is pinned for the
  lifetime of the frame.
- **`owned_release`** — payload is copied into an `OwnedMsg` and the frame is
  dropped immediately, freeing the pool slot for reuse.

Uses a custom `TrackingAllocator` (`#[global_allocator]`) to count heap allocations
and bytes allocated per run, reported to stderr by the `alloc_amplification` group.

#### Groups

- **`pool_lifecycle/deserialization_latency_vs_accumulated`** — measures the latency
  of one decode+deserialize call while `n` frames (or owned messages) are already
  held. Parameterised over `frames_held ∈ {0, 1, 2, 4, 6, 7, 8, 9, 12, 16, 32, 64}`.
  Shows how pool pressure (slots exhausted at `n = 8`) affects decode latency.

- **`pool_lifecycle/exhaustion_boundary`** — sweeps `frame_index` from 1 to
  `POOL_CAPACITY + 4` (= 12). The `n-1` preceding frames are pre-held; the benchmark
  times the `n`-th decode. The jump at `frame_index = 9` (first slot past the pool
  capacity of 8) marks the exact boundary where the pool falls back to heap
  allocation.

- **`pool_lifecycle/throughput`** — decodes and deserializes `n` messages in a tight
  loop while accumulating either held frames or owned messages.
  `n ∈ {1, 4, 8, 9, 16, 32, 64, 100, 200, 1000}`. Reports total time for the batch;
  divide by `n` for per-message cost.

- **`pool_lifecycle/copy_overhead`** — three isolated microbenchmarks on a single
  message (coinbase 64 B):
  - `acquire_and_deserialize_zc` — frame acquire + deserialization, no copy.
  - `acquire_deserialize_and_copy` — above + `OwnedMsg::from_zc()` + frame drop.
  - `copy_only` — only `OwnedMsg::from_zc()` after a pre-acquired frame (isolates
    the copy cost).

- **`pool_lifecycle/copy_overhead_by_payload_size`** — same two variants as
  `copy_overhead` but sweeps `coinbase_bytes ∈ {16, 64, 256, 1024}` to show how copy
  cost scales with payload size.

- **`pool_lifecycle/sliding_window`** — simulates a sliding window of `k` live frames
  (FIFO: oldest dropped before each new acquire) over 200 messages.
  `window_size ∈ {1, 4, 7, 8, 9, 16}`. Crossing `k = 8` forces the pool into alloc
  mode. Includes an `owned/baseline` variant where frames are always released
  immediately (zero pool pressure regardless of window size).

- **`pool_lifecycle/alloc_amplification`** — decodes `n` messages while printing
  per-run heap allocation counts and byte totals to stderr. Runs for coinbase sizes
  64 B and 1 KB, each in `zc_hold` and `owned_release` variants.
  `n ∈ {8, 9, 16, 32, 100, 1000}`. The jump between `n = 8` and `n = 9` in the
  `zc_hold` series reveals the heap amplification from pool slot exhaustion.

## Interpreting Results

Results use [criterion.rs](https://github.com/bheisler/criterion.rs):
- HTML reports in `target/criterion/`
- Each run compares against the previous stored baseline in `target/criterion/`


```rust
cargo bench --bench encoder
Gnuplot not found, using plotters backend
encoder/plain           time:   [110.87 ns 113.48 ns 116.62 ns]                          
                        change: [-5.0915% -2.8035% +0.3669%] (p = 0.08 > 0.05)
                        No change in performance detected.
Found 7 outliers among 100 measurements (7.00%)
  3 (3.00%) high mild
  4 (4.00%) high severe

encoder/creation/plain  time:   [960.61 ps 1.0010 ns 1.0416 ns]                                    
                        change: [-15.992% -14.074% -11.957%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  11 (11.00%) high mild
  2 (2.00%) high severe

cargo bench --bench decoder
Gnuplot not found, using plotters backend
decoder/plain           time:   [42.234 ns 43.800 ns 45.303 ns]                           
                        change: [-8.0265% -5.4045% -2.1627%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  4 (4.00%) high mild

decoder/creation/plain  time:   [1.1006 ns 1.1107 ns 1.1206 ns]                                    
                        change: [-2.1339% -1.5362% -0.8789%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 12 outliers among 100 measurements (12.00%)
  2 (2.00%) low mild
  9 (9.00%) high mild
  1 (1.00%) high severe

cargo bench --bench noise_roundtrip --features noise_sv2
Gnuplot not found, using plotters backend
noise/roundtrip         time:   [768.29 µs 794.08 µs 823.92 µs]                            
                        change: [-1.5071% +0.9582% +3.6829%] (p = 0.49 > 0.05)
                        No change in performance detected.
Found 5 outliers among 100 measurements (5.00%)
  3 (3.00%) high mild
  2 (2.00%) high severe

noise/encode_only       time:   [2.4948 µs 2.5411 µs 2.5968 µs]                               
                        change: [+6.9588% +9.1516% +11.297%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe

noise/handshake/step_0  time:   [104.89 µs 106.20 µs 107.83 µs]                                   
                        change: [-2.8825% -1.0092% +0.5820%] (p = 0.28 > 0.05)
                        No change in performance detected.
Found 10 outliers among 100 measurements (10.00%)
  3 (3.00%) high mild
  7 (7.00%) high severe

noise/handshake/step_1  time:   [397.62 µs 399.16 µs 400.68 µs]                                   
                        change: [-2.4291% -1.0884% +0.2469%] (p = 0.12 > 0.05)
                        No change in performance detected.
Found 6 outliers among 100 measurements (6.00%)
  1 (1.00%) low mild
  2 (2.00%) high mild
  3 (3.00%) high severe

cargo bench --bench serialization
Gnuplot not found, using plotters backend
serialization/frame_from_message                                                                             
                        time:   [2.0505 ns 2.0909 ns 2.1356 ns]
                        change: [-70.370% -69.991% -69.584%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  2 (2.00%) high mild
  5 (5.00%) high severe

cargo bench --bench buffer_exhaustion
Gnuplot not found, using plotters backend
encoder/pool_exhaustion/back_mode                                                                            
                        time:   [122.52 ns 125.57 ns 129.01 ns]
                        change: [-23.206% -20.272% -17.256%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
encoder/pool_exhaustion/alloc_mode_after_pool_exhausted                                                                             
                        time:   [125.88 ns 128.65 ns 132.16 ns]
                        change: [-17.426% -13.120% -8.2143%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 14 outliers among 100 measurements (14.00%)
  3 (3.00%) high mild
  11 (11.00%) high severe
encoder/pool_exhaustion/recovery_after_full_release                                                                             
                        time:   [137.48 ns 142.99 ns 149.91 ns]
                        change: [-11.701% -7.5142% -2.6959%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 8 outliers among 100 measurements (8.00%)
  8 (8.00%) high mild

encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/0                                                                            
                        time:   [146.14 ns 153.10 ns 163.02 ns]
                        change: [-19.844% -17.644% -14.987%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/1                                                                            
                        time:   [141.52 ns 142.48 ns 143.40 ns]
                        change: [-20.949% -18.485% -16.164%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  2 (2.00%) high mild
  2 (2.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/2                                                                             
                        time:   [157.37 ns 159.89 ns 162.92 ns]
                        change: [-1.0467% +3.2098% +8.3710%] (p = 0.18 > 0.05)
                        No change in performance detected.
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) high mild
  2 (2.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/4                                                                             
                        time:   [159.39 ns 161.81 ns 165.11 ns]
                        change: [-22.207% -19.559% -16.921%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 9 outliers among 100 measurements (9.00%)
  5 (5.00%) high mild
  4 (4.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/6                                                                             
                        time:   [170.60 ns 174.08 ns 178.28 ns]
                        change: [-19.139% -15.549% -11.934%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) high mild
  2 (2.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/7                                                                             
                        time:   [270.34 ns 276.02 ns 282.28 ns]
                        change: [-17.133% -14.460% -11.924%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  3 (3.00%) high mild
  4 (4.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/8                                                                             
                        time:   [165.16 ns 169.12 ns 174.09 ns]
                        change: [-22.623% -20.277% -17.971%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  3 (3.00%) high mild
  4 (4.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/9                                                                             
                        time:   [168.57 ns 171.42 ns 174.12 ns]
                        change: [-6.9517% -4.4596% -1.8601%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/12                                                                             
                        time:   [160.62 ns 162.32 ns 164.52 ns]
                        change: [-18.456% -14.830% -11.198%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/16                                                                             
                        time:   [161.33 ns 165.05 ns 169.68 ns]
                        change: [-7.4355% -5.8514% -4.2154%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe

decoder/pool_exhaustion/back_mode                                                                            
                        time:   [32.656 ns 32.821 ns 33.004 ns]
                        change: [-0.7253% +0.2407% +1.1953%] (p = 0.63 > 0.05)
                        No change in performance detected.
decoder/pool_exhaustion/alloc_mode_after_pool_exhausted                                                                             
                        time:   [32.818 ns 33.661 ns 34.709 ns]
                        change: [-28.065% -25.593% -23.070%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 6 outliers among 100 measurements (6.00%)
  4 (4.00%) high mild
  2 (2.00%) high severe

decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/0                                                                            
                        time:   [33.841 ns 34.371 ns 35.133 ns]
                        change: [-4.0325% -2.0133% +0.1583%] (p = 0.07 > 0.05)
                        No change in performance detected.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/1                                                                            
                        time:   [37.721 ns 38.433 ns 39.175 ns]
                        change: [+31.123% +33.013% +34.991%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/2                                                                            
                        time:   [35.203 ns 35.405 ns 35.624 ns]
                        change: [+15.664% +17.850% +19.885%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/4                                                                            
                        time:   [35.607 ns 36.364 ns 37.116 ns]
                        change: [+17.655% +19.111% +20.644%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 10 outliers among 100 measurements (10.00%)
  5 (5.00%) high mild
  5 (5.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/6                                                                            
                        time:   [38.413 ns 39.425 ns 40.584 ns]
                        change: [+25.818% +28.714% +32.336%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/7                                                                             
                        time:   [34.694 ns 35.044 ns 35.416 ns]
                        change: [+5.2417% +7.0453% +8.6133%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 6 outliers among 100 measurements (6.00%)
  3 (3.00%) low mild
  2 (2.00%) high mild
  1 (1.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/8                                                                             
                        time:   [35.740 ns 36.308 ns 37.026 ns]
                        change: [+11.951% +15.262% +18.574%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 10 outliers among 100 measurements (10.00%)
  9 (9.00%) high mild
  1 (1.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/9                                                                             
                        time:   [35.359 ns 36.258 ns 37.534 ns]
                        change: [+2.4887% +4.0192% +5.9295%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 7 outliers among 100 measurements (7.00%)
  6 (6.00%) high mild
  1 (1.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/12                                                                             
                        time:   [33.870 ns 34.054 ns 34.251 ns]
                        change: [+4.0692% +5.0597% +6.0888%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 4 outliers among 100 measurements (4.00%)
  3 (3.00%) high mild
  1 (1.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/16                                                                             
                        time:   [36.202 ns 36.990 ns 37.781 ns]
                        change: [+7.7199% +9.5525% +11.288%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 8 outliers among 100 measurements (8.00%)
  7 (7.00%) high mild
  1 (1.00%) high severe

encoder/zerocopy_pool_exhaustion/back_mode                                                                            
                        time:   [238.60 ns 239.57 ns 240.61 ns]
                        change: [-4.6980% -1.2770% +1.9454%] (p = 0.47 > 0.05)
                        No change in performance detected.
Found 7 outliers among 100 measurements (7.00%)
  1 (1.00%) low severe
  3 (3.00%) low mild
  3 (3.00%) high severe
encoder/zerocopy_pool_exhaustion/alloc_mode_after_byte_limit_4_held                                                                             
                        time:   [235.69 ns 240.00 ns 245.47 ns]
                        change: [-6.1439% -3.8571% -1.6589%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 10 outliers among 100 measurements (10.00%)
  3 (3.00%) high mild
  7 (7.00%) high severe

encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/0                                                                            
                        time:   [230.45 ns 231.81 ns 233.24 ns]
                        change: [-1.7597% -0.6800% +0.4945%] (p = 0.26 > 0.05)
                        No change in performance detected.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/1                                                                             
                        time:   [272.86 ns 281.59 ns 291.18 ns]
                        change: [+3.5720% +8.5246% +13.574%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 7 outliers among 100 measurements (7.00%)
  6 (6.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/2                                                                             
                        time:   [243.84 ns 245.74 ns 247.75 ns]
                        change: [-42.825% -38.786% -34.354%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 9 outliers among 100 measurements (9.00%)
  3 (3.00%) low mild
  3 (3.00%) high mild
  3 (3.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/3                                                                             
                        time:   [232.96 ns 234.59 ns 236.24 ns]
                        change: [-55.059% -53.530% -51.920%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/4                                                                             
                        time:   [241.41 ns 247.77 ns 255.82 ns]
                        change: [-46.140% -44.166% -42.157%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  4 (4.00%) high mild
  3 (3.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/5                                                                             
                        time:   [233.38 ns 237.70 ns 243.43 ns]
                        change: [-44.338% -42.280% -40.204%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  1 (1.00%) high mild
  4 (4.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/7                                                                             
                        time:   [269.91 ns 274.91 ns 281.16 ns]
                        change: [+7.8963% +10.732% +13.582%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 6 outliers among 100 measurements (6.00%)
  2 (2.00%) low mild
  1 (1.00%) high mild
  3 (3.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/8                                                                             
                        time:   [286.93 ns 289.04 ns 291.28 ns]
                        change: [+28.983% +32.260% +35.447%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  1 (1.00%) low mild
  2 (2.00%) high mild
  2 (2.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/12                                                                             
                        time:   [253.13 ns 256.16 ns 259.54 ns]
                        change: [+23.953% +27.161% +30.718%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 4 outliers among 100 measurements (4.00%)
  4 (4.00%) high severe

decoder/zerocopy_pool_exhaustion/back_mode                                                                            
                        time:   [98.343 ns 100.20 ns 102.14 ns]
                        change: [+38.793% +41.835% +45.578%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe
decoder/zerocopy_pool_exhaustion/alloc_mode_8_zc_frames_held                                                                             
                        time:   [121.67 ns 123.14 ns 124.86 ns]
                        change: [+35.239% +37.013% +39.016%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 6 outliers among 100 measurements (6.00%)
  5 (5.00%) high mild
  1 (1.00%) high severe

decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/0                                                                            
                        time:   [89.667 ns 92.756 ns 96.231 ns]
                        change: [+10.012% +13.133% +16.497%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/1                                                                            
                        time:   [95.465 ns 99.561 ns 104.38 ns]
                        change: [+41.944% +47.413% +52.291%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/2                                                                            
                        time:   [74.256 ns 74.793 ns 75.375 ns]
                        change: [-1.0748% -0.5495% -0.0265%] (p = 0.05 > 0.05)
                        No change in performance detected.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/4                                                                             
                        time:   [90.003 ns 91.879 ns 93.753 ns]
                        change: [+16.288% +17.515% +18.894%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 8 outliers among 100 measurements (8.00%)
  6 (6.00%) high mild
  2 (2.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/6                                                                             
                        time:   [79.229 ns 79.513 ns 79.804 ns]
                        change: [+4.6766% +5.2474% +5.7614%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/7                                                                             
                        time:   [79.243 ns 80.667 ns 82.148 ns]
                        change: [-5.6849% -4.7611% -3.7344%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 19 outliers among 100 measurements (19.00%)
  6 (6.00%) high mild
  13 (13.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/8                                                                             
                        time:   [84.604 ns 85.909 ns 87.451 ns]
                        change: [+11.507% +13.894% +16.379%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/9                                                                             
                        time:   [86.963 ns 87.129 ns 87.324 ns]
                        change: [-2.0404% -1.7293% -1.4276%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 19 outliers among 100 measurements (19.00%)
  6 (6.00%) high mild
  13 (13.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/12                                                                             
                        time:   [76.404 ns 76.760 ns 77.163 ns]
                        change: [-3.4392% -2.9208% -2.3913%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  4 (4.00%) high mild
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/16                                                                             
                        time:   [91.781 ns 94.343 ns 96.804 ns]
                        change: [+12.632% +14.255% +16.312%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 13 outliers among 100 measurements (13.00%)
  5 (5.00%) high mild
  8 (8.00%) high severe

encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/0                                                                            
                        time:   [129.77 ns 130.40 ns 131.04 ns]
                        change: [-6.5996% -5.9352% -5.2728%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/4                                                                             
                        time:   [134.11 ns 134.64 ns 135.25 ns]
                        change: [-4.4031% -3.3734% -1.9566%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 10 outliers among 100 measurements (10.00%)
  4 (4.00%) high mild
  6 (6.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/7                                                                             
                        time:   [210.27 ns 210.90 ns 211.56 ns]
                        change: [-6.1469% -5.0162% -3.3891%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/8                                                                             
                        time:   [137.54 ns 141.65 ns 146.58 ns]
                        change: [-23.822% -21.278% -18.580%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) high mild
  3 (3.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/9                                                                             
                        time:   [142.34 ns 143.49 ns 144.66 ns]
                        change: [-18.484% -16.831% -15.044%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  3 (3.00%) high mild
  2 (2.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/0                                                                            
                        time:   [181.52 ns 182.68 ns 183.86 ns]
                        change: [-4.4772% -3.5675% -2.7695%] (p = 0.00 < 0.05)
                        Performance has improved.
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/3                                                                             
                        time:   [199.72 ns 206.41 ns 213.18 ns]
                        change: [+1.1839% +2.8039% +4.7675%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 15 outliers among 100 measurements (15.00%)
  8 (8.00%) high mild
  7 (7.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/4                                                                             
                        time:   [215.42 ns 220.15 ns 225.43 ns]
                        change: [+17.025% +18.899% +21.122%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/5                                                                             
                        time:   [203.07 ns 205.52 ns 208.04 ns]
                        change: [+4.8019% +6.1938% +7.3700%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/8                                                                             
                        time:   [185.11 ns 187.72 ns 190.94 ns]
                        change: [-2.0329% -0.8008% +0.5562%] (p = 0.24 > 0.05)
                        No change in performance detected.
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) high mild
  2 (2.00%) high severe

encoder/zerocopy_payload_size_vs_exhaustion/back_mode/coinbase_bytes/16                                                                            
                        time:   [189.68 ns 192.42 ns 195.60 ns]
                        change: [+6.5959% +8.4032% +10.704%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/alloc_mode/coinbase_bytes/16                                                                             
                        time:   [186.01 ns 189.04 ns 192.46 ns]
                        change: [-7.1005% -6.1418% -5.0036%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/back_mode/coinbase_bytes/64                                                                            
                        time:   [198.91 ns 200.89 ns 203.04 ns]
                        change: [+8.6021% +9.6433% +10.683%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
encoder/zerocopy_payload_size_vs_exhaustion/alloc_mode/coinbase_bytes/64                                                                             
                        time:   [206.07 ns 213.46 ns 220.79 ns]
                        change: [+3.3025% +5.3091% +7.4220%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 7 outliers among 100 measurements (7.00%)
  6 (6.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/back_mode/coinbase_bytes/128                                                                            
                        time:   [188.46 ns 192.07 ns 196.61 ns]
                        change: [+3.7461% +8.1458% +12.406%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
encoder/zerocopy_payload_size_vs_exhaustion/alloc_mode/coinbase_bytes/128                                                                             
                        time:   [224.94 ns 231.82 ns 239.51 ns]
                        change: [-0.7326% +1.8547% +4.7770%] (p = 0.21 > 0.05)
                        No change in performance detected.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/back_mode/coinbase_bytes/200                                                                            
                        time:   [184.21 ns 187.89 ns 191.49 ns]
                        change: [-8.5289% -7.2503% -5.8519%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  8 (8.00%) high mild
  5 (5.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/alloc_mode/coinbase_bytes/200                                                                             
                        time:   [195.22 ns 197.90 ns 201.00 ns]
                        change: [-4.7766% -3.0855% -1.4934%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe

cargo bench --bench pool_lifecycle

Gnuplot not found, using plotters backend
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/0
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/0: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/0: Collecting 100 samples in estimated 5.0018 s (12M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/0: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/0
                        time:   [390.25 ns 393.93 ns 397.42 ns]
                        change: [-1.2726% +1.4757% +3.8881%] (p = 0.28 > 0.05)
                        No change in performance detected.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/1
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/1: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/1: Collecting 100 samples in estimated 5.0002 s (6.8M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/1: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/1
                        time:   [443.05 ns 450.51 ns 458.32 ns]
                        change: [+14.593% +16.844% +18.901%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/2
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/2: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/2: Collecting 100 samples in estimated 5.0018 s (6.6M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/2: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/2
                        time:   [405.93 ns 410.02 ns 415.33 ns]
                        change: [-29.725% -27.112% -24.491%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) high mild
  3 (3.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/4
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/4: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/4: Collecting 100 samples in estimated 5.0008 s (5.3M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/4: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/4
                        time:   [391.07 ns 391.95 ns 392.71 ns]
                        change: [-17.371% -16.590% -15.822%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 33 outliers among 100 measurements (33.00%)
  18 (18.00%) low severe
  1 (1.00%) low mild
  6 (6.00%) high mild
  8 (8.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/6
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/6: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/6: Collecting 100 samples in estimated 5.0028 s (3.8M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/6: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/6
                        time:   [512.04 ns 532.96 ns 556.49 ns]
                        change: [+5.6400% +8.0196% +10.955%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) high mild
  3 (3.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/7
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/7: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/7: Collecting 100 samples in estimated 5.0008 s (3.5M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/7: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/7
                        time:   [435.40 ns 454.17 ns 476.50 ns]
                        change: [-13.082% -10.891% -8.5928%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  2 (2.00%) high mild
  2 (2.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/8
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/8: Collecting 100 samples in estimated 5.0048 s (2.8M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/8: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/8
                        time:   [426.16 ns 430.01 ns 434.04 ns]
                        change: [-15.442% -12.939% -10.593%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/9
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/9: Collecting 100 samples in estimated 5.0026 s (2.9M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/9: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/9
                        time:   [422.68 ns 427.87 ns 433.43 ns]
                        change: [-14.104% -11.799% -9.5279%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/12
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/12: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/12: Collecting 100 samples in estimated 5.0089 s (2.1M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/12: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/12
                        time:   [429.45 ns 437.20 ns 446.45 ns]
                        change: [-7.1077% -2.4557% +2.5705%] (p = 0.34 > 0.05)
                        No change in performance detected.
Found 6 outliers among 100 measurements (6.00%)
  6 (6.00%) high mild
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/16
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/16: Collecting 100 samples in estimated 5.0050 s (1.9M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/16: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/16
                        time:   [393.44 ns 396.04 ns 399.55 ns]
                        change: [-23.771% -22.581% -21.413%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 20 outliers among 100 measurements (20.00%)
  3 (3.00%) low mild
  12 (12.00%) high mild
  5 (5.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/32
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/32: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/32: Collecting 100 samples in estimated 5.0029 s (1.1M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/32: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/32
                        time:   [396.60 ns 397.64 ns 398.67 ns]
                        change: [-19.515% -18.695% -17.946%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 20 outliers among 100 measurements (20.00%)
  6 (6.00%) low severe
  2 (2.00%) low mild
  3 (3.00%) high mild
  9 (9.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/64
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/64: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/64: Collecting 100 samples in estimated 5.0178 s (581k iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/64: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/zc_hold/frames_held/64
                        time:   [449.60 ns 451.32 ns 453.13 ns]
                        change: [-8.5203% -7.7002% -6.8907%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  3 (3.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/0
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/0: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/0: Collecting 100 samples in estimated 5.0007 s (9.5M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/0: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/0
                        time:   [443.32 ns 444.24 ns 445.05 ns]
                        change: [-6.0597% -5.0479% -4.1621%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 27 outliers among 100 measurements (27.00%)
  2 (2.00%) low severe
  2 (2.00%) low mild
  19 (19.00%) high mild
  4 (4.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/1
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/1: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/1: Collecting 100 samples in estimated 5.0015 s (4.9M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/1: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/1
                        time:   [519.19 ns 536.73 ns 555.13 ns]
                        change: [-7.5269% -4.4134% -1.1878%] (p = 0.01 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/2
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/2: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/2: Collecting 100 samples in estimated 5.0028 s (3.3M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/2: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/2
                        time:   [449.51 ns 456.19 ns 463.80 ns]
                        change: [-6.4367% -5.1119% -3.7370%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  5 (5.00%) high mild
  8 (8.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/4
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/4: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/4: Collecting 100 samples in estimated 5.0087 s (2.0M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/4: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/4
                        time:   [436.81 ns 438.05 ns 439.25 ns]
                        change: [-19.574% -18.351% -17.114%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 21 outliers among 100 measurements (21.00%)
  11 (11.00%) low severe
  1 (1.00%) low mild
  9 (9.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/6
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/6: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/6: Collecting 100 samples in estimated 5.0115 s (1.4M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/6: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/6
                        time:   [448.17 ns 449.93 ns 452.24 ns]
                        change: [+1.2167% +2.7296% +4.1958%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 26 outliers among 100 measurements (26.00%)
  6 (6.00%) low severe
  1 (1.00%) low mild
  2 (2.00%) high mild
  17 (17.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/7
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/7: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/7: Collecting 100 samples in estimated 5.0083 s (1.2M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/7: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/7
                        time:   [448.99 ns 449.59 ns 450.07 ns]
                        change: [+4.4499% +5.2678% +6.0334%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 13 outliers among 100 measurements (13.00%)
  5 (5.00%) low severe
  2 (2.00%) low mild
  4 (4.00%) high mild
  2 (2.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/8
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/8: Collecting 100 samples in estimated 5.0033 s (1.1M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/8: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/8
                        time:   [437.42 ns 439.17 ns 441.02 ns]
                        change: [-17.983% -16.240% -14.629%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 26 outliers among 100 measurements (26.00%)
  11 (11.00%) low severe
  2 (2.00%) low mild
  3 (3.00%) high mild
  10 (10.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/9
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/9: Collecting 100 samples in estimated 5.0103 s (1.0M iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/9: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/9
                        time:   [549.71 ns 564.19 ns 579.58 ns]
                        change: [-11.396% -7.5894% -3.4129%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/12
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/12: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/12: Collecting 100 samples in estimated 5.0192 s (626k iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/12: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/12
                        time:   [434.44 ns 435.84 ns 437.14 ns]
                        change: [+5.2013% +7.0257% +8.5582%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 30 outliers among 100 measurements (30.00%)
  18 (18.00%) low severe
  2 (2.00%) low mild
  4 (4.00%) high mild
  6 (6.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/16
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/16: Collecting 100 samples in estimated 5.0328 s (591k iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/16: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/16
                        time:   [437.05 ns 438.40 ns 439.65 ns]
                        change: [+1.8012% +2.1681% +2.5066%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 17 outliers among 100 measurements (17.00%)
  17 (17.00%) low mild
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/32
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/32: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/32: Collecting 100 samples in estimated 5.0519 s (293k iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/32: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/32
                        time:   [436.57 ns 437.94 ns 439.31 ns]
                        change: [+1.1267% +2.1308% +3.0830%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 20 outliers among 100 measurements (20.00%)
  12 (12.00%) low severe
  2 (2.00%) low mild
  2 (2.00%) high mild
  4 (4.00%) high severe
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/64
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/64: Warming up for 3.0000 s
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/64: Collecting 100 samples in estimated 5.0109 s (146k iterations)
Benchmarking pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/64: Analyzing
pool_lifecycle/deserialization_latency_vs_accumulated/owned_release/msgs_held/64
                        time:   [425.94 ns 427.41 ns 428.91 ns]
                        change: [+8.5041% +8.9851% +9.4830%] (p = 0.00 < 0.05)
                        Performance has regressed.

Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/1
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/1: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/1: Collecting 100 samples in estimated 5.0005 s (11M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/1: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/1
                        time:   [408.20 ns 409.63 ns 411.03 ns]
                        change: [-1.6092% -1.1298% -0.6656%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/1
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/1: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/1: Collecting 100 samples in estimated 5.0018 s (10M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/1: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/1
                        time:   [406.48 ns 407.99 ns 409.64 ns]
                        change: [-9.0335% -8.0902% -7.1318%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/2
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/2: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/2: Collecting 100 samples in estimated 5.0003 s (8.1M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/2: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/2
                        time:   [407.92 ns 409.33 ns 410.72 ns]
                        change: [-17.497% -14.262% -11.070%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/2
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/2: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/2: Collecting 100 samples in estimated 5.0017 s (5.3M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/2: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/2
                        time:   [407.00 ns 408.42 ns 409.84 ns]
                        change: [+0.7348% +1.2291% +1.7080%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/3
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/3: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/3: Collecting 100 samples in estimated 5.0029 s (6.9M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/3: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/3
                        time:   [409.06 ns 412.42 ns 416.36 ns]
                        change: [-10.094% -8.6913% -7.2581%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/3
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/3: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/3: Collecting 100 samples in estimated 5.0009 s (3.5M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/3: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/3
                        time:   [409.13 ns 410.72 ns 412.36 ns]
                        change: [+1.3393% +2.0505% +2.7574%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 4 outliers among 100 measurements (4.00%)
  3 (3.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/4
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/4: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/4: Collecting 100 samples in estimated 5.0041 s (5.9M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/4: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/4
                        time:   [407.89 ns 409.54 ns 411.47 ns]
                        change: [-2.6071% -0.9017% +1.1415%] (p = 0.39 > 0.05)
                        No change in performance detected.
Found 7 outliers among 100 measurements (7.00%)
  3 (3.00%) high mild
  4 (4.00%) high severe
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/4
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/4: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/4: Collecting 100 samples in estimated 5.0055 s (2.7M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/4: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/4
                        time:   [407.02 ns 409.27 ns 411.97 ns]
                        change: [-3.5183% -2.8058% -2.1141%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/5
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/5: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/5: Collecting 100 samples in estimated 5.0001 s (5.1M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/5: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/5
                        time:   [405.62 ns 406.84 ns 408.10 ns]
                        change: [-1.4527% -0.9488% -0.4635%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/5
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/5: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/5: Collecting 100 samples in estimated 5.0036 s (2.2M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/5: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/5
                        time:   [410.78 ns 412.09 ns 413.37 ns]
                        change: [-3.4682% -2.6693% -1.9036%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/6
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/6: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/6: Collecting 100 samples in estimated 5.0040 s (4.7M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/6: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/6
                        time:   [408.89 ns 410.19 ns 411.51 ns]
                        change: [-4.2699% -3.6272% -2.9833%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/6
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/6: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/6: Collecting 100 samples in estimated 5.0054 s (1.8M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/6: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/6
                        time:   [414.55 ns 419.57 ns 425.57 ns]
                        change: [-15.741% -13.572% -11.502%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 35 outliers among 100 measurements (35.00%)
  18 (18.00%) low severe
  1 (1.00%) low mild
  1 (1.00%) high mild
  15 (15.00%) high severe
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/7
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/7: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/7: Collecting 100 samples in estimated 5.0030 s (4.3M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/7: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/7
                        time:   [405.62 ns 406.70 ns 407.85 ns]
                        change: [-7.4848% -6.8225% -6.1451%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/7
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/7: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/7: Collecting 100 samples in estimated 5.0132 s (1.5M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/7: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/7
                        time:   [408.56 ns 409.91 ns 411.23 ns]
                        change: [-11.665% -10.736% -9.8590%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/8
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/8: Collecting 100 samples in estimated 5.0032 s (3.9M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/8: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/8
                        time:   [407.73 ns 409.42 ns 411.24 ns]
                        change: [+1.1933% +1.6763% +2.1410%] (p = 0.00 < 0.05)
                        Performance has regressed.
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/8
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/8: Collecting 100 samples in estimated 5.0136 s (1.4M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/8: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/8
                        time:   [410.23 ns 412.34 ns 415.07 ns]
                        change: [-2.8479% -2.0692% -1.2066%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/9
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/9: Collecting 100 samples in estimated 5.0041 s (3.4M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/9: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/9
                        time:   [407.75 ns 409.12 ns 410.49 ns]
                        change: [-6.5460% -5.5884% -4.6312%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/9
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/9: Collecting 100 samples in estimated 5.0006 s (1.2M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/9: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/9
                        time:   [408.67 ns 410.00 ns 411.29 ns]
                        change: [-4.5226% -3.6995% -2.9060%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/10
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/10: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/10: Collecting 100 samples in estimated 5.0028 s (3.2M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/10: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/10
                        time:   [407.47 ns 408.79 ns 410.17 ns]
                        change: [+0.3974% +1.2190% +1.9781%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/10
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/10: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/10: Collecting 100 samples in estimated 5.0022 s (1.1M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/10: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/10
                        time:   [407.83 ns 409.09 ns 410.32 ns]
                        change: [-2.0790% -1.4172% -0.7451%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/11
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/11: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/11: Collecting 100 samples in estimated 5.0071 s (3.0M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/11: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/11
                        time:   [406.18 ns 407.35 ns 408.61 ns]
                        change: [-12.503% -11.044% -9.6611%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/11
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/11: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/11: Collecting 100 samples in estimated 5.0140 s (990k iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/11: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/11
                        time:   [409.38 ns 410.55 ns 411.65 ns]
                        change: [-14.362% -13.016% -11.741%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/12
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/12: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/12: Collecting 100 samples in estimated 5.0022 s (2.4M iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/12: Analyzing
pool_lifecycle/exhaustion_boundary/zc_hold/frame_index/12
                        time:   [487.68 ns 495.00 ns 503.03 ns]
                        change: [-7.1604% -2.6516% +1.9874%] (p = 0.27 > 0.05)
                        No change in performance detected.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/12
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/12: Warming up for 3.0000 s
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/12: Collecting 100 samples in estimated 5.0030 s (853k iterations)
Benchmarking pool_lifecycle/exhaustion_boundary/owned_release/frame_index/12: Analyzing
pool_lifecycle/exhaustion_boundary/owned_release/frame_index/12
                        time:   [426.88 ns 435.17 ns 445.30 ns]
                        change: [+4.6926% +6.9276% +9.5118%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild

Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/1
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/1: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/1: Collecting 100 samples in estimated 5.0017 s (10M iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/1: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/1
                        time:   [408.05 ns 409.39 ns 410.76 ns]
                        change: [-6.1338% -5.3512% -4.6006%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/1
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/1: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/1: Collecting 100 samples in estimated 5.0023 s (9.7M iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/1: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/1
                        time:   [442.28 ns 443.71 ns 445.13 ns]
                        change: [-3.7824% -3.1289% -2.4869%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 17 outliers among 100 measurements (17.00%)
  14 (14.00%) low mild
  2 (2.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/4
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/4: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/4: Collecting 100 samples in estimated 5.0041 s (2.8M iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/4: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/4
                        time:   [1.6009 µs 1.6070 µs 1.6134 µs]
                        change: [-5.5006% -3.6927% -1.9571%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 29 outliers among 100 measurements (29.00%)
  16 (16.00%) low severe
  2 (2.00%) low mild
  4 (4.00%) high mild
  7 (7.00%) high severe
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/4
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/4: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/4: Collecting 100 samples in estimated 5.0010 s (2.7M iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/4: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/4
                        time:   [1.7442 µs 1.7634 µs 1.7866 µs]
                        change: [+2.8927% +4.1410% +5.3743%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 40 outliers among 100 measurements (40.00%)
  22 (22.00%) low severe
  2 (2.00%) high mild
  16 (16.00%) high severe
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/8
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/8: Collecting 100 samples in estimated 5.0051 s (1.4M iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/8: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/8
                        time:   [3.2244 µs 3.2333 µs 3.2414 µs]
                        change: [-6.6777% -5.7157% -4.7685%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 31 outliers among 100 measurements (31.00%)
  19 (19.00%) low severe
  3 (3.00%) low mild
  1 (1.00%) high mild
  8 (8.00%) high severe
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/8
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/8: Collecting 100 samples in estimated 5.0140 s (1.3M iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/8: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/8
                        time:   [3.5876 µs 3.6138 µs 3.6394 µs]
                        change: [-3.3124% -2.1190% -1.0392%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/9
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/9: Collecting 100 samples in estimated 5.0127 s (1.3M iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/9: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/9
                        time:   [3.6199 µs 3.6309 µs 3.6411 µs]
                        change: [-4.7893% -4.0647% -3.3331%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 28 outliers among 100 measurements (28.00%)
  18 (18.00%) low severe
  1 (1.00%) low mild
  6 (6.00%) high mild
  3 (3.00%) high severe
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/9
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/9: Collecting 100 samples in estimated 5.0024 s (1.2M iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/9: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/9
                        time:   [3.8728 µs 3.8856 µs 3.8980 µs]
                        change: [-1.3552% -0.7835% -0.2006%] (p = 0.01 < 0.05)
                        Change within noise threshold.
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/16
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/16: Collecting 100 samples in estimated 5.0222 s (682k iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/16: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/16
                        time:   [6.4846 µs 6.5040 µs 6.5226 µs]
                        change: [-5.7442% -5.1404% -4.5440%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/16
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/16: Collecting 100 samples in estimated 5.0060 s (631k iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/16: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/16
                        time:   [7.2051 µs 7.2280 µs 7.2492 µs]
                        change: [-2.3972% -2.0215% -1.6639%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 27 outliers among 100 measurements (27.00%)
  19 (19.00%) low severe
  1 (1.00%) low mild
  3 (3.00%) high mild
  4 (4.00%) high severe
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/32
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/32: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/32: Collecting 100 samples in estimated 5.0743 s (343k iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/32: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/32
                        time:   [13.218 µs 13.266 µs 13.313 µs]
                        change: [-3.1710% -2.7132% -2.2608%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/32
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/32: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/32: Collecting 100 samples in estimated 5.0098 s (313k iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/32: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/32
                        time:   [14.573 µs 14.620 µs 14.665 µs]
                        change: [-0.9096% -0.4719% -0.0043%] (p = 0.04 < 0.05)
                        Change within noise threshold.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/64
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/64: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/64: Collecting 100 samples in estimated 5.0140 s (172k iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/64: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/64
                        time:   [26.270 µs 26.354 µs 26.434 µs]
                        change: [-7.8607% -7.2237% -6.5815%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/64
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/64: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/64: Collecting 100 samples in estimated 5.0915 s (162k iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/64: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/64
                        time:   [28.722 µs 28.815 µs 28.905 µs]
                        change: [-10.439% -9.7695% -9.1262%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/100
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/100: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/100: Collecting 100 samples in estimated 5.1938 s (116k iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/100: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/100
                        time:   [41.036 µs 41.177 µs 41.318 µs]
                        change: [-9.2107% -8.1672% -7.1232%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 32 outliers among 100 measurements (32.00%)
  20 (20.00%) low severe
  1 (1.00%) low mild
  5 (5.00%) high mild
  6 (6.00%) high severe
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/100
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/100: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/100: Collecting 100 samples in estimated 5.0294 s (101k iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/100: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/100
                        time:   [45.164 µs 45.316 µs 45.466 µs]
                        change: [+4.2539% +4.5984% +4.9652%] (p = 0.00 < 0.05)
                        Performance has regressed.
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/200
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/200: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/200: Collecting 100 samples in estimated 5.0903 s (56k iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/200: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/200
                        time:   [88.819 µs 92.917 µs 97.529 µs]
                        change: [+13.402% +15.540% +18.248%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 7 outliers among 100 measurements (7.00%)
  2 (2.00%) high mild
  5 (5.00%) high severe
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/200
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/200: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/200: Collecting 100 samples in estimated 5.2874 s (50k iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/200: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/200
                        time:   [96.291 µs 97.038 µs 97.807 µs]
                        change: [-4.9569% -2.0895% +0.6613%] (p = 0.16 > 0.05)
                        No change in performance detected.
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/1000
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/1000: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/1000: Collecting 100 samples in estimated 7.1187 s (15k iterations)
Benchmarking pool_lifecycle/throughput/zc_hold/total_messages/1000: Analyzing
pool_lifecycle/throughput/zc_hold/total_messages/1000
                        time:   [418.80 µs 422.08 µs 425.85 µs]
                        change: [-16.666% -14.411% -12.210%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  4 (4.00%) low mild
  2 (2.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/1000
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/1000: Warming up for 3.0000 s
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/1000: Collecting 100 samples in estimated 7.4341 s (15k iterations)
Benchmarking pool_lifecycle/throughput/owned_release/total_messages/1000: Analyzing
pool_lifecycle/throughput/owned_release/total_messages/1000
                        time:   [459.78 µs 465.73 µs 473.36 µs]
                        change: [-18.729% -16.753% -14.526%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 17 outliers among 100 measurements (17.00%)
  4 (4.00%) high mild
  13 (13.00%) high severe

Benchmarking pool_lifecycle/copy_overhead/acquire_and_deserialize_zc
Benchmarking pool_lifecycle/copy_overhead/acquire_and_deserialize_zc: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead/acquire_and_deserialize_zc: Collecting 100 samples in estimated 5.0016 s (11M iterations)
Benchmarking pool_lifecycle/copy_overhead/acquire_and_deserialize_zc: Analyzing
pool_lifecycle/copy_overhead/acquire_and_deserialize_zc
                        time:   [409.68 ns 411.05 ns 412.41 ns]
                        change: [-2.1823% -0.9670% +0.2555%] (p = 0.13 > 0.05)
                        No change in performance detected.
Benchmarking pool_lifecycle/copy_overhead/acquire_deserialize_and_copy
Benchmarking pool_lifecycle/copy_overhead/acquire_deserialize_and_copy: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead/acquire_deserialize_and_copy: Collecting 100 samples in estimated 5.0022 s (10M iterations)
Benchmarking pool_lifecycle/copy_overhead/acquire_deserialize_and_copy: Analyzing
pool_lifecycle/copy_overhead/acquire_deserialize_and_copy
                        time:   [453.45 ns 455.66 ns 457.87 ns]
                        change: [-3.7324% -2.2499% -0.8774%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Benchmarking pool_lifecycle/copy_overhead/copy_only
Benchmarking pool_lifecycle/copy_overhead/copy_only: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead/copy_only: Collecting 100 samples in estimated 5.0019 s (10M iterations)
Benchmarking pool_lifecycle/copy_overhead/copy_only: Analyzing
pool_lifecycle/copy_overhead/copy_only
                        time:   [341.45 ns 342.53 ns 343.67 ns]
                        change: [-27.707% -25.351% -22.818%] (p = 0.00 < 0.05)
                        Performance has improved.

Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/16
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/16: Collecting 100 samples in estimated 5.0008 s (11M iterations)
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/16: Analyzing
pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/16
                        time:   [407.86 ns 410.56 ns 414.28 ns]
                        change: [-18.855% -15.647% -12.427%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/16
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/16: Collecting 100 samples in estimated 5.0004 s (8.8M iterations)
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/16: Analyzing
pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/16
                        time:   [566.47 ns 579.56 ns 592.98 ns]
                        change: [+9.6627% +12.222% +15.185%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/64
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/64: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/64: Collecting 100 samples in estimated 5.0000 s (9.2M iterations)
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/64: Analyzing
pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/64
                        time:   [419.75 ns 424.84 ns 430.59 ns]
                        change: [-6.5074% -4.9658% -3.4588%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 37 outliers among 100 measurements (37.00%)
  15 (15.00%) low severe
  2 (2.00%) low mild
  1 (1.00%) high mild
  19 (19.00%) high severe
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/64
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/64: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/64: Collecting 100 samples in estimated 5.0004 s (9.6M iterations)
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/64: Analyzing
pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/64
                        time:   [462.92 ns 467.43 ns 472.58 ns]
                        change: [-17.822% -15.298% -12.939%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 37 outliers among 100 measurements (37.00%)
  21 (21.00%) low mild
  5 (5.00%) high mild
  11 (11.00%) high severe
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/256
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/256: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/256: Collecting 100 samples in estimated 5.0001 s (9.8M iterations)
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/256: Analyzing
pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/256
                        time:   [441.87 ns 446.64 ns 452.24 ns]
                        change: [-28.860% -24.688% -20.189%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 10 outliers among 100 measurements (10.00%)
  3 (3.00%) high mild
  7 (7.00%) high severe
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/256
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/256: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/256: Collecting 100 samples in estimated 5.0012 s (9.2M iterations)
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/256: Analyzing
pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/256
                        time:   [492.71 ns 498.03 ns 504.18 ns]
                        change: [-28.677% -26.826% -25.120%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 12 outliers among 100 measurements (12.00%)
  6 (6.00%) high mild
  6 (6.00%) high severe
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/1024
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/1024: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/1024: Collecting 100 samples in estimated 5.0027 s (7.9M iterations)
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/1024: Analyzing
pool_lifecycle/copy_overhead_by_payload_size/acquire_and_deserialize_zc/coinbase_bytes/1024
                        time:   [553.02 ns 558.00 ns 563.91 ns]
                        change: [+8.3346% +11.174% +13.761%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 17 outliers among 100 measurements (17.00%)
  3 (3.00%) high mild
  14 (14.00%) high severe
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/1024
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/1024: Warming up for 3.0000 s
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/1024: Collecting 100 samples in estimated 5.0018 s (7.5M iterations)
Benchmarking pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/1024: Analyzing
pool_lifecycle/copy_overhead_by_payload_size/acquire_deserialize_and_copy/coinbase_bytes/1024
                        time:   [614.48 ns 619.82 ns 626.18 ns]
                        change: [+3.5055% +5.5876% +7.6454%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 19 outliers among 100 measurements (19.00%)
  2 (2.00%) low mild
  6 (6.00%) high mild
  11 (11.00%) high severe

Benchmarking pool_lifecycle/sliding_window/zc/window_size/1
Benchmarking pool_lifecycle/sliding_window/zc/window_size/1: Warming up for 3.0000 s
Benchmarking pool_lifecycle/sliding_window/zc/window_size/1: Collecting 100 samples in estimated 5.0668 s (50k iterations)
Benchmarking pool_lifecycle/sliding_window/zc/window_size/1: Analyzing
pool_lifecycle/sliding_window/zc/window_size/1
                        time:   [423.56 ns 429.78 ns 436.66 ns]
                        thrpt:  [2.2901 Melem/s 2.3268 Melem/s 2.3609 Melem/s]
                 change:
                        time:   [-8.0545% -6.0758% -4.2703%] (p = 0.00 < 0.05)
                        thrpt:  [+4.4608% +6.4689% +8.7601%]
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  2 (2.00%) high mild
  5 (5.00%) high severe
Benchmarking pool_lifecycle/sliding_window/zc/window_size/4
Benchmarking pool_lifecycle/sliding_window/zc/window_size/4: Warming up for 3.0000 s
Benchmarking pool_lifecycle/sliding_window/zc/window_size/4: Collecting 100 samples in estimated 5.4900 s (56k iterations)
Benchmarking pool_lifecycle/sliding_window/zc/window_size/4: Analyzing
pool_lifecycle/sliding_window/zc/window_size/4
                        time:   [417.84 ns 422.81 ns 428.12 ns]
                        thrpt:  [2.3358 Melem/s 2.3651 Melem/s 2.3932 Melem/s]
                 change:
                        time:   [-22.272% -19.062% -15.831%] (p = 0.00 < 0.05)
                        thrpt:  [+18.808% +23.551% +28.653%]
                        Performance has improved.
Found 40 outliers among 100 measurements (40.00%)
  15 (15.00%) low severe
  4 (4.00%) low mild
  3 (3.00%) high mild
  18 (18.00%) high severe
Benchmarking pool_lifecycle/sliding_window/zc/window_size/7
Benchmarking pool_lifecycle/sliding_window/zc/window_size/7: Warming up for 3.0000 s
Benchmarking pool_lifecycle/sliding_window/zc/window_size/7: Collecting 100 samples in estimated 5.0662 s (50k iterations)
Benchmarking pool_lifecycle/sliding_window/zc/window_size/7: Analyzing
pool_lifecycle/sliding_window/zc/window_size/7
                        time:   [417.60 ns 423.42 ns 429.73 ns]
                        thrpt:  [2.3270 Melem/s 2.3617 Melem/s 2.3946 Melem/s]
                 change:
                        time:   [-12.816% -10.510% -8.3860%] (p = 0.00 < 0.05)
                        thrpt:  [+9.1536% +11.744% +14.699%]
                        Performance has improved.
Found 9 outliers among 100 measurements (9.00%)
  4 (4.00%) high mild
  5 (5.00%) high severe
Benchmarking pool_lifecycle/sliding_window/zc/window_size/8
Benchmarking pool_lifecycle/sliding_window/zc/window_size/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/sliding_window/zc/window_size/8: Collecting 100 samples in estimated 5.0341 s (50k iterations)
Benchmarking pool_lifecycle/sliding_window/zc/window_size/8: Analyzing
pool_lifecycle/sliding_window/zc/window_size/8
                        time:   [420.07 ns 425.84 ns 432.17 ns]
                        thrpt:  [2.3139 Melem/s 2.3483 Melem/s 2.3805 Melem/s]
                 change:
                        time:   [+0.4390% +1.2292% +2.1387%] (p = 0.01 < 0.05)
                        thrpt:  [-2.0939% -1.2143% -0.4371%]
                        Change within noise threshold.
Found 8 outliers among 100 measurements (8.00%)
  3 (3.00%) high mild
  5 (5.00%) high severe
Benchmarking pool_lifecycle/sliding_window/zc/window_size/9
Benchmarking pool_lifecycle/sliding_window/zc/window_size/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/sliding_window/zc/window_size/9: Collecting 100 samples in estimated 5.0839 s (50k iterations)
Benchmarking pool_lifecycle/sliding_window/zc/window_size/9: Analyzing
pool_lifecycle/sliding_window/zc/window_size/9
                        time:   [423.04 ns 428.23 ns 433.98 ns]
                        thrpt:  [2.3043 Melem/s 2.3352 Melem/s 2.3639 Melem/s]
                 change:
                        time:   [+0.0298% +1.2392% +2.4989%] (p = 0.05 < 0.05)
                        thrpt:  [-2.4380% -1.2240% -0.0298%]
                        Change within noise threshold.
Found 12 outliers among 100 measurements (12.00%)
  4 (4.00%) high mild
  8 (8.00%) high severe
Benchmarking pool_lifecycle/sliding_window/zc/window_size/16
Benchmarking pool_lifecycle/sliding_window/zc/window_size/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/sliding_window/zc/window_size/16: Collecting 100 samples in estimated 5.0871 s (50k iterations)
Benchmarking pool_lifecycle/sliding_window/zc/window_size/16: Analyzing
pool_lifecycle/sliding_window/zc/window_size/16
                        time:   [422.12 ns 431.83 ns 443.30 ns]
                        thrpt:  [2.2558 Melem/s 2.3157 Melem/s 2.3690 Melem/s]
                 change:
                        time:   [-6.0511% -3.9985% -2.0244%] (p = 0.00 < 0.05)
                        thrpt:  [+2.0663% +4.1651% +6.4408%]
                        Performance has improved.
Found 17 outliers among 100 measurements (17.00%)
  7 (7.00%) high mild
  10 (10.00%) high severe
Benchmarking pool_lifecycle/sliding_window/owned/baseline
Benchmarking pool_lifecycle/sliding_window/owned/baseline: Warming up for 3.0000 s
Benchmarking pool_lifecycle/sliding_window/owned/baseline: Collecting 100 samples in estimated 5.1476 s (45k iterations)
Benchmarking pool_lifecycle/sliding_window/owned/baseline: Analyzing
pool_lifecycle/sliding_window/owned/baseline
                        time:   [486.39 ns 492.82 ns 499.95 ns]
                        thrpt:  [2.0002 Melem/s 2.0291 Melem/s 2.0560 Melem/s]
                 change:
                        time:   [+0.8821% +2.0469% +3.2500%] (p = 0.00 < 0.05)
                        thrpt:  [-3.1477% -2.0059% -0.8744%]
                        Change within noise threshold.
Found 9 outliers among 100 measurements (9.00%)
  3 (3.00%) high mild
  6 (6.00%) high severe

Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/8
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/8: Collecting 100 samples in estimated 5.0090 s (1.4M iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/8: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/8
                        time:   [3.5420 µs 3.6010 µs 3.6732 µs]
                        change: [+8.1123% +9.3780% +10.727%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 10 outliers among 100 measurements (10.00%)
  2 (2.00%) low mild
  6 (6.00%) high mild
  2 (2.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/8
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/8: Collecting 100 samples in estimated 5.0015 s (1.3M iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/8: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/8
                        time:   [3.7849 µs 3.8253 µs 3.8731 µs]
                        change: [+2.2980% +3.5808% +4.8640%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/8
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/8: Collecting 100 samples in estimated 5.0019 s (1.2M iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/8: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/8
                        time:   [3.9749 µs 4.0224 µs 4.0779 µs]
                        change: [+7.5239% +8.6755% +9.9549%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 9 outliers among 100 measurements (9.00%)
  2 (2.00%) low mild
  6 (6.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/8
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/8: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/8: Collecting 100 samples in estimated 5.0051 s (1.2M iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/8: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/8
                        time:   [4.2526 µs 4.3315 µs 4.4383 µs]
                        change: [+2.1547% +4.1151% +6.3939%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  3 (3.00%) high mild
  2 (2.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/9
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/9: Collecting 100 samples in estimated 5.0134 s (1.2M iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/9: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/9
                        time:   [3.9229 µs 3.9662 µs 4.0174 µs]
                        change: [-1.1847% +0.8214% +2.7922%] (p = 0.41 > 0.05)
                        No change in performance detected.
Found 6 outliers among 100 measurements (6.00%)
  6 (6.00%) high mild
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/9
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/9: Collecting 100 samples in estimated 5.0016 s (1.1M iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/9: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/9
                        time:   [4.2813 µs 4.3384 µs 4.4072 µs]
                        change: [+13.979% +15.288% +16.718%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 7 outliers among 100 measurements (7.00%)
  5 (5.00%) high mild
  2 (2.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/9
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/9: Collecting 100 samples in estimated 5.0187 s (1.1M iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/9: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/9
                        time:   [4.4517 µs 4.5246 µs 4.6208 µs]
                        change: [+13.716% +15.360% +17.586%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 8 outliers among 100 measurements (8.00%)
  1 (1.00%) low mild
  5 (5.00%) high mild
  2 (2.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/9
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/9: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/9: Collecting 100 samples in estimated 5.0234 s (1.0M iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/9: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/9
                        time:   [4.8294 µs 4.8850 µs 4.9506 µs]
                        change: [+2.3084% +4.3929% +6.4504%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 7 outliers among 100 measurements (7.00%)
  6 (6.00%) high mild
  1 (1.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/16
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/16: Collecting 100 samples in estimated 5.0255 s (611k iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/16: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/16
                        time:   [7.1634 µs 7.2763 µs 7.4057 µs]
                        change: [-2.7952% +0.7946% +4.2400%] (p = 0.67 > 0.05)
                        No change in performance detected.
Found 5 outliers among 100 measurements (5.00%)
  5 (5.00%) high mild
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/16
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/16: Collecting 100 samples in estimated 5.0116 s (667k iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/16: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/16
                        time:   [6.9679 µs 6.9914 µs 7.0151 µs]
                        change: [-13.397% -10.180% -7.0012%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 21 outliers among 100 measurements (21.00%)
  19 (19.00%) low mild
  2 (2.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/16
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/16: Collecting 100 samples in estimated 5.0081 s (616k iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/16: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/16
                        time:   [7.4276 µs 7.4527 µs 7.4795 µs]
                        change: [-8.7190% -6.5668% -4.5220%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 15 outliers among 100 measurements (15.00%)
  2 (2.00%) low severe
  6 (6.00%) high mild
  7 (7.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/16
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/16: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/16: Collecting 100 samples in estimated 5.0405 s (556k iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/16: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/16
                        time:   [8.5595 µs 8.5808 µs 8.6055 µs]
                        change: [-8.8366% -7.1152% -5.5689%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 15 outliers among 100 measurements (15.00%)
  4 (4.00%) high mild
  11 (11.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/32
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/32: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/32: Collecting 100 samples in estimated 5.0030 s (343k iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/32: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/32
                        time:   [13.316 µs 13.385 µs 13.475 µs]
                        change: [-2.9297% -0.9547% +1.1501%] (p = 0.38 > 0.05)
                        No change in performance detected.
Found 14 outliers among 100 measurements (14.00%)
  7 (7.00%) high mild
  7 (7.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/32
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/32: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/32: Collecting 100 samples in estimated 5.0196 s (313k iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/32: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/32
                        time:   [14.839 µs 15.099 µs 15.399 µs]
                        change: [+1.2236% +2.3464% +3.5966%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 12 outliers among 100 measurements (12.00%)
  3 (3.00%) high mild
  9 (9.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/32
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/32: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/32: Collecting 100 samples in estimated 5.0453 s (293k iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/32: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/32
                        time:   [14.985 µs 15.115 µs 15.283 µs]
                        change: [-11.883% -10.115% -8.3272%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 18 outliers among 100 measurements (18.00%)
  8 (8.00%) high mild
  10 (10.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/32
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/32: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/32: Collecting 100 samples in estimated 5.0087 s (278k iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/32: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/32
                        time:   [16.757 µs 16.777 µs 16.795 µs]
                        change: [-19.776% -17.574% -15.416%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  1 (1.00%) low severe
  5 (5.00%) high mild
  7 (7.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/100
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/100: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/100: Collecting 100 samples in estimated 5.1113 s (111k iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/100: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/100
                        time:   [41.461 µs 41.569 µs 41.714 µs]
                        change: [-89.693% -89.100% -88.510%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 18 outliers among 100 measurements (18.00%)
  4 (4.00%) high mild
  14 (14.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/100
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/100: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/100: Collecting 100 samples in estimated 5.0371 s (101k iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/100: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/100
                        time:   [45.841 µs 46.074 µs 46.405 µs]
                        change: [-87.960% -86.934% -85.828%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 15 outliers among 100 measurements (15.00%)
  4 (4.00%) high mild
  11 (11.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/100
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/100: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/100: Collecting 100 samples in estimated 5.1913 s (101k iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/100: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/100
                        time:   [46.486 µs 46.635 µs 46.789 µs]
                        change: [-89.823% -89.116% -88.420%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 21 outliers among 100 measurements (21.00%)
  9 (9.00%) low severe
  1 (1.00%) low mild
  3 (3.00%) high mild
  8 (8.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/100
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/100: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/100: Collecting 100 samples in estimated 5.1497 s (91k iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/100: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/100
                        time:   [55.362 µs 55.945 µs 56.665 µs]
                        change: [-85.576% -84.394% -83.129%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 27 outliers among 100 measurements (27.00%)
  14 (14.00%) low severe
  2 (2.00%) low mild
  3 (3.00%) high mild
  8 (8.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/1000
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/1000: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/1000: Collecting 100 samples in estimated 6.8304 s (15k iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/1000: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_64B/n_msgs/1000
                        time:   [411.97 µs 413.19 µs 414.39 µs]
                        change: [-89.576% -88.879% -88.138%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/1000
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/1000: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/1000: Collecting 100 samples in estimated 7.4835 s (15k iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/1000: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_64B/n_msgs/1000
                        time:   [457.51 µs 463.01 µs 470.69 µs]
                        change: [-85.852% -84.926% -83.949%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) high mild
  2 (2.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/1000
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/1000: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/1000: Collecting 100 samples in estimated 5.2742 s (10k iterations)
Benchmarking pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/1000: Analyzing
pool_lifecycle/alloc_amplification/zc_hold/coinbase_1KB/n_msgs/1000
                        time:   [475.54 µs 476.72 µs 477.96 µs]
                        change: [-88.785% -88.002% -87.178%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 16 outliers among 100 measurements (16.00%)
  2 (2.00%) low mild
  5 (5.00%) high mild
  9 (9.00%) high severe
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/1000
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/1000: Warming up for 3.0000 s
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/1000: Collecting 100 samples in estimated 6.0814 s (10k iterations)
Benchmarking pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/1000: Analyzing
pool_lifecycle/alloc_amplification/owned_release/coinbase_1KB/n_msgs/1000
                        time:   [581.55 µs 593.32 µs 608.35 µs]
                        change: [-84.546% -83.358% -82.076%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 16 outliers among 100 measurements (16.00%)
  4 (4.00%) high mild
  12 (12.00%) high severe
```