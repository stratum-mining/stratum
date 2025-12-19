use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use noise_sv2::{Initiator, Responder};
use rand::thread_rng;
use secp256k1::{Keypair, Parity, Secp256k1};

pub fn generate_key() -> Keypair {
    let secp = Secp256k1::new();
    let mut rng = thread_rng();

    loop {
        let (secret_key, _) = secp.generate_keypair(&mut rng);
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        if keypair.x_only_public_key().1 == Parity::Even {
            return keypair;
        }
    }
}

pub fn payload(len: usize) -> Vec<u8> {
    vec![0u8; len]
}

fn bench_codec_roundtrip(c: &mut Criterion) {
    let responder_key = generate_key();
    let mut initiator = Initiator::new(None);
    let mut responder = Responder::new(responder_key, 10);

    let msg_0 = initiator.step_0().unwrap();
    let (msg_2, mut responder_codec) = responder.step_1(msg_0).unwrap();
    let mut initiator_codec = initiator.step_2(msg_2).unwrap();

    for size in [64usize, 256, 1024, 4096] {
        c.bench_function(&format!("codec/roundtrip/{size}B"), |b| {
            b.iter_batched(
                || payload(size),
                |mut msg| {
                    initiator_codec.encrypt(&mut msg).unwrap();
                    responder_codec.decrypt(&mut msg).unwrap();
                    responder_codec.encrypt(&mut msg).unwrap();
                    initiator_codec.decrypt(&mut msg).unwrap();
                },
                BatchSize::SmallInput,
            )
        });
    }
}

criterion_group!(codec, bench_codec_roundtrip);
criterion_main!(codec);
