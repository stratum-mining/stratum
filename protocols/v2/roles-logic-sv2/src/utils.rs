//! # Collection of Helper Primitives
//!
//! Provides a collection of utilities and helper structures used throughout the Stratum V2
//! protocol implementation. These utilities simplify common tasks, such as ID generation and
//! management, mutex management, difficulty target calculations, merkle root calculations, and
//! more.

use binary_sv2::{Seq064K, ShortTxId, B064K, U256};
use bitcoin::Block;
use job_declaration_sv2::{DeclareMiningJob, SubmitSolutionJd};
use primitive_types::U256 as U256Primitive;
use siphasher::sip::SipHasher24;
use std::{
    cmp::max,
    convert::{TryFrom, TryInto},
    ops::{Div, Mul},
    str::FromStr,
    sync::{Mutex as Mutex_, MutexGuard, PoisonError},
};
use stratum_common::{
    bitcoin,
    bitcoin::{
        absolute::LockTime,
        blockdata::block::{Header, Version},
        consensus,
        hash_types::{BlockHash, TxMerkleNode},
        hashes::{sha256, sha256d::Hash as DHash, Hash},
        secp256k1::{All, Secp256k1},
        transaction::Version as TxVersion,
        CompactTarget, OutPoint, PublicKey, ScriptBuf, ScriptHash, Transaction, TxIn, TxOut,
        WScriptHash, Witness, XOnlyPublicKey,
    },
};
use tracing::{debug, error};

use crate::errors::Error;

/// Generator of unique IDs for channels and groups.
///
/// It keeps an internal counter, which is incremented every time a new unique id is requested.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Id {
    state: u32,
}

impl Id {
    /// Creates a new [`Id`] instance initialized to `0`.
    pub fn new() -> Self {
        Self { state: 0 }
    }

    /// Increments then returns the internal state on a new ID.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.state += 1;
        self.state
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

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

#[derive(Clone)]
pub struct Coinbase {
    pub tx: Transaction,
    // used to truncate the coinbase_tx_prefix
    pub script_sig_prefix_len: usize,
}

impl Coinbase {
    pub fn new(
        script_sig_prefix: Vec<u8>,
        version: i32,
        lock_time: u32,
        sequence: u32,
        coinbase_outputs: Vec<TxOut>,
        additional_coinbase_script_data: Vec<u8>,
        extranonce_len: u8,
    ) -> Self {
        let mut script_sig = script_sig_prefix.clone();
        script_sig.extend_from_slice(&additional_coinbase_script_data);
        script_sig.extend_from_slice(&vec![0; extranonce_len as usize]);
        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: script_sig.into(),
            sequence: bitcoin::Sequence(sequence),
            witness: Witness::from(vec![vec![0; 32]]),
        };
        let tx = Transaction {
            version: TxVersion::non_standard(version),
            lock_time: LockTime::from_consensus(lock_time),
            input: vec![tx_in],
            output: coinbase_outputs.to_vec(),
        };

        Self {
            tx,
            // TODO: move additional_coinbase_script_data to extranonce_prefix
            // (part of original PR #1248)
            script_sig_prefix_len: script_sig_prefix.len() + additional_coinbase_script_data.len(),
        }
    }

    // serialize input from Transaction object
    pub fn serialized_input(&self) -> Vec<u8> {
        let input = self.tx.input[0].clone(); // coinbase only has one input
        let mut serialized_input = Vec::new();
        serialized_input.extend_from_slice(input.previous_output.txid.as_byte_array());
        serialized_input.extend_from_slice(&input.previous_output.vout.to_le_bytes());
        serialized_input.push(input.script_sig.len() as u8);
        serialized_input.extend_from_slice(input.script_sig.as_bytes());
        serialized_input.extend_from_slice(&input.sequence.0.to_le_bytes());
        serialized_input
    }

    // serialize outputs from Transaction object
    fn serialized_outputs(&self) -> Vec<Vec<u8>> {
        self.tx.output.iter().map(consensus::serialize).collect()
    }

    // coinbase_tx_prefix is the LE bytes concatenation of the following coinbase fields:
    // - tx version
    // - input count
    // - input outPoint
    // - input index (vout)
    // - scriptSig length
    // - scriptSig prefix
    //
    // we skip the bip141 marker and flag (which usually come after tx version)
    // because those are only used for computing the `wtxid`
    // while the legacy `txid` is what is actually used for computing the merkle root
    pub fn coinbase_tx_prefix(&self) -> Result<B064K<'static>, Error> {
        let mut coinbase_tx_prefix: Vec<u8> = Vec::new();
        coinbase_tx_prefix.extend_from_slice(&self.tx.version.0.to_le_bytes());
        // skip BIP141/segwit marker and flag bytes
        coinbase_tx_prefix.push(1u8); // input count

        // truncate serialized input since coinbase_tx_prefix ends:
        // - right after script_sig_prefix ends
        // - right before extranonce starts
        let mut truncated_serialized_input = self.serialized_input();
        let truncated_input_len =
            32 // outpoint
                + 4 // input index
                + 1 // script length byte
                + self.script_sig_prefix_len // space for script_sig_prefix
            ;
        truncated_serialized_input.truncate(truncated_input_len);

        coinbase_tx_prefix.extend_from_slice(&truncated_serialized_input);
        coinbase_tx_prefix.try_into().map_err(Error::BinarySv2Error)
    }

    // coinbase_tx_suffix is the LE bytes concatenation of the following coinbase fields:
    // - input sequence
    // - serialized outputs
    // - locktime
    //
    // we do not use the witnesses (usually placed between outputs and lock time)
    // because it is only used for computing the `wtxid`
    // while the legacy `txid` is what is used for computing the merkle root
    pub fn coinbase_tx_suffix(&self) -> Result<B064K<'static>, Error> {
        let serialized_input = self.serialized_input();
        let input_sequence: &[u8] = &serialized_input[serialized_input.len() - 4..];

        let serialized_outputs = self.serialized_outputs();
        let lock_time_u32: u32 = self.tx.lock_time.to_consensus_u32();

        let mut coinbase_tx_suffix = Vec::new();
        coinbase_tx_suffix.extend_from_slice(input_sequence);
        coinbase_tx_suffix.push(serialized_outputs.len() as u8);
        coinbase_tx_suffix.extend_from_slice(&serialized_outputs.concat());
        coinbase_tx_suffix.extend_from_slice(&lock_time_u32.to_le_bytes());
        coinbase_tx_suffix.try_into().map_err(Error::BinarySv2Error)
    }
}

/// Computes the Merkle root from coinbase transaction components and a path of transaction hashes.
///
/// Validates and deserializes a coinbase transaction before building the 32-byte Merkle root.
/// Returns [`None`] if the arguments are invalid.
///
/// ## Components
/// * `coinbase_tx_prefix`: First part of the coinbase transaction (the part before the extranonce).
///   Should be converted from [`binary_sv2::B064K`].
/// * `coinbase_tx_suffix`: Coinbase transaction suffix (the part after the extranonce). Should be
///   converted from [`binary_sv2::B064K`].
/// * `extranonce`: Extra nonce space. Should be converted from [`binary_sv2::B032`] and padded with
///   zeros if not `32` bytes long.
/// * `path`: List of transaction hashes. Should be converted from [`binary_sv2::U256`].
pub fn merkle_root_from_path<T: AsRef<[u8]>>(
    coinbase_tx_prefix: &[u8],
    coinbase_tx_suffix: &[u8],
    extranonce: &[u8],
    path: &[T],
) -> Option<Vec<u8>> {
    debug!("Computing merkle root with:");
    debug!("- prefix len: {}", coinbase_tx_prefix.len());
    debug!("- prefix: {:?}", coinbase_tx_prefix);
    debug!("- suffix len: {}", coinbase_tx_suffix.len());
    debug!("- suffix: {:?}", coinbase_tx_suffix);
    debug!("- extranonce len: {}", extranonce.len());
    debug!("- extranonce: {:?}", extranonce);
    debug!("- path len: {}", path.len());

    let mut coinbase =
        Vec::with_capacity(coinbase_tx_prefix.len() + coinbase_tx_suffix.len() + extranonce.len());
    coinbase.extend_from_slice(coinbase_tx_prefix);
    coinbase.extend_from_slice(extranonce);
    coinbase.extend_from_slice(coinbase_tx_suffix);
    dbg!(&coinbase.len());

    debug!("- total coinbase len: {}", coinbase.len());
    debug!("- full coinbase: {:?}", coinbase);

    let coinbase: Transaction = match consensus::deserialize(&coinbase[..]) {
        Ok(trans) => trans,
        Err(e) => {
            error!("ERROR: {}", e);
            return None;
        }
    };

    let coinbase_id: [u8; 32] = *coinbase.compute_txid().as_ref();

    debug!("- coinbase_id: {:?}", coinbase_id);

    Some(merkle_root_from_path_(coinbase_id, path).to_vec())
}

/// Computes the Merkle root from a validated coinbase transaction and a path of transaction
/// hashes.
///
/// If the `path` is empty, the coinbase transaction hash (`coinbase_id`) is returned as the root.
///
/// ## Components
/// * `coinbase_id`: Coinbase transaction hash.
/// * `path`: List of transaction hashes. Should be converted from [`binary_sv2::U256`].
pub fn merkle_root_from_path_<T: AsRef<[u8]>>(coinbase_id: [u8; 32], path: &[T]) -> [u8; 32] {
    match path.len() {
        0 => coinbase_id,
        _ => reduce_path(coinbase_id, path),
    }
}

// Computes the Merkle root by iteratively combining the coinbase transaction hash with each
// transaction hash in the `path`.
//
// Handles the core logic of combining hashes using the Bitcoin double-SHA256 hashing algorithm.
fn reduce_path<T: AsRef<[u8]>>(coinbase_id: [u8; 32], path: &[T]) -> [u8; 32] {
    let mut root = coinbase_id;
    for node in path {
        let to_hash = [&root[..], node.as_ref()].concat();
        let hash = DHash::hash(&to_hash);
        root = *hash.as_ref();
    }
    root
}

/// Coinbase output transaction.
///
/// Typically used for parsing coinbase outputs defined in SRI role configuration files.
#[derive(Debug, Clone)]
pub struct CoinbaseOutput {
    /// Specifies type of the script used in the output.
    ///
    /// Supported values include:
    /// - `"P2PK"`: Pay-to-Public-Key
    /// - `"P2PKH"`: Pay-to-Public-Key-Hash
    /// - `"P2SH"`: Pay-to-Script-Hash
    /// - `"P2WPKH"`: Pay-to-Witness-Public-Key-Hash
    /// - `"P2WSH"`: Pay-to-Witness-Script-Hash
    /// - `"P2TR"`: Pay-to-Taproot
    pub output_script_type: String,

    /// Value associated with the script, typically a public key or script hash.
    ///
    /// This field's interpretation depends on the `output_script_type`:
    /// - For `"P2PK"`: The raw public key.
    /// - For `"P2PKH"`: A public key hash.
    /// - For `"P2WPKH"`: A witness public key hash.
    /// - For `"P2SH"`: A script hash.
    /// - For `"P2WSH"`: A witness script hash.
    /// - For `"P2TR"`: An x-only public key.
    pub output_script_value: String,
}

impl TryFrom<CoinbaseOutput> for ScriptBuf {
    type Error = Error;

    fn try_from(value: CoinbaseOutput) -> Result<Self, Self::Error> {
        match value.output_script_type.as_str() {
            "TEST" => {
                let pub_key_hash = PublicKey::from_str(&value.output_script_value)
                    .map_err(|_| Error::InvalidOutputScript)?
                    .pubkey_hash();
                Ok(ScriptBuf::new_p2pkh(&pub_key_hash))
            }
            "P2PK" => {
                let pub_key = PublicKey::from_str(&value.output_script_value)
                    .map_err(|_| Error::InvalidOutputScript)?;
                Ok(ScriptBuf::new_p2pk(&pub_key))
            }
            "P2PKH" => {
                let pub_key_hash = PublicKey::from_str(&value.output_script_value)
                    .map_err(|_| Error::InvalidOutputScript)?
                    .pubkey_hash();
                Ok(ScriptBuf::new_p2pkh(&pub_key_hash))
            }
            "P2WPKH" => {
                let w_pub_key_hash = PublicKey::from_str(&value.output_script_value)
                    .map_err(|_| Error::InvalidOutputScript)?
                    .wpubkey_hash()
                    .unwrap();
                Ok(ScriptBuf::new_p2wpkh(&w_pub_key_hash))
            }
            "P2SH" => {
                let script_hashed = ScriptHash::from_str(value.output_script_value.as_ref())
                    .map_err(|_| Error::InvalidOutputScript)?;
                Ok(ScriptBuf::new_p2sh(&script_hashed))
            }
            "P2WSH" => {
                let w_script_hashed = WScriptHash::from_str(value.output_script_value.as_ref())
                    .map_err(|_| Error::InvalidOutputScript)?;
                Ok(ScriptBuf::new_p2wsh(&w_script_hashed))
            }
            "P2TR" => {
                // From the bip
                //
                // Conceptually, every Taproot output corresponds to a combination of
                // a single public key condition (the internal key),
                // and zero or more general conditions encoded in scripts organized in a tree.
                let pub_key = XOnlyPublicKey::from_str(&value.output_script_value)
                    .map_err(|_| Error::InvalidOutputScript)?;
                Ok(ScriptBuf::new_p2tr::<All>(
                    &Secp256k1::<All>::new(),
                    pub_key,
                    None,
                ))
            }
            _ => Err(Error::UnknownOutputScriptType),
        }
    }
}

/// A list of potential errors during conversion between hashrate and target
#[derive(Debug)]
pub enum InputError {
    NegativeInput,
    DivisionByZero,
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
    let target_plus_one =
        U256Primitive::from_big_endian(target_arr.as_ref()) + U256Primitive::one();
    let denominator = shares_occurrency_frequence
        .mul(target_plus_one)
        .div(U256Primitive::from(100));
    let result = numerator.div(denominator).low_u128();
    // we multiply back by 100 so that it cancels with the same factor at the denominator
    Ok(result as f64)
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

/// Generates and manages unique IDs for groups and channels.
///
/// [`GroupId`] allows combining the group and channel [`Id`]s into a single 64-bit value, enabling
/// efficient tracking and referencing of group-channel relationships.
///
/// This is specifically used for packaging multiple channels into a single group, such that
/// multiple mining or communication channels can be managed as a cohesive unit. This is
/// particularly useful in scenarios where multiple downstreams share common properties or need to
/// be treated collectively for routing or load balancing.
///
/// A group acts as a container for multiple channels. Each channel represents a distinct
/// communication pathway between a downstream (e.g. a mining device) and an upstream (e.g. a proxy
/// or pool). Channels within a group might share common configurations, such as difficulty
/// settings or work templates. Operations like broadcasting job updates or handling difficulty
/// adjustments can be efficiently applied to all channels in a group. By treating a group as a
/// single entity, the protocol reduces overhead of managing individual channels, especially in
/// large mining farms.
#[derive(Debug, Default)]
pub struct GroupId {
    group_ids: Id,
    channel_ids: Id,
}

impl GroupId {
    /// Creates a new [`GroupId`] instance.
    ///
    /// New GroupId it starts with groups 0, since 0 is reserved for hom downstream's.
    pub fn new() -> Self {
        Self {
            group_ids: Id::new(),
            channel_ids: Id::new(),
        }
    }

    /// Generates a new unique group ID.
    ///
    /// Increments the internal group ID counter and returns the next available group ID.
    pub fn new_group_id(&mut self) -> u32 {
        self.group_ids.next()
    }

    /// Generates a new unique channel ID for a given group.
    ///
    /// Increments the internal channel ID counter and returns the next available channel ID.
    ///
    /// **Note**: The `_group_id` parameter is reserved for future use to create a hierarchical
    /// structure of IDs without breaking compatibility with older versions.
    pub fn new_channel_id(&mut self, _group_id: u32) -> u32 {
        self.channel_ids.next()
    }

    /// Combines a group ID and channel ID into a single 64-bit unique ID.
    ///
    /// Concatenates the group ID and channel ID, storing the group ID in the higher 32 bits and
    /// the channel ID in the lower 32 bits. This combined identifier is useful for efficiently
    /// tracking and referencing unique group-channel pairs.
    pub fn into_complete_id(group_id: u32, channel_id: u32) -> u64 {
        let part_1 = channel_id.to_le_bytes();
        let part_2 = group_id.to_le_bytes();
        u64::from_be_bytes([
            part_2[3], part_2[2], part_2[1], part_2[0], part_1[3], part_1[2], part_1[1], part_1[0],
        ])
    }

    /// Extracts the group ID from a complete group-channel 64-bit unique ID.
    ///
    /// The group ID is the higher 32 bits.
    pub fn into_group_id(complete_id: u64) -> u32 {
        let complete = complete_id.to_le_bytes();
        u32::from_le_bytes([complete[4], complete[5], complete[6], complete[7]])
    }

    /// Extracts the channel ID from a complete group-channel 64-bit unique ID.
    ///
    /// The channel ID is the lower 32 bits.
    pub fn into_channel_id(complete_id: u64) -> u32 {
        let complete = complete_id.to_le_bytes();
        u32::from_le_bytes([complete[0], complete[1], complete[2], complete[3]])
    }
}

#[test]
fn test_group_id_new_group_id() {
    let mut group_ids = GroupId::new();
    let _ = group_ids.new_group_id();
    let id = group_ids.new_group_id();
    assert!(id == 2);
}
#[test]
fn test_group_id_new_channel_id() {
    let mut group_ids = GroupId::new();
    let _ = group_ids.new_group_id();
    let id = group_ids.new_group_id();
    let channel_id = group_ids.new_channel_id(id);
    assert!(channel_id == 1);
}
#[test]
fn test_group_id_new_into_complete_id() {
    let group_id = u32::from_le_bytes([0, 1, 2, 3]);
    let channel_id = u32::from_le_bytes([10, 11, 12, 13]);
    let complete_id = GroupId::into_complete_id(group_id, channel_id);
    assert!([10, 11, 12, 13, 0, 1, 2, 3] == complete_id.to_le_bytes());
}

#[test]
fn test_group_id_new_into_group_id() {
    let group_id = u32::from_le_bytes([0, 1, 2, 3]);
    let channel_id = u32::from_le_bytes([10, 11, 12, 13]);
    let complete_id = GroupId::into_complete_id(group_id, channel_id);
    let channel_from_complete = GroupId::into_channel_id(complete_id);
    assert!(channel_id == channel_from_complete);
}

#[test]
fn test_merkle_root_independent_vector() {
    const REFERENCE_MERKLE_ROOT: [u8; 32] = [
        28, 204, 213, 73, 250, 160, 146, 15, 5, 127, 9, 214, 204, 20, 164, 199, 20, 181, 26, 190,
        236, 91, 40, 225, 128, 239, 213, 148, 232, 77, 4, 36,
    ];
    const BRANCH: &[[u8; 32]] = &[
        [
            224, 195, 140, 86, 17, 172, 9, 61, 54, 73, 215, 202, 109, 83, 124, 163, 215, 78, 143,
            204, 44, 242, 242, 122, 37, 106, 55, 81, 58, 234, 27, 210,
        ],
        [
            35, 10, 232, 246, 235, 117, 56, 190, 87, 77, 81, 11, 159, 79, 90, 62, 91, 52, 41, 49,
            57, 245, 219, 122, 115, 223, 199, 229, 238, 60, 47, 144,
        ],
        [
            95, 18, 132, 87, 213, 76, 188, 74, 245, 106, 18, 149, 106, 32, 209, 158, 239, 3, 17,
            26, 207, 230, 118, 149, 120, 48, 96, 66, 214, 150, 137, 220,
        ],
        [
            205, 167, 106, 179, 82, 50, 157, 76, 91, 36, 54, 226, 34, 183, 162, 179, 109, 64, 185,
            207, 103, 192, 63, 31, 141, 126, 34, 30, 68, 69, 154, 176,
        ],
        [
            251, 236, 76, 1, 218, 98, 98, 236, 144, 52, 151, 246, 95, 13, 109, 240, 240, 195, 64,
            157, 7, 142, 28, 242, 29, 123, 51, 93, 51, 36, 143, 148,
        ],
        [
            35, 146, 105, 130, 188, 39, 97, 252, 75, 229, 185, 148, 242, 106, 164, 112, 123, 66,
            34, 95, 218, 203, 50, 203, 129, 208, 109, 220, 112, 228, 121, 160,
        ],
        [
            44, 55, 125, 47, 249, 213, 175, 143, 140, 50, 219, 72, 111, 71, 125, 54, 85, 70, 4, 85,
            60, 92, 208, 35, 113, 245, 128, 139, 228, 4, 230, 177,
        ],
        [
            169, 119, 48, 178, 205, 188, 19, 220, 85, 29, 174, 45, 158, 172, 222, 238, 170, 144,
            79, 140, 56, 90, 105, 187, 204, 145, 241, 96, 75, 88, 6, 133,
        ],
        [
            72, 202, 11, 90, 167, 140, 253, 12, 58, 85, 223, 17, 82, 112, 24, 129, 186, 39, 224,
            171, 227, 192, 14, 167, 154, 248, 150, 55, 114, 169, 43, 17,
        ],
    ];
    const CB_PREFIX: &[u8] = &[
        1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 75, 3, 139, 133, 11, 250, 190, 109, 109, 43, 220,
        215, 96, 154, 211, 18, 14, 53, 53, 0, 95, 132, 159, 127, 54, 197, 70, 135, 74, 17, 149, 12,
        104, 133, 16, 182, 152, 109, 207, 13, 9, 1, 0, 0, 0, 0, 0, 0, 0,
    ];
    const CB_SUFFIX: &[u8] = &[
        89, 236, 29, 54, 20, 47, 115, 108, 117, 115, 104, 47, 0, 0, 0, 0, 3, 236, 42, 86, 37, 0, 0,
        0, 0, 25, 118, 169, 20, 124, 21, 78, 209, 220, 89, 96, 158, 61, 38, 171, 178, 223, 46, 163,
        213, 135, 205, 140, 65, 136, 172, 0, 0, 0, 0, 0, 0, 0, 0, 44, 106, 76, 41, 82, 83, 75, 66,
        76, 79, 67, 75, 58, 155, 83, 3, 23, 69, 4, 30, 18, 212, 34, 33, 76, 167, 101, 132, 91, 1,
        127, 124, 85, 238, 57, 118, 135, 107, 35, 25, 33, 0, 71, 6, 88, 0, 0, 0, 0, 0, 0, 0, 0, 38,
        106, 36, 170, 33, 169, 237, 123, 170, 130, 253, 191, 130, 150, 16, 0, 18, 157, 2, 231, 33,
        177, 230, 137, 182, 134, 51, 32, 216, 181, 6, 73, 60, 103, 211, 194, 61, 77, 64, 0, 0, 0,
        0,
    ];
    const EXTRANONCE_PREFIX: &[u8] = &[41, 101, 8, 3, 39, 21, 251];
    const EXTRANONCE: &[u8] = &[165, 6, 238, 7, 139, 252, 22, 7];

    let full_extranonce = {
        let mut xn = EXTRANONCE_PREFIX.to_vec();
        xn.extend_from_slice(EXTRANONCE);
        xn
    };

    let calculated_merkle_root =
        merkle_root_from_path(CB_PREFIX, CB_SUFFIX, &full_extranonce, BRANCH)
            .expect("Ultimate failure. Merkle root calculator returned None");
    assert_eq!(
        calculated_merkle_root, REFERENCE_MERKLE_ROOT,
        "Merkle root does not match reference"
    )
}

#[test]
fn test_merkle_root_from_path() {
    let coinbase_bytes = vec![
        1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 75, 3, 63, 146, 11, 250, 190, 109, 109, 86, 6,
        110, 64, 228, 218, 247, 203, 127, 75, 141, 53, 51, 197, 180, 38, 117, 115, 221, 103, 2, 11,
        85, 213, 65, 221, 74, 90, 97, 128, 91, 182, 1, 0, 0, 0, 0, 0, 0, 0, 49, 101, 7, 7, 139,
        168, 76, 0, 1, 0, 0, 0, 0, 0, 0, 70, 84, 183, 110, 24, 47, 115, 108, 117, 115, 104, 47, 0,
        0, 0, 0, 3, 120, 55, 179, 37, 0, 0, 0, 0, 25, 118, 169, 20, 124, 21, 78, 209, 220, 89, 96,
        158, 61, 38, 171, 178, 223, 46, 163, 213, 135, 205, 140, 65, 136, 172, 0, 0, 0, 0, 0, 0, 0,
        0, 44, 106, 76, 41, 82, 83, 75, 66, 76, 79, 67, 75, 58, 216, 82, 49, 182, 148, 133, 228,
        178, 20, 248, 55, 219, 145, 83, 227, 86, 32, 97, 240, 182, 3, 175, 116, 196, 69, 114, 83,
        46, 0, 71, 230, 205, 0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 179, 75, 32,
        206, 223, 111, 113, 150, 112, 248, 21, 36, 163, 123, 107, 168, 153, 76, 233, 86, 77, 218,
        162, 59, 48, 26, 180, 38, 62, 34, 3, 185, 0, 0, 0, 0,
    ];
    let a = [
        122, 97, 64, 124, 164, 158, 164, 14, 87, 119, 226, 169, 34, 196, 251, 51, 31, 131, 109,
        250, 13, 54, 94, 6, 177, 27, 156, 154, 101, 30, 123, 159,
    ];
    let b = [
        180, 113, 121, 253, 215, 85, 129, 38, 108, 2, 86, 66, 46, 12, 131, 139, 130, 87, 29, 92,
        59, 164, 247, 114, 251, 140, 129, 88, 127, 196, 125, 116,
    ];
    let c = [
        171, 77, 225, 148, 80, 32, 41, 157, 246, 77, 161, 49, 87, 139, 214, 236, 149, 164, 192,
        128, 195, 9, 5, 168, 131, 27, 250, 9, 60, 179, 206, 94,
    ];
    let d = [
        6, 187, 202, 75, 155, 220, 255, 166, 199, 35, 182, 220, 20, 96, 123, 41, 109, 40, 186, 142,
        13, 139, 230, 164, 116, 177, 217, 23, 16, 123, 135, 202,
    ];
    let e = [
        109, 45, 171, 89, 223, 39, 132, 14, 150, 128, 241, 113, 136, 227, 105, 123, 224, 48, 66,
        240, 189, 186, 222, 49, 173, 143, 80, 90, 110, 219, 192, 235,
    ];
    let f = [
        196, 7, 21, 180, 228, 161, 182, 132, 28, 153, 242, 12, 210, 127, 157, 86, 62, 123, 181, 33,
        84, 3, 105, 129, 148, 162, 5, 152, 64, 7, 196, 156,
    ];
    let g = [
        22, 16, 18, 180, 109, 237, 68, 167, 197, 10, 195, 134, 11, 119, 219, 184, 49, 140, 239, 45,
        27, 210, 212, 120, 186, 60, 155, 105, 106, 219, 218, 32,
    ];
    let h = [
        83, 228, 21, 241, 42, 240, 8, 254, 109, 156, 59, 171, 167, 46, 183, 60, 27, 63, 241, 211,
        235, 179, 147, 99, 46, 3, 22, 166, 159, 169, 183, 159,
    ];
    let i = [
        230, 81, 3, 190, 66, 73, 200, 55, 94, 135, 209, 50, 92, 193, 114, 202, 141, 170, 124, 142,
        206, 29, 88, 9, 22, 110, 203, 145, 238, 66, 166, 35,
    ];
    let l = [
        43, 106, 86, 239, 237, 74, 208, 202, 247, 133, 88, 42, 15, 77, 163, 186, 85, 26, 89, 151,
        5, 19, 30, 122, 108, 220, 215, 104, 152, 226, 113, 55,
    ];
    let m = [
        148, 76, 200, 221, 206, 54, 56, 45, 252, 60, 123, 202, 195, 73, 144, 65, 168, 184, 59, 130,
        145, 229, 250, 44, 213, 70, 175, 128, 34, 31, 102, 80,
    ];
    let n = [
        203, 112, 102, 31, 49, 147, 24, 25, 245, 61, 179, 146, 205, 127, 126, 100, 78, 204, 228,
        146, 209, 154, 89, 194, 209, 81, 57, 167, 88, 251, 44, 76,
    ];
    let mut path = vec![a, b, c, d, e, f, g, h, i, l, m, n];
    let expected_root = vec![
        73, 100, 41, 247, 106, 44, 1, 242, 3, 64, 100, 1, 98, 155, 40, 91, 170, 255, 170, 29, 193,
        255, 244, 71, 236, 29, 134, 218, 94, 45, 78, 77,
    ];
    let root = merkle_root_from_path(
        &coinbase_bytes[..20],
        &coinbase_bytes[30..],
        &coinbase_bytes[20..30],
        &path,
    )
    .unwrap();
    assert_eq!(expected_root, root);

    //Target coinbase_id return path
    path.clear();
    let coinbase_id = vec![
        10, 66, 217, 241, 152, 86, 5, 234, 225, 85, 251, 215, 105, 1, 21, 126, 222, 69, 40, 157,
        23, 177, 157, 106, 234, 164, 243, 206, 23, 241, 250, 166,
    ];

    let root = merkle_root_from_path(
        &coinbase_bytes[..20],
        &coinbase_bytes[30..],
        &coinbase_bytes[20..30],
        &path,
    )
    .unwrap();
    assert_eq!(coinbase_id, root);

    //Target None return path on serialization
    assert_eq!(
        merkle_root_from_path(&coinbase_bytes, &coinbase_bytes, &coinbase_bytes, &path),
        None
    );
}

/// Converts a `u256` to a [`BlockHash`] type.
pub fn u256_to_block_hash(v: U256<'static>) -> BlockHash {
    let hash: [u8; 32] = v.to_vec().try_into().unwrap();
    let hash = Hash::from_slice(&hash).unwrap();
    BlockHash::from_raw_hash(hash)
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

/// Generates a list of transaction short hashes and a hash of the full transaction list.
///
/// This function computes a tuple containing:
/// 1. A list of short transaction hashes, calculated using SipHash 24 ([`SipHasher24`]).
/// 2. A combined hash of the full list of transaction IDs.
///
/// Typically used when a compact representation of transaction IDs is needed or when a combined
/// hash of the full transaction list is required for validation or lookup, like when the Job
/// Declarator client declares a new mining job.
pub fn hash_lists_tuple(
    tx_data: Vec<Transaction>,
    tx_short_hash_nonce: u64,
) -> (Seq064K<'static, ShortTxId<'static>>, U256<'static>) {
    let mut txid_list: Vec<bitcoin::Txid> = Vec::new();
    for tx in tx_data {
        txid_list.push(tx.compute_txid());
    }
    let mut tx_short_hash_list_: Vec<ShortTxId> = Vec::new();
    for txid in txid_list.clone() {
        tx_short_hash_list_.push(get_short_hash(txid, tx_short_hash_nonce));
    }
    let tx_short_hash_list: Seq064K<'static, ShortTxId> = Seq064K::from(tx_short_hash_list_);
    let tx_hash_list_hash = tx_hash_list_hash_builder(txid_list);
    (tx_short_hash_list, tx_hash_list_hash)
}

/// Computes SipHash 24 of some transaction id (short hash)
///
/// Computes a short transaction hash using SipHash 24.
///
/// This function uses [`SipHasher24`] to compute a compact, deterministic hash of a transaction ID
/// ([`bitcoin::Txid`]), leveraging a nonce for uniqueness.
///
/// Typically used to generate short hashes for efficient transaction identification and comparison
/// in contexts where full transaction IDs are unnecessary.
pub fn get_short_hash(txid: bitcoin::Txid, tx_short_hash_nonce: u64) -> ShortTxId<'static> {
    // hash the short hash nonce
    let nonce_hash = sha256::Hash::hash(&tx_short_hash_nonce.to_le_bytes());
    // take first two integers from the hash
    let k0 = u64::from_le_bytes(nonce_hash[0..8].try_into().unwrap());
    let k1 = u64::from_le_bytes(nonce_hash[8..16].try_into().unwrap());
    // get every transaction, hash it, remove first two bytes and push the ShortTxId in a vector
    let hasher = SipHasher24::new_with_keys(k0, k1);
    let tx_hashed = hasher.hash(txid.as_ref());
    let tx_hashed_bytes: Vec<u8> = tx_hashed.to_le_bytes()[2..].to_vec();
    let short_tx_id: ShortTxId = tx_hashed_bytes.try_into().unwrap();
    short_tx_id
}

/// Computes a combined hash of a list of transaction IDs.
///
/// Concatenates all transaction IDs ([`bitcoin::Txid`]) into a single byte array and computes a
/// SHA256 hash of the resulting data.
fn tx_hash_list_hash_builder(txid_list: Vec<bitcoin::Txid>) -> U256<'static> {
    // TODO: understand if this field is redunant and to be deleted since
    // the full coinbase is known
    let mut vec_u8 = vec![];
    for txid in txid_list {
        let txid_as_byte_array: &[u8; 32] = txid.as_ref();
        vec_u8.extend_from_slice(txid_as_byte_array);
    }
    let hash: sha256::Hash = sha256::Hash::hash(&vec_u8);
    let hash_arr: [u8; 32] = *hash.as_ref();
    U256::from(hash_arr)
}

/// Creates a block from a solution submission.
///
/// Facilitates the creation of valid Bitcoin blocks by combining a declared mining job, a list of
/// transactions, and a solution message from the mining device. It encapsulates the necessary data
/// (the coinbase, a list of transactions, and a miner-provided solution) to assemble a complete
/// and valid block that can be submitted to the Bitcoin network.
///
/// It is used in the Job Declarator server to handle the final step in processing the mining job
/// solutions.
pub struct BlockCreator<'a> {
    last_declare: DeclareMiningJob<'a>,
    tx_list: Vec<bitcoin::Transaction>,
    message: SubmitSolutionJd<'a>,
}

impl<'a> BlockCreator<'a> {
    /// Creates a new [`BlockCreator`] instance.
    pub fn new(
        last_declare: DeclareMiningJob<'a>,
        tx_list: Vec<bitcoin::Transaction>,
        message: SubmitSolutionJd<'a>,
    ) -> BlockCreator<'a> {
        BlockCreator {
            last_declare,
            tx_list,
            message,
        }
    }
}

// TODO write a test for this function that takes an already mined block, and test if the new
// block created with the hash of the new block created with the block creator coincides with the
// hash of the mined block
impl<'a> From<BlockCreator<'a>> for bitcoin::Block {
    fn from(block_creator: BlockCreator<'a>) -> bitcoin::Block {
        let last_declare = block_creator.last_declare;
        let mut tx_list = block_creator.tx_list;
        let message = block_creator.message;

        let coinbase_pre = last_declare.coinbase_prefix.to_vec();
        let extranonce = message.extranonce.to_vec();
        let coinbase_suf = last_declare.coinbase_suffix.to_vec();
        let mut path: Vec<Vec<u8>> = vec![];
        for tx in &tx_list {
            let id = tx.compute_txid();
            let id_bytes: &[u8; 32] = id.as_ref();
            path.push(id_bytes.to_vec());
        }
        let merkle_root =
            merkle_root_from_path(&coinbase_pre[..], &coinbase_suf[..], &extranonce[..], &path)
                .expect("Invalid coinbase");
        let merkle_root = Hash::from_slice(merkle_root.as_slice()).unwrap();

        let prev_blockhash = u256_to_block_hash(message.prev_hash.into_static());
        let header = Header {
            version: Version::from_consensus(message.version as i32),
            prev_blockhash,
            merkle_root,
            time: message.ntime,
            bits: CompactTarget::from_consensus(message.nbits),
            nonce: message.nonce,
        };

        let coinbase = [coinbase_pre, extranonce, coinbase_suf].concat();
        let coinbase = consensus::deserialize(&coinbase[..]).unwrap();
        tx_list.insert(0, coinbase);

        let mut block = Block {
            header,
            txdata: tx_list.clone(),
        };

        block.header.merkle_root = block.compute_merkle_root().unwrap();
        block
    }
}

#[cfg(test)]
mod tests {

    use super::{hash_rate_from_target, hash_rate_to_target, *};
    use binary_sv2::{Seq0255, B064K, U256};
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
        let block_hash: U256 = block_hash_vec
            .try_into()
            .expect("Could not convert `[u8; 32]` to `U256`");

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
            let p_u256: U256 = (p_arr)
                .try_into()
                .expect("Could not convert to `U256` from `[u8; 32]`");
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
    fn gets_merkle_root_from_path() {
        let block = get_test_block();
        let expect: Vec<u8> = block.merkle_root;

        let actual = merkle_root_from_path(
            block.coinbase_tx_prefix.inner_as_ref(),
            &block.coinbase_tx_suffix.inner_as_ref(),
            &block.coinbase_script,
            &block.path.inner_as_ref(),
        )
        .unwrap();

        assert_eq!(expect, actual);
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
            average = average + (*i as f64) / attempts as f64;
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
    fn test_super_safe_lock() {
        let m = super::Mutex::new(1u32);
        m.safe_lock(|i| *i += 1).unwrap();
        // m.super_safe_lock(|i| *i = (*i).checked_add(1).unwrap()); // will not compile
        m.super_safe_lock(|i| *i = (*i).checked_add(1).unwrap_or_default()); // compiles
    }
}
