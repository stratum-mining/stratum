//! The routing is used by the handler to select am Downstream/Upstream to which realay or send a
//! message
//! TODO It seems like a good idea to hide all the traits to the user and export marker traits
//! check if possible
//!
//! CommonRouter -> implemented by routers used by the common (sub)protocol
//!
//! MiningRouter -> implemented by routers used by the mining (sub)protocol
//!
//! CommonRoutingLogic -> enum that define the enum the various routing logic for the common
//!     (sub)protocol (eg Proxy None ...).
//!
//! MiningProxyRoutingLogic -> enum that define the enum the various routing logic for the common
//!     (sub)protocol (eg Proxy None ...).
//!
//! NoRouting -> implement both CommonRouter and MiningRouter used when the routing logic needed is
//!     only None
//!
//! MiningProxyRoutingLogic -> routing logic valid for a standard Sv2 mining proxy it is both a
//!     CommonRouter and a MiningRouter
//!
use crate::{
    common_properties::{CommonDownstreamData, IsMiningDownstream, IsMiningUpstream, PairSettings},
    errors::Error,
    selectors::{
        DownstreamMiningSelector, GeneralMiningSelector, NullDownstreamMiningSelector,
        UpstreamMiningSelctor,
    },
    utils::{Id, Mutex},
};
use common_messages_sv2::{
    has_requires_std_job, Protocol, SetupConnection, SetupConnectionSuccess,
};
use mining_sv2::{OpenStandardMiningChannel, OpenStandardMiningChannelSuccess};
use std::{collections::HashMap, fmt::Debug as D, marker::PhantomData, sync::Arc};

/// CommonRouter trait it define a router needed by
/// crate::handlers::common::ParseUpstreamCommonMessages and
/// crate::handlers::common::ParseDownstreamCommonMessages
pub trait CommonRouter {
    fn on_setup_connection(
        &mut self,
        message: &SetupConnection,
    ) -> Result<(CommonDownstreamData, SetupConnectionSuccess), Error>;
}

/// MiningRouter trait it define a router needed by
/// crate::handlers::mining::ParseDownstreamMiningMessages and
/// crate::handlers::mining::ParseUpstreamMiningMessages
pub trait MiningRouter<
    Down: IsMiningDownstream,
    Up: IsMiningUpstream<Down, Sel>,
    Sel: DownstreamMiningSelector<Down>,
>: CommonRouter
{
    fn update_id_downstream(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        downstream_mining_data: &CommonDownstreamData,
    ) -> u32;

    fn update_id_upstream(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        upstream_mutex: Arc<Mutex<Up>>,
    ) -> u32;

    #[allow(clippy::result_unit_err)]
    fn on_open_standard_channel(
        &mut self,
        downstream: Arc<Mutex<Down>>,
        request: &OpenStandardMiningChannel,
    ) -> Result<Arc<Mutex<Up>>, ()>;

    #[allow(clippy::result_unit_err)]
    fn on_open_standard_channel_success(
        &mut self,
        upstream: Arc<Mutex<Up>>,
        upstream_request_id: u32,
        request: &OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, ()>;
}

/// NoRoutiung Router used when RoutingLogic::None and MiningRoutingLogic::None are needed
/// It implementnt both CommonRouter and MiningRouter. It panic if used as an actual router the
/// only pourpose of NoRouting is a marker trait for when RoutingLogic::None and MiningRoutingLogic::None
#[derive(Debug)]
pub struct NoRouting();

impl CommonRouter for NoRouting {
    fn on_setup_connection(
        &mut self,
        _: &SetupConnection,
    ) -> Result<(CommonDownstreamData, SetupConnectionSuccess), Error> {
        unreachable!()
    }
}
impl<
        Down: IsMiningDownstream + D,
        Up: IsMiningUpstream<Down, NullDownstreamMiningSelector> + D,
    > MiningRouter<Down, Up, NullDownstreamMiningSelector> for NoRouting
{
    fn update_id_downstream(
        &mut self,
        _message_type: u8,
        _payload: &mut [u8],
        _downstream_mining_data: &CommonDownstreamData,
    ) -> u32 {
        unreachable!()
    }

    fn update_id_upstream(
        &mut self,
        _message_type: u8,
        _payload: &mut [u8],
        _upstream_mutex: Arc<Mutex<Up>>,
    ) -> u32 {
        unreachable!()
    }

    fn on_open_standard_channel(
        &mut self,
        _downstream: Arc<Mutex<Down>>,
        _request: &OpenStandardMiningChannel,
    ) -> Result<Arc<Mutex<Up>>, ()> {
        unreachable!()
    }

    fn on_open_standard_channel_success(
        &mut self,
        _upstream: Arc<Mutex<Up>>,
        _upstream_request_id: u32,
        _request: &OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, ()> {
        unreachable!()
    }
}

/// Enum that contains the possibles routing logic is usually contructed before calling
/// handle_message_..()
#[derive(Debug)]
pub enum CommonRoutingLogic<Router: CommonRouter> {
    Proxy(Arc<Mutex<Router>>),
    None,
}

/// Enum that contains the possibles routing logic is usually contructed before calling
/// handle_message_..()
#[derive(Debug)]
pub enum MiningRoutingLogic<
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
    Router: MiningRouter<Down, Up, Sel>,
> {
    Proxy(Arc<Mutex<Router>>),
    None,
    _P(PhantomData<(Down, Up, Sel)>),
}

impl<Router: CommonRouter> Clone for CommonRoutingLogic<Router> {
    fn clone(&self) -> Self {
        match self {
            Self::None => Self::None,
            Self::Proxy(x) => Self::Proxy(x.clone()),
        }
    }
}

impl<
        Down: IsMiningDownstream + D,
        Up: IsMiningUpstream<Down, Sel> + D,
        Sel: DownstreamMiningSelector<Down> + D,
        Router: MiningRouter<Down, Up, Sel>,
    > Clone for MiningRoutingLogic<Down, Up, Sel, Router>
{
    fn clone(&self) -> Self {
        match self {
            Self::None => Self::None,
            Self::Proxy(x) => Self::Proxy(x.clone()),
            Self::_P(_) => panic!(),
        }
    }
}

impl<
        Down: IsMiningDownstream + D,
        Up: IsMiningUpstream<Down, Sel> + D,
        Sel: DownstreamMiningSelector<Down> + D,
    > CommonRouter for MiningProxyRoutingLogic<Down, Up, Sel>
{
    fn on_setup_connection(
        &mut self,
        message: &SetupConnection,
    ) -> Result<(CommonDownstreamData, SetupConnectionSuccess), Error> {
        let protocol = message.protocol;
        let min_v = message.min_version;
        let max_v = message.max_version;
        let flags = message.flags;
        let pair_settings = PairSettings {
            protocol,
            min_v,
            max_v,
            flags,
        };
        let header_only = has_requires_std_job(pair_settings.flags);
        match (protocol, header_only) {
            (Protocol::MiningProtocol, true) => {
                self.on_setup_connection_mining_header_only(&pair_settings)
            }
            _ => panic!(),
        }
    }
}

impl<
        Down: IsMiningDownstream + D,
        Up: IsMiningUpstream<Down, Sel> + D,
        Sel: DownstreamMiningSelector<Down> + D,
    > MiningRouter<Down, Up, Sel> for MiningProxyRoutingLogic<Down, Up, Sel>
{
    /// Update the request id from downstream to a connection-wide unique request id for
    /// downstream.
    /// This method check message type and for the message type that do have a request it update
    /// it.
    ///
    /// it return the original request id
    ///
    /// TODO remove this method and update request id after that the payload has been parsed see
    /// the todo in update_request_id()
    fn update_id_downstream(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        downstream_mining_data: &CommonDownstreamData,
    ) -> u32 {
        let upstreams = self
            .downstream_to_upstream_map
            .get(downstream_mining_data)
            .unwrap();
        // TODO the upstream selection logic should be specified by the caller
        let upstream = Self::select_upstreams(&mut upstreams.to_vec());
        let old_id = get_request_id(payload);
        upstream
            .safe_lock(|u| {
                let id_map = u.get_mapper();
                match message_type {
                    // REQUESTS
                    const_sv2::MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL => {
                        let new_req_id = id_map.unwrap().on_open_channel(old_id);
                        update_request_id(payload, new_req_id);
                    }
                    const_sv2::MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL => {
                        let new_req_id = id_map.unwrap().on_open_channel(old_id);
                        update_request_id(payload, new_req_id);
                    }
                    const_sv2::MESSAGE_TYPE_SET_CUSTOM_MINING_JOB => {
                        todo!()
                    }
                    _ => (),
                }
            })
            .unwrap();
        old_id
    }

    /// TODO as above
    fn update_id_upstream(
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
                        let downstream_id = id_map.unwrap().remove(upstream_id);
                        update_request_id(payload, downstream_id);
                        upstream_id
                    }
                    const_sv2::MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES => {
                        let upstream_id = get_request_id(payload);
                        let downstream_id = id_map.unwrap().remove(upstream_id);
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
        request: &OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, ()> {
        let downstreams = upstream
            .safe_lock(|u| {
                let selector = u.get_remote_selector();
                selector.on_open_standard_channel_success(
                    upstream_request_id,
                    request.group_channel_id,
                    request.channel_id,
                )
            })
            .unwrap();
        Ok(downstreams)
    }

    /// At this point the Sv2 connection with downstream is initialized that means that
    /// routing_logic has already preselected a set of upstreams pairable with downstream.
    fn on_open_standard_channel(
        &mut self,
        downstream: Arc<Mutex<Down>>,
        request: &OpenStandardMiningChannel,
    ) -> Result<Arc<Mutex<Up>>, ()> {
        self.on_open_standard_channel_request_header_only(downstream, request)
    }
}

/// Routing logic valid for a standard Sv2 proxy
#[derive(Debug)]
pub struct MiningProxyRoutingLogic<
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
> {
    pub upstream_selector: GeneralMiningSelector<Sel, Down, Up>,
    pub downstream_id_generator: Id,
    pub downstream_to_upstream_map: HashMap<CommonDownstreamData, Vec<Arc<Mutex<Up>>>>,
    //pub upstream_startegy: MiningUpstreamSelectionStrategy<Up,Down,Sel>,
}

//enum MiningUpstreamSelectionStrategy<Up: IsMiningUpstream<Down,Sel>, Down: IsMiningDownstream, Sel: DownstreamMiningSelector<Down>> {
//    FirstOne,
//    MinHashRate,
//    Costum(dyn Fn(Vec<Arc<Mutex<Up>>>) -> Arc<Mutex<Up>>),
//
//}
//
//impl<Up: IsMiningUpstream<Down,Sel>, Down: IsMiningDownstream, Sel: DownstreamMiningSelector<Down>> MiningUpstreamSelectionStrategy<Up, Down, Sel> {
//    pub fn chose(ups: Vec<Arc<Mutex<Up>>>) -> Arc<Mutex<Up>> {
//        todo!()
//    }
//}
fn minor_total_hr_upstream<Down, Up, Sel>(ups: &mut Vec<Arc<Mutex<Up>>>) -> Arc<Mutex<Up>>
where
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
{
    ups.iter_mut()
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
        .clone()
}

fn filter_header_only<Down, Up, Sel>(ups: &mut Vec<Arc<Mutex<Up>>>) -> Vec<Arc<Mutex<Up>>>
where
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
{
    ups.iter()
        .filter(|up_mutex| up_mutex.safe_lock(|up| !up.is_header_only()).unwrap())
        .cloned()
        .collect()
}

/// If only one upstream is avaiable return it
/// Try to return an upstream that is not header only
/// Return the upstream that have got less hash rate from downstreams
fn select_upstream<Down, Up, Sel>(ups: &mut Vec<Arc<Mutex<Up>>>) -> Arc<Mutex<Up>>
where
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
{
    if ups.len() == 1 {
        ups[0].clone()
    } else if !filter_header_only(ups).is_empty() {
        minor_total_hr_upstream(&mut filter_header_only(ups))
    } else {
        minor_total_hr_upstream(ups)
    }
}

impl<
        Down: IsMiningDownstream + D,
        Up: IsMiningUpstream<Down, Sel> + D,
        Sel: DownstreamMiningSelector<Down> + D,
    > MiningProxyRoutingLogic<Down, Up, Sel>
{
    /// TODO this should stay in a enum UpstreamSelectionLogic that get passed from the caller to
    /// the several methods
    fn select_upstreams(ups: &mut Vec<Arc<Mutex<Up>>>) -> Arc<Mutex<Up>> {
        select_upstream(ups)
    }

    /// On setup conection the proxy find all the upstreams that support the downstream connection
    /// create a downstream message parser that points to all the possible upstreams and then respond
    /// with suppported flags.
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
    ) -> Result<(CommonDownstreamData, SetupConnectionSuccess), Error> {
        let mut upstreams = self.upstream_selector.on_setup_connection(pair_settings)?;
        // TODO the upstream selection logic should be specified by the caller
        let upstream = Self::select_upstreams(&mut upstreams.0);
        let downstream_data = CommonDownstreamData {
            header_only: true,
            work_selection: false,
            version_rolling: false,
        };
        let message = SetupConnectionSuccess {
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
        request: &OpenStandardMiningChannel,
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
}

//pub type NoRoutingLogic<Down, Up> = RoutingLogic<Down, Up, NullDownstreamMiningSelector>;
//impl<Down: IsMiningDownstream + D, Up: IsMiningUpstream<Down, NullDownstreamMiningSelector> + D> NoRoutingLogic<Down, Up> {
//    pub fn new() -> Self
//    where
//        Self: D,
//    {
//        RoutingLogic::None
//    }
//}
//
//impl<Down: IsMiningDownstream + D, Up: IsMiningUpstream<Down, NullDownstreamMiningSelector> + D> Default
//    for NoRoutingLogic<Down, Up>
//{
//    fn default() -> Self {
//        Self::new()
//    }
//}

/// WARNING this function assume that request id are the first 2 bytes of the
/// payload
///
/// this function should probably stay somewhere in the binary-sv2 crate the problem here is that
/// payload is moved to the message created from payload, messages created from payload can be
/// mutaed but should bot cause changing a value in the created message is not going to replace the
/// payload bytes and so to use the updated payload eg to realy the message, the message should be
/// serialized again and the payload can not be used. For that when necessary the message should
/// export a method that change a value both in the payload and in the message. Then
/// ProxyRoutingLogic::update_id can be removed and the id will be updated after that payload has
/// been parsed. TODO make that in a github issue
fn update_request_id(payload: &mut [u8], id: u32) {
    let bytes = id.to_le_bytes();
    payload[0] = bytes[0];
    payload[1] = bytes[1];
    payload[2] = bytes[2];
    payload[3] = bytes[3];
}

/// WARNING this function assume that request id are the first 2 bytes of the
/// payload
/// TODO this function should probably stay in another crate
fn get_request_id(payload: &mut [u8]) -> u32 {
    let bytes = [payload[0], payload[1], payload[2], payload[3]];
    u32::from_le_bytes(bytes)
}
