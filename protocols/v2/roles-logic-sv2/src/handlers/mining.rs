//! # Mining Handlers
//!
//! This module defines traits and functions for handling mining-related messages within the Stratum
//! V2 protocol.
//!
//! ## Core Traits
//!
//! - `ParseUpstreamMiningMessages`: Implemented by downstream nodes to process mining messages
//!   received from upstream nodes. This trait provides methods for handling mining events like new
//!   mining jobs, share submissions, extranonce prefix updates, and channel status updates.
//! - `ParseDownstreamMiningMessages`: Implemented by upstream nodes to manage mining messages
//!   received from downstream nodes. This trait includes methods for managing tasks such as
//!   submitting shares, opening mining channels, and handling mining job responses.
//!
//! ## Message Handling
//!
//! Handlers in this module are responsible for:
//! - Parsing and deserializing mining-related messages into the appropriate types.
//! - Dispatching the deserialized messages to specific handler functions based on message type,
//!   such as handling new mining jobs, share submissions, and extranonce updates.
//! - Ensuring the integrity and validity of received messages, while interacting with downstream
//!   mining systems to ensure proper communication and task execution.
//!
//! ## Return Type
//!
//! Functions return `Result<SendTo<Down>, Error>`, where `SendTo<Down>` specifies the next action
//! for the message: whether it should be sent to the downstream node, an error response should be
//! generated, or the message should be ignored.
//!
//! ## Structure
//!
//! This module includes:
//! - Traits for processing mining-related messages for both upstream and downstream communication.
//! - Functions to parse, deserialize, and process messages related to mining, ensuring robust error
//!   handling for unexpected conditions.
//! - Support for managing mining channels, extranonce prefixes, and share submissions, while
//!   handling edge cases and ensuring the correctness of the mining process.

use crate::{common_properties::RequestIdMapper, errors::Error, parsers::Mining};
use core::convert::TryInto;
use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, Reconnect, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel, SetNewPrevHash, SetTarget,
    SubmitSharesError, SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess,
    UpdateChannel, UpdateChannelError,
};

use crate::{
    common_properties::{IsMiningDownstream, IsMiningUpstream},
    routing_logic::{MiningRouter, MiningRoutingLogic},
    selectors::DownstreamMiningSelector,
};

use super::SendTo_;

use crate::utils::Mutex;
use const_sv2::*;
use std::{fmt::Debug as D, sync::Arc};
use tracing::{debug, error, info, trace};

/// see [`SendTo_`]
pub type SendTo<Remote> = SendTo_<Mining<'static>, Remote>;

/// Represents supported channel types in a mining connection.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SupportedChannelTypes {
    Standard,
    Extended,
    Group,
    /// Represents a connection that supports both group and extended channels.
    GroupAndExtended,
}

/// Trait for parsing downstream mining messages in a Stratum V2 connection.
///
/// This trait defines methods for parsing and routing downstream messages
/// related to mining operations.
pub trait ParseDownstreamMiningMessages<
    Up: IsMiningUpstream<Self, Selector> + D,
    Selector: DownstreamMiningSelector<Self> + D,
    Router: MiningRouter<Self, Up, Selector>,
> where
    Self: IsMiningDownstream + Sized + D,
{
    /// Returns the type of channel supported by the downstream connection.
    fn get_channel_type(&self) -> SupportedChannelTypes;

    /// Handles a mining message from the downstream, given its type and payload.
    ///
    /// # Arguments
    /// - `self_mutex`: The `Arc<Mutex<Self>>` representing the downstream entity.
    /// - `message_type`: The type of the mining message.
    /// - `payload`: The raw payload of the message.
    /// - `routing_logic`: The logic for routing the message to the appropriate upstream entity.
    ///
    /// # Returns
    /// - `Result<SendTo<Up>, Error>`: The result of processing the message.
    fn handle_message_mining(
        self_mutex: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        routing_logic: MiningRoutingLogic<Self, Up, Selector, Router>,
    ) -> Result<SendTo<Up>, Error>
    where
        Self: IsMiningDownstream + Sized,
    {
        match Self::handle_message_mining_deserialized(
            self_mutex,
            (message_type, payload).try_into(),
            routing_logic,
        ) {
            Err(Error::UnexpectedMessage(0)) => Err(Error::UnexpectedMessage(message_type)),
            result => result,
        }
    }

    /// Deserializes and processes a mining message from the downstream.
    ///
    /// # Arguments
    /// - `self_mutex`: The `Arc<Mutex<Self>>` representing the downstream entity.
    /// - `message`: The mining message to be processed.
    /// - `routing_logic`: The logic for routing the message to the appropriate upstream entity.
    ///
    /// # Returns
    /// - `Result<SendTo<Up>, Error>`: The result of processing the message.
    fn handle_message_mining_deserialized(
        self_mutex: Arc<Mutex<Self>>,
        message: Result<Mining<'_>, Error>,
        routing_logic: MiningRoutingLogic<Self, Up, Selector, Router>,
    ) -> Result<SendTo<Up>, Error>
    where
        Self: IsMiningDownstream + Sized,
    {
        let (channel_type, is_work_selection_enabled, downstream_mining_data) = self_mutex
            .safe_lock(|self_| {
                (
                    self_.get_channel_type(),
                    self_.is_work_selection_enabled(),
                    self_.get_downstream_mining_data(),
                )
            })
            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
        match message {
            Ok(Mining::OpenStandardMiningChannel(mut m)) => {
                info!(
                    "Received OpenStandardMiningChannel from: {} with id: {}",
                    std::str::from_utf8(m.user_identity.as_ref()).unwrap_or("Unknown identity"),
                    m.get_request_id_as_u32()
                );
                debug!("OpenStandardMiningChannel: {:?}", m);
                // check user auth
                if !Self::is_downstream_authorized(self_mutex.clone(), &m.user_identity)? {
                    info!(
                        "On OpenStandardMiningChannel client not authorized: {:?}",
                        &m.user_identity
                    );
                    return Ok(SendTo::Respond(Mining::OpenMiningChannelError(
                        OpenMiningChannelError::new_unknown_user(m.get_request_id_as_u32()),
                    )));
                }
                let upstream = match routing_logic {
                    MiningRoutingLogic::None => None,
                    MiningRoutingLogic::Proxy(r_logic) => {
                        trace!("On OpenStandardMiningChannel r_logic is: {:?}", r_logic);
                        let up = r_logic
                            .safe_lock(|r_logic| {
                                r_logic.on_open_standard_channel(
                                    self_mutex.clone(),
                                    &mut m,
                                    &downstream_mining_data,
                                )
                            })
                            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
                        trace!("On OpenStandardMiningChannel best candidate is: {:?}", up);
                        Some(up?)
                    }
                    // Variant just used for phantom data is ok to panic
                    MiningRoutingLogic::_P(_) => panic!("Must use either MiningRoutingLogic::None or MiningRoutingLogic::Proxy for `routing_logic` param"),
                };
                trace!(
                    "On OpenStandardMiningChannel channel type is: {:?}",
                    channel_type
                );
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                    )),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::OpenExtendedMiningChannel(m)) => {
                info!(
                    "Received OpenExtendedMiningChannel from: {} with id: {}",
                    std::str::from_utf8(m.user_identity.as_ref()).unwrap_or("Unknown identity"),
                    m.get_request_id_as_u32()
                );
                debug!("OpenExtendedMiningChannel: {:?}", m);
                // check user auth
                if !Self::is_downstream_authorized(self_mutex.clone(), &m.user_identity)? {
                    info!(
                        "On OpenStandardMiningChannel client not authorized: {:?}",
                        &m.user_identity
                    );
                    return Ok(SendTo::Respond(Mining::OpenMiningChannelError(
                        OpenMiningChannelError::new_unknown_user(m.get_request_id_as_u32()),
                    )));
                };
                trace!(
                    "On OpenExtendedMiningChannel channel type is: {:?}",
                    channel_type
                );
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::UpdateChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    info!("Received UpdateChannel->Standard message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Extended => {
                    info!("Received UpdateChannel->Extended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Group => {
                    info!("Received UpdateChannel->Group message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    info!("Received UpdateChannel->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::SubmitSharesStandard(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    debug!("Received SubmitSharesStandard->Standard message");
                    trace!("SubmitSharesStandard {:?}", m);
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                )),
                SupportedChannelTypes::Group => {
                    debug!("Received SubmitSharesStandard->Group message");
                    trace!("SubmitSharesStandard {:?}", m);
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received SubmitSharesStandard->GroupAndExtended message");
                    trace!("SubmitSharesStandard {:?}", m);
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::SubmitSharesExtended(m)) => {
                debug!("Received SubmitSharesExtended message");
                trace!("SubmitSharesExtended {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                    )),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                    )),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::SetCustomMiningJob(m)) => {
                info!(
                    "Received SetCustomMiningJob message for channel: {}, with id: {}",
                    m.channel_id, m.request_id
                );
                debug!("SetCustomMiningJob: {:?}", m);
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    _ => Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_CUSTOM_MINING_JOB)),
                }
            }
            Ok(_) => Err(Error::UnexpectedMessage(0)),
            Err(e) => Err(e),
        }
    }

    /// Checks if work selection is enabled for the downstream connection.
    ///
    /// # Returns
    /// - `bool`: `true` if work selection is enabled, `false` otherwise.
    fn is_work_selection_enabled(&self) -> bool;

    /// Checks if the downstream user is authorized.
    ///
    /// # Arguments
    /// - `_self_mutex`: The `Arc<Mutex<Self>>` representing the downstream entity.
    /// - `_user_identity`: The user's identity to be checked.
    ///
    /// # Returns
    /// - `Result<bool, Error>`: `true` if the user is authorized, `false` otherwise.
    fn is_downstream_authorized(
        _self_mutex: Arc<Mutex<Self>>,
        _user_identity: &binary_sv2::Str0255,
    ) -> Result<bool, Error> {
        Ok(true)
    }

    /// Handles an `OpenStandardMiningChannel` message.
    ///
    /// This method processes an `OpenStandardMiningChannel` message and initiates the
    /// appropriate response.
    ///
    /// # Arguments
    /// - `m`: The `OpenStandardMiningChannel` message.
    /// - `up`: An optional upstream entity to which the message is forwarded.
    ///
    /// # Returns
    /// - `Result<SendTo<Up>, Error>`: The result of processing the message.
    fn handle_open_standard_mining_channel(
        &mut self,
        m: OpenStandardMiningChannel,
        up: Option<Arc<Mutex<Up>>>,
    ) -> Result<SendTo<Up>, Error>;

    /// Handles an `OpenExtendedMiningChannel` message.
    ///
    /// This method processes an `OpenExtendedMiningChannel` message and initiates the
    /// appropriate response.
    ///
    /// # Arguments
    /// - `m`: The `OpenExtendedMiningChannel` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Up>, Error>`: The result of processing the message.
    fn handle_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel,
    ) -> Result<SendTo<Up>, Error>;

    /// Handles an `UpdateChannel` message.
    ///
    /// This method processes an `UpdateChannel` message and updates the channel settings.
    ///
    /// # Arguments
    /// - `m`: The `UpdateChannel` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Up>, Error>`: The result of processing the message.
    fn handle_update_channel(&mut self, m: UpdateChannel) -> Result<SendTo<Up>, Error>;

    /// Handles a `SubmitSharesStandard` message.
    ///
    /// This method processes a `SubmitSharesStandard` message and validates the submitted shares.
    ///
    /// # Arguments
    /// - `m`: The `SubmitSharesStandard` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Up>, Error>`: The result of processing the message.
    fn handle_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<SendTo<Up>, Error>;

    /// Handles a `SubmitSharesExtended` message.
    ///
    /// This method processes a `SubmitSharesExtended` message and validates the submitted shares.
    ///
    /// # Arguments
    /// - `m`: The `SubmitSharesExtended` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Up>, Error>`: The result of processing the message.
    fn handle_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<SendTo<Up>, Error>;

    /// Handles a `SetCustomMiningJob` message.
    ///
    /// This method processes a `SetCustomMiningJob` message and applies the custom mining job
    /// settings.
    ///
    /// # Arguments
    /// - `m`: The `SetCustomMiningJob` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Up>, Error>`: The result of processing the message.
    fn handle_set_custom_mining_job(&mut self, m: SetCustomMiningJob) -> Result<SendTo<Up>, Error>;
}

/// A trait defining the parser for upstream mining messages used by a downstream.
///
/// This trait provides the functionality to handle and route various types of mining messages
/// from the upstream based on the message type and payload.
pub trait ParseUpstreamMiningMessages<
    Down: IsMiningDownstream + D,
    Selector: DownstreamMiningSelector<Down> + D,
    Router: MiningRouter<Down, Self, Selector>,
> where
    Self: IsMiningUpstream<Down, Selector> + Sized + D,
{
    /// Retrieves the type of the channel supported by this upstream parser.
    ///
    /// # Returns
    /// - `SupportedChannelTypes`: The supported channel type for this upstream.
    fn get_channel_type(&self) -> SupportedChannelTypes;

    /// Retrieves an optional RequestIdMapper, used to manage request IDs across connections.
    ///
    /// # Returns
    /// - `Option<Arc<Mutex<RequestIdMapper>>>`: An optional RequestIdMapper for request ID
    ///   modification.
    fn get_request_id_mapper(&mut self) -> Option<Arc<Mutex<RequestIdMapper>>> {
        None
    }

    /// Parses and routes SV2 mining messages from the upstream based on the message type and
    /// payload. The implementor of DownstreamMining needs to pass a RequestIdMapper if changing
    /// the request ID. Proxies typically need this to ensure the request ID is unique across
    /// the connection.
    ///
    /// # Arguments
    /// - `self_mutex`: The `Arc<Mutex<Self>>` representing the downstream entity.
    /// - `message_type`: The type of the incoming message.
    /// - `payload`: The payload containing the message data.
    /// - `routing_logic`: The logic to handle the routing of the message based on the type.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message, either sending a
    ///   response or an error.
    fn handle_message_mining(
        self_mutex: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        routing_logic: MiningRoutingLogic<Down, Self, Selector, Router>,
    ) -> Result<SendTo<Down>, Error> {
        match Self::handle_message_mining_deserialized(
            self_mutex,
            (message_type, payload).try_into(),
            routing_logic,
        ) {
            Err(Error::UnexpectedMessage(0)) => Err(Error::UnexpectedMessage(message_type)),
            result => result,
        }
    }

    /// Handles the deserialized mining message from the upstream, processing it according to the
    /// routing logic.
    ///
    /// # Arguments
    /// - `self_mutex`: The `Arc<Mutex<Self>>` representing the downstream entity.
    /// - `message`: The deserialized mining message, wrapped in a Result for error handling.
    /// - `routing_logic`: The logic used to route the message based on the type.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message, either sending a
    ///   response or an error.
    fn handle_message_mining_deserialized(
        self_mutex: Arc<Mutex<Self>>,
        message: Result<Mining, Error>,
        routing_logic: MiningRoutingLogic<Down, Self, Selector, Router>,
    ) -> Result<SendTo<Down>, Error> {
        let (channel_type, is_work_selection_enabled) = self_mutex
            .safe_lock(|s| (s.get_channel_type(), s.is_work_selection_enabled()))
            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;

        match message {
            Ok(Mining::OpenStandardMiningChannelSuccess(mut m)) => {
                let remote = match routing_logic {
                    MiningRoutingLogic::None => None,
                    MiningRoutingLogic::Proxy(r_logic) => {
                        let up = r_logic
                            .safe_lock(|r_logic| {
                                r_logic.on_open_standard_channel_success(self_mutex.clone(), &mut m)
                            })
                            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
                        Some(up?)
                    }
                    MiningRoutingLogic::_P(_) => panic!("Must use either MiningRoutingLogic::None or MiningRoutingLogic::Proxy for `routing_logic` param"),
                };
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                    )),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|s| s.handle_open_standard_mining_channel_success(m, remote))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::OpenExtendedMiningChannelSuccess(m)) => {
                info!("Received OpenExtendedMiningChannelSuccess with request id: {} and channel id: {}", m.request_id, m.channel_id);
                debug!("OpenStandardMiningChannelSuccess: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
                    )),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|s| s.handle_open_extended_mining_channel_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
                    )),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|s| s.handle_open_extended_mining_channel_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }

            Ok(Mining::OpenMiningChannelError(m)) => {
                error!(
                    "Received OpenExtendedMiningChannelError with error code {}",
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_open_mining_channel_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_open_mining_channel_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_open_mining_channel_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_open_mining_channel_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }

            Ok(Mining::UpdateChannelError(m)) => {
                error!(
                    "Received UpdateChannelError with error code {}",
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_update_channel_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_update_channel_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_update_channel_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_update_channel_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }

            Ok(Mining::CloseChannel(m)) => {
                info!("Received CloseChannel for channel id: {}", m.channel_id);
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_close_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_close_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_close_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_close_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }

            Ok(Mining::SetExtranoncePrefix(m)) => {
                info!(
                    "Received SetExtranoncePrefix for channel id: {}",
                    m.channel_id
                );
                debug!("SetExtranoncePrefix: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_set_extranonce_prefix(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::SubmitSharesSuccess(m)) => {
                debug!("Received SubmitSharesSuccess");
                trace!("SubmitSharesSuccess: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_submit_shares_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_submit_shares_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_submit_shares_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_submit_shares_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::SubmitSharesError(m)) => {
                error!(
                    "Received SubmitSharesError with error code {}",
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_submit_shares_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_submit_shares_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_submit_shares_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_submit_shares_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }

            Ok(Mining::NewMiningJob(m)) => {
                info!(
                    "Received new mining job for channel id: {} with job id: {} is future: {}",
                    m.channel_id,
                    m.job_id,
                    m.is_future()
                );
                debug!("NewMiningJob: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_new_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
                    }
                    SupportedChannelTypes::Group => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
                    }
                    SupportedChannelTypes::GroupAndExtended => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
                    }
                }
            }
            Ok(Mining::NewExtendedMiningJob(m)) => {
                info!("Received new extended mining job for channel id: {} with job id: {} is_future: {}",m.channel_id, m.job_id, m.is_future());
                debug!("NewExtendedMiningJob: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                    )),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_new_extended_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_new_extended_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_new_extended_mining_job(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::SetNewPrevHash(m)) => {
                info!(
                    "Received SetNewPrevHash channel id: {}, job id: {}",
                    m.channel_id, m.job_id
                );
                debug!("SetNewPrevHash: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_set_new_prev_hash(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_set_new_prev_hash(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_set_new_prev_hash(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_set_new_prev_hash(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }

            Ok(Mining::SetCustomMiningJobSuccess(m)) => {
                info!(
                    "Received SetCustomMiningJobSuccess for channel id: {} for job id: {}",
                    m.channel_id, m.job_id
                );
                debug!("SetCustomMiningJobSuccess: {:?}", m);
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_success(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    _ => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                    )),
                }
            }

            Ok(Mining::SetCustomMiningJobError(m)) => {
                error!(
                    "Received SetCustomMiningJobError with error code {}",
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::Group, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|x| x.handle_set_custom_mining_job_error(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    _ => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                    )),
                }
            }
            Ok(Mining::SetTarget(m)) => {
                info!("Received SetTarget for channel id: {}", m.channel_id);
                debug!("SetTarget: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_set_target(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_set_target(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_set_target(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_set_target(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }

            Ok(Mining::Reconnect(m)) => {
                info!("Received Reconnect");
                debug!("Reconnect: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|x| x.handle_reconnect(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|x| x.handle_reconnect(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_reconnect(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_reconnect(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::SetGroupChannel(m)) => {
                info!("Received SetGroupChannel");
                debug!("SetGroupChannel: {:?}", m);
                match channel_type {
                    SupportedChannelTypes::Standard => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_GROUP_CHANNEL))
                    }
                    SupportedChannelTypes::Extended => {
                        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_GROUP_CHANNEL))
                    }
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|x| x.handle_set_group_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|x| x.handle_set_group_channel(m))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(_) => Err(Error::UnexpectedMessage(0)),
            Err(e) => Err(e),
        }
    }

    /// Determines whether work selection is enabled for this upstream.
    ///
    /// # Returns
    /// - `bool`: A boolean indicating if work selection is enabled.
    fn is_work_selection_enabled(&self) -> bool;

    /// Handles a successful response for opening a standard mining channel.
    ///
    /// # Arguments
    /// - `m`: The `OpenStandardMiningChannelSuccess` message.
    /// - `remote`: An optional reference to the downstream, wrapped in an `Arc<Mutex>`.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_open_standard_mining_channel_success(
        &mut self,
        m: OpenStandardMiningChannelSuccess,
        remote: Option<Arc<Mutex<Down>>>,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles a successful response for opening an extended mining channel.
    ///
    /// # Arguments
    /// - `m`: The `OpenExtendedMiningChannelSuccess` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles an error when opening a mining channel.
    ///
    /// # Arguments
    /// - `m`: The `OpenMiningChannelError` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the error.
    fn handle_open_mining_channel_error(
        &mut self,
        m: OpenMiningChannelError,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles an error when updating a mining channel.
    ///
    /// # Arguments
    /// - `m`: The `UpdateChannelError` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the error.
    fn handle_update_channel_error(&mut self, m: UpdateChannelError)
        -> Result<SendTo<Down>, Error>;

    /// Handles a request to close a mining channel.
    ///
    /// # Arguments
    /// - `m`: The `CloseChannel` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_close_channel(&mut self, m: CloseChannel) -> Result<SendTo<Down>, Error>;

    /// Handles a request to set the extranonce prefix for mining.
    ///
    /// # Arguments
    /// - `m`: The `SetExtranoncePrefix` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_set_extranonce_prefix(
        &mut self,
        m: SetExtranoncePrefix,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles a successful submission of shares.
    ///
    /// # Arguments
    /// - `m`: The `SubmitSharesSuccess` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_submit_shares_success(
        &mut self,
        m: SubmitSharesSuccess,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles an error when submitting shares.
    ///
    /// # Arguments
    /// - `m`: The `SubmitSharesError` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the error.
    fn handle_submit_shares_error(&mut self, m: SubmitSharesError) -> Result<SendTo<Down>, Error>;

    /// Handles a new mining job.
    ///
    /// # Arguments
    /// - `m`: The `NewMiningJob` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_new_mining_job(&mut self, m: NewMiningJob) -> Result<SendTo<Down>, Error>;

    /// Handles a new extended mining job.
    ///
    /// # Arguments
    /// - `m`: The `NewExtendedMiningJob` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles a request to set the new previous hash.
    ///
    /// # Arguments
    /// - `m`: The `SetNewPrevHash` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo<Down>, Error>;

    /// Handles a successful response for setting a custom mining job.
    ///
    /// # Arguments
    /// - `m`: The `SetCustomMiningJobSuccess` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_set_custom_mining_job_success(
        &mut self,
        m: SetCustomMiningJobSuccess,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles an error when setting a custom mining job.
    ///
    /// # Arguments
    /// - `m`: The `SetCustomMiningJobError` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the error.
    fn handle_set_custom_mining_job_error(
        &mut self,
        m: SetCustomMiningJobError,
    ) -> Result<SendTo<Down>, Error>;

    /// Handles a request to set the target for mining.
    ///
    /// # Arguments
    /// - `m`: The `SetTarget` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_set_target(&mut self, m: SetTarget) -> Result<SendTo<Down>, Error>;

    /// Handles a request to reconnect the mining connection.
    ///
    /// # Arguments
    /// - `m`: The `Reconnect` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_reconnect(&mut self, m: Reconnect) -> Result<SendTo<Down>, Error>;

    /// Handles a request to set the group channel for mining.
    ///
    /// # Arguments
    /// - `_m`: The `SetGroupChannel` message.
    ///
    /// # Returns
    /// - `Result<SendTo<Down>, Error>`: The result of processing the message.
    fn handle_set_group_channel(&mut self, _m: SetGroupChannel) -> Result<SendTo<Down>, Error> {
        Ok(SendTo::None(None))
    }
}
