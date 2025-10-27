use stratum_apps::stratum_core::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers_sv2::HandleCommonMessagesFromServerAsync,
};
use tracing::{info, warn};

use crate::{
    error::JDCError,
    jd_mode::{set_jd_mode, JdMode},
    job_declarator::JobDeclarator,
};

impl HandleCommonMessagesFromServerAsync for JobDeclarator {
    type Error = JDCError;

    type Output<'a> = ();

    async fn handle_setup_connection_success(
        &mut self,
        _server_id: Option<usize>,
        msg: SetupConnectionSuccess,
    ) -> Result<Self::Output<'_>, Self::Error> {
        info!("Received: {}", msg);

        let jd_mode = match msg.flags {
            0 => JdMode::CoinbaseOnly,
            1 => JdMode::FullTemplate,
            _ => JdMode::SoloMining,
        };
        set_jd_mode(jd_mode);

        if jd_mode == JdMode::SoloMining {
            return Err(JDCError::Shutdown);
        }

        Ok(())
    }

    async fn handle_channel_endpoint_changed(
        &mut self,
        _server_id: Option<usize>,
        msg: ChannelEndpointChanged,
    ) -> Result<Self::Output<'_>, Self::Error> {
        info!("Received: {}", msg);
        Ok(())
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
        warn!("Received: {}", msg);
        Err(JDCError::Shutdown)
    }
}
