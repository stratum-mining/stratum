use super::JobDeclarator;
use roles_logic_sv2::{
    handlers::{job_declaration::ParseServerJobDeclarationMessages, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobTokenSuccess, DeclareMiningJobError, DeclareMiningJobSuccess,
        IdentifyTransactions, IdentifyTransactionsSuccess, ProvideMissingTransactions,
        ProvideMissingTransactionsSuccess,
    },
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
            tx_data_hashes: Vec::new().into(),
        };
        let message_enum =
            JobDeclaration::IdentifyTransactionsSuccess(message_identify_transactions);
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_provide_missing_transactions(
        &mut self,
        message: ProvideMissingTransactions,
    ) -> Result<SendTo, Error> {
        let tx_list = self
            .last_declare_mining_jobs_sent
            .get(&message.request_id)
            .unwrap()
            .clone()
            .unwrap()
            .tx_list
            .into_inner();
        let unknown_tx_position_list: Vec<u16> = message.unknown_tx_position_list.into_inner();
        let missing_transactions: Vec<binary_sv2::B016M> = unknown_tx_position_list
            .iter()
            .filter_map(|&pos| tx_list.get(pos as usize).cloned())
            .collect();
        let request_id = message.request_id;
        let message_provide_missing_transactions = ProvideMissingTransactionsSuccess {
            request_id,
            transaction_list: binary_sv2::Seq064K::new(missing_transactions).unwrap(),
        };
        let message_enum =
            JobDeclaration::ProvideMissingTransactionsSuccess(message_provide_missing_transactions);
        Ok(SendTo::Respond(message_enum))
    }
}
