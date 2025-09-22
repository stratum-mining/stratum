use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mining_device::{set_nonces_per_call, FastSha256d};
use rand::{thread_rng, Rng};
use std::time::Duration;
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

fn bench_microbatch(c: &mut Criterion) {
    // Report hardware SHA availability once at start
    #[cfg(target_arch = "x86_64")]
    println!(
        "Hardware SHA available (x86 SHA-NI): {}",
        std::is_x86_feature_detected!("sha")
    );
    #[cfg(target_arch = "aarch64")]
    println!(
        "Hardware SHA available (ARMv8 SHA2): {}",
        std::arch::is_aarch64_feature_detected!("sha2")
    );
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    println!("Hardware SHA detection: not applicable for this arch");

    let mut group = c.benchmark_group("mining_device_microbatch");
    // Keep output and run-time concise
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(100));
    group.measurement_time(Duration::from_secs(1));
    let header = random_header();
    let mut fast = FastSha256d::from_header_static(&header);
    // Fewer defaults for less verbose output; allow override via env var
    let batches: Vec<u32> = std::env::var("MINING_DEVICE_BATCH_SIZES")
        .ok()
        .and_then(|s| {
            s.split(',')
                .map(|p| p.trim().parse::<u32>().ok())
                .collect::<Option<Vec<u32>>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec![1, 8, 32, 128]);

    for &b in &batches {
        group.throughput(Throughput::Elements(b as u64));
        group.bench_function(BenchmarkId::from_parameter(b), |bencher| {
            set_nonces_per_call(b);
            let mut h = header;
            bencher.iter(|| {
                // Simulate one mining-loop iteration: hash "b" nonces
                let start = h.nonce;
                let time = h.time;
                for i in 0..b {
                    let hsh = fast.hash_with_nonce_time(start.wrapping_add(i), time);
                    black_box(hsh);
                }
                h.nonce = start.wrapping_add(b);
            });
        });

        // Print a concise MH/s estimate per configuration (outside Criterion's stats)
        // Do a quick one-shot timing over a small fixed workload to avoid noisy output.
        // Note: This is a convenience display; for rigorous numbers, rely on Criterion results.
        let mut h = header;
        set_nonces_per_call(b);
        let reps: u32 = 200_000 / b.max(1); // ~200k hashes in total; fast and stable
        let total_hashes: u64 = reps as u64 * b as u64;
        let start_inst = std::time::Instant::now();
        for _ in 0..reps {
            let start = h.nonce;
            let time = h.time;
            for i in 0..b {
                let _ = black_box(fast.hash_with_nonce_time(start.wrapping_add(i), time));
            }
            h.nonce = start.wrapping_add(b);
        }
        let dur = start_inst.elapsed();
        let secs = dur.as_secs_f64().max(1e-9);
        let hps = (total_hashes as f64) / secs; // hashes per second
        let mhps = hps / 1_000_000.0;
        println!("batch={b}: ~{mhps:.3} MH/s");
    }

    group.finish();
}

criterion_group!(benches, bench_microbatch);
criterion_main!(benches);
