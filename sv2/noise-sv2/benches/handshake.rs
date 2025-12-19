use criterion::{criterion_group, criterion_main, Criterion};
use noise_sv2::{Initiator, Responder};
use rand::{rngs::StdRng, SeedableRng};

pub fn rng() -> StdRng {
    StdRng::seed_from_u64(0xdead_beef)
}

pub fn payload(len: usize) -> Vec<u8> {
    vec![0u8; len]
}

fn bench_nx_handshake(c: &mut Criterion) {
    let mut group = c.benchmark_group("handshake");

    group.bench_function("handshake", |b| {
        b.iter_batched(
            || {
                let initiator = Initiator::without_pk().unwrap();
                let responder = Responder::new(
                    secp256k1::Keypair::from_secret_key(
                        &secp256k1::Secp256k1::new(),
                        &secp256k1::SecretKey::from_slice(&[1u8; 32]).unwrap(),
                    ),
                    60,
                );
                (initiator, responder)
            },
            |(mut initiator, mut responder)| {
                let msg_0 = initiator.step_0().unwrap();
                let (msg_2, _) = responder
                    .step_1_with_now_rng(msg_0, 10, &mut rng())
                    .unwrap();
                let _ = initiator.step_2_with_now(msg_2, 10).unwrap();
            },
            criterion::BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(handshake, bench_nx_handshake);
criterion_main!(handshake);
