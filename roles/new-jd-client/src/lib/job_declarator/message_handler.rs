use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::{HandleCommonMessagesFromServerAsync, HandlerError as Error},
};
use tracing::{debug, info, instrument, warn};

use crate::{jd_mode::set_jd_mode, job_declarator::JobDeclarator};

impl HandleCommonMessagesFromServerAsync for JobDeclarator {
    #[instrument(name = "setup_connection_success", skip_all)]
    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Error> {
        info!(
            version = msg.used_version,
            flags = msg.flags,
            "JobDeclarator: Setup connection succeeded"
        );
        set_jd_mode(msg.flags.into());

        Ok(())
    }

    #[instrument(name = "channel_endpoint_changed", skip_all)]
    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Error> {
        debug!(?msg, "channel endpoint changed");
        Ok(())
    }

    #[instrument(name = "reconnect", skip_all)]
    async fn handle_reconnect(&mut self, msg: Reconnect<'_>) -> Result<(), Error> {
        warn!(?msg, "reconnect requested");
        Ok(())
    }

    #[instrument(name = "setup_connection_error", skip_all)]
    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), Error> {
        warn!(?msg, "setup connection error received");
        Ok(())
    }
}
