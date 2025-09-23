use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use mining_device::FastSha256d;
use rand::{thread_rng, Rng};
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

fn bench_hasher(c: &mut Criterion) {
    let mut group = c.benchmark_group("mining_device_hasher");
    let header = random_header();

    // Baseline using rust-bitcoin block_hash()
    group.bench_function(BenchmarkId::new("baseline_block_hash", "full"), |b| {
        let mut h = header;
        b.iter(|| {
            h.nonce = h.nonce.wrapping_add(1);
            let _ = black_box(h.block_hash());
        });
    });

    // Optimized midstate+compress256
    group.bench_function(BenchmarkId::new("fast_midstate", "compress256"), |b| {
        let mut h = header;
        let mut fast = FastSha256d::from_header_static(&h);
        b.iter(|| {
            h.nonce = h.nonce.wrapping_add(1);
            let _ = black_box(fast.hash_with_nonce_time(h.nonce, h.time));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_hasher);
criterion_main!(benches);
