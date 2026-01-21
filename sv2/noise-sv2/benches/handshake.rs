use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use noise_sv2::{Initiator, Responder};

use crate::common::{generate_key_with_rng, rng};
mod common;

fn bench_nx_handshake(c: &mut Criterion) {
    let mut group = c.benchmark_group("handshake");

    group.bench_function("step_0_initiator", |b| {
        b.iter_batched(
            || Initiator::new(None),
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
                let mut initiator = Initiator::new(None);
                let responder = Responder::new(responder_key, 60);

                let msg_0 = initiator.step_0().unwrap();
                (responder, msg_0, rng)
            },
            |(mut responder, msg_0, mut rng)| {
                let _ = responder.step_1_with_now_rng(msg_0, 10, &mut rng).unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("step_2_initiator", |b| {
        b.iter_batched(
            || {
                let mut rng = rng();
                let responder_key = generate_key_with_rng(&mut rng);
                let mut initiator = Initiator::new(None);
                let mut responder = Responder::new(responder_key, 60);

                let msg_0 = initiator.step_0().unwrap();
                let (msg_2, _) = responder.step_1_with_now_rng(msg_0, 10, &mut rng).unwrap();

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
                let initiator = Initiator::new(None);
                let responder = Responder::new(responder_key, 60);
                (initiator, responder, rng)
            },
            |(mut initiator, mut responder, mut rng)| {
                let msg_0 = initiator.step_0().unwrap();
                let (msg_2, _) = responder.step_1_with_now_rng(msg_0, 10, &mut rng).unwrap();
                let _ = initiator.step_2_with_now(msg_2, 10).unwrap();
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(handshake, bench_nx_handshake);
criterion_main!(handshake);
