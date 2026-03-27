use criterion::{black_box, criterion_group, criterion_main, Criterion};
use framing_sv2::framing::Sv2Frame;

mod common;
use common::TestMsg;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

fn bench_frame_from_message(c: &mut Criterion) {
    c.bench_function("serialization/frame_from_message", |b| {
        let msg = TestMsg { data: 42u8 };
        b.iter(|| {
            let frame = Sv2Frame::<TestMsg, Slice>::from_message(msg.clone(), 0, 0, true).unwrap();
            black_box(frame);
        })
    });
}

fn bench_frame_serialize_and_create(c: &mut Criterion) {
    c.bench_function("serialization/frame_serialize_and_create", |b| {
        let msg = TestMsg { data: 42u8 };

        b.iter(|| {
            let frame = Sv2Frame::<TestMsg, Slice>::from_message(msg.clone(), 0, 0, true).unwrap();
            let mut buf = vec![0; frame.encoded_length()];
            frame.serialize(&mut buf).unwrap();
            black_box(buf);
        })
    });
}

criterion_group!(
    serialization_benches,
    bench_frame_from_message,
    bench_frame_serialize_and_create
);

criterion_main!(serialization_benches);
