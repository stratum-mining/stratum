use roles_logic_sv2::{
    common_messages_sv2::{SetupConnection, SetupConnectionSuccess},
    common_properties::CommonDownstreamData,
    errors::Error,
    handlers::common::{ParseDownstreamCommonMessages, ParseUpstreamCommonMessages},
    parsers::CommonMessages,
    routing_logic::NoRouting,
    utils::Mutex,
};
use std::sync::Arc;
/// Handles the opening connections:
/// 1. Downstream (Mining Device) <-> Upstream Proxy
/// 2. Downstream Proxy <-> Upstream Pool
struct SetupConnectionHandler {}

/// Implement the `ParseUpstreamCommonMessages` trait for `SetupConnectionHandler`.
impl ParseUpstreamCommonMessages<NoRouting> for SetupConnectionHandler {
    /// Upstream sends the Downstream (this proxy) back a `SetupConnection.Success` message on a
    /// successful connection setup. This functions handles that response.
    fn handle_setup_connection_success(
        &mut self,
        _: SetupConnectionSuccess,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        use roles_logic_sv2::handlers::common::SendTo;
        Ok(SendTo::None(None))
    }

    /// Upstream sends the Downstream (this proxy) back a `SetupConnection.Error` message on an
    /// unsuccessful connection setup. This functions handles that response.
    fn handle_setup_connection_error(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, roles_logic_sv2::errors::Error> {
        todo!()
    }
}

impl ParseDownstreamCommonMessages<NoRouting> for SetupConnectionHandler {
    fn handle_setup_connection(
        &mut self,
        incoming: SetupConnection,
        _: Option<Result<(CommonDownstreamData, SetupConnectionSuccess), Error>>,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        use roles_logic_sv2::handlers::common::SendTo;
        let header_only = incoming.requires_standard_job();
        // self.header_only = Some(header_only);
        Ok(SendTo::RelayNewMessage(
            Arc::new(Mutex::new(())),
            CommonMessages::SetupConnectionSuccess(SetupConnectionSuccess {
                flags: 0,
                used_version: 2,
            }),
        ))
    }
}
