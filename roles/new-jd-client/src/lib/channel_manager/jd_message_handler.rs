use stratum_common::roles_logic_sv2::{
    handlers_sv2::{HandleJobDeclarationMessagesFromServerAsync, HandlerError as Error},
    job_declaration_sv2::{
        AllocateMiningJobTokenSuccess, DeclareMiningJobError, DeclareMiningJobSuccess,
        ProvideMissingTransactions,
    },
};

use crate::channel_manager::ChannelManager;

impl HandleJobDeclarationMessagesFromServerAsync for ChannelManager {
    async fn handle_allocate_mining_job_token_success(
        &mut self,
        msg: AllocateMiningJobTokenSuccess<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_declare_mining_job_error(
        &mut self,
        msg: DeclareMiningJobError<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_declare_mining_job_success(
        &mut self,
        msg: DeclareMiningJobSuccess<'_>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn handle_provide_missing_transactions(
        &mut self,
        msg: ProvideMissingTransactions<'_>,
    ) -> Result<(), Error> {
        todo!()
    }
}
