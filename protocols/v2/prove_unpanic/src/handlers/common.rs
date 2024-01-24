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
use core::convert::TryInto;
use std::sync::Arc;
use tracing::debug;

/// see [`SendTo_`]
pub type SendTo = SendTo_<CommonMessages<'static>, ()>;

/// A trait that is implemented by the downstream. It should be used to parse the common messages that
/// are sent from the upstream to the downstream.
pub trait ParseUpstreamCommonMessages<Router: CommonRouter>
where
    Self: Sized,
{
    /// Takes a message type and a payload, and if the message type is a [`crate::parsers::CommonMessages`], it
    /// calls the appropriate handler function
    ///
    /// Arguments:
    ///
    /// * `message_type`: See [`const_sv2`].
    ///
    fn handle_message_common(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        _routing_logic: CommonRoutingLogic<Router>,
    ) -> Result<SendTo, Error> {
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnectionSuccess(m)) => self_
                .safe_lock(|x| x.handle_setup_connection_success(m))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(CommonMessages::SetupConnectionError(m)) => self_
                .safe_lock(|x| x.handle_setup_connection_error(m))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(CommonMessages::ChannelEndpointChanged(m)) => self_
                .safe_lock(|x| x.handle_channel_endpoint_changed(m))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(CommonMessages::SetupConnection(_)) => Err(Error::UnexpectedMessage(message_type)),
            Err(e) => Err(e),
        }
    }

    /// Called by `Self::handle_message_common` when the `SetupConnectionSuccess` message is received from the upstream node.
    fn handle_setup_connection_success(
        &mut self,
        m: SetupConnectionSuccess,
    ) -> Result<SendTo, Error>;

    /// Called by `Self::handle_message_common` when the `SetupConnectionError` message is received from the upstream node.
    fn handle_setup_connection_error(&mut self, m: SetupConnectionError) -> Result<SendTo, Error>;

    /// Called by `Self::handle_message_common` when the `ChannelEndpointChanged` message is received from the upstream node.
    fn handle_channel_endpoint_changed(
        &mut self,
        m: ChannelEndpointChanged,
    ) -> Result<SendTo, Error>;
}

/// A trait that is implemented by the upstream node, and is used to handle [`crate::parsers::CommonMessages::SetupConnection`]
/// messages sent by the downstream to the upstream
pub trait ParseDownstreamCommonMessages<Router: CommonRouter>
where
    Self: Sized,
{
    /// Used to parse a serialized downstream setup connection message into a [`crate::parsers::CommonMessages::SetupConnection`]
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
    /// message, it calls the `on_setup_connection` function on the routing logic, and then calls the
    /// `handle_setup_connection` function on the router
    ///
    /// Arguments:
    ///
    /// * `message_type`: See [`const_sv2`].
    ///
    fn handle_message_common(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        routing_logic: CommonRoutingLogic<Router>,
    ) -> Result<SendTo, Error> {
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnection(m)) => match routing_logic {
                CommonRoutingLogic::Proxy(r_logic) => {
                    debug!("Got proxy setup connection message: {:?}", m);
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
            },
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

    /// Called by `Self::handle_message_common` when a setup connection message is received from the downstream node.
    fn handle_setup_connection(
        &mut self,
        m: SetupConnection,
        result: Option<Result<(CommonDownstreamData, SetupConnectionSuccess), Error>>,
    ) -> Result<SendTo, Error>;
}
