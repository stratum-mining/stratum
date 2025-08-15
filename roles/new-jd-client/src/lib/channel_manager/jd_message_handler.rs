use stratum_common::roles_logic_sv2::{
    handlers_sv2::{HandleJobDeclarationMessagesFromServerAsync, HandlerError as Error},
    job_declaration_sv2::{
        AllocateMiningJobTokenSuccess, DeclareMiningJobError, DeclareMiningJobSuccess,
        ProvideMissingTransactions,
    },
};
use tracing::info;

use crate::channel_manager::ChannelManager;

impl HandleJobDeclarationMessagesFromServerAsync for ChannelManager {
    async fn handle_allocate_mining_job_token_success(
        &mut self,
        msg: AllocateMiningJobTokenSuccess<'_>,
    ) -> Result<(), Error> {
        info!("Received allocate Mining token success from JDS");
        Ok(())
    }

    async fn handle_declare_mining_job_error(
        &mut self,
        msg: DeclareMiningJobError<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_declare_mining_job_error from JDS");
        Ok(())
    }

    async fn handle_declare_mining_job_success(
        &mut self,
        msg: DeclareMiningJobSuccess<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_declare_mining_job_success from JDS");
        Ok(())
    }

    async fn handle_provide_missing_transactions(
        &mut self,
        msg: ProvideMissingTransactions<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_provide_missing_transactions from JDS");
        Ok(())
    }
}
