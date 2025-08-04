use stratum_common::roles_logic_sv2::{
    handlers_sv2::{HandleTemplateDistributionMessagesFromServerAsync, HandlerError as Error},
    template_distribution_sv2::*,
};

use crate::channel_manager::ChannelManager;

impl HandleTemplateDistributionMessagesFromServerAsync for ChannelManager {
    async fn handle_new_template(&mut self, msg: NewTemplate<'_>) -> Result<(), Error> {
        todo!()
    }

    async fn handle_request_tx_data_error(
        &mut self,
        msg: RequestTransactionDataError<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_request_tx_data_success(
        &mut self,
        msg: RequestTransactionDataSuccess<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash<'_>) -> Result<(), Error> {
        todo!()
    }
}
