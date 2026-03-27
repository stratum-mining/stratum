extern crate alloc;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use framing_sv2::framing::Sv2Frame;

use codec_sv2::Encoder;

#[cfg(feature = "noise_sv2")]
use codec_sv2::{HandshakeRole, NoiseEncoder, State};
#[cfg(feature = "noise_sv2")]
use noise_sv2::{Initiator, Responder};

mod common;
use common::TestMsg;

fn bench_plain_encoder(c: &mut Criterion) {
    c.bench_function("encoder/plain", |b| {
        let msg = TestMsg { data: 42u8 };
        let mut enc = Encoder::<TestMsg>::new();
        b.iter(|| {
            let frame = Sv2Frame::from_message(msg.clone(), 0, 0, true).unwrap();
            let out = enc.encode(black_box(frame)).unwrap();
            black_box(out);
        })
    });
}

fn bench_encoder_creation(c: &mut Criterion) {
    c.bench_function("encoder/creation/plain", |b| {
        b.iter(|| {
            let enc = Encoder::<TestMsg>::new();
            black_box(enc);
        })
    });
}

#[cfg(feature = "noise_sv2")]
fn setup_noise_states() -> (State, State) {
    use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};

    const AUTHORITY_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
    const AUTHORITY_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
    const CERT_VALIDITY: core::time::Duration = core::time::Duration::from_secs(3600);

    let authority_public_k: Secp256k1PublicKey = AUTHORITY_PUBLIC_K
        .to_string()
        .try_into()
        .expect("Failed to convert public key");

    let authority_private_k: Secp256k1SecretKey = AUTHORITY_PRIVATE_K
        .to_string()
        .try_into()
        .expect("Failed to convert private key");

    let initiator =
        Initiator::from_raw_k(authority_public_k.into_bytes()).expect("Failed to create initiator");

    let responder = Responder::from_authority_kp(
        &authority_public_k.into_bytes(),
        &authority_private_k.into_bytes(),
        CERT_VALIDITY,
    )
    .expect("Failed to create responder");

    let mut sender_state = State::initialized(HandshakeRole::Initiator(initiator));
    let mut receiver_state = State::initialized(HandshakeRole::Responder(responder));

    // Complete handshake
    let first_message = sender_state.step_0().expect("Step 0 failed");
    let first_message_bytes: [u8; noise_sv2::ELLSWIFT_ENCODING_SIZE] = first_message
        .get_payload_when_handshaking()
        .try_into()
        .expect("Invalid handshake message");

    let (second_message, receiver_state) = receiver_state
        .step_1(first_message_bytes)
        .expect("Step 1 failed");
    let second_message_bytes: [u8; noise_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] =
        second_message
            .get_payload_when_handshaking()
            .try_into()
            .expect("Invalid handshake message");

    let sender_state = sender_state
        .step_2(second_message_bytes)
        .expect("Step 2 failed");

    (sender_state, receiver_state)
}

#[cfg(feature = "noise_sv2")]
fn bench_noise_encoder_transport(c: &mut Criterion) {
    c.bench_function("encoder/noise/transport", |b| {
        let (sender_state, _) = setup_noise_states();
        let mut state = sender_state;
        let msg = TestMsg { data: 42u8 };
        let mut enc = NoiseEncoder::<TestMsg>::new();

        b.iter(|| {
            let sv2_frame = Sv2Frame::from_message(msg.clone(), 0, 0, true).unwrap();
            let frame = framing_sv2::framing::Frame::Sv2(sv2_frame);
            let out = enc.encode(black_box(frame), &mut state).unwrap();
            black_box(out);
        })
    });
}

#[cfg(feature = "noise_sv2")]
fn bench_noise_encoder_creation(c: &mut Criterion) {
    c.bench_function("encoder/creation/noise", |b| {
        b.iter(|| {
            let enc = NoiseEncoder::<TestMsg>::new();
            black_box(enc);
        })
    });
}

#[cfg(feature = "noise_sv2")]
fn bench_noise_handshake(c: &mut Criterion) {
    use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};

    const AUTHORITY_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
    const AUTHORITY_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
    const CERT_VALIDITY: core::time::Duration = core::time::Duration::from_secs(3600);

    c.bench_function("encoder/noise/handshake/complete", |b| {
        b.iter(|| {
            let authority_public_k: Secp256k1PublicKey =
                AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();

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

            let (second_message, receiver_state) =
                receiver_state.step_1(first_message_bytes).unwrap();
            let second_message_bytes: [u8; noise_sv2::INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] =
                second_message
                    .get_payload_when_handshaking()
                    .try_into()
                    .unwrap();

            let sender_state = sender_state.step_2(second_message_bytes).unwrap();
            black_box((sender_state, receiver_state));
        })
    });
}

#[cfg(feature = "noise_sv2")]
criterion_group!(
    encoder_benches,
    bench_plain_encoder,
    bench_encoder_creation,
    bench_noise_encoder_transport,
    bench_noise_encoder_creation,
    bench_noise_handshake
);

#[cfg(not(feature = "noise_sv2"))]
criterion_group!(encoder_benches, bench_plain_encoder, bench_encoder_creation);

criterion_main!(encoder_benches);
