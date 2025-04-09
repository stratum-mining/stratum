//! # Common Properties for Stratum V2 (Sv2) Roles
//!
//! Defines common properties, traits, and utilities for implementing upstream and downstream
//! nodes. It provides abstractions for features like connection pairing, mining job distribution,
//! and channel management. These definitions form the foundation for consistent communication and
//! behavior across Sv2 roles/applications.

use common_messages_sv2::{has_requires_std_job, Protocol, SetupConnection};
use mining_sv2::{Extranonce, Target};
use nohash_hasher::BuildNoHashHasher;
use std::collections::HashMap;

/// Defines a mining downstream node at the most basic level.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct CommonDownstreamData {
    /// Header-only mining flag.
    ///
    /// Enables the processing of standard jobs only, leaving merkle root manipulation to the
    /// upstream node.
    ///
    /// - `true`: The downstream node only processes standard jobs, relying on the upstream for
    ///   merkle root manipulation.
    /// - `false`: The downstream node handles extended jobs and merkle root manipulation.
    pub header_only: bool,

    /// Work selection flag.
    ///
    /// Enables the selection of which transactions or templates the node will work on.
    ///
    /// - `true`: The downstream node works on a custom block template, using the Job Declaration
    ///   Protocol.
    /// - `false`: The downstream node strictly follows the work provided by the upstream, based on
    ///   pre-constructed templates from the upstream (e.g. Pool).
    pub work_selection: bool,

    /// Version rolling flag.
    ///
    /// Enables rolling the block header version bits which allows for more unique hash generation
    /// on the same mining job by expanding the nonce-space. Used when other fields (e.g.
    /// `nonce` or `extranonce`) are fully exhausted.
    ///
    /// - `true`: The downstream supports version rolling and can modify specific bits in the
    ///   version field. This is useful for increasing hash rate efficiency by exploring a larger
    ///   solution space.
    /// - `false`: The downstream does not support version rolling and relies on the upstream to
    ///   provide jobs with adjusted version fields.
    pub version_rolling: bool,
}

/// Encapsulates settings for pairing upstream and downstream nodes.
///
/// Simplifies the [`SetupConnection`] configuration process by bundling the protocol, version
/// range, and flag settings.
#[derive(Debug, Copy, Clone)]
pub struct PairSettings {
    /// Protocol the settings apply to.
    pub protocol: Protocol,

    /// Minimum protocol version the node supports.
    pub min_v: u16,

    /// Minimum protocol version the node supports.
    pub max_v: u16,

    /// Flags indicating optional protocol features the node supports (e.g. header-only mining,
    /// work selection, version-rolling, etc.). Each protocol field as its own
    /// flags.
    pub flags: u32,
}

/// Properties defining behaviors common to all Sv2 upstream nodes.
pub trait IsUpstream {
    /// Returns the protocol version used by the upstream node.
    fn get_version(&self) -> u16;

    /// Returns the flags indicating the upstream node's protocol capabilities.
    fn get_flags(&self) -> u32;

    /// Lists the protocols supported by the upstream node.
    ///
    /// Used to check if the upstream supports the protocol that the downstream wants to use.
    fn get_supported_protocols(&self) -> Vec<Protocol>;

    /// Verifies if the upstream can pair with the given downstream settings.
    fn is_pairable(&self, pair_settings: &PairSettings) -> bool {
        let protocol = pair_settings.protocol;
        let min_v = pair_settings.min_v;
        let max_v = pair_settings.max_v;
        let flags = pair_settings.flags;

        let check_version = self.get_version() >= min_v && self.get_version() <= max_v;
        let check_flags = SetupConnection::check_flags(protocol, self.get_flags(), flags);
        check_version && check_flags
    }

    /// Returns the channel ID managed by the upstream node.
    fn get_id(&self) -> u32;

    /// Provides a request ID mapper for viewing and managing upstream-downstream communication.
    fn get_mapper(&mut self) -> Option<&mut RequestIdMapper>;
}

/// The types of channels that can be opened with upstream nodes.
#[derive(Debug, Clone, Copy)]
pub enum UpstreamChannel {
    /// A standard channel with a nominal hash rate.
    ///
    /// Typically used for mining devices with a direct connection to an upstream node (e.g. a pool
    /// or proxy). The hashrate is specified as a `f32` value, representing the expected
    /// computational capacity of the miner.
    Standard(f32),

    /// A grouped channel for aggregated mining.
    ///
    /// Aggregates mining work for multiple standard channels under a single group channel,
    /// enabling the upstream to manage work distribution and result aggregation for an entire
    /// group of channels.
    ///
    /// Typically used by a mining proxy managing multiple downstream miners.
    Group,

    /// An extended channel for additional features.
    ///
    /// Provides additional features or capabilities beyond standard and group channels,
    /// accommodating features like custom job templates or experimental protocol extensions.
    Extended,
}

/// Properties of a standard mining channel.
///
/// Standard channels are intended to be used by end mining devices with a nominal hashrate, where
/// each device operates on an independent channel to its upstream.
#[derive(Debug, Clone)]
pub struct StandardChannel {
    /// Identifies a specific channel, unique to each mining connection.
    ///
    /// Dynamically assigned when a mining connection is established (as part of the negotiation
    /// process) to avoid conflicts with other connections (e.g. other mining devices) managed by
    /// the same upstream node. The identifier remains stable for the whole lifetime of the
    /// connection.
    ///
    /// Used for broadcasting new jobs by [`mining_sv2::NewMiningJob`].
    pub channel_id: u32,

    /// Identifies a specific group in which the standard channel belongs.
    pub group_id: u32,

    /// Initial difficulty target assigned to the mining.
    pub target: Target,

    /// Additional nonce value used to differentiate shares within the same channel.
    ///
    /// Helps to avoid nonce collisions when multiple mining devices are working on the same job.
    ///
    /// The extranonce bytes are added to the coinbase to form a fully valid submission:
    /// `full coinbase = coinbase_tx_prefix + extranonce_prefix + extranonce + coinbase_tx_suffix`
    pub extranonce: Extranonce,
}

/// Properties of a Sv2-compatible mining upstream node.
///
/// This trait extends [`IsUpstream`] with additional functionality specific to mining, such as
/// hashrate management and channel updates.
pub trait IsMiningUpstream: IsUpstream {
    /// Returns the total hashrate managed by the upstream node.
    fn total_hash_rate(&self) -> u64;

    /// Adds hash rate to the upstream node.
    fn add_hash_rate(&mut self, to_add: u64);

    /// Returns the list of open channels on the upstream node.
    fn get_opened_channels(&mut self) -> &mut Vec<UpstreamChannel>;

    /// Updates the list of channels managed by the upstream node.
    fn update_channels(&mut self, c: UpstreamChannel);

    /// Checks if the upstream node supports header-only mining.
    fn is_header_only(&self) -> bool {
        has_requires_std_job(self.get_flags())
    }
}

/// Properties defining behaviors common to all Sv2 downstream nodes.
pub trait IsDownstream {
    /// Returns the common properties of the downstream node (e.g. support for header-only mining,
    /// work selection, and version rolling).
    fn get_downstream_mining_data(&self) -> CommonDownstreamData;
}

/// Properties of a SV2-compatible mining downstream node.
///
/// This trait extends [`IsDownstream`] with additional functionality specific to mining, such as
/// header-only mining checks.
pub trait IsMiningDownstream: IsDownstream {
    /// Checks if the downstream node supports header-only mining.
    fn is_header_only(&self) -> bool {
        self.get_downstream_mining_data().header_only
    }
}

// Implemented for the `NullDownstreamMiningSelector`.
impl IsUpstream for () {
    fn get_version(&self) -> u16 {
        unreachable!("Null upstream do not have a version");
    }

    fn get_flags(&self) -> u32 {
        unreachable!("Null upstream do not have flags");
    }

    fn get_supported_protocols(&self) -> Vec<Protocol> {
        unreachable!("Null upstream do not support any protocol");
    }
    fn get_id(&self) -> u32 {
        unreachable!("Null upstream do not have an ID");
    }

    fn get_mapper(&mut self) -> Option<&mut RequestIdMapper> {
        unreachable!("Null upstream do not have a mapper")
    }
}

// Implemented for the `NullDownstreamMiningSelector`.
impl IsDownstream for () {
    fn get_downstream_mining_data(&self) -> CommonDownstreamData {
        unreachable!("Null downstream do not have mining data");
    }
}

impl IsMiningUpstream for () {
    fn total_hash_rate(&self) -> u64 {
        unreachable!("Null selector do not have hash rate");
    }

    fn add_hash_rate(&mut self, _to_add: u64) {
        unreachable!("Null selector can not add hash rate");
    }
    fn get_opened_channels(&mut self) -> &mut Vec<UpstreamChannel> {
        unreachable!("Null selector can not open channels");
    }

    fn update_channels(&mut self, _: UpstreamChannel) {
        unreachable!("Null selector can not update channels");
    }
}

impl IsMiningDownstream for () {}

/// Maps request IDs between upstream and downstream nodes.
///
/// Most commonly used by proxies, this struct facilitates communication between upstream and
/// downstream nodes by mapping request IDs. This ensures responses are routed correctly back from
/// the upstream to the originating downstream requester, even when request IDs are modified in
/// transit.
///
/// ### Workflow
/// 1. **Request Mapping**: When a downstream node sends a request, `on_open_channel` assigns a new
///    unique upstream request ID and stores the mapping.
/// 2. **Response Mapping**: When the upstream responds, the proxy uses the map to translate the
///    upstream ID back to the original downstream ID.
/// 3. **Cleanup**: Once the responses are processed, the mapping is removed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RequestIdMapper {
    // A mapping between upstream-assigned request IDs and the original downstream IDs.
    //
    // In the hashmap, the key is the upstream request ID and the value is the corresponding
    // downstream request ID. `BuildNoHashHasher` is an optimization to bypass the hashing step for
    // integer keys.
    request_ids_map: HashMap<u32, u32, BuildNoHashHasher<u32>>,

    // A counter for assigning unique request IDs to upstream nodes, incrementing after every
    // assignment.
    next_id: u32,
}

impl RequestIdMapper {
    /// Creates a new [`RequestIdMapper`] instance.
    pub fn new() -> Self {
        Self {
            request_ids_map: HashMap::with_hasher(BuildNoHashHasher::default()),
            next_id: 0,
        }
    }

    /// Assigns a new upstream request ID for a request sent by a downstream node.
    ///
    /// Ensures every request forwarded to the upstream node has a unique ID while retaining
    /// traceability to the original downstream request.
    pub fn on_open_channel(&mut self, id: u32) -> u32 {
        let new_id = self.next_id; // Assign new upstream ID
        self.next_id += 1; // Increment next_id for future requests

        self.request_ids_map.insert(new_id, id); // Map new upstream ID to downstream ID
        new_id
    }

    /// Removes the mapping for a request ID once the response has been processed.
    pub fn remove(&mut self, upstream_id: u32) -> Option<u32> {
        self.request_ids_map.remove(&upstream_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_request_id_mapper() {
        let expect = RequestIdMapper {
            request_ids_map: HashMap::with_hasher(BuildNoHashHasher::default()),
            next_id: 0,
        };
        let actual = RequestIdMapper::new();

        assert_eq!(expect, actual);
    }

    #[test]
    fn updates_request_id_mapper_on_open_channel() {
        let id = 0;
        let mut expect = RequestIdMapper {
            request_ids_map: HashMap::with_hasher(BuildNoHashHasher::default()),
            next_id: id,
        };
        let new_id = expect.next_id;
        expect.next_id += 1;
        expect.request_ids_map.insert(new_id, id);

        let mut actual = RequestIdMapper::new();
        actual.on_open_channel(0);

        assert_eq!(expect, actual);
    }

    #[test]
    fn removes_id_from_request_id_mapper() {
        let mut request_id_mapper = RequestIdMapper::new();
        request_id_mapper.on_open_channel(0);
        assert!(!request_id_mapper.request_ids_map.is_empty());

        request_id_mapper.remove(0);
        assert!(request_id_mapper.request_ids_map.is_empty());
    }
}
