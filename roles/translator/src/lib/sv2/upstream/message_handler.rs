use crate::{error::TproxyError, sv2::Upstream};
use stratum_apps::stratum_core::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::HandleCommonMessagesFromServerAsync,
};
use tracing::{error, info};

impl HandleCommonMessagesFromServerAsync for Upstream {
    type Error = TproxyError;

    type Output<'a> = ();

    async fn handle_setup_connection_error(
        &mut self,
        _server_id: Option<usize>,
        msg: SetupConnectionError<'_>,
    ) -> Result<Self::Output<'_>, Self::Error> {
        error!("Received: {}", msg);
        todo!()
    }

    async fn handle_setup_connection_success(
        &mut self,
        _server_id: Option<usize>,
        msg: SetupConnectionSuccess,
    ) -> Result<Self::Output<'_>, Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        _server_id: Option<usize>,
        msg: ChannelEndpointChanged,
    ) -> Result<Self::Output<'_>, Self::Error> {
        info!("Received: {}", msg);
        todo!()
    }

    async fn handle_reconnect(
        &mut self,
        _server_id: Option<usize>,
        msg: Reconnect<'_>,
    ) -> Result<Self::Output<'_>, Self::Error> {
        info!("Received: {}", msg);
        todo!()
    }
}
