
use criterion::{criterion_group, criterion_main, Criterion, black_box};
use codec_sv2::StandardDecoder;
use framing_sv2::framing::Sv2Frame;

mod common;
use common::TestMsg;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

#[cfg(feature = "with_buffer_pool")]
const BACKEND: &str = "buffer_pool";

#[cfg(not(feature = "with_buffer_pool"))]
const BACKEND: &str = "vec";

fn bench_plain_decoder(c: &mut Criterion) {
    let msg = TestMsg { data: 7u8 };
    let frame = Sv2Frame::<TestMsg, Slice>::from_message(msg, 0, 0, true).unwrap();

    let mut enc_buf = Vec::new();
    frame.serialize(&mut enc_buf).unwrap();

    let mut dec = StandardDecoder::<TestMsg>::new();

    c.bench_function("decoder/plain/{BACKEND}", |b| {
        b.iter(|| {
            let w = dec.writable();
            w.copy_from_slice(&enc_buf[..w.len()]);
            let f = dec.next_frame().unwrap();
            black_box(f);
        })
    });
}

criterion_group!(decoder_benches, bench_plain_decoder);
criterion_main!(decoder_benches);
