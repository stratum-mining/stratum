use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use noise_sv2::{Initiator, Responder};

use crate::common::{generate_key_with_rng, payload, rng};
mod common;

fn bench_encryption_roundtrip(c: &mut Criterion) {
    let responder_key = generate_key_with_rng(&mut rng());
    let mut initiator = Initiator::new(None);
    let mut responder = Responder::new(responder_key, 10);

    let msg_0 = initiator.step_0().unwrap();
    let (msg_2, mut responder_transport) = responder.step_1(msg_0).unwrap();
    let mut initiator_transport = initiator.step_2(msg_2).unwrap();

    for size in [64usize, 256, 1024, 4096] {
        c.bench_function(&format!("transport/roundtrip/{size}B"), |b| {
            b.iter_batched(
                || payload(size),
                |mut msg| {
                    initiator_transport.encrypt(&mut msg).unwrap();
                    responder_transport.decrypt(&mut msg).unwrap();
                    responder_transport.encrypt(&mut msg).unwrap();
                    initiator_transport.decrypt(&mut msg).unwrap();
                },
                BatchSize::SmallInput,
            )
        });
    }
}

criterion_group!(roundtrip, bench_encryption_roundtrip);
criterion_main!(roundtrip);
