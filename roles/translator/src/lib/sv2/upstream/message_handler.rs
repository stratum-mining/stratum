use crate::sv2::upstream::data::UpstreamData;
use roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess,
    },
    handlers::common::{ParseCommonMessagesFromUpstream, SendTo as SendToCommon},
    Error,
};
use tracing::info;

impl ParseCommonMessagesFromUpstream for UpstreamData {
    fn handle_setup_connection_success(
        &mut self,
        m: SetupConnectionSuccess,
    ) -> Result<SendToCommon, Error> {
        info!(
            "Received `SetupConnectionSuccess`: version={}, flags={:b}",
            m.used_version, m.flags
        );
        Ok(SendToCommon::None(None))
    }

    fn handle_setup_connection_error(
        &mut self,
        _m: SetupConnectionError,
    ) -> Result<SendToCommon, Error> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _m: ChannelEndpointChanged,
    ) -> Result<SendToCommon, Error> {
        todo!()
    }

    fn handle_reconnect(&mut self, _m: Reconnect) -> Result<SendToCommon, Error> {
        todo!()
    }
}
