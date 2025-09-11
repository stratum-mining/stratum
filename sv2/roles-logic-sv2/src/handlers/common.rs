//! # Common Handlers
//!
//! This module defines traits and implementations for handling common Stratum V2 messages exchanged
//! between upstream and downstream nodes.
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
use crate::{errors::Error, utils::Mutex};
use parsers_sv2::CommonMessages;

use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnection, SetupConnectionError,
    SetupConnectionSuccess, *,
};
use core::convert::TryInto;
use std::sync::Arc;

/// see [`SendTo_`]
pub type SendTo = SendTo_<CommonMessages<'static>, ()>;

/// A trait that is implemented by the downstream node, and is used to handle
/// common messages sent by the upstream to the downstream
pub trait ParseCommonMessagesFromUpstream
where
    Self: Sized,
{
    /// Takes a message type and a payload, and if the message type is a
    /// [`crate::parsers::CommonMessages`], it calls the appropriate handler function
    fn handle_message_common(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_common_deserialized(
            self_,
            (message_type, payload).try_into().map_err(Into::into),
        )
    }

    /// Takes a message and it calls the appropriate handler function
    fn handle_message_common_deserialized(
        self_: Arc<Mutex<Self>>,
        message: Result<CommonMessages<'_>, Error>,
    ) -> Result<SendTo, Error> {
        match message {
            Ok(CommonMessages::SetupConnectionSuccess(m)) => {
                self_.safe_lock(|x| x.handle_setup_connection_success(m))?
            }
            Ok(CommonMessages::SetupConnectionError(m)) => {
                self_.safe_lock(|x| x.handle_setup_connection_error(m))?
            }
            Ok(CommonMessages::ChannelEndpointChanged(m)) => {
                self_.safe_lock(|x| x.handle_channel_endpoint_changed(m))?
            }
            Ok(CommonMessages::Reconnect(m)) => self_.safe_lock(|x| x.handle_reconnect(m))?,
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
    fn handle_setup_connection_success(
        &mut self,
        m: SetupConnectionSuccess,
    ) -> Result<SendTo, Error>;

    /// Handles a `SetupConnectionError` message.
    ///
    /// This method processes a `SetupConnectionError` message and handles it
    /// by delegating to the appropriate handler.
    fn handle_setup_connection_error(&mut self, m: SetupConnectionError) -> Result<SendTo, Error>;

    /// Handles a `ChannelEndpointChanged` message.
    ///
    /// This method processes a `ChannelEndpointChanged` message and handles it
    /// by delegating to the appropriate handler.
    fn handle_channel_endpoint_changed(
        &mut self,
        m: ChannelEndpointChanged,
    ) -> Result<SendTo, Error>;

    /// Handles a `Reconnect` message.
    fn handle_reconnect(&mut self, m: Reconnect) -> Result<SendTo, Error>;
}

/// A trait that is implemented by the upstream node, and is used to handle
/// common messages sent by the downstream to the upstream
pub trait ParseCommonMessagesFromDownstream
where
    Self: Sized,
{
    /// It takes a message type and a payload, and if the message is a serialized setup connection
    /// message, it calls the `on_setup_connection` function on the routing logic, and then calls
    /// the `handle_setup_connection` function on the router
    fn handle_message_common(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_common_deserialized(
            self_,
            (message_type, payload).try_into().map_err(Into::into),
        )
    }

    /// It takes a message do setup connection message, it calls
    /// the `on_setup_connection` function on the routing logic, and then calls
    /// the `handle_setup_connection` function on the router
    fn handle_message_common_deserialized(
        self_: Arc<Mutex<Self>>,
        message: Result<CommonMessages<'_>, Error>,
    ) -> Result<SendTo, Error> {
        match message {
            Ok(CommonMessages::SetupConnection(m)) => {
                self_.safe_lock(|x| x.handle_setup_connection(m))?
            }
            Ok(CommonMessages::SetupConnectionSuccess(_)) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
            )),
            Ok(CommonMessages::SetupConnectionError(_)) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
            )),
            Ok(CommonMessages::ChannelEndpointChanged(_)) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            )),
            Ok(CommonMessages::Reconnect(_)) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_RECONNECT))
            }
            Err(e) => Err(e),
        }
    }

    /// Handles a `SetupConnection` message.
    ///
    /// This method processes a `SetupConnection` message and handles it
    /// by delegating to the appropriate handler in the routing logic.
    fn handle_setup_connection(&mut self, m: SetupConnection) -> Result<SendTo, Error>;
}
