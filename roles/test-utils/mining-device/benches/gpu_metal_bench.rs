#![cfg(all(target_os = "macos", feature = "gpu-metal"))]

use criterion::{criterion_group, criterion_main, Criterion};
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

fn bench_gpu(c: &mut Criterion) {
    if let Ok((tew, max_tg)) = mining_device::gpu_device_info() {
        println!(
            "Metal device: thread_execution_width={tew}, max_total_threads_per_threadgroup={max_tg}"
        );
    }
    let header = random_header();
    let mut fast = FastSha256d::from_header_static(&header);
    // Reconstruct inputs for the GPU kernel from the CPU fast-hasher state by doing one hash and
    // capturing template
    let time = header.time;
    let nonce = header.nonce;
    let _ = fast.hash_with_nonce_time(nonce, time);

    // Unsafe: extract private fields via serialize path again to build block1 and midstate
    // consistently Rebuild the block1 and midstate with the same logic in
    // FastSha256d::from_header_static For simplicity in this bench, we redo it locally rather
    // than exposing internals.
    let ser = stratum_common::roles_logic_sv2::bitcoin::consensus::encode::serialize(&header);
    let mut header_bytes = [0u8; 80];
    header_bytes.copy_from_slice(&ser);
    let chunk0 = &header_bytes[0..64];
    let chunk1_last16 = &header_bytes[64..80];
    let mut state0 = {
        fn init() -> [u32; 8] {
            [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ]
        }
        let mut st = init();
        let mut block = [0u8; 64];
        block.copy_from_slice(chunk0);
        let ga0 = sha2::digest::generic_array::GenericArray::<u8, sha2::digest::typenum::U64>::clone_from_slice(&block);
        sha2::compress256(&mut st, std::slice::from_ref(&ga0));
        st
    };
    let mut block1 = [0u8; 64];
    block1[0..16].copy_from_slice(chunk1_last16);
    block1[16] = 0x80;
    block1[56..64].copy_from_slice(&640u64.to_be_bytes());

    // Target = zero so we don't trigger real submits; just measure speed
    let target = [0u8; 32];

    // Convert midstate to host u32 array (already big-endian words per SHA-256 state)
    let midstate = state0;

    // Sweep thread configurations to find saturation; keep per_thread constant initially
    let configs: &[(u32, u32)] = &[
        (1024, 20000),
        (2048, 20000),
        (4096, 20000),
        (8192, 20000),
        (16384, 20000),
        (32768, 20000),
        (65536, 20000),
        (131072, 20000),
        (262144, 20000),
    ];
    for &(threads, per_thread) in configs {
        match mining_device::gpu_measure_metal(
            midstate, block1, target, time, nonce, threads, per_thread,
        ) {
            Ok(mhps) => {
                println!(
                    "Metal GPU: threads={threads}, per_thread={per_thread} -> ~{mhps:.2} MH/s"
                );
            }
            Err(e) => {
                println!("Metal GPU init/dispatch failed: {e}");
            }
        }
    }
    // Also add a trivial criterion marker (not heavily used here)
    let mut group = c.benchmark_group("gpu_metal_probe");
    group.bench_function("noop", |b| b.iter(|| 1));
    group.finish();
}

criterion_group!(benches, bench_gpu);
criterion_main!(benches);
