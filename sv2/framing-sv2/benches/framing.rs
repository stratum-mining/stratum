//! Performance benchmarks for SV2 framing layer operations
//! Tests both Vec and buffer_pool backends across different message sizes

use binary_sv2::{self, Serialize};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use framing_sv2::{framing::Sv2Frame, header::Header};

// Type alias for buffer backend - Vec or buffer_pool::Slice
#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

// Backend identifier for benchmark naming
#[cfg(feature = "with_buffer_pool")]
const BACKEND: &str = "buffer_pool";

#[cfg(not(feature = "with_buffer_pool"))]
const BACKEND: &str = "vec";

// Test payload sizes
const PAYLOAD_SIZES: &[usize] = &[64, 1024, 16 * 1024, 60 * 1024, 0xFFFFFF];

// Creates a mock SV2 frame: 3-byte header + 3-byte length + payload
fn frame_from_payload_size(size: usize) -> Vec<u8> {
    let len = size as u32;
    let mut ve = Vec::with_capacity(6 + size);

    ve.push(0);
    ve.push(0);
    ve.push(0);

    ve.push((len & 0xFF) as u8);
    ve.push(((len >> 8) & 0xFF) as u8);
    ve.push(((len >> 16) & 0xFF) as u8);

    ve.extend(std::iter::repeat(2u8).take(size));

    ve
}

#[derive(Serialize, Clone)]
struct Test {
    _a: Vec<u8>,
}

impl Test {
    fn new(size: usize) -> Self {
        Test { _a: vec![2; size] }
    }
}

// Benchmarks frame construction from raw message data
fn bench_from_message(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("sv2frame::from_message::{BACKEND}"));

    for &size in PAYLOAD_SIZES {
        let test = Test::new(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| Sv2Frame::<Test, Slice>::from_message(test.clone(), 1, 0, false).unwrap())
        });
    }

    group.finish();
}

// Benchmarks serializing frames to wire format
fn bench_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("sv2frame::serialize_fresh::{BACKEND}"));

    for &size in PAYLOAD_SIZES {
        let frame = Sv2Frame::<Vec<u8>, Slice>::from_bytes(frame_from_payload_size(size).into()).unwrap();

        let mut buf = vec![0u8; frame.encoded_length()];

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                frame.clone().serialize(&mut buf).unwrap();
            })
        });
    }

    group.finish();
}

// Benchmarks parsing frames from network bytes
fn bench_from_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("sv2frame::from_bytes::{BACKEND}"));

    for &size in PAYLOAD_SIZES {
        let frame = frame_from_payload_size(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| Sv2Frame::<Vec<u8>, _>::from_bytes(black_box(frame.clone())).unwrap())
        });
    }

    group.finish();
}

// Benchmarks calculating frame size from partial data
fn bench_size_hint(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("sv2frame::size_hint::{BACKEND}"));

    for &size in PAYLOAD_SIZES {
        let frame = frame_from_payload_size(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| Sv2Frame::<Vec<u8>, Slice>::size_hint(black_box(&frame)))
        });
    }

    group.finish();
}

// Benchmarks calculating encrypted payload length from header
fn bench_encrypted_len(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("sv2frame::encrypted_len::{BACKEND}"));

    for &size in PAYLOAD_SIZES {
        let header = Header::from_bytes(&frame_from_payload_size(size)).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| black_box(header.encrypted_len()))
        });
    }

    group.finish();
}

criterion_group!(
    framing,
    bench_from_message,
    bench_serialize,
    bench_from_bytes,
    bench_size_hint,
    bench_encrypted_len
);

criterion_main!(framing);
