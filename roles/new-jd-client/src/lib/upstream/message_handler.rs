use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, Protocol, Reconnect, SetupConnection, SetupConnectionError,
        SetupConnectionSuccess,
    },
    handlers_sv2::{HandleCommonMessagesFromServerAsync, HandlerError as Error},
};
use tracing::info;

use crate::{error::JDCError, upstream::Upstream};

impl HandleCommonMessagesFromServerAsync for Upstream {
    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Error> {
        info!(
            "Received `SetupConnectionSuccess` from Pool: version={}, flags={:b}",
            msg.used_version, msg.flags
        );
        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_reconnect(&mut self, msg: Reconnect<'_>) -> Result<(), Error> {
        todo!()
    }

    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), Error> {
        todo!()
    }
}
