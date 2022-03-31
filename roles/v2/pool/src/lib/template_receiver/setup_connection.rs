use crate::{EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
use codec_sv2::Frame;
use messages_sv2::{
    common_messages_sv2::{Protocol, SetupConnection},
    handlers::common::{ParseUpstreamCommonMessages, SendTo},
    parsers::PoolMessages,
    routing_logic::{CommonRoutingLogic, NoRouting},
    utils::Mutex,
};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};

pub struct SetupConnectionHandler {}

impl SetupConnectionHandler {
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

    pub async fn setup(
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        address: SocketAddr,
    ) -> Result<(), ()> {
        let setup_connection = Self::get_setup_connection_message(address);

        let sv2_frame: StdFrame = PoolMessages::Common(setup_connection.into())
            .try_into()
            .unwrap();
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await.map_err(|_| ())?;

        let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        ParseUpstreamCommonMessages::handle_message_common(
            Arc::new(Mutex::new(SetupConnectionHandler {})),
            message_type,
            payload,
            CommonRoutingLogic::None,
        )
        .unwrap();
        Ok(())
    }
}

impl ParseUpstreamCommonMessages<NoRouting> for SetupConnectionHandler {
    fn handle_setup_connection_success(
        &mut self,
        _: messages_sv2::common_messages_sv2::SetupConnectionSuccess,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::errors::Error> {
        Ok(SendTo::None(None))
    }

    fn handle_setup_connection_error(
        &mut self,
        _: messages_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::errors::Error> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: messages_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<messages_sv2::handlers::common::SendTo, messages_sv2::errors::Error> {
        todo!()
    }
}
