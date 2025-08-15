use crate::{error::TproxyError, sv2::Upstream};
use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::HandleCommonMessagesFromServerAsync,
};
use tracing::{error, info};

impl HandleCommonMessagesFromServerAsync for Upstream {
    type Error = TproxyError;

    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), Self::Error> {
        error!("Received: {}", msg);
        todo!()
    }

    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        todo!()
    }

    async fn handle_reconnect(&mut self, msg: Reconnect<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        todo!()
    }
}
