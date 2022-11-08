use crate::lib::job_negotiator::{CommittedMiningJob, JobNegotiatorDownstream};
use binary_sv2::B0255;
use roles_logic_sv2::{
    handlers::{job_negotiation::ParseClientJobNegotiationMessages, SendTo_},
    job_negotiation_sv2::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, CommitMiningJob,
        CommitMiningJobError, CommitMiningJobSuccess, IdentifyTransactionsSuccess,
        ProvideMissingTransactionsSuccess,
    },
    parsers::JobNegotiation,
};
use serde::__private::de::IdentifierDeserializer;
use std::convert::TryInto;
use tracing::info;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use roles_logic_sv2::errors::Error;

impl JobNegotiatorDownstream {
    fn verify_job(&mut self, message: &CommitMiningJob) -> bool {
        let key: Vec<u8> = message.mining_job_token.inner_as_ref().try_into().unwrap();
        let is_token_allocated = self.token_to_job_map.contains_key(&key);
        // TODO Function to implement, it must be checked if the requested job has:
        // 1. right coinbase
        // 2. right version field
        // 3. right prev-hash
        // 4. right nbits
        // 5. a valid merkletpath
        is_token_allocated
    }
}

impl ParseClientJobNegotiationMessages for JobNegotiatorDownstream {
    fn allocate_mining_job_token(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<SendTo, Error> {
        let token = self.tokens.next();
        let token: B0255 = token.clone().to_le_bytes().to_vec().try_into().unwrap();
        let message_success = AllocateMiningJobTokenSuccess {
            request_id: message.request_id,
            mining_job_token: token.clone(),
            coinbase_output_max_additional_size: 123454321,
            async_mining_allowed: true,
        };
        let token = token.inner_as_ref().to_owned();
        self.token_to_job_map.insert(token, None);
        let message_enum = JobNegotiation::AllocateMiningJobTokenSuccess(message_success);
        info!(
            "Sending AllocateMiningJobTokenSuccess to proxy {:?}",
            message_enum
        );
        Ok(SendTo::Respond(message_enum))
    }

    fn commit_mining_job(&mut self, message: CommitMiningJob) -> Result<SendTo, Error> {
        if self.verify_job(&message) {
            let message_success = CommitMiningJobSuccess {
                request_id: message.request_id,
                new_mining_job_token: message.mining_job_token.clone().into_static(),
            };
            let message_enum_success = JobNegotiation::CommitMiningJobSuccess(message_success);
            let token = message.mining_job_token.clone().into_static();
            let message_committed = CommittedMiningJob {
                request_id: message.request_id,
                mining_job_token: message.mining_job_token.into_static(),
                version: 2,
                coinbase_tx_version: message.coinbase_tx_version,
                coinbase_prefix: message.coinbase_prefix.into_static(),
                coinbase_tx_input_n_sequence: message.coinbase_tx_input_n_sequence,
                coinbase_tx_value_remaining: message.coinbase_tx_value_remaining,
                coinbase_tx_outputs: message.coinbase_tx_outputs.into_static(),
                coinbase_tx_locktime: message.coinbase_tx_locktime,
                min_extranonce_size: 0,
                tx_short_hash_nonce: 0,
                /// Only for MVP2: must be filled with right values for production,
                /// this values are needed for block propagation
                tx_short_hash_list: vec![].try_into().unwrap(),
                tx_hash_list_hash: [0; 32].try_into().unwrap(),
                excess_data: vec![].try_into().unwrap(),
            };
            self.token_to_job_map
                .insert(token.inner_as_ref().to_owned(), Some(message_committed));
            //
            info!(
                "Commit mining job was a success: {:?}",
                message_enum_success
            );

            Ok(SendTo::Respond(message_enum_success))
        } else {
            let message_error = CommitMiningJobError {
                /// possible errors:
                /// invalid-mining-job-token
                /// invalid-job-param-value-{} - {} is replaced by a particular field name from CommitMiningJob message
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
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match (message_type, payload).try_into() {
            Ok(JobNegotiation::AllocateMiningJobToken(message)) => {
                println!("Allocate mining job token message sent to Proxy");
                self_
                    .safe_lock(|x| x.allocate_mining_job_token(message))
                    .unwrap()
            }
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
