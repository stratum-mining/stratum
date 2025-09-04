use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::{HandleCommonMessagesFromServerAsync, HandlerError as Error},
};
use tracing::{debug, error, info, warn};

use crate::{error::JDCError, template_receiver::TemplateReceiver};

impl HandleCommonMessagesFromServerAsync for TemplateReceiver {
    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Error> {
        info!("Received SetupConnectionSuccess from TP");
        debug!("SetupConnectionSuccess: {msg:?}");

        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Error> {
        info!("Received ChannelEndpointChanged from TP");
        debug!("ChannelEndpointChanged: {msg:?}");
        Ok(())
    }

    async fn handle_reconnect(&mut self, msg: Reconnect<'_>) -> Result<(), Error> {
        info!("Received Reconnect from TP");
        debug!("Reconnect: {msg:?}");
        Ok(())
    }

    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), Error> {
        warn!("Received SetupConnectionError from TP");
        error!("SetupConnectionError: {msg:?}");
        Err(JDCError::Shutdown.into())
    }
}
