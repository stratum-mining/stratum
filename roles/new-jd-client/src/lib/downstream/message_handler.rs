use stratum_common::roles_logic_sv2::{
    common_messages_sv2::SetupConnection,
    handlers_sv2::{HandleCommonMessagesFromClientAsync, HandlerError as Error},
};

use crate::downstream::Downstream;

impl HandleCommonMessagesFromClientAsync for Downstream {
    async fn handle_setup_connection(&mut self, msg: SetupConnection<'_>) -> Result<(), Error> {
        todo!()
    }
}
