pub fn is_outdated(start_timestamp_secs: u64, max_delta: u32) -> bool {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs();
    (now_secs - start_timestamp_secs) as u32 >= max_delta
}
