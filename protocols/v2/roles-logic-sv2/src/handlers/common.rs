//! # Common Handlers
//!
//! This module defines traits and implementations for handling common Stratum V2 messages exchanged
//! between upstream and downstream nodes.
//!
//! ## Core Traits
//!
//! - `ParseUpstreamCommonMessages`: Implemented by downstream nodes to handle common messages
//!   received from upstream nodes, such as setup connection results or channel endpoint changes.
//! - `ParseDownstreamCommonMessages`: Implemented by upstream nodes to process setup connection
//!   messages received from downstream nodes.
//!
//! ## Message Handling
//!
//! Handlers in this module are responsible for:
//! - Parsing and deserializing common messages.
//! - Dispatching deserialized messages to appropriate handler functions based on message type, such
//!   as `SetupConnection` or `ChannelEndpointChanged`.
//! - Ensuring robust error handling for unexpected or malformed messages.
//!
//! ## Return Type
//!
//! Functions return `Result<SendTo, Error>`, where `SendTo` specifies the next action for the
//! message: whether to forward it, respond to it, or ignore it.
//!
//! ## Structure
//!
//! This module includes:
//! - Traits for upstream and downstream message parsing and handling.
//! - Functions to process common message types while maintaining clear separation of concerns.
//! - Error handling mechanisms to address edge cases and ensure reliable communication within
//!   Stratum V2 networks.

use super::SendTo_;
use crate::{
    common_properties::CommonDownstreamData,
    errors::Error,
    parsers::CommonMessages,
    routing_logic::{CommonRouter, CommonRoutingLogic},
    utils::Mutex,
};
use common_messages_sv2::{
    ChannelEndpointChanged, SetupConnection, SetupConnectionError, SetupConnectionSuccess,
};
use const_sv2::*;
use core::convert::TryInto;
use std::sync::Arc;
use tracing::{debug, error, info, trace};

/// see [`SendTo_`]
pub type SendTo = SendTo_<CommonMessages<'static>, ()>;

/// A trait that is implemented by the downstream. It should be used to parse the common messages
/// that are sent from the upstream to the downstream.
pub trait ParseUpstreamCommonMessages<Router: CommonRouter>
where
    Self: Sized,
{
    /// Takes a message type and a payload, and if the message type is a
    /// [`crate::parsers::CommonMessages`], it calls the appropriate handler function
    ///
    /// Arguments:
    ///
    /// * `message_type`: See [`const_sv2`].
    fn handle_message_common(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        routing_logic: CommonRoutingLogic<Router>,
    ) -> Result<SendTo, Error> {
        Self::handle_message_common_deserilized(
            self_,
            (message_type, payload).try_into(),
            routing_logic,
        )
    }

    /// Takes a message and it calls the appropriate handler function
    ///
    /// Arguments:
    ///
    /// * `message_type`: See [`const_sv2`].
    fn handle_message_common_deserilized(
        self_: Arc<Mutex<Self>>,
        message: Result<CommonMessages<'_>, Error>,
        _routing_logic: CommonRoutingLogic<Router>,
    ) -> Result<SendTo, Error> {
        match message {
            Ok(CommonMessages::SetupConnectionSuccess(m)) => {
                info!(
                    "Received SetupConnectionSuccess: version={}, flags={:b}",
                    m.used_version, m.flags
                );
                self_
                    .safe_lock(|x| x.handle_setup_connection_success(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(CommonMessages::SetupConnectionError(m)) => {
                error!(
                    "Received SetupConnectionError with error code {}",
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                self_
                    .safe_lock(|x| x.handle_setup_connection_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(CommonMessages::ChannelEndpointChanged(m)) => {
                info!(
                    "Received ChannelEndpointChanged with channel id: {}",
                    m.channel_id
                );
                self_
                    .safe_lock(|x| x.handle_channel_endpoint_changed(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(CommonMessages::SetupConnection(_)) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_SETUP_CONNECTION))
            }
            Err(e) => Err(e),
        }
    }

    /// Handles a `SetupConnectionSuccess` message.
    ///
    /// This method processes a `SetupConnectionSuccess` message and handles it
    /// by delegating to the appropriate handler.
    ///
    /// # Arguments
    /// - `message`: The `SetupConnectionSuccess` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_setup_connection_success(
        &mut self,
        m: SetupConnectionSuccess,
    ) -> Result<SendTo, Error>;

    /// Handles a `SetupConnectionError` message.
    ///
    /// This method processes a `SetupConnectionError` message and handles it
    /// by delegating to the appropriate handler.
    ///
    /// # Arguments
    /// - `message`: The `SetupConnectionError` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_setup_connection_error(&mut self, m: SetupConnectionError) -> Result<SendTo, Error>;

    /// Handles a `ChannelEndpointChanged` message.
    ///
    /// This method processes a `ChannelEndpointChanged` message and handles it
    /// by delegating to the appropriate handler.
    ///
    /// # Arguments
    /// - `message`: The `ChannelEndpointChanged` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_channel_endpoint_changed(
        &mut self,
        m: ChannelEndpointChanged,
    ) -> Result<SendTo, Error>;
}

/// A trait that is implemented by the upstream node, and is used to handle
/// [`crate::parsers::CommonMessages::SetupConnection`] messages sent by the downstream to the
/// upstream
pub trait ParseDownstreamCommonMessages<Router: CommonRouter>
where
    Self: Sized,
{
    /// Used to parse a serialized downstream setup connection message into a
    /// [`crate::parsers::CommonMessages::SetupConnection`]
    fn parse_message(message_type: u8, payload: &mut [u8]) -> Result<SetupConnection, Error> {
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnection(m)) => Ok(m),
            Ok(CommonMessages::SetupConnectionSuccess(_)) => Err(Error::UnexpectedMessage(
                const_sv2::MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
            )),
            Ok(CommonMessages::SetupConnectionError(_)) => Err(Error::UnexpectedMessage(
                const_sv2::MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
            )),
            Ok(CommonMessages::ChannelEndpointChanged(_)) => Err(Error::UnexpectedMessage(
                const_sv2::MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            )),
            Err(e) => Err(e),
        }
    }

    /// It takes a message type and a payload, and if the message is a serialized setup connection
    /// message, it calls the `on_setup_connection` function on the routing logic, and then calls
    /// the `handle_setup_connection` function on the router
    ///
    /// Arguments:
    ///
    /// * `message_type`: See [`const_sv2`].
    fn handle_message_common(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        routing_logic: CommonRoutingLogic<Router>,
    ) -> Result<SendTo, Error> {
        Self::handle_message_common_deserilized(
            self_,
            (message_type, payload).try_into(),
            routing_logic,
        )
    }

    /// It takes a message do setup connection message, it calls
    /// the `on_setup_connection` function on the routing logic, and then calls
    /// the `handle_setup_connection` function on the router
    fn handle_message_common_deserilized(
        self_: Arc<Mutex<Self>>,
        message: Result<CommonMessages<'_>, Error>,
        routing_logic: CommonRoutingLogic<Router>,
    ) -> Result<SendTo, Error> {
        match message {
            Ok(CommonMessages::SetupConnection(m)) => {
                info!(
                    "Received SetupConnection: version={}, flags={:b}",
                    m.min_version, m.flags
                );
                debug!("Setup connection message: {:?}", m);
                match routing_logic {
                    CommonRoutingLogic::Proxy(r_logic) => {
                        trace!("On SetupConnection r_logic is {:?}", r_logic);
                        let result = r_logic
                            .safe_lock(|r_logic| r_logic.on_setup_connection(&m))
                            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?;
                        self_
                            .safe_lock(|x| x.handle_setup_connection(m, Some(result)))
                            .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
                    }
                    CommonRoutingLogic::None => self_
                        .safe_lock(|x| x.handle_setup_connection(m, None))
                        .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(CommonMessages::SetupConnectionSuccess(_)) => Err(Error::UnexpectedMessage(
                const_sv2::MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
            )),
            Ok(CommonMessages::SetupConnectionError(_)) => Err(Error::UnexpectedMessage(
                const_sv2::MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
            )),
            Ok(CommonMessages::ChannelEndpointChanged(_)) => Err(Error::UnexpectedMessage(
                const_sv2::MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            )),
            Err(e) => Err(e),
        }
    }

    /// Handles a `SetupConnection` message.
    ///
    /// This method processes a `SetupConnection` message and handles it
    /// by delegating to the appropriate handler in the routing logic.
    ///
    /// # Arguments
    /// - `message`: The `SetupConnection` message.
    /// - `result`: The result of the `on_setup_connection` call, if available.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_setup_connection(
        &mut self,
        m: SetupConnection,
        result: Option<Result<(CommonDownstreamData, SetupConnectionSuccess), Error>>,
    ) -> Result<SendTo, Error>;
}
