//! RequestIdMapper when present map the downstream's request_id with a newly and connection-wide
//! unique upstream's request_id
//!
//! RemoteSelector associate a channel_id and/or a request_id to a remote a remote is whatever
//! type the implementation is using to rapresents remote nodes.
//!
//! RequestIdMapper is used by proxies to change the request_id field in the message in order to:
//! 1. have connection-wide unique ids with upstream
//! 2. map the connection-wide unique id from upstream to the originale request id.
//!
//! RemoteSelector is used by proxies and TODO in order to know where messages should be realyed.
//!
//! Both RemotoSelector and RequestIdMapper in proxies are created for every upstream connection.
//! There is an 1 to 1 relation upstream connection <-> (RemotoSelector, RequestIdMapper)
//!
//! TODO
//! right now, following the above convection and using RequestIdMapper and RemotoSelector, the
//! scenario where a proxy split a downstream connection in two upstream connection is not
//! supported
use std::collections::HashMap;
use std::sync::{Arc, Mutex as Mutex_, MutexGuard, PoisonError};
pub mod common;
pub mod mining;
use crate::SetupConnection;

#[derive(Debug)]
pub struct Id {
    state: u32,
}

impl Id {
    pub fn new() -> Self {
        Self { state: 0 }
    }
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

/// SubProtocol is the Sv2 (sub)protocol that the implementor is implementing (eg: mining, common,
/// ...)
/// Remote is wathever type the implementor use to represent remote connection
pub enum SendTo_<SubProtocol, Remote> {
    Upstream(SubProtocol),
    Downstream(SubProtocol),
    Relay(Vec<Arc<Mutex<Remote>>>),
    None,
}

pub type NoRoutingLogic<Down, Up> = RoutingLogic<Down, Up, NullSelector>;

impl<Down: IsDownstream + D, Up: IsUpstream<Down, NullSelector> + D> NoRoutingLogic<Down, Up> {
    pub fn new() -> Self
    where
        Self: D,
    {
        RoutingLogic::None
    }
}

impl<Down: IsDownstream + D, Up: IsUpstream<Down, NullSelector> + D> Default
    for NoRoutingLogic<Down, Up>
{
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum RoutingLogic<
    Down: IsDownstream + D,
    Up: IsUpstream<Down, Sel> + D,
    Sel: RemoteSelector<Down> + D,
> {
    Proxy(Arc<Mutex<ProxyRoutingLogic<Down, Up, Sel>>>),
    None,
}

impl<Down: IsDownstream + D, Up: IsUpstream<Down, Sel> + D, Sel: RemoteSelector<Down> + D> Clone
    for RoutingLogic<Down, Up, Sel>
{
    fn clone(&self) -> Self {
        match self {
            Self::None => Self::None,
            Self::Proxy(x) => Self::Proxy(x.clone()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct MiningDownstreamData {
    pub id: u32,
    pub header_only: bool,
    pub work_selection: bool,
    pub version_rolling: bool,
}
use std::fmt::Debug as D;
#[derive(Debug)]
pub struct ProxyRoutingLogic<
    Down: IsDownstream + D,
    Up: IsUpstream<Down, Sel> + D,
    Sel: RemoteSelector<Down> + D,
> {
    pub upstream_selector: UpstreamSelector<Sel, Down, Up>,
    pub downstream_id_generator: Id,
    pub downstream_to_upstream_map: HashMap<MiningDownstreamData, Vec<Arc<Mutex<Up>>>>,
}

use crate::handlers::mining::{get_request_id, update_request_id};

impl<Down: IsDownstream + D, Up: IsUpstream<Down, Sel> + D, Sel: RemoteSelector<Down> + D>
    ProxyRoutingLogic<Down, Up, Sel>
{
    /// TODO this should stay in a enum UpstreamSelectionLogic that get passed from the caller to
    /// the several methods
    fn minor_total_hr_upstream(ups: Vec<Arc<Mutex<Up>>>) -> Arc<Mutex<Up>> {
        ups.into_iter()
            .reduce(|acc, item| {
                if acc.safe_lock(|x| x.total_hash_rate()).unwrap()
                    < item.safe_lock(|x| x.total_hash_rate()).unwrap()
                {
                    acc
                } else {
                    item
                }
            })
            .unwrap()
    }

    /// Update the request id from downstream to a connection-wide unique request id for
    /// downstream.
    /// This method check message type and for the message type that do have a request it update
    /// it.
    ///
    /// it return the original request id
    ///
    /// TODO remove this method and update request id after that the payload has been parsed see
    /// the todo in update_request_id()
    pub fn update_id_downstream(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        downstream_mining_data: &MiningDownstreamData,
    ) {
        let upstreams = self
            .downstream_to_upstream_map
            .get(&downstream_mining_data)
            .unwrap();
        // TODO the upstream selection logic should be specified by the caller
        let upstream = Self::minor_total_hr_upstream(upstreams.to_vec());
        upstream
            .safe_lock(|u| {
                let id_map = u.get_mapper();
                match message_type {
                    // REQUESTS
                    const_sv2::MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL => {
                        let old_id = get_request_id(payload);
                        let new_req_id = id_map.on_open_channel(old_id);
                        update_request_id(payload, new_req_id);
                    }
                    const_sv2::MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL => {
                        let old_id = get_request_id(payload);
                        let new_req_id = id_map.on_open_channel(old_id);
                        update_request_id(payload, new_req_id);
                    }
                    const_sv2::MESSAGE_TYPE_SET_CUSTOM_MINING_JOB => {
                        todo!()
                    }
                    _ => (),
                }
            })
            .unwrap();
    }

    /// TODO as above
    pub fn update_id_upstream(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        upstream_mutex: Arc<Mutex<Up>>,
    ) -> u32 {
        upstream_mutex
            .safe_lock(|u| {
                let id_map = u.get_mapper();
                match message_type {
                    const_sv2::MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS => {
                        let upstream_id = get_request_id(payload);
                        let downstream_id = id_map.remove(upstream_id);
                        update_request_id(payload, downstream_id);
                        upstream_id
                    }
                    const_sv2::MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES => {
                        let upstream_id = get_request_id(payload);
                        let downstream_id = id_map.remove(upstream_id);
                        update_request_id(payload, downstream_id);
                        upstream_id
                    }
                    const_sv2::MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR => {
                        todo!()
                    }
                    const_sv2::MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS => {
                        todo!()
                    }
                    const_sv2::MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR => {
                        todo!()
                    }
                    _ => 0,
                }
            })
            .unwrap()
    }

    /// On setup conection the proxy find all the upstreams that support the downstream connection
    /// create a downstream message parser that points to all the possible upstreams and then respond
    /// with suppported flags.
    /// If the setup connection is header only, the created downstream node must point only to one
    /// upstream.
    /// If there are no upstreams that support downstream connection TODO
    ///
    /// The upstream with min total_hash_rate is selected (TODO a method to let the caller wich
    /// upstream select from the possible ones should be added
    /// on_setup_connection_mining_header_only_2 that return a Vec of possibe upstreams)
    ///
    /// This function return downstream id that the new created downstream must return via the
    /// trait function get_id and the flags of the paired upstream
    pub fn on_setup_connection_mining_header_only(
        &mut self,
        pair_settings: &PairSettings,
    ) -> Result<(MiningDownstreamData, crate::SetupConnectionSuccess), crate::Error> {
        let upstreams = self.upstream_selector.on_setup_connection(pair_settings)?;
        // TODO the upstream selection logic should be specified by the caller
        let upstream = Self::minor_total_hr_upstream(upstreams.0);
        let id = self.downstream_id_generator.next();
        let downstream_data = MiningDownstreamData {
            id,
            header_only: false,
            work_selection: false,
            version_rolling: false,
        };
        let message = crate::SetupConnectionSuccess {
            used_version: 2,
            flags: upstream.safe_lock(|u| u.get_flags()).unwrap(),
        };
        self.downstream_to_upstream_map
            .insert(downstream_data, vec![upstream]);
        Ok((downstream_data, message))
    }

    /// On open standard channel request:
    /// 1. an upstream must be selected between the possibles upstreams for this downstream, if the
    ///    downstream is header only, just one upstream will be there so the choice is easy, if not
    ///    (TODO on_open_standard_channel_request_no_standard_job must be used)
    /// 2. request_id from downstream must be updated to a connection-wide uniques request-id for
    ///    upstream (TODO this point is done in a preavious passage by update_id but in future that
    ///    method should be removed and the id should be updated here)
    ///
    /// The selected upstream is returned
    #[allow(clippy::result_unit_err)]
    pub fn on_open_standard_channel_request_header_only(
        &mut self,
        downstream: Arc<Mutex<Down>>,
        request: &crate::OpenStandardMiningChannel,
    ) -> Result<Arc<Mutex<Up>>, ()> {
        let downstream_mining_data = downstream
            .safe_lock(|d| d.get_downstream_mining_data())
            .unwrap();
        // header only downstream must map to only one upstream
        let upstream = self
            .downstream_to_upstream_map
            .get(&downstream_mining_data)
            .unwrap()[0]
            .clone();
        upstream
            .safe_lock(|u| {
                let selector = u.get_remote_selector();
                selector.on_open_standard_channel_request(request.request_id, downstream)
            })
            .unwrap();
        Ok(upstream)
    }

    /// On open standard channel success:
    /// 1. the downstream that requested the opening of the channel must be selected an put in the
    ///    right group channel
    /// 2. request_id from upsteram must be replaced with the original request id from downstream
    ///    TODO this point is done in a preavious passage by update_id but in future that
    ///    method should be removed and the id should be updated here
    ///
    /// The selected downstream is returned
    fn on_open_standard_channel_success(
        &mut self,
        upstream: Arc<Mutex<Up>>,
        upstream_request_id: u32,
        request: &crate::OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, ()> {
        let downstreams = upstream
            .safe_lock(|u| {
                let selector = u.get_remote_selector();
                selector
                    .on_open_standard_channel_success(upstream_request_id, request.group_channel_id)
            })
            .unwrap();
        Ok(downstreams)
    }
}

impl<SubProtocol, Remote> SendTo_<SubProtocol, Remote> {
    pub fn into_message(self) -> Option<SubProtocol> {
        match self {
            Self::Upstream(t) => Some(t),
            Self::Downstream(t) => Some(t),
            Self::Relay(_) => None,
            Self::None => None,
        }
    }
    pub fn into_remote(self) -> Option<Vec<Arc<Mutex<Remote>>>> {
        match self {
            Self::Upstream(_) => None,
            Self::Downstream(_) => None,
            Self::Relay(t) => Some(t),
            Self::None => None,
        }
    }
}

/// Proxyies likely need to change the request ids of downsteam's messages. They also need to
/// remeber original id to patch the upstream's response with it
#[derive(Debug, Default)]
pub struct RequestIdMapper {
    // upstream id -> downstream id
    request_ids_map: HashMap<u32, u32>,
    next_id: u32,
}

impl RequestIdMapper {
    pub fn new() -> Self {
        Self {
            request_ids_map: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn on_open_channel(&mut self, id: u32) -> u32 {
        let new_id = self.next_id;
        self.next_id += 1;

        //let mut inner = self.request_ids_map.lock().unwrap();
        self.request_ids_map.insert(new_id, id);
        new_id
    }

    pub fn remove(&mut self, upstream_id: u32) -> u32 {
        //let mut inner = self.request_ids_map.lock().unwrap();
        self.request_ids_map.remove(&upstream_id).unwrap()
    }
}

#[derive(Debug)]
pub struct Mutex<T: ?Sized>(Mutex_<T>);

impl<T> Mutex<T> {
    pub fn safe_lock<F, Ret>(&self, thunk: F) -> Result<Ret, PoisonError<MutexGuard<'_, T>>>
    where
        F: FnOnce(&mut T) -> Ret,
    {
        let mut lock = self.0.lock()?;
        let return_value = thunk(&mut *lock);
        drop(lock);
        Ok(return_value)
    }

    pub fn new(v: T) -> Self {
        Mutex(Mutex_::new(v))
    }

    pub fn to_remove(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
}

#[derive(Debug)]
pub struct UpstreamSelector<
    Sel: RemoteSelector<Down>,
    Down: IsDownstream,
    Up: IsUpstream<Down, Sel>,
> {
    upstreams: Vec<Arc<Mutex<Up>>>,
    sel: std::marker::PhantomData<Sel>,
    down: std::marker::PhantomData<Down>,
}

impl<Sel: RemoteSelector<Down>, Up: IsUpstream<Down, Sel>, Down: IsDownstream>
    UpstreamSelector<Sel, Down, Up>
{
    #[allow(clippy::type_complexity)]
    pub fn on_setup_connection(
        &mut self,
        pair_settings: &PairSettings,
    ) -> Result<(Vec<Arc<Mutex<Up>>>, u32), crate::Error> {
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

        // TODO should return something more meaningfull
        Err(crate::Error::NoPairableUpstream((2, 2, 0)))
    }
    pub fn new(upstreams: Vec<Arc<Mutex<Up>>>) -> Self {
        Self {
            upstreams,
            sel: std::marker::PhantomData,
            down: std::marker::PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProxyRemoteSelector<Down: IsDownstream> {
    request_id_to_remotes: HashMap<u32, Arc<Mutex<Down>>>,
    group_channel_id_to_downstreams: HashMap<u32, Vec<Arc<Mutex<Down>>>>,
}

impl<Down: IsDownstream> RemoteSelector<Down> for ProxyRemoteSelector<Down> {
    fn on_open_standard_channel_request(&mut self, request_id: u32, downstream: Arc<Mutex<Down>>) {
        self.request_id_to_remotes.insert(request_id, downstream);
    }

    fn on_open_standard_channel_success(
        &mut self,
        request_id: u32,
        g_channel_id: u32,
    ) -> Arc<Mutex<Down>> {
        let downstream = self.request_id_to_remotes.remove(&request_id).unwrap();
        match self.group_channel_id_to_downstreams.get_mut(&g_channel_id) {
            None => {
                self.group_channel_id_to_downstreams
                    .insert(g_channel_id, vec![downstream.clone()]);
            }
            Some(x) => x.push(downstream.clone()),
        }
        downstream
    }

    fn get_downstreams_in_channel(&self, _channel_id: u32) -> Vec<Arc<Mutex<Down>>> {
        todo!()
    }

    fn remote_from_request_id(&mut self, _request_id: u32) -> Arc<Mutex<Down>> {
        todo!()
    }

    fn new() -> Self {
        Self {
            request_id_to_remotes: HashMap::new(),
            group_channel_id_to_downstreams: HashMap::new(),
        }
    }
}

pub trait RemoteSelector<Downstream: IsDownstream> {
    /// This get the connection-wide updated request_id
    fn on_open_standard_channel_request(
        &mut self,
        request_id: u32,
        downstream: Arc<Mutex<Downstream>>,
    );

    fn on_open_standard_channel_success(
        &mut self,
        request_id: u32,
        g_channel_id: u32,
    ) -> Arc<Mutex<Downstream>>;

    fn get_downstreams_in_channel(&self, channel_id: u32) -> Vec<Arc<Mutex<Downstream>>>;

    fn remote_from_request_id(&mut self, request_id: u32) -> Arc<Mutex<Downstream>>;

    fn new() -> Self;

    fn new_as_mutex() -> Arc<Mutex<Self>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Self::new()))
    }
}

#[derive(Debug, Copy, Clone)]
pub struct PairSettings {
    pub protocol: crate::handlers::common::Protocol,
    pub min_v: u16,
    pub max_v: u16,
    pub flags: u32,
}

pub trait IsUpstream<Down: IsDownstream, Sel: RemoteSelector<Down> + ?Sized> {
    fn get_version(&self) -> u16;
    fn get_flags(&self) -> u32;
    fn get_supported_protocols(&self) -> Vec<crate::handlers::common::Protocol>;
    fn is_pairable(&self, pair_settings: &PairSettings) -> bool {
        let protocol = pair_settings.protocol;
        let min_v = pair_settings.min_v;
        let max_v = pair_settings.max_v;
        let flags = pair_settings.flags;

        let check_version = self.get_version() >= min_v && self.get_version() <= max_v;
        let check_flags = SetupConnection::check_flags(protocol, flags, self.get_flags());
        check_version && check_flags
    }
    fn get_id(&self) -> u32;
    fn total_hash_rate(&self) -> u64;
    fn add_hash_rate(&mut self, to_add: u64);
    fn get_mapper(&mut self) -> &mut RequestIdMapper;
    fn get_remote_selector(&mut self) -> &mut Sel;
}
pub trait IsDownstream {
    fn get_downstream_mining_data(&self) -> MiningDownstreamData;
}

#[derive(Debug, Clone, Copy)]
pub struct NullSelector();

impl<Down: IsDownstream + D> IsUpstream<Down, NullSelector> for () {
    fn get_version(&self) -> u16 {
        unreachable!("0");
    }

    fn get_flags(&self) -> u32 {
        unreachable!("1");
    }

    fn get_supported_protocols(&self) -> Vec<crate::handlers::common::Protocol> {
        unreachable!("2");
    }
    fn get_id(&self) -> u32 {
        unreachable!("b");
    }

    fn total_hash_rate(&self) -> u64 {
        unreachable!("d")
    }
    fn add_hash_rate(&mut self, _to_add: u64) {
        unreachable!("e")
    }

    fn get_mapper(&mut self) -> &mut RequestIdMapper {
        todo!()
    }

    fn get_remote_selector(&mut self) -> &mut NullSelector {
        todo!()
    }
}
impl IsDownstream for () {
    fn get_downstream_mining_data(&self) -> MiningDownstreamData {
        unreachable!("c");
    }
}

impl<Down: IsDownstream + D> RemoteSelector<Down> for NullSelector {
    fn on_open_standard_channel_request(
        &mut self,
        _request_id: u32,
        _downstream: Arc<Mutex<Down>>,
    ) {
        unreachable!("5")
    }

    fn on_open_standard_channel_success(
        &mut self,
        _request_id: u32,
        _channel_id: u32,
    ) -> Arc<Mutex<Down>> {
        unreachable!("6")
    }

    fn get_downstreams_in_channel(&self, _channel_id: u32) -> Vec<Arc<Mutex<Down>>> {
        unreachable!("7")
    }

    fn remote_from_request_id(&mut self, _request_id: u32) -> Arc<Mutex<Down>> {
        unreachable!("9")
    }

    fn new() -> Self {
        unreachable!("a")
    }
}
