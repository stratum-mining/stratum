
use criterion::{criterion_group, criterion_main, Criterion, black_box};
use framing_sv2::framing::Sv2Frame;

use codec_sv2::Encoder;

#[cfg(feature = "noise_sv2")]
use codec_sv2::{NoiseEncoder, State};

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


fn bench_plain_encoder(c: &mut Criterion) {
    let msg = TestMsg { data: 42u8 };
    let frame = Sv2Frame::from_message(msg, 0, 0, true).unwrap();
    let mut enc = Encoder::<TestMsg>::new();

    c.bench_function("encoder/plain", |b| {
        b.iter(|| {
            let out = enc.encode(black_box(frame.clone())).unwrap();
            black_box(out);
        })
    });
}

#[cfg(feature = "noise_sv2")]
fn bench_noise_encoder_transport(c: &mut Criterion) {
    use noise_sv2::NoiseCodec;

    let msg = TestMsg { data: 42u8 };
    let frame = framing_sv2::framing::Frame::Sv2(
        Sv2Frame::from_message(msg, 0, 0, true).unwrap()
    );

    let codec = NoiseCodec::new_transport(
        [0u8; 32],
        [1u8; 32],
        noise_sv2::HandshakeRole::Initiator,
    ).unwrap();

    let mut state = State::with_transport_mode(codec);
    let mut enc = NoiseEncoder::<TestMsg>::new();

    c.bench_function("encoder/noise/transport", |b| {
        b.iter(|| {
            let out = enc.encode(black_box(frame.clone()), &mut state).unwrap();
            black_box(out);
        })
    });
}

criterion_group!(
    encoder_benches,
    bench_plain_encoder,
    #[cfg(feature = "noise_sv2")]
    bench_noise_encoder_transport
);

criterion_main!(encoder_benches);
