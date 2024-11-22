//! This module contains the routing logic used by handlers to determine where a message should be
//! relayed or responded to.
//!
//! ## Overview
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
//! ## Details
//!
//! ### Traits
//!
//! - **`CommonRouter`**: Defines routing behavior for common protocol messages.
//! - **`MiningRouter`**: Defines routing behavior for mining protocol messages. Requires
//!   `DownstreamMiningSelector` for downstream selection.
//!
//! ### Enums
//!
//! - **`CommonRoutingLogic`**: Represents possible routing logics for the common protocol, such as
//!   proxy-based routing or no routing.
//! - **`MiningRoutingLogic`**: Represents possible routing logics for the mining protocol,
//!   supporting additional parameters such as selectors and marker traits.
//!
//! ### Structures
//!
//! - **`NoRouting`**: A minimal implementation of `CommonRouter` and `MiningRouter` that panics
//!   when used. Its primary purpose is to serve as a placeholder for cases where no routing logic
//!   is applied.
//! - **`MiningProxyRoutingLogic`**: Implements routing logic for a standard Sv2 proxy, including
//!   upstream selection and message transformation.
//!
//! ## Future Work
//!
//! - Consider hiding all traits from the public API and exporting only marker traits.
//! - Improve upstream selection logic to be configurable by the caller.

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

/// Defines routing logic for common protocol messages.
///
/// Implemented by handlers (such as [`crate::handlers::common::ParseUpstreamCommonMessages`] and
/// [`crate::handlers::common::ParseDownstreamCommonMessages`]) to determine the behavior for common
/// protocol routing.
pub trait CommonRouter: std::fmt::Debug {
    /// Handles a `SetupConnection` message for the common protocol.
    ///
    /// # Arguments
    /// - `message`: The `SetupConnection` message received.
    ///
    /// # Returns
    /// - `Result<(CommonDownstreamData, SetupConnectionSuccess), Error>`: The routing result.
    fn on_setup_connection(
        &mut self,
        message: &SetupConnection,
    ) -> Result<(CommonDownstreamData, SetupConnectionSuccess), Error>;
}

/// Defines routing logic for mining protocol messages.
///
/// Implemented by handlers (such as [`crate::handlers::mining::ParseUpstreamMiningMessages`] and
/// [`crate::handlers::mining::ParseDownstreamMiningMessages`]) to determine the behavior for mining
/// protocol routing. This trait extends [`CommonRouter`] to handle mining-specific routing logic.
pub trait MiningRouter<
    Down: IsMiningDownstream,
    Up: IsMiningUpstream<Down, Sel>,
    Sel: DownstreamMiningSelector<Down>,
>: CommonRouter
{
    /// Handles an `OpenStandardMiningChannel` message from a downstream.
    ///
    /// # Arguments
    /// - `downstream`: The downstream mining entity.
    /// - `request`: The mining channel request message.
    /// - `downstream_mining_data`: Associated downstream mining data.
    ///
    /// # Returns
    /// - `Result<Arc<Mutex<Up>>, Error>`: The upstream mining entity.
    fn on_open_standard_channel(
        &mut self,
        downstream: Arc<Mutex<Down>>,
        request: &mut OpenStandardMiningChannel,
        downstream_mining_data: &CommonDownstreamData,
    ) -> Result<Arc<Mutex<Up>>, Error>;

    /// Handles an `OpenStandardMiningChannelSuccess` message from an upstream.
    ///
    /// # Arguments
    /// - `upstream`: The upstream mining entity.
    /// - `request`: The successful channel opening message.
    ///
    /// # Returns
    /// - `Result<Arc<Mutex<Down>>, Error>`: The downstream mining entity.
    fn on_open_standard_channel_success(
        &mut self,
        upstream: Arc<Mutex<Up>>,
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
    Up: IsMiningUpstream<Down, Sel> + D,
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

/// Routing logic for a standard Sv2 mining proxy.
#[derive(Debug)]
pub struct MiningProxyRoutingLogic<
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
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
        Up: IsMiningUpstream<Down, Sel> + D,
        Sel: DownstreamMiningSelector<Down> + D,
    > CommonRouter for MiningProxyRoutingLogic<Down, Up, Sel>
{
    /// Handles the `SetupConnection` message.
    ///
    /// This method initializes the connection between a downstream and an upstream by determining
    /// the appropriate upstream based on the provided protocol, versions, and flags.
    ///
    /// # Arguments
    /// - `message`: A reference to the `SetupConnection` message containing the connection details.
    ///
    /// # Returns
    /// - `Result<(CommonDownstreamData, SetupConnectionSuccess), Error>`: On success, returns the
    ///   downstream connection data and the corresponding setup success message. Returns an error
    ///   otherwise.
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
        Down: IsMiningDownstream + D,
        Up: IsMiningUpstream<Down, Sel> + D,
        Sel: DownstreamMiningSelector<Down> + D,
    > MiningRouter<Down, Up, Sel> for MiningProxyRoutingLogic<Down, Up, Sel>
{
    /// Handles the `OpenStandardMiningChannel` message.
    ///
    /// This method processes the request to open a standard mining channel. It selects a suitable
    /// upstream, updates the request ID to ensure uniqueness, and then delegates to
    /// `on_open_standard_channel_request_header_only` to finalize the process.
    ///
    /// # Arguments
    /// - `downstream`: The downstream requesting the channel opening.
    /// - `request`: A mutable reference to the `OpenStandardMiningChannel` message.
    /// - `downstream_mining_data`: Common data about the downstream mining setup.
    ///
    /// # Returns
    /// - `Result<Arc<Mutex<Up>>, Error>`: Returns the selected upstream for the downstream or an
    ///   error.
    fn on_open_standard_channel(
        &mut self,
        downstream: Arc<Mutex<Down>>,
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
        let new_req_id = upstream
            .safe_lock(|u| u.get_mapper().unwrap().on_open_channel(old_id))
            .map_err(|e| Error::PoisonLock(e.to_string()))?;
        request.update_id(new_req_id);
        self.on_open_standard_channel_request_header_only(downstream, request)
    }

    /// Handles the `OpenStandardMiningChannelSuccess` message.
    ///
    /// This method processes the success message received from an upstream when a standard mining
    /// channel is opened. It maps the request ID back to the original ID from the downstream and
    /// updates the associated group and channel IDs in the upstream.
    ///
    /// # Arguments
    /// - `upstream`: The upstream involved in the channel opening.
    /// - `request`: A mutable reference to the `OpenStandardMiningChannelSuccess` message.
    ///
    /// # Returns
    /// - `Result<Arc<Mutex<Down>>, Error>`: Returns the downstream corresponding to the request or
    ///   an error.
    fn on_open_standard_channel_success(
        &mut self,
        upstream: Arc<Mutex<Up>>,
        request: &mut OpenStandardMiningChannelSuccess,
    ) -> Result<Arc<Mutex<Down>>, Error> {
        let upstream_request_id = request.get_request_id_as_u32();
        let original_request_id = upstream
            .safe_lock(|u| {
                u.get_mapper()
                    .ok_or(crate::Error::RequestIdNotMapped(upstream_request_id))?
                    .remove(upstream_request_id)
                    .ok_or(Error::RequestIdNotMapped(upstream_request_id))
            })
            .map_err(|e| Error::PoisonLock(e.to_string()))??;

        request.update_id(original_request_id);
        upstream
            .safe_lock(|u| {
                let selector = u.get_remote_selector();
                selector.on_open_standard_channel_success(
                    upstream_request_id,
                    request.group_channel_id,
                    request.channel_id,
                )
            })
            .map_err(|e| Error::PoisonLock(e.to_string()))?
    }
}

/// Selects the upstream with the lowest total hash rate.
///
/// # Arguments
/// - `ups`: A mutable slice of upstream mining entities.
///
/// # Returns
/// - `Arc<Mutex<Up>>`: The upstream entity with the lowest total hash rate.
///
/// # Panics
/// This function panics if the slice is empty, as it is internally guaranteed that this function
/// will only be called with non-empty vectors.
fn minor_total_hr_upstream<Down, Up, Sel>(ups: &mut [Arc<Mutex<Up>>]) -> Arc<Mutex<Up>>
where
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
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

/// Filters upstream entities that are not configured for header-only mining.
///
/// # Arguments
/// - `ups`: A mutable slice of upstream mining entities.
///
/// # Returns
/// - `Vec<Arc<Mutex<Up>>>`: A vector of upstream entities that are not header-only.
fn filter_header_only<Down, Up, Sel>(ups: &mut [Arc<Mutex<Up>>]) -> Vec<Arc<Mutex<Up>>>
where
    Down: IsMiningDownstream + D,
    Up: IsMiningUpstream<Down, Sel> + D,
    Sel: DownstreamMiningSelector<Down> + D,
{
    ups.iter()
        .filter(
            |up_mutex| match up_mutex.safe_lock(|up| !up.is_header_only()) {
                Ok(header_only) => header_only,
                Err(_e) => false,
            },
        )
        .cloned()
        .collect()
}

/// Selects the most appropriate upstream entity based on specific criteria.
///
/// # Criteria
/// - If only one upstream is available, it is selected.
/// - If multiple upstreams exist, preference is given to those not configured as header-only.
/// - Among the remaining upstreams, the one with the lowest total hash rate is selected.
///
/// # Arguments
/// - `ups`: A mutable slice of upstream mining entities.
///
/// # Returns
/// - `Option<Arc<Mutex<Up>>>`: The selected upstream entity, or `None` if none are available.
fn select_upstream<Down, Up, Sel>(ups: &mut [Arc<Mutex<Up>>]) -> Option<Arc<Mutex<Up>>>
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
    /// Selects an upstream entity from a list of available upstreams.
    ///
    /// # Arguments
    /// - `ups`: A mutable slice of upstream mining entities.
    ///
    /// # Returns
    /// - `Option<Arc<Mutex<Up>>>`: The selected upstream entity, or `None` if none are available.
    fn select_upstreams(ups: &mut [Arc<Mutex<Up>>]) -> Option<Arc<Mutex<Up>>> {
        select_upstream(ups)
    }

    /// Handles the `SetupConnection` process for header-only mining downstreams.
    ///
    /// This method selects compatible upstreams, assigns connection flags, and maps the
    /// downstream to the selected upstreams.
    ///
    /// # Arguments
    /// - `pair_settings`: The pairing settings for the connection.
    ///
    /// # Returns
    /// - `Result<(CommonDownstreamData, SetupConnectionSuccess), Error>`: The connection result.
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
            flags: upstream
                .safe_lock(|u| u.get_flags())
                .map_err(|e| Error::PoisonLock(e.to_string()))?,
        };
        self.downstream_to_upstream_map
            .insert(downstream_data, vec![upstream]);
        Ok((downstream_data, message))
    }

    /// Handles a standard channel opening request for header-only mining downstreams.
    ///
    /// # Arguments
    /// - `downstream`: The downstream mining entity.
    /// - `request`: The standard mining channel request message.
    ///
    /// # Returns
    /// - `Result<Arc<Mutex<Up>>, Error>`: The selected upstream mining entity.
    pub fn on_open_standard_channel_request_header_only(
        &mut self,
        downstream: Arc<Mutex<Down>>,
        request: &OpenStandardMiningChannel,
    ) -> Result<Arc<Mutex<Up>>, Error> {
        let downstream_mining_data = downstream
            .safe_lock(|d| d.get_downstream_mining_data())
            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
        let upstream = self
            .downstream_to_upstream_map
            .get(&downstream_mining_data)
            .ok_or(crate::Error::NoCompatibleUpstream(downstream_mining_data))?[0]
            .clone();
        #[cfg(feature = "with_serde")]
        upstream
            .safe_lock(|u| {
                let selector = u.get_remote_selector();
                selector.on_open_standard_channel_request(request.request_id, downstream)
            })
            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
        #[cfg(not(feature = "with_serde"))]
        upstream
            .safe_lock(|u| {
                let selector = u.get_remote_selector();
                selector.on_open_standard_channel_request(request.request_id.as_u32(), downstream)
            })
            .map_err(|e| Error::PoisonLock(e.to_string()))?;
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
