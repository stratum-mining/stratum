use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use noise_sv2::{Initiator, Responder};

use crate::common::{generate_key_with_rng, rng};
mod common;

fn bench_nx_handshake(c: &mut Criterion) {
    let mut group = c.benchmark_group("handshake");

    group.bench_function("step_0_initiator", |b| {
        b.iter_batched(
            || Initiator::new(generate_key_with_rng(&mut rng()).public_key().into()),
            |mut initiator| {
                let _msg_0 = initiator.step_0().unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("step_1_responder", |b| {
        b.iter_batched(
            || {
                let mut rng = rng();
                let responder_key = generate_key_with_rng(&mut rng);
                let mut initiator = Initiator::new(responder_key.public_key().into());
                let responder = Responder::new_with_now_rng(responder_key, 60, 10, &mut rng);

                let msg_0 = initiator.step_0().unwrap();
                (responder, msg_0)
            },
            |(mut responder, msg_0)| {
                let _ = responder.step_1(msg_0).unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("step_2_initiator", |b| {
        b.iter_batched(
            || {
                let mut rng = rng();
                let responder_key = generate_key_with_rng(&mut rng);
                let mut initiator = Initiator::new(responder_key.public_key().into());
                let mut responder = Responder::new_with_now_rng(responder_key, 60, 10, &mut rng);

                let msg_0 = initiator.step_0().unwrap();
                let (msg_2, _) = responder.step_1(msg_0).unwrap();

                (initiator, msg_2)
            },
            |(mut initiator, msg_2)| {
                let _ = initiator.step_2_with_now(msg_2, 10).unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("handshake", |b| {
        b.iter_batched(
            || {
                let mut rng = rng();
                let responder_key = generate_key_with_rng(&mut rng);
                let initiator = Initiator::new(responder_key.public_key().into());
                (initiator, responder_key, rng)
            },
            |(mut initiator, responder_key, mut rng)| {
                let mut responder = Responder::new_with_now_rng(responder_key, 60, 10, &mut rng);
                let msg_0 = initiator.step_0().unwrap();
                let (msg_2, _) = responder.step_1(msg_0).unwrap();
                let _ = initiator.step_2_with_now(msg_2, 10).unwrap();
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(handshake, bench_nx_handshake);
criterion_main!(handshake);
