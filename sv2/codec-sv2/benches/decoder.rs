extern crate alloc;

use codec_sv2::{Decoded, Decoder};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use framing_sv2::framing::{EncodableFrame, MessageFrame};

mod common;
use common::TestMsg;

fn bench_plain_decoder(c: &mut Criterion) {
    c.bench_function("decoder/plain", |b| {
        let msg = TestMsg { data: 7u8 };
        let frame = MessageFrame::<TestMsg>::from_message(msg, 0, 0, true).unwrap();

        let mut enc_buf = vec![0; frame.encoded_length()];
        frame.encode_into(&mut enc_buf).unwrap();

        let mut dec = Decoder::new();

        b.iter(|| {
            let w = dec.read_buf();
            let len = w.len();
            w.copy_from_slice(&enc_buf[..len]);
            dec.advance(len).unwrap();
            let mut offset = len;

            loop {
                match dec.next_frame() {
                    Ok(Decoded::Frame(frame)) => {
                        black_box(frame);
                        break;
                    }
                    Ok(Decoded::Incomplete(_)) => {
                        let w = dec.read_buf();
                        let n = w.len();
                        w.copy_from_slice(&enc_buf[offset..offset + n]);
                        dec.advance(n).unwrap();
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
            let dec = Decoder::new();
            black_box(dec);
        })
    });
}

criterion_group!(decoder_benches, bench_plain_decoder, bench_decoder_creation);

criterion_main!(decoder_benches);
