//! This module provides selectors and routing logic for managing downstream and upstream nodes
//! in a mining proxy environment. Selectors help determine the appropriate remote(s) to relay or
//! send messages to.
//!
//! ## Components
//!
//! - **`ProxyDownstreamMiningSelector`**: A selector for managing downstream nodes in a mining
//!   proxy, mapping requests and channel IDs to specific downstream nodes or groups.
//! - **`NullDownstreamMiningSelector`**: A no-op selector for cases where routing logic is
//!   unnecessary, commonly used in test scenarios.
//! - **`GeneralMiningSelector`**: A flexible upstream selector that matches downstream nodes with
//!   compatible upstream nodes based on pairing settings and flags.
//!
//! ## Traits
//!
//! - **`DownstreamSelector`**: Base trait for all downstream selectors.
//! - **`DownstreamMiningSelector`**: Specialized trait for selectors managing mining-specific
//!   downstream nodes.
//! - **`UpstreamSelector`**: Base trait for upstream node selectors.
//! - **`UpstreamMiningSelctor`**: Specialized trait for selectors managing upstream mining nodes.
//!
//! ## Details
//!
//! ### ProxyDownstreamMiningSelector
//! - Manages mappings for request IDs, channel IDs, and downstream groups.
//! - Provides methods to handle standard channel operations, such as opening channels and
//!   retrieving or removing downstream nodes associated with a channel.
//!
//! ### NullDownstreamMiningSelector
//! - Implements all required traits but panics if called.
//! - Useful for minimal setups or as a placeholder in tests.
//!
//! ### GeneralMiningSelector
//! - Matches downstream nodes to upstream nodes based on pairing compatibility.
//! - Tracks upstream nodes and their IDs for efficient lookups.

use crate::{
    common_properties::{IsDownstream, IsMiningDownstream, IsMiningUpstream, PairSettings},
    utils::Mutex,
    Error,
};
use nohash_hasher::BuildNoHashHasher;
use std::{collections::HashMap, fmt::Debug as D, sync::Arc};

/// A selector used for routing messages to specific downstream mining nodes.
///
/// This structure maintains mappings for request IDs, channel IDs, and downstream nodes
/// to facilitate efficient message routing.
#[derive(Debug, Clone, Default)]
pub struct ProxyDownstreamMiningSelector<Down: IsDownstream> {
    // Maps request IDs to their corresponding downstream nodes.
    request_id_to_remotes: HashMap<u32, Arc<Mutex<Down>>, BuildNoHashHasher<u32>>,
    // Maps group channel IDs to a list of downstream nodes.
    channel_id_to_downstreams: HashMap<u32, Vec<Arc<Mutex<Down>>>, BuildNoHashHasher<u32>>,
    // Maps standard channel IDs to a single downstream node.
    channel_id_to_downstream: HashMap<u32, Arc<Mutex<Down>>, BuildNoHashHasher<u32>>,
}

impl<Down: IsDownstream> ProxyDownstreamMiningSelector<Down> {
    /// Creates a new `ProxyDownstreamMiningSelector`.
    ///
    /// This initializes the internal mappings with `nohash` hasher for performance.
    pub fn new() -> Self {
        Self {
            request_id_to_remotes: HashMap::with_hasher(BuildNoHashHasher::default()),
            channel_id_to_downstreams: HashMap::with_hasher(BuildNoHashHasher::default()),
            channel_id_to_downstream: HashMap::with_hasher(BuildNoHashHasher::default()),
        }
    }

    /// Creates a new `ProxyDownstreamMiningSelector` wrapped in a mutex and an `Arc`.
    ///
    /// This is useful for concurrent environments where shared ownership is needed.
    pub fn new_as_mutex() -> Arc<Mutex<Self>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Self::new()))
    }
}

impl<Down: IsMiningDownstream> ProxyDownstreamMiningSelector<Down> {
    // Removes a downstream node from all mappings.
    //
    // # Arguments
    // - `d`: The downstream node to be removed.
    fn _remove_downstream(&mut self, d: &Arc<Mutex<Down>>) {
        self.request_id_to_remotes.retain(|_, v| !Arc::ptr_eq(v, d));
        self.channel_id_to_downstream
            .retain(|_, v| !Arc::ptr_eq(v, d));
    }
}

impl<Down: IsMiningDownstream> DownstreamMiningSelector<Down>
    for ProxyDownstreamMiningSelector<Down>
{
    // Registers a request ID and its associated downstream node.
    //
    // # Arguments
    // - `request_id`: The unique request ID.
    // - `downstream`: The downstream node associated with the request.
    fn on_open_standard_channel_request(&mut self, request_id: u32, downstream: Arc<Mutex<Down>>) {
        self.request_id_to_remotes.insert(request_id, downstream);
    }

    // Finalizes the mapping of a standard channel to its downstream node.
    //
    // # Arguments
    // - `request_id`: The request ID used during the channel opening.
    // - `g_channel_id`: The group channel ID.
    // - `channel_id`: The specific standard channel ID.
    //
    // # Returns
    // - `Ok`: The downstream node associated with the request.
    // - `Err`: If the request ID is unknown.
    fn on_open_standard_channel_success(
        &mut self,
        request_id: u32,
        g_channel_id: u32,
        channel_id: u32,
    ) -> Result<Arc<Mutex<Down>>, Error> {
        let downstream = self
            .request_id_to_remotes
            .remove(&request_id)
            .ok_or(Error::UnknownRequestId(request_id))?;
        self.channel_id_to_downstream
            .insert(channel_id, downstream.clone());
        match self.channel_id_to_downstreams.get_mut(&g_channel_id) {
            None => {
                self.channel_id_to_downstreams
                    .insert(g_channel_id, vec![downstream.clone()]);
            }
            Some(x) => x.push(downstream.clone()),
        }
        Ok(downstream)
    }

    // Retrieves all downstream nodes associated with a standard/group channel ID.
    //
    // # Arguments
    // - `channel_id`: The standard/group channel ID.
    //
    // # Returns
    // - `Some`: A reference to the vector of downstream nodes.
    // - `None`: If no nodes are associated with the channel.
    fn get_downstreams_in_channel(&self, channel_id: u32) -> Option<&Vec<Arc<Mutex<Down>>>> {
        self.channel_id_to_downstreams.get(&channel_id)
    }

    // Removes all downstream nodes associated with a standard/group channel ID.
    //
    // # Arguments
    // - `channel_id`: The standard/group channel ID.
    //
    // # Returns
    // A vector of the removed downstream nodes.
    fn remove_downstreams_in_channel(&mut self, channel_id: u32) -> Vec<Arc<Mutex<Down>>> {
        let downs = self
            .channel_id_to_downstreams
            .remove(&channel_id)
            .unwrap_or_default();
        for d in &downs {
            self._remove_downstream(d);
        }
        downs
    }

    // Removes a specific downstream node from all mappings.
    //
    // # Arguments
    // - `d`: The downstream node to be removed.
    fn remove_downstream(&mut self, d: &Arc<Mutex<Down>>) {
        for dws in self.channel_id_to_downstreams.values_mut() {
            dws.retain(|node| !Arc::ptr_eq(node, d));
        }

        self._remove_downstream(d);
    }

    // Retrieves the downstream node associated with a specific standard channel ID.
    //
    // # Arguments
    // - `channel_id`: The standard channel ID.
    //
    // # Returns
    // - `Some`: The downstream node.
    // - `None`: If no node is associated with the channel.
    fn downstream_from_channel_id(&self, channel_id: u32) -> Option<Arc<Mutex<Down>>> {
        self.channel_id_to_downstream.get(&channel_id).cloned()
    }

    // Retrieves all downstream nodes currently managed by this selector.
    //
    // # Returns
    // A vector of downstream nodes.
    fn get_all_downstreams(&self) -> Vec<Arc<Mutex<Down>>> {
        self.channel_id_to_downstream.values().cloned().collect()
    }
}

impl<Down: IsMiningDownstream> DownstreamSelector<Down> for ProxyDownstreamMiningSelector<Down> {}

/// Implemented by a selector used by an upstream mining node or and upstream mining node
/// abstraction in order to find the right downstream to which a message should be sent or relayed.
pub trait DownstreamMiningSelector<Downstream: IsMiningDownstream>:
    DownstreamSelector<Downstream>
{
    /// Handles a request to open a standard channel.
    ///
    /// # Arguments
    /// - `request_id`: The ID of the request.
    /// - `downstream`: A reference to the downstream requesting the channel.
    fn on_open_standard_channel_request(
        &mut self,
        request_id: u32,
        downstream: Arc<Mutex<Downstream>>,
    );

    /// Handles a successful response to opening a standard channel.
    ///
    /// # Arguments
    /// - `request_id`: The ID of the request.
    /// - `g_channel_id`: The global channel ID.
    /// - `channel_id`: The local channel ID.
    ///
    /// # Returns
    /// - `Result<Arc<Mutex<Downstream>>, Error>`: The downstream associated with the channel or an
    ///   error.
    fn on_open_standard_channel_success(
        &mut self,
        request_id: u32,
        g_channel_id: u32,
        channel_id: u32,
    ) -> Result<Arc<Mutex<Downstream>>, Error>;

    /// Retrieves all downstream's associated with a channel ID.
    ///
    /// # Arguments
    /// - `channel_id`: The channel ID to query.
    ///
    /// # Returns
    /// - `Option<&Vec<Arc<Mutex<Downstream>>>>`: The list of downstream's or `None`.
    fn get_downstreams_in_channel(&self, channel_id: u32) -> Option<&Vec<Arc<Mutex<Downstream>>>>;

    /// Removes all downstream's associated with a channel ID.
    ///
    /// # Arguments
    /// - `channel_id`: The channel ID to remove downstream's from.
    ///
    /// # Returns
    /// - `Vec<Arc<Mutex<Downstream>>>`: The removed downstream's.
    fn remove_downstreams_in_channel(&mut self, channel_id: u32) -> Vec<Arc<Mutex<Downstream>>>;

    /// Removes a specific downstream.
    ///
    /// # Arguments
    /// - `d`: A reference to the downstream to remove.
    fn remove_downstream(&mut self, d: &Arc<Mutex<Downstream>>);

    /// Retrieves a downstream by channel ID (only for standard channels).
    ///
    /// # Arguments
    /// - `channel_id`: The channel ID to query.
    ///
    /// # Returns
    /// - `Option<Arc<Mutex<Downstream>>>`: The downstream or `None`.
    fn downstream_from_channel_id(&self, channel_id: u32) -> Option<Arc<Mutex<Downstream>>>;

    /// Retrieves all downstream's.
    ///
    /// # Returns
    /// - `Vec<Arc<Mutex<Downstream>>>`: All downstream's.
    fn get_all_downstreams(&self) -> Vec<Arc<Mutex<Downstream>>>;
}

/// A generic downstream selector.
pub trait DownstreamSelector<D: IsDownstream> {}

/// A no-op implementation of `DownstreamMiningSelector`.
///
/// This selector is primarily used for testing or minimal setups where routing logic is not needed.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullDownstreamMiningSelector();

impl NullDownstreamMiningSelector {
    /// Creates a new `NullDownstreamMiningSelector`.
    pub fn new() -> Self {
        NullDownstreamMiningSelector()
    }

    /// Creates a new `NullDownstreamMiningSelector` wrapped in a mutex and an `Arc`.
    pub fn new_as_mutex() -> Arc<Mutex<Self>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Self::new()))
    }
}

impl<Down: IsMiningDownstream + D> DownstreamMiningSelector<Down> for NullDownstreamMiningSelector {
    // Called when a standard channel open request is received.
    //
    // This method is unreachable in `NullDownstreamMiningSelector` since it is a no-op
    // implementation.
    fn on_open_standard_channel_request(
        &mut self,
        _request_id: u32,
        _downstream: Arc<Mutex<Down>>,
    ) {
        unreachable!("on_open_standard_channel_request")
    }

    // Called when a standard channel open request is successful.
    //
    // This method is unreachable in `NullDownstreamMiningSelector`.
    fn on_open_standard_channel_success(
        &mut self,
        _request_id: u32,
        _channel_id: u32,
        _channel_id_2: u32,
    ) -> Result<Arc<Mutex<Down>>, Error> {
        unreachable!("on_open_standard_channel_success")
    }

    // Retrieves the downstream'ss in a specific channel.
    //
    // This method is unreachable in `NullDownstreamMiningSelector`.
    fn get_downstreams_in_channel(&self, _channel_id: u32) -> Option<&Vec<Arc<Mutex<Down>>>> {
        unreachable!("get_downstreams_in_channel")
    }

    // Removes downstream's in a specific channel.
    //
    // This method is unreachable in `NullDownstreamMiningSelector`.
    fn remove_downstreams_in_channel(&mut self, _channel_id: u32) -> Vec<Arc<Mutex<Down>>> {
        unreachable!("remove_downstreams_in_channel")
    }

    // Removes a specific downstream node.
    //
    // This method is unreachable in `NullDownstreamMiningSelector`.
    fn remove_downstream(&mut self, _d: &Arc<Mutex<Down>>) {
        unreachable!("remove_downstream")
    }

    // Retrieves the downstream associated with a specific channel ID.
    //
    // This method is unreachable in `NullDownstreamMiningSelector`.
    fn downstream_from_channel_id(&self, _channel_id: u32) -> Option<Arc<Mutex<Down>>> {
        unreachable!("downstream_from_channel_id")
    }

    // Retrieves all downstream nodes managed by this selector.
    //
    // This method is unreachable in `NullDownstreamMiningSelector`.
    fn get_all_downstreams(&self) -> Vec<Arc<Mutex<Down>>> {
        unreachable!("get_all_downstreams")
    }
}

impl<Down: IsDownstream + D> DownstreamSelector<Down> for NullDownstreamMiningSelector {}

/// Trait for selecting upstream nodes in a mining context.
pub trait UpstreamSelector {}

/// Trait for selecting upstream mining nodes.
///
/// This trait allows pairing downstream mining nodes with upstream nodes
/// based on their settings and capabilities.
pub trait UpstreamMiningSelctor<
    Down: IsMiningDownstream,
    Up: IsMiningUpstream<Down, Sel>,
    Sel: DownstreamMiningSelector<Down>,
>: UpstreamSelector
{
    /// Handles the `SetupConnection` process.
    ///
    /// # Arguments
    /// - `pair_settings`: The settings for pairing downstream and upstream nodes.
    ///
    /// # Returns
    /// - `Ok((Vec<Arc<Mutex<Up>>>, u32))`: A vector of upstream nodes and their combined flags.
    /// - `Err`: If no upstreams are pairable.
    #[allow(clippy::type_complexity)]
    fn on_setup_connection(
        &mut self,
        pair_settings: &PairSettings,
    ) -> Result<(Vec<Arc<Mutex<Up>>>, u32), Error>;

    /// Retrieves an upstream node by its ID.
    ///
    /// # Arguments
    /// - `upstream_id`: The unique ID of the upstream node.
    ///
    /// # Returns
    /// - `Some`: The upstream node.
    /// - `None`: If no upstream is found.
    fn get_upstream(&self, upstream_id: u32) -> Option<Arc<Mutex<Up>>>;
}

/// General implementation of an upstream mining selector.
#[derive(Debug)]
pub struct GeneralMiningSelector<
    Sel: DownstreamMiningSelector<Down>,
    Down: IsMiningDownstream,
    Up: IsMiningUpstream<Down, Sel>,
> {
    /// List of upstream nodes.
    pub upstreams: Vec<Arc<Mutex<Up>>>,
    /// Mapping of upstream IDs to their respective nodes.
    pub id_to_upstream: HashMap<u32, Arc<Mutex<Up>>, BuildNoHashHasher<u32>>,
    sel: std::marker::PhantomData<Sel>,
    down: std::marker::PhantomData<Down>,
}

impl<
        Sel: DownstreamMiningSelector<Down>,
        Up: IsMiningUpstream<Down, Sel>,
        Down: IsMiningDownstream,
    > GeneralMiningSelector<Sel, Down, Up>
{
    /// Creates a new `GeneralMiningSelector`.
    ///
    /// # Arguments
    /// - `upstreams`: A vector of upstream nodes.
    pub fn new(upstreams: Vec<Arc<Mutex<Up>>>) -> Self {
        let mut id_to_upstream = HashMap::with_hasher(BuildNoHashHasher::default());
        for up in &upstreams {
            id_to_upstream.insert(up.safe_lock(|u| u.get_id()).unwrap(), up.clone());
        }
        Self {
            upstreams,
            id_to_upstream,
            sel: std::marker::PhantomData,
            down: std::marker::PhantomData,
        }
    }

    /// Updates the list of upstream nodes.
    ///
    /// # Arguments
    /// - `upstreams`: The new list of upstream nodes.
    pub fn update_upstreams(&mut self, upstreams: Vec<Arc<Mutex<Up>>>) {
        self.upstreams = upstreams;
    }
}

impl<
        Sel: DownstreamMiningSelector<Down>,
        Down: IsMiningDownstream,
        Up: IsMiningUpstream<Down, Sel>,
    > UpstreamSelector for GeneralMiningSelector<Sel, Down, Up>
{
}

impl<
        Sel: DownstreamMiningSelector<Down>,
        Down: IsMiningDownstream,
        Up: IsMiningUpstream<Down, Sel>,
    > UpstreamMiningSelctor<Down, Up, Sel> for GeneralMiningSelector<Sel, Down, Up>
{
    // Handles the `SetupConnection` process and determines the pairable upstream nodes.
    //
    // # Arguments
    // - `pair_settings`: The settings for pairing downstream and upstream nodes.
    //
    // # Returns
    // - `Ok((Vec<Arc<Mutex<Up>>>, u32))`: Pairable upstream nodes and their combined flags.
    // - `Err`: If no upstreams are pairable.
    fn on_setup_connection(
        &mut self,
        pair_settings: &PairSettings,
    ) -> Result<(Vec<Arc<Mutex<Up>>>, u32), Error> {
        let mut supported_upstreams = vec![];
        let mut supported_flags: u32 = 0;
        for node in &self.upstreams {
            let is_pairable = node
                .safe_lock(|node| node.is_pairable(pair_settings))
                .unwrap();
            if is_pairable {
                supported_flags |= node.safe_lock(|n| n.get_flags()).unwrap();
                supported_upstreams.push(node.clone());
            }
        }
        if !supported_upstreams.is_empty() {
            return Ok((supported_upstreams, supported_flags));
        }

        Err(Error::NoPairableUpstream((2, 2, 0)))
    }

    // Retrieves an upstream node by its ID.
    //
    // # Arguments
    // - `upstream_id`: The unique ID of the upstream node.
    //
    // # Returns
    // - `Some`: The upstream node.
    // - `None`: If no upstream is found.
    fn get_upstream(&self, upstream_id: u32) -> Option<Arc<Mutex<Up>>> {
        self.id_to_upstream.get(&upstream_id).cloned()
    }
}
