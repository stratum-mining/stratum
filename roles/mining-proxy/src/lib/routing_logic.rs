//! # Routing Logic
//!
//! This module contains the routing logic used by handlers to determine where a message should be
//! relayed or responded to.
//!
//! The routing logic defines a set of traits and structures to manage message routing in Stratum
//! V2. The following components are included:
//!
//! - **`CommonRouter`**: Trait implemented by routers for the common (sub)protocol.
//! - **`MiningRouter`**: Trait implemented by routers for the mining (sub)protocol.
//! - **`CommonRoutingLogic`**: Enum defining the various routing logic for the common protocol
//!   (e.g., Proxy, None).
//! - **`MiningRoutingLogic`**: Enum defining the routing logic for the mining protocol (e.g.,
//!   Proxy, None).
//! - **`NoRouting`**: Marker router that implements both `CommonRouter` and `MiningRouter` for
//!   cases where no routing logic is required.
//! - **`MiningProxyRoutingLogic`**: Routing logic valid for a standard Sv2 mining proxy,
//!   implementing both `CommonRouter` and `MiningRouter`.
//!
//! ## Future Work
//!
//! - Consider hiding all traits from the public API and exporting only marker traits.
//! - Improve upstream selection logic to be configurable by the caller.

use super::{
    downstream_mining::DownstreamMiningNode,
    selectors::{
        DownstreamMiningSelector, GeneralMiningSelector, NullDownstreamMiningSelector,
        UpstreamMiningSelctor,
    },
    upstream_mining::HasDownstreamSelector,
};
use std::{collections::HashMap, fmt::Debug as D, marker::PhantomData, sync::Arc};
use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{
        has_requires_std_job, Protocol, SetupConnection, SetupConnectionSuccess,
    },
    common_properties::{
        CommonDownstreamData, IsDownstream, IsMiningDownstream, IsMiningUpstream, IsUpstream,
        PairSettings,
    },
    mining_sv2::{OpenStandardMiningChannel, OpenStandardMiningChannelSuccess},
    utils::{Id, Mutex},
    Error,
};

/// Defines routing logic for common protocol messages.
///
/// Implemented by handlers (such as
/// [`roles_logic_sv2::handlers::common::ParseCommonMessagesFromUpstream`]
/// and [`roles_logic_sv2::handlers::common::ParseCommonMessagesFromDownstream`]) to determine the
/// behavior for common protocol routing.
pub trait CommonRouter: std::fmt::Debug {
    /// Handles a `SetupConnection` message for the common protocol.
    fn on_setup_connection(
        &mut self,
        message: &SetupConnection,
    ) -> Result<(CommonDownstreamData, SetupConnectionSuccess), Error>;
}

/// Defines routing logic for mining protocol messages.
///
/// Implemented by handlers (such as
/// [`roles_logic_sv2::handlers::mining::ParseMiningMessagesFromUpstream`]
/// and [`roles_logic_sv2::handlers::mining::ParseMiningMessagesFromDownstream`]) to determine the
/// behavior for mining protocol routing. This trait extends [`CommonRouter`] to handle
/// mining-specific routing logic.
pub trait MiningRouter<
    Down: IsMiningDownstream,
    Up: IsMiningUpstream + HasDownstreamSelector,
    Sel: DownstreamMiningSelector<Down>,
>: CommonRouter
{
    /// Handles an `OpenStandardMiningChannel` message from a downstream.
    fn on_open_standard_channel(
        &mut self,
        downstream: Arc<Mutex<Down>>,
        request: &mut OpenStandardMiningChannel,
        downstream_mining_data: &CommonDownstreamData,
    ) -> Result<Arc<Mutex<Up>>, Error>;

    /// Handles an `OpenStandardMiningChannelSuccess` message from an upstream.
    fn on_open_standard_channel_success(
        &mut self,
        upstream: &mut Up,
        request: &mut OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, Error>;
}

/// A no-operation router for scenarios where no routing logic is needed.
///
/// Implements both `CommonRouter` and `MiningRouter` but panics if invoked.
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

impl<Down: IsMiningDownstream + D, Up: IsMiningUpstream + D + HasDownstreamSelector>
    MiningRouter<Down, Up, NullDownstreamMiningSelector> for NoRouting
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
        _upstream: &mut Up,
        _request: &mut OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, Error> {
        unreachable!()
    }
}

/// Routing logic options for the common protocol.
#[derive(Debug)]
pub enum CommonRoutingLogic<Router: 'static + CommonRouter> {
    /// Proxy routing logic for the common protocol.
    Proxy(&'static Mutex<Router>),
    /// No routing logic.
    None,
}

/// Routing logic options for the mining protocol.
#[derive(Debug)]
pub enum MiningRoutingLogic<
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream + D + HasDownstreamSelector,
    Sel: DownstreamMiningSelector<Down> + D,
    Router: 'static + MiningRouter<Down, Up, Sel>,
> {
    /// Proxy routing logic for the mining protocol.
    Proxy(&'static Mutex<Router>),
    /// No routing logic.
    None,
    /// Marker for the generic parameters.
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
        Up: IsMiningUpstream + D + HasDownstreamSelector,
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

/// Routing logic for a standard Sv2 mining proxy.
#[derive(Debug)]
pub struct MiningProxyRoutingLogic<
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream + D,
    Sel: DownstreamMiningSelector<Down> + D,
> {
    /// Selector for upstream entities.
    pub upstream_selector: GeneralMiningSelector<Sel, Down, Up>,
    /// ID generator for downstream entities.
    pub downstream_id_generator: Id,
    /// Mapping from downstream to upstream entities.
    pub downstream_to_upstream_map: HashMap<CommonDownstreamData, Vec<Arc<Mutex<Up>>>>,
}

impl<
        Down: IsMiningDownstream + D,
        Up: IsMiningUpstream + D + HasDownstreamSelector,
        Sel: DownstreamMiningSelector<Down> + D,
    > CommonRouter for MiningProxyRoutingLogic<Down, Up, Sel>
{
    /// Handles the `SetupConnection` message.
    ///
    /// This method initializes the connection between a downstream and an upstream by determining
    /// the appropriate upstream based on the provided protocol, versions, and flags.
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
            // TODO: Add handler for other protocols.
            _ => Err(Error::UnimplementedProtocol),
        }
    }
}

impl<
        Up: IsMiningUpstream + D + HasDownstreamSelector,
        Sel: DownstreamMiningSelector<DownstreamMiningNode> + D,
    > MiningRouter<DownstreamMiningNode, Up, Sel>
    for MiningProxyRoutingLogic<DownstreamMiningNode, Up, Sel>
{
    // Handles the `OpenStandardMiningChannel` message.
    //
    // This method processes the request to open a standard mining channel. It selects a suitable
    // upstream, updates the request ID to ensure uniqueness, and then delegates to
    // `on_open_standard_channel_request_header_only` to finalize the process.
    fn on_open_standard_channel(
        &mut self,
        downstream: Arc<Mutex<DownstreamMiningNode>>,
        request: &mut OpenStandardMiningChannel,
        downstream_mining_data: &CommonDownstreamData,
    ) -> Result<Arc<Mutex<Up>>, Error> {
        let upstreams = self
            .downstream_to_upstream_map
            .get(downstream_mining_data)
            .ok_or(Error::NoCompatibleUpstream(*downstream_mining_data))?;
        // If we are here, a list of possible upstreams has already been selected.
        // TODO: The upstream selection logic should be specified by the caller.
        let upstream =
            Self::select_upstreams(&mut upstreams.to_vec()).ok_or(Error::NoUpstreamsConnected)?;
        let old_id = request.get_request_id_as_u32();
        let new_req_id = upstream.safe_lock(|u| u.get_mapper().unwrap().on_open_channel(old_id))?;
        request.update_id(new_req_id);
        self.on_open_standard_channel_request_header_only(downstream, request)
    }

    // Handles the `OpenStandardMiningChannelSuccess` message.
    //
    // This method processes the success message received from an upstream when a standard mining
    // channel is opened. It maps the request ID back to the original ID from the downstream and
    // updates the associated group and channel IDs in the upstream.
    fn on_open_standard_channel_success(
        &mut self,
        upstream: &mut Up,
        request: &mut OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<DownstreamMiningNode>>, Error>
    where
        Up: IsUpstream + HasDownstreamSelector,
    {
        let upstream_request_id = request.get_request_id_as_u32();
        let original_request_id = upstream
            .get_mapper()
            .ok_or(Error::RequestIdNotMapped(upstream_request_id))?
            .remove(upstream_request_id)
            .ok_or(Error::RequestIdNotMapped(upstream_request_id));

        request.update_id(original_request_id?);

        let selector = upstream.get_remote_selector();
        selector.on_open_standard_channel_success(
            upstream_request_id,
            request.group_channel_id,
            request.channel_id,
        )
    }
}

// Selects the upstream with the lowest total hash rate.
// # Panics
// This function panics if the slice is empty, as it is internally guaranteed that this function
// will only be called with non-empty vectors.
fn minor_total_hr_upstream<Up>(ups: &mut [Arc<Mutex<Up>>]) -> Arc<Mutex<Up>>
where
    Up: IsMiningUpstream + D,
{
    ups.iter_mut()
        .reduce(|acc, item| {
            // Safely locks and compares the total hash rate of each upstream.
            if acc.safe_lock(|x| x.total_hash_rate()).unwrap()
                < item.safe_lock(|x| x.total_hash_rate()).unwrap()
            {
                acc
            } else {
                item
            }
        })
        .unwrap()
        .clone() // Unwrap is safe because the function only operates on non-empty vectors.
}

// Filters upstream entities that are not configured for header-only mining.
fn filter_header_only<Up>(ups: &mut [Arc<Mutex<Up>>]) -> Vec<Arc<Mutex<Up>>>
where
    Up: IsMiningUpstream + D,
{
    ups.iter()
        .filter(|up_mutex| {
            up_mutex
                .safe_lock(|up| !up.is_header_only())
                .unwrap_or_default()
        })
        .cloned()
        .collect()
}

// Selects the most appropriate upstream entity based on specific criteria.
//
// # Criteria
// - If only one upstream is available, it is selected.
// - If multiple upstreams exist, preference is given to those not configured as header-only.
// - Among the remaining upstreams, the one with the lowest total hash rate is selected.
fn select_upstream<Up>(ups: &mut [Arc<Mutex<Up>>]) -> Option<Arc<Mutex<Up>>>
where
    Up: IsMiningUpstream + D,
{
    if ups.is_empty() {
        None
    } else if ups.len() == 1 {
        Some(ups[0].clone())
    } else if !filter_header_only::<Up>(ups).is_empty() {
        Some(minor_total_hr_upstream::<Up>(
            &mut filter_header_only::<Up>(ups),
        ))
    } else {
        Some(minor_total_hr_upstream::<Up>(ups))
    }
}

impl<
        Down: IsMiningDownstream + D,
        Up: IsMiningUpstream + D + HasDownstreamSelector,
        Sel: DownstreamMiningSelector<Down> + D,
    > MiningProxyRoutingLogic<Down, Up, Sel>
{
    // Selects an upstream entity from a list of available upstreams.
    fn select_upstreams(ups: &mut [Arc<Mutex<Up>>]) -> Option<Arc<Mutex<Up>>> {
        select_upstream::<Up>(ups)
    }

    /// Handles the `SetupConnection` process for header-only mining downstream's.
    ///
    /// This method selects compatible upstreams, assigns connection flags, and maps the
    /// downstream to the selected upstreams.
    pub fn on_setup_connection_mining_header_only(
        &mut self,
        pair_settings: &PairSettings,
    ) -> Result<(CommonDownstreamData, SetupConnectionSuccess), Error> {
        let mut upstreams = self.upstream_selector.on_setup_connection(pair_settings)?;
        let upstream =
            Self::select_upstreams(&mut upstreams.0).ok_or(Error::NoUpstreamsConnected)?;
        let downstream_data = CommonDownstreamData {
            header_only: true,
            work_selection: false,
            version_rolling: false,
        };
        let message = SetupConnectionSuccess {
            used_version: 2,
            flags: upstream.safe_lock(|u| u.get_flags())?,
        };
        self.downstream_to_upstream_map
            .insert(downstream_data, vec![upstream]);
        Ok((downstream_data, message))
    }

    /// Handles a standard channel opening request for header-only mining downstreams.
    pub fn on_open_standard_channel_request_header_only(
        &mut self,
        downstream: Arc<Mutex<DownstreamMiningNode>>,
        request: &OpenStandardMiningChannel,
    ) -> Result<Arc<Mutex<Up>>, Error> {
        let downstream_mining_data = downstream.safe_lock(|d| d.get_downstream_mining_data())?;
        let upstream = self
            .downstream_to_upstream_map
            .get(&downstream_mining_data)
            .ok_or(Error::NoCompatibleUpstream(downstream_mining_data))?[0]
            .clone();
        upstream.safe_lock(|u| {
            let selector = u.get_remote_selector();
            selector.on_open_standard_channel_request(request.request_id.as_u32(), downstream)
        })?;
        Ok(upstream)
    }
}

//pub type NoRoutingLogic<Down, Up> = RoutingLogic<Down, Up, NullDownstreamMiningSelector>;
//impl<Down: IsMiningDownstream + D, Up: IsMiningUpstream<Down, NullDownstreamMiningSelector> + D>
// NoRoutingLogic<Down, Up> {    pub fn new() -> Self
//    where
//        Self: D,
//    {
//        RoutingLogic::None
//    }
//}
//
//impl<Down: IsMiningDownstream + D, Up: IsMiningUpstream<Down, NullDownstreamMiningSelector> + D>
// Default    for NoRoutingLogic<Down, Up>
//{
//    fn default() -> Self {
//        Self::new()
//    }
//}
