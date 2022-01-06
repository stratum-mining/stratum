use super::SendTo_;
pub use crate::CommonMessages;
use crate::Error;
pub use common_messages_sv2::{
    ChannelEndpointChanged, Protocol, SetupConnection, SetupConnectionError, SetupConnectionSuccess,
};
use core::convert::TryInto;
pub type SendTo = SendTo_<CommonMessages<'static>, ()>;

/// DownstreamCommon should be implemented by:
/// * mining device: is a downstream for a proxy or for a pool
/// * proxy: is a downstram for a proxy or for a pool or for a template provider
///
/// ## Example:
/// ```txt
/// downstream -> \                / -> proxy downstreamcommoin \ _ pool 1
/// downstream ->  > proxy server  > -> proxy downstreamcommoin /
/// downstream -> /                \ -> proxy downstreamcommoin  -> pool 2
/// ```
///
pub trait ParseUpstreamCommonMessages {
    fn handle_message_common(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnectionSuccess(m)) => {
                self.handle_setup_connection_success(m)
            }
            Ok(CommonMessages::SetupConnectionError(m)) => self.handle_setup_connection_error(m),
            Ok(CommonMessages::ChannelEndpointChanged(m)) => {
                self.handle_channel_endpoint_changed(m)
            }
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

/// UpstreamCommon should be implemented by:
/// * proxy: is an upstream for mining devices and other proxies
/// * pool: is an upstream for proxies and mining devices
/// * template provider: is an upstream for proxies
///
pub trait ParseDownstreamCommonMessages {
    fn parse_message(message_type: u8, payload: &mut [u8]) -> Result<SetupConnection, Error> {
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnection(m)) => Ok(m),
            Ok(CommonMessages::SetupConnectionSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(CommonMessages::SetupConnectionError(_)) => Err(Error::UnexpectedMessage),
            Ok(CommonMessages::ChannelEndpointChanged(_)) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }

    fn handle_message_common(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        match (message_type, payload).try_into() {
            Ok(CommonMessages::SetupConnection(m)) => self.handle_setup_connection(m),
            Ok(CommonMessages::SetupConnectionSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(CommonMessages::SetupConnectionError(_)) => Err(Error::UnexpectedMessage),
            Ok(CommonMessages::ChannelEndpointChanged(_)) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }

    fn handle_setup_connection(&mut self, m: SetupConnection) -> Result<SendTo, Error>;
}
