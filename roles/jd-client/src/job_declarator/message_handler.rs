use crate::job_declarator::JobDeclarator;
use roles_logic_sv2::{
    handlers::{job_declaration::ParseServerJobDeclarationMessages, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobTokenSuccess, CommitMiningJob, CommitMiningJobError,
        CommitMiningJobSuccess, IdentifyTransactions, IdentifyTransactionsSuccess,
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
        let coinbase_output_max_additional_size = message.coinbase_output_max_additional_size;

        let new_template = self.new_template.as_ref().unwrap();

        let message_commit_mining_job = CommitMiningJob {
            request_id: message.request_id,
            mining_job_token: message.mining_job_token,
            version: 2,
            coinbase_tx_version: new_template.clone().coinbase_tx_version,
            coinbase_prefix: new_template.clone().coinbase_prefix,
            coinbase_tx_input_n_sequence: new_template.clone().coinbase_tx_input_sequence,
            coinbase_tx_value_remaining: new_template.clone().coinbase_tx_value_remaining,
            coinbase_tx_outputs: new_template.clone().coinbase_tx_outputs,
            coinbase_tx_locktime: new_template.clone().coinbase_tx_locktime,
            min_extranonce_size: 0,
            tx_short_hash_nonce: 0,
            tx_short_hash_list: todo!(),
            tx_hash_list_hash: todo!(),
            excess_data: todo!(),
            merkle_path: todo!(),
        };
        let commit_mining_job = JobDeclaration::CommitMiningJob(message_commit_mining_job);
        println!("Send commit mining job to pool: {:?}", commit_mining_job);
        Ok(SendTo::Respond(commit_mining_job))
    }

    fn handle_commit_mining_job_success(
        &mut self,
        message: CommitMiningJobSuccess,
    ) -> Result<SendTo, Error> {
        todo!()
    }

    fn handle_commit_mining_job_error(
        &mut self,
        message: CommitMiningJobError,
    ) -> Result<SendTo, Error> {
        todo!();
    }

    fn handle_identify_transactions(
        &mut self,
        message: IdentifyTransactions,
    ) -> Result<SendTo, Error> {
        let message_identify_transactions = IdentifyTransactionsSuccess {
            request_id: message.request_id,
            mining_job_token: todo!(),
            coinbase_output_max_additional_size: todo!(),
            coinbase_output: todo!(),
            async_mining_allowed: todo!(),
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
            mining_job_token: todo!(),
            coinbase_output_max_additional_size: todo!(),
            coinbase_output: todo!(),
            async_mining_allowed: todo!(),
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
            Ok(JobDeclaration::CommitMiningJobSuccess(message)) => self_
                .safe_lock(|x| x.handle_commit_mining_job_success(message))
                .unwrap(),
            Ok(JobDeclaration::CommitMiningJobError(message)) => self_
                .safe_lock(|x| x.handle_commit_mining_job_error(message))
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
            Ok(JobDeclaration::CommitMiningJob(_)) => Err(Error::UnexpectedMessage(
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
