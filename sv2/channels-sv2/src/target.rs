//! Helper functions related to [`Target`]

extern crate alloc;
use alloc::string::String;
use binary_sv2::U256;
use bitcoin::{hash_types::BlockHash, hashes::Hash, Target};
use core::{cmp::max, fmt::Write, ops::Div};
use primitive_types::U256 as U256Primitive;

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
) -> Result<Target, HashRateToTargetError> {
    // Reject non-finite input before any arithmetic. ORDER IS LOAD-BEARING:
    // this check MUST precede the zero/negative checks below. A NaN compares
    // false to everything (`NaN == 0.0` is false) and has its sign bit clear
    // (`NaN.is_sign_negative()` is false), so it slips past every guard below
    // and reaches the `as u128` cast, which saturates silently: `NaN as u128`
    // is `0` and `f64::INFINITY as u128` is `u128::MAX`. A NaN or +inf hashrate
    // would then yield a garbage target with no error — and `+inf` is the
    // dangerous one: it casts to the maximum work, collapsing the target toward
    // zero, i.e. the HARDEST difficulty (the over-difficulty / spiral
    // direction). `-inf` is also caught here (it would otherwise return
    // `NegativeInput`); keeping all non-finite rejection in one check ahead of
    // the others is what makes the guard sound.
    if !hashrate.is_finite() || !share_per_min.is_finite() {
        return Err(HashRateToTargetError::NonFiniteInput);
    }
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

    let mut target_bytes = numerator.div(denominator).to_big_endian();
    target_bytes.reverse();
    Ok(Target::from_le_bytes(target_bytes))
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

#[derive(Debug)]
pub enum HashRateToTargetError {
    DivisionByZero,
    NegativeInput,
    /// A `hashrate` or `share_per_min` argument was non-finite (`NaN` or
    /// `±infinity`). These cast to nonsense `u128` work values (`NaN` → `0`,
    /// `+inf` → `u128::MAX`) and would silently produce a garbage target, so
    /// they are rejected before any conversion.
    NonFiniteInput,
}

#[derive(Debug)]
pub enum InputError {
    NegativeInput,
    DivisionByZero,
    ArithmeticOverflow,
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
pub fn hash_rate_from_target(target: U256<'static>, share_per_min: f64) -> Result<f64, InputError> {
    // checks that we are not dividing by zero
    if share_per_min == 0.0 {
        return Err(InputError::DivisionByZero);
    }
    if share_per_min.is_sign_negative() {
        return Err(InputError::NegativeInput);
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
        return Err(InputError::DivisionByZero);
    }
    let shares_occurrency_frequence = from_u128_to_u256(shares_occurrency_frequence);
    let target_plus_one = U256Primitive::from_big_endian(target_arr.as_ref())
        .checked_add(U256Primitive::one())
        .ok_or(InputError::ArithmeticOverflow)?;
    let denominator = target_plus_one
        .checked_mul(shares_occurrency_frequence)
        .and_then(|e| e.checked_div(U256Primitive::from(100)))
        .ok_or(InputError::ArithmeticOverflow)?;
    let result = numerator.div(denominator).low_u128();
    // we multiply back by 100 so that it cancels with the same factor at the denominator
    Ok(result as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A representative valid input still converts (regression guard: the
    // non-finite screen must not reject ordinary finite values).
    #[test]
    fn finite_input_still_converts() {
        assert!(hash_rate_to_target(1_000.0, 1.0).is_ok());
        // zero hashrate is finite and non-negative — still accepted, as before.
        assert!(hash_rate_to_target(0.0, 1.0).is_ok());
    }

    // Each non-finite hashrate is rejected with the dedicated variant.
    #[test]
    fn non_finite_hashrate_is_rejected() {
        for hashrate in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                matches!(
                    hash_rate_to_target(hashrate, 1.0),
                    Err(HashRateToTargetError::NonFiniteInput)
                ),
                "hashrate {hashrate} should be NonFiniteInput",
            );
        }
    }

    // Each non-finite share_per_min is rejected — BOTH operands are screened,
    // not just the hashrate.
    #[test]
    fn non_finite_share_per_min_is_rejected() {
        for spm in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                matches!(
                    hash_rate_to_target(1_000.0, spm),
                    Err(HashRateToTargetError::NonFiniteInput)
                ),
                "share_per_min {spm} should be NonFiniteInput",
            );
        }
    }

    // ORDER IS LOAD-BEARING. A `-inf` argument is BOTH non-finite AND
    // sign-negative; if the negative check ran first it would return
    // `NegativeInput`. Pinning `NonFiniteInput` here proves the finite screen
    // precedes the negative screen — the property that keeps the guard sound.
    #[test]
    fn neg_infinity_is_non_finite_not_negative() {
        assert!(matches!(
            hash_rate_to_target(f64::NEG_INFINITY, 1.0),
            Err(HashRateToTargetError::NonFiniteInput)
        ));
        // and a NaN share_per_min must not slip through to DivisionByZero/
        // NegativeInput (it compares false to 0.0 and is sign-positive).
        assert!(matches!(
            hash_rate_to_target(1_000.0, f64::NAN),
            Err(HashRateToTargetError::NonFiniteInput)
        ));
    }

    // The headline case: a `+inf` hashrate used to cast to `u128::MAX` work and
    // collapse the target toward zero — the HARDEST difficulty (the
    // over-difficulty / spiral direction). It must now be rejected outright,
    // never silently converted to that dangerous-direction target.
    #[test]
    fn positive_infinity_hashrate_does_not_yield_a_target() {
        assert!(matches!(
            hash_rate_to_target(f64::INFINITY, 1.0),
            Err(HashRateToTargetError::NonFiniteInput)
        ));
    }

    // The pre-existing finite guards are unchanged: a genuinely negative finite
    // hashrate is still NegativeInput, and zero share_per_min still
    // DivisionByZero — the new screen narrows nothing that worked before.
    #[test]
    fn preexisting_finite_guards_unchanged() {
        assert!(matches!(
            hash_rate_to_target(-1.0, 1.0),
            Err(HashRateToTargetError::NegativeInput)
        ));
        assert!(matches!(
            hash_rate_to_target(1_000.0, 0.0),
            Err(HashRateToTargetError::DivisionByZero)
        ));
    }
}
