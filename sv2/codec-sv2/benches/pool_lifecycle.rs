extern crate alloc;

use binary_sv2;
use codec_sv2::StandardDecoder;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use framing_sv2::framing::Sv2Frame;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::VecDeque,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static HEAP_ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static HEAP_ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HEAP_ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        HEAP_ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        HEAP_ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        HEAP_ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: TrackingAllocator = TrackingAllocator;

#[inline(always)]
fn reset_alloc_counters() {
    HEAP_ALLOC_COUNT.store(0, Ordering::Relaxed);
    HEAP_ALLOC_BYTES.store(0, Ordering::Relaxed);
}

#[inline(always)]
fn read_alloc_counters() -> (u64, u64) {
    (
        HEAP_ALLOC_COUNT.load(Ordering::Relaxed),
        HEAP_ALLOC_BYTES.load(Ordering::Relaxed),
    )
}

mod common;
use common::{OwnedMsg, ZeroCopyMsg};

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

type DecodedFrame = Sv2Frame<ZeroCopyMsg<'static>, Slice>;

const COINBASE_SIZES: &[usize] = &[16, 64, 256, 1024];

const DEFAULT_COINBASE: usize = 64;

const POOL_CAPACITY: usize = 8;

fn make_encoded_frame(coinbase_size: usize) -> Vec<u8> {
    let msg = ZeroCopyMsg::new_owned(1, coinbase_size);
    let frame = Sv2Frame::<ZeroCopyMsg<'_>, Vec<u8>>::from_message(msg, 0, 0, true).unwrap();
    let mut buf = vec![0u8; frame.encoded_length()];
    frame.serialize(&mut buf).unwrap();
    buf
}

fn acquire_frame(dec: &mut StandardDecoder<ZeroCopyMsg<'static>>, enc_buf: &[u8]) -> DecodedFrame {
    let w = dec.writable();
    let header_len = w.len();
    w.copy_from_slice(&enc_buf[..header_len]);
    let mut offset = header_len;
    loop {
        match dec.next_frame() {
            Ok(frame) => return frame,
            Err(codec_sv2::Error::MissingBytes(n)) => {
                let w = dec.writable();
                w.copy_from_slice(&enc_buf[offset..offset + n]);
                offset += n;
            }
            Err(e) => panic!("decode error: {:?}", e),
        }
    }
}

fn bench_deserialization_latency_vs_accumulated(c: &mut Criterion) {
    let enc_buf = make_encoded_frame(DEFAULT_COINBASE);

    let held_counts: &[usize] = &[0, 1, 2, 4, 6, 7, 8, 9, 12, 16, 32, 64];

    let mut group = c.benchmark_group("pool_lifecycle/deserialization_latency_vs_accumulated");

    for &n in held_counts {
        group.bench_with_input(BenchmarkId::new("zc_hold/frames_held", n), &n, |b, &n| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;

                for _ in 0..iters {
                    let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();

                    let held_frames: Vec<DecodedFrame> =
                        (0..n).map(|_| acquire_frame(&mut dec, &enc_buf)).collect();

                    let t = Instant::now();

                    let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                    let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                    black_box(&msg);

                    total += t.elapsed();
                    drop(msg);
                    drop(frame);
                    drop(held_frames);
                }

                total
            })
        });
    }

    for &n in held_counts {
        group.bench_with_input(
            BenchmarkId::new("owned_release/msgs_held", n),
            &n,
            |b, &n| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();

                        let held_owned: Vec<OwnedMsg> = (0..n)
                            .map(|_| {
                                let mut frame = acquire_frame(&mut dec, &enc_buf);
                                let msg: ZeroCopyMsg<'_> =
                                    binary_sv2::from_bytes(frame.payload()).unwrap();
                                let owned = OwnedMsg::from_zc(msg);
                                drop(frame); // pool slot freed here
                                owned
                            })
                            .collect();

                        let t = Instant::now();

                        let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                        let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                        black_box(&msg);

                        total += t.elapsed();

                        drop(msg);
                        drop(frame);
                        drop(held_owned);
                    }

                    total
                })
            },
        );
    }

    group.finish();
}

fn bench_exhaustion_boundary(c: &mut Criterion) {
    let enc_buf = make_encoded_frame(DEFAULT_COINBASE);

    let mut group = c.benchmark_group("pool_lifecycle/exhaustion_boundary");

    for frame_index in 1..=(POOL_CAPACITY + 4) {
        group.bench_with_input(
            BenchmarkId::new("zc_hold/frame_index", frame_index),
            &frame_index,
            |b, &idx| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();

                        let pre: Vec<DecodedFrame> = (0..(idx - 1))
                            .map(|_| acquire_frame(&mut dec, &enc_buf))
                            .collect();

                        let t = Instant::now();
                        let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                        let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                        black_box(&msg);
                        total += t.elapsed();

                        drop(msg);
                        drop(frame);
                        drop(pre);
                    }

                    total
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("owned_release/frame_index", frame_index),
            &frame_index,
            |b, &idx| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();

                        let pre: Vec<OwnedMsg> = (0..(idx - 1))
                            .map(|_| {
                                let mut frame = acquire_frame(&mut dec, &enc_buf);
                                let msg: ZeroCopyMsg<'_> =
                                    binary_sv2::from_bytes(frame.payload()).unwrap();
                                let owned = OwnedMsg::from_zc(msg);
                                drop(frame);
                                owned
                            })
                            .collect();
                        let t = Instant::now();
                        let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                        let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                        black_box(&msg);
                        total += t.elapsed();

                        drop(msg);
                        drop(frame);
                        drop(pre);
                    }

                    total
                })
            },
        );
    }

    group.finish();
}

fn bench_throughput_n_messages(c: &mut Criterion) {
    let enc_buf = make_encoded_frame(DEFAULT_COINBASE);
    let message_counts: &[usize] = &[1, 4, 8, 9, 16, 32, 64, 100, 200, 1000];

    let mut group = c.benchmark_group("pool_lifecycle/throughput");

    for &n in message_counts {
        group.bench_with_input(
            BenchmarkId::new("zc_hold/total_messages", n),
            &n,
            |b, &n| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                        // Hold frames so pool pressure accumulates.
                        let mut held_frames: Vec<DecodedFrame> = Vec::with_capacity(n);

                        let t = Instant::now();
                        for _ in 0..n {
                            let mut frame = acquire_frame(&mut dec, &enc_buf);
                            let msg: ZeroCopyMsg<'_> =
                                binary_sv2::from_bytes(frame.payload()).unwrap();
                            black_box(&msg);
                            drop(msg);
                            held_frames.push(frame);
                        }
                        total += t.elapsed();

                        drop(held_frames);
                    }

                    total
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("owned_release/total_messages", n),
            &n,
            |b, &n| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                        let mut held_owned: Vec<OwnedMsg> = Vec::with_capacity(n);

                        let t = Instant::now();
                        for _ in 0..n {
                            let mut frame = acquire_frame(&mut dec, &enc_buf);
                            let msg: ZeroCopyMsg<'_> =
                                binary_sv2::from_bytes(frame.payload()).unwrap();
                            let owned = OwnedMsg::from_zc(msg);
                            drop(frame); // slot freed; next iteration reuses it
                            held_owned.push(owned);
                        }
                        total += t.elapsed();

                        black_box(&held_owned);
                        drop(held_owned);
                    }

                    total
                })
            },
        );
    }

    group.finish();
}

fn bench_copy_overhead_isolation(c: &mut Criterion) {
    let enc_buf = make_encoded_frame(DEFAULT_COINBASE);
    let mut group = c.benchmark_group("pool_lifecycle/copy_overhead");

    group.bench_function("acquire_and_deserialize_zc", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();

                let t = Instant::now();
                let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                black_box(&msg);
                total += t.elapsed();

                drop(msg);
                drop(frame);
            }

            total
        })
    });

    group.bench_function("acquire_deserialize_and_copy", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();

                let t = Instant::now();
                let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                let owned = OwnedMsg::from_zc(msg);
                drop(frame);
                black_box(owned);
                total += t.elapsed();
            }

            total
        })
    });

    group.bench_function("copy_only", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                let mut frame = acquire_frame(&mut dec, &enc_buf);

                let t = Instant::now();
                let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                let owned = OwnedMsg::from_zc(black_box(msg));
                total += t.elapsed();

                black_box(owned);
                drop(frame);
            }

            total
        })
    });

    group.finish();
}

fn bench_copy_overhead_by_payload_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("pool_lifecycle/copy_overhead_by_payload_size");

    for &coinbase_size in COINBASE_SIZES {
        let enc_buf = make_encoded_frame(coinbase_size);

        group.bench_with_input(
            BenchmarkId::new("acquire_and_deserialize_zc/coinbase_bytes", coinbase_size),
            &coinbase_size,
            |b, _| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                        let t = Instant::now();
                        let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                        let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                        black_box(&msg);
                        total += t.elapsed();
                        drop(msg);
                        drop(frame);
                    }
                    total
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("acquire_deserialize_and_copy/coinbase_bytes", coinbase_size),
            &coinbase_size,
            |b, _| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                        let t = Instant::now();
                        let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                        let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                        let owned = OwnedMsg::from_zc(msg);
                        drop(frame);
                        black_box(owned);
                        total += t.elapsed();
                    }
                    total
                })
            },
        );
    }

    group.finish();
}

fn bench_sliding_window_pool_pressure(c: &mut Criterion) {
    let enc_buf = make_encoded_frame(DEFAULT_COINBASE);

    let window_sizes: &[usize] = &[1, 4, 7, 8, 9, 16];

    const MSGS: usize = 200;

    let mut group = c.benchmark_group("pool_lifecycle/sliding_window");
    group.throughput(criterion::Throughput::Elements(1));

    for &k in window_sizes {
        group.bench_with_input(BenchmarkId::new("zc/window_size", k), &k, |b, &k| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;

                for _ in 0..iters {
                    let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();

                    let mut window: VecDeque<DecodedFrame> = VecDeque::with_capacity(k + 1);
                    for _ in 0..k {
                        window.push_back(acquire_frame(&mut dec, &enc_buf));
                    }

                    let mut run_total = Duration::ZERO;
                    for _ in 0..MSGS {
                        if window.len() >= k {
                            window.pop_front();
                        }
                        let t = Instant::now();
                        let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                        let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                        black_box(&msg);
                        drop(msg);
                        run_total += t.elapsed();
                        window.push_back(frame);
                    }

                    total += run_total / MSGS as u32;
                    drop(window);
                }

                total
            })
        });
    }

    group.bench_function("owned/baseline", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                let mut held: Vec<OwnedMsg> = Vec::with_capacity(MSGS);

                let mut run_total = Duration::ZERO;
                for _ in 0..MSGS {
                    let t = Instant::now();
                    let mut frame = acquire_frame(black_box(&mut dec), &enc_buf);
                    let msg: ZeroCopyMsg<'_> = binary_sv2::from_bytes(frame.payload()).unwrap();
                    let owned = OwnedMsg::from_zc(msg);
                    drop(frame);
                    run_total += t.elapsed();
                    held.push(owned);
                }

                total += run_total / MSGS as u32;
                black_box(&held);
                drop(held);
            }

            total
        })
    });

    group.finish();
}

fn bench_allocation_amplification(c: &mut Criterion) {
    let enc_buf_small = make_encoded_frame(DEFAULT_COINBASE);
    let enc_buf_large = make_encoded_frame(1024);

    let message_counts: &[usize] = &[8, 9, 16, 32, 100, 1000];

    let mut group = c.benchmark_group("pool_lifecycle/alloc_amplification");

    for &n in message_counts {
        group.bench_with_input(
            BenchmarkId::new("zc_hold/coinbase_64B/n_msgs", n),
            &n,
            |b, &n| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    let mut total_allocs: u64 = 0;
                    let mut total_bytes: u64 = 0;

                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                        let mut held: Vec<DecodedFrame> = Vec::with_capacity(n);

                        reset_alloc_counters();
                        let t = Instant::now();
                        for _ in 0..n {
                            let mut frame = acquire_frame(&mut dec, &enc_buf_small);
                            let msg: ZeroCopyMsg<'_> =
                                binary_sv2::from_bytes(frame.payload()).unwrap();
                            black_box(&msg);
                            drop(msg);
                            held.push(frame);
                        }
                        total += t.elapsed();
                        let (allocs, bytes) = read_alloc_counters();
                        total_allocs += allocs;
                        total_bytes += bytes;

                        drop(held);
                    }

                    eprintln!(
                        "[zc_hold 64B n={n:>4}] allocs/run={:.1}  bytes/run={:.0}",
                        total_allocs as f64 / iters as f64,
                        total_bytes as f64 / iters as f64,
                    );
                    total
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("owned_release/coinbase_64B/n_msgs", n),
            &n,
            |b, &n| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    let mut total_allocs: u64 = 0;
                    let mut total_bytes: u64 = 0;

                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                        let mut held: Vec<OwnedMsg> = Vec::with_capacity(n);

                        reset_alloc_counters();
                        let t = Instant::now();
                        for _ in 0..n {
                            let mut frame = acquire_frame(&mut dec, &enc_buf_small);
                            let msg: ZeroCopyMsg<'_> =
                                binary_sv2::from_bytes(frame.payload()).unwrap();
                            let owned = OwnedMsg::from_zc(msg);
                            drop(frame);
                            held.push(owned);
                        }
                        total += t.elapsed();
                        let (allocs, bytes) = read_alloc_counters();
                        total_allocs += allocs;
                        total_bytes += bytes;

                        drop(held);
                    }

                    eprintln!(
                        "[owned_rel 64B n={n:>4}] allocs/run={:.1}  bytes/run={:.0}",
                        total_allocs as f64 / iters as f64,
                        total_bytes as f64 / iters as f64,
                    );
                    total
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("zc_hold/coinbase_1KB/n_msgs", n),
            &n,
            |b, &n| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    let mut total_allocs: u64 = 0;
                    let mut total_bytes: u64 = 0;

                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                        let mut held: Vec<DecodedFrame> = Vec::with_capacity(n);

                        reset_alloc_counters();
                        let t = Instant::now();
                        for _ in 0..n {
                            let mut frame = acquire_frame(&mut dec, &enc_buf_large);
                            let msg: ZeroCopyMsg<'_> =
                                binary_sv2::from_bytes(frame.payload()).unwrap();
                            black_box(&msg);
                            drop(msg);
                            held.push(frame);
                        }
                        total += t.elapsed();
                        let (allocs, bytes) = read_alloc_counters();
                        total_allocs += allocs;
                        total_bytes += bytes;

                        drop(held);
                    }

                    eprintln!(
                        "[zc_hold  1KB n={n:>4}] allocs/run={:.1}  bytes/run={:.0}",
                        total_allocs as f64 / iters as f64,
                        total_bytes as f64 / iters as f64,
                    );
                    total
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("owned_release/coinbase_1KB/n_msgs", n),
            &n,
            |b, &n| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    let mut total_allocs: u64 = 0;
                    let mut total_bytes: u64 = 0;

                    for _ in 0..iters {
                        let mut dec = StandardDecoder::<ZeroCopyMsg<'static>>::new();
                        let mut held: Vec<OwnedMsg> = Vec::with_capacity(n);

                        reset_alloc_counters();
                        let t = Instant::now();
                        for _ in 0..n {
                            let mut frame = acquire_frame(&mut dec, &enc_buf_large);
                            let msg: ZeroCopyMsg<'_> =
                                binary_sv2::from_bytes(frame.payload()).unwrap();
                            let owned = OwnedMsg::from_zc(msg);
                            drop(frame);
                            held.push(owned);
                        }
                        total += t.elapsed();
                        let (allocs, bytes) = read_alloc_counters();
                        total_allocs += allocs;
                        total_bytes += bytes;

                        drop(held);
                    }

                    eprintln!(
                        "[owned_rel 1KB n={n:>4}] allocs/run={:.1}  bytes/run={:.0}",
                        total_allocs as f64 / iters as f64,
                        total_bytes as f64 / iters as f64,
                    );
                    total
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    pool_lifecycle_benches,
    bench_deserialization_latency_vs_accumulated,
    bench_exhaustion_boundary,
    bench_throughput_n_messages,
    bench_copy_overhead_isolation,
    bench_copy_overhead_by_payload_size,
    bench_sliding_window_pool_pressure,
    bench_allocation_amplification,
);
criterion_main!(pool_lifecycle_benches);
