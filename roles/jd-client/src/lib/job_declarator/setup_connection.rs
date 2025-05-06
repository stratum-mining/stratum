//! Job Declarator: Setup Connection Handler Module
//!
//! Handles the logic for setting up a connection with an upstream Job Declarator (JDS).
//!
//! This includes building and sending a `SetupConnection` message, receiving the response,
//! and handling common SV2 connection-related messages.

use async_channel::{Receiver, Sender};
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::{
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

/// Manages the process of sending and handling the `SetupConnection` handshake
/// for establishing a connection with a JDS.
pub struct SetupConnectionHandler {}

impl SetupConnectionHandler {
    // Builds a `SetupConnection` message using the given proxy address.
    fn get_setup_connection_message(proxy_address: SocketAddr) -> SetupConnection<'static> {
        let endpoint_host = proxy_address
            .ip()
            .to_string()
            .into_bytes()
            .try_into()
            .unwrap();
        let vendor = String::new().try_into().unwrap();
        let hardware_version = String::new().try_into().unwrap();
        let firmware = String::new().try_into().unwrap();
        let device_id = String::new().try_into().unwrap();
        let mut setup_connection = SetupConnection {
            protocol: Protocol::JobDeclarationProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
            endpoint_host,
            endpoint_port: proxy_address.port(),
            vendor,
            hardware_version,
            firmware,
            device_id,
        };
        setup_connection.allow_full_template_mode();
        setup_connection
    }

    /// This method sets up a job declarator connection.
    pub async fn setup(
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        proxy_address: SocketAddr,
    ) -> Result<(), ()> {
        let setup_connection = Self::get_setup_connection_message(proxy_address);

        let sv2_frame: StdFrame = AnyMessage::Common(setup_connection.into())
            .try_into()
            .unwrap();
        let sv2_frame = sv2_frame.into();

        sender.send(sv2_frame).await.map_err(|_| ())?;

        let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();

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
    // Handles a `SetupConnectionSuccess` message received from the JDS.
    //
    // Returns `Ok(SendTo::None(None))` indicating that no immediate message needs
    // to be sent back to the JDS as a direct response to `SetupConnectionSuccess`.
    fn handle_setup_connection_success(
        &mut self,
        m: roles_logic_sv2::common_messages_sv2::SetupConnectionSuccess,
    ) -> Result<SendTo, Error> {
        info!(
            "Received `SetupConnectionSuccess` from JDS: version={}, flags={:b}",
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
