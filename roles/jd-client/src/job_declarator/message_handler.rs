use crate::job_declarator::JobDeclarator;
use roles_logic_sv2::{
    handlers::{job_declaration::ParseServerJobDeclarationMessages, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobTokenSuccess, DeclareMiningJob, DeclareMiningJobError,
        DeclareMiningJobSuccess, IdentifyTransactions, IdentifyTransactionsSuccess,
        ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
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
        // TODO: use or discard
        let _coinbase_output_max_additional_size = message.coinbase_output_max_additional_size;

        let new_template = self.new_template.as_ref().unwrap();

        let message_declare_mining_job = DeclareMiningJob {
            request_id: message.request_id,
            mining_job_token: message.mining_job_token.into_static(),
            version: 2,
            coinbase_tx_version: new_template.clone().coinbase_tx_version,
            coinbase_prefix: new_template.clone().coinbase_prefix,
            coinbase_tx_input_n_sequence: new_template.clone().coinbase_tx_input_sequence,
            coinbase_tx_value_remaining: new_template.clone().coinbase_tx_value_remaining,
            coinbase_tx_outputs: new_template.clone().coinbase_tx_outputs,
            coinbase_tx_locktime: new_template.clone().coinbase_tx_locktime,
            min_extranonce_size: 0,
            tx_short_hash_nonce: 0,
            tx_short_hash_list: Vec::new().try_into().unwrap(),
            tx_hash_list_hash: Vec::new().try_into().unwrap(),
            excess_data: Vec::new().try_into().unwrap(),
        };
        let declare_mining_job = JobDeclaration::DeclareMiningJob(message_declare_mining_job);
        println!("Send declare mining job to pool: {:?}", declare_mining_job);
        Ok(SendTo::Respond(declare_mining_job))
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
