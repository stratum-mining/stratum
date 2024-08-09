#![cfg_attr(feature = "no_std", no_std)]

//! # Mining Protocol
//! ## Channels
//! The protocol is designed such that downstream devices (or proxies) open communication
//! channels with upstream stratum nodes within established connections. The upstream stratum
//! endpoints could be actual mining servers or proxies that pass the messages further upstream.
//! Each channel identifies a dedicated mining session associated with an authorized user.
//! Upstream stratum nodes accept work submissions and specify a mining target on a
//! per-channel basis.
//!
//! There can theoretically be up to 2^32 open channels within one physical connection to an
//! upstream stratum node. All channels are independent of each other, but share some messages
//! broadcasted from the server for higher efficiency (e.g. information about a new prevhash).
//! Each channel is identified by its channel_id (U32), which is consistent throughout the whole
//! life of the connection.
//!
//! A proxy can either transparently allow its clients to open separate channels with the server
//! (preferred behaviour) or aggregate open connections from downstream devices into its own
//! open channel with the server and translate the messages accordingly (present mainly for
//! allowing v1 proxies). Both options have some practical use cases. In either case, proxies
//! SHOULD aggregate clients’ channels into a smaller number of TCP connections. This saves
//! network traffic for broadcast messages sent by a server because fewer messages need to be
//! sent in total, which leads to lower latencies as a result. And it further increases efficiency by
//! allowing larger packets to be sent.
//!
//! The protocol defines three types of channels: **standard channels** , **extended channels** (mining
//! sessions) and **group channels** (organizational), which are useful for different purposes.
//! The main difference between standard and extended channels is that standard channels
//! cannot manipulate the coinbase transaction / Merkle path, as they operate solely on provided
//! Merkle roots. We call this **header-only mining**. Extended channels, on the other hand, are
//! given extensive control over the search space so that they can implement various advanceduse cases such as translation between v1 and v2 protocols, difficulty aggregation, custom
//! search space splitting, etc.
//!
//! This separation vastly simplifies the protocol implementation for clients that don’t support
//! extended channels, as they only need to implement the subset of protocol messages related to
//! standard channels (see Mining Protocol Messages for details).
//!
//! ### Standard Channels
//!
//! Standard channels are intended to be used by end mining devices.
//! The size of the search space for one standard channel (header-only mining) for one particular
//! value in the nTime field is 2^(NONCE_BITS + VERSION_ROLLING_BITS) = ~280Th, where
//! NONCE_BITS = 32 and VERSION_ROLLING_BITS = 16. This is a guaranteed space before
//! nTime rolling (or changing the Merkle root).
//! The protocol dedicates all directly modifiable bits (version, nonce, and nTime) from the block
//! header to one mining channel. This is the smallest assignable unit of search space by the
//! protocol. The client which opened the particular channel owns the whole assigned space and
//! can split it further if necessary (e.g. for multiple hashing boards and for individual chips etc.).
//!
//! ### Extended channels
//!
//! Extended channels are intended to be used by proxies. Upstream servers which accept
//! connections and provide work MUST support extended channels. Clients, on the other hand, do
//! not have to support extended channels, as they MAY be implemented more simply with only
//! standard channels at the end-device level. Thus, upstream servers providing work MUST also
//! support standard channels.
//! The size of search space for an extended channel is
//! 2^(NONCE_BITS+VERSION_ROLLING_BITS+extranonce_size*8) per nTime value.
//!
//! ### Group Channels
//!
//! Standard channels opened within one particular connection can be grouped together to be
//! addressable by a common communication group channel.
//! Whenever a standard channel is created it is always put into some channel group identified by
//! its group_channel_id. Group channel ID namespace is the same as channel ID namespace on a
//! particular connection but the values chosen for group channel IDs must be distinct.
//!
//! ### Future Jobs
//! An empty future block job or speculated non-empty job can be sent in advance to speedup
//! new mining job distribution. The point is that the mining server MAY have precomputed such a
//! job and is able to pre-distribute it for all active channels. The only missing information to
//! start to mine on the new block is the new prevhash. This information can be provided
//! independently.Such an approach improves the efficiency of the protocol where the upstream node
//! doesn’t waste precious time immediately after a new block is found in the network.
//!
//! ### Hashing Space Distribution
//! Each mining device has to work on a unique part of the whole search space. The full search
//! space is defined in part by valid values in the following block header fields:
//! * Nonce header field (32 bits),
//! * Version header field (16 bits, as specified by BIP 320),
//! * Timestamp header field.
//!
//! The other portion of the block header that’s used to define the full search space is the Merkle
//! root hash of all transactions in the block, projected to the last variable field in the block
//! header:
//!
//! * Merkle root, deterministically computed from:
//!   * Coinbase transaction: typically 4-8 bytes, possibly much more.
//!   * Transaction set: practically unbounded space. All roles in Stratum v2 MUST NOT
//!     use transaction selection/ordering for additional hash space extension. This
//!     stems both from the concept that miners/pools should be able to choose their
//!     transaction set freely without any interference with the protocol, and also to
//!     enable future protocol modifications to Bitcoin. In other words, any rules
//!     imposed on transaction selection/ordering by miners not described in the rest of
//!     this document may result in invalid work/blocks.
//!
//! Mining servers MUST assign a unique subset of the search space to each connection/channel
//! (and therefore each mining device) frequently and rapidly enough so that the mining devices
//! are not running out of search space. Unique jobs can be generated regularly by:
//! * Putting unique data into the coinbase for each connection/channel, and/or
//! * Using unique work from a work provider, e.g. a previous work update (note that this is
//!   likely more difficult to implement, especially in light of the requirement that transaction
//!   selection/ordering not be used explicitly for additional hash space distribution).
//!
//! This protocol explicitly expects that upstream server software is able to manage the size of
//! the hashing space correctly for its clients and can provide new jobs quickly enough.
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
mod reconnect;
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
pub use reconnect::Reconnect;
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
const MAX_EXTRANONCE_LEN: usize = 32;

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
/// Extranonce bytes which need to be added to the coinbase to form a fully valid submission:
/// (full coinbase = coinbase_tx_prefix + extranonce + coinbase_tx_suffix).
/// Representation is in big endian, so tail is for the digits relative to smaller powers
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
impl<'a> From<Extranonce> for U256<'a> {
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
impl<'a> From<Extranonce> for B032<'a> {
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
    pub fn next(&mut self) -> Option<B032> {
        increment_bytes_be(&mut self.extranonce).ok()?;
        // below unwraps never panics
        Some(self.extranonce.clone().try_into().unwrap())
    }

    pub fn to_vec(self) -> alloc::vec::Vec<u8> {
        self.extranonce
    }

    /// Return only the prefix part of the extranonce
    /// If the required size is greater than the extranonce len it return None
    pub fn into_prefix(&self, prefix_len: usize) -> Option<B032<'static>> {
        if prefix_len > self.extranonce.len() {
            None
        } else {
            let mut prefix = self.extranonce.clone();
            prefix.resize(prefix_len, 0);
            // unwrap is sage as prefix_len can not be greater than 32 cause is not possible to
            // contruct Extranonce with the inner vecto greater than 32.
            Some(prefix.try_into().unwrap())
        }
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
/// Downstram and upstream are not global terms but are relative
/// to an actor of the protocol P. In simple terms, upstream is the part of the protocol that a
/// user P sees when he looks above and downstream when he looks beneath.
///
/// An ExtendedExtranonce is defined by 3 ranges:
///
///  - range_0: is the range that represents the extended extranonce part that reserved by upstream
/// relative to P (for most upstreams nodes, e.g. a pool, this is [0..0]) and it is fixed for P.
///  - range_1: is the range that represents the extended extranonce part reserved to P. P assigns
/// to every relative downstream an extranonce with different value in the range 1 in the
/// following way: if D_i is the (i+1)-th downstream that connected to P, then D_i gets from P and
/// extranonce with range_1=i (note that the concatenation of range_1 and range_1 is the range_0
/// relative to D_i and range_2 of P is the range_1 of D_i).
///  - range_2: is the range that P reserve for the downstreams.
///
///
/// In the following examples, we examine the extended extranonce in some cases.
///
/// The user P is the pool.
///  - range_0 -> 0..0, there is no upstream relative to the pool P, so no space reserved by the
/// upstream
///  - range_1 -> 0..16 the pool P increments the first 16 bytes to be sure the each pool's
/// downstream get a different extranonce or a different extended extranoce search space (more
/// on that below*)
///  - range_2 -> 16..32 this bytes are not changed by the pool but are changed by the pool's
/// downstream
///
/// The user P is the translator.
///  - range_0 -> 0..16 these bytes are set by the pool and P shouldn't change them
///  - range_1 -> 16..24 these bytes are modified by P each time that a sv1 mining device connect,
///  so we can be sure that each connected sv1 mining device get a different extended extranonce
///  search space
///  - range_2 -> 24..32 these bytes are left free for the sv1 miniing device
///
/// The user P is a sv1 mining device.
///  - range_0 -> 0..24 these byteshadd set by the device's upstreams
/// range_1 -> 24..32 these bytes are changed by P (if capable) in order to increment the
/// search space
/// range_2 -> 32..32 no more downstream
///
///
///
/// About how the pool work having both extended and standard downstreams:
///
/// the pool reserve the first 16 bytes for herself and let downstreams change the lase 16, so
/// each one of the possible 2^16 number represent an extended channel
/// pool space                               | downstream space
/// 0000 0000 0000 0000      0000 0000 0000 0000    extended channel number 0
/// 0000 0000 0000 0001      0000 0000 0000 0000    extended channel number 1
/// 0000 0000 0000 0002      0000 0000 0000 0000    extended channel number 2
/// 0000 0000 0000 0003      0000 0000 0000 0000    extended channel number 3
/// ....
/// In order to assign extranonces to standard channels the pool reserve to herself the first
/// extended channel, so the extended extranonce received by the first open extended channel wont
/// be the channel 0 but the channel 1
/// 0000 0000 0000 0001      0000 0000 0000 0000
/// and all the extranonces for standard channel are generate from the extended channel number 0
/// so the first standard cahnnel will be
/// 0000 0000 0000 0000      0000 0000 0000 0000
/// where the second will be
/// 0000 0000 0000 0000      0000 0000 0000 0001 ecc ecc
///
///
///
/// ExtendedExtranonce type is meant to be used in cases where the extranonce length is not
/// 32bytes. So, the inner part is an array of 32bytes, but only the firsts corresponding to the
/// range_2.end are used by the pool
///
/// # Examples
///
/// ```
/// use mining_sv2::*;
/// use core::convert::TryInto;
/// // Cretae an extended extranonce of len 32 reserve the first 7 bytes for the pool.
/// let mut pool_extended_extranonce = ExtendedExtranonce::new(0..0, 0..7, 7..32);
///
/// // On open extended channel the pool do
/// let new_extended_channel_extranonce = pool_extended_extranonce.next_extended(3).unwrap();
/// let new_extended_channel_extranonce_inner = vec![0,0,0,0,0,0,1];
/// assert_eq!(new_extended_channel_extranonce.clone().to_vec(), new_extended_channel_extranonce_inner);
///
/// // Then the pool receive a request to open a standard channel
/// let new_standard_channel_extranonce = pool_extended_extranonce.next_standard().unwrap();
/// let new_standard_channel_extranonce_inner = vec![0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1];
/// assert_eq!(new_standard_channel_extranonce.to_vec(), new_standard_channel_extranonce_inner);
///
/// // Now the proxy receive the ExtdendedExtranonce previously created
/// // The proxy know the extranonce space reserved to the pool is 7 bytes and that the total
/// // extranonce len is 32 bytes and decide to reserve 4 bytes for him and leave the remaining 21 do
/// // futher downstreams.
/// let range_0 = 0..7;
/// let range_1 = 7..11;
/// let range_2 = 11..32;
/// let mut proxy_extended_extranonce = ExtendedExtranonce::from_upstream_extranonce(new_extended_channel_extranonce,range_0, range_1, range_2).unwrap();
///
/// // The proxy generate an extended extranonce for downstream
/// let new_extended_channel_extranonce = proxy_extended_extranonce.next_extended(3).unwrap();
/// let new_extended_channel_extranonce_inner = vec![0,0,0,0,0,0,1,0,0,0,1];
/// assert_eq!(new_extended_channel_extranonce.clone().to_vec(), new_extended_channel_extranonce_inner);
///
/// // When the proxy receive a share from downstream and want to recreate the all extranonce eg
/// // cause it want to check the share's work
/// let received_extranonce: Extranonce = vec![0_u8,0,0,0,0,0,0,8,0,50,0,0,0,0,0,0,0,0,0,0,0].try_into().unwrap();
/// let share_complete_extranonce = proxy_extended_extranonce.extranonce_from_downstream_extranonce(received_extranonce.clone()).unwrap();
/// let share_complete_extranonce_inner = vec![0,0,0,0,0,0,1,0,0,0,1,0,0,0,0,0,0,0,8,0,50,0,0,0,0,0,0,0,0,0,0,0];
/// assert_eq!(share_complete_extranonce.to_vec(), share_complete_extranonce_inner);
///
///
/// // Now the proxy want to send the extranonce received from downstream and the part of extranonce
/// // owned by him to the pool
/// let extranonce_to_send = proxy_extended_extranonce.without_upstream_part(Some(received_extranonce)).unwrap();
/// let extranonce_to_send_inner = vec![0,0,0,1,0,0,0,0,0,0,0,8,0,50,0,0,0,0,0,0,0,0,0,0,0];
/// assert_eq!(extranonce_to_send.to_vec(),extranonce_to_send_inner);
/// ```
pub struct ExtendedExtranonce {
    inner: [u8; MAX_EXTRANONCE_LEN],
    range_0: core::ops::Range<usize>,
    range_1: core::ops::Range<usize>,
    range_2: core::ops::Range<usize>,
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
    pub fn new(range_0: Range<usize>, range_1: Range<usize>, range_2: Range<usize>) -> Self {
        debug_assert!(range_0.start == 0);
        debug_assert!(range_0.end == range_1.start);
        debug_assert!(range_1.end == range_2.start);
        Self {
            inner: [0; MAX_EXTRANONCE_LEN],
            range_0,
            range_1,
            range_2,
        }
    }

    pub fn new_with_inner_only_test(
        range_0: Range<usize>,
        range_1: Range<usize>,
        range_2: Range<usize>,
        mut inner: alloc::vec::Vec<u8>,
    ) -> Self {
        inner.resize(MAX_EXTRANONCE_LEN, 0);
        let inner = inner.try_into().unwrap();
        Self {
            inner,
            range_0,
            range_1,
            range_2,
        }
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
    ) -> Option<Self> {
        debug_assert!(range_0.start <= range_0.end);
        debug_assert!(range_0.end <= range_1.start);
        debug_assert!(range_1.start <= range_1.end);
        debug_assert!(range_1.end <= range_2.start);
        debug_assert!(range_2.start <= range_2.end);
        if range_2.end > MAX_EXTRANONCE_LEN {
            return None;
        }
        let mut inner = v.extranonce;
        inner.resize(range_2.end, 0);
        let rest = vec![0; MAX_EXTRANONCE_LEN - inner.len()];
        // below unwraps never panics
        let inner: [u8; MAX_EXTRANONCE_LEN] = [inner, rest].concat().try_into().unwrap();
        Some(Self {
            inner,
            range_0,
            range_1,
            range_2,
        })
    }

    /// Specular of [Self::from_upstream_extranonce]
    pub fn extranonce_from_downstream_extranonce(
        &self,
        dowstream_extranonce: Extranonce,
    ) -> Option<Extranonce> {
        if dowstream_extranonce.extranonce.len() != self.range_2.end - self.range_2.start {
            return None;
        }
        let mut res = self.inner[self.range_0.start..self.range_1.end].to_vec();
        for b in dowstream_extranonce.extranonce {
            res.push(b)
        }
        res.try_into().ok()
    }

    /// This function takes in input an ExtendedExtranonce for the extended channel. The number
    /// represented by the bytes in range_2 is incremented by 1 and the ExtendedExtranonce is
    /// converted in an Extranonce. If range_2 is at maximum value, the output is None.
    pub fn next_standard(&mut self) -> Option<Extranonce> {
        let reserved_extranonce_bytes = &mut self.inner[self.range_1.start..self.range_1.end];
        for b in reserved_extranonce_bytes {
            *b = 0
        }
        let non_reserved_extranonces_bytes = &mut self.inner[self.range_2.start..self.range_2.end];
        match increment_bytes_be(non_reserved_extranonces_bytes) {
            Ok(_) => Some(self.into()),
            Err(_) => None,
        }
    }

    /// This function calculates the next extranonce, but the output is ExtendedExtranonce. The
    /// required_len variable represents the range requested by the downstream to use. The part
    /// incremented is range_1, as every downstream must have different jobs.
    pub fn next_extended(&mut self, required_len: usize) -> Option<Extranonce> {
        if required_len > self.range_2.end - self.range_2.start {
            return None;
        };
        let extended_part = &mut self.inner[self.range_1.start..self.range_1.end];
        match increment_bytes_be(extended_part) {
            Ok(_) => {
                let result = self.inner[..self.range_1.end].to_vec();
                // Safe unwrap result will be always less the MAX_EXTRANONCE_LEN
                Some(result.try_into().unwrap())
            }
            Err(_) => None,
        }
    }

    /// Return a vec with the extranonce bytes that belong to self and downstream removing the
    /// ones owned by upstream (using Sv1 terms the extranonce1 is removed)
    /// If dowstream_extranonce is Some(v) it replace the downstream extranonce part with v
    pub fn without_upstream_part(
        &self,
        downstream_extranonce: Option<Extranonce>,
    ) -> Option<Extranonce> {
        match downstream_extranonce {
            Some(downstream_extranonce) => {
                if downstream_extranonce.extranonce.len() != self.range_2.end - self.range_2.start {
                    return None;
                }
                let mut res = self.inner[self.range_1.start..self.range_1.end].to_vec();
                for b in downstream_extranonce.extranonce {
                    res.push(b)
                }
                Some(res.try_into().ok()?)
            }
            None => self.inner[self.range_1.start..self.range_2.end]
                .to_vec()
                .try_into()
                .ok(),
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

        assert!(Extranonce::new(MAX_EXTRANONCE_LEN + 1) == None);
    }

    #[test]
    fn test_from_upstream_extranonce_error() {
        let range_0 = 0..0;
        let range_1 = 0..0;
        let range_2 = 0..MAX_EXTRANONCE_LEN + 1;
        let extranonce = Extranonce::new(10).unwrap();

        let extended_extranonce =
            ExtendedExtranonce::from_upstream_extranonce(extranonce, range_0, range_1, range_2);
        assert!(extended_extranonce.is_none());
    }

    #[test]
    fn test_extranonce_from_downstream_extranonce() {
        let downstream_len = 10;

        let downstream_extranonce = Extranonce::new(downstream_len).unwrap();

        let range_0 = 0..4;
        let range_1 = 4..downstream_len;
        let range_2 = downstream_len..(downstream_len * 2 + 1);

        let extended_extraonce = ExtendedExtranonce::new(range_0, range_1, range_2);

        let extranonce =
            extended_extraonce.extranonce_from_downstream_extranonce(downstream_extranonce);

        assert!(extranonce.is_none());

        // Test with a valid downstream extranonce
        let extra_content: Vec<u8> = vec![5; downstream_len];
        let downstream_extranonce =
            Extranonce::from_vec_with_len(extra_content.clone(), downstream_len);

        let range_0 = 0..4;
        let range_1 = 4..downstream_len;
        let range_2 = downstream_len..(downstream_len * 2);

        let extended_extraonce = ExtendedExtranonce::new(range_0, range_1, range_2);

        let extranonce =
            extended_extraonce.extranonce_from_downstream_extranonce(downstream_extranonce);

        assert!(extranonce.is_some());

        //validate that the extranonce is the concatenation of the upstream part and the downstream part
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

        let extended_extraonce = ExtendedExtranonce::new(range_0, range_1, range_2);

        assert_eq!(
            extended_extraonce.without_upstream_part(Some(downstream_extranonce.clone())),
            None
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
        let extranonce_len = input.3.clone() % MAX_EXTRANONCE_LEN + 1;
        let r0 = input.0 as usize;
        let r1 = input.1 as usize;
        let r0 = r0 % (extranonce_len + 1);
        let r1 = r1 % (extranonce_len + 1);
        let mut ranges = Vec::from([r0, r1]);
        ranges.sort();
        let range_0 = 0..ranges[0];
        let range_1 = ranges[0]..ranges[1];
        let range_2 = ranges[1]..extranonce_len;
        let mut extended_extranonce_start = ExtendedExtranonce {
            inner,
            range_0: range_0.clone(),
            range_1: range_1.clone(),
            range_2: range_2.clone(),
        };

        assert_eq!(extended_extranonce_start.get_len(), extranonce_len);
        assert_eq!(
            extended_extranonce_start.get_range2_len(),
            extranonce_len - ranges[1]
        );

        let extranonce = match extended_extranonce_start.next_extended(0) {
            Some(x) => x,
            None => return true,
        };

        let extended_extranonce_final = ExtendedExtranonce::from_upstream_extranonce(
            extranonce,
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
        );
        match extended_extranonce_final {
            Some(extended_extranonce_final) => {
                for b in extended_extranonce_final.inner[range_2.start..range_2.end].iter() {
                    if b != &0 {
                        return false;
                    }
                }
                extended_extranonce_final.inner[range_0.clone().start..range_1.end]
                    == extended_extranonce_start.inner[range_0.start..range_1.end]
            }
            None => {
                for b in inner[range_1.start..range_2.end].iter() {
                    if b != &0 {
                        return true;
                    }
                }
                return false;
            }
        }
    }
    // test next_standard_method
    #[quickcheck_macros::quickcheck]
    fn test_next_standard_extranonce(input: (u8, u8, Vec<u8>, usize)) -> bool {
        let inner = from_arbitrary_vec_to_array(input.2.clone());
        let extranonce_len = input.3.clone() % MAX_EXTRANONCE_LEN + 1;
        let r0 = input.0 as usize;
        let r1 = input.1 as usize;
        let r0 = r0 % (extranonce_len + 1);
        let r1 = r1 % (extranonce_len + 1);
        let mut ranges = Vec::from([r0, r1]);
        ranges.sort();
        let range_0 = 0..ranges[0];
        let range_1 = ranges[0]..ranges[1];
        let range_2 = ranges[1]..extranonce_len;
        let extended_extranonce_start = ExtendedExtranonce {
            inner,
            range_0: range_0.clone(),
            range_1: range_1.clone(),
            range_2: range_2.clone(),
        };
        let mut extranonce_copy: Extranonce =
            Extranonce::from(&mut extended_extranonce_start.clone());
        let extranonce_expected_b032: Option<B032> = extranonce_copy.next();
        match extended_extranonce_start.clone().next_standard() {
            Some(extranonce_next) => match extranonce_expected_b032 {
                Some(b032) =>
                // the range_2 of extranonce_next must be equal to the range_2 of the
                // conversion of extranonce_copy.next() converted in extranonce
                {
                    extranonce_next.extranonce[range_2.start..range_2.end] == Extranonce::from(b032.clone()).extranonce[range_2.start..range_2.end]
                    // the range_1 of the conversion of extranonce_copy.next() converted in
                    // extranonce must remain unchanged
                    && Extranonce::from(b032.clone()).extranonce[range_1.start..range_1.end]== extended_extranonce_start.inner[range_1.start..range_1.end]
                    // the range_1 if extranonce_next is set to zero by the method .next_standard()
                    && extranonce_next.extranonce[range_1.start..range_1.end]==vec![0 as u8; range_1.len()]
                }
                None => false,
            },
            // if .next_standard() method falls in None case, this means that the range_2 is at maximum
            // value, so every entry must be 255 as u8
            None => {
                for b in inner[range_2.start..range_2.end].iter() {
                    if b != &255_u8 {
                        return false;
                    }
                }
                true
            }
        }
    }
    #[quickcheck_macros::quickcheck]
    fn test_next_stndard2(input: (u8, u8, Vec<u8>, usize)) -> bool {
        let inner = from_arbitrary_vec_to_array(input.2.clone());
        let extranonce_len = input.3.clone() % MAX_EXTRANONCE_LEN + 1;
        let r0 = input.0 as usize;
        let r1 = input.1 as usize;
        let r0 = r0 % (extranonce_len + 1);
        let r1 = r1 % (extranonce_len + 1);
        let mut ranges = Vec::from([r0, r1]);
        ranges.sort();
        let range_0 = 0..ranges[0];
        let range_1 = ranges[0]..ranges[1];
        let range_2 = ranges[1]..extranonce_len;
        let mut extended_extranonce_start = ExtendedExtranonce {
            inner,
            range_0: range_0.clone(),
            range_1: range_1.clone(),
            range_2: range_2.clone(),
        };
        match extended_extranonce_start.next_standard() {
            Some(v) => {
                extended_extranonce_start.inner[range_2.clone()] == v.extranonce[range_2]
                    && extended_extranonce_start.inner[range_0.clone()]
                        == v.extranonce[range_0.clone()]
                    && v.extranonce[range_1.clone()] == vec![0; range_1.end - range_1.start]
            }
            None => true,
        }
    }

    #[quickcheck_macros::quickcheck]
    fn test_next_extended_extranonce(input: (u8, u8, Vec<u8>, usize, usize)) -> bool {
        let inner = from_arbitrary_vec_to_array(input.2.clone());
        let extranonce_len = input.3.clone() % MAX_EXTRANONCE_LEN + 1;
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
        let mut extended_extranonce = ExtendedExtranonce {
            inner,
            range_0: range_0.clone(),
            range_1: range_1.clone(),
            range_2: range_2.clone(),
        };
        match extended_extranonce.next_extended(required_len) {
            Some(extranonce) => {
                extended_extranonce.inner[..range_1.end] == extranonce.extranonce[..]
            }
            None => {
                if required_len > range_2.end - range_2.start {
                    return true;
                };
                let mut range_1_start = inner[range_1.clone()].to_vec();
                increment_bytes_be(&mut range_1_start).is_err()
            }
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
    fn test_extranonce_to_prefix() {
        let inner = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        let extranone = Extranonce { extranonce: inner };
        let prefix = extranone.into_prefix(4).unwrap();
        assert!(vec![1, 2, 3, 4] == prefix.to_vec())
    }

    #[test]
    fn test_extranonce_to_prefix_not_greater_than_inner() {
        let inner = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        let extranone = Extranonce { extranonce: inner };
        let prefix = extranone.into_prefix(20);
        assert!(prefix.is_none())
    }

    #[test]
    fn test_extended_extranonce_get_prefix_len() {
        let range_0 = 0..2;
        let range_1 = 2..4;
        let range_2 = 4..9;
        let extended = ExtendedExtranonce::new(range_0, range_1, range_2);
        let prefix_len = extended.get_prefix_len();
        assert!(prefix_len == 4);
    }
}
