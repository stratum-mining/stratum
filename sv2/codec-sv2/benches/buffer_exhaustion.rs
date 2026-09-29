extern crate alloc;

use codec_sv2::{Decoded, Decoder, Encoder};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use framing_sv2::framing::{EncodableFrame, MessageFrame};
use std::time::{Duration, Instant};

mod common;
use common::{TestMsg, ZeroCopyMsgOwned};

fn bench_encoder_pool_back_vs_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoder/pool_exhaustion");

    group.bench_function("back_mode", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut enc = Encoder::new();
                let frame = MessageFrame::from_message(TestMsg { data: 42 }, 0, 0, true).unwrap();
                let t = Instant::now();
                let _s = enc.encode(black_box(frame)).unwrap();
                total += t.elapsed();
            }
            total
        })
    });

    group.bench_function("alloc_mode_after_pool_exhausted", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut enc = Encoder::new();
                let held: Vec<_> = (0u8..8)
                    .map(|i| {
                        enc.encode(
                            MessageFrame::from_message(TestMsg { data: i }, 0, 0, true).unwrap(),
                        )
                        .unwrap()
                    })
                    .collect();
                let frame = MessageFrame::from_message(TestMsg { data: 99 }, 0, 0, true).unwrap();
                let t = Instant::now();
                let _s = enc.encode(black_box(frame)).unwrap();
                total += t.elapsed();
                drop(_s);
                drop(held);
            }
            total
        })
    });

    group.bench_function("recovery_after_full_release", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut enc = Encoder::new();
                let held: Vec<_> = (0u8..8)
                    .map(|i| {
                        enc.encode(
                            MessageFrame::from_message(TestMsg { data: i }, 0, 0, true).unwrap(),
                        )
                        .unwrap()
                    })
                    .collect();
                let _overflow = enc
                    .encode(MessageFrame::from_message(TestMsg { data: 99 }, 0, 0, true).unwrap())
                    .unwrap();
                drop(_overflow);
                drop(held);
                let frame = MessageFrame::from_message(TestMsg { data: 1 }, 0, 0, true).unwrap();
                let t = Instant::now();
                let _s = enc.encode(black_box(frame)).unwrap();
                total += t.elapsed();
                drop(_s);
            }
            total
        })
    });

    group.finish();
}

fn bench_encoder_per_slot_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoder/pool_exhaustion/per_slot_latency");

    for &held in [0usize, 1, 2, 4, 6, 7, 8, 9, 12, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new("slots_held_before_measure", held),
            &held,
            |b, &held| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let mut enc = Encoder::new();
                        let pre: Vec<_> = (0..held)
                            .map(|i| {
                                enc.encode(
                                    MessageFrame::from_message(
                                        TestMsg { data: i as u8 },
                                        0,
                                        0,
                                        true,
                                    )
                                    .unwrap(),
                                )
                                .unwrap()
                            })
                            .collect();
                        let frame =
                            MessageFrame::from_message(TestMsg { data: 42 }, 0, 0, true).unwrap();
                        let t = Instant::now();
                        let _s = enc.encode(black_box(frame)).unwrap();
                        total += t.elapsed();
                        drop(_s);
                        drop(pre);
                    }
                    total
                })
            },
        );
    }
    group.finish();
}

fn bench_encoder_zc_pool_back_vs_alloc(c: &mut Criterion) {
    let coinbase_size: usize = 64;

    let mut group = c.benchmark_group("encoder/zerocopy_pool_exhaustion");

    group.bench_function("back_mode", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut enc = Encoder::new();
                let msg = ZeroCopyMsgOwned::new_owned(1, coinbase_size);
                let frame = MessageFrame::from_message(msg, 0, 0, true).unwrap();
                let t = Instant::now();
                let _s = enc.encode(black_box(frame)).unwrap();
                total += t.elapsed();
                drop(_s);
            }
            total
        })
    });

    group.bench_function("alloc_mode_after_byte_limit_4_held", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut enc = Encoder::new();
                // 4 × 108 = 432 B consumed; 80 B remain (not enough for 5th @ 108 B).
                let held: Vec<_> = (0u32..4)
                    .map(|i| {
                        let msg = ZeroCopyMsgOwned::new_owned(i, coinbase_size);
                        enc.encode(MessageFrame::from_message(msg, 0, 0, true).unwrap())
                            .unwrap()
                    })
                    .collect();
                // 5th encode: byte capacity exceeded → Alloc mode.
                let msg = ZeroCopyMsgOwned::new_owned(99, coinbase_size);
                let frame = MessageFrame::from_message(msg, 0, 0, true).unwrap();
                let t = Instant::now();
                let _s = enc.encode(black_box(frame)).unwrap();
                total += t.elapsed();
                drop(_s);
                drop(held);
            }
            total
        })
    });

    group.finish();
}

fn bench_encoder_zc_per_slot_latency(c: &mut Criterion) {
    let coinbase_size: usize = 64;
    let mut group = c.benchmark_group("encoder/zerocopy_pool_exhaustion/per_slot_latency");

    for &held in [0usize, 1, 2, 3, 4, 5, 7, 8, 12].iter() {
        group.bench_with_input(
            BenchmarkId::new("slots_held_before_measure", held),
            &held,
            |b, &held| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let mut enc = Encoder::new();
                        let pre: Vec<_> = (0..held)
                            .map(|i| {
                                let msg = ZeroCopyMsgOwned::new_owned(i as u32, coinbase_size);
                                enc.encode(MessageFrame::from_message(msg, 0, 0, true).unwrap())
                                    .unwrap()
                            })
                            .collect();
                        let msg = ZeroCopyMsgOwned::new_owned(99, coinbase_size);
                        let frame = MessageFrame::from_message(msg, 0, 0, true).unwrap();
                        let t = Instant::now();
                        let _s = enc.encode(black_box(frame)).unwrap();
                        total += t.elapsed();
                        drop(_s);
                        drop(pre);
                    }
                    total
                })
            },
        );
    }
    group.finish();
}

fn bench_encoder_owned_vs_zc_exhaustion(c: &mut Criterion) {
    let coinbase_size: usize = 64;
    let mut group = c.benchmark_group("encoder/owned_vs_zerocopy_exhaustion");

    for &held in [0usize, 4, 7, 8, 9].iter() {
        group.bench_with_input(
            BenchmarkId::new("owned_TestMsg_7B/slots_held", held),
            &held,
            |b, &held| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let mut enc = Encoder::new();
                        let pre: Vec<_> = (0..held)
                            .map(|i| {
                                enc.encode(
                                    MessageFrame::from_message(
                                        TestMsg { data: i as u8 },
                                        0,
                                        0,
                                        true,
                                    )
                                    .unwrap(),
                                )
                                .unwrap()
                            })
                            .collect();
                        let t = Instant::now();
                        let _s = enc
                            .encode(
                                MessageFrame::from_message(TestMsg { data: 42 }, 0, 0, true)
                                    .unwrap(),
                            )
                            .unwrap();
                        total += t.elapsed();
                        drop(_s);
                        drop(pre);
                    }
                    total
                })
            },
        );
    }

    for &held in [0usize, 3, 4, 5, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("zerocopy_ZeroCopyMsg_108B/slots_held", held),
            &held,
            |b, &held| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let mut enc = Encoder::new();
                        let pre: Vec<_> = (0..held)
                            .map(|i| {
                                let msg = ZeroCopyMsgOwned::new_owned(i as u32, coinbase_size);
                                enc.encode(MessageFrame::from_message(msg, 0, 0, true).unwrap())
                                    .unwrap()
                            })
                            .collect();
                        let msg = ZeroCopyMsgOwned::new_owned(99, coinbase_size);
                        let t = Instant::now();
                        let _s = enc
                            .encode(MessageFrame::from_message(msg, 0, 0, true).unwrap())
                            .unwrap();
                        total += t.elapsed();
                        drop(_s);
                        drop(pre);
                    }
                    total
                })
            },
        );
    }

    group.finish();
}

fn bench_decoder_pool_back_vs_alloc(
    c: &mut Criterion,
    group_name: &str,
    enc_buf: &[u8],
    held_label: &str,
) {
    let mut group = c.benchmark_group(group_name);

    group.bench_function("back_mode", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut dec = Decoder::new();
                let w = dec.read_buf();
                let len = w.len();
                w.copy_from_slice(&enc_buf[..len]);
                dec.advance(len).unwrap();
                let mut offset = len;
                let t = Instant::now();
                loop {
                    match dec.next_frame() {
                        Ok(Decoded::Frame(f)) => {
                            total += t.elapsed();
                            black_box(f);
                            break;
                        }
                        Ok(Decoded::Incomplete(_)) => {
                            let w = dec.read_buf();
                            let n = w.len();
                            w.copy_from_slice(&enc_buf[offset..offset + n]);
                            dec.advance(n).unwrap();
                            offset += n;
                        }
                        Err(e) => panic!("decode error: {:?}", e),
                    }
                }
            }
            total
        })
    });

    group.bench_function(held_label, |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut dec = Decoder::new();
                let mut held = Vec::with_capacity(9);

                for _ in 0..8 {
                    let w = dec.read_buf();
                    let len = w.len();
                    w.copy_from_slice(&enc_buf[..len]);
                    dec.advance(len).unwrap();
                    let mut offset = len;
                    loop {
                        match dec.next_frame() {
                            Ok(Decoded::Frame(f)) => {
                                held.push(f);
                                break;
                            }
                            Ok(Decoded::Incomplete(_)) => {
                                let w = dec.read_buf();
                                let n = w.len();
                                w.copy_from_slice(&enc_buf[offset..offset + n]);
                                dec.advance(n).unwrap();
                                offset += n;
                            }
                            Err(e) => panic!("decode error: {:?}", e),
                        }
                    }
                }

                let w = dec.read_buf();
                let len = w.len();
                w.copy_from_slice(&enc_buf[..len]);
                dec.advance(len).unwrap();
                let mut offset = len;
                let t = Instant::now();
                loop {
                    match dec.next_frame() {
                        Ok(Decoded::Frame(f)) => {
                            total += t.elapsed();
                            held.push(f);
                            break;
                        }
                        Ok(Decoded::Incomplete(_)) => {
                            let w = dec.read_buf();
                            let n = w.len();
                            w.copy_from_slice(&enc_buf[offset..offset + n]);
                            dec.advance(n).unwrap();
                            offset += n;
                        }
                        Err(e) => panic!("decode error: {:?}", e),
                    }
                }
                drop(held);
            }
            total
        })
    });

    group.finish();
}

fn bench_decoder_per_slot_latency(c: &mut Criterion, group_name: &str, enc_buf: &[u8]) {
    let mut group = c.benchmark_group(group_name);
    for &held in [0usize, 1, 2, 4, 6, 7, 8, 9, 12, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new("slots_held_before_measure", held),
            &held,
            |b, &held| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let mut dec = Decoder::new();
                        let mut pre = Vec::with_capacity(held + 1);

                        for _ in 0..held {
                            let w = dec.read_buf();
                            let len = w.len();
                            w.copy_from_slice(&enc_buf[..len]);
                            dec.advance(len).unwrap();
                            let mut offset = len;
                            loop {
                                match dec.next_frame() {
                                    Ok(Decoded::Frame(f)) => {
                                        pre.push(f);
                                        break;
                                    }
                                    Ok(Decoded::Incomplete(_)) => {
                                        let w = dec.read_buf();
                                        let n = w.len();
                                        w.copy_from_slice(&enc_buf[offset..offset + n]);
                                        dec.advance(n).unwrap();
                                        offset += n;
                                    }
                                    Err(e) => panic!("decode error: {:?}", e),
                                }
                            }
                        }

                        let w = dec.read_buf();
                        let len = w.len();
                        w.copy_from_slice(&enc_buf[..len]);
                        dec.advance(len).unwrap();
                        let mut offset = len;
                        let t = Instant::now();
                        loop {
                            match dec.next_frame() {
                                Ok(Decoded::Frame(f)) => {
                                    total += t.elapsed();
                                    pre.push(f);
                                    break;
                                }
                                Ok(Decoded::Incomplete(_)) => {
                                    let w = dec.read_buf();
                                    let n = w.len();
                                    w.copy_from_slice(&enc_buf[offset..offset + n]);
                                    dec.advance(n).unwrap();
                                    offset += n;
                                }
                                Err(e) => panic!("decode error: {:?}", e),
                            }
                        }
                        drop(pre);
                    }
                    total
                })
            },
        );
    }
    group.finish();
}

fn bench_encoder_zc_payload_size_vs_exhaustion(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoder/zerocopy_payload_size_vs_exhaustion");

    for &coinbase_size in [16usize, 64, 128, 200].iter() {
        let frame_size = 6 + 4 + 32 + 2 + coinbase_size;
        let threshold = 512usize / frame_size;

        group.bench_with_input(
            BenchmarkId::new("back_mode/coinbase_bytes", coinbase_size),
            &coinbase_size,
            |b, &cs| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let mut enc = Encoder::new();
                        let msg = ZeroCopyMsgOwned::new_owned(1, cs);
                        let t = Instant::now();
                        let _s = enc
                            .encode(MessageFrame::from_message(msg, 0, 0, true).unwrap())
                            .unwrap();
                        total += t.elapsed();
                        drop(_s);
                    }
                    total
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("alloc_mode/coinbase_bytes", coinbase_size),
            &coinbase_size,
            |b, &cs| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let mut enc = Encoder::new();
                        let held: Vec<_> = (0..threshold)
                            .map(|i| {
                                let msg = ZeroCopyMsgOwned::new_owned(i as u32, cs);
                                enc.encode(MessageFrame::from_message(msg, 0, 0, true).unwrap())
                                    .unwrap()
                            })
                            .collect();
                        let msg = ZeroCopyMsgOwned::new_owned(99, cs);
                        let t = Instant::now();
                        let _s = enc
                            .encode(MessageFrame::from_message(msg, 0, 0, true).unwrap())
                            .unwrap();
                        total += t.elapsed();
                        drop(_s);
                        drop(held);
                    }
                    total
                })
            },
        );
    }

    group.finish();
}

// The decoder no longer deserialises, so "zero-copy vs owned" does not exist on this path: the
// two groups differ only in payload size, and run the same code.
fn bench_decoder_exhaustion(c: &mut Criterion) {
    let msg = TestMsg { data: 7u8 };
    let frame = MessageFrame::<TestMsg>::from_message(msg, 0, 0, true).unwrap();
    let mut small = vec![0u8; frame.encoded_length()];
    frame.encode_into(&mut small).unwrap();
    let large = common::make_encoded_frame(64);

    bench_decoder_pool_back_vs_alloc(
        c,
        "decoder/pool_exhaustion",
        &small,
        "alloc_mode_after_pool_exhausted",
    );
    bench_decoder_per_slot_latency(c, "decoder/pool_exhaustion/per_slot_latency", &small);
    bench_decoder_pool_back_vs_alloc(
        c,
        "decoder/zerocopy_pool_exhaustion",
        &large,
        "alloc_mode_8_zc_frames_held",
    );
    bench_decoder_per_slot_latency(
        c,
        "decoder/zerocopy_pool_exhaustion/per_slot_latency",
        &large,
    );
}

criterion_group!(
    exhaustion_benches,
    bench_encoder_pool_back_vs_alloc,
    bench_encoder_per_slot_latency,
    bench_decoder_exhaustion,
    bench_encoder_zc_pool_back_vs_alloc,
    bench_encoder_zc_per_slot_latency,
    bench_encoder_owned_vs_zc_exhaustion,
    bench_encoder_zc_payload_size_vs_exhaustion,
);
criterion_main!(exhaustion_benches);
