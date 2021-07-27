#![no_std]

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
//! job and is able to pre-distribute it for all active channels. The only missing information to start
//! to mine on the new block is the new prevhash. This information can be provided
//! independently.Such an approach improves the efficiency of the protocol where the upstream node doesn’t
//! waste precious time immediately after a new block is found in the network.
//!
//! ### Hashing Space Distribution
//! Each mining device has to work on a unique part of the whole search space. The full search
//! space is defined in part by valid values in the following block header fields:
//! * Nonce header field (32 bits),
//! * Version header field (16 bits, as specified by BIP 320),
//! * Timestamp header field.
//!
//! The other portion of the block header that’s used to define the full search space is the Merkle
//! root hash of all transactions in the block, projected to the last variable field in the block header:
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
//! This protocol explicitly expects that upstream server software is able to manage the size of the
//! hashing space correctly for its clients and can provide new jobs quickly enough.

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
pub use new_mining_job::NewExtendedMiningJob;
pub use new_mining_job::NewMiningJob;
pub use open_channel::OpenMiningChannelError;
pub use open_channel::{OpenExtendedMiningChannel, OpenExtendedMiningChannelSuccess};
pub use open_channel::{OpenStandardMiningChannel, OpenStandardMiningChannelSuccess};
pub use reconnect::Reconnect;
pub use set_custom_mining_job::{
    SetCustomMiningJob, SetCustomMiningJobError, SetCustomMiningJobSuccess,
};
pub use set_extranonce_prefix::SetExtranoncePrefix;
pub use set_group_channel::SetGroupChannel;
pub use set_new_prev_hash::SetNewPrevHash;
pub use set_target::SetTarget;
pub use submit_shares::SubmitSharesExtended;
pub use submit_shares::SubmitSharesStandard;
pub use submit_shares::{SubmitSharesError, SubmitSharesSuccess};
pub use update_channel::{UpdateChannel, UpdateChannelError};
