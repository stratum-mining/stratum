use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::{HandleCommonMessagesFromServerAsync, HandlerError as Error},
};
use tracing::{debug, error, info};

use crate::{
    error::JDCError,
    jd_mode::{set_jd_mode, JdMode},
    job_declarator::JobDeclarator,
};

impl HandleCommonMessagesFromServerAsync for JobDeclarator {
    async fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<(), Error> {
        info!("Received SetupConnectionSuccess from JDS");
        debug!(?msg, "SetupConnectionSuccess details");

        let jd_mode = match msg.flags {
            0 => JdMode::CoinbaseOnly,
            1 => JdMode::FullTemplate,
            _ => JdMode::SoloMining,
        };
        set_jd_mode(jd_mode);

        if jd_mode == JdMode::SoloMining {
            return Err(JDCError::Shutdown.into());
        }

        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<(), Error> {
        info!(
            "Received ChannelEndpointChanged from JDS with channel_id: {}",
            msg.channel_id
        );
        debug!("ChannelEndpointChanged: {msg:?}");
        Ok(())
    }

    async fn handle_reconnect(&mut self, msg: Reconnect<'_>) -> Result<(), Error> {
        info!("Received Reconnect from JDS");
        debug!("Reconnect: {msg:?}");
        Ok(())
    }

    async fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError<'_>,
    ) -> Result<(), Error> {
        info!("Received SetupConnectionError from JDS");
        error!("SetupConnectionError: {msg:?}");
        Err(JDCError::Shutdown.into())
    }
}
