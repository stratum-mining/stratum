use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{SetupConnection, SetupConnectionSuccess},
    handlers_sv2::{HandleCommonMessagesFromClientAsync, HandlerError as Error},
};
use tracing::info;

use crate::downstream::Downstream;

impl HandleCommonMessagesFromClientAsync for Downstream {
    async fn handle_setup_connection(&mut self, msg: SetupConnection<'_>) -> Result<(), Error> {
        info!(
            "Received `SetupConnection`: version={}, flags={:b}",
            msg.min_version, msg.flags
        );
        Ok(())
    }
}
