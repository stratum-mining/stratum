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

#[test]
fn fast_hasher_matches_baseline() {
    let mut h = random_header();
    let mut fast = FastSha256d::from_header_static(&h);

    for _ in 0..1000 {
        // Advance nonce, occasionally tweak time
        h.nonce = h.nonce.wrapping_add(1);
        if h.nonce % 128 == 0 {
            h.time = h.time.wrapping_add(1);
        }
        let fast_hash = fast.hash_with_nonce_time(h.nonce, h.time);
        let baseline: [u8; 32] = *h.block_hash().to_raw_hash().as_ref();
        assert_eq!(fast_hash, baseline, "Fast hasher must match baseline");
    }
}
