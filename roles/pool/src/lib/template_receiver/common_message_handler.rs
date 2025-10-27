use stratum_apps::stratum_core::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::HandleCommonMessagesFromServerAsync,
};
use tracing::{error, info};

use crate::{error::PoolError, template_receiver::TemplateReceiver};

impl HandleCommonMessagesFromServerAsync for TemplateReceiver {
    type Error = PoolError;

    type Output<'a> = ();

    async fn handle_setup_connection_success(
        &mut self,
        _server_id: Option<usize>,
        msg: SetupConnectionSuccess,
    ) -> Result<Self::Output<'_>, Self::Error> {
        info!(
            "Received `SetupConnectionSuccess` from TP: version={}, flags={:b}",
            msg.used_version, msg.flags
        );
        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        _server_id: Option<usize>,
        msg: ChannelEndpointChanged,
    ) -> Result<Self::Output<'_>, Self::Error> {
        info!(
            "Received ChannelEndpointChanged with channel id: {}",
            msg.channel_id
        );
        Err(PoolError::Shutdown)
    }

    async fn handle_reconnect(
        &mut self,
        _server_id: Option<usize>,
        msg: Reconnect<'_>,
    ) -> Result<Self::Output<'_>, Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_setup_connection_error(
        &mut self,
        _server_id: Option<usize>,
        msg: SetupConnectionError<'_>,
    ) -> Result<Self::Output<'_>, Self::Error> {
        error!(
            "Received `SetupConnectionError` from TP with error code {}",
            std::str::from_utf8(msg.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Err(PoolError::Shutdown)
    }
}
