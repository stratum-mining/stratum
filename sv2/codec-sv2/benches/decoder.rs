extern crate alloc;

use codec_sv2::StandardDecoder;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use framing_sv2::framing::Sv2Frame;

mod common;
use common::TestMsg;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

fn bench_plain_decoder(c: &mut Criterion) {
    c.bench_function("decoder/plain", |b| {
        let msg = TestMsg { data: 7u8 };
        let frame = Sv2Frame::<TestMsg, Slice>::from_message(msg, 0, 0, true).unwrap();

        let mut enc_buf = vec![0; frame.encoded_length()];
        frame.serialize(&mut enc_buf).unwrap();

        let mut dec = StandardDecoder::<TestMsg>::new();

        b.iter(|| {
            let w = dec.writable();
            let len = w.len();
            w.copy_from_slice(&enc_buf[..len]);
            let mut offset = len;

            loop {
                match dec.next_frame() {
                    Ok(frame) => {
                        black_box(frame);
                        break;
                    }
                    Err(codec_sv2::Error::MissingBytes(n)) => {
                        let w = dec.writable();
                        w.copy_from_slice(&enc_buf[offset..offset + n]);
                        offset += n;
                    }
                    Err(_) => panic!("Unexpected decode error"),
                }
            }
        })
    });
}

fn bench_decoder_creation(c: &mut Criterion) {
    c.bench_function("decoder/creation/plain", |b| {
        b.iter(|| {
            let dec = StandardDecoder::<TestMsg>::new();
            black_box(dec);
        })
    });
}

criterion_group!(decoder_benches, bench_plain_decoder, bench_decoder_creation);

criterion_main!(decoder_benches);
