//! Handles the setup connection handshake with the Template Provider.
//!
//! [`SetupConnectionHandler`] builds and sends a `SetupConnection` message,
//! processes the response, and implements `ParseCommonMessagesFromUpstream` for
//! handling common upstream messages.
use super::super::{
    error::{PoolError, PoolResult},
    mining_pool::{EitherFrame, StdFrame},
};
use async_channel::{Receiver, Sender};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use stratum_common::roles_logic_sv2::{
    self, codec_sv2,
    common_messages_sv2::{Protocol, Reconnect, SetupConnection, SetupConnectionError},
    errors::Error,
    handlers::common::{ParseCommonMessagesFromUpstream, SendTo},
    parsers_sv2::{AnyMessage, CommonMessages},
    utils::Mutex,
};
use tracing::{error, info};

/// Handles the connection setup process with the Template Provider.
pub struct SetupConnectionHandler {}

impl SetupConnectionHandler {
    // Creates a `SetupConnection` message for the given network address.
    #[allow(clippy::result_large_err)]
    fn get_setup_connection_message(address: SocketAddr) -> PoolResult<SetupConnection<'static>> {
        let endpoint_host = address.ip().to_string().into_bytes().try_into()?;
        let vendor = String::new().try_into()?;
        let hardware_version = String::new().try_into()?;
        let firmware = String::new().try_into()?;
        let device_id = String::new().try_into()?;
        Ok(SetupConnection {
            protocol: Protocol::TemplateDistributionProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
            endpoint_host,
            endpoint_port: address.port(),
            vendor,
            hardware_version,
            firmware,
            device_id,
        })
    }

    /// Establishes a connection with the Template Provider by sending a `SetupConnection` message
    /// and validating the response.
    pub async fn setup(
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        address: SocketAddr,
    ) -> PoolResult<()> {
        let setup_connection = Self::get_setup_connection_message(address)?;

        let sv2_frame: StdFrame = AnyMessage::Common(setup_connection.into()).try_into()?;
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await?;

        let mut incoming: StdFrame = receiver
            .recv()
            .await?
            .try_into()
            .map_err(|e| PoolError::Codec(codec_sv2::Error::FramingSv2Error(e)))?;
        let message_type = incoming
            .get_header()
            .ok_or_else(|| PoolError::Custom(String::from("No header set")))?
            .msg_type();
        let payload = incoming.payload();

        ParseCommonMessagesFromUpstream::handle_message_common(
            Arc::new(Mutex::new(SetupConnectionHandler {})),
            message_type,
            payload,
        )?;
        Ok(())
    }
}

impl ParseCommonMessagesFromUpstream for SetupConnectionHandler {
    // Handles a successful setup connection response from the template provider.
    fn handle_setup_connection_success(
        &mut self,
        m: roles_logic_sv2::common_messages_sv2::SetupConnectionSuccess,
    ) -> Result<SendTo, Error> {
        info!(
            "Received `SetupConnectionSuccess` from TP: version={}, flags={:b}",
            m.used_version, m.flags
        );
        Ok(SendTo::None(None))
    }

    // Handles an error response during the setup connection process.
    fn handle_setup_connection_error(&mut self, m: SetupConnectionError) -> Result<SendTo, Error> {
        error!(
            "Received `SetupConnectionError` from TP with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        let flags = m.flags;
        let message = SetupConnectionError {
            flags,
            // this error code is currently a hack because there is a lifetime problem with
            // `error_code`.
            error_code: "unsupported-feature-flags"
                .to_string()
                .into_bytes()
                .try_into()
                .unwrap(),
        };
        Ok(SendTo::RelayNewMessage(
            CommonMessages::SetupConnectionError(message),
        ))
    }

    // Handles a channel endpoint change notification from the template provider.
    fn handle_channel_endpoint_changed(
        &mut self,
        m: roles_logic_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<SendTo, Error> {
        info!(
            "Received ChannelEndpointChanged with channel id: {}",
            m.channel_id
        );
        Err(Error::UnexpectedMessage(
            roles_logic_sv2::common_messages_sv2::MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
        ))
    }

    // Handles a reconnect request from the template provider (not implemented yet).
    fn handle_reconnect(&mut self, _m: Reconnect) -> Result<SendTo, Error> {
        todo!()
    }
}
