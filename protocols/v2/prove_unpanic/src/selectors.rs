//! Selectors are used from the routing logic in order to chose to which remote or set of remotes
//! a message should be ralyied, or to which remote or set of remotes a message should be sent.
use crate::{
    common_properties::{IsDownstream, IsMiningDownstream, IsMiningUpstream, PairSettings},
    utils::Mutex,
    Error,
};
use nohash_hasher::BuildNoHashHasher;
use std::{collections::HashMap, fmt::Debug as D, sync::Arc};

/// A DownstreamMiningSelector useful for routing messages in a mining proxy
#[derive(Debug, Clone, Default)]
pub struct ProxyDownstreamMiningSelector<Down: IsDownstream> {
    request_id_to_remotes: HashMap<u32, Arc<Mutex<Down>>, BuildNoHashHasher<u32>>,
    channel_id_to_downstreams: HashMap<u32, Vec<Arc<Mutex<Down>>>, BuildNoHashHasher<u32>>,
    channel_id_to_downstream: HashMap<u32, Arc<Mutex<Down>>, BuildNoHashHasher<u32>>,
}

impl<Down: IsDownstream> ProxyDownstreamMiningSelector<Down> {
    pub fn new() -> Self {
        Self {
            request_id_to_remotes: HashMap::with_hasher(BuildNoHashHasher::default()),
            channel_id_to_downstreams: HashMap::with_hasher(BuildNoHashHasher::default()),
            channel_id_to_downstream: HashMap::with_hasher(BuildNoHashHasher::default()),
        }
    }
    pub fn new_as_mutex() -> Arc<Mutex<Self>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Self::new()))
    }
}

impl<Down: IsMiningDownstream> ProxyDownstreamMiningSelector<Down> {
    fn _remove_downstream(&mut self, d: &Arc<Mutex<Down>>) {
        self.request_id_to_remotes.retain(|_, v| !Arc::ptr_eq(v, d));
        self.channel_id_to_downstream
            .retain(|_, v| !Arc::ptr_eq(v, d));
    }
}

impl<Down: IsMiningDownstream> DownstreamMiningSelector<Down>
    for ProxyDownstreamMiningSelector<Down>
{
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
            dws.retain(|d| !Arc::ptr_eq(d, d));
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

/// Implemented by a selector used by an upstream mining node or and upstream mining node
/// abstraction in order to find the right downstream to which a message should be sent or relayied
pub trait DownstreamMiningSelector<Downstream: IsMiningDownstream>:
    DownstreamSelector<Downstream>
{
    fn on_open_standard_channel_request(
        &mut self,
        request_id: u32,
        downstream: Arc<Mutex<Downstream>>,
    );

    fn on_open_standard_channel_success(
        &mut self,
        request_id: u32,
        g_channel_id: u32,
        channel_id: u32,
    ) -> Result<Arc<Mutex<Downstream>>, Error>;

    // group / standard naming is terrible channel_id in this case can be  either the channel_id
    // or the group_channel_id
    fn get_downstreams_in_channel(&self, channel_id: u32) -> Option<&Vec<Arc<Mutex<Downstream>>>>;

    fn remove_downstreams_in_channel(&mut self, channel_id: u32) -> Vec<Arc<Mutex<Downstream>>>;

    fn remove_downstream(&mut self, d: &Arc<Mutex<Downstream>>);

    // only for standard
    fn downstream_from_channel_id(&self, channel_id: u32) -> Option<Arc<Mutex<Downstream>>>;

    fn get_all_downstreams(&self) -> Vec<Arc<Mutex<Downstream>>>;
}

pub trait DownstreamSelector<D: IsDownstream> {}

/// A DownstreamMiningSelector that do nothing. Useful when ParseDownstreamCommonMessages or
/// ParseUpstreamCommonMessages must be implemented in very simple application (eg for test
/// puorposes)
#[derive(Debug, Clone, Copy, Default)]
pub struct NullDownstreamMiningSelector();

impl NullDownstreamMiningSelector {
    pub fn new() -> Self {
        NullDownstreamMiningSelector()
    }
    pub fn new_as_mutex() -> Arc<Mutex<Self>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Self::new()))
    }
}

impl<Down: IsMiningDownstream + D> DownstreamMiningSelector<Down> for NullDownstreamMiningSelector {
    fn on_open_standard_channel_request(
        &mut self,
        _request_id: u32,
        _downstream: Arc<Mutex<Down>>,
    ) {
        unreachable!("on_open_standard_channel_request")
    }

    fn on_open_standard_channel_success(
        &mut self,
        _request_id: u32,
        _channel_id: u32,
        _channel_id_2: u32,
    ) -> Result<Arc<Mutex<Down>>, Error> {
        unreachable!("on_open_standard_channel_success")
    }

    fn get_downstreams_in_channel(&self, _channel_id: u32) -> Option<&Vec<Arc<Mutex<Down>>>> {
        unreachable!("get_downstreams_in_channel")
    }
    fn remove_downstreams_in_channel(&mut self, _channel_id: u32) -> Vec<Arc<Mutex<Down>>> {
        unreachable!("remove_downstreams_in_channel")
    }

    fn downstream_from_channel_id(&self, _channel_id: u32) -> Option<Arc<Mutex<Down>>> {
        unreachable!("downstream_from_channel_id")
    }
    fn get_all_downstreams(&self) -> Vec<Arc<Mutex<Down>>> {
        unreachable!("get_all_downstreams")
    }

    fn remove_downstream(&mut self, _d: &Arc<Mutex<Down>>) {
        unreachable!("remove_downstream")
    }
}

impl<Down: IsDownstream + D> DownstreamSelector<Down> for NullDownstreamMiningSelector {}

pub trait UpstreamSelector {}

pub trait UpstreamMiningSelctor<
    Down: IsMiningDownstream,
    Up: IsMiningUpstream<Down, Sel>,
    Sel: DownstreamMiningSelector<Down>,
>: UpstreamSelector
{
    #[allow(clippy::type_complexity)]
    fn on_setup_connection(
        &mut self,
        pair_settings: &PairSettings,
    ) -> Result<(Vec<Arc<Mutex<Up>>>, u32), Error>;
    fn get_upstream(&self, upstream_id: u32) -> Option<Arc<Mutex<Up>>>;
}

/// Upstream selector is used to chose between a set of known mining upstream nodes which one/ones
/// can accept messages from a specific mining downstream node
#[derive(Debug)]
pub struct GeneralMiningSelector<
    Sel: DownstreamMiningSelector<Down>,
    Down: IsMiningDownstream,
    Up: IsMiningUpstream<Down, Sel>,
> {
    pub upstreams: Vec<Arc<Mutex<Up>>>,
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
    pub fn new(upstreams: Vec<Arc<Mutex<Up>>>) -> Self {
        let mut id_to_upstream = HashMap::with_hasher(BuildNoHashHasher::default());
        for up in &upstreams {
            // Is ok to unwrap safe_lock result
            id_to_upstream.insert(up.safe_lock(|u| u.get_id()).unwrap(), up.clone());
        }
        Self {
            upstreams,
            id_to_upstream,
            sel: std::marker::PhantomData,
            down: std::marker::PhantomData,
        }
    }
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
    /// Return the set of mining upstream nodes that can accept messages from a downstream with
    /// the passed PairSettings and the sum of all the accepted flags
    #[allow(clippy::type_complexity)]
    fn on_setup_connection(
        &mut self,
        pair_settings: &PairSettings,
    ) -> Result<(Vec<Arc<Mutex<Up>>>, u32), Error> {
        let mut supported_upstreams = vec![];
        let mut supported_flags: u32 = 0;
        for node in &self.upstreams {
            let is_pairable = node
                .safe_lock(|node| node.is_pairable(pair_settings))
                // Is ok to unwrap safe_lock result
                .unwrap();
            if is_pairable {
                // Is ok to unwrap safe_lock result
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
