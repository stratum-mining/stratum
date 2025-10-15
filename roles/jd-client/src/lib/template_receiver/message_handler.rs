use stratum_apps::stratum_core::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::HandleCommonMessagesFromServerAsync,
};
use tracing::{info, warn};

use crate::{error::JDCError, template_receiver::TemplateReceiver};

impl HandleCommonMessagesFromServerAsync for TemplateReceiver {
    type Error = JDCError;

    async fn handle_setup_connection_success(
        &mut self,
        _server_id: Option<usize>,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        _server_id: Option<usize>,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_reconnect(
        &mut self,
        _server_id: Option<usize>,
        msg: Reconnect<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_setup_connection_error(
        &mut self,
        _server_id: Option<usize>,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        Err(JDCError::Shutdown)
    }
}
