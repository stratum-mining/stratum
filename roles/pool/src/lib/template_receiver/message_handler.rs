use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::HandleCommonMessagesFromServerAsync,
};
use tracing::{error, info};

use crate::{error::PoolError, template_receiver::TemplateReceiver};

impl HandleCommonMessagesFromServerAsync for TemplateReceiver {
    type Error = PoolError;

    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Self::Error> {
        info!(
            "Received `SetupConnectionSuccess` from TP: version={}, flags={:b}",
            msg.used_version, msg.flags
        );
        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Self::Error> {
        info!(
            "Received ChannelEndpointChanged with channel id: {}",
            msg.channel_id
        );
        Ok(())
    }

    async fn handle_reconnect(&mut self, msg: Reconnect<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), Self::Error> {
        error!(
            "Received `SetupConnectionError` from TP with error code {}",
            std::str::from_utf8(msg.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Err(PoolError::Shutdown)
    }
}
