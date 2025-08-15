use crate::sv2::Upstream;
use roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::{HandleCommonMessagesFromServerAsync, HandlerError},
};
use tracing::{info, warn};

impl HandleCommonMessagesFromServerAsync for Upstream {
    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), HandlerError> {
        warn!("Received: {}", msg);

        todo!()
    }

    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), HandlerError> {
        info!("Received: {}", msg);

        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), HandlerError> {
        info!("Received: {}", msg);

        todo!()
    }

    async fn handle_reconnect(&mut self, msg: Reconnect<'_>) -> Result<(), HandlerError> {
        info!("Received: {}", msg);
        todo!()
    }
}
