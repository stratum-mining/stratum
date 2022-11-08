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

pub type SendTo = SendTo_<CommonMessages<'static>, ()>;

pub trait ParseUpstreamCommonMessages<Router: CommonRouter>
where
    Self: Sized,
{
    // Is fine to unwrap on safe_lock
    fn handle_message_common(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        _routing_logic: CommonRoutingLogic<Router>,
    ) -> Result<SendTo, Error> {
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnectionSuccess(m)) => self_
                .safe_lock(|x| x.handle_setup_connection_success(m))
                .unwrap(),
            Ok(CommonMessages::SetupConnectionError(m)) => self_
                .safe_lock(|x| x.handle_setup_connection_error(m))
                .unwrap(),
            Ok(CommonMessages::ChannelEndpointChanged(m)) => self_
                .safe_lock(|x| x.handle_channel_endpoint_changed(m))
                .unwrap(),
            Ok(CommonMessages::SetupConnection(_)) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }

    fn handle_setup_connection_success(
        &mut self,
        m: SetupConnectionSuccess,
    ) -> Result<SendTo, Error>;

    fn handle_setup_connection_error(&mut self, m: SetupConnectionError) -> Result<SendTo, Error>;

    fn handle_channel_endpoint_changed(
        &mut self,
        m: ChannelEndpointChanged,
    ) -> Result<SendTo, Error>;
}

pub trait ParseDownstreamCommonMessages<Router: CommonRouter>
where
    Self: Sized,
{
    fn parse_message(message_type: u8, payload: &mut [u8]) -> Result<SetupConnection, Error> {
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnection(m)) => Ok(m),
            Ok(CommonMessages::SetupConnectionSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(CommonMessages::SetupConnectionError(_)) => Err(Error::UnexpectedMessage),
            Ok(CommonMessages::ChannelEndpointChanged(_)) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }

    // Is fine to unwrap on safe_lock
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
                        .unwrap();
                    self_
                        .safe_lock(|x| x.handle_setup_connection(m, Some(result)))
                        .unwrap()
                }
                CommonRoutingLogic::None => self_
                    .safe_lock(|x| x.handle_setup_connection(m, None))
                    .unwrap(),
            },
            Ok(CommonMessages::SetupConnectionSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(CommonMessages::SetupConnectionError(_)) => Err(Error::UnexpectedMessage),
            Ok(CommonMessages::ChannelEndpointChanged(_)) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }

    fn handle_setup_connection(
        &mut self,
        m: SetupConnection,
        result: Option<Result<(CommonDownstreamData, SetupConnectionSuccess), Error>>,
    ) -> Result<SendTo, Error>;
}
