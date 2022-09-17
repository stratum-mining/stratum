use crate::lib::job_negotiator::JobNegotiatorDownstream;
use roles_logic_sv2::{
    handlers::{job_negotiation::ParseClientJobNegotiationMessages, SendTo_},
    job_negotiation_sv2::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, CommitMiningJob,
        CommitMiningJobError, CommitMiningJobSuccess, IdentifyTransactionsSuccess,
        ProvideMissingTransactionsSuccess,
    },
    parsers::JobNegotiation,
};
use std::convert::TryInto;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use roles_logic_sv2::errors::Error;

impl JobNegotiatorDownstream {
    fn verify_job(&mut self, message: &CommitMiningJob) -> bool {
        let is_token_allocated = self
            .token_to_job_map
            .contains_key(&message.mining_job_token);
        // TODO Function to implement, it must be checked if the requested job has:
        // 1. right coinbase
        // 2. right version field
        // 3. right prev-hash
        // 4. right nbits
        // 5. a valid merketpath
        is_token_allocated
    }
}

impl ParseClientJobNegotiationMessages for JobNegotiatorDownstream {
    fn allocate_mining_job_token(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<SendTo, Error> {
        let token = self.tokens.next();
        self.token_to_job_map.insert(token, None);
        let message_success = AllocateMiningJobTokenSuccess {
            request_id: message.request_id,
            mining_job_token: token,
            coinbase_output_max_additional_size: 0,
            async_mining_allowed: false,
        };
        let message_enum = JobNegotiation::AllocateMiningJobTokenSuccess(message_success);
        Ok(SendTo::Respond(message_enum))
    }

    fn commit_mining_job(&mut self, message: CommitMiningJob) -> Result<SendTo, Error> {
        if self.verify_job(&message) {
            let message_success = CommitMiningJobSuccess {
                request_id: message.request_id,
                new_mining_job_token: message.mining_job_token,
            };
            let message_enum_success = JobNegotiation::CommitMiningJobSuccess(message_success);
            self.token_to_job_map
                .insert(message.mining_job_token, Some(message.into()));
            Ok(SendTo::Respond(message_enum_success))
        } else {
            let message_error = CommitMiningJobError {
                request_id: message.request_id,
                error_code: todo!(),
                error_details: todo!(),
            };
            let message_enum_error = JobNegotiation::CommitMiningJobError(message_error);
            Ok(SendTo::Respond(message_enum_error))
        }
    }

    fn identify_transactions_success(
        &mut self,
        message: IdentifyTransactionsSuccess,
    ) -> Result<SendTo, Error> {
        let message_success = IdentifyTransactionsSuccess {
            request_id: message.request_id,
            tx_hash_list: todo!(),
        };
        let message_enum = JobNegotiation::IdentifyTransactionsSuccess(message_success);
        Ok(SendTo::Respond(message_enum))
    }

    fn provide_missing_transactions_success(
        &mut self,
        message: ProvideMissingTransactionsSuccess,
    ) -> Result<SendTo, Error> {
        let message_success = ProvideMissingTransactionsSuccess {
            request_id: message.request_id,
            transaction_list: todo!(),
        };
        let message_enum = JobNegotiation::ProvideMissingTransactionsSuccess(message_success);
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_message_job_negotiation(
        self_: std::sync::Arc<roles_logic_sv2::utils::Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<roles_logic_sv2::handlers::job_negotiation::SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match (message_type, payload).try_into() {
            Ok(JobNegotiation::AllocateMiningJobToken(message)) => self_
                .safe_lock(|x| x.allocate_mining_job_token(message))
                .unwrap(),
            Ok(JobNegotiation::CommitMiningJob(message)) => {
                self_.safe_lock(|x| x.commit_mining_job(message)).unwrap()
            }
            Ok(JobNegotiation::IdentifyTransactionsSuccess(message)) => self_
                .safe_lock(|x| x.identify_transactions_success(message))
                .unwrap(),
            Ok(JobNegotiation::ProvideMissingTransactionsSuccess(message)) => self_
                .safe_lock(|x| x.provide_missing_transactions_success(message))
                .unwrap(),
            Ok(JobNegotiation::AllocateMiningJobTokenSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::CommitMiningJobSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::CommitMiningJobError(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::IdentifyTransactions(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::ProvideMissingTransactions(_)) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }
}
