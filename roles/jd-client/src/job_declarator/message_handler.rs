use crate::job_declarator::JobDeclarator;
use roles_logic_sv2::{
    handlers::{job_declaration::ParseServerJobDeclarationMessages, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobTokenSuccess, DeclareMiningJobError, DeclareMiningJobSuccess,
        IdentifyTransactions, IdentifyTransactionsSuccess, ProvideMissingTransactions,
        ProvideMissingTransactionsSuccess,
    },
    mining_sv2::{SubmitSharesError, SubmitSharesSuccess},
    parsers::JobDeclaration,
};
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use roles_logic_sv2::errors::Error;

impl ParseServerJobDeclarationMessages for JobDeclarator {
    fn handle_allocate_mining_job_token_success(
        &mut self,
        message: AllocateMiningJobTokenSuccess,
    ) -> Result<SendTo, Error> {
        self.allocated_tokens.push(message.into_static());

        Ok(SendTo::None(None))
    }

    fn handle_declare_mining_job_success(
        &mut self,
        message: DeclareMiningJobSuccess,
    ) -> Result<SendTo, Error> {
        let message = JobDeclaration::DeclareMiningJobSuccess(message.into_static());
        Ok(SendTo::None(Some(message)))
    }

    fn handle_declare_mining_job_error(
        &mut self,
        _message: DeclareMiningJobError,
    ) -> Result<SendTo, Error> {
        Ok(SendTo::None(None))
    }

    fn handle_identify_transactions(
        &mut self,
        message: IdentifyTransactions,
    ) -> Result<SendTo, Error> {
        let message_identify_transactions = IdentifyTransactionsSuccess {
            request_id: message.request_id,
            tx_data_hashes: Vec::new().try_into().unwrap(),
        };
        let message_enum =
            JobDeclaration::IdentifyTransactionsSuccess(message_identify_transactions);
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_provide_missing_transactions(
        &mut self,
        message: ProvideMissingTransactions,
    ) -> Result<SendTo, Error> {
        let message_provide_missing_transactions = ProvideMissingTransactionsSuccess {
            request_id: message.request_id,
            transaction_list: Vec::new().try_into().unwrap(),
        };
        let message_enum =
            JobDeclaration::ProvideMissingTransactionsSuccess(message_provide_missing_transactions);
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_submit_shares_success(
        &mut self,
        _message: SubmitSharesSuccess,
    ) -> Result<SendTo, Error> {
        Ok(SendTo::None(None))
    }

    fn handle_submit_shares_error(&mut self, _message: SubmitSharesError) -> Result<SendTo, Error> {
        Ok(SendTo::None(None))
    }
}
