use crate::job_declarator::JobDeclarator;
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
        _message: DeclareMiningJobSuccess,
    ) -> Result<SendTo, Error> {
        Ok(SendTo::None(None))
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

    fn handle_message_job_declaration(
        self_: std::sync::Arc<roles_logic_sv2::utils::Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match (message_type, payload).try_into() {
            Ok(JobDeclaration::AllocateMiningJobTokenSuccess(message)) => self_
                .safe_lock(|x| x.handle_allocate_mining_job_token_success(message))
                .unwrap(),
            Ok(JobDeclaration::DeclareMiningJobSuccess(message)) => self_
                .safe_lock(|x| x.handle_declare_mining_job_success(message))
                .unwrap(),
            Ok(JobDeclaration::DeclareMiningJobError(message)) => self_
                .safe_lock(|x| x.handle_declare_mining_job_error(message))
                .unwrap(),
            Ok(JobDeclaration::IdentifyTransactions(message)) => self_
                .safe_lock(|x| x.handle_identify_transactions(message))
                .unwrap(),
            Ok(JobDeclaration::ProvideMissingTransactions(message)) => self_
                .safe_lock(|x| x.handle_provide_missing_transactions(message))
                .unwrap(),
            Ok(JobDeclaration::AllocateMiningJobToken(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x50", 16).unwrap(),
            )),
            Ok(JobDeclaration::DeclareMiningJob(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x57", 16).unwrap(),
            )),
            Ok(JobDeclaration::IdentifyTransactionsSuccess(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x61", 16).unwrap(),
            )),
            Ok(JobDeclaration::ProvideMissingTransactionsSuccess(_)) => Err(
                Error::UnexpectedMessage(u8::from_str_radix("0x63", 16).unwrap()),
            ),
            Err(e) => Err(e),
        }
    }
}
