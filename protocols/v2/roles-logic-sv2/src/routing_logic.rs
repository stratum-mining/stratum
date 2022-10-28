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
    selectors::{
        DownstreamMiningSelector, GeneralMiningSelector, NullDownstreamMiningSelector,
        UpstreamMiningSelctor,
    },
    utils::{Id, Mutex},
    Error,
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
    #[allow(clippy::result_unit_err)]
    fn on_open_standard_channel(
        &mut self,
        downstream: Arc<Mutex<Down>>,
        request: &mut OpenStandardMiningChannel,
        downstream_mining_data: &CommonDownstreamData,
    ) -> Result<Arc<Mutex<Up>>, Error>;

    #[allow(clippy::result_unit_err)]
    fn on_open_standard_channel_success(
        &mut self,
        upstream: Arc<Mutex<Up>>,
        request: &mut OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, Error>;
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
    fn on_open_standard_channel(
        &mut self,
        _downstream: Arc<Mutex<Down>>,
        _request: &mut OpenStandardMiningChannel,
        _downstream_mining_data: &CommonDownstreamData,
    ) -> Result<Arc<Mutex<Up>>, Error> {
        unreachable!()
    }

    fn on_open_standard_channel_success(
        &mut self,
        _upstream: Arc<Mutex<Up>>,
        _request: &mut OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, Error> {
        unreachable!()
    }
}

/// Enum that contains the possibles routing logic is usually contructed before calling
/// handle_message_..()
#[derive(Debug)]
pub enum CommonRoutingLogic<Router: 'static + CommonRouter> {
    Proxy(&'static Mutex<Router>),
    None,
}

/// Enum that contains the possibles routing logic is usually contructed before calling
/// handle_message_..()
//#[derive(Debug)]
//pub enum MiningRoutingLogic<
//    Down: IsMiningDownstream + D,
//    Up: IsMiningUpstream<Down, Sel> + D,
//    Sel: DownstreamMiningSelector<Down> + D,
//    //Router: std::ops::DerefMut<Target= dyn MiningRouter<Down, Up, Sel>>,
//    Router: MiningRouter<Down, Up, Sel>,
//> {
//    Proxy(Mutex<Router>),
//    None,
//    _P(PhantomData<(Down, Up, Sel)>),
//}
#[derive(Debug)]
pub enum MiningRoutingLogic<
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
    Router: 'static + MiningRouter<Down, Up, Sel>,
> {
    Proxy(&'static Mutex<Router>),
    None,
    _P(PhantomData<(Down, Up, Sel)>),
}

impl<Router: CommonRouter> Clone for CommonRoutingLogic<Router> {
    fn clone(&self) -> Self {
        match self {
            Self::None => Self::None,
            Self::Proxy(x) => Self::Proxy(x),
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
            Self::Proxy(x) => Self::Proxy(x),
            // Variant used only for PhantomData safe to panic here
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
            // TODO add handler for other protocols
            _ => Err(Error::UnimplementedProtocol),
        }
    }
}

impl<
        Down: IsMiningDownstream + D,
        Up: IsMiningUpstream<Down, Sel> + D,
        Sel: DownstreamMiningSelector<Down> + D,
    > MiningRouter<Down, Up, Sel> for MiningProxyRoutingLogic<Down, Up, Sel>
{
    /// On open standard channel success:
    /// 1. the downstream that requested the opening of the channel must be selected an put in the
    ///    right group channel
    /// 2. request_id from upsteram is replaced with the original request id from downstream
    ///
    /// The selected downstream is returned
    fn on_open_standard_channel_success(
        &mut self,
        upstream: Arc<Mutex<Up>>,
        request: &mut OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, Error> {
        let upstream_request_id = request.get_request_id_as_u32();
        let original_request_id = upstream
            // if we are here get_mapper should always return Some(mappe) so below unwrap is ok
            .safe_lock(|u| u.get_mapper().unwrap().remove(upstream_request_id))
            // Is fine to unwrap a safe_lock result
            .unwrap()
            .ok_or(Error::RequestIdNotMapped(upstream_request_id))?;
        request.update_id(original_request_id);
        let downstreams = upstream
            .safe_lock(|u| {
                let selector = u.get_remote_selector();
                selector.on_open_standard_channel_success(
                    upstream_request_id,
                    request.group_channel_id,
                    request.channel_id,
                )
            })
            // Is fine to unwrap a safe_lock result
            .unwrap();
        downstreams
    }

    /// At this point the Sv2 connection with downstream is initialized that means that
    /// routing_logic has already preselected a set of upstreams pairable with downstream.
    ///
    /// It update the request id from downstream to a connection-wide unique request id for
    /// downstream.
    fn on_open_standard_channel(
        &mut self,
        downstream: Arc<Mutex<Down>>,
        request: &mut OpenStandardMiningChannel,
        downstream_mining_data: &CommonDownstreamData,
    ) -> Result<Arc<Mutex<Up>>, Error> {
        let upstreams = self
            .downstream_to_upstream_map
            .get(downstream_mining_data)
            // If we are here a list of possible upstream has been already selected so the below
            // unwrap can not panic
            .unwrap();
        // TODO the upstream selection logic should be specified by the caller
        let upstream =
            Self::select_upstreams(&mut upstreams.to_vec()).ok_or(Error::NoUpstreamsConnected)?;
        let old_id = request.get_request_id_as_u32();
        let new_req_id = upstream
            // if we are here get_mapper should always return Some(mappe) so below unwrap is ok
            .safe_lock(|u| u.get_mapper().unwrap().on_open_channel(old_id))
            // Is fine to unwrap a safe_lock result
            .unwrap();
        request.update_id(new_req_id);
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

fn minor_total_hr_upstream<Down, Up, Sel>(ups: &mut Vec<Arc<Mutex<Up>>>) -> Arc<Mutex<Up>>
where
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
{
    ups.iter_mut()
        .reduce(|acc, item| {
            // Is fine to unwrap a safe_lock result
            if acc.safe_lock(|x| x.total_hash_rate()).unwrap()
                < item.safe_lock(|x| x.total_hash_rate()).unwrap()
            {
                acc
            } else {
                item
            }
        })
        // Internal private function we only call thi function with non void vectors so is safe to
        // unwrap here
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
        // Is fine to unwrap a safe_lock result
        .filter(|up_mutex| up_mutex.safe_lock(|up| !up.is_header_only()).unwrap())
        .cloned()
        .collect()
}

/// If only one upstream is avaiable return it
/// Try to return an upstream that is not header only
/// Return the upstream that have got less hash rate from downstreams
fn select_upstream<Down, Up, Sel>(ups: &mut Vec<Arc<Mutex<Up>>>) -> Option<Arc<Mutex<Up>>>
where
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
{
    if ups.is_empty() {
        None
    } else if ups.len() == 1 {
        Some(ups[0].clone())
    } else if !filter_header_only(ups).is_empty() {
        Some(minor_total_hr_upstream(&mut filter_header_only(ups)))
    } else {
        Some(minor_total_hr_upstream(ups))
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
    fn select_upstreams(ups: &mut Vec<Arc<Mutex<Up>>>) -> Option<Arc<Mutex<Up>>> {
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
        let upstream =
            Self::select_upstreams(&mut upstreams.0).ok_or(Error::NoUpstreamsConnected)?;
        let downstream_data = CommonDownstreamData {
            header_only: true,
            work_selection: false,
            version_rolling: false,
        };
        let message = SetupConnectionSuccess {
            used_version: 2,
            // Is fine to unwrap a safe_lock result
            flags: upstream.safe_lock(|u| u.get_flags()).unwrap(),
        };
        self.downstream_to_upstream_map
            .insert(downstream_data, vec![upstream]);
        Ok((downstream_data, message))
    }

    /// On open standard channel request:
    /// 1. an upstream must be selected between the possibles upstreams for this downstream, if the
    ///    downstream* is header only, just one upstream will be there so the choice is easy, if not
    ///    (TODO on_open_standard_channel_request_no_standard_job must be used)
    /// 2. request_id from downstream is updated to a connection-wide uniques request-id for
    ///    upstreams
    ///
    ///    The selected upstream is returned
    ///
    ///
    ///    * The downstream that want to open a channel did already connected with the proxy so a
    ///    valid upstream has already been selected (other wise downstream can not be connected).
    ///    If the downstream is header only only one valid upstream have beem selected (cause an
    ///    header only mining device can be connected only with one pool)
    #[allow(clippy::result_unit_err)]
    pub fn on_open_standard_channel_request_header_only(
        &mut self,
        downstream: Arc<Mutex<Down>>,
        request: &OpenStandardMiningChannel,
    ) -> Result<Arc<Mutex<Up>>, Error> {
        let downstream_mining_data = downstream
            .safe_lock(|d| d.get_downstream_mining_data())
            // Is fine to unwrap a safe_lock result
            .unwrap();
        // header only downstream must map to only one upstream
        let upstream = self
            .downstream_to_upstream_map
            .get(&downstream_mining_data)
            // if we are here upstream has already been selected so is ok to unwrap here
            .unwrap()[0]
            .clone();
        #[cfg(feature = "with_serde")]
        upstream
            .safe_lock(|u| {
                let selector = u.get_remote_selector();
                selector.on_open_standard_channel_request(request.request_id, downstream)
            })
            // Is fine to unwrap a safe_lock result
            .unwrap();
        #[cfg(not(feature = "with_serde"))]
        upstream
            .safe_lock(|u| {
                let selector = u.get_remote_selector();
                selector.on_open_standard_channel_request(request.request_id.as_u32(), downstream)
            })
            // Is fine to unwrap a safe_lock result
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
