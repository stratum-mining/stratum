//! # Collection of Helper Primitives
//!
//! Provides a collection of utilities and helper structures used throughout the Stratum V2
//! protocol implementation. These utilities simplify common tasks, such as ID generation and
//! management, mutex management, difficulty target calculations, merkle root calculations, and
//! more.

use bitcoin::{
    blockdata::block::{Header, Version},
    hash_types::{BlockHash, TxMerkleNode},
    hashes::sha256d::Hash as DHash,
    CompactTarget,
};
use codec_sv2::binary_sv2::U256;
use mining_sv2::Target;
use primitive_types::U256 as U256Primitive;
use std::{
    cmp::max,
    ops::Div,
    sync::{Mutex as Mutex_, MutexGuard, PoisonError},
};

use crate::errors::Error;

/// Custom synchronization primitive for managing shared mutable state.
///
/// This custom mutex implementation builds on [`std::sync::Mutex`] to enhance usability and safety
/// in concurrent environments. It provides ergonomic methods to safely access and modify inner
/// values while reducing the risk of deadlocks and panics. It is used throughout SRI applications
/// to managed shared state across multiple threads, such as tracking active mining sessions,
/// routing jobs, and managing connections safely and efficiently.
///
/// ## Advantages
/// - **Closure-Based Locking:** The `safe_lock` method encapsulates the locking process, ensuring
///   the lock is automatically released after the closure completes.
/// - **Error Handling:** `safe_lock` enforces explicit handling of potential [`PoisonError`]
///   conditions, reducing the risk of panics caused by poisoned locks.
/// - **Panic-Safe Option:** The `super_safe_lock` method provides an alternative that unwraps the
///   result of `safe_lock`, with optional runtime safeguards against panics.
/// - **Extensibility:** Includes feature-gated functionality to customize behavior, such as
///   stricter runtime checks using external tools like
///   [`no-panic`](https://github.com/dtolnay/no-panic).
#[derive(Debug)]
pub struct Mutex<T: ?Sized>(Mutex_<T>);

impl<T> Mutex<T> {
    /// Mutex safe lock.
    ///
    /// Safely locks the `Mutex` and executes a closer (`thunk`) with a mutable reference to the
    /// inner value. This ensures that the lock is automatically released after the closure
    /// completes, preventing deadlocks. It explicitly returns a [`PoisonError`] containing a
    /// [`MutexGuard`] to the inner value in cases where the lock is poisoned.
    ///
    /// To prevent poison lock errors, unwraps should never be used within the closure. The result
    /// should always be returned and handled outside of the sage lock.
    pub fn safe_lock<F, Ret>(&self, thunk: F) -> Result<Ret, PoisonError<MutexGuard<'_, T>>>
    where
        F: FnOnce(&mut T) -> Ret,
    {
        let mut lock = self.0.lock()?;
        let return_value = thunk(&mut *lock);
        drop(lock);
        Ok(return_value)
    }

    /// Mutex super safe lock.
    ///
    /// Locks the `Mutex` and executes a closure (`thunk`) with a mutable reference to the inner
    /// value, panicking if the lock is poisoned.
    ///
    /// This is a convenience wrapper around `safe_lock` for cases where explicit error handling is
    /// unnecessary or undesirable. Use with caution in production code.
    pub fn super_safe_lock<F, Ret>(&self, thunk: F) -> Ret
    where
        F: FnOnce(&mut T) -> Ret,
    {
        //#[cfg(feature = "disable_nopanic")]
        {
            self.safe_lock(thunk).unwrap()
        }
        //#[cfg(not(feature = "disable_nopanic"))]
        //{
        //    // based on https://github.com/dtolnay/no-panic
        //    struct __NoPanic;
        //    extern "C" {
        //        #[link_name = "super_safe_lock called on a function that may panic"]
        //        fn trigger() -> !;
        //    }
        //    impl core::ops::Drop for __NoPanic {
        //        fn drop(&mut self) {
        //            unsafe {
        //                trigger();
        //            }
        //        }
        //    }
        //    let mut lock = self.0.lock().expect("threads to never panic");
        //    let __guard = __NoPanic;
        //    let return_value = thunk(&mut *lock);
        //    core::mem::forget(__guard);
        //    drop(lock);
        //    return_value
        //}
    }

    /// Creates a new [`Mutex`] instance, storing the initial value inside.
    pub fn new(v: T) -> Self {
        Mutex(Mutex_::new(v))
    }

    /// Removes lock for direct access.
    ///
    /// Acquires a lock on the [`Mutex`] and returns a [`MutexGuard`] for direct access to the
    /// inner value. Allows for manual lock handling and is useful in scenarios where closures are
    /// not convenient.
    pub fn to_remove(&self) -> Result<MutexGuard<'_, T>, PoisonError<MutexGuard<'_, T>>> {
        self.0.lock()
    }
}

/// A list of potential errors during conversion between hashrate and target
#[derive(Debug)]
pub enum InputError {
    NegativeInput,
    DivisionByZero,
    ArithmeticOverflow,
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
) -> Result<U256<'static>, crate::Error> {
    // checks that we are not dividing by zero
    if share_per_min == 0.0 {
        return Err(Error::TargetError(InputError::DivisionByZero));
    }
    if share_per_min.is_sign_negative() {
        return Err(Error::TargetError(InputError::NegativeInput));
    };
    if hashrate.is_sign_negative() {
        return Err(Error::TargetError(InputError::NegativeInput));
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

/// Calculates the hashrate (H/s) required to produce a specific number of shares per minute for a
/// given mining target (big endian).
///
/// It is the inverse of [`hash_rate_to_target`], enabling backward calculations to estimate a
/// mining device's performance from its submitted shares.
///
/// Typically used to calculate the mining device's effective hashrate during runtime based on the
/// submitted shares and the assigned target, also helps detect changes in miner performance and
/// recalibrate the target (using [`hash_rate_to_target`]) if necessary.
///
/// ## Formula
/// ```text
/// h = (2^256 - t) / (s * (t + 1))
/// ```
///
/// Where:
/// - `h`: Mining device hashrate (H/s).
/// - `t`: Target threshold.
/// - `s`: Shares per minute.
pub fn hash_rate_from_target(target: U256<'static>, share_per_min: f64) -> Result<f64, Error> {
    // checks that we are not dividing by zero
    if share_per_min == 0.0 {
        return Err(Error::HashrateError(InputError::DivisionByZero));
    }
    if share_per_min.is_sign_negative() {
        return Err(Error::HashrateError(InputError::NegativeInput));
    }
    let mut target_arr: [u8; 32] = [0; 32];
    let slice: &mut [u8] = &mut target_arr;
    slice.copy_from_slice(target.inner_as_ref());
    target_arr.reverse();
    let target = U256Primitive::from_big_endian(target_arr.as_ref());
    // we calculate the numerator 2^256-t
    // note that [255_u8,;32] actually is 2^256 -1, but 2^256 -t = (2^256-1) - (t-1)
    let max_target = [255_u8; 32];
    let max_target = U256Primitive::from_big_endian(max_target.as_ref());
    let numerator = max_target - (target - U256Primitive::one());
    // now we calculate the denominator s(t+1)
    // *100 here to move the fractional bit up so we can make this an int later
    let shares_occurrency_frequence = 60_f64 / (share_per_min) * 100.0;
    // note that t+1 cannot be zero because t unsigned. Therefore the denominator is zero if and
    // only if s is zero.
    let shares_occurrency_frequence = shares_occurrency_frequence as u128;
    if shares_occurrency_frequence == 0_u128 {
        return Err(Error::HashrateError(InputError::DivisionByZero));
    }
    let shares_occurrency_frequence = from_u128_to_u256(shares_occurrency_frequence);
    let target_plus_one = U256Primitive::from_big_endian(target_arr.as_ref())
        .checked_add(U256Primitive::one())
        .ok_or(Error::HashrateError(InputError::ArithmeticOverflow))?;
    let denominator = target_plus_one
        .checked_mul(shares_occurrency_frequence)
        .and_then(|e| e.checked_div(U256Primitive::from(100)))
        .ok_or(Error::HashrateError(InputError::ArithmeticOverflow))?;
    let result = numerator.div(denominator).low_u128();
    // we multiply back by 100 so that it cancels with the same factor at the denominator
    Ok(result as f64)
}

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

/// Converts a `u128` to a [`U256`].
fn from_u128_to_u256(input: u128) -> U256Primitive {
    let input: [u8; 16] = input.to_be_bytes();
    let mut be_bytes = [0_u8; 32];
    for (i, b) in input.iter().enumerate() {
        be_bytes[16 + i] = *b;
    }
    U256Primitive::from_big_endian(be_bytes.as_ref())
}

// Returns a new `Header`.
//
// Expected endianness inputs:
// `version`     LE
// `prev_hash`   BE
// `merkle_root` BE
// `time`        BE
// `bits`        BE
// `nonce`       BE
#[allow(dead_code)]
pub(crate) fn new_header(
    version: i32,
    prev_hash: &[u8],
    merkle_root: &[u8],
    time: u32,
    bits: u32,
    nonce: u32,
) -> Result<Header, Error> {
    if prev_hash.len() != 32 {
        return Err(Error::ExpectedLen32(prev_hash.len()));
    }
    if merkle_root.len() != 32 {
        return Err(Error::ExpectedLen32(merkle_root.len()));
    }
    let mut prev_hash_arr = [0u8; 32];
    prev_hash_arr.copy_from_slice(prev_hash);
    let prev_hash = DHash::from_bytes_ref(&prev_hash_arr);

    let mut merkle_root_arr = [0u8; 32];
    merkle_root_arr.copy_from_slice(merkle_root);
    let merkle_root = DHash::from_bytes_ref(&merkle_root_arr);

    Ok(Header {
        version: Version::from_consensus(version),
        prev_blockhash: BlockHash::from_raw_hash(*prev_hash),
        merkle_root: TxMerkleNode::from_raw_hash(*merkle_root),
        time,
        bits: CompactTarget::from_consensus(bits),
        nonce,
    })
}

#[cfg(test)]
mod tests {

    use super::{hash_rate_from_target, hash_rate_to_target, *};
    use codec_sv2::binary_sv2::{Seq0255, B064K, U256};
    use rand::Rng;
    use serde::Deserialize;
    use std::{convert::TryInto, num::ParseIntError};

    fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }

    #[derive(Debug, Deserialize)]
    struct TestBlockToml {
        block_hash: String,
        version: u32,
        prev_hash: String,
        time: u32,
        merkle_root: String,
        nbits: u32,
        nonce: u32,
        coinbase_tx_prefix: String,
        coinbase_script: String,
        coinbase_tx_suffix: String,
        path: Vec<String>,
    }

    #[derive(Debug)]
    struct TestBlock<'decoder> {
        #[allow(dead_code)]
        block_hash: U256<'decoder>,
        version: u32,
        prev_hash: Vec<u8>,
        time: u32,
        merkle_root: Vec<u8>,
        nbits: u32,
        nonce: u32,
        coinbase_tx_prefix: B064K<'decoder>,
        coinbase_script: Vec<u8>,
        coinbase_tx_suffix: B064K<'decoder>,
        path: Seq0255<'decoder, U256<'decoder>>,
    }

    fn get_test_block<'decoder>() -> TestBlock<'decoder> {
        let test_file = std::fs::read_to_string("reg-test-block.toml")
            .expect("Could not read file from string");
        let block: TestBlockToml =
            toml::from_str(&test_file).expect("Could not parse toml file as `TestBlockToml`");

        // Get block hash
        let block_hash_vec =
            decode_hex(&block.block_hash).expect("Could not decode hex string to `Vec<u8>`");
        let mut block_hash_vec: [u8; 32] = block_hash_vec
            .try_into()
            .expect("Slice is incorrect length");
        block_hash_vec.reverse();
        let block_hash: U256 = block_hash_vec.into();

        // Get prev hash
        let mut prev_hash: Vec<u8> =
            decode_hex(&block.prev_hash).expect("Could not convert `String` to `&[u8]`");
        prev_hash.reverse();

        // Get Merkle root
        let mut merkle_root =
            decode_hex(&block.merkle_root).expect("Could not decode hex string to `Vec<u8>`");
        // Swap endianness to LE
        merkle_root.reverse();

        // Get Merkle path
        let mut path_vec = Vec::<U256>::new();
        for p in block.path {
            let p_vec = decode_hex(&p).expect("Could not decode hex string to `Vec<u8>`");
            let p_arr: [u8; 32] = p_vec.try_into().expect("Slice is incorrect length");
            let p_u256: U256 = (p_arr).into();
            path_vec.push(p_u256);
        }

        let path = Seq0255::new(path_vec).expect("Could not convert `Vec<U256>` to `Seq0255`");

        // Pass in coinbase as three pieces:
        //   coinbase_tx_prefix + coinbase script + coinbase_tx_suffix
        let coinbase_tx_prefix_vec = decode_hex(&block.coinbase_tx_prefix)
            .expect("Could not decode hex string to `Vec<u8>`");
        let coinbase_tx_prefix: B064K = coinbase_tx_prefix_vec
            .try_into()
            .expect("Could not convert `Vec<u8>` into `B064K`");

        let coinbase_script =
            decode_hex(&block.coinbase_script).expect("Could not decode hex `String` to `Vec<u8>`");

        let coinbase_tx_suffix_vec = decode_hex(&block.coinbase_tx_suffix)
            .expect("Could not decode hex `String` to `Vec<u8>`");
        let coinbase_tx_suffix: B064K = coinbase_tx_suffix_vec
            .try_into()
            .expect("Could not convert `Vec<u8>` to `B064K`");

        TestBlock {
            block_hash,
            version: block.version,
            prev_hash,
            time: block.time,
            merkle_root,
            nbits: block.nbits,
            nonce: block.nonce,
            coinbase_tx_prefix,
            coinbase_script,
            coinbase_tx_suffix,
            path,
        }
    }

    #[test]

    fn gets_new_header() -> Result<(), Error> {
        let block = get_test_block();

        if !block.prev_hash.len() == 32 {
            return Err(Error::ExpectedLen32(block.prev_hash.len()));
        }
        if !block.merkle_root.len() == 32 {
            return Err(Error::ExpectedLen32(block.merkle_root.len()));
        }
        let mut prev_hash_arr = [0u8; 32];
        prev_hash_arr.copy_from_slice(&block.prev_hash);
        let prev_hash = DHash::from_bytes_ref(&prev_hash_arr);

        let mut merkle_root_arr = [0u8; 32];
        merkle_root_arr.copy_from_slice(&block.merkle_root);
        let merkle_root = DHash::from_bytes_ref(&merkle_root_arr);

        let expect = Header {
            version: Version::from_consensus(block.version as i32),
            prev_blockhash: BlockHash::from_raw_hash(*prev_hash),
            merkle_root: TxMerkleNode::from_raw_hash(*merkle_root),
            time: block.time,
            bits: CompactTarget::from_consensus(block.nbits),
            nonce: block.nonce,
        };

        let actual_block = get_test_block();
        let actual = new_header(
            block.version as i32,
            &actual_block.prev_hash,
            &actual_block.merkle_root,
            block.time,
            block.nbits,
            block.nonce,
        )?;
        assert_eq!(actual, expect);
        Ok(())
    }

    #[test]
    fn test_hash_rate_to_target() {
        let mut rng = rand::thread_rng();
        let mut successes = 0;

        let hr = 10.0; // 10 h/s
        let hrs = hr * 60.0; // number of hashes in 1 minute
        let mut target = hash_rate_to_target(hr, 1.0).unwrap().to_vec();
        target.reverse();
        let target = U256Primitive::from_big_endian(&target[..]);

        let mut i: i64 = 0;
        let mut results = vec![];
        let attempts = 1000;
        while successes < attempts {
            let a: u128 = rng.gen();
            let b: u128 = rng.gen();
            let a = a.to_be_bytes();
            let b = b.to_be_bytes();
            let concat = [&a[..], &b[..]].concat().to_vec();
            i += 1;
            if U256Primitive::from_big_endian(&concat[..]) <= target {
                results.push(i);
                i = 0;
                successes += 1;
            }
        }

        let mut average: f64 = 0.0;
        for i in &results {
            average += (*i as f64) / attempts as f64;
        }
        let delta = (hrs - average) as i64;
        assert!(delta.abs() < 100);
    }

    #[test]
    fn test_hash_rate_from_target() {
        let hr = 202470.828;
        let expected_share_per_min = 1.0;
        let target = hash_rate_to_target(hr, expected_share_per_min).unwrap();
        let realized_share_per_min = expected_share_per_min * 10.0; // increase SPM by 10x
        let hash_rate = hash_rate_from_target(target.clone(), realized_share_per_min).unwrap();
        let new_hr = (hr * 10.0).trunc();

        assert!(
            hash_rate == new_hr,
            "hash_rate_from_target equation was not properly transformed"
        )
    }

    #[test]
    fn test_hash_rate_from_target_zero_share_per_min() {
        // Test division by zero error handling when share_per_min is 0.
        let hr = 202470.828;
        let expected_share_per_min = 1.0;
        let target = hash_rate_to_target(hr, expected_share_per_min).unwrap();
        let share_per_min = 0.0;
        let result = hash_rate_from_target(target, share_per_min);

        assert!(
            matches!(
                result,
                Err(Error::HashrateError(InputError::DivisionByZero))
            ),
            "Should return division by zero error"
        );
    }

    #[test]
    fn test_hash_rate_from_target_arithmetic_overflow() {
        // Test arithmetic overflow error handling with extremely low share_per_min and maximal
        // target.
        let mut target_bytes = [0xff; 32];
        target_bytes[0] = 0x7f; // Reduce the magnitude to avoid direct overflow
        let target_sv2 = U256::from(target_bytes);
        let share_per_min = 0.001;
        let result = hash_rate_from_target(target_sv2, share_per_min);

        assert!(
            matches!(
                result,
                Err(Error::HashrateError(InputError::ArithmeticOverflow))
            ),
            "Should return arithmetic overflow error"
        );
    }

    #[test]
    fn test_super_safe_lock() {
        let m = super::Mutex::new(1u32);
        m.safe_lock(|i| *i += 1).unwrap();
        // m.super_safe_lock(|i| *i = (*i).checked_add(1).unwrap()); // will not compile
        m.super_safe_lock(|i| *i = (*i).checked_add(1).unwrap_or_default()); // compiles
    }

    #[test]
    fn test_target_to_difficulty() {
        // Test target: 0x000000000004864c000000000000000000000000000000000000000000000000
        let target_bytes = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4c, 0x86, 0x04, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];
        let target = Target::from(target_bytes);
        let difficulty = target_to_difficulty(target);

        // Expected difficulty: 14484.162361
        let expected_difficulty = 14484.162361;
        let epsilon = 0.000001; // Small value for floating point comparison

        assert!(
            (difficulty - expected_difficulty).abs() < epsilon,
            "Expected difficulty {}, got {}",
            expected_difficulty,
            difficulty
        );

        let max_target_bytes = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
        ];
        let max_target = Target::from(max_target_bytes);
        let max_difficulty = target_to_difficulty(max_target);

        let expected_max_difficulty = 1.0;
        let epsilon = 0.000001; // Small value for floating point comparison

        assert!(
            (max_difficulty - expected_max_difficulty).abs() < epsilon,
            "Expected difficulty {}, got {}",
            expected_max_difficulty,
            max_difficulty
        );
    }

    #[test]
    fn test_hash_rate_from_target_with_max_target() {
        use codec_sv2::binary_sv2::U256;
        // This is the maximum value for a 256-bit unsigned integer
        let max_u128 = 340282366920938463463374607431768211455u128;
        // Compose the bytes for U256::MAX
        let mut max_bytes = [0u8; 32];
        max_bytes[..16].copy_from_slice(&max_u128.to_be_bytes());
        max_bytes[16..].copy_from_slice(&max_u128.to_be_bytes());
        let target = U256::from(max_bytes);
        let share_per_min = 4.0;
        let result = hash_rate_from_target(target, share_per_min);
        assert!(
            matches!(
                result,
                Err(Error::HashrateError(InputError::ArithmeticOverflow))
            ),
            "Expected ArithmeticOverflow error, got: {:?}",
            result
        );
    }
}
