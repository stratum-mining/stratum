use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mining_device::FastSha256d;
use rand::{thread_rng, Rng};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier,
    },
    thread,
    time::Instant,
};
use stratum_common::roles_logic_sv2::bitcoin::{
    block::Version, blockdata::block::Header, hash_types::BlockHash, hashes::Hash, CompactTarget,
};

fn random_header() -> Header {
    let mut rng = thread_rng();
    let prev_hash: [u8; 32] = rng.gen();
    let prev_hash = Hash::from_byte_array(prev_hash);
    let merkle_root: [u8; 32] = rng.gen();
    let merkle_root = Hash::from_byte_array(merkle_root);
    Header {
        version: Version::from_consensus(rng.gen::<i32>()),
        prev_blockhash: BlockHash::from_raw_hash(prev_hash),
        merkle_root,
        time: std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(60))
            .unwrap()
            .as_secs() as u32,
        bits: CompactTarget::from_consensus(rng.gen()),
        nonce: 0,
    }
}

fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("mining_device_scaling");
    // Measure logical CPUs and test scaling from 1..=N
    let logical_cpus = num_cpus::get().max(1);
    let workers: Vec<usize> = (1..=logical_cpus).collect();

    // Keep runs short but representative
    group.sample_size(10);
    group.warm_up_time(std::time::Duration::from_millis(100));
    group.measurement_time(std::time::Duration::from_secs(1));

    let header = random_header();

    // Helper: quick one-shot timing used only for concise logging (outside Criterion loop)
    let quick_measure_total_mhps = |n: usize| -> f64 {
        // Each measurement hashes this many nonces total across n threads
        let per_thread: u32 = 200_000 / (n as u32).max(1);
        let total_hashes = per_thread as u64 * n as u64;
        let stop = Arc::new(AtomicBool::new(false));
        let barrier = Arc::new(Barrier::new(n));
        let mut handles = Vec::with_capacity(n);
        for i in 0..n {
            let stop = stop.clone();
            let barrier = barrier.clone();
            let mut h = header;
            h.nonce = i as u32;
            let mut fast = FastSha256d::from_header_static(&h);
            let per = per_thread;
            handles.push(thread::spawn(move || {
                barrier.wait();
                let start = Instant::now();
                let time = h.time;
                let mut nonce = h.nonce;
                for _ in 0..per {
                    let _ = black_box(fast.hash_with_nonce_time(nonce, time));
                    nonce = nonce.wrapping_add(n as u32); // stride to avoid overlap
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                }
                start.elapsed()
            }));
        }
        let mut max_elapsed = std::time::Duration::ZERO;
        for h in handles {
            let d = h.join().unwrap();
            if d > max_elapsed {
                max_elapsed = d;
            }
        }
        let secs = max_elapsed.as_secs_f64().max(1e-9);
        let hps = (total_hashes as f64) / secs;
        hps / 1_000_000.0
    };

    // Print one concise line per worker count, including incremental gain vs previous
    let mut prev_workers: Option<usize> = None;
    let mut prev_mhps: Option<f64> = None;

    for &n in &workers {
        // Each iteration hashes this many nonces total across n threads
        let per_thread: u32 = 200_000 / (n as u32).max(1);
        let total_hashes = per_thread as u64 * n as u64;
        group.throughput(Throughput::Elements(total_hashes));

        // One-shot concise summary print (not part of Criterion timing)
        let mhps = quick_measure_total_mhps(n);
        if let (Some(pn), Some(prev)) = (prev_workers, prev_mhps) {
            let added = n.saturating_sub(pn).max(1);
            let delta = mhps - prev;
            let pct = if prev > 0.0 {
                (delta / prev) * 100.0
            } else {
                0.0
            };
            let per_cpu = delta / (added as f64);
            println!(
                "workers={n}: ~{mhps:.3} MH/s (total) | +{delta:.3} vs prev (+{pct:.1}%), ~{per_cpu:.3} MH/s per added worker"
            );
        } else {
            println!("workers={n}: ~{mhps:.3} MH/s (total)");
        }
        prev_workers = Some(n);
        prev_mhps = Some(mhps);

        group.bench_function(BenchmarkId::from_parameter(n), |b| {
            b.iter(|| {
                let stop = Arc::new(AtomicBool::new(false));
                let barrier = Arc::new(Barrier::new(n));
                let mut handles = Vec::with_capacity(n);
                for i in 0..n {
                    let stop = stop.clone();
                    let barrier = barrier.clone();
                    let mut h = header;
                    h.nonce = i as u32;
                    let mut fast = FastSha256d::from_header_static(&h);
                    let per = per_thread;
                    handles.push(thread::spawn(move || {
                        // start together
                        barrier.wait();
                        let start = Instant::now();
                        let time = h.time;
                        let mut nonce = h.nonce;
                        for _ in 0..per {
                            // One hash per step; inner batching isn't necessary here
                            let _ = black_box(fast.hash_with_nonce_time(nonce, time));
                            nonce = nonce.wrapping_add(n as u32); // stride to avoid overlap
                            if stop.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                        start.elapsed()
                    }));
                }
                // Collect times and compute MH/s
                let mut max_elapsed = std::time::Duration::ZERO;
                for h in handles {
                    let d = h.join().unwrap();
                    if d > max_elapsed {
                        max_elapsed = d;
                    }
                }
                let _secs = max_elapsed.as_secs_f64().max(1e-9);
                let _hps = (total_hashes as f64) / _secs;
                let _mhps = _hps / 1_000_000.0;
                // Intentionally no println! inside Criterion iteration to keep output concise
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_scaling);
criterion_main!(benches);
