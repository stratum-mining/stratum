use crate::sv2::Upstream;
use roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::{HandleCommonMessagesFromServerAsync, HandlerError},
};
use tracing::{error, info};

impl HandleCommonMessagesFromServerAsync for Upstream {
    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), HandlerError> {
        error!(
            "Received SetupConnectionError: version={}, flags={:b}",
            msg.error_code, msg.flags
        );

        todo!()
    }

    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), HandlerError> {
        info!(
            "Received SetupConnectionSuccess: version={}, flags={:b}",
            msg.used_version, msg.flags
        );

        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), HandlerError> {
        info!(
            "Received `ChannelEndpointChanged`: channel_id: {}",
            msg.channel_id
        );

        todo!()
    }

    async fn handle_reconnect(&mut self, msg: Reconnect<'_>) -> Result<(), HandlerError> {
        info!(
            "Received `Reconnect`: new_host: {}, new_port: {}",
            msg.new_host, msg.new_port
        );
        todo!()
    }
}
