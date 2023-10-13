use crate::{
    error::{PoolError, PoolResult},
    EitherFrame, StdFrame,
};
use async_channel::{Receiver, Sender};
use codec_sv2::Frame;
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection},
    errors::Error,
    handlers::common::{ParseUpstreamCommonMessages, SendTo},
    parsers::PoolMessages,
    routing_logic::{CommonRoutingLogic, NoRouting},
    utils::Mutex,
};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};

pub struct SetupConnectionHandler {}

impl SetupConnectionHandler {
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

    pub async fn setup(
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        address: SocketAddr,
    ) -> PoolResult<()> {
        let setup_connection = Self::get_setup_connection_message(address)?;

        let sv2_frame: StdFrame = PoolMessages::Common(setup_connection.into()).try_into()?;
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

        ParseUpstreamCommonMessages::handle_message_common(
            Arc::new(Mutex::new(SetupConnectionHandler {})),
            message_type,
            payload,
            CommonRoutingLogic::None,
        )?;
        Ok(())
    }
}

impl ParseUpstreamCommonMessages<NoRouting> for SetupConnectionHandler {
    fn handle_setup_connection_success(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionSuccess,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        Ok(SendTo::None(None))
    }

    fn handle_setup_connection_error(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        //return error result
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        Err(Error::UnexpectedMessage(
            const_sv2::MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
        ))
    }
}
