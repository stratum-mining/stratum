use crate::lib::job_negotiator::JobNegotiator;
use roles_logic_sv2::{
    handlers::{job_negotiation::ParseServerJobNegotiationMessages, SendTo_},
    job_negotiation_sv2::{
        AllocateMiningJobTokenSuccess, CommitMiningJob, IdentifyTransactions, ProvideMissingTransactionsSuccess, CommitMiningJobError, CommitMiningJobSuccess,
     ProvideMissingTransactions, IdentifyTransactionsSuccess,
    },
    parsers::JobNegotiation,
};
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use roles_logic_sv2::errors::Error;
use std::convert::TryInto;

impl ParseServerJobNegotiationMessages for JobNegotiator {
    fn allocate_mining_job_token_success(
        &mut self,
        message: AllocateMiningJobTokenSuccess,
    ) -> Result<SendTo, Error> {
        let coinbase_output_max_additional_size = message.coinbase_output_max_additional_size;

        let new_template = self.last_new_template.unwrap();

        let message_commit_mining_job = CommitMiningJob { 
            request_id: message.request_id,
            mining_job_token: message.mining_job_token, 
            version: 2, 
            coinbase_tx_version: 0, 
            coinbase_prefix: new_template.coinbase_prefix, 
            coinbase_tx_input_n_sequence: 0, 
            coinbase_tx_value_remaining: 0, 
            coinbase_tx_outputs: new_template.coinbase_tx_outputs, 
            coinbase_tx_locktime: 0, 
            min_extranonce_size: 0, 
            tx_short_hash_nonce: 0, 
            tx_short_hash_list, 
            tx_hash_list_hash, 
            excess_data, 
        };
        let commit_mining_job = JobNegotiation::CommitMiningJob(message_commit_mining_job);
        println!("Sending AllocateMiningJobTokenSuccess to proxy {:?}", commit_mining_job);
        Ok(SendTo::None(Some(commit_mining_job)))
    }

    fn commit_mining_job_success(
        &mut self,
        message: CommitMiningJobSuccess,
    ) -> Result<SendTo, Error> {
        todo!()
    }

    fn commit_mining_job_error(&mut self, message: CommitMiningJobError) -> Result<SendTo, Error> {
        todo!();
    }

    fn identify_transactions(&mut self, message: IdentifyTransactions) -> Result<SendTo, Error> {
        let message_identify_transactions = IdentifyTransactionsSuccess {
            request_id: message.request_id,
            tx_hash_list: todo!(),
        };
        let message_enum = JobNegotiation::IdentifyTransactionsSuccess(message_identify_transactions);
        Ok(SendTo::Respond(message_enum))
    }

    fn provide_missing_transactions(
        &mut self,
        message: ProvideMissingTransactions,
    ) -> Result<SendTo, Error> {
        let message_provide_missing_transactions = ProvideMissingTransactionsSuccess {
            request_id: message.request_id,
            transaction_list: todo!(),
        };
        let message_enum = JobNegotiation::ProvideMissingTransactionsSuccess(message_provide_missing_transactions);
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_message_job_negotiation(
        self_: std::sync::Arc<roles_logic_sv2::utils::Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match (message_type, payload).try_into() {
            Ok(JobNegotiation::AllocateMiningJobTokenSuccess(message)) => self_
                .safe_lock(|x| x.allocate_mining_job_token_success(message))
                .unwrap(),
            Ok(JobNegotiation::CommitMiningJobSuccess(message)) => {
                self_.safe_lock(|x| x.commit_mining_job_success(message)).unwrap()
            }
            Ok(JobNegotiation::CommitMiningJobError(message)) => {
                self_.safe_lock(|x| x.commit_mining_job_error(message)).unwrap()
            }
            Ok(JobNegotiation::IdentifyTransactions(message)) => self_
                .safe_lock(|x| x.identify_transactions(message))
                .unwrap(),
            Ok(JobNegotiation::ProvideMissingTransactions(message)) => self_
                .safe_lock(|x| x.provide_missing_transactions(message))
                .unwrap(),
            Ok(JobNegotiation::AllocateMiningJobToken(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::CommitMiningJob(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::IdentifyTransactionsSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::ProvideMissingTransactionsSuccess(_)) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
    }
    }
}
