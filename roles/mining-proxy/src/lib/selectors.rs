//! # Selectors and Message Routing
//!
//! This module provides selectors and routing logic for managing downstream and upstream nodes
//! in a mining proxy environment. Selectors help determine the appropriate remote(s) to relay or
//! send messages to.

use network_helpers_sv2::roles_logic_sv2::{
    common_properties::{IsDownstream, IsMiningDownstream, IsMiningUpstream, PairSettings},
    utils::Mutex,
    Error,
};
use nohash_hasher::BuildNoHashHasher;
use std::{collections::HashMap, fmt::Debug as D, sync::Arc};

/// Proxy selector for routing messages to downstream mining nodes.
///
/// Maintains mappings for request IDs, channel IDs, and downstream nodes to facilitate message
/// routing.
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
    /// Creates a new [`ProxyDownstreamMiningSelector`] instance.
    pub fn new() -> Self {
        // `BuildNoHashHasher` is an optimization to bypass the hashing step for integer keys
        Self {
            request_id_to_remotes: HashMap::with_hasher(BuildNoHashHasher::default()),
            channel_id_to_downstreams: HashMap::with_hasher(BuildNoHashHasher::default()),
            channel_id_to_downstream: HashMap::with_hasher(BuildNoHashHasher::default()),
        }
    }

    /// Creates a new [`ProxyDownstreamMiningSelector`] instance wrapped in an `Arc<Mutex>`.
    pub fn new_as_mutex() -> Arc<Mutex<Self>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Self::new()))
    }
}

impl<Down: IsMiningDownstream> ProxyDownstreamMiningSelector<Down> {
    // Removes a specific downstream node from all mappings.
    fn _remove_downstream(&mut self, d: &Arc<Mutex<Down>>) {
        self.request_id_to_remotes.retain(|_, v| !Arc::ptr_eq(v, d));
        self.channel_id_to_downstream
            .retain(|_, v| !Arc::ptr_eq(v, d));
    }
}

impl<Down: IsMiningDownstream> DownstreamMiningSelector<Down>
    for ProxyDownstreamMiningSelector<Down>
{
    /// Records a request to open a standard channel with an associated downstream node.
    fn on_open_standard_channel_request(&mut self, request_id: u32, downstream: Arc<Mutex<Down>>) {
        self.request_id_to_remotes.insert(request_id, downstream);
    }

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
    fn get_downstreams_in_channel(&self, channel_id: u32) -> Option<&Vec<Arc<Mutex<Down>>>> {
        self.channel_id_to_downstreams.get(&channel_id)
    }

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

    fn remove_downstream(&mut self, d: &Arc<Mutex<Down>>) {
        for dws in self.channel_id_to_downstreams.values_mut() {
            dws.retain(|node| !Arc::ptr_eq(node, d));
        }

        self._remove_downstream(d);
    }

    fn downstream_from_channel_id(&self, channel_id: u32) -> Option<Arc<Mutex<Down>>> {
        self.channel_id_to_downstream.get(&channel_id).cloned()
    }

    fn get_all_downstreams(&self) -> Vec<Arc<Mutex<Down>>> {
        self.channel_id_to_downstream.values().cloned().collect()
    }
}

impl<Down: IsMiningDownstream> DownstreamSelector<Down> for ProxyDownstreamMiningSelector<Down> {}

/// Specialized trait for selectors managing downstream mining nodes.
///
/// Logic for an upstream mining node to locate the correct downstream node to which a message
/// should be sent or relayed.
pub trait DownstreamMiningSelector<Downstream: IsMiningDownstream>:
    DownstreamSelector<Downstream>
{
    /// Handles a downstream node's request to open a standard channel.
    fn on_open_standard_channel_request(
        &mut self,
        request_id: u32,
        downstream: Arc<Mutex<Downstream>>,
    );

    /// Handles the successful opening of a standard channel with a downstream node. Returns an
    /// error if the request ID is unknown.
    fn on_open_standard_channel_success(
        &mut self,
        request_id: u32,
        g_channel_id: u32,
        channel_id: u32,
    ) -> Result<Arc<Mutex<Downstream>>, Error>;

    /// Retrieves all downstream nodes associated with a channel ID.
    fn get_downstreams_in_channel(&self, channel_id: u32) -> Option<&Vec<Arc<Mutex<Downstream>>>>;

    /// Removes all downstream nodes associated with a channel, returning all removed downstream
    /// nodes.
    fn remove_downstreams_in_channel(&mut self, channel_id: u32) -> Vec<Arc<Mutex<Downstream>>>;

    /// Removes a specific downstream.
    fn remove_downstream(&mut self, d: &Arc<Mutex<Downstream>>);

    // Retrieves the downstream node associated with a specific standard channel ID.
    //
    // Only for standard channels.
    fn downstream_from_channel_id(&self, channel_id: u32) -> Option<Arc<Mutex<Downstream>>>;

    /// Retrieves all downstream nodes managed by the selector.
    fn get_all_downstreams(&self) -> Vec<Arc<Mutex<Downstream>>>;
}

/// Base trait for selectors managing downstream nodes.
pub trait DownstreamSelector<D: IsDownstream> {}

/// No-op selector for cases where routing logic is unnecessary.
///
/// Primarily used for testing, it implements all required traits, but panics with an
/// [`unreachable`] if called.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullDownstreamMiningSelector();

impl NullDownstreamMiningSelector {
    /// Creates a new [`NullDownstreamMiningSelector`] instance.
    pub fn new() -> Self {
        NullDownstreamMiningSelector()
    }

    /// Creates a new [`NullDownstreamMiningSelector`] instance wrapped in an `Arc<Mutex>`.
    pub fn new_as_mutex() -> Arc<Mutex<Self>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Self::new()))
    }
}

impl<Down: IsMiningDownstream + D> DownstreamMiningSelector<Down> for NullDownstreamMiningSelector {
    /// [`unreachable`] in this no-op implementation.
    fn on_open_standard_channel_request(
        &mut self,
        _request_id: u32,
        _downstream: Arc<Mutex<Down>>,
    ) {
        unreachable!("on_open_standard_channel_request")
    }

    /// [`unreachable`] in this no-op implementation.
    fn on_open_standard_channel_success(
        &mut self,
        _request_id: u32,
        _channel_id: u32,
        _channel_id_2: u32,
    ) -> Result<Arc<Mutex<Down>>, Error> {
        unreachable!("on_open_standard_channel_success")
    }

    /// [`unreachable`] in this no-op implementation.
    fn get_downstreams_in_channel(&self, _channel_id: u32) -> Option<&Vec<Arc<Mutex<Down>>>> {
        unreachable!("get_downstreams_in_channel")
    }

    /// [`unreachable`] in this no-op implementation.
    fn remove_downstreams_in_channel(&mut self, _channel_id: u32) -> Vec<Arc<Mutex<Down>>> {
        unreachable!("remove_downstreams_in_channel")
    }

    /// [`unreachable`] in this no-op implementation.
    fn remove_downstream(&mut self, _d: &Arc<Mutex<Down>>) {
        unreachable!("remove_downstream")
    }

    /// [`unreachable`] in this no-op implementation.
    fn downstream_from_channel_id(&self, _channel_id: u32) -> Option<Arc<Mutex<Down>>> {
        unreachable!("downstream_from_channel_id")
    }

    /// [`unreachable`] in this no-op implementation.
    fn get_all_downstreams(&self) -> Vec<Arc<Mutex<Down>>> {
        unreachable!("get_all_downstreams")
    }
}

impl<Down: IsDownstream + D> DownstreamSelector<Down> for NullDownstreamMiningSelector {}

/// Base trait for selectors managing upstream nodes.
pub trait UpstreamSelector {}

/// Specialized trait for selectors managing upstream mining nodes.
///
/// This trait is implemented by roles with multiple upstream connections, such as proxies or
/// pools. It provides logic to route messages received by the implementing role (e.g., from mining
/// devices or downstream proxies) to the appropriate upstream nodes.
///
/// For example, a mining proxy with multiple upstream pools would implement this trait to handle
/// upstream connection setup, failover, and routing logic.
pub trait UpstreamMiningSelctor<
    Down: IsMiningDownstream,
    Up: IsMiningUpstream,
    Sel: DownstreamMiningSelector<Down>,
>: UpstreamSelector
{
    /// Handles the setup of connections to upstream nodes with the given [`PairSettings`].
    ///
    /// Returns an error if the upstream and downstream node(s) are unpairable.
    #[allow(clippy::type_complexity)]
    fn on_setup_connection(
        &mut self,
        pair_settings: &PairSettings,
    ) -> Result<(Vec<Arc<Mutex<Up>>>, u32), Error>;

    /// Retrieves an upstream node by its ID, if exists.
    fn get_upstream(&self, upstream_id: u32) -> Option<Arc<Mutex<Up>>>;
}

/// Selector for routing messages to downstream nodes based on pair settings and flags.
///
/// Tracks upstream nodes and their IDs for efficient lookups.
#[derive(Debug)]
pub struct GeneralMiningSelector<
    Sel: DownstreamMiningSelector<Down>,
    Down: IsMiningDownstream,
    Up: IsMiningUpstream,
> {
    /// List of upstream nodes.
    pub upstreams: Vec<Arc<Mutex<Up>>>,

    /// Mapping of upstream IDs to their corresponding upstream nodes.
    pub id_to_upstream: HashMap<u32, Arc<Mutex<Up>>, BuildNoHashHasher<u32>>,

    sel: std::marker::PhantomData<Sel>,

    down: std::marker::PhantomData<Down>,
}

impl<Sel: DownstreamMiningSelector<Down>, Up: IsMiningUpstream, Down: IsMiningDownstream>
    GeneralMiningSelector<Sel, Down, Up>
{
    /// Creates a new [`GeneralMiningSelector`] instance with the given upstream nodes.
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
    pub fn update_upstreams(&mut self, upstreams: Vec<Arc<Mutex<Up>>>) {
        self.upstreams = upstreams;
    }
}

impl<Sel: DownstreamMiningSelector<Down>, Down: IsMiningDownstream, Up: IsMiningUpstream>
    UpstreamSelector for GeneralMiningSelector<Sel, Down, Up>
{
}

impl<Sel: DownstreamMiningSelector<Down>, Down: IsMiningDownstream, Up: IsMiningUpstream>
    UpstreamMiningSelctor<Down, Up, Sel> for GeneralMiningSelector<Sel, Down, Up>
{
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

    fn get_upstream(&self, upstream_id: u32) -> Option<Arc<Mutex<Up>>> {
        self.id_to_upstream.get(&upstream_id).cloned()
    }
}
