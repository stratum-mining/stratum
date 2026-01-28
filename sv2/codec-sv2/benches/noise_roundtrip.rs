
#[cfg(feature = "noise_sv2")]
use criterion::{criterion_group, criterion_main, Criterion, black_box};

#[cfg(feature = "noise_sv2")]
use codec_sv2::{NoiseEncoder, StandardNoiseDecoder, State};

#[cfg(feature = "noise_sv2")]
use framing_sv2::framing::Frame;

#[cfg(feature = "noise_sv2")]
mod common;
#[cfg(feature = "noise_sv2")]
use common::TestMsg;

#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

#[cfg(feature = "with_buffer_pool")]
const BACKEND: &str = "buffer_pool";

#[cfg(not(feature = "with_buffer_pool"))]
const BACKEND: &str = "vec";


#[cfg(feature = "noise_sv2")]
fn bench_noise_roundtrip(c: &mut Criterion) {
    use framing_sv2::framing::Sv2Frame;
    use noise_sv2::NoiseCodec;

    let msg = TestMsg { data: 9u8 };

    let frame = Frame::Sv2(
        Sv2Frame::from_message(msg, 0, 0, true).unwrap()
    );

    let codec_a = NoiseCodec::new_transport(
        [0u8; 32],
        [1u8; 32],
        noise_sv2::HandshakeRole::Initiator,
    ).unwrap();

    let codec_b = NoiseCodec::new_transport(
        [1u8; 32],
        [0u8; 32],
        noise_sv2::HandshakeRole::Responder,
    ).unwrap();

    let mut enc_state = State::with_transport_mode(codec_a);
    let mut dec_state = State::with_transport_mode(codec_b);

    let mut enc = NoiseEncoder::<TestMsg>::new();
    let mut dec = StandardNoiseDecoder::<TestMsg>::new();

    c.bench_function("noise/roundtrip", |b| {
        b.iter(|| {
            let encrypted = enc.encode(black_box(frame.clone()), &mut enc_state).unwrap();
            let w = dec.writable();
            w.copy_from_slice(&encrypted[..w.len()]);
            let decoded = dec.next_frame(&mut dec_state).unwrap();
            black_box(decoded);
        })
    });
}

criterion_group!(
    noise_benches,
    #[cfg(feature = "noise_sv2")]
    bench_noise_roundtrip
);

criterion_main!(noise_benches);
