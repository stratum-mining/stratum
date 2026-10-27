extern crate alloc;

#[cfg(feature = "noise_sv2")]
use criterion::{black_box, criterion_group, criterion_main, Criterion};

#[cfg(feature = "noise_sv2")]
use codec_sv2::{
    Decrypted, NoiseDecoder, NoiseEncoder, TransportDecryptState, TransportEncryptState,
};

#[cfg(feature = "noise_sv2")]
use framing_sv2::framing::MessageFrame;

#[cfg(feature = "noise_sv2")]
#[cfg(feature = "noise_sv2")]
mod common;
#[cfg(feature = "noise_sv2")]
use common::TestMsg;

#[cfg(feature = "noise_sv2")]
fn setup_noise_engine_pair() -> (
    NoiseEncoder,
    NoiseDecoder,
    TransportEncryptState,
    TransportDecryptState,
) {
    let (sender, receiver) = common::make_handshake_pair();

    let (first_message, sender) = sender.step_0().unwrap();
    let first_message_bytes: [u8; noise_sv2::ELLSWIFT_ENCODING_SIZE] =
        first_message.payload().try_into().unwrap();

    let (second_message, receiver_transport) = receiver.step_1(first_message_bytes).unwrap();
    let second_message_bytes: [u8; noise_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] =
        second_message.payload().try_into().unwrap();

    let sender_transport = sender.step_2(second_message_bytes).unwrap();

    let enc = NoiseEncoder::new();
    let dec = NoiseDecoder::new();

    let (sender_encrypt, _) = sender_transport.split();
    let (_, receiver_decrypt) = receiver_transport.split();
    (enc, dec, sender_encrypt, receiver_decrypt)
}

#[cfg(feature = "noise_sv2")]
fn bench_noise_roundtrip(c: &mut Criterion) {
    c.bench_function("noise/roundtrip", |b| {
        let msg = TestMsg { data: 9u8 };

        b.iter(|| {
            // Set up fresh Noise engines for each iteration (their state cannot be reused).
            let (mut enc, _, mut enc_state, mut dec_state) = setup_noise_engine_pair();

            // Encode
            let frame = MessageFrame::from_message(msg.clone(), 0, 0, true).unwrap();
            let encrypted = enc
                .encode_transport(black_box(frame), &mut enc_state)
                .unwrap();

            // Decode
            let mut dec = NoiseDecoder::new();
            let w = dec.read_buf();
            let len = w.len();
            w[..len].copy_from_slice(&encrypted[0..len]);
            dec.advance(len).unwrap();
            let mut offset = len;

            loop {
                match dec.next_transport_frame(dec_state) {
                    Ok(Decrypted::Frame(decoded, state)) => {
                        black_box(decoded);
                        dec_state = state;
                        break;
                    }
                    Ok(Decrypted::Incomplete(state)) => {
                        dec_state = state;
                        let w = dec.read_buf();
                        let n = w.len();
                        w.copy_from_slice(&encrypted[offset..offset + n]);
                        dec.advance(n).unwrap();
                        offset += n;
                    }
                    Err(e) => panic!("Decode error: {:?}", e),
                }
            }
        })
    });
}

#[cfg(feature = "noise_sv2")]
fn bench_noise_encode_only(c: &mut Criterion) {
    c.bench_function("noise/encode_only", |b| {
        let (mut enc, _, mut enc_state, _) = setup_noise_engine_pair();

        let msg = TestMsg { data: 42u8 };

        b.iter(|| {
            let frame = MessageFrame::from_message(msg.clone(), 0, 0, true).unwrap();
            let encrypted = enc
                .encode_transport(black_box(frame), &mut enc_state)
                .unwrap();
            black_box(encrypted);
        })
    });
}

#[cfg(feature = "noise_sv2")]
fn bench_noise_handshake_steps(c: &mut Criterion) {
    c.bench_function("noise/handshake/step_0", |b| {
        b.iter(|| {
            let (sender, _) = common::make_handshake_pair();
            black_box(sender.step_0().unwrap());
        })
    });

    c.bench_function("noise/handshake/step_1", |b| {
        let (sender, _) = common::make_handshake_pair();
        let (first_message, _sender) = sender.step_0().unwrap();
        let first_message_bytes: [u8; noise_sv2::ELLSWIFT_ENCODING_SIZE] =
            first_message.payload().try_into().unwrap();

        b.iter(|| {
            let (_, receiver) = common::make_handshake_pair();
            let (second_message, _) = receiver.step_1(first_message_bytes).unwrap();
            black_box(second_message);
        })
    });
}

#[cfg(feature = "noise_sv2")]
criterion_group!(
    noise_benches,
    bench_noise_roundtrip,
    bench_noise_encode_only,
    bench_noise_handshake_steps
);

#[cfg(feature = "noise_sv2")]
criterion_main!(noise_benches);

#[cfg(not(feature = "noise_sv2"))]
fn main() {
    eprintln!("Noise benchmarks require the 'noise_sv2' feature");
}
