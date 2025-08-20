//! Helper functions related to [`Target`]

extern crate alloc;
use alloc::string::String;
use binary_sv2::U256;
use bitcoin::{hash_types::BlockHash, hashes::Hash};
use core::{cmp::max, fmt::Write, ops::Div};
use mining_sv2::Target;
use primitive_types::U256 as U256Primitive;
/// Converts a `Target` to a `f64` difficulty.
pub fn target_to_difficulty(target: Target) -> f64 {
    // Genesis block target: 0x00000000ffff0000000000000000000000000000000000000000000000000000
    // (in little endian)
    let max_target_bytes = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00,
        0x00, 0x00,
    ];
    let max_target = U256Primitive::from_little_endian(&max_target_bytes);

    // Convert input target to U256Primitive
    let target_u256: U256<'static> = target.into();
    let mut target_bytes = [0u8; 32];
    target_bytes.copy_from_slice(target_u256.inner_as_ref());
    let target = U256Primitive::from_little_endian(&target_bytes);

    // Calculate difficulty = max_target / target
    // We need to handle the full 256-bit values properly
    // Convert to f64 by taking the ratio of the most significant bits
    let max_target_high = (max_target >> 128).low_u128() as f64;
    let max_target_low = max_target.low_u128() as f64;
    let target_high = (target >> 128).low_u128() as f64;
    let target_low = target.low_u128() as f64;

    // Combine high and low parts with appropriate scaling
    let max_target_f64 = max_target_high * (2.0f64.powi(128)) + max_target_low;
    let target_f64 = target_high * (2.0f64.powi(128)) + target_low;

    max_target_f64 / target_f64
}

/// Converts a `u256` to a [`BlockHash`] type.
pub fn u256_to_block_hash(v: U256<'static>) -> BlockHash {
    let hash: [u8; 32] = v.to_vec().try_into().unwrap();
    let hash = Hash::from_slice(&hash).unwrap();
    BlockHash::from_raw_hash(hash)
}

/// Helper function to format bytes as hex string
/// useful for visualizing targets
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{b:02x}")
            .expect("Writing hex bytes to pre-allocated string should never fail");
    }
    s
}

/// Calculates the mining target threshold for a mining device based on its hashrate (H/s) and
/// desired share frequency (shares/min).
///
/// Determines the maximum hash value (target), in big endian, that a mining device can produce to
/// find a valid share. The target is derived from the miner's hashrate and the expected number of
/// shares per minute, aligning the miner's workload with the upstream's (e.g. pool's) share
/// frequency requirements.
///
/// Typically used during connection setup to assign a starting target based on the mining device's
/// reported hashrate and to recalculate during runtime when a mining device's hashrate changes,
/// ensuring they submit shares at the desired rate.
///
/// ## Formula
/// ```text
/// t = (2^256 - sh) / (sh + 1)
/// ```
///
/// Where:
/// - `h`: Mining device hashrate (H/s).
/// - `s`: Shares per second `60 / shares/min` (s).
/// - `sh`: `h * s`, the mining device's work over `s` seconds.
///
/// According to \[1] and \[2], it is possible to model the probability of finding a block with
/// a random variable X whose distribution is negative hypergeometric \[3]. Such a variable is
/// characterized as follows:
///
/// Say that there are `n` (`2^256`) elements (possible hash values), of which `t` (values <=
/// target) are defined as success and the remaining as failures. The variable `X` has co-domain
/// the positive integers, and `X=k` is the event where element are drawn one after the other,
/// without replacement, and only the `k`th element is successful. The expected value of this
/// variable is `(n-t)/(t+1)`. So, on average, a miner has to perform `(2^256-t)/(t+1)` hashes
/// before finding hash whose value is below the target `t`.
///
/// If the pool wants, on average, a share every `s` seconds, then, on average, the miner has to
/// perform `h*s` hashes before finding one that is smaller than the target, where `h` is the
/// miner's hashrate. Therefore, `s*h= (2^256-t)/(t+1)`. If we consider `h` the global Bitcoin's
/// hashrate, `s = 600` seconds and `t` the Bitcoin global target, then, for all the blocks we
/// tried, the two members of the equations have the same order of magnitude and, most of the
/// cases, they coincide with the first two digits.
///
/// We take this as evidence of the correctness of our calculations. Thus, if the pool wants on
/// average a share every `s` seconds from a miner with hashrate `h`, then the target `t` for the
/// miner is `t = (2^256-sh)/(sh+1)`.
///
/// \[1] [https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3399742](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3399742)
///
/// \[2] [https://www.zora.uzh.ch/id/eprint/173483/1/SSRN-id3399742-2.pdf](https://www.zora.uzh.ch/id/eprint/173483/1/SSRN-id3399742-2.pdf)
///
/// \[3] [https://en.wikipedia.org/wiki/Negative_hypergeometric_distribution](https://en.wikipedia.org/wiki/Negative_hypergeometric_distribution)
pub fn hash_rate_to_target(
    hashrate: f64,
    share_per_min: f64,
) -> Result<U256<'static>, HashRateToTargetError> {
    // checks that we are not dividing by zero
    if share_per_min == 0.0 {
        return Err(HashRateToTargetError::DivisionByZero);
    }
    if share_per_min.is_sign_negative() {
        return Err(HashRateToTargetError::NegativeInput);
    };
    if hashrate.is_sign_negative() {
        return Err(HashRateToTargetError::NegativeInput);
    };

    // if we want 5 shares per minute, this means that s=60/5=12 seconds interval between shares
    // this quantity will be at the numerator, so we multiply the result by 100 again later
    let shares_occurrency_frequence = 60_f64 / share_per_min;

    let h_times_s = hashrate * shares_occurrency_frequence;
    let h_times_s = h_times_s as u128;

    // We calculate the denominator: h*s+1
    // the denominator is h*s+1, where h*s is an u128, so always positive.
    // this means that the denominator can never be zero
    // we add 100 in place of 1 because h*s is actually h*s*100, we in order to simplify later we
    // must calculate (h*s+1)*100
    let h_times_s_plus_one = max(h_times_s, h_times_s + 1);

    let h_times_s_plus_one = from_u128_to_u256(h_times_s_plus_one);
    let denominator = h_times_s_plus_one;

    // We calculate the numerator: 2^256-sh
    let two_to_256_minus_one = [255_u8; 32];
    let two_to_256_minus_one = U256Primitive::from_big_endian(two_to_256_minus_one.as_ref());

    let mut h_times_s_array = [0u8; 32];
    h_times_s_array[16..].copy_from_slice(&h_times_s.to_be_bytes());
    let numerator = two_to_256_minus_one - U256Primitive::from_big_endian(h_times_s_array.as_ref());

    let mut target = numerator.div(denominator).to_big_endian();
    target.reverse();
    Ok(U256::<'static>::from(target))
}

/// Converts a `u128` to a [`U256`].
pub fn from_u128_to_u256(input: u128) -> U256Primitive {
    let input: [u8; 16] = input.to_be_bytes();
    let mut be_bytes = [0_u8; 32];
    for (i, b) in input.iter().enumerate() {
        be_bytes[16 + i] = *b;
    }
    U256Primitive::from_big_endian(be_bytes.as_ref())
}

pub enum HashRateToTargetError {
    DivisionByZero,
    NegativeInput,
}
