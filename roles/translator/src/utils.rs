pub fn is_outdated(start_timestamp_secs: u64, max_delta: u32) -> bool {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs();
    (now_secs - start_timestamp_secs) as u32 >= max_delta
}

/// currently the pool only supports 16 bytes exactly for its channels
/// to use but that may change
pub fn proxy_extranonce1_len(channel_extranonce2_size: usize, downstream_extranonce2_len: usize) -> usize {
    // full_extranonce_len - pool_extranonce1_len - miner_extranonce2 = tproxy_extranonce1_len
    channel_extranonce2_size - downstream_extranonce2_len
}
