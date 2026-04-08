extern crate alloc;

#[cfg(feature = "noise_sv2")]
use criterion::{black_box, criterion_group, criterion_main, Criterion};

#[cfg(feature = "noise_sv2")]
use codec_sv2::{HandshakeRole, NoiseEncoder, StandardNoiseDecoder, State};

#[cfg(feature = "noise_sv2")]
use framing_sv2::framing::{Frame, Sv2Frame};

#[cfg(feature = "noise_sv2")]
use noise_sv2::{Initiator, Responder};

#[cfg(feature = "noise_sv2")]
mod common;
#[cfg(feature = "noise_sv2")]
use common::TestMsg;

#[cfg(feature = "noise_sv2")]
fn setup_noise_codec_pair() -> (
    NoiseEncoder<TestMsg>,
    StandardNoiseDecoder<TestMsg>,
    State,
    State,
) {
    use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};

    const AUTHORITY_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
    const AUTHORITY_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
    const CERT_VALIDITY: core::time::Duration = core::time::Duration::from_secs(3600);

    let authority_public_k: Secp256k1PublicKey = AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();

    let authority_private_k: Secp256k1SecretKey =
        AUTHORITY_PRIVATE_K.to_string().try_into().unwrap();

    let initiator = Initiator::from_raw_k(authority_public_k.into_bytes()).unwrap();
    let responder = Responder::from_authority_kp(
        &authority_public_k.into_bytes(),
        &authority_private_k.into_bytes(),
        CERT_VALIDITY,
    )
    .unwrap();

    let mut sender_state = State::initialized(HandshakeRole::Initiator(initiator));
    let mut receiver_state = State::initialized(HandshakeRole::Responder(responder));

    let first_message = sender_state.step_0().unwrap();
    let first_message_bytes: [u8; noise_sv2::ELLSWIFT_ENCODING_SIZE] = first_message
        .get_payload_when_handshaking()
        .try_into()
        .unwrap();

    let (second_message, receiver_state) = receiver_state.step_1(first_message_bytes).unwrap();
    let second_message_bytes: [u8; noise_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] =
        second_message
            .get_payload_when_handshaking()
            .try_into()
            .unwrap();

    let sender_state = sender_state.step_2(second_message_bytes).unwrap();

    let enc = NoiseEncoder::<TestMsg>::new();
    let dec = StandardNoiseDecoder::<TestMsg>::new();

    (enc, dec, sender_state, receiver_state)
}

#[cfg(feature = "noise_sv2")]
fn bench_noise_roundtrip(c: &mut Criterion) {
    c.bench_function("noise/roundtrip", |b| {
        let msg = TestMsg { data: 9u8 };

        b.iter(|| {
            // Setup fresh codec pair for each iteration (noise state can't be reused)
            let (mut enc, _, mut enc_state, mut dec_state) = setup_noise_codec_pair();

            // Encode
            let sv2_frame = Sv2Frame::from_message(msg.clone(), 0, 0, true).unwrap();
            let frame = Frame::Sv2(sv2_frame);
            let encrypted = enc.encode(black_box(frame), &mut enc_state).unwrap();

            // Decode
            let mut dec = StandardNoiseDecoder::<TestMsg>::new();
            let w = dec.writable();
            let len = w.len();
            w[..len].copy_from_slice(&encrypted[0..len]);
            let mut offset = len;

            loop {
                match dec.next_frame(&mut dec_state) {
                    Ok(decoded) => {
                        black_box(decoded);
                        break;
                    }
                    Err(codec_sv2::Error::MissingBytes(n)) => {
                        let w = dec.writable();
                        w[..n].copy_from_slice(&encrypted[offset..offset + n]);
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
        let (mut enc, _, mut enc_state, _) = setup_noise_codec_pair();

        let msg = TestMsg { data: 42u8 };

        b.iter(|| {
            let sv2_frame = Sv2Frame::from_message(msg.clone(), 0, 0, true).unwrap();
            let frame = Frame::Sv2(sv2_frame);
            let encrypted = enc.encode(black_box(frame), &mut enc_state).unwrap();
            black_box(encrypted);
        })
    });
}

#[cfg(feature = "noise_sv2")]
fn bench_noise_handshake_steps(c: &mut Criterion) {
    use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};

    const AUTHORITY_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
    const AUTHORITY_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
    const CERT_VALIDITY: core::time::Duration = core::time::Duration::from_secs(3600);

    c.bench_function("noise/handshake/step_0", |b| {
        b.iter(|| {
            let authority_public_k: Secp256k1PublicKey =
                AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();

            let initiator = Initiator::from_raw_k(authority_public_k.into_bytes()).unwrap();
            let mut sender_state = State::initialized(HandshakeRole::Initiator(initiator));

            let first_message = sender_state.step_0().unwrap();
            black_box(first_message);
        })
    });

    c.bench_function("noise/handshake/step_1", |b| {
        let authority_public_k: Secp256k1PublicKey =
            AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();

        let authority_private_k: Secp256k1SecretKey =
            AUTHORITY_PRIVATE_K.to_string().try_into().unwrap();

        let initiator = Initiator::from_raw_k(authority_public_k.into_bytes()).unwrap();
        let mut sender_state = State::initialized(HandshakeRole::Initiator(initiator));

        let first_message = sender_state.step_0().unwrap();
        let first_message_bytes: [u8; noise_sv2::ELLSWIFT_ENCODING_SIZE] = first_message
            .get_payload_when_handshaking()
            .try_into()
            .unwrap();

        b.iter(|| {
            let responder = Responder::from_authority_kp(
                &authority_public_k.into_bytes(),
                &authority_private_k.into_bytes(),
                CERT_VALIDITY,
            )
            .unwrap();

            let mut receiver_state = State::initialized(HandshakeRole::Responder(responder));
            let (second_message, _) = receiver_state.step_1(first_message_bytes).unwrap();
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
