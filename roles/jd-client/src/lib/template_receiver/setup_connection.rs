//! Template Receiver: Setup Connection Handler
//!
//! Handles setup connection logic using the Template Distribution Protocol in SV2.
//!
//! This includes sending a `SetupConnection` message and processing responses from the upstream.

use async_channel::{Receiver, Sender};
use network_helpers_sv2::roles_logic_sv2::{
    self,
    codec_sv2::{StandardEitherFrame, StandardSv2Frame},
    common_messages_sv2::{Protocol, Reconnect, SetupConnection},
    handlers::common::{ParseCommonMessagesFromUpstream, SendTo},
    parsers::AnyMessage,
    utils::Mutex,
    Error,
};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use tracing::info;

pub type Message = AnyMessage<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

// A handler responsible for managing the setup connection handshake.
pub struct SetupConnectionHandler {}

impl SetupConnectionHandler {
    /// Builds a `SetupConnection` message using the given socket address.
    fn get_setup_connection_message(address: SocketAddr) -> SetupConnection<'static> {
        let endpoint_host = address.ip().to_string().into_bytes().try_into().unwrap();
        let vendor = String::new().try_into().unwrap();
        let hardware_version = String::new().try_into().unwrap();
        let firmware = String::new().try_into().unwrap();
        let device_id = String::new().try_into().unwrap();
        SetupConnection {
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
        }
    }

    /// processes the setup connection lifecycle.
    pub async fn setup(
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        address: SocketAddr,
    ) -> Result<(), ()> {
        let setup_connection = Self::get_setup_connection_message(address);

        let sv2_frame: StdFrame = AnyMessage::Common(setup_connection.into())
            .try_into()
            .unwrap();
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await.map_err(|_| ())?;

        let mut incoming: StdFrame = receiver
            .recv()
            .await
            .expect("Connection to TP closed!")
            .try_into()
            .expect("Failed to parse incoming SetupConnectionResponse");
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        ParseCommonMessagesFromUpstream::handle_message_common(
            Arc::new(Mutex::new(SetupConnectionHandler {})),
            message_type,
            payload,
        )
        .unwrap();
        Ok(())
    }
}

impl ParseCommonMessagesFromUpstream for SetupConnectionHandler {
    // Handles a `SetupConnectionSuccess` message received from the upstream.
    //
    // Returns `Ok(SendTo::None(None))` indicating that no immediate message needs
    // to be sent back to the server as a response to `SetupConnectionSuccess`.
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

    fn handle_setup_connection_error(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        todo!()
    }

    fn handle_reconnect(&mut self, _m: Reconnect) -> Result<SendTo, Error> {
        todo!()
    }
}
