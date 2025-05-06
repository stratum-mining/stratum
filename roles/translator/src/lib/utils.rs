/// Calculates the required length of the proxy's extranonce1.
///
/// The proxy needs to calculate an extranonce1 value to send to the
/// upstream server.  This function determines the length of that
/// extranonce1 value
/// FIXME: The pool only supported 16 bytes exactly for its
/// `extranonce1` field is no longer the case and the
/// code needs to be changed to support variable `extranonce1` lengths.
pub fn proxy_extranonce1_len(
    channel_extranonce2_size: usize,
    downstream_extranonce2_len: usize,
) -> usize {
    // full_extranonce_len - pool_extranonce1_len - miner_extranonce2 = tproxy_extranonce1_len
    channel_extranonce2_size - downstream_extranonce2_len
}
