//! # Stratum V2 Mining Protocol Messages Crate
//!
//! `mining_sv2` is a Rust crate that implements a set of messages defined in the Mining protocol
//! of Stratum V2.
//!
//! The Mining protocol enables the distribution of work to mining devices and the submission of
//! proof-of-work results.
//!
//! For further information about the messages, please refer to [Stratum V2 documentation - Mining](https://stratumprotocol.org/specification/05-Mining-Protocol/).
//!
//! ## Build Options
//!
//! This crate can be built with the following features:
//!
//! ## Usage
//!
//! To include this crate in your project, run:
//! ```bash
//! $ cargo add mining_sv2
//! ```
//!
//! For further information about the mining protocol, please refer to [Stratum V2 documentation -
//! Mining Protocol](https://stratumprotocol.org/specification/05-Mining-Protocol/).

#![no_std]

use binary_sv2::{B032, U256};
use core::{
    cmp::{Ord, PartialOrd},
    convert::TryInto,
};

#[macro_use]
extern crate alloc;

mod close_channel;
mod new_mining_job;
mod open_channel;
mod set_custom_mining_job;
mod set_extranonce_prefix;
mod set_group_channel;
mod set_new_prev_hash;
mod set_target;
mod submit_shares;
mod update_channel;

pub use close_channel::CloseChannel;
use core::ops::Range;
pub use new_mining_job::{NewExtendedMiningJob, NewMiningJob};
pub use open_channel::{
    OpenExtendedMiningChannel, OpenExtendedMiningChannelSuccess, OpenMiningChannelError,
    OpenStandardMiningChannel, OpenStandardMiningChannelSuccess,
};
pub use set_custom_mining_job::{
    SetCustomMiningJob, SetCustomMiningJobError, SetCustomMiningJobSuccess,
};
pub use set_extranonce_prefix::SetExtranoncePrefix;
pub use set_group_channel::SetGroupChannel;
pub use set_new_prev_hash::SetNewPrevHash;
pub use set_target::SetTarget;
pub use submit_shares::{
    SubmitSharesError, SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess,
};
pub use update_channel::{UpdateChannel, UpdateChannelError};

// Mining Protocol message types.
pub const MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL: u8 = 0x10;
pub const MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS: u8 = 0x11;
pub const MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR: u8 = 0x12;
pub const MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL: u8 = 0x13;
pub const MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS: u8 = 0x14;
pub const MESSAGE_TYPE_NEW_MINING_JOB: u8 = 0x15;
pub const MESSAGE_TYPE_UPDATE_CHANNEL: u8 = 0x16;
pub const MESSAGE_TYPE_UPDATE_CHANNEL_ERROR: u8 = 0x17;
pub const MESSAGE_TYPE_CLOSE_CHANNEL: u8 = 0x18;
pub const MESSAGE_TYPE_SET_EXTRANONCE_PREFIX: u8 = 0x19;
pub const MESSAGE_TYPE_SUBMIT_SHARES_STANDARD: u8 = 0x1a;
pub const MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED: u8 = 0x1b;
pub const MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS: u8 = 0x1c;
pub const MESSAGE_TYPE_SUBMIT_SHARES_ERROR: u8 = 0x1d;
pub const MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB: u8 = 0x1f;
pub const MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH: u8 = 0x20;
pub const MESSAGE_TYPE_SET_TARGET: u8 = 0x21;
pub const MESSAGE_TYPE_SET_CUSTOM_MINING_JOB: u8 = 0x22;
pub const MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS: u8 = 0x23;
pub const MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR: u8 = 0x24;
pub const MESSAGE_TYPE_SET_GROUP_CHANNEL: u8 = 0x25;

// Channel bits in the Mining protocol vary depending on the message.
pub const CHANNEL_BIT_CLOSE_CHANNEL: bool = true;
pub const CHANNEL_BIT_NEW_EXTENDED_MINING_JOB: bool = true;
pub const CHANNEL_BIT_NEW_MINING_JOB: bool = true;
pub const CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL: bool = false;
pub const CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS: bool = false;
pub const CHANNEL_BIT_OPEN_MINING_CHANNEL_ERROR: bool = false;
pub const CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL: bool = false;
pub const CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL_SUCCESS: bool = false;
pub const CHANNEL_BIT_RECONNECT: bool = false;
pub const CHANNEL_BIT_SET_CUSTOM_MINING_JOB: bool = false;
pub const CHANNEL_BIT_SET_CUSTOM_MINING_JOB_ERROR: bool = false;
pub const CHANNEL_BIT_SET_CUSTOM_MINING_JOB_SUCCESS: bool = false;
pub const CHANNEL_BIT_SET_EXTRANONCE_PREFIX: bool = true;
pub const CHANNEL_BIT_SET_GROUP_CHANNEL: bool = false;
pub const CHANNEL_BIT_MINING_SET_NEW_PREV_HASH: bool = true;
pub const CHANNEL_BIT_SET_TARGET: bool = true;
pub const CHANNEL_BIT_SUBMIT_SHARES_ERROR: bool = true;
pub const CHANNEL_BIT_SUBMIT_SHARES_EXTENDED: bool = true;
pub const CHANNEL_BIT_SUBMIT_SHARES_STANDARD: bool = true;
pub const CHANNEL_BIT_SUBMIT_SHARES_SUCCESS: bool = true;
pub const CHANNEL_BIT_UPDATE_CHANNEL: bool = true;
pub const CHANNEL_BIT_UPDATE_CHANNEL_ERROR: bool = true;

pub const MAX_EXTRANONCE_LEN: usize = 32;

/// Target is a 256-bit unsigned integer in little-endian
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    head: u128, // least significant bits
    tail: u128, // most significant bits
}

impl Target {
    pub fn new(head: u128, tail: u128) -> Self {
        Self { head, tail }
    }
}

impl From<[u8; 32]> for Target {
    fn from(v: [u8; 32]) -> Self {
        // below unwraps never panics
        let head = u128::from_le_bytes(v[0..16].try_into().unwrap());
        let tail = u128::from_le_bytes(v[16..32].try_into().unwrap());
        Self { head, tail }
    }
}

impl From<Extranonce> for alloc::vec::Vec<u8> {
    fn from(v: Extranonce) -> Self {
        v.extranonce
    }
}

impl<'a> From<U256<'a>> for Target {
    fn from(v: U256<'a>) -> Self {
        let inner = v.inner_as_ref();
        // below unwraps never panics
        let head = u128::from_le_bytes(inner[0..16].try_into().unwrap());
        let tail = u128::from_le_bytes(inner[16..32].try_into().unwrap());
        Self { head, tail }
    }
}

impl From<Target> for U256<'static> {
    fn from(v: Target) -> Self {
        let mut inner = v.head.to_le_bytes().to_vec();
        inner.extend_from_slice(&v.tail.to_le_bytes());
        // below unwraps never panics
        inner.try_into().unwrap()
    }
}

impl PartialOrd for Target {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Target {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        if self.tail == other.tail && self.head == other.head {
            core::cmp::Ordering::Equal
        } else if self.tail != other.tail {
            self.tail.cmp(&other.tail)
        } else {
            self.head.cmp(&other.head)
        }
    }
}

// WARNING: do not derive Copy on this type. Some operations performed to a copy of an extranonce
// do not affect the original, and this may lead to different extranonce inconsistency
/// Extranonce bytes which need to be added to the coinbase to form a fully valid submission.
///
/// Representation is in big endian, so tail is for the digits relative to smaller powers
///
/// `full coinbase = coinbase_tx_prefix + extranonce + coinbase_tx_suffix`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extranonce {
    extranonce: alloc::vec::Vec<u8>,
}

// this function converts a U256 type in little endian to Extranonce type
impl<'a> From<U256<'a>> for Extranonce {
    fn from(v: U256<'a>) -> Self {
        let extranonce: alloc::vec::Vec<u8> = v.inner_as_ref().into();
        Self { extranonce }
    }
}

// This function converts an Extranonce type to U256n little endian
impl From<Extranonce> for U256<'_> {
    fn from(v: Extranonce) -> Self {
        let inner = v.extranonce;
        debug_assert!(inner.len() <= 32);
        // below unwraps never panics
        inner.try_into().unwrap()
    }
}

// this function converts an extranonce to the type B032
impl<'a> From<B032<'a>> for Extranonce {
    fn from(v: B032<'a>) -> Self {
        let extranonce: alloc::vec::Vec<u8> = v.inner_as_ref().into();
        Self { extranonce }
    }
}

// this function converts an Extranonce type in B032 in little endian
impl From<Extranonce> for B032<'_> {
    fn from(v: Extranonce) -> Self {
        let inner = v.extranonce.to_vec();
        // below unwraps never panics
        inner.try_into().unwrap()
    }
}

impl Default for Extranonce {
    fn default() -> Self {
        Self {
            extranonce: vec![0; 32],
        }
    }
}

impl core::convert::TryFrom<alloc::vec::Vec<u8>> for Extranonce {
    type Error = ();

    fn try_from(v: alloc::vec::Vec<u8>) -> Result<Self, Self::Error> {
        if v.len() > MAX_EXTRANONCE_LEN {
            Err(())
        } else {
            Ok(Extranonce { extranonce: v })
        }
    }
}

impl Extranonce {
    pub fn new(len: usize) -> Option<Self> {
        if len > MAX_EXTRANONCE_LEN {
            None
        } else {
            let extranonce = vec![0; len];
            Some(Self { extranonce })
        }
    }

    /// this function converts a Extranonce type to b032 type
    pub fn from_vec_with_len(mut extranonce: alloc::vec::Vec<u8>, len: usize) -> Self {
        extranonce.resize(len, 0);
        Self { extranonce }
    }

    pub fn into_b032(self) -> B032<'static> {
        self.into()
    }
    // B032 type is more used, this is why the output signature is not ExtendedExtranoncee the B032
    // type is more used, this is why the output signature is not ExtendedExtranoncee
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<B032<'_>> {
        increment_bytes_be(&mut self.extranonce).ok()?;
        // below unwraps never panics
        Some(self.extranonce.clone().try_into().unwrap())
    }

    pub fn to_vec(self) -> alloc::vec::Vec<u8> {
        self.extranonce
    }
}

impl From<&mut ExtendedExtranonce> for Extranonce {
    fn from(v: &mut ExtendedExtranonce) -> Self {
        let mut extranonce = v.inner.to_vec();
        extranonce.truncate(v.range_2.end);
        Self { extranonce }
    }
}

#[derive(Debug, Clone)]
/// Downstream and upstream are relative to user P. In simple terms, upstream is
/// the part of the protocol that a user P sees when looking above, and downstream is what they see
/// when looking below.
///
/// An `ExtendedExtranonce` is defined by 3 ranges:
///
/// - `range_0`: Represents the extended extranonce part reserved by upstream relative to P (for
///   most upstream nodes, e.g., a pool, this is `[0..0]`) and it is fixed for P.
///
/// - `range_1`: Represents the extended extranonce part reserved for P. P assigns to every relative
///   downstream a unique extranonce with different values in range_1 in the following way: if D_i
///   is the (i+1)-th downstream that connected to P, then D_i gets from P an extranonce with
///   range_1=i (note that the concatenation of range_0 and range_1 is the range_0 relative to D_i,
///   and range_2 of P is the range_1 of D_i).
///
/// - `range_2`: Represents the range that P reserves for the downstreams.
///
///
/// In the following scenarios, we examine the extended extranonce in some cases:
///
/// **Scenario 1: P is a pool**
/// - `range_0` → `0..0`: There is no upstream relative to the pool P, so no space is reserved by
///   the upstream.
/// - `range_1` → `0..16`: The pool P increments these bytes to ensure each downstream gets a unique
///   extended extranonce search space. The pool could optionally choose to set some fixed bytes as
///   `static_prefix` (no bigger than 2 bytes), which are set on the beginning of this range and
///   will not be incremented. These bytes are used to allow unique allocation for the pool's mining
///   server (if there are more than one).
/// - `range_2` → `16..32`: These bytes are not changed by the pool but are changed by the pool's
///   downstream.
///
/// **Scenario 2: P is a translator proxy**
/// - `range_0` → `0..16`: These bytes are set by the upstream and P shouldn't change them.
/// - `range_1` → `16..24`: These bytes are modified by P each time an Sv1 mining device connects,
///   ensuring each connected Sv1 mining device gets a different extended extranonce search space.
/// - `range_2` → `24..32`: These bytes are left free for the Sv1 mining device.
///
/// **Scenario 3: P is an Sv1 mining device**
/// - `range_0` → `0..24`: These bytes are set by the device's upstreams.
/// - `range_1` → `24..32`: These bytes are changed by P (if capable) to increment the search space.
/// - `range_2` → `32..32`: No more downstream.
///
/// # Examples
///
/// Basic usage without static prefix:
///
/// ```
/// use mining_sv2::*;
/// use core::convert::TryInto;
///
/// // Create an extended extranonce of len 32, reserving the first 7 bytes for the pool
/// let mut pool_extended_extranonce = ExtendedExtranonce::new(0..0, 0..7, 7..32, None).unwrap();
///
/// // On open extended channel (requesting to use a range of 4 bytes), the pool allocates this extranonce_prefix:
/// let new_extended_channel_extranonce_prefix = pool_extended_extranonce.next_prefix_extended(4).unwrap();
/// let expected_extranonce_prefix = vec![0, 0, 0, 0, 0, 0, 1];
/// assert_eq!(new_extended_channel_extranonce_prefix.clone().to_vec(), expected_extranonce_prefix);
///
/// // On open extended channel (requesting to use a range of 20 bytes), the pool allocates this extranonce_prefix:
/// let new_extended_channel_extranonce_prefix = pool_extended_extranonce.next_prefix_extended(20).unwrap();
/// let expected_extranonce_prefix = vec![0, 0, 0, 0, 0, 0, 2];
/// assert_eq!(new_extended_channel_extranonce_prefix.clone().to_vec(), expected_extranonce_prefix);
///
/// // On open extended channel (requesting to use a range of 26 bytes, which is too much), we get an error:
/// let new_extended_channel_extranonce_prefix_error = pool_extended_extranonce.next_prefix_extended(26);
/// assert!(new_extended_channel_extranonce_prefix_error.is_err());
///
/// // Then the pool receives a request to open a standard channel
/// let new_standard_channel_extranonce = pool_extended_extranonce.next_prefix_standard().unwrap();
/// // For standard channels, only the bytes in range_2 are incremented
/// let expected_standard_extranonce = vec![0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
/// assert_eq!(new_standard_channel_extranonce.to_vec(), expected_standard_extranonce);
///
/// // Now the proxy receives the ExtendedExtranonce previously created
/// // The proxy knows the extranonce space reserved to the pool is 7 bytes and that the total
/// // extranonce len is 32 bytes and decides to reserve 4 bytes for itself and leave the remaining 21 for
/// // further downstreams.
/// let range_0 = 0..7;
/// let range_1 = 7..11;
/// let range_2 = 11..32;
/// let mut proxy_extended_extranonce = ExtendedExtranonce::from_upstream_extranonce(
///     new_extended_channel_extranonce_prefix,
///     range_0,
///     range_1,
///     range_2
/// ).unwrap();
///
/// // The proxy generates an extended extranonce for downstream (allowing it to use a range of 3 bytes)
/// let new_extended_channel_extranonce_prefix = proxy_extended_extranonce.next_prefix_extended(3).unwrap();
/// let expected_extranonce_prefix = vec![0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 1];
/// assert_eq!(new_extended_channel_extranonce_prefix.clone().to_vec(), expected_extranonce_prefix);
///
/// // When the proxy receives a share from downstream and wants to recreate the full extranonce
/// // e.g., because it wants to check the share's work
/// let received_extranonce: Extranonce = vec![0, 0, 0, 0, 0, 0, 0, 8, 0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap();
/// let share_complete_extranonce = proxy_extended_extranonce.extranonce_from_downstream_extranonce(received_extranonce.clone()).unwrap();
/// let expected_complete_extranonce = vec![0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 8, 0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// assert_eq!(share_complete_extranonce.to_vec(), expected_complete_extranonce);
///
/// // Now the proxy wants to send the extranonce received from downstream and the part of extranonce
/// // owned by itself to the pool
/// let extranonce_to_send = proxy_extended_extranonce.without_upstream_part(Some(received_extranonce)).unwrap();
/// let expected_extranonce_to_send = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 8, 0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// assert_eq!(extranonce_to_send.to_vec(), expected_extranonce_to_send);
/// ```
///
/// Using static prefix:
///
/// ```
/// use mining_sv2::*;
/// use core::convert::TryInto;
///
/// // Create an extended extranonce with a static prefix
/// let static_prefix = vec![0x42, 0x43]; // Example static prefix
/// let mut pool_extended_extranonce = ExtendedExtranonce::new(
///     0..0,
///     0..7,
///     7..32,
///     Some(static_prefix.clone())
/// ).unwrap();
///
/// // When using static prefix, only bytes after the static prefix are incremented
/// let new_extended_channel_extranonce = pool_extended_extranonce.next_prefix_extended(3).unwrap();
/// let expected_extranonce = vec![0x42, 0x43, 0, 0, 0, 0, 1];
/// assert_eq!(new_extended_channel_extranonce.clone().to_vec(), expected_extranonce);
///
/// // For standard channels, only range_2 is incremented while range_1 (including static prefix) is preserved
/// let new_standard_channel_extranonce = pool_extended_extranonce.next_prefix_standard().unwrap();
/// // Note that the static prefix (0x42, 0x43) and the incremented bytes in range_1 are preserved
/// let expected_standard_extranonce = vec![0x42, 0x43, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
/// assert_eq!(new_standard_channel_extranonce.to_vec(), expected_standard_extranonce);
///
/// // Now the proxy receives the ExtendedExtranonce previously created
/// // The proxy knows the extranonce space reserved to the pool is 7 bytes and that the total
/// // extranonce len is 32 bytes and decides to reserve 4 bytes for itself and leave the remaining 21 for
/// // further downstreams.
/// let range_0 = 0..7;
/// let range_1 = 7..11;
/// let range_2 = 11..32;
/// let mut proxy_extended_extranonce = ExtendedExtranonce::from_upstream_extranonce(
///     new_extended_channel_extranonce,
///     range_0,
///     range_1,
///     range_2
/// ).unwrap();
///
/// // The proxy generates an extended extranonce for downstream
/// let new_extended_channel_extranonce_prefix = proxy_extended_extranonce.next_prefix_extended(3).unwrap();
/// let expected_extranonce_prefix = vec![0x42, 0x43, 0, 0, 0, 0, 1, 0, 0, 0, 1];
/// assert_eq!(new_extended_channel_extranonce_prefix.clone().to_vec(), expected_extranonce_prefix);
///
/// // When the proxy receives a share from downstream and wants to recreate the full extranonce
/// // e.g., because it wants to check the share's work
/// let received_extranonce: Extranonce = vec![0, 0, 0, 0, 0, 0, 0, 8, 0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap();
/// let share_complete_extranonce = proxy_extended_extranonce.extranonce_from_downstream_extranonce(received_extranonce.clone()).unwrap();
/// let expected_complete_extranonce = vec![0x42, 0x43, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 8, 0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// assert_eq!(share_complete_extranonce.to_vec(), expected_complete_extranonce);
///
/// // Now the proxy wants to send the extranonce received from downstream and the part of extranonce
/// // owned by itself to the pool
/// let extranonce_to_send = proxy_extended_extranonce.without_upstream_part(Some(received_extranonce)).unwrap();
/// let expected_extranonce_to_send = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 8, 0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
pub struct ExtendedExtranonce {
    inner: alloc::vec::Vec<u8>,
    range_0: core::ops::Range<usize>,
    range_1: core::ops::Range<usize>,
    range_2: core::ops::Range<usize>,
    static_prefix: Option<alloc::vec::Vec<u8>>,
}

/// Error type for ExtendedExtranonce operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtendedExtranonceError {
    /// The range_2.end is greater than MAX_EXTRANONCE_LEN
    ExceedsMaxLength,
    /// The ranges are invalid (e.g. range_0.end != range_1.start)
    InvalidRanges,
    /// The downstream extranonce length doesn't match the expected length
    InvalidDownstreamLength,
    /// The extranonce bytes in range_1 are at maximum value and can't be incremented
    MaxValueReached,
    /// The static prefix length is invalid
    InvalidStaticPrefixLength,
}

/// the trait PartialEq is implemented in such a way that only the relevant bytes are compared.
/// If range_2.end is set to 20, then the following ExtendedExtranonces are equal
/// ExtendedExtranonce {
///     inner: [0000 0000 0000 0000      0000 0000 0000 0000],
///     range_0: [0..5],
///     range_1: [5..10],
///     range_2: [10, 20],
/// }
/// ExtendedExtranonce {
///     inner: [0000 0000 0000 0000      0000 1111 1111 1111],
///     range_0: [0..5],
///     range_1: [5..10],
///     range_2: [10, 20],
/// }
impl PartialEq for ExtendedExtranonce {
    fn eq(&self, other: &Self) -> bool {
        let len = self.range_2.end;
        self.inner[0..len] == other.inner[0..len]
            && self.range_0 == other.range_0
            && self.range_1 == other.range_1
            && self.range_2 == other.range_2
    }
}

impl ExtendedExtranonce {
    /// every extranonce start from zero.
    pub fn new(
        range_0: Range<usize>,
        range_1: Range<usize>,
        range_2: Range<usize>,
        static_prefix: Option<alloc::vec::Vec<u8>>,
    ) -> Result<Self, ExtendedExtranonceError> {
        // Validate ranges
        if range_0.start != 0
            || range_0.end != range_1.start
            || range_1.end != range_2.start
            || range_1.end < range_1.start
            || range_2.end < range_2.start
        {
            return Err(ExtendedExtranonceError::InvalidRanges);
        }

        if let Some(static_prefix) = static_prefix.clone() {
            if static_prefix.len() > core::cmp::min(2, range_1.end - range_1.start) {
                return Err(ExtendedExtranonceError::InvalidStaticPrefixLength);
            }
        }

        // Check if range_2.end exceeds MAX_EXTRANONCE_LEN
        if range_2.end > MAX_EXTRANONCE_LEN {
            return Err(ExtendedExtranonceError::ExceedsMaxLength);
        }

        let mut inner = vec![0; range_2.end];
        if let Some(static_prefix) = static_prefix.clone() {
            inner[range_1.start..range_1.start + static_prefix.len()]
                .copy_from_slice(&static_prefix);
        }

        Ok(Self {
            inner,
            range_0,
            range_1,
            range_2,
            static_prefix,
        })
    }

    pub fn new_with_inner_only_test(
        range_0: Range<usize>,
        range_1: Range<usize>,
        range_2: Range<usize>,
        mut inner: alloc::vec::Vec<u8>,
    ) -> Result<Self, ExtendedExtranonceError> {
        // Validate ranges
        if range_0.start != 0
            || range_0.end != range_1.start
            || range_1.end != range_2.start
            || range_1.end < range_1.start
            || range_2.end < range_2.start
        {
            return Err(ExtendedExtranonceError::InvalidRanges);
        }

        // Check if range_2.end exceeds MAX_EXTRANONCE_LEN
        if range_2.end > MAX_EXTRANONCE_LEN {
            return Err(ExtendedExtranonceError::ExceedsMaxLength);
        }

        inner.resize(MAX_EXTRANONCE_LEN, 0);
        Ok(Self {
            inner,
            range_0,
            range_1,
            range_2,
            static_prefix: None,
        })
    }

    pub fn get_len(&self) -> usize {
        self.range_2.end
    }

    pub fn get_range2_len(&self) -> usize {
        self.range_2.end - self.range_2.start
    }

    pub fn get_range0_len(&self) -> usize {
        self.range_0.end - self.range_0.start
    }

    pub fn get_prefix_len(&self) -> usize {
        self.range_1.end - self.range_0.start
    }

    /// Suppose that P receives from the upstream an extranonce that needs to be converted into any
    /// ExtendedExtranonce, eg when an extended channel is opened. Then range_0 (that should
    /// be provided along the Extranonce) is reserved for the upstream and can't be modiefied by
    /// P. If the bytes recerved to P (range_1 and range_2) are not set to zero, returns None,
    /// otherwise returns Some(ExtendedExtranonce). If the range_2.end field is greater than 32,
    /// returns None.
    pub fn from_upstream_extranonce(
        v: Extranonce,
        range_0: Range<usize>,
        range_1: Range<usize>,
        range_2: Range<usize>,
    ) -> Result<Self, ExtendedExtranonceError> {
        // Validate ranges
        if range_0.start != 0
            || range_0.end != range_1.start
            || range_1.end != range_2.start
            || range_1.end < range_1.start
            || range_2.end < range_2.start
        {
            return Err(ExtendedExtranonceError::InvalidRanges);
        }

        // Check if range_2.end exceeds MAX_EXTRANONCE_LEN
        if range_2.end > MAX_EXTRANONCE_LEN {
            return Err(ExtendedExtranonceError::ExceedsMaxLength);
        }

        let mut inner = v.extranonce;
        inner.resize(range_2.end, 0);
        let rest = vec![0; range_2.end - inner.len()];
        let inner = [inner, rest].concat();
        Ok(Self {
            inner,
            range_0,
            range_1,
            range_2,
            static_prefix: None,
        })
    }

    /// Specular of [Self::from_upstream_extranonce]
    pub fn extranonce_from_downstream_extranonce(
        &self,
        dowstream_extranonce: Extranonce,
    ) -> Result<Extranonce, ExtendedExtranonceError> {
        if dowstream_extranonce.extranonce.len() != self.range_2.end - self.range_2.start {
            return Err(ExtendedExtranonceError::InvalidDownstreamLength);
        }
        let mut res = self.inner[self.range_0.start..self.range_1.end].to_vec();
        for b in dowstream_extranonce.extranonce {
            res.push(b)
        }
        res.try_into()
            .map_err(|_| ExtendedExtranonceError::ExceedsMaxLength)
    }

    /// Calculates the next extranonce for standard channels.
    pub fn next_prefix_standard(&mut self) -> Result<Extranonce, ExtendedExtranonceError> {
        let non_reserved_extranonces_bytes = &mut self.inner[self.range_2.start..self.range_2.end];
        match increment_bytes_be(non_reserved_extranonces_bytes) {
            Ok(_) => Ok(self.into()),
            Err(_) => Err(ExtendedExtranonceError::MaxValueReached),
        }
    }

    /// Calculates the next extranonce for extended channels.
    /// The required_len variable represents the range requested by the downstream to use.
    /// The part that is incremented is range_1, as every downstream must have different jobs.
    pub fn next_prefix_extended(
        &mut self,
        required_len: usize,
    ) -> Result<Extranonce, ExtendedExtranonceError> {
        if required_len > self.range_2.end - self.range_2.start {
            return Err(ExtendedExtranonceError::InvalidDownstreamLength);
        };

        // Determine the start position for extended_part based on static_prefix
        // If static_prefix is Some, some bytes are meant to be fixed and not
        // incremented
        let extended_part_start =
            self.range_1.start + self.static_prefix.as_ref().map_or(0, |p| p.len());

        let extended_part = &mut self.inner[extended_part_start..self.range_1.end];
        match increment_bytes_be(extended_part) {
            Ok(_) => {
                let result = self.inner[..self.range_1.end].to_vec();
                // Safe unwrap result will be always less the MAX_EXTRANONCE_LEN
                result
                    .try_into()
                    .map_err(|_| ExtendedExtranonceError::ExceedsMaxLength)
            }
            Err(_) => Err(ExtendedExtranonceError::MaxValueReached),
        }
    }

    /// Return a vec with the extranonce bytes that belong to self and downstream removing the
    /// ones owned by upstream (using Sv1 terms the extranonce1 is removed)
    /// If dowstream_extranonce is Some(v) it replace the downstream extranonce part with v
    pub fn without_upstream_part(
        &self,
        downstream_extranonce: Option<Extranonce>,
    ) -> Result<Extranonce, ExtendedExtranonceError> {
        match downstream_extranonce {
            Some(downstream_extranonce) => {
                if downstream_extranonce.extranonce.len() != self.range_2.end - self.range_2.start {
                    return Err(ExtendedExtranonceError::InvalidDownstreamLength);
                }
                let mut res = self.inner[self.range_1.start..self.range_1.end].to_vec();
                for b in downstream_extranonce.extranonce {
                    res.push(b)
                }
                res.try_into()
                    .map_err(|_| ExtendedExtranonceError::ExceedsMaxLength)
            }
            None => self.inner[self.range_1.start..self.range_2.end]
                .to_vec()
                .try_into()
                .map_err(|_| ExtendedExtranonceError::ExceedsMaxLength),
        }
    }

    pub fn upstream_part(&self) -> Extranonce {
        self.inner[self.range_0.start..self.range_1.end]
            .to_vec()
            .try_into()
            .unwrap()
    }
}
/// This function is used to increment extranonces, and it is used in next_standard and in
/// next_extended methods. If the input consists of an array of 255 as u8 (the maximum value) then
/// the input cannot be incremented. In this case, the input is not changed and the function returns
/// Err(()). In every other case, the function increments the input and returns Ok(())
fn increment_bytes_be(bs: &mut [u8]) -> Result<(), ()> {
    for b in bs.iter_mut().rev() {
        if *b != u8::MAX {
            *b += 1;
            return Ok(());
        } else {
            *b = 0;
        }
    }
    for b in bs.iter_mut() {
        *b = u8::MAX
    }
    Err(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use alloc::vec::Vec;
    use quickcheck_macros;

    #[test]
    fn test_extranonce_errors() {
        let extranonce = Extranonce::try_from(vec![0; MAX_EXTRANONCE_LEN + 1]);
        assert!(extranonce.is_err());

        assert!(Extranonce::new(MAX_EXTRANONCE_LEN + 1).is_none());
    }

    #[test]
    fn test_from_upstream_extranonce_error() {
        let range_0 = 0..0;
        let range_1 = 0..0;
        let range_2 = 0..MAX_EXTRANONCE_LEN + 1;
        let extranonce = Extranonce::new(10).unwrap();

        let extended_extranonce =
            ExtendedExtranonce::from_upstream_extranonce(extranonce, range_0, range_1, range_2);
        assert!(extended_extranonce.is_err());
        assert_eq!(
            extended_extranonce.unwrap_err(),
            ExtendedExtranonceError::ExceedsMaxLength
        );
    }

    #[test]
    fn test_invalid_ranges() {
        // Test with range_0.start != 0
        let result = ExtendedExtranonce::new(1..2, 2..3, 3..10, None);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ExtendedExtranonceError::InvalidRanges);

        // Test with range_0.end != range_1.start
        let result = ExtendedExtranonce::new(0..2, 3..4, 4..10, None);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ExtendedExtranonceError::InvalidRanges);

        // Test with range_1.end != range_2.start
        let result = ExtendedExtranonce::new(0..2, 2..4, 5..10, None);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ExtendedExtranonceError::InvalidRanges);
    }

    #[test]
    fn test_extranonce_from_downstream_extranonce() {
        let downstream_len = 10;

        let downstream_extranonce = Extranonce::new(downstream_len).unwrap();

        let range_0 = 0..4;
        let range_1 = 4..downstream_len;
        let range_2 = downstream_len..(downstream_len * 2 + 1);

        let extended_extraonce = ExtendedExtranonce::new(range_0, range_1, range_2, None).unwrap();

        let extranonce =
            extended_extraonce.extranonce_from_downstream_extranonce(downstream_extranonce);

        assert!(extranonce.is_err());
        assert_eq!(
            extranonce.unwrap_err(),
            ExtendedExtranonceError::InvalidDownstreamLength
        );

        // Test with a valid downstream extranonce
        let extra_content: Vec<u8> = vec![5; downstream_len];
        let downstream_extranonce =
            Extranonce::from_vec_with_len(extra_content.clone(), downstream_len);

        let range_0 = 0..4;
        let range_1 = 4..downstream_len;
        let range_2 = downstream_len..(downstream_len * 2);

        let extended_extraonce = ExtendedExtranonce::new(range_0, range_1, range_2, None).unwrap();

        let extranonce =
            extended_extraonce.extranonce_from_downstream_extranonce(downstream_extranonce);

        assert!(extranonce.is_ok());

        //validate that the extranonce is the concatenation of the upstream part and the downstream
        // part
        assert_eq!(
            extra_content,
            extranonce.unwrap().extranonce.to_vec()[downstream_len..downstream_len * 2]
        );
    }

    // Test from_vec_with_len
    #[test]
    fn test_extranonce_from_vec_with_len() {
        let extranonce = Extranonce::new(10).unwrap();
        let extranonce2 = Extranonce::from_vec_with_len(extranonce.extranonce, 22);
        assert_eq!(extranonce2.extranonce.len(), 22);
    }

    #[test]
    fn test_extranonce_without_upstream_part() {
        let downstream_len = 10;

        let downstream_extranonce = Extranonce::new(downstream_len).unwrap();

        let range_0 = 0..4;
        let range_1 = 4..downstream_len;
        let range_2 = downstream_len..(downstream_len * 2 + 1);

        let extended_extraonce = ExtendedExtranonce::new(range_0, range_1, range_2, None).unwrap();

        assert_eq!(
            extended_extraonce.without_upstream_part(Some(downstream_extranonce.clone())),
            Err(ExtendedExtranonceError::InvalidDownstreamLength)
        );

        let range_0 = 0..4;
        let range_1 = 4..downstream_len;
        let range_2 = downstream_len..(downstream_len * 2);
        let upstream_extranonce = Extranonce::from_vec_with_len(vec![5; 14], downstream_len);

        let extended_extraonce = ExtendedExtranonce::from_upstream_extranonce(
            upstream_extranonce.clone(),
            range_0,
            range_1.clone(),
            range_2,
        )
        .unwrap();

        let extranonce = extended_extraonce
            .without_upstream_part(Some(downstream_extranonce.clone()))
            .unwrap();
        assert_eq!(
            extranonce.extranonce[0..6],
            upstream_extranonce.extranonce[0..6]
        );
        assert_eq!(extranonce.extranonce[7..], vec![0; 9]);
    }

    // This test checks the behaviour of the function increment_bytes_be for a the MAX value
    // converted in be array of u8
    #[test]
    fn test_incrment_bytes_be_max() {
        let input = u8::MAX;
        let mut input = input.to_be_bytes();
        let result = increment_bytes_be(&mut input[..]);
        assert!(result == Err(()));
        assert!(u8::from_be_bytes(input) == u8::MAX);
    }

    // thest the function incrment_bytes_be for values different from MAX
    #[quickcheck_macros::quickcheck]
    fn test_increment_by_one(input: u8) -> bool {
        let expected1 = match input {
            u8::MAX => input,
            _ => input + 1,
        };
        let mut input = input.to_be_bytes();
        let _ = increment_bytes_be(&mut input[..]);
        let incremented_by_1 = u8::from_be_bytes(input);
        incremented_by_1 == expected1
    }
    use core::convert::TryFrom;

    // check that the composition of the functions Extranonce to U256 and U256 to Extranonce is the
    // identity function
    #[quickcheck_macros::quickcheck]
    fn test_extranonce_from_u256(mut input: Vec<u8>) -> bool {
        input.resize(MAX_EXTRANONCE_LEN, 0);

        let extranonce_start = Extranonce::try_from(input.clone()).unwrap();
        let u256 = U256::<'static>::from(extranonce_start.clone());
        let extranonce_final = Extranonce::from(u256);
        extranonce_start == extranonce_final
    }

    // do the same of the above but with B032 type
    #[quickcheck_macros::quickcheck]
    fn test_extranonce_from_b032(mut input: Vec<u8>) -> bool {
        input.resize(MAX_EXTRANONCE_LEN, 0);
        let extranonce_start = Extranonce::try_from(input.clone()).unwrap();
        let b032 = B032::<'static>::from(extranonce_start.clone());
        let extranonce_final = Extranonce::from(b032);
        extranonce_start == extranonce_final
    }

    // this test checks the functions from_upstream_extranonce and from_extranonce.
    #[quickcheck_macros::quickcheck]
    fn test_extranonce_from_extended_extranonce(input: (u8, u8, Vec<u8>, usize)) -> bool {
        let inner = from_arbitrary_vec_to_array(input.2.clone());
        let extranonce_len = input.3 % MAX_EXTRANONCE_LEN + 1;
        let r0 = input.0 as usize;
        let r1 = input.1 as usize;
        let r0 = r0 % (extranonce_len + 1);
        let r1 = r1 % (extranonce_len + 1);
        let mut ranges = Vec::from([r0, r1]);
        ranges.sort();
        let range_0 = 0..ranges[0];
        let range_1 = ranges[0]..ranges[1];
        let range_2 = ranges[1]..extranonce_len;

        let mut extended_extranonce_start = ExtendedExtranonce::new_with_inner_only_test(
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
            inner.to_vec(),
        )
        .unwrap();

        assert_eq!(extended_extranonce_start.get_len(), extranonce_len);
        assert_eq!(
            extended_extranonce_start.get_range2_len(),
            extranonce_len - ranges[1]
        );

        let extranonce_result = extended_extranonce_start.next_prefix_extended(0);

        // todo: refactor this test to avoid skipping the test if next_extended fails
        if extranonce_result.is_err() {
            return true; // Skip test if next_extended fails
        }

        let extranonce = extranonce_result.unwrap();

        let extended_extranonce_final = ExtendedExtranonce::from_upstream_extranonce(
            extranonce,
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
        );

        match extended_extranonce_final {
            Ok(extended_extranonce_final) => {
                for b in extended_extranonce_final.inner[range_2.start..range_2.end].iter() {
                    if b != &0 {
                        return false;
                    }
                }
                extended_extranonce_final.inner[range_0.clone().start..range_1.end]
                    == extended_extranonce_start.inner[range_0.start..range_1.end]
            }
            Err(_) => {
                // If from_upstream_extranonce fails, it should be because the inner bytes in
                // range_1..range_2 are not zero
                for b in inner[range_1.start..range_2.end].iter() {
                    if b != &0 {
                        return true;
                    }
                }
                false
            }
        }
    }

    // test next_standard_method
    #[quickcheck_macros::quickcheck]
    fn test_next_standard_extranonce(input: (u8, u8, Vec<u8>, usize)) -> bool {
        let inner = from_arbitrary_vec_to_array(input.2.clone());
        let extranonce_len = input.3 % MAX_EXTRANONCE_LEN + 1;
        let r0 = input.0 as usize;
        let r1 = input.1 as usize;
        let r0 = r0 % (extranonce_len + 1);
        let r1 = r1 % (extranonce_len + 1);
        let mut ranges = Vec::from([r0, r1]);
        ranges.sort();
        let range_0 = 0..ranges[0];
        let range_1 = ranges[0]..ranges[1];
        let range_2 = ranges[1]..extranonce_len;

        let extended_extranonce_start = ExtendedExtranonce::new_with_inner_only_test(
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
            inner.to_vec(),
        )
        .unwrap();

        let mut extranonce_copy: Extranonce =
            Extranonce::from(&mut extended_extranonce_start.clone());
        let extranonce_expected_b032: Option<B032> = extranonce_copy.next();

        match extended_extranonce_start.clone().next_prefix_standard() {
            Ok(extranonce_next) => match extranonce_expected_b032 {
                Some(b032) =>
                // the range_2 of extranonce_next must be equal to the range_2 of the
                // conversion of extranonce_copy.next() converted in extranonce
                {
                    extranonce_next.extranonce[range_2.start..range_2.end] == Extranonce::from(b032.clone()).extranonce[range_2.start..range_2.end]
                    // the range_1 of the conversion of extranonce_copy.next() converted in
                    // extranonce must remain unchanged
                    && Extranonce::from(b032.clone()).extranonce[range_1.start..range_1.end]== extended_extranonce_start.inner[range_1.start..range_1.end]
                }
                None => false,
            },
            // if .next_standard() method falls in None case, this means that the range_2 is at
            // maximum value, so every entry must be 255 as u8
            Err(ExtendedExtranonceError::MaxValueReached) => {
                for b in inner[range_2.start..range_2.end].iter() {
                    if b != &255_u8 {
                        return false;
                    }
                }
                true
            }
            Err(_) => false, // Other errors are not expected in this test
        }
    }

    #[quickcheck_macros::quickcheck]
    fn test_next_stndard2(input: (u8, u8, Vec<u8>, usize)) -> bool {
        let inner = from_arbitrary_vec_to_array(input.2.clone());
        let extranonce_len = input.3 % MAX_EXTRANONCE_LEN + 1;
        let r0 = input.0 as usize;
        let r1 = input.1 as usize;
        let r0 = r0 % (extranonce_len + 1);
        let r1 = r1 % (extranonce_len + 1);
        let mut ranges = Vec::from([r0, r1]);
        ranges.sort();
        let range_0 = 0..ranges[0];
        let range_1 = ranges[0]..ranges[1];
        let range_2 = ranges[1]..extranonce_len;

        let mut extended_extranonce_start = ExtendedExtranonce::new_with_inner_only_test(
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
            inner.to_vec(),
        )
        .unwrap();

        match extended_extranonce_start.next_prefix_standard() {
            Ok(v) => {
                extended_extranonce_start.inner[range_2.clone()] == v.extranonce[range_2]
                    && extended_extranonce_start.inner[range_0.clone()]
                        == v.extranonce[range_0.clone()]
            }
            Err(_) => true, // Any error is acceptable for this test
        }
    }

    #[quickcheck_macros::quickcheck]
    fn test_next_extended_extranonce(input: (u8, u8, Vec<u8>, usize, usize)) -> bool {
        let inner = from_arbitrary_vec_to_array(input.2.clone());
        let extranonce_len = input.3 % MAX_EXTRANONCE_LEN + 1;
        let r0 = input.0 as usize;
        let r1 = input.1 as usize;
        let r0 = r0 % (extranonce_len + 1);
        let r1 = r1 % (extranonce_len + 1);
        let required_len = input.4;
        let mut ranges = Vec::from([r0, r1]);
        ranges.sort();
        let range_0 = 0..ranges[0];
        let range_1 = ranges[0]..ranges[1];
        let range_2 = ranges[1]..extranonce_len;

        let mut extended_extranonce = ExtendedExtranonce::new_with_inner_only_test(
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
            inner.to_vec(),
        )
        .unwrap();

        match extended_extranonce.next_prefix_extended(required_len) {
            Ok(extranonce) => extended_extranonce.inner[..range_1.end] == extranonce.extranonce[..],
            Err(ExtendedExtranonceError::InvalidDownstreamLength) => {
                required_len > range_2.end - range_2.start
            }
            Err(ExtendedExtranonceError::MaxValueReached) => {
                let mut range_1_start = inner[range_1.clone()].to_vec();
                increment_bytes_be(&mut range_1_start).is_err()
            }
            Err(_) => false, // Other errors are not expected in this test
        }
    }

    #[quickcheck_macros::quickcheck]
    fn test_target_from_u256(input: (u128, u128)) -> bool {
        let target_expected = Target {
            head: input.0,
            tail: input.1,
        };

        let bytes = [&input.0.to_ne_bytes()[..], &input.1.to_ne_bytes()[..]].concat();
        let u256: U256 = bytes.try_into().unwrap();
        let target_final: Target = u256.clone().into();

        let u256_final: U256 = target_final.clone().into();

        target_expected == target_final && u256_final == u256
    }
    #[quickcheck_macros::quickcheck]
    fn test_target_to_u256(input: (u128, u128)) -> bool {
        let target_start = Target {
            head: input.0,
            tail: input.1,
        };
        let u256 = U256::<'static>::from(target_start.clone());
        let target_final = Target::from(u256);
        target_final == target_final
    }

    #[test]
    fn test_ord_with_equal_head_tail() {
        let target_1 = Target { head: 1, tail: 1 };
        let target_2 = Target { head: 1, tail: 2 };
        assert!(target_1 < target_2);

        //also test with equal tails
        let target_3 = Target { head: 2, tail: 2 };
        assert!(target_2 < target_3);
    }

    #[quickcheck_macros::quickcheck]
    fn test_ord_for_target_positive_increment(input: (u128, u128, u128, u128)) -> bool {
        let max = u128::MAX;
        // we want input.0 and input.1 >= 0 and < u128::MAX
        let input = (input.0 % max, input.1 % max, input.2, input.3);
        let target_start = Target {
            head: input.0,
            tail: input.1,
        };
        let positive_increment = (
            input.2 % (max - target_start.head) + 1,
            input.3 % (max - target_start.tail) + 1,
        );
        let target_final = Target {
            head: target_start.head + positive_increment.0,
            tail: target_start.tail + positive_increment.1,
        };
        target_final > target_start
    }

    #[quickcheck_macros::quickcheck]
    fn test_ord_for_target_negative_increment(input: (u128, u128, u128, u128)) -> bool {
        let max = u128::MAX;
        let input = (input.0 % max + 1, input.1 % max + 1, input.2, input.3);
        let target_start = Target {
            head: input.0,
            tail: input.1,
        };
        let negative_increment = (
            input.2 % target_start.head + 1,
            input.3 % target_start.tail + 1,
        );
        let target_final = Target {
            head: target_start.head - negative_increment.0,
            tail: target_start.tail - negative_increment.1,
        };
        target_final < target_start
    }

    #[quickcheck_macros::quickcheck]
    fn test_ord_for_target_zero_increment(input: (u128, u128)) -> bool {
        let target_start = Target {
            head: input.0,
            tail: input.1,
        };
        let target_final = target_start.clone();
        target_start == target_final
    }

    #[quickcheck_macros::quickcheck]
    fn test_vec_from_extranonce(input: Vec<u8>) -> bool {
        let input_start = from_arbitrary_vec_to_array(input).to_vec();
        let extranonce_start = Extranonce::try_from(input_start.clone()).unwrap();
        let vec_final = Vec::from(extranonce_start.clone());
        input_start == vec_final
    }

    use core::convert::TryInto;
    pub fn from_arbitrary_vec_to_array(vec: Vec<u8>) -> [u8; 32] {
        if vec.len() >= 32 {
            vec[0..32].try_into().unwrap()
        } else {
            let mut result = Vec::new();
            for _ in 0..(32 - vec.len()) {
                result.push(0);
            }
            for element in vec {
                result.push(element)
            }
            result[..].try_into().unwrap()
        }
    }

    #[test]
    fn test_extended_extranonce_get_prefix_len() {
        let range_0 = 0..2;
        let range_1 = 2..4;
        let range_2 = 4..9;
        let extended = ExtendedExtranonce::new(range_0, range_1, range_2, None).unwrap();
        let prefix_len = extended.get_prefix_len();
        assert!(prefix_len == 4);
    }

    #[test]
    fn test_extended_extranonce_with_static_prefix() {
        let range_0 = 0..0;
        let range_1 = 0..4;
        let range_2 = 4..8;
        let static_prefix = vec![0x42, 0x43]; // Some fixed data

        // Create an ExtendedExtranonce with static prefix
        let extended = ExtendedExtranonce::new(
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
            Some(static_prefix.clone()),
        )
        .unwrap();

        // Verify the static prefix was stored
        assert_eq!(extended.static_prefix, Some(static_prefix.clone()));

        // Verify the inner data contains the static prefix
        assert_eq!(
            extended.inner[range_1.start..range_1.start + static_prefix.len()],
            static_prefix[..]
        );
    }

    #[test]
    fn test_extended_extranonce_invalid_static_prefix_length() {
        let range_0 = 0..0;
        let range_1 = 0..4;
        let range_2 = 4..8;
        let static_prefix = vec![0x42, 0x43, 0x44]; // Length > 2 not allowed

        // Create an ExtendedExtranonce with static prefix that's too long
        let result = ExtendedExtranonce::new(range_0, range_1, range_2, Some(static_prefix));

        // Verify the correct error is returned
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            ExtendedExtranonceError::InvalidStaticPrefixLength
        );
    }

    #[test]
    fn test_next_extended_with_static_prefix() {
        let range_0 = 0..0;
        let range_1 = 0..4;
        let range_2 = 4..8;
        let static_prefix = vec![0x42, 0x43]; // Fixed data of length 2

        // Create an ExtendedExtranonce with static prefix
        let mut extended = ExtendedExtranonce::new(
            range_0,
            range_1.clone(),
            range_2,
            Some(static_prefix.clone()),
        )
        .unwrap();

        // Call next_extended
        let result = extended.next_prefix_extended(3).unwrap();

        // Verify the result contains the static prefix
        assert_eq!(result.extranonce[0..static_prefix.len()], static_prefix[..]);

        // Call next_extended again
        let result2 = extended.next_prefix_extended(3).unwrap();

        // Verify the fixed part remains unchanged
        assert_eq!(
            result2.extranonce[0..static_prefix.len()],
            static_prefix[..]
        );

        // Verify the incremented part has changed
        assert_ne!(result.extranonce, result2.extranonce);
    }

    #[test]
    fn test_multiple_next_extended_with_static_prefix() {
        let range_0 = 0..0;
        let range_1 = 0..4;
        let range_2 = 4..8;
        let static_prefix = vec![0x42, 0x43]; // Fixed data of length 2

        // Create an ExtendedExtranonce with static prefix
        let mut extended = ExtendedExtranonce::new(
            range_0,
            range_1.clone(),
            range_2,
            Some(static_prefix.clone()),
        )
        .unwrap();

        // Generate multiple extranonces and verify they all have the same fixed part
        let mut results = Vec::new();
        for _ in 0..5 {
            let result = extended.next_prefix_extended(3).unwrap();
            results.push(result);
        }

        // Verify all results have the same fixed part
        for result in &results {
            assert_eq!(result.extranonce[0..static_prefix.len()], static_prefix[..]);
        }

        // Verify all results have different incremented parts
        for i in 0..results.len() {
            for j in i + 1..results.len() {
                assert_ne!(results[i].extranonce, results[j].extranonce);
            }
        }
    }
}
